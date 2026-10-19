//! The resonating bodies of modelled instruments: a piano's soundboard and case, the wooden
//! boxes of the violin family. A body is a bank of resonances, each a mode of the plates and the
//! air inside, between the force on the bridge and the sound in the room. A few are the
//! instrument's signature modes, measured on real instruments (the air mode A0, the corpus
//! bending and the two main wood modes B1- and B1+ of a violin); the rest are spread densely and
//! irregularly under the instrument's average response (the violin's "bridge hill" around
//! 2.5 kHz, a soundboard's falling radiation above 4 kHz). That irregularity is what a real body
//! has: each note, and each instant of a vibrato, lands on different peaks, which is much of
//! what makes a bowed note sound alive rather than synthetic. Each mode radiates to the left and
//! right with its own weights, as a body radiates differently in each direction, so a mono
//! string comes out wide.
//!
//! A bowed instrument's other strings ring in sympathy through the bridge: four open strings
//! tuned to the instrument's, fed a little of the bridge's motion. A piano's do too, all of them,
//! while the sustain pedal holds the dampers off: the halo of a pedalled chord.

use crate::fx::filter::{Biquad, Shape, flush};
use crate::piano::hash01;

const LANES: usize = 8;

/// Linear interpolation through `points` (sorted by x), held flat beyond the ends.
pub(crate) fn curve(points: &[(f32, f32)], x: f32) -> f32 {
    let Some(&(x0, y0)) = points.first() else { return 0.0 };
    if x <= x0 {
        return y0;
    }
    for pair in points.windows(2) {
        if let [(xa, ya), (xb, yb)] = *pair
            && x <= xb
        {
            let t = if xb > xa { (x - xa) / (xb - xa) } else { 1.0 };
            return ya + (yb - ya) * t;
        }
    }
    points.last().map_or(0.0, |p| p.1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Piano,
    Violin,
    Viola,
    Cello,
    DoubleBass,
}

impl BodyKind {
    /// The open strings, as MIDI notes, that ring in sympathy (none for the piano here).
    fn open_strings(self) -> &'static [u8] {
        match self {
            BodyKind::Piano => &[],
            BodyKind::Violin => &[55, 62, 69, 76],
            BodyKind::Viola => &[48, 55, 62, 69],
            BodyKind::Cello => &[36, 43, 50, 57],
            BodyKind::DoubleBass => &[28, 33, 38, 43],
        }
    }

    /// The body that suits a pitch in a string section: the bass parts on a double bass, and
    /// so on up.
    pub fn for_pitch(midi: f32) -> BodyKind {
        if midi < 40.0 {
            BodyKind::DoubleBass
        } else if midi < 53.0 {
            BodyKind::Cello
        } else if midi < 62.0 {
            BodyKind::Viola
        } else {
            BodyKind::Violin
        }
    }

    #[cfg(test)]
    pub const ALL: [BodyKind; 5] = [BodyKind::Piano, BodyKind::Violin, BodyKind::Viola, BodyKind::Cello, BodyKind::DoubleBass];
}

/// A body's design: signature modes, then the dense ones.
struct Design {
    /// (frequency, Q, level in dB).
    signature: &'static [(f32, f32, f32)],
    /// How many dense modes, from where to where.
    dense: usize,
    low: f32,
    high: f32,
    /// The average response of the dense modes, dB against log2 of frequency.
    envelope: &'static [(f32, f32)],
    /// Q of the dense modes against log2 of frequency.
    q: &'static [(f32, f32)],
    /// Dense modes scaled by this in frequency (a viola is a larger violin).
    scale: f32,
    /// How much of the bridge's force reaches the room directly, against through the modes.
    direct: f32,
    /// The bottom, below which the body hardly radiates, and the top.
    low_cut: f32,
    high_cut: f32,
    seed: u32,
}

const VIOLIN_MODES: [(f32, f32, f32); 6] =
    [(275.0, 14.0, 0.0), (405.0, 25.0, -9.0), (460.0, 22.0, -5.0), (470.0, 28.0, 1.0), (545.0, 26.0, 3.0), (650.0, 30.0, -6.0)];
const VIOLIN_ENVELOPE: [(f32, f32); 7] = [(7.0, -14.0), (8.2, -4.0), (9.3, -6.0), (10.3, -3.0), (11.35, 4.0), (12.1, -2.0), (13.3, -18.0)];
const VIOLIN_Q: [(f32, f32); 3] = [(8.0, 32.0), (11.0, 22.0), (13.0, 14.0)];

