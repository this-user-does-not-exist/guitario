//! Building blocks: a biquad filter (the audio EQ cookbook's responses), a one-pole low-pass, a
//! fractional delay line and an envelope follower.

use std::f32::consts::PI;

/// Values this small are flushed to zero in feedback paths, so tails never decay into denormals.
const TINY: f32 = 1e-20;

pub(crate) fn flush(x: f32) -> f32 {
    if x.abs() < TINY || !x.is_finite() { 0.0 } else { x }
}

pub(crate) fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Shape {
    LowPass,
    HighPass,
    /// Unity gain at its centre.
    BandPass,
    Notch,
    Peak,
    LowShelf,
    HighShelf,
}

/// A second-order filter, direct form I, one state per channel.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: [f32; 2],
    x2: [f32; 2],
    y1: [f32; 2],
    y2: [f32; 2],
    /// Passes the signal untouched (a peak or shelf at 0 dB).
    bypass: bool,
}

impl Biquad {
    #[cfg(test)]
    pub(crate) fn new(shape: Shape, freq: f32, q: f32, gain_db: f32, sr: f32) -> Biquad {
        let mut f = Biquad::default();
        f.set(shape, freq, q, gain_db, sr);
        f
    }

    /// New coefficients, keeping the state so a moving knob does not click.
    pub(crate) fn set(&mut self, shape: Shape, freq: f32, q: f32, gain_db: f32, sr: f32) {
        let sr = if sr.is_finite() && sr > 1.0 { sr } else { 48_000.0 };
        let freq = if freq.is_finite() { freq.clamp(10.0, sr * 0.45) } else { 1000.0 };
        let q = if q.is_finite() { q.clamp(0.1, 20.0) } else { 0.707 };
        let gain_db = if gain_db.is_finite() { gain_db.clamp(-48.0, 48.0) } else { 0.0 };
        self.bypass = matches!(shape, Shape::Peak | Shape::LowShelf | Shape::HighShelf) && gain_db.abs() < 0.01;
        let w = 2.0 * PI * freq / sr;
        let (sin, cos) = w.sin_cos();
        let alpha = sin / (2.0 * q);
        let a = 10f32.powf(gain_db / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match shape {
            Shape::LowPass => ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            Shape::HighPass => ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            Shape::BandPass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            Shape::Notch => (1.0, -2.0 * cos, 1.0, 1.0 + alpha, -2.0 * cos, 1.0 - alpha),
            Shape::Peak => (1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a),
            Shape::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + s),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - s),
                    (a + 1.0) + (a - 1.0) * cos + s,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - s,
                )
            }
            Shape::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + s),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - s),
                    (a + 1.0) - (a - 1.0) * cos + s,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - s,
                )
            }
        };
        let a0 = if a0.abs() > 1e-9 { a0 } else { 1.0 };
        (self.b0, self.b1, self.b2, self.a1, self.a2) = (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0);
    }

    /// Filters one sample of channel `ch` (0 or 1).
    pub(crate) fn tick(&mut self, ch: usize, x: f32) -> f32 {
        if self.bypass {
            return x;
        }
        let c = ch.min(1);
        let (Some(x1), Some(x2), Some(y1), Some(y2)) = (self.x1.get(c).copied(), self.x2.get(c).copied(), self.y1.get(c).copied(), self.y2.get(c).copied())
        else {
            return x;
        };
        let y = flush(self.b0 * x + self.b1 * x1 + self.b2 * x2 - self.a1 * y1 - self.a2 * y2);
        if let (Some(a), Some(b), Some(c1), Some(d)) = (self.x1.get_mut(c), self.x2.get_mut(c), self.y1.get_mut(c), self.y2.get_mut(c)) {
            (*b, *a, *d, *c1) = (x1, x, y1, y);
        }
        y
    }
}

/// A one-pole low-pass: cheap darkening for feedback loops.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OnePole {
    k: f32,
    z: f32,
}

impl OnePole {
    pub(crate) fn new(freq: f32, sr: f32) -> OnePole {
        let mut f = OnePole::default();
        f.set(freq, sr);
        f
    }

    pub(crate) fn set(&mut self, freq: f32, sr: f32) {
        let x = (-2.0 * PI * freq.max(1.0) / sr.max(1.0)).exp();
        self.k = if x.is_finite() { 1.0 - x } else { 1.0 };
    }

    pub(crate) fn tick(&mut self, x: f32) -> f32 {
        self.z = flush(self.z + self.k * (x - self.z));
        self.z
    }
}

/// A circular buffer read at any delay, with linear interpolation between samples.
#[derive(Clone, Debug)]
pub(crate) struct DelayLine {
    buf: Vec<f32>,
    pos: usize,
}

