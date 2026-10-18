//! Decoded audio for sampled instruments: WAV (16/24/32-bit int, 32-bit float, with the loop a
//! `smpl` chunk carries), FLAC and OGG Vorbis.

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

/// Mono or stereo audio at its native rate, kept as 16-bit values: half the memory of floats, exact
/// for the 16-bit recordings libraries mostly ship, and for 24-bit ones a loss only below -96 dB.
/// Extra channels beyond two are dropped.
#[derive(Debug)]
pub struct Sample {
    pub rate: u32,
    pub left: Vec<i16>,
    pub right: Option<Vec<i16>>,
    /// The loop the file itself marks (a WAV `smpl` chunk's first loop): its first and last
    /// frame.
    pub loop_points: Option<(u64, u64)>,
}

impl Sample {
    pub fn frames(&self) -> usize {
        self.left.len()
    }

    /// The memory its audio takes, in bytes.
    pub fn bytes(&self) -> usize {
        (self.left.len() + self.right.as_ref().map_or(0, Vec::len)) * std::mem::size_of::<i16>()
    }

    /// A sample from float audio (-1..=1), one channel or two.
    pub fn from_f32(rate: u32, left: &[f32], right: Option<&[f32]>) -> Sample {
        Sample { rate, left: left.iter().map(|x| to_i16(*x)).collect(), right: right.map(|r| r.iter().map(|x| to_i16(*x)).collect()), loop_points: None }
    }

    /// Frame `i`, both channels, -1..1.
    #[inline]
    pub fn get(&self, i: isize) -> (f32, f32) {
        let Ok(idx) = usize::try_from(i) else { return (0.0, 0.0) };
        let l = self.left.get(idx).map_or(0.0, |x| f32::from(*x) / 32_768.0);
        let r = self.right.as_ref().map_or(l, |r| r.get(idx).map_or(0.0, |x| f32::from(*x) / 32_768.0));
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
            left.push(to_i16(f.first().copied().unwrap_or(0.0)));
            if let Some(r) = right.as_mut() {
                r.push(to_i16(f.get(1).copied().unwrap_or(0.0)));
            }
        }
        // A stereo file that is really mono (identical channels) costs half the memory as mono.
        if right.as_ref().is_some_and(|r| *r == left) {
            right = None;
        }
        Ok(Sample { rate, left, right, loop_points: None })
    }

    /// Decodes by file extension (`wav`, `flac` or `ogg`).
    pub fn decode(bytes: &[u8], name: &str) -> Result<Sample, SfzError> {
        let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        match ext.as_str() {
            "wav" | "wave" => decode_wav(bytes),
            "flac" => decode_flac(bytes),
            "ogg" | "oga" => decode_ogg(bytes),
            other => Err(SfzError::Decode(format!("unsupported sample format .{other} ({name})"))),
        }
    }
}

