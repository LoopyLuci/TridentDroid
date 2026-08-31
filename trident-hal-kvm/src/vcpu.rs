#![cfg(target_os = "linux")]

use anyhow::{Context, Result};
use kvm_ioctls::VcpuExit as KvmExit;
use tracing::debug;
use trident_hal::{Regs, Segment, Sregs, VcpuExit};

use super::KvmVm;

pub struct KvmVcpu {
    pub(crate) id: u32,
    fd: kvm_ioctls::VcpuFd,
}

impl KvmVcpu {
    pub fn new(vm: &KvmVm, id: u32) -> Result<Self> {
        let fd = vm
            .fd
            .create_vcpu(id as u64)
            .with_context(|| format!("KVM create_vcpu({}) failed", id))?;
        Ok(Self { id, fd })
    }

    pub fn run(&mut self) -> Result<VcpuExit> {
        loop {
            match self.fd.run().context("KVM_RUN failed")? {
                KvmExit::IoIn(port, data) => {
                    return Ok(VcpuExit::IoIn {
                        port,
                        data: data.to_vec(),
                    });
                }
                KvmExit::IoOut(port, data) => {
                    return Ok(VcpuExit::IoOut {
                        port,
                        data: data.to_vec(),
                    });
                }
                KvmExit::MmioRead(addr, data) => {
                    return Ok(VcpuExit::MmioRead {
                        addr,
                        len: data.len(),
                        data: data.to_vec(),
                    });
                }
                KvmExit::MmioWrite(addr, data) => {
                    return Ok(VcpuExit::MmioWrite {
                        addr,
                        data: data.to_vec(),
                    });
                }
                KvmExit::Hlt => return Ok(VcpuExit::Hlt),
                KvmExit::Shutdown => return Ok(VcpuExit::Shutdown),
                KvmExit::Debug(_) => return Ok(VcpuExit::Debug),
                KvmExit::IrqWindowOpen => {
                    debug!("vCPU {} IrqWindowOpen — re-entering", self.id);
                }
                other => {
                    debug!(
                        "vCPU {} unhandled KVM exit {:?} — re-entering",
                        self.id, other
                    );
                }
            }
        }
    }

    pub fn get_regs(&self) -> Result<Regs> {
        let k = self.fd.get_regs().context("KVM get_regs")?;
        Ok(Regs {
            rax: k.rax,
            rbx: k.rbx,
            rcx: k.rcx,
            rdx: k.rdx,
            rsi: k.rsi,
            rdi: k.rdi,
            rsp: k.rsp,
            rbp: k.rbp,
            r8: k.r8,
            r9: k.r9,
            r10: k.r10,
            r11: k.r11,
            r12: k.r12,
            r13: k.r13,
            r14: k.r14,
            r15: k.r15,
            rip: k.rip,
            rflags: k.rflags,
        })
    }

    pub fn set_regs(&self, r: &Regs) -> Result<()> {
        let k = kvm_bindings::kvm_regs {
            rax: r.rax,
            rbx: r.rbx,
            rcx: r.rcx,
            rdx: r.rdx,
            rsi: r.rsi,
            rdi: r.rdi,
            rsp: r.rsp,
            rbp: r.rbp,
            r8: r.r8,
            r9: r.r9,
            r10: r.r10,
            r11: r.r11,
            r12: r.r12,
            r13: r.r13,
            r14: r.r14,
            r15: r.r15,
            rip: r.rip,
            rflags: r.rflags,
        };
        self.fd.set_regs(&k).context("KVM set_regs")
    }

    pub fn get_sregs(&self) -> Result<Sregs> {
        let k = self.fd.get_sregs().context("KVM get_sregs")?;
        Ok(Sregs {
            cr0: k.cr0,
            cr2: k.cr2,
            cr3: k.cr3,
            cr4: k.cr4,
            cr8: k.cr8,
            efer: k.efer,
            cs: kvm_seg_to_hal(&k.cs),
            ds: kvm_seg_to_hal(&k.ds),
            es: kvm_seg_to_hal(&k.es),
            fs: kvm_seg_to_hal(&k.fs),
            gs: kvm_seg_to_hal(&k.gs),
            ss: kvm_seg_to_hal(&k.ss),
            tr: kvm_seg_to_hal(&k.tr),
            ldt: kvm_seg_to_hal(&k.ldt),
            gdt_base: k.gdt.base,
            gdt_limit: k.gdt.limit,
            idt_base: k.idt.base,
            idt_limit: k.idt.limit,
        })
    }

    pub fn set_sregs(&self, s: &Sregs) -> Result<()> {
        let mut k = self.fd.get_sregs().context("KVM get_sregs (for set)")?;
        k.cr0 = s.cr0;
        k.cr2 = s.cr2;
        k.cr3 = s.cr3;
        k.cr4 = s.cr4;
        k.cr8 = s.cr8;
        k.efer = s.efer;
        k.cs = hal_seg_to_kvm(&s.cs);
        k.ds = hal_seg_to_kvm(&s.ds);
        k.es = hal_seg_to_kvm(&s.es);
        k.fs = hal_seg_to_kvm(&s.fs);
        k.gs = hal_seg_to_kvm(&s.gs);
        k.ss = hal_seg_to_kvm(&s.ss);
        k.tr = hal_seg_to_kvm(&s.tr);
        k.ldt = hal_seg_to_kvm(&s.ldt);
        k.gdt = kvm_bindings::kvm_dtable {
            base: s.gdt_base,
            limit: s.gdt_limit,
            padding: [0; 3],
        };
        k.idt = kvm_bindings::kvm_dtable {
            base: s.idt_base,
            limit: s.idt_limit,
            padding: [0; 3],
        };
        self.fd.set_sregs(&k).context("KVM set_sregs")
    }
}

fn kvm_seg_to_hal(s: &kvm_bindings::kvm_segment) -> Segment {
    Segment {
        base: s.base,
        limit: s.limit,
        selector: s.selector,
        type_: s.type_,
        s: s.s,
        present: s.present,
        dpl: s.dpl,
        db: s.db,
        l: s.l,
        g: s.g,
        avl: s.avl,
        unusable: s.unusable,
    }
}

fn hal_seg_to_kvm(s: &Segment) -> kvm_bindings::kvm_segment {
    kvm_bindings::kvm_segment {
        base: s.base,
        limit: s.limit,
        selector: s.selector,
        type_: s.type_,
        s: s.s,
        present: s.present,
        dpl: s.dpl,
        db: s.db,
        l: s.l,
        g: s.g,
        avl: s.avl,
        unusable: s.unusable,
        padding: 0,
    }
}
