//! Kernel loader: writes a bzImage + cmdline + initrd into guest RAM.
//! Platform-agnostic — operates on a raw `&[u8]` slice of guest memory.

use anyhow::{Context, Result};
use tracing::info;

pub struct KernelLoader;

impl KernelLoader {
    /// Load a bzImage kernel, optional initrd, and cmdline into `ram`.
    /// Returns the kernel entry-point GPA.
    pub fn load(
        ram: &mut [u8],
        kernel_path: &str,
        initrd_path: Option<&str>,
        cmdline: &str,
    ) -> Result<u64> {
        // `linux-loader` is a Linux-only crate (it depends on vm-memory which
        // uses Unix mmap internally).  On Windows we use a direct PE/ELF parser.
        #[cfg(target_os = "linux")]
        return Self::load_linux(ram, kernel_path, initrd_path, cmdline);

        #[cfg(windows)]
        return Self::load_windows(ram, kernel_path, initrd_path, cmdline);
    }

    #[cfg(target_os = "linux")]
    fn load_linux(
        ram: &mut [u8],
        kernel_path: &str,
        initrd_path: Option<&str>,
        cmdline: &str,
    ) -> Result<u64> {
        use linux_loader::{
            cmdline::Cmdline,
            loader::{self, bzimage::BzImage, KernelLoader as _},
        };
        use vm_memory::{GuestAddress, GuestMemoryMmap};

        // Wrap the caller's raw slice in a GuestMemoryMmap view.
        // SAFETY: `ram` is a valid, aligned allocation that outlives this call.
        let guest_mem = unsafe {
            GuestMemoryMmap::<()>::from_raw_regions_unguarded(
                &[(GuestAddress(0), ram.as_ptr() as usize, ram.len())],
            )
            .context("Failed to create GuestMemoryMmap view")?
        };

        let mut kernel_file = std::fs::File::open(kernel_path)
            .with_context(|| format!("Cannot open kernel: {}", kernel_path))?;

        let result = BzImage::load(
            &guest_mem,
            None,
            &mut kernel_file,
            Some(GuestAddress(0x0100_0000)),
        )
        .context("bzImage load failed")?;

        let mut cmd = Cmdline::new(4096);
        cmd.insert_str(cmdline).context("cmdline insert failed")?;
        loader::load_cmdline(&guest_mem, GuestAddress(0x0002_0000), &cmd)
            .context("Failed to write cmdline")?;

        if let Some(path) = initrd_path {
            let mut buf = Vec::new();
            std::fs::File::open(path)
                .with_context(|| format!("Cannot open initrd: {}", path))?
                .read_to_end(&mut buf)
                .context("initrd read failed")?;
            let addr = (ram.len() as u64 - buf.len() as u64) & !0xFFF;
            guest_mem
                .write_slice(&buf, GuestAddress(addr))
                .context("Failed to write initrd")?;
            info!("initrd: {} bytes @ GPA {:#x}", buf.len(), addr);
        }

        Ok(result.kernel_load.0)
    }

