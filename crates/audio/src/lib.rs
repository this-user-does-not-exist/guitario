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

/// Sound › Audio Settings…: which output to play through and how big its buffers are.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioSettings {
    /// An output device by name; `None` is the system default.
    pub device: Option<String>,
    /// Frames per buffer; `None` lets the device choose. Smaller is quicker to respond, larger
    /// is safer from dropouts.
    pub buffer: Option<u32>,
}

/// Buffer sizes offered in the settings.
pub const BUFFER_SIZES: [u32; 6] = [64, 128, 256, 512, 1024, 2048];

/// An output device the system offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputInfo {
    pub name: String,
    pub default: bool,
}

/// The output devices, by name, with the system default marked.
pub fn outputs() -> Vec<OutputInfo> {
    let host = cpal::default_host();
    let default = host.default_output_device().and_then(|d| d.description().ok()).map(|d| d.name().to_string());
    let Ok(devices) = host.output_devices() else { return Vec::new() };
    devices
        .filter_map(|d| d.description().ok().map(|d| d.name().to_string()))
        .map(|name| OutputInfo { default: Some(&name) == default.as_ref(), name })
        .collect()
}

/// The buffer size to ask for: the wanted one, clamped into what the device supports.
pub fn buffer_for(supported: &cpal::SupportedBufferSize, wanted: Option<u32>) -> cpal::BufferSize {
    match (wanted, supported) {
        (None, _) => cpal::BufferSize::Default,
        (Some(n), cpal::SupportedBufferSize::Range { min, max }) if min <= max => cpal::BufferSize::Fixed(n.clamp(*min, *max)),
        (Some(_), _) => cpal::BufferSize::Default,
    }
}

pub struct AudioOut {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    pub slot: Slot,
    /// The device playing, by name.
    pub device: String,
    /// The settings named a device that is not there; the default plays instead.
    pub fell_back: bool,
}

impl AudioOut {
    /// Opens the default output device at its preferred rate, in `f32`.
    pub fn open() -> Result<AudioOut, AudioError> {
        AudioOut::open_with(&AudioSettings::default())
    }

    /// Opens the output `settings` name (or the default, if it is not there) with their buffer size.
    pub fn open_with(settings: &AudioSettings) -> Result<AudioOut, AudioError> {
        let host = cpal::default_host();
        let named = settings
            .device
            .as_ref()
            .and_then(|want| host.output_devices().ok()?.find(|d| d.description().ok().is_some_and(|desc| desc.name() == want.as_str())));
        let fell_back = settings.device.is_some() && named.is_none();
        let device = named.or_else(|| host.default_output_device()).ok_or(AudioError::NoDevice)?;
        let name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "Audio output".into());
        let supported = device.default_output_config().map_err(|e| AudioError::Device(e.to_string()))?;
        let channels = usize::from(supported.channels().max(1));
        let sample_rate = supported.sample_rate();
        let mut config: cpal::StreamConfig = supported.config();
        config.buffer_size = buffer_for(supported.buffer_size(), settings.buffer);
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
        Ok(AudioOut { _stream: stream, sample_rate, slot, device: name, fell_back })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_buffer_size_is_kept_inside_what_the_device_allows() {
        let range = cpal::SupportedBufferSize::Range { min: 128, max: 1024 };
        assert_eq!(buffer_for(&range, None), cpal::BufferSize::Default);
        assert_eq!(buffer_for(&range, Some(256)), cpal::BufferSize::Fixed(256));
        assert_eq!(buffer_for(&range, Some(64)), cpal::BufferSize::Fixed(128));
        assert_eq!(buffer_for(&range, Some(9000)), cpal::BufferSize::Fixed(1024));
        assert_eq!(buffer_for(&cpal::SupportedBufferSize::Unknown, Some(256)), cpal::BufferSize::Default);
        // A device reporting a backwards range is not trusted.
        assert_eq!(buffer_for(&cpal::SupportedBufferSize::Range { min: 512, max: 64 }, Some(256)), cpal::BufferSize::Default);
    }
}
