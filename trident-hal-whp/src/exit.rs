//! WHP exit handling stubs
//!
//! Converts native WHP exit reasons into the HAL's platform-agnostic
//! `VcpuExit` enum.  This is a stub — a real implementation would parse
//! the exit context structs.

use trident_hal::VcpuExit;
use windows::Win32::System::Hypervisor::WHV_RUN_VP_EXIT_REASON;

/// Convert a WHP VP exit reason.  Returns `Some(VcpuExit)` for terminal
/// exits, `None` to re-enter the VP.
pub fn classify_exit(_exit_reason: WHV_RUN_VP_EXIT_REASON) -> Option<VcpuExit> {
    // Stub: always return Hlt so the VM exits cleanly
    // A real implementation would dispatch on the actual exit reason,
    // parse IO/Memory access contexts, synthesize CPUID results, etc.
    Some(VcpuExit::Hlt)
}
