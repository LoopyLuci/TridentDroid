//! Device manager and PCI bus for TridentDroid VMM.
//!
//! This module provides the central device emulation layer.
//! `DeviceManager` holds all devices and routes MMIO/PCI access.
//! `Device` is the common trait all virtual devices implement.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};
use crate::vmm::virtio::VirtioDevice;

pub trait Device: Send + Sync {
    fn name(&self) -> &str;
    fn read(&self, offset: u64, data: &mut [u8], ram: &[u8]) -> Result<(), String>;
    fn write(&mut self, offset: u64, data: &[u8], ram: &mut [u8], _vcpu_index: usize, _ctx: VcpuExitData) -> Result<(), String>;
    fn kick_guest(&self) -> bool { false }
}

impl<T: VirtioDevice> Device for T {
    fn name(&self) -> &str { self.name() }
    fn read(&self, offset: u64, data: &mut [u8], _ram: &[u8]) -> Result<(), String> {
        self.config_read(offset, data).map_err(|e| e.to_string())
    }
    fn write(&mut self, offset: u64, data: &[u8], _ram: &mut [u8], _vcpu_index: usize, _ctx: VcpuExitData) -> Result<(), String> {
        self.config_write(offset, data).map_err(|e| e.to_string())
    }
    fn kick_guest(&self) -> bool { false }
}

#[derive(Clone, Copy)]
pub struct VcpuExitData {
    pub vcpu_index: u8,
    pub port: u16,
    pub data_ptr: u64,
    pub len: u32,
}

pub struct DeviceManager {
    devices: Vec<Arc<Mutex<dyn Device + Send + Sync>>>,
    mmio_bases: HashMap<usize, u64>,
    next_mmio: u64,
}

impl DeviceManager {
    const MMIO_BASE: u64 = 0x1000_0000;
    const MMIO_STEP: u64 = 0x1000;

    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            mmio_bases: HashMap::new(),
            next_mmio: Self::MMIO_BASE,
        }
    }

    pub fn init() -> Result<Self, anyhow::Error> {
        debug!("Device manager initialized");
        Ok(Self::new())
    }

    pub fn register_virtio(&mut self, device: Arc<Mutex<dyn Device + Send + Sync>>) -> usize {
        let idx = self.devices.len();
        let base = self.next_mmio;
        self.next_mmio += Self::MMIO_STEP;
        self.mmio_bases.insert(idx, base);
        self.devices.push(device);
        info!("Virtio device #{idx} registered at MMIO base {base:#x}");
        idx
    }

    pub fn mmio_read(&self, addr: u64, data: &mut [u8]) -> Result<(), String> {
        for (idx, base) in &self.mmio_bases {
            if addr >= *base && addr < *base + Self::MMIO_STEP {
                let _offset = addr - base;
                let ram = vec![];
                return self.devices[*idx].lock().unwrap().read(_offset, data, &ram);
            }
        }
        for b in data.iter_mut() { *b = 0; }
        Ok(())
    }

    pub fn mmio_write(&self, addr: u64, data: &[u8]) -> Result<(), String> {
        for (idx, base) in &self.mmio_bases {
            if addr >= *base && addr < *base + Self::MMIO_STEP {
                let _offset = addr - base;
                let mut ram = vec![];
                let ctx = VcpuExitData { vcpu_index: 0, port: 0, data_ptr: 0, len: data.len() as u32 };
                return self.devices[*idx].lock().unwrap().write(_offset, data, &mut ram, 0, ctx);
            }
        }
        Ok(())
    }

    pub fn notify_queue(&self, idx: usize, queue_idx: u16) -> Result<(), String> {
        debug!("Notify queue {queue_idx} for device #{idx}");
        Ok(())
    }

    pub fn reset_all(&mut self) -> Result<(), String> {
        info!("Resetting all devices");
        Ok(())
    }

    pub fn poll_all(&mut self) -> Result<(), String> { Ok(()) }
}
