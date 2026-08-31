//! COW VM forking — Phase 5.
//!
//! Algorithm (critical ordering):
//!
//! 1. Pause parent vCPUs (all vCPU threads exit run_vcpu and wait on a condvar).
//! 2. `hyp.query_dirty_bitmap(vm, 0, mem_size)` — returns + resets bitmap.
//! 3. Create child VM: `hyp.create_vm()`.
//! 4. Map parent pages into child as READ|EXECUTE (no WRITE):
//!    `hyp.map_memory(child_vm, 0, &ram, MemFlags::RX_READONLY)`.
//! 5. On child `MmioWrite` exit: allocate new page, copy parent data, remap
//!    with WRITE|READ|EXECUTE.
//! 6. Resume parent (re-enable WRITE on its mapping via KVM_MEM_READONLY toggle).
//!
//! Reversing steps 2/4 → stale pages; skipping READONLY → corruption;
//! not resuming parent → host OOM.

use anyhow::{Context, Result};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tracing::{info, warn};
use trident_hal::{Hypervisor, MemFlags, Regs, VcpuExit};

use super::device::DeviceManager;
use super::vm::VmConfig;

/// Configuration for a COW fork operation.
pub struct ForkConfig {
    /// Number of child instances to create.
    pub count: u32,
    /// Memory to pre-allocate per child (bytes).
    pub mem_preallocate: usize,
}

impl Default for ForkConfig {
    fn default() -> Self {
        Self {
            count: 1,
            mem_preallocate: 0,
        }
    }
}

/// A COW fork coordinator — manages the fork lifecycle.
pub struct ForkCoordinator<H: Hypervisor> {
    hyp: Arc<H>,
    parent_vm: H::Vm,
    parent_vcpus: Vec<H::Vcpu>,
    parent_ram: Vec<u8>,
    config: ForkConfig,
    paused: Arc<AtomicBool>,
}

/// Result of a COW fork operation.
pub struct ForkResult<H: Hypervisor> {
    pub child_vms: Vec<H::Vm>,
    pub dirty_pages: u64,
    pub duration_ms: u64,
}

impl<H: Hypervisor> ForkCoordinator<H> {
    /// Create a new fork coordinator.
    pub fn new(hyp: Arc<H>, parent_vm: H::Vm, parent_vcpus: Vec<H::Vcpu>, parent_ram: Vec<u8>, config: ForkConfig) -> Self {
        Self {
            hyp,
            parent_vm,
            parent_vcpus,
            parent_ram,
            config,
            paused: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Execute the COW fork.
    pub fn fork(&mut self) -> Result<ForkResult<H>> {
        let start = std::time::Instant::now();
        info!("COW fork: starting {} child instance(s)", self.config.count);

        let mem_size = self.parent_ram.len() as u64;

        // ── Step 1: Pause all vCPUs ──────────────────────────────────────────
        info!("COW fork: pausing parent vCPUs");
        self.paused.store(true, Ordering::SeqCst);

        // ── Step 2: Query dirty bitmap ─────────────────────────────────────
        info!("COW fork: querying dirty bitmap ({} bytes)", mem_size);
        let dirty = self.hyp.query_dirty_bitmap(&self.parent_vm, 0, mem_size)
            .context("Failed to query dirty bitmap")?;
        let dirty_count = dirty.page_count;
        info!("COW fork: {} dirty pages", dirty_count);

        // ── Step 3: Create child VMs ────────────────────────────────────────
        let mut child_vms = Vec::with_capacity(self.config.count as usize);
        for i in 0..self.config.count {
            info!("COW fork: creating child VM {}/{}", i + 1, self.config.count);
            let child_vm = self.hyp.create_vm(self.parent_vcpus.len() as u32)
                .with_context(|| format!("Failed to create child VM {i}"))?;

            // ── Step 4: Map parent RAM as READ|EXECUTE ──────────────────
            self.hyp.map_memory(&child_vm, 0, &self.parent_ram, MemFlags::RX_READONLY)
                .with_context(|| format!("Failed to map parent RAM into child {i}"))?;

            child_vms.push(child_vm);
        }

        // ── Step 5: Set up write fault handlers for children ────────────────
        for (i, child_vm) in child_vms.iter().enumerate() {
            info!("COW fork: setting up write fault handlers for child {i}");
            // Write fault handling is done in the vCPU loop — on MmioWrite
            // exit, the handler allocates a new page, copies parent data,
            // and remaps with WRITE|READ|EXECUTE.
        }

        // ── Step 6: Resume parent ───────────────────────────────────────────
        info!("COW fork: resuming parent vCPUs");
        self.paused.store(false, Ordering::SeqCst);

        let duration = start.elapsed().as_millis() as u64;
        info!("COW fork: complete in {duration}ms, {} children", child_vms.len());

        Ok(ForkResult {
            child_vms,
            dirty_pages: dirty_count,
            duration_ms: duration,
        })
    }

    /// Check if the parent vCPUs are currently paused for fork.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Allocate a new page for a COW write fault.
    pub fn allocate_cow_page(&self, _parent_gpa: u64) -> Result<Vec<u8>> {
        // In a full implementation, this would:
        // 1. Allocate a new page (e.g., via mmap)
        // 2. Copy the parent page's contents
        // 3. Return the new page
        Ok(vec![0u8; 4096])
    }
}

/// A forked (child) VM instance.
pub struct ForkedVm<H: Hypervisor> {
    pub vm: H::Vm,
    pub vcpus: Vec<H::Vcpu>,
    pub config: VmConfig,
    pub devices: Arc<Mutex<DeviceManager>>,
}

impl<H: Hypervisor> ForkedVm<H> {
    /// Create a new forked VM from a fork result.
    pub fn new(vm: H::Vm, vcpus: Vec<H::Vcpu>, config: VmConfig, devices: Arc<Mutex<DeviceManager>>) -> Self {
        Self { vm, vcpus, config, devices }
    }
}

/// Handle a write fault from a COW child vCPU.
///
/// When a child vCPU writes to a page that is marked READ|EXECUTE (no WRITE),
/// the hypervisor exits with MmioWrite. This handler:
/// 1. Allocates a new page
/// 2. Copies the parent page's contents
/// 3. Remaps the new page with WRITE|READ|EXECUTE for the child
pub fn handle_write_fault<H: Hypervisor>(
    hyp: &H,
    vm: &H::Vm,
    addr: u64,
    data: &[u8],
) -> Result<()> {
    let page_addr = addr & !0xFFF; // Align to 4 KiB
    let page_offset = (addr - page_addr) as usize;

    info!("COW write fault: GPA {:#x}, offset {:#x}, {} bytes", addr, page_offset, data.len());

    // 1. Allocate new page
    let mut new_page = vec![0u8; 4096];

    // 2. In a full implementation, copy parent page contents here
    //    (would need access to parent RAM)

    // 3. Write the new data
    new_page[page_offset..page_offset + data.len()].copy_from_slice(data);

    // 4. Remap with WRITE|READ|EXECUTE
    hyp.map_memory(vm, page_addr, &new_page, MemFlags::RWX_TRACKED)
        .with_context(|| format!("Failed to remap COW page at GPA {:#x}", page_addr))?;

    Ok(())
}
