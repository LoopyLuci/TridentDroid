//! Smoke tests for the real WHP vCPU execution loop (Tier 1) and the
//! callback-based PIO/MMIO read-completion path (Tier 2).
//!
//! Boots tiny hand-written real-mode binaries through the actual
//! `Hypervisor` trait — `WHvCreateVirtualProcessor`, `WHvRunVirtualProcessor`,
//! register get/set, and exit classification — with no kernel image
//! involved. This is independent of full Android/Linux boot and exists to
//! prove the WHP backend genuinely executes guest instructions and
//! correctly completes both write-only exits (RIP advance) and read exits
//! (RIP advance *and* delivering the `on_access` callback's result into a
//! guest register — the bug Tier 2's `VcpuAccess`/callback redesign exists
//! to fix; before that fix, this could only be observed as "nothing happens
//! to guest state," not an error, so it needed a dedicated test). Can only
//! run on real Windows + WHP hardware and can't be a normal unit test.
//!
//! Neither test relies on reaching a trailing `hlt` to end the run: on real
//! x86, `HLT` with IF=0 and no NMI/SMI/INIT source halts the processor
//! until one of those arrives — same as real silicon, not a WHP quirk —
//! and these tests set up neither, so `WHvRunVirtualProcessor` would block
//! indefinitely waiting for something that will never come (confirmed
//! empirically while writing the Tier 1 version of this test). KVM
//! sidesteps this because it unconditionally intercepts HLT regardless of
//! IF state; WHP does not appear to offer that same always-trap policy.
//! Instead, each `on_access` closure below returns an `Err` once it has
//! observed what the test needs, which `run_vcpu` propagates straight out
//! — a clean way to end a synthetic run without depending on HLT/interrupt
//! semantics at all. Real guest kernels only reach a genuinely unrecoverable
//! HLT with interrupts disabled (e.g. a panic's `while(1) halt()`), so none
//! of this is expected to affect the real boot-to-panic milestone.

#![cfg(target_os = "windows")]

use anyhow::Result;
use trident_hal::{Hypervisor, MemFlags, Regs, Segment, Sregs, VcpuAccess};
use trident_hal_whp::WhpHypervisor;

/// Sentinel error used to end a test's run loop from inside `on_access`,
/// once it has seen everything it needs — see the module doc for why.
const DONE: &str = "__smoke_test_done__";

/// Allocate `size` bytes of zeroed, page-aligned guest RAM. `WHvMapGpaRange`
/// requires a page-aligned host VA — unlike a plain `Vec<u8>`, which
/// Windows' allocator does not guarantee to be 4 KiB-aligned even for large
/// sizes — so this goes through a page-aligned `Layout` directly.
fn alloc_guest_ram(size: usize) -> &'static mut [u8] {
    let layout = std::alloc::Layout::from_size_align(size, 4096).unwrap();
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
    assert!(!ptr.is_null(), "page-aligned RAM allocation failed");
    unsafe { std::slice::from_raw_parts_mut(ptr, size) }
}

/// Flat real-mode entry: CS base 0, selector 0, so RIP maps directly onto
/// guest-physical addresses — no segmentation arithmetic needed for these
/// tests. Sets `rip` to `entry` and clears IF.
fn setup_flat_real_mode(hv: &WhpHypervisor, vcpu: &mut trident_hal_whp::WhpVcpu, entry: u64) {
    let flat_code_seg = Segment {
        base: 0,
        limit: 0xffff,
        selector: 0,
        type_: 0b1011, // present, execute/read code segment
        s: 1,          // non-system (code/data) segment
        present: 1,
        dpl: 0,
        db: 0,
        l: 0,
        g: 0,
        avl: 0,
        unusable: 0,
    };
    let flat_data_seg = Segment {
        type_: 0b0011, // read/write data segment
        ..flat_code_seg.clone()
    };

    let mut sregs: Sregs = hv.get_sregs(vcpu).expect("get_sregs");
    sregs.cs = flat_code_seg;
    sregs.ds = flat_data_seg.clone();
    sregs.es = flat_data_seg.clone();
    sregs.ss = flat_data_seg;
    hv.set_sregs(vcpu, &sregs).expect("set_sregs");

    let mut regs: Regs = hv.get_regs(vcpu).expect("get_regs");
    regs.rip = entry;
    regs.rflags = 0x2; // bit 1 is reserved-as-1; IF left clear
    hv.set_regs(vcpu, &regs).expect("set_regs");
}

/// Runs `vcpu` until `on_access` returns the `DONE` sentinel error (the
/// expected/successful ending for these tests) — any other `Err`, or a
/// clean `Ok` return (meaning a real terminal exit like `Hlt` happened
/// before the test was done), fails the test.
fn run_until_done(
    hv: &WhpHypervisor,
    vcpu: &mut trident_hal_whp::WhpVcpu,
    on_access: &mut dyn FnMut(VcpuAccess) -> Result<()>,
) {
    match hv.run_vcpu(vcpu, on_access) {
        Err(e) if e.to_string() == DONE => {}
        Err(e) => panic!("run_vcpu failed: {e:#}"),
        Ok(exit) => panic!("vCPU reached a terminal exit before the test finished: {exit:?}"),
    }
}

