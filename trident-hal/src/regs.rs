//! Platform-agnostic CPU register structs.

/// x86-64 general-purpose and program-counter registers.
#[derive(Clone, Debug, Default)]
pub struct Regs {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rsp: u64,
    pub rbp: u64,
    pub r8:  u64,
    pub r9:  u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip:    u64,
    pub rflags: u64,
}

/// One x86-64 segment descriptor (for CS, DS, SS, ES, FS, GS, TR, LDTR).
#[derive(Clone, Debug, Default)]
pub struct Segment {
    pub base:     u64,
    pub limit:    u32,
    pub selector: u16,
    /// Segment type (4 bits from the access-rights byte).
    pub type_:    u8,
    /// S bit: 1 = code/data segment, 0 = system segment (TSS/LDT/gate).
    pub s:        u8,
    pub present:  u8,
    pub dpl:      u8,
    /// Default operation size (D/B bit).
    pub db:       u8,
    /// 64-bit code segment (L bit).
    pub l:        u8,
    /// Granularity (G bit) — limit is in 4 KiB units when set.
    pub g:        u8,
    pub avl:      u8,
    pub unusable: u8,
}

/// Control registers, EFER, and all segment descriptors.
#[derive(Clone, Debug, Default)]
pub struct Sregs {
    pub cr0:  u64,
    pub cr2:  u64,
    pub cr3:  u64,
    pub cr4:  u64,
    pub cr8:  u64,
    pub efer: u64,

    pub cs:   Segment,
    pub ds:   Segment,
    pub es:   Segment,
    pub fs:   Segment,
    pub gs:   Segment,
    pub ss:   Segment,
    pub tr:   Segment,
    pub ldt:  Segment,

    /// GDTR base and limit.
    pub gdt_base:  u64,
    pub gdt_limit: u16,
    /// IDTR base and limit.
    pub idt_base:  u64,
    pub idt_limit: u16,
}
