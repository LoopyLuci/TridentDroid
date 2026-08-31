//! COW VM forking — Phase 3.
//!
//! Algorithm (identical on WHP and KVM via the HAL):
//!
//! 1. Pause parent vCPUs (HAL pause is implicit — vCPU threads exit run_vcpu
//!    and wait on a condvar; the fork coordinator holds a lock).
//! 2. Call `hyp.query_dirty_bitmap(vm, 0, mem_size)` — returns + resets bitmap.
//! 3. Create child VM: `hyp.create_vm()`.
//! 4. Map parent pages into child as READ|EXECUTE (no WRITE):
//!    `hyp.map_memory(child_vm, 0, &ram, MemFlags::RX_READONLY)`.
//! 5. On child `MmioWrite` exit: allocate new page, copy parent data, remap
//!    with WRITE|READ|EXECUTE.
//! 6. Resume parent (re-enable WRITE on its mapping).
//!
//! This module provides the data structures; the full implementation is Phase 3.

use trident_hal::DirtyBitmap;

pub struct ForkSnapshot {
    pub dirty: DirtyBitmap,
}
