use core::marker::PhantomData;

use crate::position::BlockRef;

/// Non-blocking lookup of the expected view at a block, over caller-supplied data.
pub trait Anchor {
    /// Projection compared against the fold's own view.
    type View: PartialEq;

    /// Expected view at a block, or None when the caller has no expectation there.
    fn expected(&self, at: &BlockRef) -> Option<Self::View>;
}

/// Anchor that never has an expectation; the default for drivers without one.
pub struct NoAnchor<V>(PhantomData<fn() -> V>);

impl<V> Default for NoAnchor<V> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<V: PartialEq> Anchor for NoAnchor<V> {
    type View = V;

    fn expected(&self, _at: &BlockRef) -> Option<V> {
        None
    }
}
