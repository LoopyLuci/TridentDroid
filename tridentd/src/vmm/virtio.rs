//! Virtio transport layer and device implementations.
//!
//! This module provides:
//! - Virtio PCI/MMIO transport layer
//! - Virtio block device (for system.img/vendor.img)
//! - Virtio network device
//! - Virtio console device
//! - Virtio input device (keyboard, touchscreen)
//!
//! All devices follow the Virtio 1.2 specification.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tracing::{debug, info, warn};

// ── Constants ───────────────────────────────────────────────────────────────

/// Virtio PCI config space offsets.
pub mod pci {
    pub const VENDOR_ID: u16 = 0x1AF4;
    /// Device ID base (0x1040 + device_type).
    pub const DEVICE_ID_BASE: u16 = 0x1040;
    pub const SUBSYS_DEVICE_ID: u16 = 0x00;
    pub const COMMON_CFG_OFFSET: u64 = 0x0;
    pub const COMMON_CFG_LENGTH: u64 = 0x100;
    pub const ISR_OFFSET: u64 = 0x100;
    pub const ISR_LENGTH: u64 = 0x10;
    pub const CONFIG_SPACE_SIZE: usize = 0x1000;
}

/// Virtio MMIO register offsets (from the device's MMIO base).
pub mod mmio {
    pub const MAGIC: u64 = 0x000;
    pub const VERSION: u64 = 0x004;
    pub const DEVICE_ID: u64 = 0x008;
    pub const VENDOR_ID: u64 = 0x00C;
    pub const DEVICE_FEATURES: u64 = 0x010;
    pub const DEVICE_FEATURES_SEL: u64 = 0x014;
    pub const DRIVER_FEATURES: u64 = 0x020;
    pub const DRIVER_FEATURES_SEL: u64 = 0x024;
    pub const QUEUE_SEL: u64 = 0x030;
    pub const QUEUE_NUM_MAX: u64 = 0x034;
    pub const QUEUE_NUM: u64 = 0x038;
    pub const QUEUE_READY: u64 = 0x044;
    pub const QUEUE_NOTIFY: u64 = 0x050;
    pub const INTERRUPT_STATUS: u64 = 0x060;
    pub const INTERRUPT_ACK: u64 = 0x064;
    pub const STATUS: u64 = 0x070;
    pub const QUEUE_DESC_LOW: u64 = 0x080;
    pub const QUEUE_DESC_HIGH: u64 = 0x084;
    pub const QUEUE_DRIVER_LOW: u64 = 0x090;
    pub const QUEUE_DRIVER_HIGH: u64 = 0x094;
    pub const QUEUE_DEVICE_LOW: u64 = 0x0A0;
    pub const QUEUE_DEVICE_HIGH: u64 = 0x0A4;
    pub const CONFIG_GENERATION: u64 = 0x0FC;
    pub const CONFIG_OFFSET: u64 = 0x100;
}

/// Virtqueue descriptor flags (`struct virtq_desc.flags`).
pub const VIRTQ_DESC_F_NEXT: u16 = 1;
pub const VIRTQ_DESC_F_WRITE: u16 = 2;

// ── Status Register ─────────────────────────────────────────────────────────

/// Virtio status register bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VirtioStatus(pub u32);

impl VirtioStatus {
    pub const ACKNOWLEDGE: u32 = 1;
    pub const DRIVER: u32 = 2;
    pub const DRIVER_OK: u32 = 4;
    pub const FEATURES_OK: u32 = 8;
    pub const DEVICE_NEEDS_RESET: u32 = 64;
    pub const FAILED: u32 = 128;

    #[inline]
    pub fn has(&self, bit: u32) -> bool {
        self.0 & bit != 0
    }

    #[inline]
    pub fn set(&mut self, bit: u32) {
        self.0 |= bit;
    }

    #[inline]
    pub fn clear(&mut self, bit: u32) {
        self.0 &= !bit;
    }
}

// ── Virtqueue Structures ────────────────────────────────────────────────────

/// Virtqueue descriptor.
#[derive(Debug, Clone, Default)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

/// Virtqueue available ring.
#[derive(Debug, Clone, Default)]
pub struct VirtqAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: Vec<u16>,
    pub used_event: u16,
}