impl DelayLine {
    /// Room for `max` samples of delay (at least one).
    pub(crate) fn new(max: usize) -> DelayLine {
        DelayLine { buf: vec![0.0; max.clamp(1, 1 << 22) + 2], pos: 0 }
    }

    pub(crate) fn push(&mut self, x: f32) {
        if let Some(s) = self.buf.get_mut(self.pos) {
            *s = flush(x);
        }
        self.pos = (self.pos + 1) % self.buf.len().max(1);
    }

    /// The sample written `delay` samples before the last push (1.0 is the last one).
    pub(crate) fn read(&self, delay: f32) -> f32 {
        let n = self.buf.len();
        let d = if delay.is_finite() { delay.clamp(1.0, (n - 1) as f32) } else { 1.0 };
        let whole = d.floor();
        let frac = d - whole;
        let i = (self.pos + n - whole as usize) % n;
        let j = (i + n - 1) % n;
        let a = self.buf.get(i).copied().unwrap_or(0.0);
        let b = self.buf.get(j).copied().unwrap_or(0.0);
        a + (b - a) * frac
    }
}

/// Follows a signal's level: fast up (attack), slower down (release), in linear gain.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Envelope {
    attack: f32,
    release: f32,
    pub(crate) level: f32,
}

impl Envelope {
    pub(crate) fn set(&mut self, attack_ms: f32, release_ms: f32, sr: f32) {
        let coef = |ms: f32| {
            let x = (-1.0 / (ms.max(0.01) * 0.001 * sr.max(1.0))).exp();
            if x.is_finite() { x } else { 0.0 }
        };
        self.attack = coef(attack_ms);
        self.release = coef(release_ms);
    }

    pub(crate) fn tick(&mut self, x: f32) -> f32 {
        let x = x.abs();
        let c = if x > self.level { self.attack } else { self.release };
        self.level = flush(x + c * (self.level - x));
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The gain of `f` at `freq`, measured with a sine.
    pub(crate) fn gain_at(mut f: impl FnMut(f32) -> f32, freq: f32, sr: f32) -> f32 {
        let n = (sr * 0.5) as usize;
        let mut peak = 0.0f32;
        for i in 0..n {
            let y = f((2.0 * PI * freq * i as f32 / sr).sin());
            if i > n / 2 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn the_cookbook_shapes_have_their_responses() {
        let sr = 48_000.0;
        let mut lp = Biquad::new(Shape::LowPass, 1000.0, 0.707, 0.0, sr);
        assert!(gain_at(|x| lp.tick(0, x), 100.0, sr) > 0.95);
        let mut lp = Biquad::new(Shape::LowPass, 1000.0, 0.707, 0.0, sr);
        assert!(gain_at(|x| lp.tick(0, x), 8000.0, sr) < 0.03);
        let mut peak = Biquad::new(Shape::Peak, 1000.0, 1.0, 6.0, sr);
        assert!((gain_at(|x| peak.tick(0, x), 1000.0, sr) - db_to_gain(6.0)).abs() < 0.05);
        let mut shelf = Biquad::new(Shape::LowShelf, 200.0, 0.707, -12.0, sr);
        assert!((gain_at(|x| shelf.tick(0, x), 30.0, sr) - db_to_gain(-12.0)).abs() < 0.05);
        let mut bp = Biquad::new(Shape::BandPass, 1000.0, 2.0, 0.0, sr);
        assert!(gain_at(|x| bp.tick(0, x), 1000.0, sr) > 0.95);
        let mut bp = Biquad::new(Shape::BandPass, 1000.0, 2.0, 0.0, sr);
        assert!(gain_at(|x| bp.tick(0, x), 8000.0, sr) < 0.1);
        let mut notch = Biquad::new(Shape::Notch, 1000.0, 2.0, 0.0, sr);
        assert!(gain_at(|x| notch.tick(0, x), 1000.0, sr) < 0.05);
        let mut hs = Biquad::new(Shape::HighShelf, 4000.0, 0.707, 6.0, sr);
        assert!((gain_at(|x| hs.tick(1, x), 100.0, sr) - 1.0).abs() < 0.05);
        // Hostile settings give a stable, finite filter.
        let mut bad = Biquad::new(Shape::Peak, f32::NAN, f32::INFINITY, f32::NAN, 0.0);
        assert!((0..1000).map(|_| bad.tick(5, 1.0)).all(f32::is_finite));
    }

    #[test]
    fn a_delay_line_reads_back_what_it_was_given() {
        let mut d = DelayLine::new(10);
        for i in 0..20 {
            d.push(i as f32);
        }
        assert_eq!(d.read(1.0), 19.0);
        assert_eq!(d.read(3.0), 17.0);
        assert_eq!(d.read(2.5), 17.5);
        assert!(d.read(f32::NAN).is_finite() && d.read(1e9).is_finite());
    }
}
