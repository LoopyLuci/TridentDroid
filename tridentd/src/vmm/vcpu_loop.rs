//! VcpuRunner - platform-agnostic vCPU run loop and register configuration.
//!
//! This module contains only trait-generic code; it has no knowledge of
//! whether the backend is WHP or KVM.

use anyhow::{Context, Result};
use std::io::Write;
use std::sync::Arc;
use tracing::{debug, info};
use trident_hal::{Hypervisor, Regs, Segment, Sregs, VcpuExit};

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
        ram: Arc<std::sync::Mutex<Vec<u8>>>,
    ) -> Result<()> {
        let stdout = std::io::stdout();
        let mut uart_lcr: u8 = 0;
        let mut uart_ier: u8 = 0;

        loop {
            match hyp.run_vcpu(&mut vcpu).context("run_vcpu failed")? {
                VcpuExit::IoOut { port, data: data_vec }
                    if (0x3F8..=0x3FF).contains(&port) =>
                {
                    let data = data_vec.as_slice();
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
                VcpuExit::IoIn { port, mut data, .. }
                    if (0x3F8..=0x3FF).contains(&port) =>
                {
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
                VcpuExit::IoIn { port, mut data, .. } => {
                    let val: u8 = match port {
                        0x61 => 0x20,
                        0x40..=0x42 => 0x00,
                        0x43 => 0x00,
                        _ => 0xFF,
                    };
                    for b in data.iter_mut() { *b = val; }
                }
                VcpuExit::IoOut { .. } => {}
                VcpuExit::MmioRead { addr, len, .. } => {
                    let mut data_vec = vec![0u8; len as usize];
                    {
                        let devs = devices.lock().unwrap();
                        if devs.mmio_read(addr, &mut data_vec).is_ok() { continue; }
                    }
                    {
                        let ram_guard = ram.lock().unwrap();
                        let start = addr as usize;
                        let end = (start + data_vec.len()).min(ram_guard.len());
                        data_vec[..end - start].copy_from_slice(&ram_guard[start..end]);
                    }
                }
                VcpuExit::MmioWrite { addr, data: data_vec, .. } => {
                    {
                        let devs = devices.lock().unwrap();
                        if devs.mmio_write(addr, &data_vec).is_ok() { continue; }
                    }
                    {
                        let mut ram_guard = ram.lock().unwrap();
                        let start = addr as usize;
                        let end = (start + data_vec.len()).min(ram_guard.len());
                        ram_guard[start..end].copy_from_slice(&data_vec[..end - start]);
                    }
                }
                VcpuExit::Hlt => {
                    info!("vCPU HLT - guest halted cleanly");
                    break;
                }
                VcpuExit::Shutdown => {
                    info!("vCPU SHUTDOWN");
                    break;
                }
                VcpuExit::Debug => {
                    debug!("vCPU DEBUG breakpoint");
                }
                _ => {
                    debug!("vCPU unhandled exit - re-entering");
                }
            }
        }
        Ok(())
    }
}
