#![cfg(target_os = "linux")]
//! Host-side guest RAM allocation helpers for the KVM backend.
//!
//! The KVM backend uses `vm-memory`'s `GuestMemoryMmap` for the allocation
//! but calls `map_memory` on `KvmVm` to register it with KVM.

use anyhow::{Context, Result};
use vm_memory::{GuestAddress, GuestMemory as _, GuestMemoryMmap};

/// Allocate anonymous host memory for a guest RAM region.
///
/// Returns a `GuestMemoryMmap` (which owns the mmap) and the host VA of
/// the first byte (needed for `KvmVm::map_memory`).
pub fn allocate_guest_ram(size_bytes: u64) -> Result<(GuestMemoryMmap, u64)> {
    let mem = GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), size_bytes as usize)])
        .context("Failed to mmap guest RAM")?;

    let host_addr = mem
        .get_host_address(GuestAddress(0))
        .context("Could not resolve host address for GuestAddress(0)")? as u64;

    Ok((mem, host_addr))
}
