#![cfg(target_os = "windows")]

use anyhow::Result;
use std::ptr;
use tracing::debug;
use trident_hal::VcpuExit;
use windows::Win32::System::Hypervisor::WHV_PARTITION_HANDLE;

use super::partition::WhpVm;

/// Virtual processor backed by Windows Hypervisor Platform.
pub struct WhpVcpu {
    pub(crate) partition: WHV_PARTITION_HANDLE,
    pub(crate) index: u32,
    /// Cached guest RAM pointer for MMIO fallback (not Send, handled via interior mutability).
    #[allow(dead_code)]
    pub(crate) ram_ptr: *const u8,
    #[allow(dead_code)]
    pub(crate) ram_len: usize,
}

// SAFETY: WhpVcpu is only accessed from a single thread (the vCPU runner thread).
unsafe impl Send for WhpVcpu {}
unsafe impl Sync for WhpVcpu {}

impl WhpVcpu {
    pub fn new(_vm: &WhpVm, index: u32) -> Result<Self> {
        unsafe {
            Ok(Self {
                partition: std::mem::zeroed(),
                index,
                ram_ptr: ptr::null(),
                ram_len: 0,
            })
        }
    }

    pub fn start_rip_sampler(&self, _interval_ms: u64) {
        // Stub
    }

    pub fn run(&mut self) -> Result<VcpuExit> {
        debug!("WHP vCPU {} run loop (stub)", self.index);
        Ok(VcpuExit::Hlt)
    }
}

impl Drop for WhpVcpu {
    fn drop(&mut self) {
        // Stub
    }
}
