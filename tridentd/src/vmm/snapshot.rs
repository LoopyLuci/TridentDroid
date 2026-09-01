//! Snapshot file format: a small bincode-serialized metadata header
//! (VmConfig, per-vCPU register state, one opaque state blob per device, in
//! registration order) followed by the raw guest RAM bytes verbatim.
//!
//! Metadata is kept separate from the (potentially multi-GB) RAM dump
//! deliberately — bincode-ing the whole thing as one value would mean an
//! extra full copy of RAM in memory during (de)serialization.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::Path;
use trident_hal::{Regs, Sregs};

use super::vm::VmConfig;

const MAGIC: [u8; 8] = *b"TRIDSNAP";
const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone)]
pub struct VcpuState {
    pub regs: Regs,
    pub sregs: Sregs,
}

#[derive(Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub config: VmConfig,
    pub vcpus: Vec<VcpuState>,
    /// One opaque `Device::snapshot_state()` blob per registered device, in
    /// the same registration order `Vm::create`/`Vm::restore` always use.
    pub devices: Vec<Vec<u8>>,
    pub ram_len: u64,
}

pub struct WrittenSnapshot {
    pub total_bytes: u64,
}

/// Write a snapshot file: magic, format version, metadata length, bincode
/// metadata, then the raw RAM bytes.
pub fn write_snapshot(path: &Path, meta: &SnapshotMetadata, ram: &[u8]) -> Result<WrittenSnapshot> {
    anyhow::ensure!(
        meta.ram_len as usize == ram.len(),
        "SnapshotMetadata.ram_len doesn't match the RAM buffer passed in"
    );
    let meta_bytes = bincode::serialize(meta).context("serializing snapshot metadata")?;

    let mut file = std::fs::File::create(path)
        .with_context(|| format!("creating snapshot file {}", path.display()))?;
    file.write_all(&MAGIC)?;
    file.write_all(&FORMAT_VERSION.to_le_bytes())?;
    file.write_all(&(meta_bytes.len() as u64).to_le_bytes())?;
    file.write_all(&meta_bytes)?;
    file.write_all(ram)?;
    file.flush()?;

    let total_bytes = (MAGIC.len() + 4 + 8 + meta_bytes.len() + ram.len()) as u64;
    Ok(WrittenSnapshot { total_bytes })
}

/// Read a snapshot file back into its metadata and raw RAM bytes.
pub fn read_snapshot(path: &Path) -> Result<(SnapshotMetadata, Vec<u8>)> {
    let mut file = std::fs::File::open(path)
        .with_context(|| format!("opening snapshot file {}", path.display()))?;

    let mut magic = [0u8; 8];
    file.read_exact(&mut magic).context("reading snapshot magic")?;
    anyhow::ensure!(magic == MAGIC, "{} is not a TridentDroid snapshot file", path.display());

    let mut version_bytes = [0u8; 4];
    file.read_exact(&mut version_bytes)?;
    let version = u32::from_le_bytes(version_bytes);
    anyhow::ensure!(
        version == FORMAT_VERSION,
        "unsupported snapshot format version {version} (expected {FORMAT_VERSION})"
    );

    let mut len_bytes = [0u8; 8];
    file.read_exact(&mut len_bytes)?;
    let meta_len = u64::from_le_bytes(len_bytes) as usize;
    let mut meta_bytes = vec![0u8; meta_len];
    file.read_exact(&mut meta_bytes).context("reading snapshot metadata")?;
    let meta: SnapshotMetadata = bincode::deserialize(&meta_bytes).context("deserializing snapshot metadata")?;

    let mut ram = vec![0u8; meta.ram_len as usize];
    file.read_exact(&mut ram).context("reading snapshot RAM dump")?;

    Ok((meta, ram))
}