fn design(kind: BodyKind) -> Design {
    // The violin family share a shape, scaled: the viola's body is about 15 % larger, the cello's
    // and the double bass's much more so, with their bridge hills lower but not as much lower.
    let violin = |scale: f32, high_cut: f32, seed: u32| Design {
        signature: &VIOLIN_MODES,
        dense: 88,
        low: 300.0,
        high: 9000.0,
        envelope: &VIOLIN_ENVELOPE,
        q: &VIOLIN_Q,
        scale,
        direct: 0.08,
        low_cut: 180.0 * scale.sqrt(),
        high_cut,
        seed,
    };
    match kind {
        BodyKind::Piano => Design {
            signature: &[],
            dense: 72,
            low: 45.0,
            high: 6500.0,
            envelope: &[(5.5, -6.0), (6.6, 2.0), (8.6, 1.0), (10.5, -1.0), (12.0, -5.0), (12.7, -12.0)],
            q: &[(5.5, 30.0), (9.0, 26.0), (12.5, 16.0)],
            scale: 1.0,
            direct: 0.55,
            low_cut: 40.0,
            high_cut: 12_000.0,
            seed: 11,
        },
        BodyKind::Violin => violin(1.0, 11_000.0, 21),
        BodyKind::Viola => violin(0.82, 9000.0, 31),
        BodyKind::Cello => violin(0.37, 6500.0, 41),
        BodyKind::DoubleBass => violin(0.22, 4000.0, 51),
    }
}

#[derive(Clone, Copy, Default)]
struct Block {
    y1: [f32; LANES],
    y2: [f32; LANES],
    a1: [f32; LANES],
    a2: [f32; LANES],
    b: [f32; LANES],
    left: [f32; LANES],
    right: [f32; LANES],
}

/// An open string ringing in sympathy: a comb filter at its pitch.
struct Sympathy {
    buf: Vec<f32>,
    pos: usize,
    delay: f32,
    gain: f32,
    lp: f32,
}

impl Sympathy {
    fn new(midi: f32, t60: f32, sr: f32) -> Sympathy {
        let f = 440.0 * 2f32.powf((midi - 69.0) / 12.0);
        let delay = sr / f - 0.5;
        Sympathy { buf: vec![0.0; delay as usize + 4], pos: 0, delay, gain: 10f32.powf(-3.0 / (t60 * f)), lp: 0.0 }
    }

    fn tick(&mut self, x: f32) -> f32 {
        let n = self.buf.len();
        if n < 4 {
            return 0.0;
        }
        let d = self.delay.clamp(1.0, (n - 2) as f32);
        let read = self.pos as f32 + n as f32 - d;
        let i0 = read.floor() as usize % n;
        let frac = read - read.floor();
        let y = self.buf.get(i0).copied().unwrap_or(0.0) * (1.0 - frac) + self.buf.get((i0 + 1) % n).copied().unwrap_or(0.0) * frac;
        self.lp = flush(self.lp + 0.6 * (y - self.lp));
        if let Some(slot) = self.buf.get_mut(self.pos) {
            *slot = flush(x + self.lp * self.gain);
        }
        self.pos = (self.pos + 1) % n;
        y
    }
}

pub struct Body {
    pub kind: BodyKind,
    blocks: Vec<Block>,
    x1: f32,
    x2: f32,
    direct: f32,
    wet: f32,
    low_cut: Biquad,
    high_cut: Biquad,
    sympathy: Vec<Sympathy>,
    /// A piano's strings for its pedal, with their loop gains free and damped.
    pedal_strings: Vec<(Sympathy, f32, f32)>,
    pedal: bool,
    /// Samples since the pedal was last down, so released strings are skipped once quiet.
    pedal_off: u32,
    /// Samples since anything came in, so a silent body costs nothing.
    idle: u32,
    sr: f32,
}

/// The level of the sympathetic strings' input and output.
const SYMPATHY_IN: f32 = 0.004;
const SYMPATHY_OUT: f32 = 0.35;
/// The piano keys whose strings resonate with the pedal down (A0 to C7), and how strongly.
const PEDAL_LOW: u8 = 21;
const PEDAL_HIGH: u8 = 96;
const PEDAL_IN: f32 = 0.003;

