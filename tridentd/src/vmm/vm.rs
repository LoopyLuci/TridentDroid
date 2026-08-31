use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{info, warn};
use trident_hal::{Hypervisor, MemFlags};

use super::loader::KernelLoader;
use super::vcpu_loop::VcpuRunner;
use super::device::DeviceManager;

#[derive(Debug, Clone)]
pub struct VmConfig {
    pub vcpu_count:  u8,
    /// RAM in MiB.
    pub memory_mib:  u64,
    pub kernel_path: String,
    pub initrd_path: Option<String>,
    pub cmdline:     String,
    /// PCI address of SR-IOV VF for direct display (e.g. "0000:03:00.1").
    pub sriov_vf:    Option<String>,
}

/// A running virtual machine.
///
/// Generic over the platform backend — on Windows `H = WhpHypervisor`,
/// on Linux `H = KvmHypervisor`.  The VMM code above this never sees the
/// concrete type; it only uses the `Hypervisor` trait.
pub struct Vm<H: Hypervisor> {
    hyp:    Arc<H>,
    /// Kept alive for the VM handle; vCPU handles own their partition/fd references.
    _vm:    H::Vm,
    vcpus:  Vec<H::Vcpu>,
    /// Guest RAM: aligned heap allocation kept alive for the VM's lifetime.
    ram:    Vec<u8>,
    config: VmConfig,
    /// Device manager — owns all virtio devices.
    devices: Arc<std::sync::Mutex<super::device::DeviceManager>>,
}

impl<H: Hypervisor> Vm<H> {
    pub fn create(hyp: &Arc<H>, config: VmConfig) -> Result<Self> {
        let vm = hyp.create_vm(config.vcpu_count as u32).context("Failed to create VM")?;

        // ── Allocate guest RAM ─────────────────────────────────────────────
        let mem_bytes = (config.memory_mib as usize) << 20;
        // 4 KiB-aligned allocation so the backend can page-map it directly.
        let mut ram = aligned_alloc(mem_bytes)?;

        hyp.map_memory(
            &vm,
            0, // GPA 0 = start of guest physical address space
            &ram,
            MemFlags::RWX_TRACKED,
        )
        .context("Failed to map guest RAM")?;

        // ── Load kernel into RAM ───────────────────────────────────────────
        let entry_gpa = KernelLoader::load(
            &mut ram,
            &config.kernel_path,
            config.initrd_path.as_deref(),
            &config.cmdline,
        )
        .context("Failed to load kernel")?;
        info!("Kernel entry GPA: {:#x}", entry_gpa);
        // Dump first 16 bytes at entry to verify kernel was loaded correctly.
        let entry_bytes = &ram[entry_gpa as usize..entry_gpa as usize + 16];
        eprintln!("DBG entry bytes: {:02x?}", entry_bytes);

        // ── Create vCPUs and configure registers ───────────────────────────
        let mut vcpus = Vec::with_capacity(config.vcpu_count as usize);
        for id in 0..config.vcpu_count as u32 {
            let mut vcpu = hyp
                .create_vcpu(&vm, id)
                .with_context(|| format!("Failed to create vCPU {}", id))?;

            VcpuRunner::configure_boot_regs(hyp.as_ref(), &mut vcpu, entry_gpa, mem_bytes as u64)
                .with_context(|| format!("Failed to configure vCPU {} registers", id))?;
            hyp.set_vcpu_ram_hint(&mut vcpu, ram.as_ptr(), ram.len());
            hyp.start_debug_sampler(&vcpu, 500);

            vcpus.push(vcpu);
        }

        // ── Device setup ───────────────────────────────────────────────────
        let mut devices = DeviceManager::init().context("Device init failed")?;
        // Register virtio devices for the VM.
        // We do this after RAM is allocated but before vCPUs run.

        // Virtio console (for kernel messages and shell).
        devices
            .register_virtio(Arc::new(super::device::virtio::VirtioConsole::new("console")));
        // Virtio block (for system.img/vendor.img).
        devices
            .register_virtio(Arc::new(super::device::virtio::VirtioBlk::new("system")));
        // Virtio network.
        devices
            .register_virtio(Arc::new(super::device::virtio::VirtioNet::new("net")));
        // Virtio input.
        devices
            .register_virtio(Arc::new(super::device::virtio::VirtioInput::new("input")));

        let devices = Arc::new(std::sync::Mutex::new(devices));

        Ok(Self {
            hyp: hyp.clone(),
            _vm: vm,
            vcpus,
            ram,
            config,
            devices,
        })
    }

    /// Start all vCPU threads and wait for them to exit.
    pub async fn run(self) -> Result<()> {
        info!("Starting {} vCPU(s) [{}/{}]",
            self.vcpus.len(),
            if cfg!(windows) { "WHP" } else { "KVM" },
            if cfg!(windows) { "Windows" } else { "Linux" }
        );

        // SR-IOV direct display (Linux only for now)
        if let Some(ref vf) = self.config.sriov_vf {
            #[cfg(target_os = "linux")]
            crate::gpu::sriov::attach_vf_display(vf)?;
            #[cfg(windows)]
            warn!("SR-IOV VF specified but not yet supported on Windows: {}", vf);
        }

        let hyp = self.hyp.clone();
        let devices = self.devices.clone();
        let ram = std::sync::Arc::new(std::sync::Mutex::new(std::mem::take(&mut self.ram)));
        let handles: Vec<_> = self
            .vcpus
            .into_iter()
            .map(|vcpu| {
                let h = hyp.clone();
                let devs = devices.clone();
                let ram = Arc::clone(&ram);
                tokio::task::spawn_blocking(move || {
                    tridentd_lib::vmm::vcpu_loop::VcpuRunner::run_loop(h.as_ref(), vcpu, devs, ram)
                })
            })
            .collect();

        for handle in handles {
            handle.await??;
        }

        info!("All vCPUs exited");
        Ok(())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Allocate a 4 KiB-aligned Vec<u8> of exactly `size` bytes.
fn aligned_alloc(size: usize) -> Result<Vec<u8>> {
    // Rust's global allocator aligns Vec to the element type (u8 = 1 byte).
    // We need 4 KiB alignment for WHP/KVM GPA mapping.
    // Over-allocate by PAGE_SIZE and take the aligned sub-slice... but we
    // need to own it.  Simplest portable approach: use a boxed slice with
    // manual alignment via alloc::alloc::alloc.
    use std::alloc::{alloc_zeroed, Layout};
    const PAGE: usize = 4096;
    let layout = Layout::from_size_align(size, PAGE)
        .map_err(|e| anyhow::anyhow!("Layout error: {}", e))?;
    let ptr = unsafe { alloc_zeroed(layout) };
    anyhow::ensure!(!ptr.is_null(), "Out of memory allocating {} MiB guest RAM", size >> 20);
    // SAFETY: we just allocated this with the correct layout.
    Ok(unsafe { Vec::from_raw_parts(ptr, size, size) })
}