    #[cfg(windows)]
    fn load_windows(
        ram: &mut [u8],
        kernel_path: &str,
        initrd_path: Option<&str>,
        cmdline: &str,
    ) -> Result<u64> {
        // On Windows we parse the bzImage header directly (no linux-loader).
        // The bzImage boot protocol is documented in linux/Documentation/x86/boot.rst.
        use std::io::Read;

        let mut data = Vec::new();
        std::fs::File::open(kernel_path)
            .with_context(|| format!("Cannot open kernel: {}", kernel_path))?
            .read_to_end(&mut data)?;

        // bzImage magic at offset 0x1FE = 0xAA55 (boot sector), 0x202 = "HdrS"
        anyhow::ensure!(
            data.len() > 0x210,
            "File too small to be a bzImage: {}",
            kernel_path
        );
        anyhow::ensure!(
            &data[0x202..0x206] == b"HdrS",
            "Not a bzImage (missing HdrS magic): {}",
            kernel_path
        );

        // Read setup_sects (offset 0x1F1); default 4 if 0.
        let setup_sects = if data[0x1F1] == 0 { 4u64 } else { data[0x1F1] as u64 };
        let kernel_offset = (setup_sects + 1) * 512;

        // Load address for the protected-mode kernel: 0x100000 (1 MiB).
        const LOAD_ADDR: u64 = 0x0010_0000;
        let kernel_data = &data[kernel_offset as usize..];

        anyhow::ensure!(
            LOAD_ADDR as usize + kernel_data.len() <= ram.len(),
            "Kernel too large for guest RAM"
        );

        ram[LOAD_ADDR as usize..LOAD_ADDR as usize + kernel_data.len()]
            .copy_from_slice(kernel_data);
        info!("Kernel loaded: {} bytes @ GPA {:#x}", kernel_data.len(), LOAD_ADDR);

        // ── Write boot_params (zero-page) at 0x10000 ─────────────────────────
        //
        // The boot_params struct is defined in arch/x86/include/uapi/asm/bootparam.h.
        // The setup_header portion starts at byte 0x1F1 of the bzImage and occupies
        // bytes 0x0000..0x0202 of boot_params. Everything before 0x1F1 in boot_params
        // is separate fields (screen_info, apm_bios_info, etc.) that we zero-fill.
        const ZP: usize = 0x0001_0000; // zero-page base GPA

        // Zero the entire boot_params region first (4096 bytes is the minimum).
        let zero_page_len = 4096usize;
        for b in &mut ram[ZP..ZP + zero_page_len] {
            *b = 0;
        }

        // Copy the setup header (from offset 0x1F1 to the end of the setup sector).
        // The setup sector is setup_sects + 1 sectors (the +1 is the boot sector).
        let setup_hdr_start = 0x1F1usize;
        let setup_sectors = (setup_sects + 1) as usize;
        let setup_hdr_end = std::cmp::min(setup_sectors * 512, data.len());
        let hdr_copy_len = setup_hdr_end - setup_hdr_start;
        ram[ZP + setup_hdr_start..ZP + setup_hdr_start + hdr_copy_len]
            .copy_from_slice(&data[setup_hdr_start..setup_hdr_end]);

        // ── Patch required fields in setup_header ────────────────────────────
        //
        // These fields are documented in Documentation/x86/boot.rst.
        //
        // Offset 0x210: type_of_loader (u8) — bootloader ID. 0xFF = undefined.
        // Offset 0x211: loadflags (u8) — bit 0 = LOADED_HIGH (kernel at >=1MiB).
        // Offset 0x214: code32_start (u32) — physical start address of the 32-bit kernel.
        //               The decompressor jumps here. MUST be set to LOAD_ADDR.
        // Offset 0x224: cmd_line_ptr (u32) — 32-bit physical address of the cmdline.
        // Offset 0x230: kernel_alignment (u32) — physical address alignment for relocatable kernel.
        // Offset 0x238: cmdline_size (u32) — maximum size of the cmdline.
        // Offset 0x258: pref_address (u64) — preferred load address for relocatable kernel.
        ram[ZP + 0x210] = 0xFF; // type_of_loader: undefined
        ram[ZP + 0x211] |= 0x01; // loadflags: LOADED_HIGH

        // code32_start — critical for the kernel to know where to decompress/jump.
        let code32_start: u32 = LOAD_ADDR as u32;
        ram[ZP + 0x214..ZP + 0x218].copy_from_slice(&code32_start.to_le_bytes());

        // cmd_line_ptr — 32-bit physical address of cmdline.
        let cmd_ptr: u32 = 0x0002_0000;
        ram[ZP + 0x224..ZP + 0x228].copy_from_slice(&cmd_ptr.to_le_bytes());

        // kernel_alignment — for relocatable kernels (protocol >= 2.10).
        let kernel_alignment: u32 = 0x0010_0000; // 1 MiB alignment
        ram[ZP + 0x230..ZP + 0x234].copy_from_slice(&kernel_alignment.to_le_bytes());

        // cmdline_size — maximum size the kernel will accept.
        let cmdline_size: u32 = 4096;
        ram[ZP + 0x238..ZP + 0x23C].copy_from_slice(&cmdline_size.to_le_bytes());

        // pref_address — preferred load address (for KASLR-disabled or as hint).
        let pref_addr: u64 = LOAD_ADDR;
        ram[ZP + 0x258..ZP + 0x260].copy_from_slice(&pref_addr.to_le_bytes());

        // ── Write cmdline at 0x20000 ─────────────────────────────────────────
        let cmdline_bytes = cmdline.as_bytes();
        anyhow::ensure!(cmdline_bytes.len() < 4096, "Cmdline too long");
        ram[0x0002_0000..0x0002_0000 + cmdline_bytes.len()].copy_from_slice(cmdline_bytes);
        ram[0x0002_0000 + cmdline_bytes.len()] = 0; // NUL terminator

        // ── Write e820 memory map ───────────────────────────────────────────
        //
        // The e820 map describes physical memory to the kernel.
        // Entry format: addr(u64) + size(u64) + type(u32) = 20 bytes each.
        // E820_RAM = 1, E820_RESERVED = 2.
        // We skip the VGA hole (0xA0000..0x100000) and the zero-page area.
        let mem_size = ram.len() as u64;
        let e820: &[(u64, u64, u32)] = &[
            (0x0000_0000, 0x0009_F000, 1),                    // conventional RAM (636 KiB)
            (0x0010_0000, mem_size - 0x0010_0000, 1),          // extended RAM (1 MiB..top)
        ];
        ram[ZP + 0x1E8] = e820.len() as u8;
        for (i, &(addr, size, typ)) in e820.iter().enumerate() {
            let off = ZP + 0x2D0 + i * 20;
            ram[off..off + 8].copy_from_slice(&addr.to_le_bytes());
            ram[off + 8..off + 16].copy_from_slice(&size.to_le_bytes());
            ram[off + 16..off + 20].copy_from_slice(&typ.to_le_bytes());
        }

        // ── Write initrd if provided ────────────────────────────────────────
        if let Some(path) = initrd_path {
            let mut buf = Vec::new();
            std::fs::File::open(path)
                .with_context(|| format!("Cannot open initrd: {}", path))?
                .read_to_end(&mut buf)?;
            let addr = (ram.len() as u64 - buf.len() as u64) & !0xFFF;
            ram[addr as usize..addr as usize + buf.len()].copy_from_slice(&buf);
            info!("initrd: {} bytes @ GPA {:#x}", buf.len(), addr);

            // Set initrd_addr_max in boot_params (offset 0x22C, u32).
            let initrd_max: u32 = addr as u32;
            ram[ZP + 0x22C..ZP + 0x230].copy_from_slice(&initrd_max.to_le_bytes());
        }

        // ── Write ACPI RSDP (Required for modern kernel boot) ───────────────
        //
        // The ACPI Root System Description Pointer is placed in the EBDA
        // (Extended BIOS Data Area) or BIOS ROM region. We place it at 0x7E000.
        // The RSDP is 20 bytes (v1.0) or 36 bytes (v2.0+).
        Self::write_acpi_rsdp(ram, 0x0007_E000, mem_size)?;

        // ── Write SMBIOS entry point ────────────────────────────────────────
        //
        // Required for some kernel subsystems (DMI, etc.).
        // We place it in the BIOS ROM region (0xF0000..0xFFFFF).
        Self::write_smbios_entry(ram, 0x000F_0000)?;

        Ok(LOAD_ADDR)
    }