/// Virtqueue used ring.
#[derive(Debug, Clone, Default)]
pub struct VirtqUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: Vec<VirtqUsedElem>,
    pub avail_event: u16,
}

/// Element in the used ring.
#[derive(Debug, Clone, Default)]
pub struct VirtqUsedElem {
    pub id: u32,
    pub len: u32,
}

/// A virtqueue instance.
#[derive(Debug, Serialize, Deserialize)]
pub struct VirtQueue {
    pub idx: u16,
    pub num_max: u16,
    pub num: u16,
    pub desc_addr: u64,
    pub avail_addr: u64,
    pub used_addr: u64,
    pub ready: bool,
    pub last_avail_idx: u16,
}

impl VirtQueue {
    pub fn new(idx: u16, num_max: u16) -> Self {
        Self {
            idx,
            num_max,
            num: num_max,
            desc_addr: 0,
            avail_addr: 0,
            used_addr: 0,
            ready: false,
            last_avail_idx: 0,
        }
    }

    pub fn set_addrs(&mut self, desc: u64, avail: u64, used: u64) {
        self.desc_addr = desc;
        self.avail_addr = avail;
        self.used_addr = used;
    }

    /// Read a descriptor from guest memory.
    pub fn read_desc(&self, ram: &[u8], idx: u16) -> Option<VirtqDesc> {
        let idx = idx as usize;
        if idx >= self.num as usize {
            return None;
        }
        let offset = self.desc_addr as usize + idx * std::mem::size_of::<VirtqDesc>();
        let end = offset + std::mem::size_of::<VirtqDesc>();
        if end > ram.len() {
            return None;
        }
        let bytes = &ram[offset..end];
        // Parse the descriptor: addr(8) + len(4) + flags(2) + next(2) = 16 bytes.
        let addr = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
        let len = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
        let flags = u16::from_le_bytes(bytes[12..14].try_into().ok()?);
        let next = u16::from_le_bytes(bytes[14..16].try_into().ok()?);
        Some(VirtqDesc {
            addr,
            len,
            flags,
            next,
        })
    }

    /// Read the available ring index from guest memory. Layout is
    /// `flags(u16) @ +0, idx(u16) @ +2, ring[...] @ +4` — the index is at
    /// offset 2, not 0.
    pub fn read_avail_idx(&self, ram: &[u8]) -> u16 {
        if self.avail_addr == 0 || self.avail_addr as usize + 4 > ram.len() {
            return 0;
        }
        let lo = self.avail_addr as usize + 2;
        let hi = self.avail_addr as usize + 4;
        u16::from_le_bytes(ram[lo..hi].try_into().unwrap())
    }

    /// Read an available ring entry from guest memory.
    pub fn read_avail_entry(&self, ram: &[u8], idx: u16) -> u16 {
        let offset = self.avail_addr as usize + 4 + idx as usize * 2;
        if offset + 2 > ram.len() {
            return 0;
        }
        u16::from_le_bytes(ram[offset..offset + 2].try_into().unwrap())
    }

    /// Write a used ring entry to guest memory.
    pub fn write_used_entry(&self, ram: &mut [u8], idx: u16, elem: &VirtqUsedElem) {
        let offset = self.used_addr as usize + 4 + idx as usize * 8;
        if offset + 8 <= ram.len() {
            ram[offset..offset + 4].copy_from_slice(&elem.id.to_le_bytes());
            ram[offset + 4..offset + 8].copy_from_slice(&elem.len.to_le_bytes());
        }
    }

    /// Write the used ring index to guest memory. Same layout as the avail
    /// ring — `flags(u16) @ +0, idx(u16) @ +2` — the index goes at offset 2.
    pub fn write_used_idx(&self, ram: &mut [u8], idx: u16) {
        if self.used_addr as usize + 4 <= ram.len() {
            let lo = self.used_addr as usize + 2;
            let hi = self.used_addr as usize + 4;
            ram[lo..hi].copy_from_slice(&idx.to_le_bytes());
        }
    }

