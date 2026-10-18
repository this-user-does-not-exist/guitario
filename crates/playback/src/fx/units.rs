//! The effect models. Each takes its settings from an [`Effect`] and processes stereo samples.

use super::filter::{Biquad, DelayLine, Envelope, OnePole, Shape, db_to_gain, flush};
use guitario_score::{Effect, EffectType, GRAPHIC_BANDS};
use std::f32::consts::PI;

/// A knob from 0 to 10 as a fraction.
fn frac(e: &Effect, name: &str) -> f32 {
    (e.value(name) / 10.0).clamp(0.0, 1.0)
}

/// A knob centred on 5 as ±`range` decibels.
fn knob_db(e: &Effect, name: &str, range: f32) -> f32 {
    (e.value(name) - 5.0) / 5.0 * range
}

/// The dry/wet blend: the dry signal fades by half as the wet one comes in.
fn blend(dry: f32, wet: f32, mix: f32) -> f32 {
    dry * (1.0 - 0.5 * mix) + wet * mix
}

/// A sine oscillator.
#[derive(Clone, Copy, Debug, Default)]
struct Lfo {
    phase: f32,
    step: f32,
}

impl Lfo {
    fn set(&mut self, hz: f32, sr: f32) {
        self.step = 2.0 * PI * hz.max(0.0) / sr.max(1.0);
    }

    fn tick(&mut self, offset: f32) -> f32 {
        self.phase = (self.phase + self.step) % (2.0 * PI);
        (self.phase + offset).sin()
    }
}

pub(crate) struct Compressor {
    env: Envelope,
    threshold: f32,
    slope: f32,
    makeup: f32,
}

impl Compressor {
    fn set(&mut self, e: &Effect, sr: f32) {
        self.env.set(e.value("attack"), e.value("release"), sr);
        self.threshold = e.value("threshold");
        self.slope = 1.0 - 1.0 / e.value("ratio").max(1.0);
        self.makeup = e.value("gain");
    }

    /// The gain for a detector input.
    fn gain(&mut self, peak: f32) -> f32 {
        let level = self.env.tick(peak);
        let over = 20.0 * level.max(1e-6).log10() - self.threshold;
        db_to_gain(self.makeup - over.max(0.0) * self.slope)
    }
}

pub(crate) struct Gate {
    env: Envelope,
    open: f32,
    threshold: f32,
    close: f32,
    attack: f32,
}

impl Gate {
    fn set(&mut self, e: &Effect, sr: f32) {
        self.env.set(0.5, 20.0, sr);
        self.threshold = db_to_gain(e.value("threshold"));
        self.close = (-1.0 / (e.value("release").max(1.0) * 0.001 * sr)).exp();
        self.attack = (-1.0 / (0.0005 * sr)).exp();
    }

    fn gain(&mut self, peak: f32) -> f32 {
        let level = self.env.tick(peak);
        let target = if level > self.threshold { 1.0 } else { 0.0 };
        let c = if target > self.open { self.attack } else { self.close };
        self.open = flush(target + c * (self.open - target));
        self.open
    }
}

