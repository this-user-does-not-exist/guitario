//! L2 playback: turns a score into sound with no audio device involved.
//!
//! [`TempoMap`] converts ticks to seconds following repeats and tempo changes. [`Player`] schedules
//! notes and metronome clicks sample-accurately and synthesises them with a plucked-string model
//! (Karplus–Strong with fractional delay, so bends, slides and vibrato glide). Each track then
//! runs its effect chain (amp, cabinet, pedals, reverb …) and equalizer, and the master bus its
//! compressor, reverb and equalizer ([`TrackSound`], [`Player::set_sound`], [`Player::set_master`]). It renders into any `f32` buffer: the shell feeds it to the
//! sound card, and WAV export renders it offline. A track can instead play a sampled SFZ
//! instrument ([`sfz::Instrument`], [`Player::set_instrument`]).

mod articulation;
#[cfg(not(target_arch = "wasm32"))]
mod bank;
mod fx;
#[cfg(not(target_arch = "wasm32"))]
pub mod install;
mod player;
mod role;
pub mod sfz;
mod synth;
mod tempo;
pub mod tuner;
mod wav;

pub use articulation::{Articulation, used_keys_by_articulation};
#[cfg(not(target_arch = "wasm32"))]
pub use bank::{BANK_FILE, Program, SoundBank};
pub use player::{Mix, Options, Player, TrackSound, used_keys, used_keys_by_track};
pub use role::Role;
pub use tempo::{BeatTime, TempoMap};
pub use wav::{flac_bytes, render_player, render_wav, wav_bytes};