    /// Read data from a descriptor buffer in guest memory.
    pub fn read_buffer(&self, ram: &[u8], desc: &VirtqDesc) -> Vec<u8> {
        let start = desc.addr as usize;
        let end = start + desc.len as usize;
        if end > ram.len() {
            return vec![0u8; desc.len as usize];
        }
        ram[start..end].to_vec()
    }

    /// Write data to a descriptor buffer in guest memory.
    pub fn write_buffer(&self, ram: &mut [u8], desc: &VirtqDesc, data: &[u8]) {
        let start = desc.addr as usize;
        let end = start + std::cmp::min(desc.len as usize, data.len());
        if end <= ram.len() {
            ram[start..end].copy_from_slice(&data[..end - start]);
        }
    }

    /// Walk a descriptor chain starting at `head_idx`, following `.next`
    /// while `VIRTQ_DESC_F_NEXT` is set. Bounded by `num_max` hops so a
    /// corrupt/hostile chain (e.g. a cycle) can't loop forever.
    pub fn read_chain(&self, ram: &[u8], head_idx: u16) -> Vec<VirtqDesc> {
        let mut chain = Vec::new();
        let mut idx = head_idx;
        for _ in 0..self.num_max {
            let Some(desc) = self.read_desc(ram, idx) else { break };
            let has_next = desc.flags & VIRTQ_DESC_F_NEXT != 0;
            let next = desc.next;
            chain.push(desc);
            if !has_next {
                break;
            }
            idx = next;
        }
        chain
    }
}

/// Shared virtio-mmio "common config" state every device embeds — the
/// register-level negotiation/queue-setup state, as opposed to each
/// device's own config space or virtqueue payload handling.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CommonConfig {
    pub device_features_sel: u32,
    pub driver_features_sel: u32,
    pub driver_features: u64,
    pub queue_sel: u16,
    pub queues: Vec<VirtQueue>,
    pub status: VirtioStatus,
    pub interrupt_status: u32,
    pub config_generation: u32,
}

// ── VirtioDevice Trait ──────────────────────────────────────────────────────

/// Trait for virtio devices.
pub trait VirtioDevice: Send + Sync {
    /// Device ID (1 = net, 2 = blk, 3 = console, 18 = input).
    fn device_id(&self) -> u16;

    /// Device name.
    fn name(&self) -> &str;

    /// Maximum queue size.
    fn max_queue_size(&self) -> u16;

    /// Device features (u64, bitfield).
    fn features(&self) -> u64;

    /// Read config register.
    fn config_read(&self, offset: u64, data: &mut [u8]) -> Result<()>;

    /// Write config register.
    fn config_write(&mut self, offset: u64, data: &[u8]) -> Result<()>;

    /// Notify a queue that new buffers are available.
    fn notify_queue(&mut self, queue_idx: u16, ram: &mut [u8]) -> Result<()>;

    /// Reset the device.
    fn reset(&mut self) -> Result<()>;

    /// Shared register-level transport state (feature negotiation, queue
    /// setup, status). The generic `Device` impl in `device.rs` reads/writes
    /// this directly to implement the virtio-mmio common-config register
    /// block, so every device must expose it.
    fn common(&self) -> &CommonConfig;
    fn common_mut(&mut self) -> &mut CommonConfig;

    /// Serialize this device's transport/negotiation state (and any small
    /// ephemeral device-specific state, e.g. pending buffers) for a
    /// snapshot. Deliberately excludes anything re-derivable from
    /// `VmConfig` on restore (backing disk image paths/contents) — those
    /// are re-supplied fresh, not duplicated into the snapshot file.
    fn snapshot_state(&self) -> Result<Vec<u8>>;

    /// Inverse of `snapshot_state` — called on a freshly-constructed device
    /// (already given its `VmConfig`-derived setup, e.g. `set_backing`) to
    /// restore its transport/negotiation state.
    fn restore_state(&mut self, data: &[u8]) -> Result<()>;
}

// ── VirtioBlk ───────────────────────────────────────────────────────────────

/// Virtio block device (for system.img, vendor.img).
pub struct VirtioBlk {
    device_id: u16,
    name: String,
    features: u64,
    common: CommonConfig,
    backing_path: Option<String>,
    backing_data: Option<Vec<u8>>,
    sector_size: u64,
    capacity: u64,
}