/// A rational approximation of tanh (within 2% everywhere, exact at 0 and ±∞), several times
/// cheaper than the library's.
pub(crate) fn fast_tanh(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    let x2 = x * x;
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

/// A soft clipper that leans one way, as a tube does, so even harmonics come in.
fn tube(x: f32, bias: f32) -> f32 {
    fast_tanh(x + bias) - fast_tanh(bias)
}

/// The cubic soft clip: linear-ish in the middle, flat past ±1.
fn cubic(x: f32) -> f32 {
    let x = x.clamp(-1.0, 1.0);
    1.5 * (x - x * x * x / 3.0)
}

/// An overdrive (a mid push into a soft clip) or a distortion (a harder clip, more gain).
pub(crate) struct Drive {
    hard: bool,
    pre: Biquad,
    tone: Biquad,
    post: Biquad,
    gain: f32,
    level: f32,
}

impl Drive {
    fn set(&mut self, e: &Effect, sr: f32) {
        let d = e.value("drive");
        let tone = 900.0 * 2f32.powf(frac(e, "tone") * 3.0);
        if self.hard {
            self.pre.set(Shape::HighPass, 180.0, 0.707, 0.0, sr);
            self.gain = 4.0 + d * d * 4.0;
        } else {
            self.pre.set(Shape::Peak, 720.0, 0.8, 8.0, sr);
            self.gain = 1.5 + d * d * 1.2;
        }
        self.tone.set(Shape::LowPass, tone, 0.707, 0.0, sr);
        self.post.set(Shape::HighPass, 40.0, 0.707, 0.0, sr);
        self.level = db_to_gain(knob_db(e, "level", 12.0)) * if self.hard { 0.35 } else { 0.5 };
    }

    fn chan(&mut self, c: usize, x: f32) -> f32 {
        let x = self.pre.tick(c, x) * self.gain;
        let y = if self.hard { cubic(fast_tanh(x) * 1.2) } else { tube(x, 0.15) };
        self.post.tick(c, self.tone.tick(c, y)) * self.level
    }
}

/// A preamp (one to three clipping stages), a three-band tone stack, a power amp and presence.
pub(crate) struct Amp {
    stages: usize,
    input: Biquad,
    between: [Biquad; 3],
    fizz: Biquad,
    stage_gain: f32,
    bass: Biquad,
    mid: Biquad,
    treble: Biquad,
    presence: Biquad,
    out: f32,
}

impl Amp {
    fn set(&mut self, e: &Effect, sr: f32) {
        let model = e.value("model").round() as usize;
        let g = frac(e, "gain");
        let (stages, tighten, gain, mid_hz, out) = match model {
            1 => (2, 90.0, 1.0 + 9.0 * g * g, 700.0, 0.75),
            2 => (3, 130.0, 1.0 + 14.0 * g * g, 650.0, 0.65),
            3 => (1, 30.0, 1.0 + 3.0 * g, 500.0, 0.8),
            _ => (1, 60.0, 1.0 + 2.5 * g, 800.0, 0.8),
        };
        self.stages = stages;
        self.stage_gain = gain;
        self.input.set(Shape::HighPass, tighten, 0.707, 0.0, sr);
        for f in &mut self.between {
            f.set(Shape::HighPass, 35.0, 0.707, 0.0, sr);
        }
        self.fizz.set(Shape::LowPass, if stages > 1 { 7000.0 } else { 12000.0 }, 0.707, 0.0, sr);
        self.bass.set(Shape::LowShelf, 120.0, 0.707, knob_db(e, "bass", 12.0), sr);
        self.mid.set(Shape::Peak, mid_hz, 0.8, knob_db(e, "mid", 10.0), sr);
        self.treble.set(Shape::HighShelf, 2800.0, 0.707, knob_db(e, "treble", 12.0), sr);
        self.presence.set(Shape::HighShelf, 5000.0, 0.707, knob_db(e, "presence", 8.0), sr);
        self.out = out * db_to_gain(knob_db(e, "master", 12.0));
    }

    fn chan(&mut self, c: usize, x: f32) -> f32 {
        let mut y = self.input.tick(c, x);
        for f in self.between.iter_mut().take(self.stages) {
            y = f.tick(c, tube(y * self.stage_gain, 0.2));
        }
        y = self.fizz.tick(c, y);
        y = self.treble.tick(c, self.mid.tick(c, self.bass.tick(c, y)));
        // The power amp: a gentle squash before presence.
        y = fast_tanh(y * 1.2) / 1.2;
        self.presence.tick(c, y) * self.out
    }
}

/// A speaker cabinet as a set of filters: low resonance, cone break-up and a steep top roll-off.
pub(crate) struct Cabinet {
    bands: [Biquad; 6],
}

impl Cabinet {
    fn set(&mut self, e: &Effect, sr: f32) {
        // High-pass (Hz, Q), low bump, low-mid notch, presence peak, roll-off.
        let (hp, hp_q, bump, notch, peak, top) = match e.value("model").round() as usize {
            0 => (90.0, 0.8, (140.0, 1.0), (500.0, -1.0), (2200.0, 3.0), 5500.0),
            2 => (75.0, 1.3, (110.0, 3.0), (450.0, -3.0), (2800.0, 4.0), 4500.0),
            3 => (45.0, 0.9, (80.0, 2.0), (350.0, -2.0), (1500.0, 2.0), 4000.0),
            _ => (85.0, 1.0, (120.0, 2.0), (480.0, -2.0), (2500.0, 3.0), 5000.0),
        };
        // From the cone's centre (bright) to its edge (darker).
        let mic = frac(e, "mic");
        let top = top * (1.3 - 0.6 * mic);
        let [a, b, c, d, f, g] = &mut self.bands;
        a.set(Shape::HighPass, hp, hp_q, 0.0, sr);
        b.set(Shape::Peak, bump.0, 1.0, bump.1, sr);
        c.set(Shape::Peak, notch.0, 1.2, notch.1, sr);
        d.set(Shape::Peak, peak.0, 1.5, peak.1 - 3.0 * mic, sr);
        f.set(Shape::LowPass, top, 0.707, 0.0, sr);
        g.set(Shape::LowPass, top * 1.15, 0.9, 0.0, sr);
    }

    fn chan(&mut self, c: usize, x: f32) -> f32 {
        self.bands.iter_mut().fold(x, |y, b| b.tick(c, y))
    }
}

/// Peaks and shelves in series, then a level: the graphic and mixer equalizers.
pub(crate) struct Eq {
    bands: Vec<Biquad>,
    level: f32,
    flat: bool,
}

impl Eq {
    fn set(&mut self, e: &Effect, sr: f32) {
        self.flat = e.is_neutral();
        let shapes: Vec<(Shape, f32, f32, f32)> = match e.kind {
            EffectType::MixerEq => vec![
                (Shape::LowShelf, 100.0, 0.707, e.value("low")),
                (Shape::Peak, e.value("lowMidFreq"), 1.0, e.value("lowMid")),
                (Shape::Peak, e.value("midFreq"), e.value("midQ"), e.value("mid")),
                (Shape::Peak, e.value("highMidFreq"), 1.0, e.value("highMid")),
                (Shape::HighShelf, 8000.0, 0.707, e.value("high")),
            ],
            _ => GRAPHIC_BANDS
                .iter()
                .zip(e.kind.params())
                .map(|(hz, spec)| {
                    let shape = match *hz as u32 {
                        31 => Shape::LowShelf,
                        16000 => Shape::HighShelf,
                        _ => Shape::Peak,
                    };
                    (shape, *hz, 1.4, e.value(spec.name))
                })
                .collect(),
        };
        self.level = if e.kind == EffectType::Equalizer { db_to_gain(e.value("level")) } else { 1.0 };
        self.bands.resize(shapes.len(), Biquad::default());
        for (b, (shape, hz, q, gain)) in self.bands.iter_mut().zip(shapes) {
            b.set(shape, hz, q, gain, sr);
        }
    }

    fn chan(&mut self, c: usize, x: f32) -> f32 {
        if self.flat {
            return x;
        }
        self.bands.iter_mut().fold(x, |y, b| b.tick(c, y)) * self.level
    }
}

pub(crate) struct Chorus {
    lines: [DelayLine; 2],
    lfo: Lfo,
    sr: f32,
    depth: f32,
    mix: f32,
}

impl Chorus {
    fn set(&mut self, e: &Effect, sr: f32) {
        self.lfo.set(e.value("rate"), sr);
        self.depth = frac(e, "depth") * 0.006 * sr;
        self.mix = e.value("mix") / 100.0;
    }

    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let base = 0.008 * self.sr;
        let a = self.lfo.tick(0.0);
        let b = (self.lfo.phase + 0.5 * PI).sin();
        let [dl, dr] = &mut self.lines;
        dl.push(l);
        dr.push(r);
        let wl = dl.read(base + self.depth * 0.5 * (1.0 + a));
        let wr = dr.read(base + self.depth * 0.5 * (1.0 + b));
        (blend(l, wl, self.mix), blend(r, wr, self.mix))
    }
}

