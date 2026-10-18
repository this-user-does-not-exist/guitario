//! L4 audio: one sound-card output stream that pulls samples from a shared [`Player`] slot, and
//! an input stream ([`AudioIn`]) the tuner listens to.
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

/// Most recent input samples kept, mono.
pub const INPUT_KEEP: usize = 1 << 16;

/// The newest samples a sound-card input has delivered, mono, oldest first.
pub type Recording = Arc<Mutex<std::collections::VecDeque<f32>>>;

/// A microphone or line input, for the tuner: the stream runs while this lives and keeps the last
/// [`INPUT_KEEP`] samples, its channels mixed to mono.
pub struct AudioIn {
    _stream: cpal::Stream,
    pub sample_rate: u32,
    pub recording: Recording,
    /// The device listening, by name.
    pub device: String,
}

/// Mixes interleaved frames to mono and appends them, dropping the oldest past the limit. A busy
/// lock drops the buffer rather than wait on the audio thread.
fn record(recording: &Recording, data: impl Iterator<Item = f32>, channels: usize) {
    let Ok(mut buf) = recording.try_lock() else { return };
    let mut sum = 0.0f32;
    for (i, x) in data.enumerate() {
        sum += if x.is_finite() { x } else { 0.0 };
        if (i + 1) % channels == 0 {
            buf.push_back(sum / channels as f32);
            sum = 0.0;
        }
    }
    let extra = buf.len().saturating_sub(INPUT_KEEP);
    buf.drain(..extra);
}

impl AudioIn {
    /// Opens the default input. On macOS the first use asks the user's permission; a refusal
    /// gives silence or an error, never a crash.
    pub fn open() -> Result<AudioIn, AudioError> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or(AudioError::NoDevice)?;
        let name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "Audio input".into());
        let supported = device.default_input_config().map_err(|e| AudioError::Device(e.to_string()))?;
        let channels = usize::from(supported.channels().max(1));
        let sample_rate = supported.sample_rate();
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.config();
        let recording: Recording = Arc::new(Mutex::new(std::collections::VecDeque::with_capacity(INPUT_KEEP)));
        let shared = Arc::clone(&recording);
        let error = |err: cpal::Error| eprintln!("audio input error: {err}");
        let stream = match format {
            cpal::SampleFormat::F32 => {
                device.build_input_stream(config, move |d: &[f32], _: &cpal::InputCallbackInfo| record(&shared, d.iter().copied(), channels), error, None)
            }
            cpal::SampleFormat::I16 => device.build_input_stream(
                config,
                move |d: &[i16], _: &cpal::InputCallbackInfo| record(&shared, d.iter().map(|x| f32::from(*x) / 32768.0), channels),
                error,
                None,
            ),
            cpal::SampleFormat::U16 => device.build_input_stream(
                config,
                move |d: &[u16], _: &cpal::InputCallbackInfo| record(&shared, d.iter().map(|x| (f32::from(*x) - 32768.0) / 32768.0), channels),
                error,
                None,
            ),
            cpal::SampleFormat::I32 => device.build_input_stream(
                config,
                move |d: &[i32], _: &cpal::InputCallbackInfo| record(&shared, d.iter().map(|x| *x as f32 / 2_147_483_648.0), channels),
                error,
                None,
            ),
            other => return Err(AudioError::Device(format!("input sample format {other} is not supported"))),
        }
        .map_err(|e| AudioError::Device(e.to_string()))?;
        stream.play().map_err(|e| AudioError::Device(e.to_string()))?;
        Ok(AudioIn { _stream: stream, sample_rate, recording, device: name })
    }

    /// The last `n` samples (fewer at first).
    pub fn latest(&self, n: usize) -> Vec<f32> {
        let buf = self.recording.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        buf.iter().skip(buf.len().saturating_sub(n)).copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_is_mixed_to_mono_and_kept_to_the_limit() {
        let rec: Recording = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        record(&rec, [1.0f32, 0.0, 0.5, 0.5, f32::NAN, 1.0].into_iter(), 2);
        assert_eq!(rec.lock().unwrap().iter().copied().collect::<Vec<_>>(), vec![0.5, 0.5, 0.5]);
        record(&rec, std::iter::repeat_n(0.25f32, INPUT_KEEP * 2), 1);
        let buf = rec.lock().unwrap();
        assert_eq!(buf.len(), INPUT_KEEP);
        assert!(buf.iter().all(|x| *x == 0.25));
    }

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
