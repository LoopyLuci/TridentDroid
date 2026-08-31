//! Fallback virtio-gpu + Venus Vulkan passthrough (Phase 2.3).

use anyhow::Result;

pub struct VirtioGpu;

impl VirtioGpu {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub fn process_command(&mut self, _cmd: &[u8]) -> Result<Vec<u8>> {
        todo!("Phase 2.3: Venus command dispatch via rutabaga_gfx")
    }
}
