//! TridentHAL — platform-agnostic hypervisor abstraction.
//!
//! The `Hypervisor` trait is the single seam between the portable VMM
//! (`tridentd`) and the host-OS virtualization API (WHP on Windows,
//! KVM on Linux).  Everything above the trait is pure Rust with no
//! platform-specific code; everything below it is a thin wrapper around
//! one OS API.

pub mod regs;
pub mod exit;
pub mod mem;

pub use exit::{VcpuAccess, VcpuExit};
pub use mem::{DirtyBitmap, MemFlags};
pub use regs::{Regs, Segment, Sregs};

use anyhow::Result;

/// The central abstraction.  One value of this type represents the
/// virtualization engine for the current process; implementations are
/// `WhpHypervisor` (Windows) and `KvmHypervisor` (Linux).
///
/// # Thread-safety contract
/// - The `Hypervisor` itself is `Send + Sync` — it may be cloned or
///   wrapped in `Arc` and shared across threads.
/// - `Vm` handles are `Send + Sync` — multiple vCPU threads share one VM.
/// - `Vcpu` handles are `Send` but **not** `Sync` — each vCPU must be
///   owned by exactly one OS thread at a time (required by both WHP and KVM).
pub trait Hypervisor: Send + Sync + 'static {
    /// An opaque VM handle.  Must be `Send + Sync` so it can be passed to
    /// multiple vCPU threads that hold a shared reference.
    type Vm: Send + Sync;

    /// An opaque vCPU handle.  `Send` so it can be moved to a dedicated
    /// OS thread; deliberately not `Sync` to prevent concurrent `run_vcpu`
    /// calls on the same vCPU.
    type Vcpu: Send;

    // ── VM lifecycle ────────────────────────────────────────────────────────

    /// Create a new, empty virtual machine with the given number of vCPUs.
    ///
    /// `vcpu_count` is required up-front because WHP's `WHvSetupPartition`
    /// needs `ProcessorCount` set before the partition is finalised.
    fn create_vm(&self, vcpu_count: u32) -> Result<Self::Vm>;

    // ── vCPU management ─────────────────────────────────────────────────────

    /// Create vCPU number `id` inside `vm`.  `id` must be unique per VM.
    fn create_vcpu(&self, vm: &Self::Vm, id: u32) -> Result<Self::Vcpu>;

    /// Run the vCPU until a terminal exit (Hlt/Shutdown/Debug).
    ///
    /// **Must be called from the thread that owns the vCPU.**
    /// Every PIO/MMIO access encountered along the way is handed to
    /// `on_access` synchronously — the callback must fill the buffer on
    /// read accesses — before the backend re-enters the vCPU, since that's
    /// the only point where the result can still reach the guest (a
    /// register write for reads, an unconditional RIP advance either way).
    fn run_vcpu(
        &self,
        vcpu: &mut Self::Vcpu,
        on_access: &mut dyn FnMut(VcpuAccess) -> Result<()>,
    ) -> Result<VcpuExit>;

    /// Optional hint: give the backend a read-only view of guest RAM so
    /// diagnostic samplers can inspect memory (e.g. dump instruction bytes).
    /// The default is a no-op.
    fn set_vcpu_ram_hint(&self, _vcpu: &mut Self::Vcpu, _ptr: *const u8, _len: usize) {}

    /// Spawn a background thread that periodically cancels the vCPU to sample
    /// its RIP. For debugging only — the default is a no-op.
    fn start_debug_sampler(&self, _vcpu: &Self::Vcpu, _interval_ms: u64) {}

    // ── Register access ─────────────────────────────────────────────────────

    fn get_regs(&self, vcpu: &Self::Vcpu) -> Result<Regs>;
    fn set_regs(&self, vcpu: &Self::Vcpu, regs: &Regs) -> Result<()>;
    fn get_sregs(&self, vcpu: &Self::Vcpu) -> Result<Sregs>;
    fn set_sregs(&self, vcpu: &Self::Vcpu, sregs: &Sregs) -> Result<()>;

    // ── Guest physical memory ────────────────────────────────────────────────

    /// Map a region of host memory into the guest's physical address space.
    ///
    /// `host_mem` must remain valid and pinned for the lifetime of the
    /// mapping; the caller is responsible for keeping the backing allocation
    /// alive.  `flags` controls read / write / execute permissions and
    /// whether dirty-page tracking is enabled (needed for forking).
    fn map_memory(
        &self,
        vm: &Self::Vm,
        guest_phys: u64,
        host_mem: &[u8],
        flags: MemFlags,
    ) -> Result<()>;

    /// Remove a GPA mapping previously established by `map_memory`.
    fn unmap_memory(&self, vm: &Self::Vm, guest_phys: u64, size: u64) -> Result<()>;

    /// Return and **atomically reset** the dirty-page bitmap for a GPA range.
    ///
    /// The range must have been mapped with `MemFlags::TRACK_DIRTY`.
    /// Each bit in the returned bitmap represents one 4 KiB page; bit `n`
    /// is set if the page at `guest_phys + n * 4096` was written since the
    /// last call.
    fn query_dirty_bitmap(
        &self,
        vm: &Self::Vm,
        guest_phys: u64,
        size: u64,
    ) -> Result<DirtyBitmap>;
}
