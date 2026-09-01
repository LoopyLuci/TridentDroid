//! Converts native WHP exit reasons into the HAL's platform-agnostic
//! `VcpuExit`/`VcpuAccess` types.
//!
//! Neither PIO nor MMIO exits are retired by WHP before it signals the
//! exit — the VMM must always advance RIP itself, and for reads, must
//! also write the result into a guest register itself (WHP does not do
//! this for you either). Both PIO and MMIO reads are therefore handled
//! synchronously right here, via the `on_access` callback threaded down
//! from `Hypervisor::run_vcpu` — there is no other point where the result
//! can still reach the guest before the vCPU resumes.
//!
//! The two exit contexts differ in what they hand back for advancing RIP
//! (confirmed empirically, not just from the header, since
//! `WHV_X64_IO_PORT_ACCESS_CONTEXT`'s `InstructionByteCount` is documented
//! in the field list but is actually always 0 in practice):
//!
//! - `WHV_MEMORY_ACCESS_CONTEXT` (MMIO) genuinely populates
//!   `InstructionBytes`/`InstructionByteCount`, so we decode the trapped
//!   instruction directly from the exit context.
//! - `WHV_X64_IO_PORT_ACCESS_CONTEXT` (PIO) does not — `InstructionByteCount`
//!   is always 0. To advance RIP we read the trapped instruction straight
//!   out of guest RAM at `Cs.Base + Rip` (using the `ram_ptr`/`ram_len` hint
//!   threaded through `Hypervisor::set_vcpu_ram_hint`) and decode just the
//!   IN/OUT opcode forms.
//!
//! For MMIO, no effective-address computation is needed either way since
//! `Gpa` is already resolved by WHP — decoding is scoped to plain MOV forms
//! (0x88/0x89/0x8A/0x8B), which covers the 32-bit register-based MMIO ABI
//! used by both the IOAPIC and virtio-mmio; immediate-store forms
//! (0xC6/0xC7) fall back to a zero-value write rather than erroring, and an
//! unrecognized MMIO *read* form simply drops the value (logged) since
//! there's no destination register to write it into.

use anyhow::{Context, Result};
use tracing::debug;
use trident_hal::{VcpuAccess, VcpuExit};
use windows::Win32::System::Hypervisor::{
    WHvGetVirtualProcessorRegisters, WHvRunVpExitReasonMemoryAccess,
    WHvRunVpExitReasonUnrecoverableException, WHvRunVpExitReasonX64Halt,
    WHvRunVpExitReasonX64IoPortAccess, WHvSetVirtualProcessorRegisters, WHvX64RegisterR10,
    WHvX64RegisterR11, WHvX64RegisterR12, WHvX64RegisterR13, WHvX64RegisterR14,
    WHvX64RegisterR15, WHvX64RegisterR8, WHvX64RegisterR9, WHvX64RegisterRax,
    WHvX64RegisterRbp, WHvX64RegisterRbx, WHvX64RegisterRcx, WHvX64RegisterRdi,
    WHvX64RegisterRdx, WHvX64RegisterRip, WHvX64RegisterRsi, WHvX64RegisterRsp,
    WHV_MEMORY_ACCESS_CONTEXT, WHV_PARTITION_HANDLE, WHV_REGISTER_NAME, WHV_REGISTER_VALUE,
    WHV_RUN_VP_EXIT_CONTEXT, WHV_VP_EXIT_CONTEXT, WHV_X64_IO_PORT_ACCESS_CONTEXT,
};

/// ModRM.reg (extended by REX.R) -> WHV_REGISTER_NAME, in the standard x86
/// GPR encoding order (0=RAX,1=RCX,...,7=RDI,8=R8,...,15=R15).
const REG_NAME_BY_INDEX: [WHV_REGISTER_NAME; 16] = [
    WHvX64RegisterRax,
    WHvX64RegisterRcx,
    WHvX64RegisterRdx,
    WHvX64RegisterRbx,
    WHvX64RegisterRsp,
    WHvX64RegisterRbp,
    WHvX64RegisterRsi,
    WHvX64RegisterRdi,
    WHvX64RegisterR8,
    WHvX64RegisterR9,
    WHvX64RegisterR10,
    WHvX64RegisterR11,
    WHvX64RegisterR12,
    WHvX64RegisterR13,
    WHvX64RegisterR14,
    WHvX64RegisterR15,
];

