//! SR-IOV direct display — Linux only.
//! Maps a VF's BAR0 framebuffer and exports it as a DMA-BUF.
//! See docs/dev-agent-notes.md §2 for the design rationale.

use anyhow::{bail, Context, Result};
use memmap2::{MmapOptions, MmapRaw};
use std::{fs::OpenOptions, os::unix::io::IntoRawFd, path::PathBuf};
use tracing::info;

const FB_WIDTH:  u32 = 1920;
const FB_HEIGHT: u32 = 1080;
const FB_STRIDE: u32 = FB_WIDTH * 4;

pub struct SriovDisplay {
    pub bar0_mmap: MmapRaw,
    pub dmabuf_fd: Option<i32>,
    pub width:  u32,
    pub height: u32,
}

impl SriovDisplay {
    pub fn open(vf_pci_addr: &str) -> Result<Self> {
        let resource0 = PathBuf::from(format!(
            "/sys/bus/pci/devices/{}/resource0",
            vf_pci_addr
        ));
        if !resource0.exists() {
            bail!("BAR0 resource not found: {}. Is SR-IOV enabled and the VF bound to vfio-pci?",
                  resource0.display());
        }
        let file = OpenOptions::new().read(true).write(true)
            .open(&resource0)
            .with_context(|| format!("Cannot open {}", resource0.display()))?;

        let bar0_size = (FB_STRIDE * FB_HEIGHT) as usize;
        // SAFETY: BAR0 is a hardware MMIO region; MAP_SHARED writes directly to VRAM.
        let mmap = unsafe {
            MmapOptions::new().len(bar0_size).map_raw(&file)
                .context("mmap BAR0 failed")?
        };
        info!("SR-IOV BAR0 mapped: {} bytes @ {:?} (VF {})", bar0_size, mmap.as_ptr(), vf_pci_addr);
        Ok(Self { bar0_mmap: mmap, dmabuf_fd: None, width: FB_WIDTH, height: FB_HEIGHT })
    }

    pub fn write_test_pattern(&mut self, r: u8, g: u8, b: u8) {
        let ptr = self.bar0_mmap.as_mut_ptr();
        for i in 0..(self.width * self.height) as usize {
            unsafe {
                ptr.add(i * 4).write(b);
                ptr.add(i * 4 + 1).write(g);
                ptr.add(i * 4 + 2).write(r);
                ptr.add(i * 4 + 3).write(0xff);
            }
        }
    }

    pub fn export_dmabuf(&mut self) -> Result<i32> {
        // Full VFIO_IOMMU_MAP_DMA path — Phase 2.2.
        let fd = std::fs::File::open("/dev/udmabuf")
            .context("/dev/udmabuf not available — load the udmabuf module")?
            .into_raw_fd();
        self.dmabuf_fd = Some(fd);
        Ok(fd)
    }

    pub fn send_dmabuf_fd(&self, sock_path: &str) -> Result<()> {
        use std::os::unix::{io::AsRawFd, net::UnixDatagram};
        let fd = self.dmabuf_fd.context("Call export_dmabuf() first")?;
        let sock = UnixDatagram::unbound()?;
        sock.connect(sock_path)?;
        send_fd_scm_rights(&sock, fd)?;
        info!("DMA-BUF fd {} sent to {}", fd, sock_path);
        Ok(())
    }
}

/// Attach SR-IOV display; called from Vm::run when sriov_vf is set.
pub fn attach_vf_display(vf_addr: &str) -> Result<()> {
    let mut display = SriovDisplay::open(vf_addr)?;
    display.write_test_pattern(0, 255, 0); // green = alive
    info!("SR-IOV display attached, test pattern written");
    Ok(())
}

fn send_fd_scm_rights(sock: &std::os::unix::net::UnixDatagram, fd: i32) -> Result<()> {
    use std::{io::IoSlice, os::unix::io::AsRawFd};
    let iov = [IoSlice::new(b"\0")];
    let cmsg = unsafe {
        let len = libc::CMSG_SPACE(std::mem::size_of::<i32>() as u32) as usize;
        let mut buf = vec![0u8; len];
        let hdr = buf.as_mut_ptr() as *mut libc::cmsghdr;
        (*hdr).cmsg_len    = libc::CMSG_LEN(std::mem::size_of::<i32>() as u32) as _;
        (*hdr).cmsg_level  = libc::SOL_SOCKET;
        (*hdr).cmsg_type   = libc::SCM_RIGHTS;
        *(libc::CMSG_DATA(hdr) as *mut i32) = fd;
        buf
    };
    let msg = libc::msghdr {
        msg_name: std::ptr::null_mut(), msg_namelen: 0,
        msg_iov: iov.as_ptr() as *mut _, msg_iovlen: iov.len() as _,
        msg_control: cmsg.as_ptr() as *mut _, msg_controllen: cmsg.len() as _,
        msg_flags: 0,
    };
    if unsafe { libc::sendmsg(sock.as_raw_fd(), &msg, 0) } < 0 {
        bail!("sendmsg: {}", std::io::Error::last_os_error());
    }
    Ok(())
}
