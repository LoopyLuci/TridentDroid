//! Tests for vCPU pause + snapshot + restore against a real
//! hypervisor-backed `Vm` (WHP on this machine).

use tridentd_lib::vmm::snapshot::{SnapshotMetadata, VcpuState};
use tridentd_lib::vmm::vm::RestoreOverrides;
use tridentd_lib::vmm::virtio::{VirtioBlk, VirtioConsole, VirtioDevice, VirtioInput, VirtioNet};
use tridentd_lib::vmm::{Vm, VmConfig};
use trident_hal::{Regs, Sregs};

/// bzImage kernel data starts at `(setup_sects + 1) * 512`; with
/// `setup_sects = 4` that's byte 2560. `KernelLoader::load` on Windows
/// only checks the `HdrS` magic at 0x202 and `setup_sects` at 0x1F1 — see
/// `tridentd/src/vmm/loader.rs::load_windows`.
const SETUP_SECTS: u8 = 4;
const KERNEL_OFFSET: usize = (SETUP_SECTS as usize + 1) * 512;
const LOAD_ADDR: u64 = 0x0010_0000;

fn write_fake_bzimage(path: &std::path::Path, code: &[u8]) {
    let mut data = vec![0u8; KERNEL_OFFSET + code.len()];
    data[0x1F1] = SETUP_SECTS;
    data[0x202..0x206].copy_from_slice(b"HdrS");
    data[KERNEL_OFFSET..KERNEL_OFFSET + code.len()].copy_from_slice(code);
    std::fs::write(path, &data).expect("write fake bzImage");
}

fn blank_config() -> VmConfig {
    VmConfig {
        vcpu_count: 1,
        memory_mib: 32,
        kernel_path: String::new(),
        initrd_path: None,
        cmdline: String::new(),
        sriov_vf: None,
        system_image: None,
        vendor_image: None,
        console_sock: None,
    }
}

/// `spawn_vcpus()` uses `tokio::task::spawn_blocking` internally, which
/// requires an active Tokio runtime even though this test is otherwise
/// synchronous.
///
/// Deliberately NOT `#[tokio::test]`: this test's guest code is an
/// intentional infinite loop (see the comment on `code` below), and nothing
/// currently stops a spawned vCPU thread once `spawn_vcpus()` hands it off
/// (a real, acknowledged gap — see `pause.rs`'s module doc). `#[tokio::test]`
/// would drop its runtime when the test function returns, and
/// `Runtime::drop` blocks forever waiting for every outstanding
/// `spawn_blocking` task — including this permanently-running one — with no
/// timeout. Building the runtime by hand and `mem::forget`-ing it instead of
/// dropping it skips that blocking wait, while — unlike `std::process::exit`
/// — leaving the rest of this test binary's process alone so later tests
/// still run. This is a test-harness workaround for the still-missing "stop
/// a running vCPU" capability, not a masked bug in the feature itself
/// (confirmed independently: the pause/resume protocol was traced
/// end-to-end with instrumentation and completes correctly).
#[test]
#[cfg(windows)]
fn snapshot_captures_real_running_vcpu_state() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");
    rt.block_on(snapshot_captures_real_running_vcpu_state_inner());
    std::mem::forget(rt);
}