impl VirtioBlk {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            device_id: 2,
            name: name.into(),
            features: Self::device_features(),
            common: CommonConfig {
                queues: vec![VirtQueue::new(0, 256)],
                ..Default::default()
            },
            backing_path: None,
            backing_data: None,
            sector_size: 512,
            capacity: 0,
        }
    }

    fn device_features() -> u64 {
        1 << 0 | 1 << 1 | 1 << 4 | 1 << 6 | 1 << 9 | 1 << 10 | 1 << 11
    }

    /// Set the backing disk image.
    pub fn set_backing(&mut self, path: impl Into<String>) -> Result<()> {
        let path = path.into();
        let metadata = std::fs::metadata(&path)
            .with_context(|| format!("Failed to stat disk image: {}", path))?;
        self.capacity = metadata.len();
        self.sector_size = 512;
        self.backing_path = Some(path);
        let cap = self.capacity;
        info!(
            "VirtioBlk: backing set to {} ({} bytes)",
            self.backing_path.as_ref().unwrap(),
            cap
        );
        Ok(())
    }

    /// Set the backing data directly (for in-memory images).
    pub fn set_backing_data(&mut self, data: Vec<u8>) {
        self.capacity = data.len() as u64;
        self.sector_size = 512;
        self.backing_data = Some(data);
    }

    /// Handle a read request.
    fn handle_read(&self, _ram: &mut [u8], offset: u64, len: u64) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; len as usize];
        if let Some(ref data) = self.backing_data {
            let start = offset as usize;
            let end = std::cmp::min(start + len as usize, data.len());
            if start < data.len() {
                buf[..end - start].copy_from_slice(&data[start..end]);
            }
        } else if let Some(ref path) = self.backing_path {
            use std::io::{Read, Seek, SeekFrom};
            let mut file = std::fs::File::open(path)?;
            file.seek(SeekFrom::Start(offset))?;
            let _ = file.read(&mut buf)?;
        }
        Ok(buf)
    }

    /// Handle a write request.
    fn handle_write(&mut self, _ram: &mut [u8], offset: u64, data: &[u8]) -> Result<()> {
        if let Some(ref mut backing) = self.backing_data {
            let start = offset as usize;
            let end = std::cmp::min(start + data.len(), backing.len());
            if start < backing.len() {
                backing[start..end].copy_from_slice(&data[..end - start]);
            }
        } else if let Some(ref path) = self.backing_path {
            use std::io::{Seek, SeekFrom, Write};
            let mut file = std::fs::OpenOptions::new().write(true).open(path)?;
            file.seek(SeekFrom::Start(offset))?;
            file.write_all(data)?;
        }
        Ok(())
    }
}

