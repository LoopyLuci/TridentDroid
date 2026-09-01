use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};
use trident_hal::{Hypervisor, MemFlags};

use super::device::DeviceManager;
use super::loader::KernelLoader;
use super::pause::PauseGate;
use super::snapshot::{self, SnapshotMetadata, VcpuState, WrittenSnapshot};
use super::vcpu_loop::VcpuRunner;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Overrides accepted on restore (mirrors `RestoreRequest`'s optional
/// hardware overrides — `0`/absent means "use the snapshot's own value").
#[derive(Default)]
pub struct RestoreOverrides {
    pub vcpu_count: Option<u8>,
    pub memory_mib: Option<u64>,
}

/// A running virtual machine.
///
/// Generic over the platform backend — on Windows `H = WhpHypervisor`,
/// on Linux `H = KvmHypervisor`.  The VMM code above this never sees the
/// concrete type; it only uses the `Hypervisor` trait.
pub struct Vm<H: Hypervisor> {
    // Field order matters here: Rust drops struct fields top-to-bottom, and
    // both `H::Vcpu`'s `Drop` (e.g. `WHvDeleteVirtualProcessor`) and
    // `H::Vm`'s `Drop` (`WHvDeletePartition`) need the backend (`hyp`) to
    // still be "live", and each vCPU's partition to still exist — so
    // `vcpus` must be listed (and therefore dropped) before `_vm`, and
    // `hyp` must be listed after both. Getting this backwards doesn't
    // error, it corrupts the process (confirmed empirically on WHP:
    // dropping `hyp` before the vCPUs/partition it created reliably
    // crashes with STATUS_HEAP_CORRUPTION).
    vcpus:  Vec<H::Vcpu>,
    /// The VM/partition handle. `Arc`-wrapped (not owned outright) because
    /// `spawn_vcpus()` lets vCPU threads outlive the `Vm` value itself —
    /// each spawned thread clones this to keep the underlying partition
    /// alive for as long as it's still calling into it. Dropping the last
    /// reference (this field plus every spawned thread's clone) is what
    /// actually tears the partition down.
    _vm:    Arc<H::Vm>,
    /// Guest RAM. Shared (not owned outright) from creation time so both
    /// the running vCPU threads and `snapshot()` can reach it.
    ram:    Arc<std::sync::Mutex<AlignedRam>>,
    config: VmConfig,
    /// Device manager — owns all virtio devices.
    devices: Arc<std::sync::Mutex<super::device::DeviceManager>>,
    /// Coordinates pausing every vCPU thread for a clean snapshot.
    pause_gate: Arc<PauseGate>,
    /// The hypervisor backend. Declared last so it's dropped last — see the
    /// field-order note on `vcpus` above.
    hyp: Arc<H>,
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
        let devices = Arc::new(std::sync::Mutex::new(build_devices(&config)?));
        let pause_gate = PauseGate::new(vcpus.len());
        let ram = Arc::new(std::sync::Mutex::new(ram));

        Ok(Self {
            hyp: hyp.clone(),
            _vm: Arc::new(vm),
            vcpus,
            ram,
            config,
            devices,
            pause_gate,
        })
    }

    /// Rebuild a `Vm` from a snapshot file. The vCPUs and devices start
    /// fresh (new hypervisor handles), then have the saved register/device
    /// state pushed into them — the underlying `H::Vm`/`H::Vcpu` handles
    /// themselves are never serialized (they're opaque by design).
    pub fn restore(hyp: &Arc<H>, path: &Path, overrides: RestoreOverrides) -> Result<Self> {
        let (meta, ram_bytes) = snapshot::read_snapshot(path)?;
        let mut config = meta.config;
        if let Some(n) = overrides.vcpu_count { config.vcpu_count = n; }
        if let Some(m) = overrides.memory_mib { config.memory_mib = m; }

        anyhow::ensure!(
            meta.vcpus.len() == config.vcpu_count as usize,
            "snapshot has {} vCPU(s) but restore requested {} — vCPU-count override on restore isn't supported yet",
            meta.vcpus.len(),
            config.vcpu_count
        );

        let vm = hyp.create_vm(config.vcpu_count as u32).context("Failed to create VM for restore")?;

        let mem_bytes = (config.memory_mib as usize) << 20;
        anyhow::ensure!(
            ram_bytes.len() <= mem_bytes,
            "snapshot RAM ({} bytes) is larger than the configured {} MiB",
            ram_bytes.len(),
            config.memory_mib
        );
        let mut ram = aligned_alloc(mem_bytes)?;
        ram[..ram_bytes.len()].copy_from_slice(&ram_bytes);

        hyp.map_memory(&vm, 0, &ram, MemFlags::RWX_TRACKED)
            .context("Failed to map guest RAM for restore")?;

        let mut vcpus = Vec::with_capacity(config.vcpu_count as usize);
        for (id, state) in meta.vcpus.iter().enumerate() {
            let mut vcpu = hyp.create_vcpu(&vm, id as u32)
                .with_context(|| format!("Failed to create vCPU {id} for restore"))?;
            hyp.set_regs(&vcpu, &state.regs).context("restoring vCPU regs")?;
            hyp.set_sregs(&vcpu, &state.sregs).context("restoring vCPU sregs")?;
            hyp.set_vcpu_ram_hint(&mut vcpu, ram.as_ptr(), ram.len());
            hyp.start_debug_sampler(&vcpu, 500);
            vcpus.push(vcpu);
        }

        // Devices: rebuild in the exact same order `build_devices` always
        // uses, then push the saved state into each one.
        let devices = build_devices(&config)?;
        anyhow::ensure!(
            devices.device_count() == meta.devices.len(),
            "snapshot has {} device(s) but this build registers {} — snapshot was likely taken with a different tridentd version",
            meta.devices.len(),
            devices.device_count()
        );
        for (idx, blob) in meta.devices.iter().enumerate() {
            let dev = devices.device_at(idx).expect("index within device_count()");
            dev.lock().unwrap().restore_state(blob).map_err(|e| anyhow::anyhow!(e))
                .with_context(|| format!("restoring device #{idx} state"))?;
        }
        let devices = Arc::new(std::sync::Mutex::new(devices));

        let pause_gate = PauseGate::new(vcpus.len());
        let ram = Arc::new(std::sync::Mutex::new(ram));

        info!("Restored VM from {} ({} vCPU(s), {} MiB RAM)", path.display(), vcpus.len(), config.memory_mib);

        Ok(Self {
            hyp: hyp.clone(),
            _vm: Arc::new(vm),
            vcpus,
            ram,
            config,
            devices,
            pause_gate,
        })
    }

    /// Build an Android-specific kernel cmdline.
    pub fn android_cmdline(
        &self,
        console: &str,
        root: &str,
        extra: &str,
    ) -> String {
        let mut cmd = format!(
            "console={} earlyprintk=serial androidboot.hardware=trident \
             root={} skip_initrc init=/init androidboot.selinux=permissive \
             firmware_class.path=/vendor/firmware",
            console, root
        );
        if !extra.is_empty() {
            cmd.push(' ');
            cmd.push_str(extra);
        }
        cmd
    }

    /// Build a full Android cmdline with all standard options.
    pub fn android_cmdline_full(&self) -> String {
        self.android_cmdline("ttyS0", "/dev/vda1", "androidboot.serialno=TRIDENT001")
    }

    /// Spawn every vCPU's run-loop thread and return their join handles.
    /// After this returns, `self.vcpus` is empty — vCPU ownership has
    /// transferred to the spawned threads — but `self` remains valid and
    /// usable (e.g. `snapshot()`) via its `Arc`-shared fields.
    pub fn spawn_vcpus(&mut self) -> Vec<tokio::task::JoinHandle<Result<()>>> {
        let vcpus = std::mem::take(&mut self.vcpus);
        vcpus
            .into_iter()
            .enumerate()
            .map(|(index, vcpu)| {
                let h = self.hyp.clone();
                let devs = self.devices.clone();
                let ram = self.ram.clone();
                let pause_gate = self.pause_gate.clone();
                // Keeps the underlying partition/VM handle alive for as
                // long as this thread is running, even if the `Vm` value
                // itself (and every other clone of `_vm`) gets dropped —
                // `vcpu` remains valid to run only while its partition is.
                let _vm_keepalive = self._vm.clone();
                tokio::task::spawn_blocking(move || {
                    let _vm_keepalive = _vm_keepalive;
                    crate::vmm::vcpu_loop::VcpuRunner::run_loop(&h, vcpu, devs, ram, pause_gate, index)
                })
            })
            .collect()
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

        let handles = self.spawn_vcpus();
        for handle in handles {
            handle.await??;
        }

        info!("All vCPUs exited");
        Ok(())
    }

    /// Pause every vCPU, capture full VM state (config, per-vCPU
    /// registers, device transport state, raw RAM), write it to `path`,
    /// then resume. Safe to call while vCPU threads (from `spawn_vcpus`)
    /// are running concurrently.
    /// Precondition: `spawn_vcpus()` must already have been called (and its
    /// threads must be alive) — snapshotting before any vCPU thread exists
    /// to park would block forever waiting for pause acknowledgment.
    pub fn snapshot(&self, path: &Path) -> Result<WrittenSnapshot> {
        self.pause_gate.pause_and(|parked| {
            let vcpus: Vec<VcpuState> = parked
                .iter()
                .map(|p| VcpuState { regs: p.regs.clone(), sregs: p.sregs.clone() })
                .collect();

            let device_blobs = {
                let devices = self.devices.lock().unwrap();
                let mut blobs = Vec::with_capacity(devices.device_count());
                for idx in 0..devices.device_count() {
                    let dev = devices.device_at(idx).expect("index within device_count()");
                    let blob = dev
                        .lock()
                        .unwrap()
                        .snapshot_state()
                        .map_err(|e| anyhow::anyhow!(e))
                        .with_context(|| format!("snapshotting device #{idx} state"))?;
                    blobs.push(blob);
                }
                blobs
            };

            let ram_guard = self.ram.lock().unwrap();
            let meta = SnapshotMetadata {
                config: self.config.clone(),
                vcpus,
                devices: device_blobs,
                ram_len: ram_guard.len() as u64,
            };
            snapshot::write_snapshot(path, &meta, &ram_guard)
        })
    }
}

