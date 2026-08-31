//! Device manager and PCI bus for TridentDroid VMM.
//!
//! This module provides the central device emulation layer.
//! `DeviceManager` holds all devices and routes MMIO/PCI access.
//! `PciBus` provides a minimal PCI ECAM implementation.
//! `Device` is the common trait all virtual devices implement.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::{debug, info};
use trident_hal::VcpuExit;

pub mod virtio;

pub trait Device: Send + Sync {
    fn name(&self) -> &str;
    fn read(&self, offset: u64, data: &mut [u8], ram: &[u8]) -> Result<(), String>;
    fn write(
        &mut self,
        offset: u64,
        data: &[u8],
        ram: &mut [u8],
        vcpu_index: usize,
        ctx: VcpuExitData,
    ) -> Result<(), String>;
    fn kick_guest(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy)]
pub struct VcpuExitData {
    pub vcpu_index: u8,
    pub port: u16,
    pub data_ptr: u64,
    pub len: u32,
}

/// Device manager — owns all virtio devices and routes MMIO/PIO.
pub struct DeviceManager {
    devices: Vec<Arc<dyn Device + Send + Sync>>,
    /// MMIO region base address for each device (indexed by device index).
    mmio_bases: HashMap<usize, u64>,
    /// Next MMIO base address to assign.
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

    pub fn init() -> Result<Self, String> {
        info!("Device manager initialized");
        Ok(Self::new())
    }

    /// Register a virtio device and assign it an MMIO region.
    pub fn register_virtio(&mut self, device: Arc<dyn Device + Send + Sync>) -> usize {
        let idx = self.devices.len();
        let base = self.next_mmio;
        self.next_mmio += Self::MMIO_STEP;
        self.mmio_bases.insert(idx, base);
        self.devices.push(device);
        info!(
            "Virtio device #{} registered at MMIO base {:#x}",
            idx, base
        );
        idx
    }

    /// Read from an MMIO region. Returns Ok(()) if a device handled it.
    pub fn mmio_read(&self, addr: u64, data: &mut [u8]) -> Result<(), String> {
        for (idx, base) in &self.mmio_bases {
            if addr >= *base && addr < *base + Self::MMIO_STEP {
                let offset = addr - base;
                let ram = vec![0u8; 0]; // Dummy RAM for now
                return self.devices[*idx].read(offset, data, &ram);
            }
        }
        // No device mapped — return zeros
        for b in data.iter_mut() {
            *b = 0;
        }
        Ok(())
    }

    /// Write to an MMIO region. Returns Ok(()) if a device handled it.
    pub fn mmio_write(&mut self, addr: u64, data: &[u8]) -> Result<(), String> {
        for (idx, base) in &self.mmio_bases {
            if addr >= *base && addr < *base + Self::MMIO_STEP {
                let offset = addr - base;
                let mut ram = vec![0u8; 0]; // Dummy RAM for now
                let ctx = VcpuExitData {
                    vcpu_index: 0,
                    port: 0,
                    data_ptr: 0,
                    len: data.len() as u32,
                };
                return self.devices[*idx].write(offset, data, &mut ram, 0, ctx);
            }
        }
        // No device mapped — ignore
        Ok(())
    }

    /// Notify a device's virtqueue (called when guest writes to queue notify register).
    pub fn notify_queue(&mut self, idx: usize, queue_idx: u16) -> Result<(), String> {
        // For now, just log
        debug!("Notify queue {} for device #{}", queue_idx, idx);
        Ok(())
    }

    /// Reset all devices.
    pub fn reset_all(&mut self) -> Result<(), String> {
        info!("Resetting all devices");
        Ok(())
    }

    /// Poll all devices for pending work.
    pub fn poll_all(&mut self) -> Result<(), String> {
        Ok(())
    }
}
