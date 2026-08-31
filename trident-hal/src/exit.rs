//! Platform-agnostic VM exit reasons.
//!
//! Both WHP and KVM expose a superset of these exits; the HAL backends
//! translate their native exit structs into this enum before returning
//! to the VMM.  Unhandled platform-specific exits that the VMM need not
//! know about (e.g. APIC access, NMI windows) are handled silently inside
//! the backend and never appear here.

/// Result of one `Hypervisor::run_vcpu` call.
#[derive(Debug)]
#[non_exhaustive]
pub enum VcpuExit {
    /// Guest executed an IN instruction (port read).
    IoIn {
        port: u16,
        /// Pre-sized to the access width (1, 2, or 4 bytes); backend fills it.
        data: Vec<u8>,
    },

    /// Guest executed an OUT instruction (port write).
    IoOut {
        port: u16,
        data: Vec<u8>,
    },

    /// Guest accessed an unmapped or MMIO GPA (read).
    /// The backend must fill `data` with the bytes read from the device
    /// (or zeroed if the device is unmapped).
    MmioRead {
        addr: u64,
        len: usize,
        /// Data read from the device (or zeroed placeholder if not handled).
        data: Vec<u8>,
    },

    /// Guest accessed an unmapped or MMIO GPA (write).
    MmioWrite {
        addr: u64,
        data: Vec<u8>,
    },

    /// Guest executed HLT with interrupts disabled — clean shutdown.
    Hlt,

    /// Platform signalled an unrecoverable guest fault.
    Shutdown,

    /// Debug breakpoint or single-step (used in Phase 5 debugger).
    Debug,
}
