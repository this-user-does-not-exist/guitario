//! The tuner's ears: the pitch of one string (YIN: the period whose cumulative-mean-normalised
//! difference first dips under a threshold, refined between samples), and how far each string of a
//! strum is from its target (the strongest spectral peak near the target, found with Goertzel
//! filters over a Hann window and refined between steps). Pure and allocation-light, so it runs on
//! the UI thread a few times a second.

/// How much of the input the tuner listens to, in seconds: six periods of the lowest note.
pub const WINDOW: f32 = 0.2;

/// Lowest and highest pitches the single-string detector looks for, in Hz: a low B on a
/// five-string bass to a high fret on the top string.
const LOWEST: f32 = 28.0;
const HIGHEST: f32 = 1500.0;
/// YIN's dip threshold: lower is stricter.
const THRESHOLD: f32 = 0.15;

/// A detected pitch and how periodic the signal is (0 noise to 1 a pure tone).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Detected {
    pub frequency: f32,
    pub clarity: f32,
}

/// Cents from `target` to `frequency`.
pub fn cents(frequency: f32, target: f32) -> f32 {
    1200.0 * (frequency / target.max(f32::MIN_POSITIVE)).log2()
}

/// The frequency of MIDI note `key` at A4 = 440 Hz.
pub fn frequency_of(key: f32) -> f32 {
    440.0 * 2f32.powf((key - 69.0) / 12.0)
}

/// The nearest MIDI note to `frequency`, and the cents from it.
pub fn nearest_note(frequency: f32) -> (u8, f32) {
    let key = 69.0 + 12.0 * (frequency / 440.0).log2();
    let near = key.round().clamp(0.0, 127.0);
    (near as u8, (key - near) * 100.0)
}

/// The pitch of a single note in `samples` (mono, at `rate`), or `None` for silence, noise or a
/// signal too short to hold two periods of the lowest note.
pub fn detect(samples: &[f32], rate: u32) -> Option<Detected> {
    let rate = rate as f32;
    if !(1000.0..=400_000.0).contains(&rate) || samples.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let min_tau = (rate / HIGHEST).floor().max(2.0) as usize;
    let max_tau = (rate / LOWEST).ceil() as usize;
    // Each lag compares a window with itself one lag later.
    let window = samples.len().checked_sub(max_tau)?;
    if window < max_tau.min(2048) || window < 64 {
        return None;
    }
    let energy: f32 = samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32;
    if energy < 1e-7 {
        return None;
    }
    // The difference function and its cumulative mean normalisation.
    let mut d = vec![0.0f32; max_tau + 2];
    let mut running = 0.0f32;
    let mut cmnd = vec![1.0f32; max_tau + 2];
    for tau in 1..=max_tau + 1 {
        let (Some(a), Some(b)) = (samples.get(..window), samples.get(tau..tau + window)) else { break };
        let sum: f32 = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum();
        if let Some(slot) = d.get_mut(tau) {
            *slot = sum;
        }
        running += sum;
        if let Some(slot) = cmnd.get_mut(tau) {
            *slot = if running > 0.0 { sum * tau as f32 / running } else { 1.0 };
        }
    }
    // The first dip under the threshold, followed to its bottom; else the deepest dip.
    let mut tau = (min_tau..=max_tau).find(|t| cmnd.get(*t).is_some_and(|v| *v < THRESHOLD));
    if let Some(mut t) = tau {
        while t < max_tau && cmnd.get(t + 1) < cmnd.get(t) {
            t += 1;
        }
        tau = Some(t);
    }
    let tau = match tau {
        Some(t) => t,
        None => {
            let (t, v) = (min_tau..=max_tau).filter_map(|t| cmnd.get(t).map(|v| (t, *v))).min_by(|a, b| a.1.total_cmp(&b.1))?;
            if v > 0.35 {
                return None;
            }
            t
        }
    };
    let clarity = (1.0 - cmnd.get(tau).copied().unwrap_or(1.0)).clamp(0.0, 1.0);
    // Between samples: a parabola through the raw differences around the dip.
    let (Some(&y0), Some(&y1), Some(&y2)) = (d.get(tau - 1), d.get(tau), d.get(tau + 1)) else { return None };
    let denom = y0 - 2.0 * y1 + y2;
    let shift = if denom.abs() > f32::EPSILON { (0.5 * (y0 - y2) / denom).clamp(-1.0, 1.0) } else { 0.0 };
    let period = tau as f32 + shift;
    (period > 0.0).then(|| Detected { frequency: rate / period, clarity })
}

