#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    sync::Arc,
    vec::Vec,
};
#[cfg(feature = "std")]
use std::{
    sync::Arc,
    vec::Vec,
};

use crate::{
    Hash,
    TreeError,
};

/// Number of hashes per chunk for structural sharing.
pub(crate) const CHUNK_SIZE: usize = 128;

/// Number of chunks per immutable segment
pub(crate) const CHUNKS_PER_SEGMENT: usize = 256;

#[cfg(not(feature = "storage"))]
#[derive(Clone)]
pub(crate) struct Chunk(Arc<[Hash; CHUNK_SIZE]>);

#[cfg(not(feature = "storage"))]
impl Chunk {
    #[inline(always)]
    pub(crate) fn as_slice(&self) -> &[Hash; CHUNK_SIZE] {
        &self.0
    }

    #[inline(always)]
    pub(crate) fn make_mut(&mut self) -> &mut [Hash; CHUNK_SIZE] {
        Arc::make_mut(&mut self.0)
    }

    #[inline]
    pub(crate) fn new_memory(data: [Hash; CHUNK_SIZE]) -> Self {
        Self(Arc::new(data))
    }

    #[cfg(test)]
    pub(crate) fn ptr_eq(a: &Self, b: &Self) -> bool {
        Arc::ptr_eq(&a.0, &b.0)
    }
}

#[cfg(feature = "storage")]
#[derive(Clone)]
pub(crate) struct Chunk(ChunkInner);

#[cfg(feature = "storage")]
#[derive(Clone)]
enum ChunkInner {
    Memory(Arc<[Hash; CHUNK_SIZE]>),
    Mapped {
        region: Arc<crate::storage::data::MmapRegion>,
        offset: usize,
    },
}

#[cfg(feature = "storage")]
impl Chunk {
    #[inline(always)]
    pub(crate) fn as_slice(&self) -> &[Hash; CHUNK_SIZE] {
        match &self.0 {
            ChunkInner::Memory(arc) => arc,
            ChunkInner::Mapped { region, offset } => {
                // SAFETY: offset validated at construction
                unsafe { &*(region.as_ptr().add(*offset).cast::<[Hash; CHUNK_SIZE]>()) }
            }
        }
    }

    #[inline(always)]
    pub(crate) fn make_mut(&mut self) -> &mut [Hash; CHUNK_SIZE] {
        if matches!(&self.0, ChunkInner::Mapped { .. }) {
            let data = *self.as_slice();
            self.0 = ChunkInner::Memory(Arc::new(data));
        }
        match &mut self.0 {
            ChunkInner::Memory(arc) => Arc::make_mut(arc),
            ChunkInner::Mapped { .. } => unreachable!(),
        }
    }

    #[inline]
    pub(crate) fn new_memory(data: [Hash; CHUNK_SIZE]) -> Self {
        Self(ChunkInner::Memory(Arc::new(data)))
    }

    pub(crate) fn new_mapped(
        region: Arc<crate::storage::data::MmapRegion>,
        offset: usize,
    ) -> Self {
        const CHUNK_BYTE_SIZE: usize = CHUNK_SIZE * 32;
        assert!(
            offset + CHUNK_BYTE_SIZE <= region.valid_len(),
            "Chunk::new_mapped: offset {offset} + {CHUNK_BYTE_SIZE} exceeds valid_len {}",
            region.valid_len()
        );
        Self(ChunkInner::Mapped { region, offset })
    }

