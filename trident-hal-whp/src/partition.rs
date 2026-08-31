//! WHP partition (= VM) management.

use anyhow::{Context, Result};
use tracing::info;
use trident_hal::{DirtyBitmap, MemFlags};
use windows::Win32::System::Hypervisor::*;

/// A WHP partition handle.
///
/// `WHV_PARTITION_HANDLE` is a raw pointer-sized value; we wrap it in a
/// newtype so we can implement `Send + Sync` on it safely.
pub struct WhpVm {
    handle: WHV_PARTITION_HANDLE,
}

// SAFETY: WHP partition handles are safe to send across threads; all WHv*
// partition functions are documented as thread-safe.
unsafe impl Send for WhpVm {}
unsafe impl Sync for WhpVm {}

impl WhpVm {
    /// Create a new WHP partition and finalise it so vCPUs can be added.
    pub fn create(vcpu_count: u32) -> Result<Self> {
        let handle = unsafe {
            WHvCreatePartition().context("WHvCreatePartition failed")?
        };

        let prop = WHV_PARTITION_PROPERTY {
            ProcessorCount: vcpu_count,
        };
        unsafe {
            WHvSetPartitionProperty(
                handle,
                WHvPartitionPropertyCodeProcessorCount,
                &prop as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<WHV_PARTITION_PROPERTY>() as u32,
            )
            .context("WHvSetPartitionProperty(ProcessorCount) failed")?;
        }

        // Finalise the partition — no more property changes after this.
        unsafe {
            WHvSetupPartition(handle).context("WHvSetupPartition failed")?;
        }

        info!("WHP partition created");
        Ok(Self { handle })
    }

    pub(crate) fn handle(&self) -> WHV_PARTITION_HANDLE {
        self.handle
    }
}

impl Drop for WhpVm {
    fn drop(&mut self) {
        unsafe {
            let _ = WHvDeletePartition(self.handle);
        }
    }
}

// ── Memory management ────────────────────────────────────────────────────────

pub fn map_memory(vm: &WhpVm, guest_phys: u64, host_mem: &[u8], flags: MemFlags) -> Result<()> {
    let mut whp_flags = WHV_MAP_GPA_RANGE_FLAGS(0);

    if flags.contains(MemFlags::READ)    { whp_flags |= WHvMapGpaRangeFlagRead; }
    if flags.contains(MemFlags::WRITE)   { whp_flags |= WHvMapGpaRangeFlagWrite; }
    if flags.contains(MemFlags::EXECUTE) { whp_flags |= WHvMapGpaRangeFlagExecute; }
    if flags.contains(MemFlags::TRACK_DIRTY) {
        whp_flags |= WHvMapGpaRangeFlagTrackDirtyPages;
    }

    unsafe {
        WHvMapGpaRange(
            vm.handle(),
            host_mem.as_ptr() as *const std::ffi::c_void,
            guest_phys,
            host_mem.len() as u64,
            whp_flags,
        )
        .context("WHvMapGpaRange failed")?;
    }

    info!(
        "WHP GPA map: {:#x}..{:#x} flags={:?}",
        guest_phys,
        guest_phys + host_mem.len() as u64,
        flags
    );
    Ok(())
}

pub fn unmap_memory(vm: &WhpVm, guest_phys: u64, size: u64) -> Result<()> {
    unsafe {
        WHvUnmapGpaRange(vm.handle(), guest_phys, size)
            .context("WHvUnmapGpaRange failed")?;
    }
    Ok(())
}

/// Query and atomically reset the dirty-page bitmap for a GPA range.
///
/// WHP's `WHvQueryGpaRangeDirtyBitmap` resets the dirty bits in one atomic
/// operation — there is no separate reset step (unlike KVM's `get_dirty_log`).
pub fn query_dirty_bitmap(vm: &WhpVm, guest_phys: u64, size: u64) -> Result<DirtyBitmap> {
    let page_count  = size / 4096;
    let words_needed = ((page_count + 63) / 64) as usize;
    let bitmap_bytes = words_needed * 8;

    let mut words = vec![0u64; words_needed];

    unsafe {
        WHvQueryGpaRangeDirtyBitmap(
            vm.handle(),
            guest_phys,
            size,
            Some(words.as_mut_ptr()),
            bitmap_bytes as u32,
        )
        .context("WHvQueryGpaRangeDirtyBitmap failed")?;
    }

    Ok(DirtyBitmap { base_gpa: guest_phys, page_count, words })
}

// ── WHP availability check ───────────────────────────────────────────────────

/// Verify WHP is available on this system before attempting to create a partition.
///
/// Common failure reasons:
///   - Windows < build 17134
///   - "Windows Hypervisor Platform" optional feature not enabled
///   - Hyper-V not enabled
pub fn check_whp_available() -> Result<()> {
    let mut cap: WHV_CAPABILITY = unsafe { std::mem::zeroed() };
    let mut written = 0u32;

    unsafe {
        WHvGetCapability(
            WHvCapabilityCodeHypervisorPresent,
            &mut cap as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<WHV_CAPABILITY>() as u32,
            Some(&mut written as *mut u32),
        )
        .context("WHvGetCapability failed — is Windows Hypervisor Platform enabled?")?;
    }

    anyhow::ensure!(
        unsafe { cap.HypervisorPresent }.as_bool(),
        "WHP reports HypervisorPresent=false. \
         Enable 'Windows Hypervisor Platform' in Windows optional features \
         and ensure Hyper-V is active."
    );

    info!("WHP: HypervisorPresent=true");
    Ok(())
}
