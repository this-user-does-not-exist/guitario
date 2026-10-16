use crate::{Options, Player};
use guitario_score::Score;

/// Renders the whole score offline (plus a short ring-out) as interleaved stereo.
pub fn render_wav(score: &Score, sample_rate: u32) -> Vec<f32> {
    render_player(&mut Player::new(score, sample_rate, Options::default()))
}

/// Renders a prepared player (for instance one with sampled instruments set) from the start.
pub fn render_player(p: &mut Player) -> Vec<f32> {
    let secs = (p.duration() + 2.0).min(60.0 * 30.0);
    let frames = (secs * f64::from(p.sample_rate())) as usize;
    let mut out = vec![0.0f32; frames * 2];
    p.play_from(0.0);
    p.render(&mut out, 2);
    out
}

/// 16-bit PCM WAV bytes for interleaved stereo samples.
pub fn wav_bytes(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + samples.len() * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&2u16.to_le_bytes()); // stereo
    b.extend_from_slice(&sample_rate.to_le_bytes());
    b.extend_from_slice(&(sample_rate * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_valid() {
        let s = Score::new();
        let pcm = render_wav(&s, 22_050);
        let b = wav_bytes(&pcm, 22_050);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(&b[8..12], b"WAVE");
        assert_eq!(b.len(), 44 + pcm.len() * 2);
        // 2 s of 4/4 at 120 plus 2 s ring-out, stereo.
        assert_eq!(pcm.len(), 4 * 22_050 * 2);
    }
}