/// A float sample (-1..=1) as 16 bits; a 16-bit source comes back exactly.
fn to_i16(x: f32) -> i16 {
    if x.is_finite() { (x * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16 } else { 0 }
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
    let mut sample = Sample::from_interleaved(spec.sample_rate, channels, &data)?;
    sample.loop_points = smpl_loop(bytes).filter(|(_, end)| usize::try_from(*end).is_ok_and(|e| e < sample.frames()));
    Ok(sample)
}

/// The first loop of a WAV file's `smpl` chunk: start and end frames (the end is played).
fn smpl_loop(bytes: &[u8]) -> Option<(u64, u64)> {
    let u32_at = |b: &[u8], at: usize| b.get(at..at + 4).and_then(|x| x.try_into().ok()).map(u32::from_le_bytes);
    if bytes.get(0..4) != Some(b"RIFF".as_slice()) || bytes.get(8..12) != Some(b"WAVE".as_slice()) {
        return None;
    }
    let mut at = 12usize;
    while let (Some(id), Some(size)) = (bytes.get(at..at + 4), u32_at(bytes, at + 4)) {
        let size = usize::try_from(size).ok()?;
        let body = at.checked_add(8)?;
        if id == b"smpl" {
            let chunk = bytes.get(body..body.checked_add(size)?.min(bytes.len()))?;
            if u32_at(chunk, 28)? == 0 {
                return None;
            }
            let (start, end) = (u32_at(chunk, 36 + 8)?, u32_at(chunk, 36 + 12)?);
            return (end > start).then_some((u64::from(start), u64::from(end)));
        }
        // Chunks are padded to an even length.
        at = body.checked_add(size)?.checked_add(size % 2)?;
    }
    None
}

/// OGG Vorbis through lewton (pure Rust). A decoder bug on a hostile file is turned into an error
/// rather than taking the app down.
fn decode_ogg(bytes: &[u8]) -> Result<Sample, SfzError> {
    let decode = || -> Result<Sample, SfzError> {
        let mut reader = lewton::inside_ogg::OggStreamReader::new(Cursor::new(bytes)).map_err(|e| SfzError::Decode(e.to_string()))?;
        let channels = usize::from(reader.ident_hdr.audio_channels);
        if !(1..=8).contains(&channels) {
            return Err(SfzError::Decode("bad sample format".into()));
        }
        let rate = reader.ident_hdr.audio_sample_rate;
        let mut data: Vec<f32> = Vec::new();
        while let Some(packet) = reader.read_dec_packet_itl().map_err(|e| SfzError::Decode(e.to_string()))? {
            data.extend(packet.iter().map(|s| f32::from(*s) / 32_768.0));
            if data.len() / channels > MAX_FRAMES {
                return Err(SfzError::Decode("sample too long".into()));
            }
        }
        Sample::from_interleaved(rate, channels, &data)
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(decode)).unwrap_or_else(|_| Err(SfzError::Decode("the OGG decoder failed on this file".into())))
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
    fn sixteen_bit_audio_is_kept_exactly_and_floats_are_clamped() {
        let frames: Vec<Vec<i16>> = [-32_768i16, -1, 0, 1, 12_345, 32_767].iter().map(|v| vec![*v]).collect();
        let s = Sample::decode(&wav_bytes(8000, 1, &frames), "a.wav").unwrap();
        assert_eq!(s.left, vec![-32_768, -1, 0, 1, 12_345, 32_767]);
        assert_eq!(s.bytes(), 12);
        let f = Sample::from_f32(8000, &[2.0, -2.0, f32::NAN, 0.5], None);
        assert_eq!(f.left, vec![32_767, -32_768, 0, 16_384]);
        assert!((f.get(3).0 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn identical_channels_collapse_to_mono() {
        let s = Sample::decode(&wav_bytes(44_100, 2, &[vec![5, 5], vec![9, 9]]), "x.wav").unwrap();
        assert!(s.right.is_none());
    }

    /// `wav` with a `smpl` chunk holding one loop from `start` to `end`.
    fn with_loop(wav: &[u8], start: u32, end: u32) -> Vec<u8> {
        let mut smpl = vec![0u8; 36 + 24];
        smpl[28..32].copy_from_slice(&1u32.to_le_bytes());
        smpl[36 + 8..36 + 12].copy_from_slice(&start.to_le_bytes());
        smpl[36 + 12..36 + 16].copy_from_slice(&end.to_le_bytes());
        let mut out = wav.to_vec();
        out.extend(b"smpl");
        out.extend((smpl.len() as u32).to_le_bytes());
        out.extend(smpl);
        let riff = (out.len() - 8) as u32;
        out[4..8].copy_from_slice(&riff.to_le_bytes());
        out
    }

    #[test]
    fn a_wav_carries_its_loop() {
        let frames: Vec<Vec<i16>> = (0..1000).map(|i| vec![i as i16]).collect();
        let plain = wav_bytes(44_100, 1, &frames);
        assert_eq!(Sample::decode(&plain, "a.wav").unwrap().loop_points, None);
        assert_eq!(Sample::decode(&with_loop(&plain, 100, 899), "a.wav").unwrap().loop_points, Some((100, 899)));
        // A loop past the end of the audio, or backwards, is ignored; a cut-short chunk too.
        assert_eq!(Sample::decode(&with_loop(&plain, 100, 5000), "a.wav").unwrap().loop_points, None);
        assert_eq!(Sample::decode(&with_loop(&plain, 500, 100), "a.wav").unwrap().loop_points, None);
        let looped = with_loop(&plain, 100, 899);
        assert_eq!(smpl_loop(&looped[..looped.len() - 10]), None);
        assert_eq!(smpl_loop(b"RIFF\xff\xff\xff\xffWAVEsmpl\xff\xff\xff\xff"), None);
    }

    #[test]
    fn ogg_vorbis_decodes_and_hostile_files_are_errors() {
        let bytes = include_bytes!("../../tests/data/sine-440.ogg");
        let s = Sample::decode(bytes, "sine.ogg").unwrap();
        assert_eq!((s.rate, s.right.is_none()), (22_050, true));
        assert!((s.frames() as i64 - 11_025).abs() < 600, "{} frames", s.frames());
        // Half a second of 440 Hz: about 220 rising zero crossings.
        let rising = s.left.windows(2).filter(|w| w[0] < 0 && w[1] >= 0).count();
        assert!((rising as i64 - 220).abs() <= 6, "{rising}");
        for cut in [0, 10, 58, 200, bytes.len() / 2, bytes.len() - 1] {
            let _ = Sample::decode(&bytes[..cut], "t.ogg");
        }
        // Flipped bytes anywhere: an error or a sample, never a crash.
        let mut seed = 0x2545_f491u32;
        for _ in 0..300 {
            let mut b = bytes.to_vec();
            for _ in 0..4 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let i = seed as usize % b.len();
                b[i] ^= (seed >> 8) as u8 | 1;
            }
            if let Ok(s) = Sample::decode(&b, "f.ogg") {
                assert!(s.frames() <= MAX_FRAMES);
            }
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(Sample::decode(b"nope", "a.wav").is_err());
        assert!(Sample::decode(b"nope", "a.flac").is_err());
        assert!(Sample::decode(b"", "a.ogg").is_err());
        assert!(Sample::decode(b"OggS garbage", "a.ogg").is_err());
        assert!(Sample::decode(b"", "a.mp3").is_err());
        assert!(Sample::from_interleaved(0, 2, &[0.0]).is_err());
        assert!(Sample::from_interleaved(44_100, 0, &[0.0]).is_err());
    }
}