impl VirtioDevice for VirtioBlk {
    fn device_id(&self) -> u16 {
        self.device_id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn max_queue_size(&self) -> u16 {
        256
    }

    fn features(&self) -> u64 {
        self.features
    }

    fn config_read(&self, offset: u64, data: &mut [u8]) -> Result<()> {
        let mut config = [0u8; 64];
        config[0..8].copy_from_slice(&self.capacity.to_le_bytes());
        config[8..12].copy_from_slice(&(self.capacity / self.sector_size).to_le_bytes());
        config[12..16].copy_from_slice(&256u32.to_le_bytes());
        config[16..20].copy_from_slice(&16u32.to_le_bytes());
        config[20..24].copy_from_slice(&1u32.to_le_bytes());
        config[24..28].copy_from_slice(&self.sector_size.to_le_bytes());
        let len = std::cmp::min(data.len(), (config.len() as u64 - offset) as usize);
        data[..len].copy_from_slice(&config[offset as usize..offset as usize + len]);
        Ok(())
    }

    fn config_write(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        debug!(
            "VirtioBlk config write: offset={:#x}, len={}",
            offset,
            data.len()
        );
        Ok(())
    }

    fn notify_queue(&mut self, queue_idx: u16, ram: &mut [u8]) -> Result<()> {
        if queue_idx != 0 {
            return Ok(());
        }
        let avail_idx = self.common.queues[0].read_avail_idx(ram);
        let mut last_idx = self.common.queues[0].last_avail_idx;
        while last_idx != avail_idx {
            let desc_idx = self.common.queues[0].read_avail_entry(ram, last_idx);
            // A standard virtio-blk request is a 3-descriptor chain: header
            // (readable, 16 bytes: type/reserved/sector) -> data buffer
            // (writable for a read request, readable for a write request)
            // -> status byte (writable). Walking the whole chain — rather
            // than treating the head descriptor's own buffer as the entire
            // request — is what actually makes reads/writes transfer real
            // data instead of just acking an empty request.
            let chain = self.common.queues[0].read_chain(ram, desc_idx);
            let total_len: u32 = chain.iter().map(|d| d.len).sum();
            let Some(hdr_desc) = chain.first() else { break };
            let hdr = self.common.queues[0].read_buffer(ram, hdr_desc);

            let status: u8 = if hdr.len() >= 16 {
                let req_type = u32::from_le_bytes(hdr[0..4].try_into().unwrap());
                let sector = u64::from_le_bytes(hdr[8..16].try_into().unwrap());
                let offset = sector * self.sector_size;
                match (req_type, chain.get(1), chain.last()) {
                    (0, Some(data_desc), Some(_status_desc)) => {
                        let len = self.capacity.saturating_sub(offset).min(data_desc.len as u64);
                        let buf = self.handle_read(ram, offset, len)?;
                        self.common.queues[0].write_buffer(ram, data_desc, &buf);
                        debug!("VirtioBlk read: sector={}, len={}", sector, len);
                        0
                    }
                    (1, Some(data_desc), Some(_status_desc)) => {
                        let buf = self.common.queues[0].read_buffer(ram, data_desc);
                        self.handle_write(ram, offset, &buf)?;
                        debug!("VirtioBlk write: sector={}, len={}", sector, buf.len());
                        0
                    }
                    (5, ..) => {
                        debug!("VirtioBlk flush");
                        0
                    }
                    _ => {
                        warn!("VirtioBlk unknown/malformed request type: {}", req_type);
                        2 // VIRTIO_BLK_S_UNSUPP
                    }
                }
            } else {
                1 // VIRTIO_BLK_S_IOERR — header too short to parse
            };

            // A real guest driver waits on this status byte, not just the
            // used-ring entry — write it into the chain's last descriptor.
            if let Some(status_desc) = chain.last() {
                self.common.queues[0].write_buffer(ram, status_desc, &[status]);
            }

            self.common.queues[0].write_used_entry(
                ram,
                last_idx,
                &VirtqUsedElem { id: desc_idx as u32, len: total_len },
            );
            last_idx = last_idx.wrapping_add(1);
        }
        self.common.queues[0].last_avail_idx = last_idx;
        self.common.queues[0].write_used_idx(ram, last_idx);
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.common.status = VirtioStatus(0);
        self.common.driver_features = 0;
        self.common.config_generation = 0;
        for q in &mut self.common.queues {
            q.ready = false;
            q.last_avail_idx = 0;
        }
        Ok(())
    }

    fn common(&self) -> &CommonConfig {
        &self.common
    }

    fn common_mut(&mut self) -> &mut CommonConfig {
        &mut self.common
    }

    fn snapshot_state(&self) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct S<'a> {
            common: &'a CommonConfig,
            sector_size: u64,
            capacity: u64,
        }
        Ok(bincode::serialize(&S { common: &self.common, sector_size: self.sector_size, capacity: self.capacity })?)
    }

    fn restore_state(&mut self, data: &[u8]) -> Result<()> {
        #[derive(Deserialize)]
        struct S {
            common: CommonConfig,
            sector_size: u64,
            capacity: u64,
        }
        let s: S = bincode::deserialize(data)?;
        self.common = s.common;
        self.sector_size = s.sector_size;
        self.capacity = s.capacity;
        Ok(())
    }
}

// ── VirtioConsole ───────────────────────────────────────────────────────────

/// Virtio console device (for log output from the guest).
pub struct VirtioConsole {
    device_id: u16,
    name: String,
    features: u64,
    common: CommonConfig,
    rx_buffers: VecDeque<Vec<u8>>,
    tx_buffers: VecDeque<Vec<u8>>,
    /// Console socket path (optional).
    console_sock: Option<String>,
}

