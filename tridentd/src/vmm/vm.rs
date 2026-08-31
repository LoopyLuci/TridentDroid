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
    /// Path to Android system.img (raw or sparse).
    pub system_image: Option<String>,
    /// Path to Android vendor.img (raw or sparse).
    pub vendor_image: Option<String>,
    /// Console socket path.
    pub console_sock: Option<String>,
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
        let vm = hyp.create_vm(config.vcpu_count as u32)
            .with_context(|| "Failed to create VM")?;

        // ── Allocate guest RAM ─────────────────────────────────────────────
        let mem_bytes = (config.memory_mib as usize) << 20;
        let mut ram = aligned_alloc(mem_bytes)?;

        hyp.map_memory(
            &vm,
            0,
            &ram,
            MemFlags::RWX_TRACKED,
        )
        .with_context(|| "Failed to map guest RAM")?;

        // ── Load kernel into RAM ───────────────────────────────────────────
        let entry_gpa = KernelLoader::load(
            &mut ram,
            &config.kernel_path,
            config.initrd_path.as_deref(),
            &config.cmdline,
        )
        .with_context(|| "Failed to load kernel")?;
        info!("Kernel entry GPA: {:#x}", entry_gpa);
        let entry_bytes = &ram[entry_gpa as usize..entry_gpa as usize + 16];
        eprintln!("DBG entry bytes: {:02x?}", entry_bytes);

        // ── Create vCPUs and configure registers ───────────────────────────
        let mut vcpus = Vec::with_capacity(config.vcpu_count as usize);
        for id in 0..config.vcpu_count as u32 {
            let mut vcpu = hyp
                .create_vcpu(&vm, id)
                .with_context(|| format!("Failed to create vCPU {id}"))?;

            VcpuRunner::configure_boot_regs(hyp.as_ref(), &mut vcpu, entry_gpa, mem_bytes as u64)
                .with_context(|| format!("Failed to configure vCPU {id} registers"))?;
            hyp.set_vcpu_ram_hint(&mut vcpu, ram.as_ptr(), ram.len());
            hyp.start_debug_sampler(&vcpu, 500);

            vcpus.push(vcpu);
        }

        // ── Device setup ───────────────────────────────────────────────────
        let mut devices = DeviceManager::init()
            .with_context(|| "Device init failed")?;
        // Register virtio devices for the VM.
        // We do this after RAM is allocated but before vCPUs run.

        // Virtio console (for kernel messages and shell).
        let mut console = super::virtio::VirtioConsole::new("console");
        if let Some(ref sock) = config.console_sock {
            console.set_console_sock(sock.clone());
        }
        devices
            .register_virtio(Arc::new(std::sync::Mutex::new(
                console
            )));

        // Virtio block (for system.img/vendor.img).
        let mut blk = super::virtio::VirtioBlk::new("system");
        if let Some(ref path) = config.system_image {
            blk.set_backing(path)?;
        }
        devices.register_virtio(Arc::new(std::sync::Mutex::new(blk)));

        // Virtio block 2 (for vendor.img).
        let mut vendor_blk = super::virtio::VirtioBlk::new("vendor");
        if let Some(ref path) = config.vendor_image {
            vendor_blk.set_backing(path)?;
        }
        devices.register_virtio(Arc::new(std::sync::Mutex::new(vendor_blk)));

        // Virtio network.
        devices
            .register_virtio(Arc::new(std::sync::Mutex::new(
                super::virtio::VirtioNet::new("net")
            )));

        // Virtio input.
        devices
            .register_virtio(Arc::new(std::sync::Mutex::new(
                super::virtio::VirtioInput::new("input")
            )));

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
    pub async fn run(mut self) -> Result<()> {
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
            warn!("SR-IOV VF specified but not yet supported on Windows: {vf}");
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
                    crate::vmm::vcpu_loop::VcpuRunner::run_loop(&h, vcpu, devs, ram)
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
    use std::alloc::{alloc_zeroed, Layout};
    const PAGE: usize = 4096;
    let layout = Layout::from_size_align(size, PAGE)
        .map_err(|e| anyhow::anyhow!("Layout error: {e}"))?;
    let ptr = unsafe { alloc_zeroed(layout) };
    anyhow::ensure!(!ptr.is_null(), "Out of memory allocating {} MiB guest RAM", size >> 20);
    // SAFETY: we just allocated this with the correct layout.
    Ok(unsafe { Vec::from_raw_parts(ptr, size, size) })
}
