//! SFZ sampled instruments: parse an `.sfz` file, decode its samples and play regions as voices.
//! See [`Instrument`]. File access lives in `load` and is compiled out on wasm.

mod instrument;
#[cfg(not(target_arch = "wasm32"))]
mod load;
mod parse;
mod sample;
mod voice;

pub use instrument::{Hit, Instrument};
pub use parse::{LoopMode, Parsed, Region, parse, parse_key};
pub use sample::{Sample, SfzError};
pub use voice::SampleVoice;