pub(crate) struct Tremolo {
    lfo: Lfo,
    depth: f32,
}

impl Tremolo {
    fn gain(&mut self) -> f32 {
        1.0 - self.depth * 0.5 * (1.0 - self.lfo.tick(0.0))
    }
}

pub(crate) struct Delay {
    lines: [DelayLine; 2],
    tone: [OnePole; 2],
    sr: f32,
    /// The delay in samples now and where it glides to, so turning the time knob does not click.
    now: f32,
    target: f32,
    feedback: f32,
    mix: f32,
}

impl Delay {
    fn set(&mut self, e: &Effect, sr: f32) {
        self.target = e.value("time") * 0.001 * sr;
        if self.now <= 0.0 {
            self.now = self.target;
        }
        self.feedback = e.value("feedback") / 100.0;
        self.mix = e.value("mix") / 100.0;
        let hz = 1200.0 * 2f32.powf(frac(e, "tone") * 3.3);
        for t in &mut self.tone {
            t.set(hz, sr);
        }
    }

    /// Moves the delay time one sample towards the knob's.
    fn glide(&mut self) {
        self.now += (self.target - self.now) * (5.0 / self.sr.max(1.0));
    }

    fn chan(&mut self, c: usize, x: f32) -> f32 {
        let (Some(line), Some(tone)) = (self.lines.get_mut(c), self.tone.get_mut(c)) else { return x };
        let wet = line.read(self.now);
        line.push(x + tone.tick(wet) * self.feedback);
        blend(x, wet, self.mix)
    }
}

