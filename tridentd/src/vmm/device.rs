//! Device manager and PCI bus for TridentDroid VMM.
//!
//! This module provides the central device emulation layer.
//! `DeviceManager` holds all devices and routes MMIO/PCI access.
//! `Device` is the common trait all virtual devices implement.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};
use crate::vmm::virtio::{mmio, VirtioDevice};

pub trait Device: Send + Sync {
    fn name(&self) -> &str;
    fn read(&self, offset: u64, data: &mut [u8], ram: &[u8]) -> Result<(), String>;
    fn write(&mut self, offset: u64, data: &[u8], ram: &mut [u8], _vcpu_index: usize, _ctx: VcpuExitData) -> Result<(), String>;
    fn kick_guest(&self) -> bool { false }
    fn snapshot_state(&self) -> Result<Vec<u8>, String>;
    fn restore_state(&mut self, data: &[u8]) -> Result<(), String>;
}

fn read_u32(data: &[u8]) -> u32 {
    let mut bytes = [0u8; 4];
    let len = data.len().min(4);
    bytes[..len].copy_from_slice(&data[..len]);
    u32::from_le_bytes(bytes)
}

fn write_u32(data: &mut [u8], value: u32) {
    let bytes = value.to_le_bytes();
    let len = data.len().min(4);
    data[..len].copy_from_slice(&bytes[..len]);
}

/// Set the low or high 32 bits of a 64-bit guest address register, leaving
/// the other half untouched (the virtio-mmio spec always writes these as
/// two separate 32-bit registers).
fn set_lo(field: &mut u64, val: u32) {
    *field = (*field & 0xFFFF_FFFF_0000_0000) | val as u64;
}
fn set_hi(field: &mut u64, val: u32) {
    *field = (*field & 0x0000_0000_FFFF_FFFF) | ((val as u64) << 32);
}

impl<T: VirtioDevice> Device for T {
    fn name(&self) -> &str {
        VirtioDevice::name(self)
    }

    fn read(&self, offset: u64, data: &mut [u8], _ram: &[u8]) -> Result<(), String> {
        match offset {
            mmio::MAGIC => write_u32(data, 0x74726976), // "virt"
            mmio::VERSION => write_u32(data, 2),
            mmio::DEVICE_ID => write_u32(data, self.device_id() as u32),
            mmio::VENDOR_ID => write_u32(data, 0x1AF4),
            mmio::DEVICE_FEATURES => {
                let feats = self.features();
                let word = if self.common().device_features_sel == 0 {
                    feats as u32
                } else {
                    (feats >> 32) as u32
                };
                write_u32(data, word);
            }
            mmio::QUEUE_NUM_MAX => write_u32(data, self.max_queue_size() as u32),
            mmio::QUEUE_READY => {
                let sel = self.common().queue_sel as usize;
                let ready = self.common().queues.get(sel).is_some_and(|q| q.ready);
                write_u32(data, ready as u32);
            }
            mmio::INTERRUPT_STATUS => write_u32(data, self.common().interrupt_status),
            mmio::STATUS => write_u32(data, self.common().status.0),
            mmio::CONFIG_GENERATION => write_u32(data, self.common().config_generation),
            off if off >= mmio::CONFIG_OFFSET => {
                return self.config_read(off - mmio::CONFIG_OFFSET, data).map_err(|e| e.to_string());
            }
            _ => {
                for b in data.iter_mut() { *b = 0; }
            }
        }
        Ok(())
    }

