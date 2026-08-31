//! Guest memory flags and dirty-page bitmap.

use bitflags::bitflags;

bitflags! {
    /// Permission and tracking flags for a guest physical memory mapping.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct MemFlags: u32 {
        const READ        = 0b0001;
        const WRITE       = 0b0010;
        const EXECUTE     = 0b0100;
        /// Enable dirty-page tracking for this range (required for COW fork).
        const TRACK_DIRTY = 0b1000;

        /// Convenience: full RWX with dirty tracking (used for parent VM RAM).
        const RWX_TRACKED = Self::READ.bits()
                          | Self::WRITE.bits()
                          | Self::EXECUTE.bits()
                          | Self::TRACK_DIRTY.bits();

        /// Read-only with execute (used for COW child RAM pages).
        const RX_READONLY = Self::READ.bits() | Self::EXECUTE.bits();
    }
}

/// Dirty-page bitmap returned by `Hypervisor::query_dirty_bitmap`.
///
/// One bit per 4 KiB page; bit `n` is set if the page at
/// `base_gpa + n * 4096` was written since the last query.
/// The bitmap is always reset atomically when read.
pub struct DirtyBitmap {
    /// Base GPA of the range this bitmap covers.
    pub base_gpa: u64,
    /// Number of 4 KiB pages covered.
    pub page_count: u64,
    /// Packed bitmap — `ceil(page_count / 64)` words.
    pub words: Vec<u64>,
}

impl DirtyBitmap {
    /// Returns `true` if the page at index `page_idx` is dirty.
    #[inline]
    pub fn is_dirty(&self, page_idx: u64) -> bool {
        let word = (page_idx / 64) as usize;
        let bit  = page_idx % 64;
        self.words.get(word).map_or(false, |w| w & (1 << bit) != 0)
    }

    /// Iterator over the GPA of every dirty page.
    pub fn dirty_gpas(&self) -> impl Iterator<Item = u64> + '_ {
        (0..self.page_count).filter(move |&i| self.is_dirty(i)).map(move |i| {
            self.base_gpa + i * 4096
        })
    }
}