/// A feedback comb with a low-pass in its loop.
struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn tick(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let Some(slot) = self.buf.get_mut(self.pos) else { return 0.0 };
        let out = *slot;
        self.store = flush(out * (1.0 - damp) + self.store * damp);
        *slot = flush(x + self.store * feedback);
        self.pos = (self.pos + 1) % self.buf.len().max(1);
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

impl Allpass {
    fn tick(&mut self, x: f32) -> f32 {
        let Some(slot) = self.buf.get_mut(self.pos) else { return x };
        let held = *slot;
        *slot = flush(x + held * 0.5);
        self.pos = (self.pos + 1) % self.buf.len().max(1);
        held - x
    }
}

/// Comb and all-pass lengths at 44.1 kHz (Schroeder and Moorer's layout, as in Freeverb).
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
/// The right channel's lines are this much longer, for width.
const SPREAD: usize = 23;

pub(crate) struct Reverb {
    combs: [Vec<Comb>; 2],
    allpasses: [Vec<Allpass>; 2],
    pre: [DelayLine; 2],
    predelay: f32,
    feedback: f32,
    damp: f32,
    mix: f32,
}

impl Reverb {
    fn new(sr: f32) -> Reverb {
        let scale = |n: usize| ((n as f32 * sr / 44_100.0) as usize).max(1);
        let combs = |extra: usize| COMBS.iter().map(|n| Comb { buf: vec![0.0; scale(n + extra)], pos: 0, store: 0.0 }).collect();
        let passes = |extra: usize| ALLPASSES.iter().map(|n| Allpass { buf: vec![0.0; scale(n + extra)], pos: 0 }).collect();
        let pre = (0.21 * sr) as usize;
        Reverb {
            combs: [combs(0), combs(SPREAD)],
            allpasses: [passes(0), passes(SPREAD)],
            pre: [DelayLine::new(pre), DelayLine::new(pre)],
            predelay: 1.0,
            feedback: 0.84,
            damp: 0.2,
            mix: 0.2,
        }
    }

    fn set(&mut self, e: &Effect, sr: f32) {
        self.feedback = 0.7 + 0.28 * frac(e, "size");
        self.damp = 0.4 * frac(e, "damping");
        self.mix = e.value("mix") / 100.0;
        self.predelay = (e.value("predelay") * 0.001 * sr).max(1.0);
    }

    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let input = (l + r) * 0.015;
        let mut wet = [0.0f32; 2];
        for (c, w) in wet.iter_mut().enumerate() {
            let (Some(pre), Some(combs), Some(passes)) = (self.pre.get_mut(c), self.combs.get_mut(c), self.allpasses.get_mut(c)) else { continue };
            pre.push(input);
            let x = pre.read(self.predelay);
            let mut y: f32 = combs.iter_mut().map(|comb| comb.tick(x, self.feedback, self.damp)).sum();
            for a in passes.iter_mut() {
                y = a.tick(y);
            }
            *w = y * 3.0;
        }
        let [wl, wr] = wet;
        (blend(l, wl, self.mix), blend(r, wr, self.mix))
    }
}

/// One effect in a chain.
pub(crate) enum Unit {
    Compressor(Compressor),
    Gate(Gate),
    Drive(Drive),
    Amp(Box<Amp>),
    Cabinet(Cabinet),
    Eq(Eq),
    Chorus(Chorus),
    Tremolo(Tremolo),
    Delay(Delay),
    Reverb(Box<Reverb>),
}