#[test]
fn executes_real_mode_pio_writes_via_real_whp() {
    let hv = match WhpHypervisor::new() {
        Ok(hv) => hv,
        Err(e) => {
            eprintln!("SKIP: WHP not available on this machine: {e}");
            return;
        }
    };

    let vm = hv.create_vm(1).expect("create_vm");
    let mut vcpu = hv.create_vcpu(&vm, 0).expect("create_vcpu");

    let ram = alloc_guest_ram(1 << 20);

    // mov dx, 0x03f8 ; mov al, 0x41 ('A') ; out dx, al ; mov al, 0x42 ('B') ; out dx, al
    let code: [u8; 9] = [0xBA, 0xF8, 0x03, 0xB0, 0x41, 0xEE, 0xB0, 0x42, 0xEE];
    let entry: usize = 0x1000;
    ram[entry..entry + code.len()].copy_from_slice(&code);

    hv.map_memory(&vm, 0, ram, MemFlags::READ | MemFlags::WRITE | MemFlags::EXECUTE)
        .expect("map_memory");
    hv.set_vcpu_ram_hint(&mut vcpu, ram.as_ptr(), ram.len());
    setup_flat_real_mode(&hv, &mut vcpu, entry as u64);

    // Expect exactly two OUT accesses, in order, each with the right port and
    // byte, proving both real execution *and* correct RIP advancement
    // between them (a stalled/incorrect advance would re-trap on the same
    // instruction with the same byte instead of progressing to the next).
    let expected = [0x41u8, 0x42u8];
    let mut seen = 0usize;
    let mut on_access = |access: VcpuAccess| -> Result<()> {
        match access {
            VcpuAccess::IoOut { port, data } => {
                assert_eq!(port, 0x03f8, "expected writes to the COM1 THR port");
                assert_eq!(
                    data.first().copied(),
                    expected.get(seen).copied(),
                    "unexpected byte at access {seen} — RIP likely did not advance between OUTs"
                );
                seen += 1;
                if seen == expected.len() {
                    anyhow::bail!(DONE);
                }
                Ok(())
            }
            other => panic!("unexpected vCPU access: {other:?}"),
        }
    };

    run_until_done(&hv, &mut vcpu, &mut on_access);
    assert_eq!(seen, 2, "expected exactly two OUT accesses");
}

#[test]
fn pio_read_result_reaches_guest_register_via_real_whp() {
    let hv = match WhpHypervisor::new() {
        Ok(hv) => hv,
        Err(e) => {
            eprintln!("SKIP: WHP not available on this machine: {e}");
            return;
        }
    };

    let vm = hv.create_vm(1).expect("create_vm");
    let mut vcpu = hv.create_vcpu(&vm, 0).expect("create_vcpu");

    let ram = alloc_guest_ram(1 << 20);

    // mov dx, 0x03f8 ; in al, dx ; out dx, al
    //
    // This is the direct regression test for the read-completion bug this
    // redesign fixes: the `IN` fills AL from whatever `on_access` supplies
    // for `VcpuAccess::IoIn`. If that value never actually reaches the AL
    // register (the original bug — RIP would advance immediately with the
    // callback's fill discarded), the following `OUT` would echo stale/zero
    // AL instead of the callback's sentinel byte.
    const SENTINEL: u8 = 0x99;
    let code: [u8; 5] = [0xBA, 0xF8, 0x03, 0xEC, 0xEE];
    let entry: usize = 0x1000;
    ram[entry..entry + code.len()].copy_from_slice(&code);

    hv.map_memory(&vm, 0, ram, MemFlags::READ | MemFlags::WRITE | MemFlags::EXECUTE)
        .expect("map_memory");
    hv.set_vcpu_ram_hint(&mut vcpu, ram.as_ptr(), ram.len());
    setup_flat_real_mode(&hv, &mut vcpu, entry as u64);

    let mut saw_out = false;
    let mut on_access = |access: VcpuAccess| -> Result<()> {
        match access {
            VcpuAccess::IoIn { port, data } => {
                assert_eq!(port, 0x03f8);
                data[0] = SENTINEL;
                Ok(())
            }
            VcpuAccess::IoOut { port, data } => {
                assert_eq!(port, 0x03f8);
                assert_eq!(
                    data.first().copied(),
                    Some(SENTINEL),
                    "IN result never reached AL — read completion is broken"
                );
                saw_out = true;
                anyhow::bail!(DONE);
            }
            other => panic!("unexpected vCPU access: {other:?}"),
        }
    };

    run_until_done(&hv, &mut vcpu, &mut on_access);
    assert!(saw_out, "never observed the echoing OUT");
}
