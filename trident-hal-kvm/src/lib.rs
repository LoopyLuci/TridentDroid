#![cfg(target_os = "linux")]
//! KVM backend for TridentHAL.
//!
//! Wraps `kvm-ioctls` behind the `Hypervisor` trait so the VMM above it
//! has no direct knowledge of KVM.

mod vcpu;
mod vm;
mod mem;

pub use vm::KvmVm;
pub use vcpu::KvmVcpu;

use anyhow::{Context, Result};
use kvm_ioctls::Kvm;
use trident_hal::{DirtyBitmap, Hypervisor, MemFlags, Regs, Sregs, VcpuAccess, VcpuExit};

/// The KVM hypervisor backend.  Create one per process with `KvmHypervisor::open()`.
pub struct KvmHypervisor {
    kvm: Kvm,
}

impl KvmHypervisor {
    pub fn open() -> Result<Self> {
        let kvm = Kvm::new().context("Failed to open /dev/kvm — is the kvm module loaded?")?;
        check_capabilities(&kvm)?;
        Ok(Self { kvm })
    }
}

// SAFETY: Kvm wraps a file descriptor; sharing the fd across threads is safe
// because all KVM ioctls are thread-safe by the KVM ABI.
unsafe impl Send for KvmHypervisor {}
unsafe impl Sync for KvmHypervisor {}

impl Hypervisor for KvmHypervisor {
    type Vm   = KvmVm;
    type Vcpu = KvmVcpu;

    fn create_vm(&self, _vcpu_count: u32) -> Result<KvmVm> {
        KvmVm::new(&self.kvm)
    }

    fn create_vcpu(&self, vm: &KvmVm, id: u32) -> Result<KvmVcpu> {
        KvmVcpu::new(vm, id)
    }

    fn run_vcpu(
        &self,
        vcpu: &mut KvmVcpu,
        on_access: &mut dyn FnMut(VcpuAccess) -> Result<()>,
    ) -> Result<VcpuExit> {
        vcpu.run(on_access)
    }

    fn get_regs(&self, vcpu: &KvmVcpu) -> Result<Regs> {
        vcpu.get_regs()
    }

    fn set_regs(&self, vcpu: &KvmVcpu, regs: &Regs) -> Result<()> {
        vcpu.set_regs(regs)
    }

    fn get_sregs(&self, vcpu: &KvmVcpu) -> Result<Sregs> {
        vcpu.get_sregs()
    }

    fn set_sregs(&self, vcpu: &KvmVcpu, sregs: &Sregs) -> Result<()> {
        vcpu.set_sregs(sregs)
    }

    fn map_memory(
        &self,
        vm: &KvmVm,
        guest_phys: u64,
        host_mem: &[u8],
        flags: MemFlags,
    ) -> Result<()> {
        vm.map_memory(guest_phys, host_mem, flags)
    }

    fn unmap_memory(&self, vm: &KvmVm, guest_phys: u64, size: u64) -> Result<()> {
        vm.unmap_memory(guest_phys, size)
    }

    fn query_dirty_bitmap(&self, vm: &KvmVm, guest_phys: u64, size: u64) -> Result<DirtyBitmap> {
        vm.query_dirty_bitmap(guest_phys, size)
    }
}

// ── Capability check ──────────────────────────────────────────────────────────

fn check_capabilities(kvm: &Kvm) -> Result<()> {
    use kvm_ioctls::Cap;
    let required = [
        Cap::UserMemory,
        Cap::SetTssAddr,
        Cap::Irqchip,
        Cap::Ioeventfd,
        Cap::Irqfd,
        Cap::PitState2,
    ];
    for cap in required {
        anyhow::ensure!(
            kvm.check_extension(cap),
            "KVM missing required capability: {:?}",
            cap
        );
    }
    Ok(())
}