impl VirtioConsole {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            device_id: 3,
            name: name.into(),
            features: (1 << 0) | (1 << 1),
            common: CommonConfig {
                queues: vec![VirtQueue::new(0, 256), VirtQueue::new(1, 256)],
                ..Default::default()
            },
            rx_buffers: VecDeque::new(),
            tx_buffers: VecDeque::new(),
            console_sock: None,
        }
    }

    /// Set the console socket path.
    pub fn set_console_sock(&mut self, path: String) {
        self.console_sock = Some(path);
    }

    /// Get the next transmit buffer (data from guest).
    pub fn pop_tx(&mut self) -> Option<Vec<u8>> {
        self.tx_buffers.pop_front()
    }

    /// Push a receive buffer (data to guest).
    pub fn push_rx(&mut self, data: Vec<u8>) {
        self.rx_buffers.push_back(data);
    }
}

impl VirtioDevice for VirtioConsole {
    fn device_id(&self) -> u16 {
        self.device_id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn max_queue_size(&self) -> u16 {
        256
    }

    fn features(&self) -> u64 {
        self.features
    }

    fn config_read(&self, offset: u64, data: &mut [u8]) -> Result<()> {
        let mut config = [0u8; 16];
        config[0..2].copy_from_slice(&80u16.to_le_bytes());
        config[2..4].copy_from_slice(&24u16.to_le_bytes());
        config[4..8].copy_from_slice(&1u32.to_le_bytes());
        let len = std::cmp::min(data.len(), (config.len() as u64 - offset) as usize);
        data[..len].copy_from_slice(&config[offset as usize..offset as usize + len]);
        Ok(())
    }

    fn config_write(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        debug!(
            "VirtioConsole config write: offset={:#x}, len={}",
            offset,
            data.len()
        );
        Ok(())
    }

    fn notify_queue(&mut self, queue_idx: u16, ram: &mut [u8]) -> Result<()> {
        let queue = &mut self.common.queues[queue_idx as usize];
        let avail_idx = queue.read_avail_idx(ram);
        while queue.last_avail_idx != avail_idx {
            let desc_idx = queue.read_avail_entry(ram, queue.last_avail_idx);
            if let Some(desc) = queue.read_desc(ram, desc_idx) {
                let buf = queue.read_buffer(ram, &desc);
                match queue_idx {
                    0 => {
                        self.rx_buffers.push_back(buf);
                    }
                    1 => {
                        let text = String::from_utf8_lossy(&buf);
                        print!("{}", text);
                    }
                    _ => {}
                }
                queue.write_used_entry(
                    ram,
                    queue.last_avail_idx,
                    &VirtqUsedElem {
                        id: desc_idx as u32,
                        len: desc.len,
                    },
                );
                queue.last_avail_idx = queue.last_avail_idx.wrapping_add(1);
            }
        }
        queue.write_used_idx(ram, queue.last_avail_idx);
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.common.status = VirtioStatus(0);
        self.common.driver_features = 0;
        self.rx_buffers.clear();
        self.tx_buffers.clear();
        for q in &mut self.common.queues {
            q.ready = false;
            q.last_avail_idx = 0;
        }
        Ok(())
    }

    fn common(&self) -> &CommonConfig {
        &self.common
    }

    fn common_mut(&mut self) -> &mut CommonConfig {
        &mut self.common
    }

    fn snapshot_state(&self) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct S<'a> {
            common: &'a CommonConfig,
            rx_buffers: &'a VecDeque<Vec<u8>>,
            tx_buffers: &'a VecDeque<Vec<u8>>,
        }
        Ok(bincode::serialize(&S { common: &self.common, rx_buffers: &self.rx_buffers, tx_buffers: &self.tx_buffers })?)
    }

    fn restore_state(&mut self, data: &[u8]) -> Result<()> {
        #[derive(Deserialize)]
        struct S {
            common: CommonConfig,
            rx_buffers: VecDeque<Vec<u8>>,
            tx_buffers: VecDeque<Vec<u8>>,
        }
        let s: S = bincode::deserialize(data)?;
        self.common = s.common;
        self.rx_buffers = s.rx_buffers;
        self.tx_buffers = s.tx_buffers;
        Ok(())
    }
}