/// Register the standard virtio device set (console, system/vendor block,
/// net, input) for `config` — used identically by `Vm::create` and
/// `Vm::restore` so registration order (and therefore MMIO base + snapshot
/// device-index assignment) always matches.
fn build_devices(config: &VmConfig) -> Result<DeviceManager> {
    let mut devices = DeviceManager::init().with_context(|| "Device init failed")?;

    let mut console = super::virtio::VirtioConsole::new("console");
    if let Some(ref sock) = config.console_sock {
        console.set_console_sock(sock.clone());
    }
    devices.register_virtio(Arc::new(std::sync::Mutex::new(console)));

    let mut blk = super::virtio::VirtioBlk::new("system");
    if let Some(ref path) = config.system_image {
        blk.set_backing(path)?;
    }
    devices.register_virtio(Arc::new(std::sync::Mutex::new(blk)));

    let mut vendor_blk = super::virtio::VirtioBlk::new("vendor");
    if let Some(ref path) = config.vendor_image {
        vendor_blk.set_backing(path)?;
    }
    devices.register_virtio(Arc::new(std::sync::Mutex::new(vendor_blk)));

    devices.register_virtio(Arc::new(std::sync::Mutex::new(
        super::virtio::VirtioNet::new("net"),
    )));

    devices.register_virtio(Arc::new(std::sync::Mutex::new(
        super::virtio::VirtioInput::new("input"),
    )));

    Ok(devices)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// A page-aligned heap buffer for guest RAM, sized exactly to `len`.
///
/// `WHvMapGpaRange` requires a page-aligned host VA, but a plain `Vec<u8>`
/// gives no alignment guarantee beyond `align_of::<u8>() == 1`. The
/// straightforward-looking fix — `alloc_zeroed` with a page-aligned
/// `Layout`, then `Vec::from_raw_parts(ptr, len, len)` — is actually
/// unsound: `Vec<u8>`'s own `Drop` deallocates using `Layout::array::<u8>(cap)`
/// (alignment 1), which doesn't match the alignment the memory was
/// actually allocated with. That mismatch is undefined behavior per the
/// `GlobalAlloc` contract, and empirically **does** corrupt the process
/// heap on Windows' allocator — confirmed by this exact bug: it went
/// unnoticed through Tiers 0-2 because nothing ever actually dropped a
/// `Vm`'s RAM (it was always moved into a vCPU thread that runs until
/// process exit); Tier 3's snapshot/restore tests were the first code path
/// to ever construct-then-drop a `Vm`, and reliably hit
/// `STATUS_HEAP_CORRUPTION` on drop until this was fixed.
///
/// This type owns the allocation directly and deallocates with the exact
/// `Layout` it was created with. `Deref`/`DerefMut` to `[u8]` mean it's a
/// drop-in replacement everywhere RAM was previously passed as `&[u8]`/
/// `&mut [u8]` (deref coercion handles the rest, including through the
/// `MutexGuard` wrapper `Vm::ram` uses).
pub struct AlignedRam {
    ptr: std::ptr::NonNull<u8>,
    len: usize,
    layout: std::alloc::Layout,
}

// SAFETY: this is a unique owned heap allocation like `Vec<u8>` (Send +
// Sync for the same reason `Vec<u8>` is: `u8` itself is Send + Sync and we
// don't expose any aliasing beyond normal borrow-checked &/&mut access).
unsafe impl Send for AlignedRam {}
unsafe impl Sync for AlignedRam {}

impl AlignedRam {
    const PAGE: usize = 4096;

    fn new(len: usize) -> Result<Self> {
        let layout = std::alloc::Layout::from_size_align(len, Self::PAGE)
            .map_err(|e| anyhow::anyhow!("Layout error: {e}"))?;
        // SAFETY: `layout` has non-zero size whenever `len > 0`, which is
        // always true for real guest RAM sizes; `alloc_zeroed` is being
        // called with a valid layout per its documented safety contract.
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        let ptr = std::ptr::NonNull::new(ptr)
            .ok_or_else(|| anyhow::anyhow!("Out of memory allocating {} MiB guest RAM", len >> 20))?;
        Ok(Self { ptr, len, layout })
    }
}

impl std::ops::Deref for AlignedRam {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        // SAFETY: `ptr` points to `len` initialized (zeroed on alloc) bytes
        // that we exclusively own for the lifetime of `self`.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

impl std::ops::DerefMut for AlignedRam {
    fn deref_mut(&mut self) -> &mut [u8] {
        // SAFETY: same as `deref`, with exclusive access via `&mut self`.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len) }
    }
}

impl Drop for AlignedRam {
    fn drop(&mut self) {
        // SAFETY: `self.layout` is exactly the layout `self.ptr` was
        // allocated with in `new` — the whole point of this type.
        unsafe { std::alloc::dealloc(self.ptr.as_ptr(), self.layout) }
    }
}

/// Allocate a 4 KiB-aligned buffer of exactly `size` bytes.
fn aligned_alloc(size: usize) -> Result<AlignedRam> {
    AlignedRam::new(size)
}
