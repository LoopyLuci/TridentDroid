# Implementation Plan

## Phase 0: Environment Setup (Day 1)

1. Install Ubuntu 24.04 LTS on the target machine.
2. Enable KVM and IOMMU (AMD Vi):
   ```bash
   sudo apt install qemu-kvm libvirt-daemon-system
   sudo sed -i 's/GRUB_CMDLINE_LINUX=""/GRUB_CMDLINE_LINUX="amd_iommu=on iommu=pt"/' /etc/default/grub
   sudo update-grub
   reboot
   ```
3. Verify IOMMU groups:
   ```bash
   ./iommu_viewer.sh | grep -i vga
   ```
4. Configure SR‑IOV on the RX 7900 XTX:
   ```bash
   echo 16 > /sys/class/drm/card0/device/sriov_numvfs
   # Verify with lspci | grep VGA
   ```
5. Install Rust (stable):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup component add rustfmt clippy
   ```
6. Clone this repository and `cd` into it.

## Phase 1: Core VMM & Boot (Week 1‑2)

### 1.1 Minimal KVM VMM
- Create a Rust binary `trident-vmm`.
- Use `kvm-ioctls` to create a VM, set up memory (single slot initially), and load a guest kernel (a custom minimal Linux kernel).
- Implement a simple serial console output.
- **Milestone**: `Hello from guest kernel` printed.

### 1.2 Memory & CPU Setup
- Allocate guest RAM via `vm-memory` crate.
- Configure vCPUs (start with 4).
- Map the guest's entire memory as a single slot with `KVM_MEM_LOG_DIRTY_PAGES` flag (required for forking later).

### 1.3 Boot Android GSI
- Prepare an Android 14 GSI (`system.img`, `vendor.img`) and a minimal kernel with Android configs.
- Add virtio‑block for system/vendor images and virtio‑net for network.
- Boot to Android home screen using `virtio-gpu` (software rendering initially).
- **Milestone**: `adb shell` works.

## Phase 2: GPU Acceleration (Week 2‑3)

### 2.1 SR‑IOV Direct Display
- Implement the VF BAR0 mapping as described in `CONTEXT.md`.
- In the VMM, detect if the instance is assigned a VF; if so, configure guest kernel command line `amdgpu.dc=0`.
- After guest boots, mmap BAR0 resource0.
- Use Android's `gralloc` HAL with a custom host‑side renderer that writes frames directly to the mmap'd framebuffer.
- **Sub‑step**: Create a simple test that writes a color pattern to the VF framebuffer and verify it appears on a physical monitor (or via read‑back).

### 2.2 Streaming via DMA‑BUF
- Export the mmap'd framebuffer as a DMA‑BUF fd using `export_dmabuf_from_pci_bar()`.
- Send that fd over a Unix socket to the gRPC streaming service.
- In the streaming service, map the DMA‑BUF and encode frames (use `libva` for hardware encoding).

### 2.3 Fallback: Virtio‑gpu + Venus
- Integrate `rutabaga_gfx` or `gfxstream` for virtio‑gpu support.
- Enable Vulkan passthrough via Venus protocol.

## Phase 3: Instance Forking & Snapshots (Week 3‑4)

### 3.1 Parent Preparation
- Boot a "golden" instance with a clean Android state.
- Implement the correct pause‑dirty‑fork sequence (code from the Dev Agent review).

### 3.2 COW Memory Forking
- Use `KVM_SET_USER_MEMORY_REGION2` with `KVM_MEM_READONLY` for child.
- Implement `KVM_EXIT_MMIO` handler to allocate new page on write fault.
- Track dirty pages per child.

### 3.3 Parallel Device Reset
- Reset all virtio devices concurrently using `futures::join_all`.
- Ensure vCPU states are reset.

### 3.4 Performance Validation
- Fork 12 instances and measure boot time; target <8s.
- Use `perf` to find bottlenecks.

## Phase 4: gRPC Control Plane & CI Integration (Week 4‑5)

### 4.1 Define Protocol
- Create `proto/tridentd.proto`:
  ```protobuf
  service TridentDaemon {
    rpc LaunchInstance(LaunchRequest) returns (InstanceInfo);
    rpc Snapshot(SnapshotRequest) returns (SnapshotResponse);
    rpc Fork(ForkRequest) returns (InstanceInfo);
    rpc AdbShell(stream AdbShellRequest) returns (stream AdbShellResponse);
    rpc StreamDisplay(DisplayStreamRequest) returns (stream DisplayFrame);
  }
  ```

### 4.2 Implement Server
- Use `tonic` with mTLS (code provided in `src/server.rs` template).
- Wire each RPC to the VMM's internal API.

### 4.3 CI Integration
- Provide a `tools/trident_ci.sh` script that:
  1. Starts the `tridentd` server.
  2. Runs CTS tests or custom tests.
  3. Tears down instances.

## Phase 5: Custom ROM Support & Polish (Week 5+)

- Implement booting from arbitrary kernel/ramdisk/command‑line.
- Handle Android versions without virtio (e.g., using goldfish devices as fallback).
- Add snapshot/restore of full system state.
- Implement virtual sensors (GPS, accelerometer) via virtio‑sensors.
- Security hardening (seccomp, namespaces, cgroups).

## Development Notes

- Always run `cargo clippy` and `cargo fmt` before commits.
- Use `unsafe` only in well‑documented modules for PCI BAR mapping and KVM ioctls.
- The VMM binary should be compiled with `RUSTFLAGS="-C target-cpu=native"` for maximum performance.
