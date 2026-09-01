//! VcpuRunner - platform-agnostic vCPU run loop and register configuration.
//!
//! This module contains only trait-generic code; it has no knowledge of
//! whether the backend is WHP or KVM.

use anyhow::{Context, Result};
use std::io::Write;
use std::sync::Arc;
use tracing::{debug, info};
use trident_hal::{Hypervisor, Regs, Segment, Sregs, VcpuAccess, VcpuExit};

use super::pause::{is_pause_requested, ParkedVcpu, PauseGate, PauseRequested};

pub struct VcpuRunner;

impl VcpuRunner {
    pub fn configure_boot_regs<H: Hypervisor>(
        hyp: &H,
        vcpu: &mut H::Vcpu,
        kernel_entry_gpa: u64,
        _mem_size: u64,
    ) -> Result<()> {
        let regs = Regs {
            rip: kernel_entry_gpa,
            rsi: 0x0001_0000,
            rsp: 0x0008_0000,
            rflags: 0x0000_0002,
            ..Default::default()
        };
        hyp.set_regs(vcpu, &regs).context("set_regs failed")?;

        let code32 = Segment {
            base: 0,
            limit: 0xffff_ffff,
            selector: 0x10,
            type_: 0x0b,
            s: 1,
            present: 1,
            dpl: 0,
            db: 1,
            l: 0,
            g: 1,
            ..Default::default()
        };
        let data32 = Segment {
            base: 0,
            limit: 0xffff_ffff,
            selector: 0x18,
            type_: 0x03,
            s: 1,
            present: 1,
            dpl: 0,
            db: 1,
            l: 0,
            g: 1,
            ..Default::default()
        };
        let sregs = Sregs {
            cr0: 0x0000_0001,
            cr2: 0,
            cr3: 0,
            cr4: 0,
            cr8: 0,
            efer: 0,
            cs: code32,
            ds: data32.clone(),
            es: data32.clone(),
            fs: data32.clone(),
            gs: data32.clone(),
            ss: data32,
            gdt_base: 0x0000_0500,
            gdt_limit: 0x1f,
            idt_base: 0,
            idt_limit: 0,
            ..Default::default()
        };
        hyp.set_sregs(vcpu, &sregs).context("set_sregs failed")?;
        info!("vCPU configured: RIP={:#x} (32-bit protected mode)", kernel_entry_gpa);
        Ok(())
    }

    pub fn run_loop<H: Hypervisor>(
        hyp: &Arc<H>,
        mut vcpu: H::Vcpu,
        devices: Arc<std::sync::Mutex<crate::vmm::device::DeviceManager>>,
        ram: Arc<std::sync::Mutex<super::vm::AlignedRam>>,
        pause_gate: Arc<PauseGate>,
        vcpu_index: usize,
    ) -> Result<()> {
        let stdout = std::io::stdout();
        let mut uart_lcr: u8 = 0;
        let mut uart_ier: u8 = 0;

        // PIO/MMIO accesses are handled synchronously here, inside the
        // backend's own run loop — this is the only point where a read's
        // result can still reach the guest (see `VcpuAccess`'s doc comment).
        // It's also the natural checkpoint for pausing (see `pause.rs`):
        // this callback fires on essentially every guest instruction that
        // touches an I/O port or device, so bailing out here when a pause
        // is requested reliably unwinds `run_vcpu` back to this function's
        // own loop below, where register state can still be captured.
        let mut on_access = |access: VcpuAccess| -> Result<()> {
            if pause_gate.should_pause() {
                anyhow::bail!(PauseRequested);
            }
            match access {
                VcpuAccess::IoOut { port, data } if (0x3F8..=0x3FF).contains(&port) => {
                    match port {
                        0x3F9 if uart_lcr & 0x80 == 0 => {
                            if let Some(&v) = data.first() { uart_ier = v; }
                        }
                        0x3FB => {
                            if let Some(&v) = data.first() { uart_lcr = v; }
                        }
                        0x3F8 if uart_lcr & 0x80 == 0 => {
                            let mut out = stdout.lock();
                            let _ = out.write_all(data);
                            let _ = out.flush();
                        }
                        _ => {}
                    }
                }
                VcpuAccess::IoIn { port, data } if (0x3F8..=0x3FF).contains(&port) => {
                    let val: u8 = match port {
                        0x3F9 if uart_lcr & 0x80 == 0 => uart_ier,
                        0x3FA => {
                            if uart_ier & 0x02 != 0 { 0xC2 } else { 0xC1 }
                        }
                        0x3FB => uart_lcr,
                        0x3FD => 0x60,
                        0x3FE => 0xB0,
                        _ => 0x00,
                    };
                    for b in data.iter_mut() { *b = val; }
                }
                VcpuAccess::IoIn { port, data } => {
                    let val: u8 = match port {
                        0x61 => 0x20,
                        0x40..=0x42 => 0x00,
                        0x43 => 0x00,
                        _ => 0xFF,
                    };
                    for b in data.iter_mut() { *b = val; }
                }
                VcpuAccess::IoOut { .. } => {}
                VcpuAccess::MmioRead { addr, data } => {
                    let ram_guard = ram.lock().unwrap();
                    let handled = devices
                        .lock()
                        .unwrap()
                        .mmio_read(addr, data, &ram_guard)
                        .is_ok();
                    if !handled {
                        let start = addr as usize;
                        let end = (start + data.len()).min(ram_guard.len());
                        if end > start {
                            data[..end - start].copy_from_slice(&ram_guard[start..end]);
                        }
                    }
                }
                VcpuAccess::MmioWrite { addr, data } => {
                    let mut ram_guard = ram.lock().unwrap();
                    let handled = devices
                        .lock()
                        .unwrap()
                        .mmio_write(addr, data, &mut ram_guard)
                        .is_ok();
                    if !handled {
                        let start = addr as usize;
                        let end = (start + data.len()).min(ram_guard.len());
                        if end > start {
                            ram_guard[start..end].copy_from_slice(&data[..end - start]);
                        }
                    }
                }
                _ => {}
            }
            Ok(())
        };

        loop {
            match hyp.run_vcpu(&mut vcpu, &mut on_access) {
                Ok(VcpuExit::Hlt) => {
                    info!("vCPU HLT - guest halted cleanly");
                    break;
                }
                Ok(VcpuExit::Shutdown) => {
                    info!("vCPU SHUTDOWN");
                    break;
                }
                Ok(VcpuExit::Debug) => {
                    debug!("vCPU DEBUG breakpoint");
                }
                Ok(_) => {
                    debug!("vCPU unhandled exit - re-entering");
                }
                Err(e) if is_pause_requested(&e) => {
                    let regs = hyp.get_regs(&vcpu).context("get_regs during pause")?;
                    let sregs = hyp.get_sregs(&vcpu).context("get_sregs during pause")?;
                    pause_gate.park(vcpu_index, ParkedVcpu { regs, sregs });
                    // Resumed — the coordinator may have restored different
                    // register state (e.g. after a restore, though that
                    // path currently rebuilds vCPUs fresh rather than
                    // resuming one mid-flight); re-fetch nothing here and
                    // just re-enter, matching whatever the vCPU's registers
                    // now are.
                }
                Err(e) => return Err(e).context("run_vcpu failed"),
            }
        }
        Ok(())
    }
}