impl Unit {
    pub(crate) fn new(e: &Effect, sr: f32) -> Unit {
        let mut unit = match e.kind {
            EffectType::Compressor => Unit::Compressor(Compressor { env: Envelope::default(), threshold: 0.0, slope: 0.0, makeup: 0.0 }),
            EffectType::NoiseGate => Unit::Gate(Gate { env: Envelope::default(), open: 0.0, threshold: 0.0, close: 0.0, attack: 0.0 }),
            EffectType::Overdrive | EffectType::Distortion => Unit::Drive(Drive {
                hard: e.kind == EffectType::Distortion,
                pre: Biquad::default(),
                tone: Biquad::default(),
                post: Biquad::default(),
                gain: 1.0,
                level: 1.0,
            }),
            EffectType::Amp => Unit::Amp(Box::new(Amp {
                stages: 1,
                input: Biquad::default(),
                between: [Biquad::default(); 3],
                fizz: Biquad::default(),
                stage_gain: 1.0,
                bass: Biquad::default(),
                mid: Biquad::default(),
                treble: Biquad::default(),
                presence: Biquad::default(),
                out: 1.0,
            })),
            EffectType::Cabinet => Unit::Cabinet(Cabinet { bands: [Biquad::default(); 6] }),
            EffectType::Equalizer | EffectType::MixerEq => Unit::Eq(Eq { bands: Vec::new(), level: 1.0, flat: true }),
            EffectType::Chorus => {
                let n = (0.03 * sr) as usize;
                Unit::Chorus(Chorus { lines: [DelayLine::new(n), DelayLine::new(n)], lfo: Lfo::default(), sr, depth: 0.0, mix: 0.0 })
            }
            EffectType::Tremolo => Unit::Tremolo(Tremolo { lfo: Lfo::default(), depth: 0.0 }),
            EffectType::Delay => {
                let n = (2.05 * sr) as usize;
                Unit::Delay(Delay {
                    lines: [DelayLine::new(n), DelayLine::new(n)],
                    tone: [OnePole::new(8000.0, sr); 2],
                    sr,
                    now: 0.0,
                    target: 0.0,
                    feedback: 0.0,
                    mix: 0.0,
                })
            }
            EffectType::Reverb => Unit::Reverb(Box::new(Reverb::new(sr))),
        };
        unit.set(e, sr);
        unit
    }

    pub(crate) fn set(&mut self, e: &Effect, sr: f32) {
        match self {
            Unit::Compressor(u) => u.set(e, sr),
            Unit::Gate(u) => u.set(e, sr),
            Unit::Drive(u) => u.set(e, sr),
            Unit::Amp(u) => u.set(e, sr),
            Unit::Cabinet(u) => u.set(e, sr),
            Unit::Eq(u) => u.set(e, sr),
            Unit::Chorus(u) => u.set(e, sr),
            Unit::Tremolo(u) => {
                u.lfo.set(e.value("rate"), sr);
                u.depth = frac(e, "depth");
            }
            Unit::Delay(u) => u.set(e, sr),
            Unit::Reverb(u) => u.set(e, sr),
        }
    }

    /// Whether the unit passes the sound unchanged (a flat equalizer).
    pub(crate) fn is_transparent(&self) -> bool {
        matches!(self, Unit::Eq(eq) if eq.flat)
    }

    /// Whether the unit makes a mono signal stereo (so the chain cannot stay mono past it).
    pub(crate) fn widens(&self) -> bool {
        matches!(self, Unit::Chorus(_) | Unit::Reverb(_))
    }

    pub(crate) fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        let both = |l: f32, r: f32, g: f32| (l * g, r * g);
        match self {
            Unit::Compressor(u) => both(l, r, u.gain(l.abs().max(r.abs()))),
            Unit::Gate(u) => both(l, r, u.gain(l.abs().max(r.abs()))),
            Unit::Drive(u) => (u.chan(0, l), u.chan(1, r)),
            Unit::Amp(u) => (u.chan(0, l), u.chan(1, r)),
            Unit::Cabinet(u) => (u.chan(0, l), u.chan(1, r)),
            Unit::Eq(u) => (u.chan(0, l), u.chan(1, r)),
            Unit::Chorus(u) => u.tick(l, r),
            Unit::Tremolo(u) => both(l, r, u.gain()),
            Unit::Delay(u) => {
                u.glide();
                (u.chan(0, l), u.chan(1, r))
            }
            Unit::Reverb(u) => u.tick(l, r),
        }
    }

    /// One mono sample, through the left channel's state: half the work for a mono source. Not
    /// for a unit that [widens](Unit::widens).
    pub(crate) fn mono(&mut self, x: f32) -> f32 {
        match self {
            Unit::Compressor(u) => x * u.gain(x.abs()),
            Unit::Gate(u) => x * u.gain(x.abs()),
            Unit::Drive(u) => u.chan(0, x),
            Unit::Amp(u) => u.chan(0, x),
            Unit::Cabinet(u) => u.chan(0, x),
            Unit::Eq(u) => u.chan(0, x),
            Unit::Tremolo(u) => x * u.gain(),
            Unit::Delay(u) => {
                u.glide();
                u.chan(0, x)
            }
            Unit::Chorus(_) | Unit::Reverb(_) => x,
        }
    }
}
