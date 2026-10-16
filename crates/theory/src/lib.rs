//! L0 music theory primitives: durations, time signatures, tunings and pitch.
//!
//! Everything here is pure data with no workspace dependencies, so every other crate can use it.

mod chords;
pub mod drums;
mod duration;
mod pitch;
mod tuning;

pub use chords::{chord_name, pitch_class_name};
pub use duration::{Duration, NoteValue, TICKS_PER_QUARTER, TimeSignature};
pub use pitch::{Pitch, note_name};
pub use tuning::{Tuning, TuningError};