impl Body {
    pub fn new(kind: BodyKind, sr: f32) -> Body {
        let d = design(kind);
        let mut modes: Vec<(f32, f32, f32)> = Vec::new();
        for &(f, q, db) in d.signature {
            modes.push((f * d.scale, q, db));
        }
        let (lo, hi) = ((d.low * d.scale).log2(), (d.high * d.scale.sqrt()).log2());
        for i in 0..d.dense {
            let jitter = hash01(d.seed, i as u32) - 0.5;
            let at = lo + (hi - lo) * ((i as f32 + 0.5 + 0.8 * jitter) / d.dense as f32);
            let f = at.exp2();
            // The envelope is drawn for the violin's size; a larger body has it lower.
            let rel = at - d.scale.log2() * 0.6;
            let level = curve(d.envelope, rel) + 20.0 * (0.25 + 0.75 * hash01(d.seed + 1, i as u32)).log10();
            let q = curve(d.q, rel) * (0.7 + 0.6 * hash01(d.seed + 2, i as u32));
            modes.push((f, q, level));
        }
        let nyquist = sr * 0.45;
        let mut blocks = Vec::new();
        for (c, chunk) in modes.chunks(LANES).enumerate() {
            let mut b = Block::default();
            for (lane, &(f, q, db)) in chunk.iter().enumerate() {
                if f >= nyquist {
                    continue;
                }
                let w = std::f32::consts::TAU * f / sr;
                let r = (-w / (2.0 * q.max(1.0))).exp();
                let amp = 10f32.powf(db / 20.0);
                // Each mode radiates more to one side than the other, with its own phase, so the
                // two channels differ without cancelling when summed to mono.
                let i = (c * LANES + lane) as u32;
                let side = hash01(d.seed + 3, i) * 2.0 - 1.0;
                let sign = if hash01(d.seed + 4, i) < 0.5 { -1.0 } else { 1.0 };
                if let (Some(a1), Some(a2), Some(bb), Some(l), Some(rr)) =
                    (b.a1.get_mut(lane), b.a2.get_mut(lane), b.b.get_mut(lane), b.left.get_mut(lane), b.right.get_mut(lane))
                {
                    *a1 = 2.0 * r * w.cos();
                    *a2 = r * r;
                    *bb = (1.0 - r * r) / 2.0;
                    *l = amp * sign * (1.0 + 0.8 * side);
                    *rr = amp * sign * (1.0 - 0.8 * side);
                }
            }
            blocks.push(b);
        }
        let sympathy = kind.open_strings().iter().map(|&m| Sympathy::new(f32::from(m), 1.8, sr)).collect();
        // The piano's strings with the dampers off: a fresh string rings for many seconds low
        // down, one or two high up; with the pedal released the dampers stop them.
        let pedal_strings: Vec<(Sympathy, f32, f32)> = if kind == BodyKind::Piano {
            (PEDAL_LOW..=PEDAL_HIGH)
                .map(|m| {
                    let m = f32::from(m);
                    let t60 = curve(&[(21.0, 12.0), (60.0, 6.0), (96.0, 1.5)], m);
                    let free = Sympathy::new(m, t60, sr);
                    let up = free.gain;
                    let down = Sympathy::new(m, 0.15, sr).gain;
                    (free, up, down)
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut low_cut = Biquad::default();
        low_cut.set(Shape::HighPass, d.low_cut, 0.6, 0.0, sr);
        let mut high_cut = Biquad::default();
        high_cut.set(Shape::LowPass, d.high_cut, 0.6, 0.0, sr);
        let mut body = Body {
            kind,
            blocks,
            x1: 0.0,
            x2: 0.0,
            direct: d.direct,
            wet: 1.0,
            low_cut,
            high_cut,
            sympathy,
            pedal_strings,
            pedal: false,
            pedal_off: u32::MAX,
            idle: u32::MAX,
            sr,
        };
        body.wet = body.normalise();
        body
    }

    /// The gain that gives the modes unit power for white noise in, measured on their impulse
    /// response.
    fn normalise(&mut self) -> f32 {
        let n = (self.sr * 0.6) as usize;
        let (mut el, mut er) = (0.0f64, 0.0f64);
        for i in 0..n {
            let (l, r) = self.modes(if i == 0 { 1.0 } else { 0.0 });
            el += f64::from(l * l);
            er += f64::from(r * r);
        }
        self.reset();
        let e = ((el + er) * 0.5).sqrt() as f32;
        if e > 1e-9 { (1.0 - self.direct) / e } else { 0.0 }
    }

    fn reset(&mut self) {
        for b in &mut self.blocks {
            b.y1 = [0.0; LANES];
            b.y2 = [0.0; LANES];
        }
        (self.x1, self.x2) = (0.0, 0.0);
    }

    fn modes(&mut self, x: f32) -> (f32, f32) {
        let drive = x - self.x2;
        (self.x2, self.x1) = (self.x1, x);
        let (mut al, mut ar) = ([0.0f32; LANES], [0.0f32; LANES]);
        for b in &mut self.blocks {
            for ((((y1, y2), (a1, a2)), (bb, (l, r))), (sl, sr)) in
                b.y1.iter_mut()
                    .zip(b.y2.iter_mut())
                    .zip(b.a1.iter().zip(&b.a2))
                    .zip(b.b.iter().zip(b.left.iter().zip(&b.right)))
                    .zip(al.iter_mut().zip(ar.iter_mut()))
            {
                let y = bb * drive + a1 * *y1 - a2 * *y2;
                *y2 = *y1;
                *y1 = flush(y);
                *sl += l * y;
                *sr += r * y;
            }
        }
        (al.iter().sum(), ar.iter().sum())
    }

    /// The bridge's force in, left and right; the radiated sound out.
    pub fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let x = 0.5 * (l + r);
        if x == 0.0 && !self.pedal {
            // Nothing coming in: once the resonances have died away, skip them.
            self.idle = self.idle.saturating_add(1);
            if self.idle > (self.sr * 1.5) as u32 {
                return (0.0, 0.0);
            }
        } else {
            self.idle = 0;
        }
        let mut ring = 0.0;
        for s in &mut self.sympathy {
            ring += s.tick(x * SYMPATHY_IN);
        }
        if self.pedal_off < (self.sr * 0.6) as u32 {
            self.pedal_off = if self.pedal { 0 } else { self.pedal_off + 1 };
            let input = if self.pedal { x * PEDAL_IN } else { 0.0 };
            for (s, _, _) in &mut self.pedal_strings {
                ring += s.tick(input);
            }
        }
        let x = x + ring * SYMPATHY_OUT;
        let (ml, mr) = self.modes(x);
        let ol = self.direct * (l + ring * SYMPATHY_OUT) + self.wet * ml;
        let or = self.direct * (r + ring * SYMPATHY_OUT) + self.wet * mr;
        let ol = self.low_cut.tick(0, ol);
        let or = self.low_cut.tick(1, or);
        (self.high_cut.tick(0, ol), self.high_cut.tick(1, or))
    }

    /// The sustain pedal: down, every string rings with what is played; up, the dampers stop them.
    pub fn set_pedal(&mut self, down: bool) {
        if down == self.pedal || self.pedal_strings.is_empty() {
            return;
        }
        self.pedal = down;
        if down {
            self.pedal_off = 0;
        }
        for (s, free, damped) in &mut self.pedal_strings {
            s.gain = if down { *free } else { *damped };
        }
    }

    /// Whether the body has fallen silent.
    pub fn is_idle(&self) -> bool {
        self.idle > (self.sr * 1.5) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves_interpolate_and_hold() {
        let c = [(0.0, 1.0), (10.0, 3.0)];
        assert_eq!(curve(&c, -5.0), 1.0);
        assert_eq!(curve(&c, 5.0), 2.0);
        assert_eq!(curve(&c, 50.0), 3.0);
        assert_eq!(curve(&[], 1.0), 0.0);
    }

    #[test]
    fn bodies_pass_noise_at_about_unit_power_and_go_quiet() {
        for kind in BodyKind::ALL {
            let mut b = Body::new(kind, 48_000.0);
            let mut noise = crate::synth::Noise::new(3);
            let n = 48_000;
            let (mut ein, mut eout) = (0.0f32, 0.0f32);
            for _ in 0..n {
                let x = noise.next();
                let (l, r) = b.tick(x, x);
                assert!(l.is_finite() && r.is_finite());
                ein += x * x;
                eout += 0.5 * (l * l + r * r);
            }
            let gain = (eout / ein).sqrt();
            println!("{kind:?}: power gain {gain:.3}");
            assert!((0.2..2.0).contains(&gain), "{kind:?}: {gain}");
            for _ in 0..(48_000 * 2) {
                b.tick(0.0, 0.0);
            }
            assert!(b.is_idle());
        }
    }

    #[test]
    fn a_pedalled_chord_leaves_a_halo_the_dampers_stop() {
        // A second of a C major chord into the soundboard, then silence: with the pedal down the
        // other strings ring on, quieter than the chord; with it up, they barely sound.
        let tail = |pedal: bool| {
            let mut b = Body::new(BodyKind::Piano, 48_000.0);
            b.set_pedal(pedal);
            let sr = 48_000.0;
            let (mut during, mut after) = (0.0f32, 0.0f32);
            for i in 0..96_000 {
                let t = i as f32 / sr;
                let x = if i < 48_000 { [130.8f32, 164.8, 196.0, 261.6].iter().map(|f| (std::f32::consts::TAU * f * t).sin() * 0.2).sum() } else { 0.0 };
                let (l, r) = b.tick(x, x);
                if i < 48_000 {
                    during += l * l + r * r;
                } else if i > 52_800 {
                    after += l * l + r * r;
                }
            }
            10.0 * (after / during).log10()
        };
        let (down, up) = (tail(true), tail(false));
        println!("halo: pedal down {down:.1} dB, up {up:.1} dB");
        assert!((-40.0..-8.0).contains(&down), "{down}");
        assert!(up < down - 10.0, "{up} {down}");
    }
}
