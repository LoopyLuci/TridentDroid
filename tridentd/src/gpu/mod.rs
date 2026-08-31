pub mod virtio_gpu;

// SR-IOV direct display: Linux-only (uses /sys/bus/pci/ + DMA-BUF + SCM_RIGHTS).
// On Windows, direct display goes through a DXGI shared surface (Phase 4).
#[cfg(target_os = "linux")]
pub mod sriov;

#[cfg(windows)]
pub mod sriov_stub;
#[cfg(windows)]
pub use sriov_stub as sriov;
