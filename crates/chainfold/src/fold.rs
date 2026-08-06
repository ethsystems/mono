use crate::{
    error::FoldError,
    position::Position,
};

/// Consumer state machine folding events at strictly increasing positions.
pub trait Fold {
    /// Event the fold consumes.
    type Event;
    /// Comparable projection of the state, used for anchor checks.
    type View: PartialEq;
    /// Failure the fold classifies as skip, halt, or poison.
    type Error;

    /// Folds one event into the state at a strictly increasing position.
    fn apply(
        &mut self,
        pos: Position,
        event: &Self::Event,
    ) -> Result<(), FoldError<Self::Error>>;
    /// Reads the comparable projection of the current state.
    fn view(&self) -> Self::View;
}
