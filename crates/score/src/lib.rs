//! L1 score model: the pure data a Guitario document is made of.
//!
//! A [`Score`] has one list of master-bar [`MeasureHeader`]s (time signature, key, tempo, repeats)
//! and a list of [`Track`]s, each holding exactly one [`Measure`] per header. No editing behaviour
//! lives here beyond queries and validation; edits are engine commands.

mod bend;
mod curve;
mod header_footer;
mod lyrics;
mod model;
mod playback;
mod sound;
mod validate;
mod whammy;

pub use bend::{BEND_END, BEND_MAX, BEND_MAX_POINTS, Bend, BendKind, BendPoint};
pub use header_footer::{
    DEFAULT_PAGE_FONT, FirstPage, HeaderFooter, MAX_PAGE_TEXT, OtherPages, PAGE_TEXT_KEYS, PAGE_TEXT_SIZES, PageText, TOKENS, TextAlign, expand,
};
pub use lyrics::{Syllable, dispatch_lyrics, syllables};
pub use model::*;
pub use playback::{PlayedBeat, PlayedHarmonic, PlayedWhammy, playback_order, timeline};
pub use sound::{
    AMP_MODELS, CABINET_MODELS, Effect, EffectCategory, EffectType, GRAPHIC_BANDS, MASTER_SLOTS, MAX_TRACK_EFFECTS, Master, ParamSpec, Unit, default_chain,
    mixer_eq,
};
pub use validate::{MAX_FERMATAS, MAX_LYRICS_TEXT, ScoreError};
pub use whammy::{BarVibrato, WHAMMY_END, WHAMMY_MAX, WHAMMY_MAX_POINTS, WHAMMY_MIN, Whammy, WhammyKind, WhammyPoint};
