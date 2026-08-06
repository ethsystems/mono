use crate::{
    batch::Batch,
    position::{
        BlockRef,
        Position,
    },
};

/// Oldest position a source can replay from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayHorizon {
    /// Source replays the whole chain.
    Genesis,
    /// Source replays only from this block upward.
    FromBlock(u64),
}

/// Batched event supply over one chain with a single consistent view.
///
/// Normative obligations: serve one consistent chain view per batch, never mix
/// endpoints; set `boundary` to the current header of the cursor block whenever a
/// cursor is given; deliver only blocks strictly after the cursor block, each span
/// carrying the block's complete and non-empty event set in log order; report the
/// horizon honestly. An empty batch means nothing new exists past the cursor, and a
/// block with no events is left out of the spans rather than spanned empty.
pub trait EventSource {
    /// Consumer event the source decodes logs into.
    type Event;
    /// Failure polling the source.
    type Error;

    /// Fills `out` with the blocks strictly after `cursor` and the cursor's boundary header.
    fn next_batch(
        &mut self,
        cursor: Option<Position>,
        out: &mut Batch<Self::Event>,
    ) -> Result<(), Self::Error>;

    /// Oldest block this source can still replay.
    fn horizon(&self) -> ReplayHorizon;
}

/// Fork-recovery capability: random access headers for bisection.
pub trait ProbeSource: EventSource {
    /// Ok(None) means no block at that number exists on the current chain.
    fn header_at(&mut self, number: u64) -> Result<Option<BlockRef>, Self::Error>;
}