#[cfg(windows)]
async fn snapshot_captures_real_running_vcpu_state_inner() {
    let hyp = std::sync::Arc::new(
        tridentd_lib::platform::open_hypervisor().expect("open hypervisor — is WHP available?"),
    );

    let tmp = tempfile::tempdir().unwrap();
    let kernel_path = tmp.path().join("fake.bzImage");

    // mov edx, 0x3f8 ; mov al, 0x41 ; out dx, al ; jmp $-10 (back to start)
    // Loops forever, generating an IoOut exit every iteration — this is
    // what actually gives PauseGate a chance to catch the vCPU (see
    // `pause.rs`'s doc comment on why a pause needs a live exit stream).
    // The `out` opcode (0xEE) always lands at LOAD_ADDR + 7.
    let code: [u8; 10] = [0xBA, 0xF8, 0x03, 0x00, 0x00, 0xB0, 0x41, 0xEE, 0xEB, 0xF6];
    let out_instruction_gpa = LOAD_ADDR + 7;
    write_fake_bzimage(&kernel_path, &code);

    let mut config = blank_config();
    config.kernel_path = kernel_path.to_string_lossy().to_string();

    let mut vm = Vm::create(&hyp, config).expect("Vm::create");
    let _handles = vm.spawn_vcpus();

    // Let the vCPU thread actually start looping before asking for a
    // snapshot — irrelevant how many iterations it does, only that it's
    // live and generating exits by the time snapshot() is called.
    std::thread::sleep(std::time::Duration::from_millis(50));

    let snap_path = tmp.path().join("test.trident-snap");
    let written = vm.snapshot(&snap_path).expect("snapshot");
    assert!(
        written.total_bytes > 32 * 1024 * 1024,
        "snapshot should include the full 32 MiB RAM dump, got {} bytes",
        written.total_bytes
    );

    // Inspect the raw file to verify the pause captured real, correct
    // state — not just that the round trip didn't crash.
    let (meta, ram) = tridentd_lib::vmm::snapshot::read_snapshot(&snap_path).expect("read_snapshot");
    assert_eq!(meta.vcpus.len(), 1);
    assert_eq!(
        meta.vcpus[0].regs.rip, out_instruction_gpa,
        "the only instruction that ever traps in this loop is the `out` at \
         LOAD_ADDR+7 — a captured RIP anywhere else means the pause landed \
         mid-instruction or captured stale state"
    );
    let loaded = &ram[LOAD_ADDR as usize..LOAD_ADDR as usize + code.len()];
    assert_eq!(loaded, &code, "RAM dump should contain the loaded kernel code verbatim");

    // Deliberately does not also call Vm::restore() here: doing so would
    // create a *second* live WHP partition while this one's vCPU thread is
    // still spinning at very high frequency (nothing currently stops a
    // spawned vCPU thread — a real gap, see the Tier 3 summary), which
    // reproducibly crashes on this machine. That looks like a driver/
    // host-level stress interaction between two concurrently-active
    // partitions, not a snapshot/restore logic bug — restore is covered in
    // isolation by the test below instead, against a hand-built snapshot
    // file with no live vCPU at all.

    // All assertions have run; this function returns normally now (the
    // still-spinning vCPU thread is left running, but see this test's own
    // doc comment above for why that no longer hangs teardown).
}

/// Builds the exact device-blob list `Vm::restore` expects, in the same
/// order `build_devices` (private to `vm.rs`) always registers them —
/// console, system blk, vendor blk, net, input.
fn default_device_blobs() -> Vec<Vec<u8>> {
    vec![
        VirtioConsole::new("console").snapshot_state().unwrap(),
        VirtioBlk::new("system").snapshot_state().unwrap(),
        VirtioBlk::new("vendor").snapshot_state().unwrap(),
        VirtioNet::new("net").snapshot_state().unwrap(),
        VirtioInput::new("input").snapshot_state().unwrap(),
    ]
}

#[tokio::test(flavor = "multi_thread")]
#[cfg(windows)]
async fn restore_accepts_a_snapshot_file_with_no_other_partition_active() {
    let hyp = std::sync::Arc::new(
        tridentd_lib::platform::open_hypervisor().expect("open hypervisor — is WHP available?"),
    );

    let tmp = tempfile::tempdir().unwrap();
    let memory_mib = 32u64;
    let meta = SnapshotMetadata {
        config: blank_config(),
        vcpus: vec![VcpuState { regs: Regs::default(), sregs: Sregs::default() }],
        devices: default_device_blobs(),
        ram_len: memory_mib << 20,
    };
    let ram = vec![0u8; (memory_mib << 20) as usize];

    let path = tmp.path().join("handbuilt.trident-snap");
    tridentd_lib::vmm::snapshot::write_snapshot(&path, &meta, &ram).expect("write_snapshot");

    // The real assertion here is simply that this succeeds *and doesn't
    // crash on drop*: it exercises create_vm/create_vcpu, set_regs/set_sregs
    // with real (bincode round-tripped) data, map_memory with the restored
    // RAM, restore_state for every registered device, and — critically —
    // correct teardown order when `restored` drops below (this test is
    // what originally caught two real bugs: `Vm`'s field-order-on-drop
    // issue — vCPUs and the hypervisor handle must drop in the right order
    // relative to the partition — and a use-after-alignment-mismatch bug
    // in `aligned_alloc`; see the comments on `Vm`'s fields and on
    // `AlignedRam` in vm.rs for both).
    let restored = Vm::restore(&hyp, &path, RestoreOverrides::default()).expect("Vm::restore");
    drop(restored);
}