    /// Write the ACPI Root System Description Pointer (RSDP) at `addr`.
    ///
    /// The RSDP v2.0 is 36 bytes:
    /// - 8 bytes: "RSD PTR " signature
    /// - 1 byte: checksum
    /// - 6 bytes: OEMID
    /// - 1 byte: revision (2)
    /// - 4 bytes: RSDT physical address
    /// - 4 bytes: length (36)
    /// - 8 bytes: XSDT physical address
    /// - 1 byte: extended checksum
    /// - 3 bytes: reserved
    fn write_acpi_rsdp(ram: &mut [u8], addr: u64, _mem_size: u64) -> Result<()> {
        const RSDT_ADDR: u32 = 0x0007_C000; // RSDT just before RSDP

        // Placeholder for the full RSDP structure.
        let mut rsdp = [0u8; 36];
        rsdp[0..8].copy_from_slice(b"RSD PTR ");
        rsdp[9..15].copy_from_slice(b"TRIENT"); // OEMID
        rsdp[15] = 2; // revision = 2.0
        rsdp[16..20].copy_from_slice(&RSDT_ADDR.to_le_bytes()); // rsdt_addr
        rsdp[20..24].copy_from_slice(&36u32.to_le_bytes()); // length
        rsdp[24..32].copy_from_slice(&(RSDT_ADDR as u64).to_le_bytes()); // xsdt_addr (use RSDT for both)

        // Compute checksum for the first 20 bytes (v1.0 part).
        let sum20: u8 = rsdp[..20].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        rsdp[8] = 0u8.wrapping_sub(sum20); // checksum byte

        // Compute extended checksum for all 36 bytes.
        let sum36: u8 = rsdp.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        rsdp[32] = 0u8.wrapping_sub(sum36); // extended checksum

        let addr = addr as usize;
        anyhow::ensure!(
            addr + rsdp.len() <= ram.len(),
            "RSDP address out of bounds"
        );
        ram[addr..addr + rsdp.len()].copy_from_slice(&rsdp);

        // Write the RSDT (Root System Description Table).
        // RSDT header: "RSDT" (4) + length (4) + revision (1) + checksum (1) + OEMID (6) + OEMTableID (8) + OEMRevision (4) + CreatorID (4) + CreatorRevision (4) = 36 bytes.
        // Then pointers to other tables (FADT, MADT, etc.).
        const RSDT_HEADER_SIZE: usize = 36;
        let rsdt_len = RSDT_HEADER_SIZE as u32 + 4 * 2; // 2 entries: FADT + MADT
        let mut rsdt = vec![0u8; rsdt_len as usize];
        rsdt[0..4].copy_from_slice(b"RSDT");
        rsdt[4..8].copy_from_slice(&rsdt_len.to_le_bytes());
        rsdt[8] = 1; // revision

        // FADT pointer at offset 36 (first entry).
        let fadt_addr: u32 = 0x0007_B000;
        rsdt[RSDT_HEADER_SIZE..RSDT_HEADER_SIZE + 4].copy_from_slice(&fadt_addr.to_le_bytes());

        // MADT pointer at offset 40 (second entry).
        let madt_addr: u32 = 0x0007_A000;
        rsdt[RSDT_HEADER_SIZE + 4..RSDT_HEADER_SIZE + 8].copy_from_slice(&madt_addr.to_le_bytes());

        // RSDT checksum.
        let rsdt_sum: u8 = rsdt.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        rsdt[9] = 0u8.wrapping_sub(rsdt_sum);

        let rsdt_addr = RSDT_ADDR as usize;
        anyhow::ensure!(
            rsdt_addr + rsdt.len() <= ram.len(),
            "RSDT address out of bounds"
        );
        ram[rsdt_addr..rsdt_addr + rsdt.len()].copy_from_slice(&rsdt);

        // Write FADT (Fixed ACPI Description Table).
        // FADT header (40 bytes) + registers + Dsdt + XDsdt + PM profiles...
        // Minimum: 116 bytes for ACPI 5.0.
        let fadt_len = 148u32;
        let mut fadt = vec![0u8; fadt_len as usize];
        fadt[0..4].copy_from_slice(b"FACP");
        fadt[4..8].copy_from_slice(&fadt_len.to_le_bytes());
        fadt[8] = 6; // revision (ACPI 6.0)

        // SCI_INT at offset 46 (u16) — System Control Interrupt.
        fadt[46..48].copy_from_slice(&9u16.to_le_bytes()); // IRQ 9

        // PM1a_EVT_BLK at offset 64 (u32) — Power Management 1a Event Block.
        // Using a virtual address that won't conflict with anything.
        fadt[64..68].copy_from_slice(&0x1000u32.to_le_bytes());

        // PM1a_CNT_BLK at offset 72 (u32) — Power Management 1a Control Block.
        fadt[72..76].copy_from_slice(&0x1002u32.to_le_bytes());

        // PM2_CNT_BLK at offset 80 (u32).
        fadt[80..84].copy_from_slice(&0x1004u32.to_le_bytes());

        // PM_TMR_BLK at offset 88 (u32).
        fadt[88..92].copy_from_slice(&0x1008u32.to_le_bytes());

        // SMI_CMD at offset 48 (u32) — SCI system management interrupt command.
        fadt[48..52].copy_from_slice(&0u32.to_le_bytes());

        // ACPI_ENABLE at offset 52 (u8).
        fadt[52] = 0xA0;

        // ACPI_DISABLE at offset 53 (u8).
        fadt[53] = 0xA1;

        // FADT checksum.
        let fadt_sum: u8 = fadt.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        fadt[9] = 0u8.wrapping_sub(fadt_sum);

        let fadt_addr = fadt_addr as usize;
        anyhow::ensure!(
            fadt_addr + fadt.len() <= ram.len(),
            "FADT address out of bounds"
        );
        ram[fadt_addr..fadt_addr + fadt.len()].copy_from_slice(&fadt);

        // Write MADT (Multiple APIC Description Table).
        // MADT header (44 bytes) + APIC structures.
        // We provide a single Local APIC structure for CPU 0.
        let madt_len = 44u32 + 8 + 12; // header + local_apic + io_apic
        let mut madt = vec![0u8; madt_len as usize];
        madt[0..4].copy_from_slice(b"APIC");
        madt[4..8].copy_from_slice(&madt_len.to_le_bytes());
        madt[8] = 6; // revision

        // Local APIC address at offset 36 (u32).
        madt[36..40].copy_from_slice(&0xFEE0_0000u32.to_le_bytes());

        // Flags at offset 40 (u32) — bit 0 = PC-AT compatible.
        madt[40..44].copy_from_slice(&1u32.to_le_bytes());

        // Local APIC structure (type 0) at offset 44.
        madt[44] = 0; // type: Processor Local APIC
        madt[45] = 8; // length
        madt[46] = 0; // ACPI processor ID
        madt[47] = 0; // APIC ID
        madt[48..52].copy_from_slice(&1u32.to_le_bytes()); // flags: enabled

        // I/O APIC structure (type 1) at offset 52.
        madt[52] = 1; // type: I/O APIC
        madt[53] = 12; // length
        madt[54] = 0; // I/O APIC ID
        madt[55] = 0; // reserved
        madt[56..60].copy_from_slice(&0xFEC0_0000u32.to_le_bytes()); // I/O APIC address
        madt[60..64].copy_from_slice(&0u32.to_le_bytes()); // global system interrupt base

        // MADT checksum.
        let madt_sum: u8 = madt.iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        madt[9] = 0u8.wrapping_sub(madt_sum);

        let madt_addr = madt_addr as usize;
        anyhow::ensure!(
            madt_addr + madt.len() <= ram.len(),
            "MADT address out of bounds"
        );
        ram[madt_addr..madt_addr + madt.len()].copy_from_slice(&madt);

        info!(
            "ACPI tables written: RSDP@{:#x}, RSDT@{:#x}, FADT@{:#x}, MADT@{:#x}",
            addr, RSDT_ADDR, fadt_addr, madt_addr
        );
        Ok(())
    }

