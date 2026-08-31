//! Register get/set for WHP virtual processors.

use anyhow::Result;
use tracing::debug;
use trident_hal::{Regs, Sregs};

use super::processor::WhpVcpu;

// ── GP registers: stubs ──────────────────────────────────────────────────────

pub fn get_regs(vcpu: &WhpVcpu) -> Result<Regs> {
    debug!("WHP get_regs stub for vCPU {}", vcpu.index);
    Ok(Regs::default())
}

pub fn set_regs(_vcpu: &WhpVcpu, _r: &Regs) -> Result<()> {
    Ok(())
}

// ── Segment registers: stubs ─────────────────────────────────────────────────

pub fn get_sregs(vcpu: &WhpVcpu) -> Result<Sregs> {
    debug!("WHP get_sregs stub for vCPU {}", vcpu.index);
    Ok(Sregs::default())
}

pub fn set_sregs(_vcpu: &WhpVcpu, _s: &Sregs) -> Result<()> {
    Ok(())
}