/// Convert one WHP VP exit. Returns `Some(VcpuExit)` for terminal exits the
/// run loop must return to its caller, `None` to re-enter the VP (either a
/// PIO/MMIO access was fully handled in-place via `on_access` — including
/// advancing RIP — or the exit reason needs no VMM action at all).
///
/// `ram` is a read-only view of guest RAM at GPA 0 (from
/// `Hypervisor::set_vcpu_ram_hint`), needed to decode trapped PIO
/// instructions — see the module doc. `None` if the hint hasn't been set;
/// PIO exits will then fail to advance RIP and re-trap (logged, not silent).
pub fn classify_exit(
    partition: WHV_PARTITION_HANDLE,
    index: u32,
    ctx: &WHV_RUN_VP_EXIT_CONTEXT,
    ram: Option<&[u8]>,
    on_access: &mut dyn FnMut(VcpuAccess) -> Result<()>,
) -> Result<Option<VcpuExit>> {
    match ctx.ExitReason {
        WHvRunVpExitReasonX64Halt => Ok(Some(VcpuExit::Hlt)),

        WHvRunVpExitReasonX64IoPortAccess => {
            handle_io(
                partition,
                index,
                unsafe { &ctx.Anonymous.IoPortAccess },
                &ctx.VpContext,
                ram,
                on_access,
            )?;
            Ok(None)
        }

        WHvRunVpExitReasonMemoryAccess => {
            handle_mmio(
                partition,
                index,
                unsafe { &ctx.Anonymous.MemoryAccess },
                ctx.VpContext.Rip,
                on_access,
            )?;
            Ok(None)
        }

        WHvRunVpExitReasonUnrecoverableException => Ok(Some(VcpuExit::Shutdown)),

        other => {
            debug!(
                "WHP vCPU {} unhandled exit {:?} at rip={:#x} — re-entering",
                index, other, ctx.VpContext.Rip
            );
            Ok(None)
        }
    }
}

// ── IO port access ────────────────────────────────────────────────────────────

fn handle_io(
    partition: WHV_PARTITION_HANDLE,
    index: u32,
    io: &WHV_X64_IO_PORT_ACCESS_CONTEXT,
    vp: &WHV_VP_EXIT_CONTEXT,
    ram: Option<&[u8]>,
    on_access: &mut dyn FnMut(VcpuAccess) -> Result<()>,
) -> Result<()> {
    // AccessInfo bitfield (verified against WinHvPlatformDefs.h):
    // bit0 IsWrite, bits1-3 AccessSize (byte count), bit4 StringOp, bit5 RepPrefix.
    let info = unsafe { io.AccessInfo.AsUINT32 };
    let is_write = (info & 0x1) != 0;
    let width = (((info >> 1) & 0x7) as usize).clamp(1, 4);

    if is_write {
        let bytes = io.Rax.to_le_bytes();
        on_access(VcpuAccess::IoOut {
            port: io.PortNumber,
            data: &bytes[..width],
        })?;
    } else {
        let mut buf = [0u8; 4];
        on_access(VcpuAccess::IoIn {
            port: io.PortNumber,
            data: &mut buf[..width],
        })?;
        // IN only replaces the low `width` bytes of RAX — the rest of the
        // register is preserved, matching real x86 IN semantics.
        let mut rax_bytes = io.Rax.to_le_bytes();
        rax_bytes[..width].copy_from_slice(&buf[..width]);
        write_reg64(partition, index, WHvX64RegisterRax, u64::from_le_bytes(rax_bytes))?;
    }

    let phys = vp.Cs.Base.wrapping_add(vp.Rip);
    let insn_len = ram.and_then(|ram| {
        let start = phys as usize;
        ram.get(start..(start + 16).min(ram.len()))
            .and_then(decode_io_insn_len)
    });

    match insn_len {
        Some(len) => set_rip(partition, index, vp.Rip + len as u64)?,
        None => debug!(
            "WHP vCPU {} IoPortAccess at gpa={:#x} rip={:#x}: could not determine instruction \
             length (no RAM hint set, or undecodable opcode) — RIP not advanced, will re-trap",
            index, phys, vp.Rip
        ),
    }

    Ok(())
}

/// Decode the length of a trapped IN/OUT instruction. WHP does not report
/// this for PIO exits, so unlike MMIO decode this reads real guest bytes
/// rather than a hardware-supplied copy.
fn decode_io_insn_len(bytes: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            0x66 | 0xF2 | 0xF3 => i += 1, // operand-size / rep prefixes
            _ => break,
        }
    }
    let opcode = *bytes.get(i)?;
    let base_len = i + 1;
    match opcode {
        // IN/OUT AL|eAX, imm8  and  OUT imm8, AL|eAX — carry a 1-byte port immediate.
        0xE4 | 0xE5 | 0xE6 | 0xE7 => Some(base_len + 1),
        // IN/OUT AL|eAX, DX — port comes from DX, no immediate.
        0xEC | 0xED | 0xEE | 0xEF => Some(base_len),
        // INS/OUTS (string forms).
        0x6C | 0x6D | 0x6E | 0x6F => Some(base_len),
        _ => None,
    }
}

// ── Memory access (MMIO) — needs instruction decode + manual RIP advance ─────

