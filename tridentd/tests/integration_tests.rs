//! Integration tests for the HAL-based TridentDroid implementation.
//!
//! These tests validate the cross-platform VMM stack in `tridentd_lib`.

#[cfg(target_os = "linux")]
mod linux_tests {
    use tridentd_lib::vmm::{Vm, VmConfig};

    /// Verify that VM creation succeeds up to kernel load on Linux/KVM.
    /// The kernel file doesn't exist, so we expect a kernel-load error —
    /// but memory allocation, KVM setup, and vCPU creation must succeed.
    #[test]
    fn test_vm_creation_reaches_kernel_load() {
        if !std::path::Path::new("/dev/kvm").exists() {
            eprintln!("SKIP: /dev/kvm not available");
            return;
        }

        let config = VmConfig {
            vcpu_count: 1,
            memory_mib: 64,
            kernel_path: "nonexistent_kernel".to_string(),
            initrd_path: None,
            cmdline: String::new(),
            sriov_vf: None,
        };

        let hyp = std::sync::Arc::new(
            tridentd_lib::platform::open_hypervisor().expect("Failed to open hypervisor"),
        );

        match Vm::create(&hyp, config) {
            Ok(_) => {
                // Unexpected but harmless
            }
            Err(e) => {
                let msg = format!("{:#}", e);
                assert!(
                    msg.contains("kernel") || msg.contains("Cannot open") || msg.contains("load"),
                    "Unexpected error (expected kernel-load failure): {}",
                    msg
                );
                eprintln!("Got expected kernel-load error: {}", msg);
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod sriov_tests {
    use tridentd_lib::gpu::sriov::SriovDisplay;

    /// SR-IOV BAR0 mmap integration test.
    /// Requires root and a VF bound to vfio-pci.
    /// Skipped automatically when VF_PCI_ADDR env var is not set.
    #[test]
    fn test_sriov_mmap() {
        let vf_addr = match std::env::var("VF_PCI_ADDR") {
            Ok(a) => a,
            Err(_) => {
                eprintln!("SKIP: set VF_PCI_ADDR=0000:03:00.1 to run this test");
                return;
            }
        };

        let mut display = SriovDisplay::open(&vf_addr)
            .expect("Failed to open VF BAR0 — is vfio-pci bound?");

        // Write a solid red pattern
        display.write_test_pattern(255, 0, 0);

        // Read back the first pixel to verify the mmap is live
        let ptr = display.bar0_mmap.as_ptr();
        let (b, g, r, a) = unsafe { (*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)) };
        assert_eq!((r, g, b, a), (255, 0, 0, 255), "Framebuffer read-back mismatch");

        eprintln!("SR-IOV BAR0 mmap OK — red pattern written to VF {}", vf_addr);
    }

    #[test]
    fn test_sriov_missing_vf_returns_error() {
        let result = SriovDisplay::open("0000:ff:ff.7");
        assert!(result.is_err(), "Expected error for non-existent VF");
    }
}

/// Cross-platform test: verify platform hypervisor opens successfully.
#[test]
fn test_platform_hypervisor_opens() {
    let result = tridentd_lib::platform::open_hypervisor();
    assert!(result.is_ok(), "Failed to open platform hypervisor: {:?}", result.err());
}
