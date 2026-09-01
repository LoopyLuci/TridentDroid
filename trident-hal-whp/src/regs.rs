//! Register get/set for WHP virtual processors.

use anyhow::{Context, Result};
use trident_hal::{Regs, Segment, Sregs};
use windows::Win32::System::Hypervisor::{
    WHvGetVirtualProcessorRegisters, WHvSetVirtualProcessorRegisters, WHV_REGISTER_NAME,
    WHV_REGISTER_VALUE, WHV_X64_SEGMENT_REGISTER, WHV_X64_SEGMENT_REGISTER_0,
    WHV_X64_TABLE_REGISTER, WHvX64RegisterCr0, WHvX64RegisterCr2, WHvX64RegisterCr3,
    WHvX64RegisterCr4, WHvX64RegisterCr8, WHvX64RegisterCs, WHvX64RegisterDs, WHvX64RegisterEfer,
    WHvX64RegisterEs, WHvX64RegisterFs, WHvX64RegisterGdtr, WHvX64RegisterGs, WHvX64RegisterIdtr,
    WHvX64RegisterLdtr, WHvX64RegisterR10, WHvX64RegisterR11, WHvX64RegisterR12,
    WHvX64RegisterR13, WHvX64RegisterR14, WHvX64RegisterR15, WHvX64RegisterR8, WHvX64RegisterR9,
    WHvX64RegisterRax, WHvX64RegisterRbp, WHvX64RegisterRbx, WHvX64RegisterRcx,
    WHvX64RegisterRdi, WHvX64RegisterRdx, WHvX64RegisterRflags, WHvX64RegisterRip,
    WHvX64RegisterRsi, WHvX64RegisterRsp, WHvX64RegisterSs, WHvX64RegisterTr,
};

use super::processor::WhpVcpu;

const GP_NAMES: [WHV_REGISTER_NAME; 18] = [
    WHvX64RegisterRax,
    WHvX64RegisterRbx,
    WHvX64RegisterRcx,
    WHvX64RegisterRdx,
    WHvX64RegisterRsi,
    WHvX64RegisterRdi,
    WHvX64RegisterRsp,
    WHvX64RegisterRbp,
    WHvX64RegisterR8,
    WHvX64RegisterR9,
    WHvX64RegisterR10,
    WHvX64RegisterR11,
    WHvX64RegisterR12,
    WHvX64RegisterR13,
    WHvX64RegisterR14,
    WHvX64RegisterR15,
    WHvX64RegisterRip,
    WHvX64RegisterRflags,
];

const SREG_NAMES: [WHV_REGISTER_NAME; 16] = [
    WHvX64RegisterCr0,
    WHvX64RegisterCr2,
    WHvX64RegisterCr3,
    WHvX64RegisterCr4,
    WHvX64RegisterCr8,
    WHvX64RegisterEfer,
    WHvX64RegisterCs,
    WHvX64RegisterDs,
    WHvX64RegisterEs,
    WHvX64RegisterFs,
    WHvX64RegisterGs,
    WHvX64RegisterSs,
    WHvX64RegisterTr,
    WHvX64RegisterLdtr,
    WHvX64RegisterGdtr,
    WHvX64RegisterIdtr,
];

// ── GP registers ──────────────────────────────────────────────────────────────

pub fn get_regs(vcpu: &WhpVcpu) -> Result<Regs> {
    let mut values = [WHV_REGISTER_VALUE::default(); 18];
    unsafe {
        WHvGetVirtualProcessorRegisters(
            vcpu.partition,
            vcpu.index,
            GP_NAMES.as_ptr(),
            GP_NAMES.len() as u32,
            values.as_mut_ptr(),
        )
        .context("WHvGetVirtualProcessorRegisters (GP) failed")?;
    }
    let r = |i: usize| unsafe { values[i].Reg64 };
    Ok(Regs {
        rax: r(0),
        rbx: r(1),
        rcx: r(2),
        rdx: r(3),
        rsi: r(4),
        rdi: r(5),
        rsp: r(6),
        rbp: r(7),
        r8: r(8),
        r9: r(9),
        r10: r(10),
        r11: r(11),
        r12: r(12),
        r13: r(13),
        r14: r(14),
        r15: r(15),
        rip: r(16),
        rflags: r(17),
    })
}

pub fn set_regs(vcpu: &WhpVcpu, reg: &Regs) -> Result<()> {
    let mut values = [WHV_REGISTER_VALUE::default(); 18];
    values[0].Reg64 = reg.rax;
    values[1].Reg64 = reg.rbx;
    values[2].Reg64 = reg.rcx;
    values[3].Reg64 = reg.rdx;
    values[4].Reg64 = reg.rsi;
    values[5].Reg64 = reg.rdi;
    values[6].Reg64 = reg.rsp;
    values[7].Reg64 = reg.rbp;
    values[8].Reg64 = reg.r8;
    values[9].Reg64 = reg.r9;
    values[10].Reg64 = reg.r10;
    values[11].Reg64 = reg.r11;
    values[12].Reg64 = reg.r12;
    values[13].Reg64 = reg.r13;
    values[14].Reg64 = reg.r14;
    values[15].Reg64 = reg.r15;
    values[16].Reg64 = reg.rip;
    values[17].Reg64 = reg.rflags;
    unsafe {
        WHvSetVirtualProcessorRegisters(
            vcpu.partition,
            vcpu.index,
            GP_NAMES.as_ptr(),
            GP_NAMES.len() as u32,
            values.as_ptr(),
        )
        .context("WHvSetVirtualProcessorRegisters (GP) failed")
    }
}

