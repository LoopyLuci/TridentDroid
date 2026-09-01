#![cfg(target_os = "windows")]

use trident_hal::{Hypervisor, MemFlags};
use trident_hal_whp::WhpHypervisor;

#[test]
fn vcpu_then_vm_drop_order_does_not_crash() {
    let hv = match WhpHypervisor::new() {
        Ok(hv) => hv,
        Err(e) => {
            eprintln!("SKIP: WHP not available: {e}");
            return;
        }
    };

    let vm = hv.create_vm(1).expect("create_vm");
    let vcpu = hv.create_vcpu(&vm, 0).expect("create_vcpu");

    eprintln!("[ckpt] dropping vcpu first");
    drop(vcpu);
    eprintln!("[ckpt] vcpu dropped ok, dropping vm");
    drop(vm);
    eprintln!("[ckpt] vm dropped ok");
}

#[test]
fn vcpu_with_ram_hint_then_vm_drop_order_does_not_crash() {
    let hv = match WhpHypervisor::new() {
        Ok(hv) => hv,
        Err(e) => {
            eprintln!("SKIP: WHP not available: {e}");
            return;
        }
    };

    let ram_size = 32usize << 20;
    let layout = std::alloc::Layout::from_size_align(ram_size, 4096).unwrap();
    let ram_ptr = unsafe { std::alloc::alloc_zeroed(layout) };
    assert!(!ram_ptr.is_null());
    let ram: &mut [u8] = unsafe { std::slice::from_raw_parts_mut(ram_ptr, ram_size) };

    let vm = hv.create_vm(1).expect("create_vm");
    hv.map_memory(&vm, 0, ram, MemFlags::READ | MemFlags::WRITE | MemFlags::EXECUTE)
        .expect("map_memory");
    let mut vcpu = hv.create_vcpu(&vm, 0).expect("create_vcpu");
    hv.set_vcpu_ram_hint(&mut vcpu, ram.as_ptr(), ram.len());

    eprintln!("[ckpt] dropping vcpu first");
    drop(vcpu);
    eprintln!("[ckpt] vcpu dropped ok, dropping vm");
    drop(vm);
    eprintln!("[ckpt] vm dropped ok");
}
