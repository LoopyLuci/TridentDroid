#![cfg(target_os = "linux")]
use anyhow::{Context, Result};
use kvm_bindings::{kvm_pit_config, kvm_userspace_memory_region, KVM_MEM_LOG_DIRTY_PAGES,
                   KVM_MEM_READONLY};
use kvm_ioctls::{Kvm, VmFd};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use tracing::info;
use trident_hal::{DirtyBitmap, MemFlags};

/// A KVM VM handle.  Wraps `VmFd` in an `Arc` so multiple `KvmVcpu`s can
/// hold a reference without duplicating the fd.
pub struct KvmVm {
    pub(crate) fd: Arc<VmFd>,
    /// Next slot index — KVM requires each `set_user_memory_region` call to
    /// use a unique, stable slot number.
    next_slot: AtomicU32,
}

// SAFETY: VmFd wraps a file descriptor; KVM VM ioctls are thread-safe.
unsafe impl Send for KvmVm {}
unsafe impl Sync for KvmVm {}

impl KvmVm {
    pub fn new(kvm: &Kvm) -> Result<Self> {
        let fd = Arc::new(kvm.create_vm().context("KVM create_vm failed")?);
        info!("KVM VM created (api_version={})", kvm.get_api_version());

        // In-kernel IRQ chip (APIC) — must be created before vCPUs.
        fd.create_irq_chip().context("create_irq_chip failed")?;

        // In-kernel PIT (timer).
        fd.create_pit2(kvm_pit_config { flags: 0, ..Default::default() })
            .context("create_pit2 failed")?;

        Ok(Self { fd, next_slot: AtomicU32::new(0) })
    }

    pub fn map_memory(&self, guest_phys: u64, host_mem: &[u8], flags: MemFlags) -> Result<()> {
        let slot = self.next_slot.fetch_add(1, Ordering::Relaxed);

        let mut kvm_flags: u32 = 0;
        if flags.contains(MemFlags::TRACK_DIRTY) {
            kvm_flags |= KVM_MEM_LOG_DIRTY_PAGES;
        }
        if !flags.contains(MemFlags::WRITE) {
            kvm_flags |= KVM_MEM_READONLY;
        }

        let region = kvm_userspace_memory_region {
            slot,
            guest_phys_addr: guest_phys,
            memory_size: host_mem.len() as u64,
            userspace_addr: host_mem.as_ptr() as u64,
            flags: kvm_flags,
        };

        // SAFETY: caller guarantees `host_mem` stays valid for the mapping lifetime.
        unsafe {
            self.fd
                .set_user_memory_region(region)
                .with_context(|| format!("KVM set_user_memory_region slot={}", slot))?;
        }

        info!(
            "KVM memory slot {}: GPA {:#x}..{:#x} flags={:?}",
            slot,
            guest_phys,
            guest_phys + host_mem.len() as u64,
            flags,
        );
        Ok(())
    }

    pub fn unmap_memory(&self, guest_phys: u64, size: u64) -> Result<()> {
        // Unmap by setting memory_size = 0 on the slot that covers guest_phys.
        // For simplicity we track slot→gpa; a production implementation should
        // maintain a slot map.  Stub for Phase 3.
        let _ = (guest_phys, size);
        todo!("Phase 3: slot map needed to look up slot by GPA for unmap")
    }

    pub fn query_dirty_bitmap(&self, guest_phys: u64, size: u64) -> Result<DirtyBitmap> {
        let page_count = size / 4096;
        let words_needed = ((page_count + 63) / 64) as usize;
        let mut words = vec![0u64; words_needed];

        // Slot 0 is assumed to cover the full RAM range (single-slot design).
        // Phase 3 will generalise this to the slot map.
        self.fd
            .get_dirty_log(0, &mut words)
            .context("KVM_GET_DIRTY_LOG failed")?;

        Ok(DirtyBitmap { base_gpa: guest_phys, page_count, words })
    }
}