// ── VirtioInput ─────────────────────────────────────────────────────────────

/// Virtio input device (keyboard, touchscreen).
pub struct VirtioInput {
    device_id: u16,
    name: String,
    features: u64,
    common: CommonConfig,
    events: VecDeque<InputEvent>,
}

/// Input event (virtio_input_event).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InputEvent {
    pub type_: u16,
    pub code: u16,
    pub value: u32,
}

impl VirtioInput {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            device_id: 18,
            name: name.into(),
            features: 0,
            common: CommonConfig {
                queues: vec![VirtQueue::new(0, 256), VirtQueue::new(1, 256)],
                ..Default::default()
            },
            events: VecDeque::new(),
        }
    }

    /// Push an input event.
    pub fn push_event(&mut self, event: InputEvent) {
        self.events.push_back(event);
    }
}

impl VirtioDevice for VirtioInput {
    fn device_id(&self) -> u16 {
        self.device_id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn max_queue_size(&self) -> u16 {
        256
    }

    fn features(&self) -> u64 {
        self.features
    }

    fn config_read(&self, _offset: u64, data: &mut [u8]) -> Result<()> {
        let config = [0u8; 64];
        let len = std::cmp::min(data.len(), (config.len() as u64 - _offset) as usize);
        data[..len].copy_from_slice(&config[..len]);
        Ok(())
    }

    fn config_write(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        debug!(
            "VirtioInput config write: offset={:#x}, len={}",
            offset,
            data.len()
        );
        Ok(())
    }

    fn notify_queue(&mut self, queue_idx: u16, ram: &mut [u8]) -> Result<()> {
        let queue = &mut self.common.queues[queue_idx as usize];
        let avail_idx = queue.read_avail_idx(ram);
        while queue.last_avail_idx != avail_idx {
            let desc_idx = queue.read_avail_entry(ram, queue.last_avail_idx);
            if let Some(desc) = queue.read_desc(ram, desc_idx) {
                match queue_idx {
                    0 => {
                        let mut off = 0;
                        while off + 8 <= desc.len as usize {
                            if let Some(event) = self.events.pop_front() {
                                let src = desc.addr as usize + off;
                                let end = src + 8;
                                if src < ram.len() {
                                    let mut bytes = [0u8; 8];
                                    bytes[0..2].copy_from_slice(&event.type_.to_le_bytes());
                                    bytes[2..4].copy_from_slice(&event.code.to_le_bytes());
                                    bytes[4..8].copy_from_slice(&event.value.to_le_bytes());
                                    let len = std::cmp::min(8, end - src);
                                    ram[src..src + len].copy_from_slice(&bytes[..len]);
                                }
                                off += 8;
                            } else {
                                break;
                            }
                        }
                    }
                    1 => {
                        // transmitter — ignore for now
                    }
                    _ => {}
                }
                queue.write_used_entry(
                    ram,
                    queue.last_avail_idx,
                    &VirtqUsedElem {
                        id: desc_idx as u32,
                        len: desc.len,
                    },
                );
                queue.last_avail_idx = queue.last_avail_idx.wrapping_add(1);
            }
        }
        queue.write_used_idx(ram, queue.last_avail_idx);
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.common.status = VirtioStatus(0);
        self.common.driver_features = 0;
        self.events.clear();
        for q in &mut self.common.queues {
            q.ready = false;
            q.last_avail_idx = 0;
        }
        Ok(())
    }

    fn common(&self) -> &CommonConfig {
        &self.common
    }

    fn common_mut(&mut self) -> &mut CommonConfig {
        &mut self.common
    }

    fn snapshot_state(&self) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct S<'a> {
            common: &'a CommonConfig,
            events: &'a VecDeque<InputEvent>,
        }
        Ok(bincode::serialize(&S { common: &self.common, events: &self.events })?)
    }

    fn restore_state(&mut self, data: &[u8]) -> Result<()> {
        #[derive(Deserialize)]
        struct S {
            common: CommonConfig,
            events: VecDeque<InputEvent>,
        }
        let s: S = bincode::deserialize(data)?;
        self.common = s.common;
        self.events = s.events;
        Ok(())
    }
}