/// The magnitude of `samples` (already windowed) at `frequency`, by the Goertzel recurrence.
/// In `f64`: at low frequencies the coefficient is close to 2 and `f32` drifts by cents.
fn goertzel(samples: &[f32], rate: f32, frequency: f32) -> f32 {
    let w = 2.0 * std::f64::consts::PI * f64::from(frequency) / f64::from(rate);
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for x in samples {
        let s0 = f64::from(*x) + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt() as f32
}

/// How far each target (Hz) is from the peak a strum has near it, in cents (`None` when that
/// string is not heard). The search spans a semitone either side, so a string far out of tune
/// shows as about ±100.
pub fn strings(samples: &[f32], rate: u32, targets: &[f32]) -> Vec<Option<f32>> {
    let rate_f = rate as f32;
    if samples.len() < 1024 || !(1000.0..=400_000.0).contains(&rate_f) || samples.iter().any(|x| !x.is_finite()) {
        return vec![None; targets.len()];
    }
    let n = samples.len();
    let windowed: Vec<f32> =
        samples.iter().enumerate().map(|(i, x)| x * (0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos())).collect();
    // Peak amplitude a full-scale sine would give, for the threshold.
    let scale = windowed.len() as f32 / 4.0;
    let peaks: Vec<Option<(f32, f32)>> = targets
        .iter()
        .map(|&target| {
            if !(10.0..rate_f / 2.0).contains(&target) {
                return None;
            }
            let at = |c: f32| goertzel(&windowed, rate_f, target * 2f32.powf(c / 1200.0));
            // Coarse, then fine around the best.
            let coarse = (-100..=100).step_by(5).map(|c| (c as f32, at(c as f32))).max_by(|a, b| a.1.total_cmp(&b.1))?;
            let fine = (-10..=10).map(|k| coarse.0 + k as f32 * 0.5).map(|c| (c, at(c))).max_by(|a, b| a.1.total_cmp(&b.1))?;
            let (y0, y1, y2) = (at(fine.0 - 0.5), fine.1, at(fine.0 + 0.5));
            let denom = y0 - 2.0 * y1 + y2;
            let shift = if denom.abs() > f32::EPSILON { (0.5 * (y0 - y2) / denom).clamp(-0.5, 0.5) * 0.5 } else { 0.0 };
            Some((fine.0 + shift, y1 / scale))
        })
        .collect();
    let loudest = peaks.iter().flatten().map(|p| p.1).fold(0.0f32, f32::max);
    peaks.into_iter().map(|p| p.filter(|(c, amp)| *amp > 0.002 && *amp >= loudest * 0.08 && c.abs() < 99.0).map(|(c, _)| c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn tone(freq: f32, seconds: f32, harmonics: &[f32]) -> Vec<f32> {
        let n = (RATE as f32 * seconds) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                harmonics.iter().enumerate().map(|(h, a)| a * (2.0 * std::f32::consts::PI * freq * (h + 1) as f32 * t).sin()).sum::<f32>() * 0.3
            })
            .collect()
    }

    /// A little noise, from a fixed seed.
    fn noisy(mut x: Vec<f32>, amount: f32) -> Vec<f32> {
        let mut seed = 0x1234_5678u32;
        for v in &mut x {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            *v += (seed as f32 / u32::MAX as f32 - 0.5) * amount;
        }
        x
    }

    #[test]
    fn single_strings_are_found_within_two_cents() {
        // Every string of standard guitar and five-string bass, a little sharp or flat.
        for key in [23.0f32, 28.0, 33.0, 40.0, 45.0, 50.0, 55.0, 59.0, 64.0, 76.0, 88.0] {
            for off in [-30.0f32, -7.0, 0.0, 4.5, 23.0] {
                let freq = frequency_of(key + off / 100.0);
                for (shape, partials) in [("sine", &[1.0f32][..]), ("string", &[1.0, 0.6, 0.4, 0.25, 0.15, 0.1][..])] {
                    // The tuner listens to the last 0.2 s.
                    let samples = noisy(tone(freq, WINDOW, partials), 0.02);
                    let got = detect(&samples, RATE).unwrap_or_else(|| panic!("{shape} at {freq} Hz not found"));
                    let err = cents(got.frequency, freq);
                    assert!(err.abs() <= 2.0, "{shape} {freq:.2} Hz: read {:.2} Hz ({err:+.2} cents)", got.frequency);
                    let (near, off_cents) = nearest_note(got.frequency);
                    assert_eq!(near, key as u8);
                    assert!((off_cents - off).abs() <= 2.0);
                }
            }
        }
    }

    #[test]
    fn silence_and_noise_have_no_pitch() {
        assert!(detect(&vec![0.0; 8000], RATE).is_none());
        assert!(detect(&noisy(vec![0.0; 8000], 0.5), RATE).is_none());
        assert!(detect(&[0.1; 100], RATE).is_none(), "too short");
        assert!(detect(&[f32::NAN; 8000], RATE).is_none());
        assert!(detect(&tone(110.0, 0.1, &[1.0]), 0).is_none());
    }

    #[test]
    fn a_strum_reads_each_string() {
        // Standard tuning; the G string 12 cents flat, the B 8 sharp, the high E not played.
        let targets: Vec<f32> = [40.0f32, 45.0, 50.0, 55.0, 59.0, 64.0].iter().map(|k| frequency_of(*k)).collect();
        let offsets = [0.0f32, 3.0, -2.0, -12.0, 8.0];
        let mut mix = vec![0.0f32; (RATE as f32 * 0.4) as usize];
        for (t, off) in targets.iter().zip(offsets) {
            let s = tone(t * 2f32.powf(off / 1200.0), 0.4, &[1.0, 0.5, 0.3, 0.2]);
            for (m, x) in mix.iter_mut().zip(s) {
                *m += x;
            }
        }
        let read = strings(&noisy(mix, 0.02), RATE, &targets);
        for (i, off) in offsets.iter().enumerate() {
            let got = read[i].unwrap_or_else(|| panic!("string {i} not heard"));
            assert!((got - off).abs() <= 2.0, "string {i}: {got:+.2} cents, wanted {off:+}");
        }
        assert!(read[5].is_none() || read[5].is_some_and(|c| c.abs() < 5.0), "the high E rings only as a harmonic of the A");
        assert_eq!(strings(&[0.0; 10], RATE, &targets), vec![None; 6]);
    }
}
