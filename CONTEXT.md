# Context & Design Rationale

This document captures the entire design discussion and decisions that shaped TridentDroid. It serves as a permanent reference for the building agent.

## 1. Vision

The emulator must be **the** tool for Android testing on high‑end AMD hardware. It must replace the bloated Android Emulator with a Rust‑based, KVM‑accelerated, SR‑IOV‑enabled hypervisor that is so light it can run dozens of instances concurrently.

## 2. Key Technical Decisions

### Hypervisor
- **Rust VMM** using the `kvm-ioctls` + `vm-memory` crates.
- Leverage **KVM** on Linux for near‑bare‑metal CPU/memory performance.
- Guest kernel: a minimal Linux 6.6+ with only virtio, binder, ashmem, and necessary drivers. Shared kernel image across instances.

### GPU Acceleration
Two modes, selected at launch:
1. **SR‑IOV direct display** (default for VF‑capable instances):  
   The host maps the VF's BAR0 framebuffer directly and writes the Android UI into it, bypassing the guest GPU stack completely. This eliminates virtio‑gpu overhead.
2. **Virtio‑gpu + Venus** (fallback):  
   For environments without SR‑IOV, use Vulkan passthrough via Venus protocol over virtio‑gpu.

**Direct Display Implementation** (from the Emulator Dev Agent):
- Blacklist `amdgpu.dc=0` in the guest to leave the display controller idle.
- Host mmaps the VF's BAR0 resource0.
- Android's SurfaceFlinger output is rendered by the host into that mmap'd framebuffer.
- For streaming, export the BAR0 memory as a DMA‑BUF fd and share it with the gRPC streaming task via Unix socket.

### Instance Forking & Snapshotting
- Based on KVM dirty log and `KVM_MEM_READONLY` (copy‑on‑write).
- **Critical sequence** (validated by the Dev Agent):
  1. Pause **all** vCPUs of the parent.
  2. Call `KVM_KVMCLOCK_CTRL` to fully quiesce.
  3. `sync_dirty_log()` then `get_dirty_log()` to capture all writes.
  4. Fork: create child VM with same memory, mark all slots `KVM_MEM_READONLY`.
  5. On child write fault, allocate new page, copy parent page, update child's EPT.
  6. Re‑enable `MANUAL_DIRTY_LOG_PROTECT2` on parent.
- Device state reset must be parallelized and ordered (virtio queues first, then clear MSI‑X).

### Networking
- User‑mode networking with `slirp‑ng` for simplicity, or macvtap for performance.
- Virtual sensors (GPS, accelerometer) via virtio‑sensors.

### API & Automation
- **gRPC** service using `tonic`, with **mTLS** authentication (client and server certificates).
- Alternative: Unix domain socket with peer credentials for local CI.
- Full ADB passthrough over TCP.

### Performance Targets
- Cold boot: <2s
- Snapshot restore: <200ms
- Fork 12 instances: boot all 12 in under 8s (see TODO)
- 60 FPS UI streaming with <3% host CPU overhead

## 3. Technology Stack

### Workspace structure (Cargo workspace with 4 crates)
```
trident-hal/        ← platform-agnostic Hypervisor trait, VcpuExit, Regs, MemFlags
trident-hal-kvm/    ← Linux backend: kvm-ioctls, vm-memory (Linux only)
trident-hal-whp/    ← Windows backend: windows::Win32::System::Hypervisor (Windows only)
tridentd/           ← VMM binary + device models + gRPC server (cross-platform)
```

### Platform backend selection
- `tridentd/src/platform.rs` — compile-time selection via `cfg(windows)` / `cfg(target_os = "linux")`
- `PlatformHypervisor` type alias resolves to `WhpHypervisor` or `KvmHypervisor`
- **Windows**: WHP via `windows` crate — no kernel driver, coexists with Hyper-V/WSL2/Docker
- **Linux**: KVM via `kvm-ioctls` — battle-tested, no custom module

### Other crates
- gRPC: `tonic`, `prost`, `rustls`
- Async runtime: `tokio`
- Certificates: OpenSSL (one‑time generation via `tools/gen_certs.sh`)
- GPU (Linux): direct PCI BAR mapping with `memmap2` + DMA‑BUF export via `libc`
- GPU (Windows): DXGI shared surface — Phase 4

## 4. System Image Support
- Generic System Images (GSIs) via `system.img` + `vendor.img`
- Boot custom kernels using `-kernel`, `-initrd`, `-append` (emulated by VMM)
- AVDs: on‑the‑fly conversion using `android-emulator‑hypervisor‑driver` logic

## 5. References
- The original enhanced prompt (previous handover)
- Emulator Dev Agent's review of fork_instance, SR‑IOV display, and mTLS (reproduced in `docs/dev-agent-notes.md`)
