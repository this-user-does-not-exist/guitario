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
    pub(crate) fn open_strings(self) -> &'static [u8] {
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
    /// (frequency, Q, level in dB above the envelope).
    signature: &'static [(f32, f32, f32)],
    /// How many dense modes, from where to where, and how they are spread: evenly in log
    /// frequency at `spacing` 0, evenly in frequency at 1. A wooden box has a few modes well
    /// apart low down and more and more, overlapping, higher up, so its modes are spread nearer
    /// evenly in frequency; spread evenly in log frequency they crowd its lower octaves and make
    /// its response smooth there, where a real one is jagged.
    dense: usize,
    low: f32,
    high: f32,
    spacing: f32,
    /// The weakest a dense mode may be against the envelope (a share of its amplitude): the
    /// smaller, the more jagged the response.
    floor: f32,
    /// The body's average response, dB against log2 of frequency: the dense modes' level, and
    /// what the signature modes stand out from. Each instrument's is fitted to recordings of it
    /// (see [`VIOLIN_ENVELOPE`]).
    envelope: &'static [(f32, f32)],
    /// Q of the dense modes against log2 of frequency.
    q: &'static [(f32, f32)],
    /// Dense modes scaled by this in frequency (a viola is a larger violin).
    scale: f32,
    /// How much of the bridge's force reaches the room directly, against through the modes.
    direct: f32,
    /// The bottom, below which the body hardly radiates: second-order high-passes (frequency at
    /// the violin's size, Q) in a row. And the top.
    low_cut: &'static [(f32, f32)],
    high_cut: f32,
    /// Its output's level in dB, so the instruments keep the balance they had before their bodies
    /// were fitted to recordings.
    level: f32,
    seed: u32,
}

const VIOLIN_MODES: [(f32, f32, f32); 6] =
    [(275.0, 14.0, 0.0), (405.0, 25.0, -9.0), (460.0, 22.0, -5.0), (470.0, 28.0, 1.0), (545.0, 26.0, 3.0), (650.0, 30.0, -6.0)];
const VIOLIN_ENVELOPE: [(f32, f32); 18] = [
    (7.97, 3.1),
    (8.30, 1.9),
    (8.63, -3.2),
    (8.97, -3.3),
    (9.30, -0.9),
    (9.63, 5.1),
    (9.97, 8.1),
    (10.30, 3.2),
    (10.63, -1.9),
    (10.97, -2.4),
    (11.30, 5.4),
    (11.63, 8.8),
    (11.97, 0.9),
    (12.30, -5.2),
    (12.63, -9.2),
    (12.97, -29.7),
    (13.30, -30.0),
    (13.63, 10.7),
];
const VIOLA_ENVELOPE: [(f32, f32); 19] = [
    (7.68, 6.8),
    (8.01, 0.0),
    (8.35, -5.2),
    (8.68, -2.1),
    (9.01, 7.1),
    (9.35, 8.3),
    (9.68, 1.2),
    (10.01, 4.2),
    (10.35, 8.9),
    (10.68, 1.4),
    (11.01, -4.4),
    (11.35, -0.4),
    (11.68, 0.6),
    (12.01, -11.3),
    (12.35, -19.6),
    (12.68, -17.6),
    (13.01, -28.6),
    (13.35, -19.4),
    (13.68, 14.1),
];
const CELLO_ENVELOPE: [(f32, f32); 19] = [
    (6.64, -30.0),
    (6.98, -30.0),
    (7.31, 14.4),
    (7.64, 4.2),
    (7.98, 2.9),
    (8.31, 12.8),
    (8.64, 7.8),
    (8.98, 1.2),
    (9.31, 4.1),
    (9.64, 0.4),
    (9.98, -3.1),
    (10.31, 0.0),
    (10.64, -0.7),
    (10.98, 0.2),
    (11.31, -3.5),
    (11.64, -15.6),
    (11.98, -22.3),
    (12.31, -22.6),
    (12.64, -13.1),
];
const BASS_ENVELOPE: [(f32, f32); 21] = [
    (5.78, -30.0),
    (6.11, 4.3),
    (6.45, 15.0),
    (6.78, 9.3),
    (7.11, 12.1),
    (7.45, 15.0),
    (7.78, 6.1),
    (8.11, -9.0),
    (8.45, 5.3),
    (8.78, 15.0),
    (9.11, 5.1),
    (9.45, -9.0),
    (9.78, -8.7),
    (10.11, 0.0),
    (10.45, 2.7),
    (10.78, -7.5),
    (11.11, -15.5),
    (11.45, -22.2),
    (11.78, -29.1),
    (12.11, -20.9),
    (12.45, -10.4),
];
/// Below the air mode the box hardly radiates: the air through the f-holes and the plates move
/// against each other and cancel, and the radiation falls off a cliff. Recorded violins have the
/// fundamental of their open G 21 to 30 dB under their strongest harmonic, of A3 12 dB and of C4
/// none: a sixth-order Chebyshev high-pass (1 dB ripple) cornered at 240 Hz, its sharpest stage
/// a resonance just under A0.
const VIOLIN_LOW_CUT: [(f32, f32); 3] = [(679.6, 0.76), (321.4, 2.2), (241.1, 8.0)];
const VIOLIN_Q: [(f32, f32); 4] = [(8.0, 40.0), (10.0, 45.0), (11.5, 45.0), (13.0, 30.0)];