// ── Segment/control registers ────────────────────────────────────────────────

pub fn get_sregs(vcpu: &WhpVcpu) -> Result<Sregs> {
    let mut values = [WHV_REGISTER_VALUE::default(); 16];
    unsafe {
        WHvGetVirtualProcessorRegisters(
            vcpu.partition,
            vcpu.index,
            SREG_NAMES.as_ptr(),
            SREG_NAMES.len() as u32,
            values.as_mut_ptr(),
        )
        .context("WHvGetVirtualProcessorRegisters (sregs) failed")?;
    }
    let r = |i: usize| unsafe { values[i].Reg64 };
    let seg = |i: usize| whp_seg_to_hal(unsafe { &values[i].Segment });
    let gdt = unsafe { values[14].Table };
    let idt = unsafe { values[15].Table };

    Ok(Sregs {
        cr0: r(0),
        cr2: r(1),
        cr3: r(2),
        cr4: r(3),
        cr8: r(4),
        efer: r(5),
        cs: seg(6),
        ds: seg(7),
        es: seg(8),
        fs: seg(9),
        gs: seg(10),
        ss: seg(11),
        tr: seg(12),
        ldt: seg(13),
        gdt_base: gdt.Base,
        gdt_limit: gdt.Limit,
        idt_base: idt.Base,
        idt_limit: idt.Limit,
    })
}

pub fn set_sregs(vcpu: &WhpVcpu, s: &Sregs) -> Result<()> {
    let mut values = [WHV_REGISTER_VALUE::default(); 16];
    values[0].Reg64 = s.cr0;
    values[1].Reg64 = s.cr2;
    values[2].Reg64 = s.cr3;
    values[3].Reg64 = s.cr4;
    values[4].Reg64 = s.cr8;
    values[5].Reg64 = s.efer;
    values[6].Segment = hal_seg_to_whp(&s.cs);
    values[7].Segment = hal_seg_to_whp(&s.ds);
    values[8].Segment = hal_seg_to_whp(&s.es);
    values[9].Segment = hal_seg_to_whp(&s.fs);
    values[10].Segment = hal_seg_to_whp(&s.gs);
    values[11].Segment = hal_seg_to_whp(&s.ss);
    values[12].Segment = hal_seg_to_whp(&s.tr);
    values[13].Segment = hal_seg_to_whp(&s.ldt);
    values[14].Table = WHV_X64_TABLE_REGISTER {
        Pad: [0; 3],
        Limit: s.gdt_limit,
        Base: s.gdt_base,
    };
    values[15].Table = WHV_X64_TABLE_REGISTER {
        Pad: [0; 3],
        Limit: s.idt_limit,
        Base: s.idt_base,
    };
    unsafe {
        WHvSetVirtualProcessorRegisters(
            vcpu.partition,
            vcpu.index,
            SREG_NAMES.as_ptr(),
            SREG_NAMES.len() as u32,
            values.as_ptr(),
        )
        .context("WHvSetVirtualProcessorRegisters (sregs) failed")
    }
}

// ── Segment attribute bitfield conversion ────────────────────────────────────
//
// WHV_X64_SEGMENT_REGISTER.Attributes packs (verified against the locally
// installed WinHvPlatformDefs.h):
//   bits 0-3  SegmentType      bit 7   Present
//   bit  4    NonSystemSegment bits 8-11 Reserved
//   bits 5-6  DPL              bit 12  Available (AVL)
//   bit 13    Long (L)         bit 14  Default (DB)   bit 15  Granularity (G)
//
// WHP has no explicit "unusable" bit — treated as the inverse of Present,
// matching how VMX-style unusable segments are conventionally derived.

fn whp_seg_to_hal(seg: &WHV_X64_SEGMENT_REGISTER) -> Segment {
    let attrs: u16 = unsafe { seg.Anonymous.Attributes };
    let present = ((attrs >> 7) & 1) as u8;
    Segment {
        base: seg.Base,
        limit: seg.Limit,
        selector: seg.Selector,
        type_: (attrs & 0xF) as u8,
        s: ((attrs >> 4) & 1) as u8,
        present,
        dpl: ((attrs >> 5) & 0x3) as u8,
        db: ((attrs >> 14) & 1) as u8,
        l: ((attrs >> 13) & 1) as u8,
        g: ((attrs >> 15) & 1) as u8,
        avl: ((attrs >> 12) & 1) as u8,
        unusable: (present == 0) as u8,
    }
}

fn hal_seg_to_whp(seg: &Segment) -> WHV_X64_SEGMENT_REGISTER {
    let attrs: u16 = ((seg.type_ as u16) & 0xF)
        | (((seg.s as u16) & 1) << 4)
        | (((seg.dpl as u16) & 0x3) << 5)
        | (((seg.present as u16) & 1) << 7)
        | (((seg.avl as u16) & 1) << 12)
        | (((seg.l as u16) & 1) << 13)
        | (((seg.db as u16) & 1) << 14)
        | (((seg.g as u16) & 1) << 15);
    WHV_X64_SEGMENT_REGISTER {
        Base: seg.base,
        Limit: seg.limit,
        Selector: seg.selector,
        Anonymous: WHV_X64_SEGMENT_REGISTER_0 { Attributes: attrs },
    }
}
