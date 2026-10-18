//! L1 score model: the pure data a Guitario document is made of.
//!
//! A [`Score`] has one list of master-bar [`MeasureHeader`]s (time signature, key, tempo, repeats)
//! and a list of [`Track`]s, each holding exactly one [`Measure`] per header. No editing behaviour
//! lives here beyond queries and validation; edits are engine commands.

mod bend;
mod lyrics;
mod model;
mod playback;
mod validate;

pub use bend::{BEND_END, BEND_MAX, BEND_MAX_POINTS, Bend, BendKind, BendPoint};
pub use lyrics::{Syllable, dispatch_lyrics, syllables};
pub use model::*;
pub use playback::{PlayedBeat, playback_order, timeline};
pub use validate::{MAX_FERMATAS, MAX_LYRICS_TEXT, ScoreError};
