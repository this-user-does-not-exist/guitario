//! L4 audio output: one sound-card stream that pulls samples from a shared [`Player`] slot.
//! The UI swaps players in and out of the slot; the audio callback never blocks on it (a busy
//! lock yields one buffer of silence) and never panics.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use guitario_playback::Player;
use std::sync::{Arc, Mutex, TryLockError};

pub type Slot = Arc<Mutex<Option<Player>>>;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("no audio output device")]
    NoDevice,
    #[error("audio device error: {0}")]
    Device(String),
}

pub struct AudioOut {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    pub slot: Slot,
}

impl AudioOut {
    /// Opens the default output device at its preferred rate, in `f32`.
    pub fn open() -> Result<AudioOut, AudioError> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(AudioError::NoDevice)?;
        let supported = device.default_output_config().map_err(|e| AudioError::Device(e.to_string()))?;
        let channels = usize::from(supported.channels().max(1));
        let sample_rate = supported.sample_rate();
        let config: cpal::StreamConfig = supported.config();
        let slot: Slot = Arc::new(Mutex::new(None));
        let shared = Arc::clone(&slot);
        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let filled = match shared.try_lock() {
                        Ok(mut guard) => guard.as_mut().map(|p| p.render(data, channels)).is_some(),
                        Err(TryLockError::Poisoned(p)) => p.into_inner().as_mut().map(|p| p.render(data, channels)).is_some(),
                        Err(TryLockError::WouldBlock) => false,
                    };
                    if !filled {
                        data.iter_mut().for_each(|x| *x = 0.0);
                    }
                },
                |err| eprintln!("audio stream error: {err}"),
                None,
            )
            .map_err(|e| AudioError::Device(e.to_string()))?;
        stream.play().map_err(|e| AudioError::Device(e.to_string()))?;
        Ok(AudioOut { _stream: stream, sample_rate, slot })
    }
}