fn handle_mmio(
    partition: WHV_PARTITION_HANDLE,
    index: u32,
    mem: &WHV_MEMORY_ACCESS_CONTEXT,
    rip: u64,
    on_access: &mut dyn FnMut(VcpuAccess) -> Result<()>,
) -> Result<()> {
    // AccessInfo bitfield: bits0-1 AccessType (0=Read,1=Write,2=Execute).
    let info = unsafe { mem.AccessInfo.AsUINT32 };
    let is_write = (info & 0x3) == 1;

    let insn_len = mem.InstructionByteCount as usize;
    let insn = &mem.InstructionBytes[..insn_len.min(mem.InstructionBytes.len())];
    let decoded = decode_mov(insn);
    let width = decoded.as_ref().map_or(4, |d| d.width);

    if is_write {
        let data = match decoded.as_ref().and_then(|d| d.reg_index) {
            Some(reg_idx) => {
                let value = read_gpr(partition, index, reg_idx)?;
                value.to_le_bytes()[..width].to_vec()
            }
            None => {
                debug!(
                    "WHP vCPU {} MMIO write at gpa={:#x} rip={:#x}: unrecognized instruction \
                     (bytes={:02x?}) — writing zero",
                    index, mem.Gpa, rip, insn
                );
                vec![0u8; width]
            }
        };
        on_access(VcpuAccess::MmioWrite { addr: mem.Gpa, data: &data })?;
    } else {
        let mut buf = [0u8; 8];
        on_access(VcpuAccess::MmioRead {
            addr: mem.Gpa,
            data: &mut buf[..width],
        })?;
        match decoded.as_ref().and_then(|d| d.reg_index) {
            Some(reg_idx) => {
                let mut val_bytes = [0u8; 8];
                val_bytes[..width].copy_from_slice(&buf[..width]);
                write_reg64(
                    partition,
                    index,
                    REG_NAME_BY_INDEX[(reg_idx & 0xF) as usize],
                    u64::from_le_bytes(val_bytes),
                )?;
            }
            None => debug!(
                "WHP vCPU {} MMIO read at gpa={:#x} rip={:#x}: unrecognized instruction \
                 (bytes={:02x?}) — destination register unknown, value dropped",
                index, mem.Gpa, rip, insn
            ),
        }
    }

    // WHP never advances RIP for us on MMIO exits — do it now, using WHP's
    // own reported instruction length (not a length we computed).
    set_rip(partition, index, rip + mem.InstructionByteCount as u64)?;

    Ok(())
}

struct DecodedMov {
    width: usize,
    /// ModRM.reg (extended by REX.R), meaningful for both directions here:
    /// the source register for a store, the destination register for a load.
    reg_index: Option<u8>,
}

/// Decode just enough of a trapped MMIO instruction to recover its access
/// width and which GPR is the source (store) or destination (load).
/// Scoped to `mov r/m, r` / `mov r, r/m` (0x88/0x89/0x8A/0x8B) — the forms
/// IOAPIC and virtio-mmio actually use. Returns `None` for anything else
/// (e.g. immediate-store `mov r/m, imm`), which the caller treats as a
/// width-4 zero-value write, or a dropped/logged value for a load.
fn decode_mov(bytes: &[u8]) -> Option<DecodedMov> {
    let mut i = 0;
    let mut operand_size_override = false;
    let mut rex_w = false;
    let mut rex_r = false;

    while i < bytes.len() {
        match bytes[i] {
            0x66 => {
                operand_size_override = true;
                i += 1;
            }
            0x67 | 0xF0 | 0xF2 | 0xF3 | 0x2E | 0x36 | 0x3E | 0x26 | 0x64 | 0x65 => {
                i += 1;
            }
            b @ 0x40..=0x4F => {
                rex_w = (b & 0x08) != 0;
                rex_r = (b & 0x04) != 0;
                i += 1;
            }
            _ => break,
        }
    }

    let opcode = *bytes.get(i)?;
    i += 1;
    let byte_op = match opcode {
        0x88 | 0x8A => true,
        0x89 | 0x8B => false,
        _ => return None,
    };

    let width = if byte_op {
        1
    } else if rex_w {
        8
    } else if operand_size_override {
        2
    } else {
        4
    };

    let reg_index = bytes
        .get(i)
        .map(|modrm| ((modrm >> 3) & 0x7) | if rex_r { 0x8 } else { 0 });

    Some(DecodedMov { width, reg_index })
}

fn read_gpr(partition: WHV_PARTITION_HANDLE, index: u32, reg_idx: u8) -> Result<u64> {
    let name = REG_NAME_BY_INDEX[(reg_idx & 0xF) as usize];
    let mut value = WHV_REGISTER_VALUE::default();
    unsafe {
        WHvGetVirtualProcessorRegisters(partition, index, &name, 1, &mut value)
            .context("WHvGetVirtualProcessorRegisters (MMIO source reg) failed")?;
        Ok(value.Reg64)
    }
}

fn write_reg64(partition: WHV_PARTITION_HANDLE, index: u32, name: WHV_REGISTER_NAME, value: u64) -> Result<()> {
    let mut v = WHV_REGISTER_VALUE::default();
    v.Reg64 = value;
    unsafe {
        WHvSetVirtualProcessorRegisters(partition, index, &name, 1, &v)
            .context("WHvSetVirtualProcessorRegisters failed")
    }
}

fn set_rip(partition: WHV_PARTITION_HANDLE, index: u32, rip: u64) -> Result<()> {
    write_reg64(partition, index, WHvX64RegisterRip, rip)
}
