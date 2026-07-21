/// mmap wrapper
pub(crate) struct MmapRegion {
    mmap: memmap2::MmapMut,
    valid_len: usize,
}

impl MmapRegion {
    pub(crate) fn new(mmap: memmap2::MmapMut, valid_len: usize) -> Self {
        Self { mmap, valid_len }
    }

    pub(crate) fn as_ptr(&self) -> *const u8 {
        self.mmap.as_ptr()
    }

    pub(crate) fn valid_len(&self) -> usize {
        self.valid_len
    }
}
