//! End-to-end test of the virtio-mmio transport (Tier 2) against a real
//! `VirtioBlk`, driven purely through `DeviceManager::mmio_read/write` and a
//! hand-built in-memory RAM buffer — no real hypervisor/kernel needed. This
//! is the most direct proof the transport actually works: feature
//! negotiation readback, queue setup, and a full 3-descriptor virtio-blk
//! read request dispatched via `QUEUE_NOTIFY`.

use std::sync::{Arc, Mutex};
use tridentd_lib::vmm::device::DeviceManager;
use tridentd_lib::vmm::virtio::{mmio, VirtioBlk, VirtioDevice, VIRTQ_DESC_F_NEXT, VIRTQ_DESC_F_WRITE};

/// `DeviceManager` assigns the first registered device this MMIO base.
const BASE: u64 = 0x1000_0000;

fn write_reg(devices: &DeviceManager, ram: &mut [u8], offset: u64, value: u32) {
    devices
        .mmio_write(BASE + offset, &value.to_le_bytes(), ram)
        .expect("mmio_write failed");
}

fn read_reg(devices: &DeviceManager, ram: &[u8], offset: u64) -> u32 {
    let mut buf = [0u8; 4];
    devices
        .mmio_read(BASE + offset, &mut buf, ram)
        .expect("mmio_read failed");
    u32::from_le_bytes(buf)
}

