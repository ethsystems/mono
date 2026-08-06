#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::vec::Vec;

use crate::{
    position::Position,
    ring::BlockRing,
};

/// Fixed K-slot ring of engine restore points.
#[derive(Debug)]
pub(crate) struct CheckpointRing<F> {
    slots: Vec<Option<Slot<F>>>,
    next: usize,
}

#[derive(Debug)]
pub(crate) struct Slot<F> {
    pub(crate) fold: F,
    pub(crate) cursor: Option<Position>,
    pub(crate) ring: BlockRing,
}

impl<F> CheckpointRing<F> {
    pub(crate) fn new(slots: usize) -> Self {
        let mut buffer = Vec::with_capacity(slots);
        buffer.resize_with(slots, || None);
        Self {
            slots: buffer,
            next: 0,
        }
    }

    pub(crate) fn count(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// Stores a slot at the next write position; overwrites the oldest when full.
    pub(crate) fn store(&mut self, slot: Slot<F>) {
        let len = self.slots.len();
        if len == 0 {
            return;
        }
        self.slots[self.next] = Some(slot);
        self.next = (self.next + 1) % len;
    }

    /// Oldest retained slot; the mirror of best_at_or_below's newest-first scan.
    pub(crate) fn oldest(&self) -> Option<&Slot<F>> {
        let len = self.slots.len();
        for step in 0..len {
            let index = (self.next + step) % len;
            if let Some(slot) = &self.slots[index] {
                return Some(slot);
            }
        }
        None
    }

    /// Newest slot with cursor block at or below the argument; empty-cursor slots always qualify.
    #[cold]
    pub(crate) fn best_at_or_below(&self, block: u64) -> Option<&Slot<F>> {
        let len = self.slots.len();
        if len == 0 {
            return None;
        }
        for step in 0..len {
            let index = (self.next + len - 1 - step) % len;
            if let Some(slot) = &self.slots[index]
                && slot.cursor.is_none_or(|cursor| cursor.block <= block)
            {
                return Some(slot);
            }
        }
        None
    }

    /// Drops slots with cursor block strictly above the argument.
    #[cold]
    pub(crate) fn drop_above(&mut self, block: u64) {
        for slot in &mut self.slots {
            if let Some(inner) = slot
                && inner.cursor.is_some_and(|cursor| cursor.block > block)
            {
                *slot = None;
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        for slot in &mut self.slots {
            *slot = None;
        }
    }
}
