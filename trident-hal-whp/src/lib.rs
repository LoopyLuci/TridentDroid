//! Windows Hypervisor Platform (WHP) backend for TridentHAL.
//!
//! Requires:
//!   - Windows 10 build 17134+ (April 2018 Update)
//!   - "Windows Hypervisor Platform" optional feature enabled
//!   - Hyper-V enabled (WHP runs on top of Hyper-V)
//!
//! This backend implements the `Hypervisor` trait using the WHv* Win32 API.
//! No kernel driver is needed — WHP is entirely user-mode.

#[cfg(windows)]
mod partition;
#[cfg(windows)]
mod processor;
#[cfg(windows)]
mod regs;
#[cfg(windows)]
mod exit;

#[cfg(windows)]
pub use partition::WhpVm;
#[cfg(windows)]
pub use processor::WhpVcpu;

use anyhow::Result;
use trident_hal::{DirtyBitmap, Hypervisor, MemFlags, Regs, Sregs, VcpuExit};

/// The WHP hypervisor backend.  Create one with `WhpHypervisor::new()`.
pub struct WhpHypervisor;

impl WhpHypervisor {
    pub fn new() -> Result<Self> {
        #[cfg(not(windows))]
        anyhow::bail!("WhpHypervisor is only supported on Windows");
        #[cfg(windows)]
        {
            partition::check_whp_available()?;
            Ok(Self)
        }
    }
}

impl Default for WhpHypervisor {
    fn default() -> Self {
        Self
    }
}

// Platform stub types for non-Windows builds
#[cfg(not(windows))]
pub struct WhpVm;
#[cfg(not(windows))]
pub struct WhpVcpu;
#[cfg(not(windows))]
unsafe impl Send for WhpVm {}
#[cfg(not(windows))]
unsafe impl Sync for WhpVm {}
#[cfg(not(windows))]
unsafe impl Send for WhpVcpu {}

unsafe impl Send for WhpHypervisor {}
unsafe impl Sync for WhpHypervisor {}

impl Hypervisor for WhpHypervisor {
    type Vm = WhpVm;
    type Vcpu = WhpVcpu;

    fn create_vm(&self, vcpu_count: u32) -> Result<WhpVm> {
        #[cfg(windows)]
        return partition::WhpVm::create(vcpu_count);
        #[cfg(not(windows))]
        { let _ = vcpu_count; anyhow::bail!("WHP not available on this platform"); }
    }

    fn create_vcpu(&self, vm: &WhpVm, id: u32) -> Result<WhpVcpu> {
        #[cfg(windows)]
        return processor::WhpVcpu::new(vm, id);
        #[cfg(not(windows))]
        { let _ = (vm, id); anyhow::bail!("WHP not available"); }
    }

    fn set_vcpu_ram_hint(&self, vcpu: &mut WhpVcpu, ptr: *const u8, len: usize) {
        vcpu.ram_ptr = ptr;
        vcpu.ram_len = len;
    }

    fn start_debug_sampler(&self, vcpu: &WhpVcpu, interval_ms: u64) {
        #[cfg(windows)]
        { let _ = vcpu.start_rip_sampler(interval_ms); }
        #[cfg(not(windows))]
        { let _ = (vcpu, interval_ms); }
    }

    fn run_vcpu(&self, vcpu: &mut WhpVcpu) -> Result<VcpuExit> {
        #[cfg(windows)]
        return vcpu.run();
        #[cfg(not(windows))]
        { let _ = vcpu; anyhow::bail!("WHP not available"); }
    }

    fn get_regs(&self, vcpu: &WhpVcpu) -> Result<Regs> {
        #[cfg(windows)]
        return regs::get_regs(vcpu);
        #[cfg(not(windows))]
        { let _ = vcpu; anyhow::bail!("WHP not available"); }
    }

    fn set_regs(&self, vcpu: &WhpVcpu, r: &Regs) -> Result<()> {
        #[cfg(windows)]
        return regs::set_regs(vcpu, r);
        #[cfg(not(windows))]
        { let _ = (vcpu, r); anyhow::bail!("WHP not available"); }
    }

    fn get_sregs(&self, vcpu: &WhpVcpu) -> Result<Sregs> {
        #[cfg(windows)]
        return regs::get_sregs(vcpu);
        #[cfg(not(windows))]
        { let _ = vcpu; anyhow::bail!("WHP not available"); }
    }

    fn set_sregs(&self, vcpu: &WhpVcpu, s: &Sregs) -> Result<()> {
        #[cfg(windows)]
        return regs::set_sregs(vcpu, s);
        #[cfg(not(windows))]
        { let _ = (vcpu, s); anyhow::bail!("WHP not available"); }
    }

    fn map_memory(&self, vm: &WhpVm, guest_phys: u64, host_mem: &[u8], flags: MemFlags) -> Result<()> {
        #[cfg(windows)]
        return partition::map_memory(vm, guest_phys, host_mem, flags);
        #[cfg(not(windows))]
        { let _ = (vm, guest_phys, host_mem, flags); anyhow::bail!("WHP not available"); }
    }

    fn unmap_memory(&self, vm: &WhpVm, guest_phys: u64, size: u64) -> Result<()> {
        #[cfg(windows)]
        return partition::unmap_memory(vm, guest_phys, size);
        #[cfg(not(windows))]
        { let _ = (vm, guest_phys, size); anyhow::bail!("WHP not available"); }
    }

    fn query_dirty_bitmap(&self, vm: &WhpVm, guest_phys: u64, size: u64) -> Result<DirtyBitmap> {
        #[cfg(windows)]
        return partition::query_dirty_bitmap(vm, guest_phys, size);
        #[cfg(not(windows))]
        { let _ = (vm, guest_phys, size); anyhow::bail!("WHP not available"); }
    }
}