#[test]
fn virtio_blk_transport_negotiates_and_transfers_a_read_request() {
    let mut ram = vec![0u8; 0x10000];

    let backing = b"HELLO FROM VIRTIO BLK BACKING DATA".to_vec();
    let mut blk = VirtioBlk::new("test-blk");
    blk.set_backing_data(backing.clone());
    let expected_features = blk.features();

    let mut devices = DeviceManager::init().expect("DeviceManager::init");
    devices.register_virtio(Arc::new(Mutex::new(blk)));

    // ── MAGIC / VERSION / DEVICE_ID probing (what a real driver checks first) ──
    assert_eq!(read_reg(&devices, &ram, mmio::MAGIC), 0x74726976);
    assert_eq!(read_reg(&devices, &ram, mmio::VERSION), 2);
    assert_eq!(read_reg(&devices, &ram, mmio::DEVICE_ID), 2); // virtio-blk

    // ── Feature negotiation readback ──
    write_reg(&devices, &mut ram, mmio::DEVICE_FEATURES_SEL, 0);
    assert_eq!(
        read_reg(&devices, &ram, mmio::DEVICE_FEATURES) as u64,
        expected_features & 0xFFFF_FFFF,
        "DEVICE_FEATURES readback didn't match the device's real features()"
    );

    // ── Queue setup ──
    const DESC_ADDR: u64 = 0x2000;
    const AVAIL_ADDR: u64 = 0x3000;
    const USED_ADDR: u64 = 0x4000;
    const HDR_ADDR: u64 = 0x5000;
    const DATA_ADDR: u64 = 0x5100;
    const STATUS_ADDR: u64 = 0x5300;

    write_reg(&devices, &mut ram, mmio::QUEUE_SEL, 0);
    assert_eq!(read_reg(&devices, &ram, mmio::QUEUE_NUM_MAX), 256);
    write_reg(&devices, &mut ram, mmio::QUEUE_NUM, 256);
    write_reg(&devices, &mut ram, mmio::QUEUE_DESC_LOW, DESC_ADDR as u32);
    write_reg(&devices, &mut ram, mmio::QUEUE_DESC_HIGH, (DESC_ADDR >> 32) as u32);
    write_reg(&devices, &mut ram, mmio::QUEUE_DRIVER_LOW, AVAIL_ADDR as u32);
    write_reg(&devices, &mut ram, mmio::QUEUE_DRIVER_HIGH, (AVAIL_ADDR >> 32) as u32);
    write_reg(&devices, &mut ram, mmio::QUEUE_DEVICE_LOW, USED_ADDR as u32);
    write_reg(&devices, &mut ram, mmio::QUEUE_DEVICE_HIGH, (USED_ADDR >> 32) as u32);
    write_reg(&devices, &mut ram, mmio::QUEUE_READY, 1);
    assert_eq!(read_reg(&devices, &ram, mmio::QUEUE_READY), 1);

    write_reg(&devices, &mut ram, mmio::STATUS, 0b0000_1111); // ACK|DRIVER|FEATURES_OK|DRIVER_OK
    assert_eq!(read_reg(&devices, &ram, mmio::STATUS), 0b0000_1111);

    // ── Build a standard 3-descriptor virtio-blk READ request by hand ──
    // desc[0]: header (readable), 16 bytes at HDR_ADDR, next -> desc[1]
    write_desc(&mut ram, DESC_ADDR, 0, HDR_ADDR, 16, VIRTQ_DESC_F_NEXT, 1);
    // desc[1]: data buffer (writable), 512 bytes at DATA_ADDR, next -> desc[2]
    write_desc(&mut ram, DESC_ADDR, 1, DATA_ADDR, 512, VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE, 2);
    // desc[2]: status byte (writable), 1 byte at STATUS_ADDR, no next
    write_desc(&mut ram, DESC_ADDR, 2, STATUS_ADDR, 1, VIRTQ_DESC_F_WRITE, 0);

    // Header: type=0 (VIRTIO_BLK_T_IN / read), reserved=0, sector=0.
    ram[HDR_ADDR as usize..HDR_ADDR as usize + 4].copy_from_slice(&0u32.to_le_bytes());
    ram[HDR_ADDR as usize + 4..HDR_ADDR as usize + 8].copy_from_slice(&0u32.to_le_bytes());
    ram[HDR_ADDR as usize + 8..HDR_ADDR as usize + 16].copy_from_slice(&0u64.to_le_bytes());

    // Avail ring: flags=0, idx=1, ring[0]=0 (points at desc chain head 0).
    ram[AVAIL_ADDR as usize..AVAIL_ADDR as usize + 2].copy_from_slice(&0u16.to_le_bytes());
    ram[AVAIL_ADDR as usize + 2..AVAIL_ADDR as usize + 4].copy_from_slice(&1u16.to_le_bytes());
    ram[AVAIL_ADDR as usize + 4..AVAIL_ADDR as usize + 6].copy_from_slice(&0u16.to_le_bytes());

    // ── Kick it ──
    write_reg(&devices, &mut ram, mmio::QUEUE_NOTIFY, 0);

    // ── Verify the data buffer actually received the backing data, and the
    //    status byte reports success — this is the actual proof of transfer. ──
    let data_start = DATA_ADDR as usize;
    assert_eq!(
        &ram[data_start..data_start + backing.len()],
        backing.as_slice(),
        "data descriptor did not receive the backing image's bytes"
    );
    assert_eq!(ram[STATUS_ADDR as usize], 0, "status byte should report VIRTIO_BLK_S_OK");

    // Used ring index should have advanced past the one processed request.
    let used_idx = u16::from_le_bytes(ram[USED_ADDR as usize + 2..USED_ADDR as usize + 4].try_into().unwrap());
    assert_eq!(used_idx, 1, "used ring index should advance by one after processing the request");
}

fn write_desc(ram: &mut [u8], table_addr: u64, idx: u16, addr: u64, len: u32, flags: u16, next: u16) {
    let offset = table_addr as usize + idx as usize * 16;
    ram[offset..offset + 8].copy_from_slice(&addr.to_le_bytes());
    ram[offset + 8..offset + 12].copy_from_slice(&len.to_le_bytes());
    ram[offset + 12..offset + 14].copy_from_slice(&flags.to_le_bytes());
    ram[offset + 14..offset + 16].copy_from_slice(&next.to_le_bytes());
}