    /// Write the SMBIOS 3.0 entry point at `addr`.
    ///
    /// The SMBIOS entry point is a 32-byte structure that tells the kernel
    /// where to find the SMBIOS tables. We place it in the BIOS ROM region.
    fn write_smbios_entry(ram: &mut [u8], addr: u64) -> Result<()> {
        // SMBIOS 3.0 entry point structure (32 bytes).
        let mut entry = [0u8; 32];
        entry[0..5].copy_from_slice(b"_SM3_"); // anchor string
        entry[5] = 0x00; // entry point revision (3.0)
        entry[6] = 32; // entry point length
        entry[10..12].copy_from_slice(&0x0100u16.to_le_bytes()); // SMBIOS version (1.0.0)
        entry[12] = 0; // maximum structure size (0 = no limit)
        entry[13] = 0; // maximum structure size high byte

        // Inter anchor string "_DMI_" at offset 16.
        entry[16..21].copy_from_slice(b"_DMI_");

        // Compute checksum for bytes 0..15.
        let sum: u8 = entry[..16].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        entry[15] = 0u8.wrapping_sub(sum);

        // Compute extended checksum for bytes 16..31.
        let ext_sum: u8 = entry[16..].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
        entry[31] = 0u8.wrapping_sub(ext_sum);

        let addr = addr as usize;
        anyhow::ensure!(
            addr + entry.len() <= ram.len(),
            "SMBIOS entry point address out of bounds"
        );
        ram[addr..addr + entry.len()].copy_from_slice(&entry);

        info!("SMBIOS 3.0 entry point written @ GPA {:#x}", addr);
        Ok(())
    }
}
