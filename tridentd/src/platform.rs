//! Platform backend selection.
//!
//! This is the single place where the compile-time backend is chosen.
//! All other code uses `PlatformHypervisor` via the `Hypervisor` trait.

use anyhow::Result;

// ── Backend type alias ────────────────────────────────────────────────────────

#[cfg(windows)]
pub type PlatformHypervisor = trident_hal_whp::WhpHypervisor;

#[cfg(target_os = "linux")]
pub type PlatformHypervisor = trident_hal_kvm::KvmHypervisor;

#[cfg(not(any(windows, target_os = "linux")))]
compile_error!("TridentDroid only supports Windows and Linux.");

// ── Constructor ───────────────────────────────────────────────────────────────

/// Open the platform hypervisor backend.
///
/// On Windows: verifies WHP is available (Hyper-V + optional feature enabled).
/// On Linux:   opens /dev/kvm and checks required capabilities.
pub fn open_hypervisor() -> Result<PlatformHypervisor> {
    #[cfg(windows)]
    { trident_hal_whp::WhpHypervisor::new() }

    #[cfg(target_os = "linux")]
    { trident_hal_kvm::KvmHypervisor::open() }
}