// ── VirtioNet ───────────────────────────────────────────────────────────────

/// Virtio network device.
pub struct VirtioNet {
    device_id: u16,
    name: String,
    features: u64,
    common: CommonConfig,
    mac: [u8; 6],
    rx_packets: VecDeque<Vec<u8>>,
}

impl VirtioNet {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            device_id: 1,
            name: name.into(),
            features: (1 << 0) | (1 << 1) | (1 << 5) | (1 << 10) | (1 << 16),
            common: CommonConfig {
                queues: vec![VirtQueue::new(0, 256), VirtQueue::new(1, 256)],
                ..Default::default()
            },
            mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
            rx_packets: VecDeque::new(),
        }
    }
}

impl VirtioDevice for VirtioNet {
    fn device_id(&self) -> u16 {
        self.device_id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn max_queue_size(&self) -> u16 {
        256
    }

    fn features(&self) -> u64 {
        self.features
    }

    fn config_read(&self, offset: u64, data: &mut [u8]) -> Result<()> {
        let mut config = [0u8; 32];
        config[0..6].copy_from_slice(&self.mac);
        config[6..8].copy_from_slice(&0u16.to_le_bytes());
        config[8..10].copy_from_slice(&1u16.to_le_bytes());
        config[10..12].copy_from_slice(&1500u16.to_le_bytes());
        let len = std::cmp::min(data.len(), (config.len() as u64 - offset) as usize);
        data[..len].copy_from_slice(&config[offset as usize..offset as usize + len]);
        Ok(())
    }

    fn config_write(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        debug!(
            "VirtioNet config write: offset={:#x}, len={}",
            offset,
            data.len()
        );
        Ok(())
    }

    fn notify_queue(&mut self, queue_idx: u16, ram: &mut [u8]) -> Result<()> {
        let queue = &mut self.common.queues[queue_idx as usize];
        let avail_idx = queue.read_avail_idx(ram);
        while queue.last_avail_idx != avail_idx {
            let desc_idx = queue.read_avail_entry(ram, queue.last_avail_idx);
            if let Some(desc) = queue.read_desc(ram, desc_idx) {
                match queue_idx {
                    0 => {
                        // RX packets — empty for now (no host networking
                        // backend wired up yet; out of scope for this pass,
                        // see the Tier 2 plan's "explicitly out of scope").
                    }
                    1 => {
                        // TX packets — empty for now, same reason.
                    }
                    _ => {}
                }
                queue.write_used_entry(
                    ram,
                    queue.last_avail_idx,
                    &VirtqUsedElem {
                        id: desc_idx as u32,
                        len: desc.len,
                    },
                );
                queue.last_avail_idx = queue.last_avail_idx.wrapping_add(1);
            }
        }
        queue.write_used_idx(ram, queue.last_avail_idx);
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
        self.common.status = VirtioStatus(0);
        self.common.driver_features = 0;
        self.rx_packets.clear();
        for q in &mut self.common.queues {
            q.ready = false;
            q.last_avail_idx = 0;
        }
        Ok(())
    }

    fn common(&self) -> &CommonConfig {
        &self.common
    }

    fn common_mut(&mut self) -> &mut CommonConfig {
        &mut self.common
    }

    fn snapshot_state(&self) -> Result<Vec<u8>> {
        #[derive(Serialize)]
        struct S<'a> {
            common: &'a CommonConfig,
            mac: [u8; 6],
            rx_packets: &'a VecDeque<Vec<u8>>,
        }
        Ok(bincode::serialize(&S { common: &self.common, mac: self.mac, rx_packets: &self.rx_packets })?)
    }

    fn restore_state(&mut self, data: &[u8]) -> Result<()> {
        #[derive(Deserialize)]
        struct S {
            common: CommonConfig,
            mac: [u8; 6],
            rx_packets: VecDeque<Vec<u8>>,
        }
        let s: S = bincode::deserialize(data)?;
        self.common = s.common;
        self.mac = s.mac;
        self.rx_packets = s.rx_packets;
        Ok(())
    }
}
