use std::sync::Arc;

use parking_lot::{
    Condvar,
    Mutex,
};

/// Shared state for tracking the last durably flushed WAL sequence number
pub(crate) struct DurabilityTracker {
    inner: Arc<(Mutex<Option<u64>>, Condvar)>,
}

impl DurabilityTracker {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new((Mutex::new(None), Condvar::new())),
        }
    }

    /// Create a token that can wait for the given sequence number to be flushed
    pub(crate) fn token(&self, seq: u64) -> DurabilityToken {
        DurabilityToken {
            seq,
            inner: Arc::clone(&self.inner),
        }
    }

    /// Mark all entries up to `seq` as durably flushed and wake waiters
    pub(crate) fn mark_flushed(&self, seq: u64) {
        let (lock, cvar) = &*self.inner;
        let mut flushed = lock.lock();
        match *flushed {
            Some(prev) if seq <= prev => return,
            _ => *flushed = Some(seq),
        }
        cvar.notify_all();
    }
}

/// A token returned from `RotorTree::insert` that tracks whether the
/// corresponding WAL entry has been flushed to disk
#[derive(Debug)]
pub struct DurabilityToken {
    seq: u64,
    inner: Arc<(Mutex<Option<u64>>, Condvar)>,
}

impl DurabilityToken {
    /// Block until the WAL entry for this insert has been fsynced
    pub fn wait(&self) {
        let (lock, cvar) = &*self.inner;
        let mut flushed = lock.lock();
        while !matches!(*flushed, Some(f) if f >= self.seq) {
            cvar.wait(&mut flushed);
        }
    }

    /// Non-blocking check if entry was fsynced
    pub fn is_durable(&self) -> bool {
        let (lock, _) = &*self.inner;
        matches!(*lock.lock(), Some(f) if f >= self.seq)
    }
}
