//! Platform-agnostic VM exit reasons.
//!
//! Both WHP and KVM expose a superset of these exits; the HAL backends
//! translate their native exit structs into this enum before returning
//! to the VMM.  Unhandled platform-specific exits that the VMM need not
//! know about (e.g. APIC access, NMI windows) are handled silently inside
//! the backend and never appear here.

/// Result of one `Hypervisor::run_vcpu` call — only the exits that end the
/// run loop (nothing left for the backend to complete on its own).
///
/// PIO/MMIO accesses are *not* represented here: they need a result handed
/// back to the guest (a register write for a read, RIP advance either way)
/// before the vCPU can safely resume, which can only happen synchronously
/// inside the backend's own run loop — see `VcpuAccess` and
/// `Hypervisor::run_vcpu`'s `on_access` callback.
#[derive(Debug)]
#[non_exhaustive]
pub enum VcpuExit {
    /// Guest executed HLT with interrupts disabled — clean shutdown.
    Hlt,

    /// Platform signalled an unrecoverable guest fault.
    Shutdown,

    /// Debug breakpoint or single-step (used in Phase 5 debugger).
    Debug,
}

/// One PIO or MMIO access, handed to `Hypervisor::run_vcpu`'s `on_access`
/// callback synchronously (i.e. still inside the backend's native exit
/// handling, before it re-enters the vCPU). The callback must fill `data`
/// for the read variants before returning — the backend delivers whatever
/// is in the buffer at that point into the guest (a register for PIO/MMIO
/// reads), so leaving it unfilled means the guest sees zero, not an error.
#[derive(Debug)]
#[non_exhaustive]
pub enum VcpuAccess<'a> {
    /// Guest executed an IN instruction (port read). `data` is pre-sized to
    /// the access width (1, 2, or 4 bytes) — fill it with the read result.
    IoIn { port: u16, data: &'a mut [u8] },

    /// Guest executed an OUT instruction (port write); `data` holds what
    /// the guest wrote.
    IoOut { port: u16, data: &'a [u8] },

    /// Guest accessed an unmapped or MMIO GPA (read) — fill `data` with the
    /// bytes read from the device (or leave zeroed if unmapped).
    MmioRead { addr: u64, data: &'a mut [u8] },

    /// Guest accessed an unmapped or MMIO GPA (write); `data` holds what
    /// the guest wrote.
    MmioWrite { addr: u64, data: &'a [u8] },
}
