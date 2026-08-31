//! Windows stub for SR-IOV direct display.
//! Phase 4 will implement this via DXGI shared surfaces and DX12 resource sharing.

use anyhow::Result;

pub fn attach_vf_display(_vf_addr: &str) -> Result<()> {
    anyhow::bail!(
        "SR-IOV direct display is not yet implemented on Windows. \
         Phase 4 will use DXGI shared surfaces. \
         Use --sriov-vf only on Linux for now."
    )
}
