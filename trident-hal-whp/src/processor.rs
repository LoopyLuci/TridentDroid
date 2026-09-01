#![cfg(target_os = "windows")]

use anyhow::{Context, Result};
use std::ptr;
use tracing::error;
use trident_hal::{VcpuAccess, VcpuExit};
use windows::Win32::System::Hypervisor::{
    WHvCreateVirtualProcessor, WHvDeleteVirtualProcessor, WHvRunVirtualProcessor,
    WHV_PARTITION_HANDLE, WHV_RUN_VP_EXIT_CONTEXT,
};

use super::exit::classify_exit;
use super::partition::WhpVm;

/// Virtual processor backed by Windows Hypervisor Platform.
pub struct WhpVcpu {
    pub(crate) partition: WHV_PARTITION_HANDLE,
    pub(crate) index: u32,
    /// Cached guest RAM pointer, set via `set_vcpu_ram_hint`. Used to decode
    /// trapped PIO instructions (WHP doesn't report their length itself).
    pub(crate) ram_ptr: *const u8,
    pub(crate) ram_len: usize,
}

// SAFETY: WhpVcpu is only accessed from a single thread (the vCPU runner thread).
unsafe impl Send for WhpVcpu {}
unsafe impl Sync for WhpVcpu {}

impl WhpVcpu {
    pub fn new(vm: &WhpVm, index: u32) -> Result<Self> {
        let partition = vm.handle();
        unsafe {
            WHvCreateVirtualProcessor(partition, index, 0)
                .context("WHvCreateVirtualProcessor failed")?;
        }
        Ok(Self {
            partition,
            index,
            ram_ptr: ptr::null(),
            ram_len: 0,
        })
    }

    pub fn start_rip_sampler(&self, _interval_ms: u64) {
        // Stub — WHvCancelRunVirtualProcessor-based sampling is future work.
    }

    pub fn run(&mut self, on_access: &mut dyn FnMut(VcpuAccess) -> Result<()>) -> Result<VcpuExit> {
        loop {
            let mut ctx = WHV_RUN_VP_EXIT_CONTEXT::default();
            unsafe {
                WHvRunVirtualProcessor(
                    self.partition,
                    self.index,
                    &mut ctx as *mut _ as *mut std::ffi::c_void,
                    std::mem::size_of::<WHV_RUN_VP_EXIT_CONTEXT>() as u32,
                )
                .context("WHvRunVirtualProcessor failed")?;
            }

            // SAFETY: `ram_ptr`/`ram_len` are set once via `set_vcpu_ram_hint`
            // before the vCPU thread starts running, and must remain valid
            // for the vCPU's lifetime (the caller's contract, same as for
            // the KVM backend's guest memory).
            let ram = if self.ram_ptr.is_null() {
                None
            } else {
                Some(unsafe { std::slice::from_raw_parts(self.ram_ptr, self.ram_len) })
            };

            if let Some(exit) = classify_exit(self.partition, self.index, &ctx, ram, on_access)? {
                return Ok(exit);
            }
            // `None` means the exit was fully handled in-place (RIP advanced
            // where needed) — re-enter the hypervisor.
        }
    }
}

impl Drop for WhpVcpu {
    fn drop(&mut self) {
        unsafe {
            if let Err(e) = WHvDeleteVirtualProcessor(self.partition, self.index) {
                error!(
                    "WHvDeleteVirtualProcessor failed for vCPU {}: {:?}",
                    self.index, e
                );
            }
        }
    }
}
