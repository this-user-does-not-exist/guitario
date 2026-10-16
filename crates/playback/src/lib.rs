//! L2 playback: turns a score into sound with no audio device involved.
//!
//! [`TempoMap`] converts ticks to seconds following repeats and tempo changes. [`Player`] schedules
//! notes and metronome clicks sample-accurately and synthesises them with a plucked-string model
//! (Karplus–Strong with fractional delay, so bends, slides and vibrato glide), plus a simple amp
//! model for overdriven programs. It renders into any `f32` buffer: the shell feeds it to the
//! sound card, and WAV export renders it offline. A track can instead play a sampled SFZ
//! instrument ([`sfz::Instrument`], [`Player::set_instrument`]).

#[cfg(not(target_arch = "wasm32"))]
mod bank;
mod player;
pub mod sfz;
mod synth;
mod tempo;
mod wav;

#[cfg(not(target_arch = "wasm32"))]
pub use bank::{BANK_FILE, Role, SoundBank};
pub use player::{Options, Player, used_keys};
pub use tempo::{BeatTime, TempoMap};
pub use wav::{render_player, render_wav, wav_bytes};