    #[cfg(test)]
    pub(crate) fn ptr_eq(a: &Self, b: &Self) -> bool {
        match (&a.0, &b.0) {
            (ChunkInner::Memory(a), ChunkInner::Memory(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// A single level of the tree stored as segmented chunks plus a
/// fixed-size tail buffer.
#[derive(Clone)]
pub(crate) struct ChunkedLevel {
    /// Immutable segments of committed chunks, shared with snapshots.
    segments: Vec<Arc<[Chunk; CHUNKS_PER_SEGMENT]>>,
    /// Mutable buffer of committed chunks not yet frozen into a segment.
    /// At most `CHUNKS_PER_SEGMENT - 1` items.
    pending: Vec<Chunk>,
    /// Fixed-size tail buffer (partially filled).
    tail: [Hash; CHUNK_SIZE],
    /// Number of valid entries in `tail`.
    tail_len: usize,
    /// Total number of hashes in this level.
    len: usize,
}

impl ChunkedLevel {
    pub(crate) fn new() -> Self {
        Self {
            segments: Vec::new(),
            pending: Vec::new(),
            tail: [[0u8; 32]; CHUNK_SIZE],
            tail_len: 0,
            len: 0,
        }
    }

    /// Construct a level from checkpoint data, partitioning chunks into
    /// segments and pending.
    #[cfg(feature = "storage")]
    pub(crate) fn from_parts(
        chunks: Vec<Chunk>,
        tail: [Hash; CHUNK_SIZE],
        tail_len: usize,
        len: usize,
    ) -> Self {
        let full_segments = chunks.len() / CHUNKS_PER_SEGMENT;
        let mut segments = Vec::with_capacity(full_segments);
        let mut drain = chunks.into_iter();
        for _ in 0..full_segments {
            let seg: Vec<Chunk> = drain.by_ref().take(CHUNKS_PER_SEGMENT).collect();
            let boxed: Box<[Chunk; CHUNKS_PER_SEGMENT]> = seg
                .into_boxed_slice()
                .try_into()
                .unwrap_or_else(|_| unreachable!());
            segments.push(Arc::from(boxed));
        }
        let pending: Vec<Chunk> = drain.collect();

        Self {
            segments,
            pending,
            tail,
            tail_len,
            len,
        }
    }

    /// Total number of hashes in this level.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Total number of committed chunks (segments + pending).
    #[inline]
    pub(crate) fn chunk_count(&self) -> usize {
        self.segments.len() * CHUNKS_PER_SEGMENT + self.pending.len()
    }

    /// Resolve a chunk index to a slice reference.
    #[inline(always)]
    fn chunk_slice(&self, chunk_idx: usize) -> &[Hash; CHUNK_SIZE] {
        let committed = self.segments.len() * CHUNKS_PER_SEGMENT;
        if chunk_idx < committed {
            let seg_idx = chunk_idx / CHUNKS_PER_SEGMENT;
            let seg_off = chunk_idx % CHUNKS_PER_SEGMENT;
            self.segments[seg_idx][seg_off].as_slice()
        } else {
            self.pending[chunk_idx - committed].as_slice()
        }
    }

    /// Read a hash at the given index.
    #[inline]
    pub(crate) fn get(&self, index: usize) -> Result<Hash, TreeError> {
        if index >= self.len {
            return Err(TreeError::IndexOutOfRange {
                index: index as u64,
                size: self.len as u64,
            });
        }
        let chunk_idx = index / CHUNK_SIZE;
        let offset = index % CHUNK_SIZE;
        if chunk_idx < self.chunk_count() {
            Ok(self.chunk_slice(chunk_idx)[offset])
        } else {
            Ok(self.tail[offset])
        }
    }

    /// Copy a contiguous group of hashes into `out`.
    /// Fast path when the group falls within a single chunk or tail.
    #[inline(always)]
    pub(crate) fn get_group(&self, start: usize, count: usize, out: &mut [Hash]) {
        let chunk_idx = start / CHUNK_SIZE;
        let offset = start % CHUNK_SIZE;
        if offset + count <= CHUNK_SIZE {
            let src = if chunk_idx < self.chunk_count() {
                &self.chunk_slice(chunk_idx)[offset..offset + count]
            } else {
                &self.tail[offset..offset + count]
            };
            out[..count].copy_from_slice(src);
        } else {
            for (i, item) in out.iter_mut().enumerate().take(count) {
                *item = self.get(start + i).expect("checked prev; qed");
            }
        }
    }

    /// Borrow a contiguous group of `count` hashes starting at `start`,
    /// when it lies wholly within a single chunk or the tail.
    ///
    /// Returns `None` if the group straddles a chunk/tail boundary, in which
    /// case the caller must fall back to a copying read. For the batched
    /// parent path, full groups of arity N in {2,4,8,16} are chunk-aligned
    /// (`CHUNK_SIZE % N == 0`), so this always returns `Some`.
    #[inline]
    pub(crate) fn group_slice(&self, start: usize, count: usize) -> Option<&[Hash]> {
        if start + count > self.len {
            return None;
        }
        let chunk_idx = start / CHUNK_SIZE;
        let offset = start % CHUNK_SIZE;
        if offset + count > CHUNK_SIZE {
            return None;
        }
        if chunk_idx < self.chunk_count() {
            Some(&self.chunk_slice(chunk_idx)[offset..offset + count])
        } else {
            Some(&self.tail[offset..offset + count])
        }
    }

    /// Write a hash at the given index
    #[inline]
    pub(crate) fn set(&mut self, index: usize, value: Hash) -> Result<(), TreeError> {
        if self.len <= index {
            self.ensure_len(index + 1)?;
        }
        self.set_preallocated(index, value);
        Ok(())
    }

    /// Caller must ensure `index < self.len`
    #[inline(always)]
    pub(crate) fn set_preallocated(&mut self, index: usize, value: Hash) {
        debug_assert!(
            index < self.len,
            "set_preallocated: index {index} >= len {}",
            self.len
        );
        let chunk_idx = index / CHUNK_SIZE;
        let offset = index % CHUNK_SIZE;
        let committed = self.segments.len() * CHUNKS_PER_SEGMENT;
        if chunk_idx < committed {
            let seg_idx = chunk_idx / CHUNKS_PER_SEGMENT;
            let seg_off = chunk_idx % CHUNKS_PER_SEGMENT;
            Arc::make_mut(&mut self.segments[seg_idx])[seg_off].make_mut()[offset] =
                value;
        } else if chunk_idx - committed < self.pending.len() {
            self.pending[chunk_idx - committed].make_mut()[offset] = value;
        } else {
            self.tail[offset] = value;
        }
    }

    /// Append a hash. Promotes the tail when it reaches
    /// `CHUNK_SIZE`.
    #[cfg(test)]
    #[inline]
    pub(crate) fn push(&mut self, value: Hash) -> Result<(), TreeError> {
        self.tail[self.tail_len] = value;
        self.tail_len = self.tail_len.checked_add(1).ok_or(TreeError::MathError)?;
        self.len = self.len.checked_add(1).ok_or(TreeError::MathError)?;
        if self.tail_len == CHUNK_SIZE {
            self.promote_tail();
        }
        Ok(())
    }

    pub(crate) fn extend(&mut self, values: &[Hash]) -> Result<(), TreeError> {
        if values.is_empty() {
            return Ok(());
        }
        let new_len = self
            .len
            .checked_add(values.len())
            .ok_or(TreeError::MathError)?;

        let mut remaining = values;

        // fill current tail
        if self.tail_len > 0 {
            let space = CHUNK_SIZE - self.tail_len;
            let to_copy = space.min(remaining.len());
            self.tail[self.tail_len..self.tail_len + to_copy]
                .copy_from_slice(&remaining[..to_copy]);
            self.tail_len += to_copy;
            remaining = &remaining[to_copy..];
            if self.tail_len == CHUNK_SIZE {
                self.promote_tail();
            }
        }

        // full chunks — bypass tail
        let full_chunks = remaining.len() / CHUNK_SIZE;
        if full_chunks > 0 {
            self.pending.reserve(full_chunks.min(CHUNKS_PER_SEGMENT));
            for i in 0..full_chunks {
                let start = i * CHUNK_SIZE;
                let chunk: [Hash; CHUNK_SIZE] = remaining[start..start + CHUNK_SIZE]
                    .try_into()
                    .expect("slice len == CHUNK_SIZE; qed");
                self.push_chunk(Chunk::new_memory(chunk));
            }
            remaining = &remaining[full_chunks * CHUNK_SIZE..];
        }

        // tail remainder
        if !remaining.is_empty() {
            self.tail[..remaining.len()].copy_from_slice(remaining);
            self.tail_len = remaining.len();
        }

        self.len = new_len;
        Ok(())
    }

    pub(crate) fn ensure_len(&mut self, target: usize) -> Result<(), TreeError> {
        if self.len >= target {
            return Ok(());
        }
        let needed = target - self.len;

        let tail_space = CHUNK_SIZE - self.tail_len;
        let fill_tail = tail_space.min(needed);
        debug_assert!(
            self.tail[self.tail_len..self.tail_len + fill_tail]
                .iter()
                .all(|h| *h == [0u8; 32]),
            "ensure_len: tail slots must be zeroed"
        );
        self.tail_len += fill_tail;
        let mut filled = fill_tail;
        if self.tail_len == CHUNK_SIZE {
            self.promote_tail();
        }

        let remaining = needed - filled;
        let full_chunks = remaining / CHUNK_SIZE;
        if full_chunks > 0 {
            for _ in 0..full_chunks {
                self.push_chunk(Chunk::new_memory([[0u8; 32]; CHUNK_SIZE]));
            }
            filled += full_chunks * CHUNK_SIZE;
        }

        let leftover = needed - filled;
        self.tail_len += leftover;

        self.len = target;
        Ok(())
    }

    /// Promote the full tail into a chunk, freezing pending if full
    fn promote_tail(&mut self) {
        debug_assert_eq!(self.tail_len, CHUNK_SIZE);
        self.push_chunk(Chunk::new_memory(self.tail));
        self.tail = [[0u8; 32]; CHUNK_SIZE];
        self.tail_len = 0;
    }

    /// Push a chunk to pending, freezing into a segment when full
    fn push_chunk(&mut self, chunk: Chunk) {
        self.pending.push(chunk);
        if self.pending.len() == CHUNKS_PER_SEGMENT {
            self.freeze_pending();
        }
    }

    /// Freeze the full pending buffer into an immutable segment
    fn freeze_pending(&mut self) {
        debug_assert_eq!(self.pending.len(), CHUNKS_PER_SEGMENT);
        let pending = core::mem::take(&mut self.pending);
        let boxed_arr: Box<[Chunk; CHUNKS_PER_SEGMENT]> = pending
            .into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| unreachable!()); // qed
        self.segments.push(Arc::from(boxed_arr));
    }

    /// Collect chunks from index `already` onward
    #[cfg(feature = "storage")]
    pub(crate) fn chunks_since(&self, already: usize) -> Vec<Chunk> {
        let total = self.chunk_count();
        if already >= total {
            return Vec::new();
        }
        let committed = self.segments.len() * CHUNKS_PER_SEGMENT;
        let mut result = Vec::with_capacity(total - already);

        // Collect from segments
        if already < committed {
            let start_seg = already / CHUNKS_PER_SEGMENT;
            let start_off = already % CHUNKS_PER_SEGMENT;
            for (seg_i, segment) in self.segments.iter().enumerate().skip(start_seg) {
                let from = if seg_i == start_seg { start_off } else { 0 };
                for chunk in &segment[from..] {
                    result.push(chunk.clone());
                }
            }
        }

        // Collect from pending
        let pending_start = already.saturating_sub(committed);
        if pending_start < self.pending.len() {
            for chunk in &self.pending[pending_start..] {
                result.push(chunk.clone());
            }
        }

        result
    }

    /// Remap the first `count` chunks to mmap-backed chunks (one region per shard)
    #[cfg(feature = "storage")]
    pub(crate) fn remap_chunks(
        &mut self,
        count: usize,
        regions: &[Arc<crate::storage::data::MmapRegion>],
    ) {
        use crate::storage::checkpoint::shard_address;

        let total = self.chunk_count();
        let remap_count = count.min(total);
        if remap_count == 0 || regions.is_empty() {
            return;
        }

        let committed = self.segments.len() * CHUNKS_PER_SEGMENT;
        let mut unmapped: Vec<Chunk> =
            Vec::with_capacity(total.saturating_sub(remap_count));
        for chunk_idx in remap_count..total {
            if chunk_idx < committed {
                let seg_idx = chunk_idx / CHUNKS_PER_SEGMENT;
                let seg_off = chunk_idx % CHUNKS_PER_SEGMENT;
                unmapped.push(self.segments[seg_idx][seg_off].clone());
            } else {
                unmapped.push(self.pending[chunk_idx - committed].clone());
            }
        }

        self.segments.clear();
        self.pending.clear();

        (0..remap_count)
            .map(|chunk_idx| {
                let (shard_idx, offset_in_shard) = shard_address(chunk_idx);
                Chunk::new_mapped(Arc::clone(&regions[shard_idx]), offset_in_shard)
            })
            .chain(unmapped)
            .for_each(|chunk| self.push_chunk(chunk));
    }

    /// Access the tail buffer
    #[cfg(feature = "storage")]
    pub(crate) fn tail_data(&self) -> &[Hash; CHUNK_SIZE] {
        &self.tail
    }

    #[cfg(test)]
    pub(crate) fn tail_len(&self) -> usize {
        self.tail_len
    }

    #[cfg(test)]
    pub(crate) fn get_chunk(&self, idx: usize) -> &Chunk {
        let committed = self.segments.len() * CHUNKS_PER_SEGMENT;
        if idx < committed {
            &self.segments[idx / CHUNKS_PER_SEGMENT][idx % CHUNKS_PER_SEGMENT]
        } else {
            &self.pending[idx - committed]
        }
    }
}