fn design(kind: BodyKind) -> Design {
    // The violin family share a shape, scaled: the viola's body is about 15 % larger, the cello's
    // and the double bass's much more so, with their bridge hills lower but not as much lower.
    let violin = |scale: f32, envelope: &'static [(f32, f32)], high_cut: f32, level: f32, seed: u32| Design {
        signature: &VIOLIN_MODES,
        dense: 88,
        low: 300.0,
        high: 12_000.0,
        spacing: 0.6,
        floor: 0.04,
        envelope,
        q: &VIOLIN_Q,
        scale,
        direct: 0.03,
        low_cut: &VIOLIN_LOW_CUT,
        high_cut,
        level,
        seed,
    };
    match kind {
        BodyKind::Piano => Design {
            signature: &[],
            dense: 72,
            low: 45.0,
            high: 6500.0,
            spacing: 0.0,
            floor: 0.25,
            envelope: &[(5.5, -6.0), (6.6, 2.0), (8.6, 1.0), (10.5, -1.0), (12.0, -5.0), (12.7, -12.0)],
            q: &[(5.5, 30.0), (9.0, 26.0), (12.5, 16.0)],
            scale: 1.0,
            direct: 0.55,
            low_cut: &[(40.0, 0.6)],
            high_cut: 12_000.0,
            level: 0.0,
            seed: 11,
        },
        BodyKind::Violin => violin(1.0, &VIOLIN_ENVELOPE, 15_000.0, 1.7, 21),
        BodyKind::Viola => violin(0.82, &VIOLA_ENVELOPE, 14_000.0, -1.8, 31),
        BodyKind::Cello => violin(0.37, &CELLO_ENVELOPE, 12_000.0, -2.0, 41),
        BodyKind::DoubleBass => violin(0.22, &BASS_ENVELOPE, 9000.0, -6.1, 51),
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

/// The low-pass in a sympathetic string's loop, `y += a (x - y)`.
const STRING_LP: f32 = 0.6;

impl Sympathy {
    /// A string at `f` Hz ringing for `t60` seconds. The loop's delay is shortened by the low-pass's
    /// own delay at `f`, so the string resonates at its pitch, not a few cents (tens, high up) flat.
    fn new(f: f32, t60: f32, sr: f32) -> Sympathy {
        let f = f.clamp(20.0, sr * 0.2);
        let w = std::f32::consts::TAU * f / sr;
        let b = 1.0 - STRING_LP;
        let lp_delay = (b * w.sin()).atan2(1.0 - b * w.cos()) / w;
        let delay = (sr / f - lp_delay).max(2.0);
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
        self.lp = flush(self.lp + STRING_LP * (y - self.lp));
        if let Some(slot) = self.buf.get_mut(self.pos) {
            *slot = flush(x + self.lp * self.gain);
        }
        self.pos = (self.pos + 1) % n;
        y
    }
}

/// The soundboard and case as a diffuse resonator: eight delay lines a few milliseconds long,
/// mixed into each other (a Householder feedback delay network) and losing their highs faster
/// than their lows. Above a few hundred hertz a soundboard's modes are too dense to hear one by
/// one; what is heard is this short wooden bloom under the strings, which a few dozen discrete
/// resonances alone do not give (they ring, and sound synthetic).
struct Board {
    lines: Vec<(Vec<f32>, usize)>,
    /// Per line: loop gain at low frequencies, and the low-pass that takes the highs off sooner.
    gain: Vec<f32>,
    damp: Vec<f32>,
    state: Vec<f32>,
}

/// Line lengths in milliseconds, spread so the echoes never line up.
const BOARD_MS: [f32; 8] = [2.9, 3.7, 4.6, 5.9, 7.1, 8.8, 10.7, 13.3];

impl Board {
    fn new(sr: f32, t60_low: f32, highs_faster: f32) -> Board {
        let lens: Vec<usize> = BOARD_MS.iter().map(|ms| ((ms * 0.001 * sr) as usize).max(2)).collect();
        let gain = lens.iter().map(|&n| 10f32.powf(-3.0 * n as f32 / (t60_low * sr))).collect();
        // A one-pole in each loop, stronger in the longer lines: the highs die `highs_faster` times
        // as fast, roughly.
        let damp = lens.iter().map(|&n| ((1.0 - highs_faster.recip()) * (n as f32 / (0.01 * sr)).min(1.0)).clamp(0.0, 0.95)).collect();
        Board { lines: lens.iter().map(|&n| (vec![0.0; n], 0)).collect(), gain, damp, state: vec![0.0; 8] }
    }

    fn tick(&mut self, x: f32) -> (f32, f32) {
        let mut outs = [0.0f32; 8];
        for ((o, (buf, pos)), (g, (d, st))) in outs.iter_mut().zip(&self.lines).zip(self.gain.iter().zip(self.damp.iter().zip(self.state.iter_mut()))) {
            let y = buf.get(*pos).copied().unwrap_or(0.0);
            *st = flush(y * (1.0 - d) + *st * d);
            *o = *st * g;
        }
        // Householder mixing: each line takes the input and every line's output less twice the mean.
        let mean = outs.iter().sum::<f32>() * (2.0 / 8.0);
        for ((buf, pos), o) in self.lines.iter_mut().zip(&outs) {
            if let Some(slot) = buf.get_mut(*pos) {
                *slot = flush(x + o - mean);
            }
            *pos = (*pos + 1) % buf.len().max(1);
        }
        let [a, b, c, d, e, f, g, h] = outs;
        (a - c + e - g, b - d + f - h)
    }
}

pub struct Body {
    pub kind: BodyKind,
    /// Which desk of a section it is, if any (see [`Body::at_desk`]).
    pub desk: Option<u8>,
    /// Its output's gains to the left and right, and for a desk, what its far side hears (which
    /// side, 0 left or 1 right, and through what).
    lean: (f32, f32),
    far: Option<(usize, Diffuser)>,
    blocks: Vec<Block>,
    x1: f32,
    x2: f32,
    direct: f32,
    wet: f32,
    low_cut: Vec<Biquad>,
    high_cut: Biquad,
    sympathy: Vec<Sympathy>,
    /// A piano's strings for its pedal, with their loop gains free and damped.
    pedal_strings: Vec<(Sympathy, f32, f32)>,
    /// The piano's diffuse soundboard and case, and its level.
    board: Option<Board>,
    board_level: f32,
    pedal: bool,
    /// Samples since the pedal was last down, so released strings are skipped once quiet.
    pedal_off: u32,
    /// Samples since anything came in, so a silent body costs nothing.
    idle: u32,
    sr: f32,
}

/// How far a section's desks sit to either side (radians from the centre of a pan law).
const DESK_LEAN: f32 = 0.45;

/// What reaches the far side of a section from one of its desks: the same sound by other paths
/// round the room, a few milliseconds of all-passes (Schroeder's), so its phase against the near
/// side wanders with frequency and the two sides decorrelate as recorded sections' do (within
/// about 0.2 of uncorrelated in every octave from 250 Hz up), its level untouched.
struct Diffuser {
    stages: Vec<(Vec<f32>, usize)>,
}

const DIFFUSER_MS: [[f32; 3]; 2] = [[0.43, 1.07, 2.31], [0.61, 1.39, 1.93]];
const DIFFUSER_GAIN: f32 = -0.6;

impl Diffuser {
    fn new(sr: f32, desk: u32) -> Diffuser {
        let ms = DIFFUSER_MS.get(desk as usize % 2).copied().unwrap_or_default();
        Diffuser { stages: ms.iter().map(|m| (vec![0.0; ((m * 0.001 * sr) as usize).max(1)], 0)).collect() }
    }

    fn tick(&mut self, x: f32) -> f32 {
        let mut y = x;
        for (line, pos) in &mut self.stages {
            let n = line.len().max(1);
            let old = line.get(*pos).copied().unwrap_or(0.0);
            let v = flush(y + DIFFUSER_GAIN * old);
            if let Some(slot) = line.get_mut(*pos) {
                *slot = v;
            }
            *pos = (*pos + 1) % n;
            y = old - DIFFUSER_GAIN * v;
        }
        y
    }
}

/// The level of the sympathetic strings' input and output.
const SYMPATHY_IN: f32 = 0.004;
const SYMPATHY_OUT: f32 = 0.35;
/// The soundboard's bloom: how long it rings low down, how much faster the highs go, and its level
/// against the strings' direct sound (power gain for noise in).
const BOARD_T60: f32 = 0.45;
const BOARD_HIGHS: f32 = 5.0;
const BOARD_LEVEL: f32 = 0.5;
/// The piano keys whose strings resonate with the pedal down (A0 to C7), and how strongly.
const PEDAL_LOW: u8 = 21;
const PEDAL_HIGH: u8 = 96;
const PEDAL_IN: f32 = 0.003;

impl Body {
    #[cfg(test)]
    pub fn new(kind: BodyKind, sr: f32) -> Body {
        Body::at_desk(kind, sr, None)
    }

    /// One desk of a section: the players on its left (desk 0) or its right (desk 1), sounding
    /// through instruments of their own (modes placed and weighted differently) and heard from
    /// their side. Recorded string sections are as good as uncorrelated between left and right
    /// (0.05), where a single player through a single body is not (0.7).
    pub fn at_desk(kind: BodyKind, sr: f32, desk: Option<u8>) -> Body {
        let d = design(kind);
        // The desks' instruments share the modes the envelope was fitted to (another draw of
        // them would have another average response) and radiate them differently.
        let radiate = d.seed.wrapping_add(desk.map_or(0, |k| 1000 * (u32::from(k) + 1)));
        let angle = std::f32::consts::FRAC_PI_4 + desk.map_or(0.0, |k| if k == 0 { -DESK_LEAN } else { DESK_LEAN });
        let level = 10f32.powf(d.level / 20.0);
        let lean = (angle.cos() * std::f32::consts::SQRT_2 * level, angle.sin() * std::f32::consts::SQRT_2 * level);
        let mut modes: Vec<(f32, f32, f32)> = Vec::new();
        // The signature modes stand out from the envelope by their level.
        for &(f, q, db) in d.signature {
            modes.push((f * d.scale, q, db + curve(d.envelope, (f * d.scale).max(1.0).log2())));
        }
        let (lo, hi) = (d.low * d.scale, d.high * d.scale.sqrt());
        let p = d.spacing.clamp(0.0, 1.0);
        for i in 0..d.dense {
            let jitter = hash01(d.seed, i as u32) - 0.5;
            let t = (i as f32 + 0.5 + 0.8 * jitter) / d.dense as f32;
            let f = if p > 0.0 { (lo.powf(p) + t * (hi.powf(p) - lo.powf(p))).powf(p.recip()) } else { lo * (hi / lo).powf(t) };
            let at = f.max(1.0).log2();
            // The modes' Q is drawn for the violin's size; a larger body has it lower.
            let rel = at - d.scale.log2() * 0.6;
            let level = curve(d.envelope, at) + 20.0 * (d.floor + (1.0 - d.floor) * hash01(d.seed + 1, i as u32)).log10();
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
                let side = hash01(radiate + 3, i) * 2.0 - 1.0;
                let sign = if hash01(radiate + 4, i) < 0.5 { -1.0 } else { 1.0 };
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
        let sympathy = kind.open_strings().iter().map(|&m| Sympathy::new(440.0 * 2f32.powf((f32::from(m) - 69.0) / 12.0), 1.8, sr)).collect();
        // The piano's strings with the dampers off: a fresh string rings for many seconds low
        // down, one or two high up; with the pedal released the dampers stop them.
        let pedal_strings: Vec<(Sympathy, f32, f32)> = if kind == BodyKind::Piano {
            (PEDAL_LOW..=PEDAL_HIGH)
                .map(|m| {
                    let m = f32::from(m);
                    let t60 = curve(&[(21.0, 12.0), (60.0, 6.0), (96.0, 1.5)], m);
                    // Tuned as the piano's own keys are, stretch and all.
                    let f = crate::piano::tuned(m);
                    let free = Sympathy::new(f, t60, sr);
                    let up = free.gain;
                    let down = Sympathy::new(f, 0.15, sr).gain;
                    (free, up, down)
                })
                .collect()
        } else {
            Vec::new()
        };
        let low_cut = d
            .low_cut
            .iter()
            .map(|&(f, q)| {
                let mut b = Biquad::default();
                b.set(Shape::HighPass, f * d.scale, q, 0.0, sr);
                b
            })
            .collect();
        let mut high_cut = Biquad::default();
        high_cut.set(Shape::LowPass, d.high_cut, 0.6, 0.0, sr);
        let far = desk.map(|k| (if k == 0 { 1 } else { 0 }, Diffuser::new(sr, u32::from(k))));
        let mut body = Body {
            kind,
            desk,
            lean,
            far,
            blocks,
            x1: 0.0,
            x2: 0.0,
            direct: d.direct,
            wet: 1.0,
            low_cut,
            high_cut,
            sympathy,
            pedal_strings,
            board: None,
            board_level: 0.0,
            pedal: false,
            pedal_off: u32::MAX,
            idle: u32::MAX,
            sr,
        };
        body.wet = body.normalise();
        if kind == BodyKind::Piano {
            // Unit power for white noise in, measured on its impulse response like the modes',
            // then set under the strings.
            let mut board = Board::new(sr, BOARD_T60, BOARD_HIGHS);
            let n = (sr * 0.8) as usize;
            let e: f64 = (0..n).map(|i| board.tick(if i == 0 { 1.0 } else { 0.0 })).map(|(l, r)| f64::from(l * l + r * r) * 0.5).sum();
            body.board_level = if e > 1e-12 { BOARD_LEVEL / e.sqrt() as f32 } else { 0.0 };
            body.board = Some(Board::new(sr, BOARD_T60, BOARD_HIGHS));
        }
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
        let (bl, br) = match self.board.as_mut() {
            Some(b) => {
                let (bl, br) = b.tick(x);
                (bl * self.board_level, br * self.board_level)
            }
            None => (0.0, 0.0),
        };
        let ol = self.direct * (l + ring * SYMPATHY_OUT) + self.wet * ml + bl;
        let or = self.direct * (r + ring * SYMPATHY_OUT) + self.wet * mr + br;
        let (mut ol, mut or) = (ol, or);
        for b in &mut self.low_cut {
            ol = b.tick(0, ol);
            or = b.tick(1, or);
        }
        let (mut ol, mut or) = (self.high_cut.tick(0, ol) * self.lean.0, self.high_cut.tick(1, or) * self.lean.1);
        if let Some((side, diffuser)) = self.far.as_mut() {
            let x = if *side == 0 { &mut ol } else { &mut or };
            *x = diffuser.tick(*x);
        }
        (ol, or)
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

    #[test]
    fn sympathetic_strings_ring_at_their_pitch() {
        // An impulse into one string; its strongest response within 60 cents below to 20 above its
        // pitch must be within a cent of it, at the bottom and the top of the piano's pedal strings.
        for m in [33.0f32, 60.0, 84.0, 96.0] {
            let f = crate::piano::tuned(m);
            let mut s = Sympathy::new(f, 4.0, 48_000.0);
            let x: Vec<f32> = (0..48_000).map(|i| s.tick(if i == 0 { 1.0 } else { 0.0 })).collect();
            let power = |c: f32| {
                let w = std::f32::consts::TAU * f * 2f32.powf(c / 1200.0) / 48_000.0;
                let (re, im) = x.iter().enumerate().fold((0.0f32, 0.0f32), |(re, im), (i, v)| (re + v * (w * i as f32).cos(), im + v * (w * i as f32).sin()));
                re * re + im * im
            };
            let best = (-120..=40).map(|c| c as f32 * 0.5).max_by(|a, b| power(*a).total_cmp(&power(*b))).unwrap();
            assert!(best.abs() <= 1.0, "key {m}: {best} cents");
        }
    }
}
