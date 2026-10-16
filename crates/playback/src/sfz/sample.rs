//! Decoded audio for sampled instruments: WAV (16/24/32-bit int, 32-bit float) and FLAC.

use std::io::Cursor;

/// Longest sample accepted, in frames (about 6 minutes at 48 kHz); longer files are rejected so a
/// hostile or huge file cannot exhaust memory.
const MAX_FRAMES: usize = 16_000_000;

#[derive(Debug, thiserror::Error)]
pub enum SfzError {
    #[error("{0}")]
    Decode(String),
    #[error("io: {0}")]
    Io(String),
    #[error("no usable regions in {0}")]
    Empty(String),
}

/// Mono or stereo audio at its native rate. Extra channels beyond two are dropped.
#[derive(Debug)]
pub struct Sample {
    pub rate: u32,
    pub left: Vec<f32>,
    pub right: Option<Vec<f32>>,
}

impl Sample {
    pub fn frames(&self) -> usize {
        self.left.len()
    }

    #[inline]
    pub fn get(&self, i: isize) -> (f32, f32) {
        let Ok(idx) = usize::try_from(i) else { return (0.0, 0.0) };
        let l = self.left.get(idx).copied().unwrap_or(0.0);
        let r = self.right.as_ref().map_or(l, |r| r.get(idx).copied().unwrap_or(0.0));
        (l, r)
    }

    pub fn from_interleaved(rate: u32, channels: usize, data: &[f32]) -> Result<Sample, SfzError> {
        if channels == 0 || rate == 0 || rate > 768_000 {
            return Err(SfzError::Decode("bad sample format".into()));
        }
        let frames = data.len() / channels;
        if frames > MAX_FRAMES {
            return Err(SfzError::Decode("sample too long".into()));
        }
        let mut left = Vec::with_capacity(frames);
        let mut right = (channels > 1).then(|| Vec::with_capacity(frames));
        for f in data.chunks_exact(channels) {
            left.push(f.first().copied().unwrap_or(0.0));
            if let Some(r) = right.as_mut() {
                r.push(f.get(1).copied().unwrap_or(0.0));
            }
        }
        // A stereo file that is really mono (identical channels) costs half the memory as mono.
        if right.as_ref().is_some_and(|r| *r == left) {
            right = None;
        }
        Ok(Sample { rate, left, right })
    }

    /// Decodes by file extension (`wav` or `flac`).
    pub fn decode(bytes: &[u8], name: &str) -> Result<Sample, SfzError> {
        let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "wav" | "wave" => decode_wav(bytes),
            "flac" => decode_flac(bytes),
            other => Err(SfzError::Decode(format!("unsupported sample format .{other} ({name})"))),
        }
    }
}

fn decode_wav(bytes: &[u8]) -> Result<Sample, SfzError> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes)).map_err(|e| SfzError::Decode(e.to_string()))?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels);
    if usize::try_from(reader.len()).map_or(true, |n| n / channels.max(1) > MAX_FRAMES) {
        return Err(SfzError::Decode("sample too long".into()));
    }
    let data: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (2f32.powi(i32::from(spec.bits_per_sample.clamp(1, 32)) - 1));
            reader.samples::<i32>().map(|s| s.unwrap_or(0) as f32 * scale).collect()
        }
    };
    Sample::from_interleaved(spec.sample_rate, channels, &data)
}

fn decode_flac(bytes: &[u8]) -> Result<Sample, SfzError> {
    let mut reader = claxon::FlacReader::new(Cursor::new(bytes)).map_err(|e| SfzError::Decode(e.to_string()))?;
    let info = reader.streaminfo();
    let channels = info.channels as usize;
    let scale = 1.0 / (2f32.powi(info.bits_per_sample.clamp(1, 32) as i32 - 1));
    let mut data = Vec::new();
    for s in reader.samples() {
        data.push(s.map_err(|e| SfzError::Decode(e.to_string()))? as f32 * scale);
        if data.len() > MAX_FRAMES.saturating_mul(channels.clamp(1, 8)) {
            return Err(SfzError::Decode("sample too long".into()));
        }
    }
    Sample::from_interleaved(info.sample_rate, channels, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn wav_bytes(rate: u32, channels: u16, frames: &[Vec<i16>]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        let spec = hound::WavSpec { channels, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        if let Ok(mut w) = hound::WavWriter::new(&mut buf, spec) {
            for f in frames {
                for s in f {
                    let _ = w.write_sample(*s);
                }
            }
            let _ = w.finalize();
        }
        buf.into_inner()
    }

    #[test]
    fn wav_roundtrip_mono_and_stereo() {
        let mono = Sample::decode(&wav_bytes(44_100, 1, &[vec![16384], vec![-16384]]), "a.wav").unwrap();
        assert_eq!((mono.rate, mono.frames(), mono.right.is_none()), (44_100, 2, true));
        assert!((mono.get(0).0 - 0.5).abs() < 1e-3 && (mono.get(1).1 + 0.5).abs() < 1e-3);
        let st = Sample::decode(&wav_bytes(48_000, 2, &[vec![100, -100], vec![200, -200]]), "a.WAV").unwrap();
        assert!(st.right.is_some());
        assert_eq!(st.get(-1), (0.0, 0.0));
        assert_eq!(st.get(99), (0.0, 0.0));
    }

    #[test]
    fn identical_channels_collapse_to_mono() {
        let s = Sample::decode(&wav_bytes(44_100, 2, &[vec![5, 5], vec![9, 9]]), "x.wav").unwrap();
        assert!(s.right.is_none());
    }

    #[test]
    fn rejects_garbage() {
        assert!(Sample::decode(b"nope", "a.wav").is_err());
        assert!(Sample::decode(b"nope", "a.flac").is_err());
        assert!(Sample::decode(b"", "a.ogg").is_err());
        assert!(Sample::from_interleaved(0, 2, &[0.0]).is_err());
        assert!(Sample::from_interleaved(44_100, 0, &[0.0]).is_err());
    }
}