    fn write(&mut self, offset: u64, data: &[u8], ram: &mut [u8], _vcpu_index: usize, _ctx: VcpuExitData) -> Result<(), String> {
        let val = read_u32(data);
        match offset {
            mmio::DEVICE_FEATURES_SEL => self.common_mut().device_features_sel = val,
            mmio::DRIVER_FEATURES_SEL => self.common_mut().driver_features_sel = val,
            mmio::DRIVER_FEATURES => {
                let sel = self.common().driver_features_sel;
                let common = self.common_mut();
                if sel == 0 {
                    set_lo(&mut common.driver_features, val);
                } else {
                    set_hi(&mut common.driver_features, val);
                }
            }
            mmio::QUEUE_SEL => self.common_mut().queue_sel = val as u16,
            mmio::QUEUE_NUM => {
                let sel = self.common().queue_sel as usize;
                if let Some(q) = self.common_mut().queues.get_mut(sel) {
                    q.num = val as u16;
                }
            }
            mmio::QUEUE_READY => {
                let sel = self.common().queue_sel as usize;
                if let Some(q) = self.common_mut().queues.get_mut(sel) {
                    q.ready = val != 0;
                }
            }
            mmio::QUEUE_DESC_LOW => {
                let sel = self.common().queue_sel as usize;
                if let Some(q) = self.common_mut().queues.get_mut(sel) { set_lo(&mut q.desc_addr, val); }
            }
            mmio::QUEUE_DESC_HIGH => {
                let sel = self.common().queue_sel as usize;
                if let Some(q) = self.common_mut().queues.get_mut(sel) { set_hi(&mut q.desc_addr, val); }
            }
            mmio::QUEUE_DRIVER_LOW => {
                let sel = self.common().queue_sel as usize;
                if let Some(q) = self.common_mut().queues.get_mut(sel) { set_lo(&mut q.avail_addr, val); }
            }
            mmio::QUEUE_DRIVER_HIGH => {
                let sel = self.common().queue_sel as usize;
                if let Some(q) = self.common_mut().queues.get_mut(sel) { set_hi(&mut q.avail_addr, val); }
            }
            mmio::QUEUE_DEVICE_LOW => {
                let sel = self.common().queue_sel as usize;
                if let Some(q) = self.common_mut().queues.get_mut(sel) { set_lo(&mut q.used_addr, val); }
            }
            mmio::QUEUE_DEVICE_HIGH => {
                let sel = self.common().queue_sel as usize;
                if let Some(q) = self.common_mut().queues.get_mut(sel) { set_hi(&mut q.used_addr, val); }
            }
            mmio::INTERRUPT_ACK => {
                let common = self.common_mut();
                common.interrupt_status &= !val;
            }
            mmio::STATUS => {
                if val == 0 {
                    self.reset().map_err(|e| e.to_string())?;
                } else {
                    self.common_mut().status.0 = val;
                }
            }
            mmio::QUEUE_NOTIFY => {
                self.notify_queue(val as u16, ram).map_err(|e| e.to_string())?;
            }
            off if off >= mmio::CONFIG_OFFSET => {
                self.config_write(off - mmio::CONFIG_OFFSET, data).map_err(|e| e.to_string())?;
            }
            _ => {}
        }
        Ok(())
    }

    fn kick_guest(&self) -> bool { false }

    fn snapshot_state(&self) -> Result<Vec<u8>, String> {
        VirtioDevice::snapshot_state(self).map_err(|e| e.to_string())
    }

    fn restore_state(&mut self, data: &[u8]) -> Result<(), String> {
        VirtioDevice::restore_state(self, data).map_err(|e| e.to_string())
    }
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

    pub fn mmio_read(&self, addr: u64, data: &mut [u8], ram: &[u8]) -> Result<(), String> {
        for (idx, base) in &self.mmio_bases {
            if addr >= *base && addr < *base + Self::MMIO_STEP {
                let offset = addr - base;
                return self.devices[*idx].lock().unwrap().read(offset, data, ram);
            }
        }
        Err("no device at address".to_string())
    }

    pub fn mmio_write(&self, addr: u64, data: &[u8], ram: &mut [u8]) -> Result<(), String> {
        for (idx, base) in &self.mmio_bases {
            if addr >= *base && addr < *base + Self::MMIO_STEP {
                let offset = addr - base;
                let ctx = VcpuExitData { vcpu_index: 0, port: 0, data_ptr: 0, len: data.len() as u32 };
                return self.devices[*idx].lock().unwrap().write(offset, data, ram, 0, ctx);
            }
        }
        Err("no device at address".to_string())
    }

    /// Number of registered devices, in registration order — snapshot and
    /// restore rely on this order being reproduced identically by whatever
    /// builds the `DeviceManager` (see `Vm::create`/`Vm::restore`).
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    pub fn device_at(&self, idx: usize) -> Option<Arc<Mutex<dyn Device + Send + Sync>>> {
        self.devices.get(idx).cloned()
    }

    pub fn reset_all(&mut self) -> Result<(), String> {
        info!("Resetting all devices");
        Ok(())
    }

    pub fn poll_all(&mut self) -> Result<(), String> { Ok(()) }
}
