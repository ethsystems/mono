//! Durability seam: the apply path offers snapshots, the sink decides when to fsync.

use crate::{
    engine::Engine,
    error::DurabilityLost,
    position::Position,
};

/// Durability sink for engine snapshots; the apply path never fsyncs.
pub trait SnapshotSink<F> {
    /// Offers the engine's durable point for persistence; must not fsync.
    fn offer(&mut self, engine: &Engine<F>) -> Result<(), DurabilityLost>;
    /// Cursor a restart would recover; falls when a resync persists older state.
    fn durable_cursor(&self) -> Option<Position>;
}

/// Sink that persists nothing; the default for drivers without durability.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoSink;

impl<F> SnapshotSink<F> for NoSink {
    fn offer(&mut self, _engine: &Engine<F>) -> Result<(), DurabilityLost> {
        Ok(())
    }

    fn durable_cursor(&self) -> Option<Position> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        engine::EngineConfig,
        test_util::RecordingFold,
    };

    #[test]
    fn no_sink_offer_succeeds_without_a_durable_cursor() {
        // given a NoSink and a fresh engine
        let engine = Engine::new(
            RecordingFold::default(),
            EngineConfig {
                ring_capacity: 8,
                checkpoint_slots: 2,
            },
        )
        .unwrap();
        let mut sink = NoSink;
        // when the engine is offered
        let offered = sink.offer(&engine);
        // then the offer succeeds and the durable cursor stays None
        assert_eq!(offered, Ok(()));
        assert_eq!(SnapshotSink::<RecordingFold>::durable_cursor(&sink), None);
    }
}
