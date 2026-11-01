//! A bowed string, modelled: a digital waveguide (two delay lines, from the bow to the bridge and
//! from the bow to the finger or nut) meeting the bow at a friction junction. The bow pulls the
//! string along while static friction holds it and lets it slip back when the string's tension
//! wins, the stick-slip cycle that sets up Helmholtz motion: a sawtooth force on the bridge,
//! with the notches that the bow's distance from the bridge cuts into its spectrum. The friction
//! is the hyperbolic curve fitted to rosin measurements, solved exactly at the junction each
//! sample (after Smith's and Woodhouse's bowed-string models), so the attack transients, a bow
//! pressed too hard (raucous) or drawn too fast (surface sound) and the noise of rosin come out
//! of the model, not from a sample.
//!
//! The player's part is modelled too: the bow's speed and force rise into a stroke and fall away
//! at its end; each new note on a sounding string is a bow change (down-bow, up-bow), passing
//! through standstill; slurred notes change pitch under one stroke; vibrato starts a moment into
//! a held note, a little irregular in rate and depth; an open string cannot be vibrated. The
//! body ([`crate::body`]) is per track.

use crate::body::{BodyKind, curve};
use crate::synth::Noise;

/// Friction coefficients of rosin on gut or steel: static, dynamic, and the slip speed (in the
/// waveguide's units) over which one gives way to the other.
const MU_STATIC: f32 = 0.8;
const MU_DYNAMIC: f32 = 0.35;
const SLIP_SPEED: f32 = 0.05;
const MIN_FREQ: f32 = 25.0;
const MAX_RING: f32 = 60.0;

/// A delay line read at a fractional delay with cubic Lagrange interpolation (flat in level up to
/// a high frequency, so a vibrato does not make the tone flicker).
struct Line {
    buf: Vec<f32>,
    w: usize,
}

impl Line {
    fn new(len: usize) -> Line {
        Line { buf: vec![0.0; len.max(8)], w: 0 }
    }

    /// Adds `x` to the sample pushed last.
    fn adjust_last(&mut self, x: f32) {
        let n = self.buf.len();
        if let Some(s) = self.buf.get_mut(if self.w == 0 { n.saturating_sub(1) } else { self.w - 1 })
            && x.is_finite()
        {
            *s += x;
        }
    }

    fn push(&mut self, x: f32) {
        if let Some(s) = self.buf.get_mut(self.w) {
            *s = if x.is_finite() && x.abs() > 1e-20 { x } else { 0.0 };
        }
        self.w = if self.w + 1 >= self.buf.len() { 0 } else { self.w + 1 };
    }

    /// The sample pushed `d` samples ago (1 is the last one), `d` at least 2.
    fn read(&self, d: f32) -> f32 {
        let n = self.buf.len();
        let d = d.clamp(2.0, (n - 3) as f32);
        let i = d.floor();
        let f = d - i;
        // The sample `k` ago, then its neighbours one newer and two older, wrapping.
        let back = self.w + n - i as usize;
        let i0 = if back >= n { back - n } else { back };
        let im = if i0 + 1 == n { 0 } else { i0 + 1 };
        let i1 = if i0 == 0 { n - 1 } else { i0 - 1 };
        let i2 = if i1 == 0 { n - 1 } else { i1 - 1 };
        let at = |j: usize| self.buf.get(j).copied().unwrap_or(0.0);
        let (xm, x0, x1, x2) = (at(im), at(i0), at(i1), at(i2));
        let hm = -f * (f - 1.0) * (f - 2.0) / 6.0;
        let h0 = (f + 1.0) * (f - 1.0) * (f - 2.0) / 2.0;
        let h1 = -(f + 1.0) * f * (f - 2.0) / 2.0;
        let h2 = (f + 1.0) * f * (f - 1.0) / 6.0;
        hm * xm + h0 * x0 + h1 * x1 + h2 * x2
    }
}

/// The velocity the bow puts into the string at the junction, from the bow's speed less the
/// string's incoming velocity (`dv`), and the bow's force, and whether the string now sticks to
/// the bow. The string's wave impedance is 1.
///
/// Where the string's reaction `2 (a - x)` (a line in the slip speed `x`) meets the friction
/// curve `F (mu_d + (mu_s - mu_d) v0 / (v0 + x))` twice as well as at sticking, the string
/// keeps what it was doing: a sticking string sticks while static friction can hold it, and a
/// slipping one slips on, at the outer crossing, until there is none (McIntyre and Woodhouse's
/// hysteresis rule, which the Helmholtz motion needs).
fn friction(dv: f32, force: f32, stuck: bool) -> (f32, bool) {
    let a = dv.abs();
    if !a.is_finite() || !force.is_finite() {
        return (0.0, true);
    }
    let holds = 2.0 * a <= MU_STATIC * force;
    if stuck && holds {
        return (dv, true);
    }
    // Slipping: the larger root of 2x² - b x - c = 0.
    let b = 2.0 * (a - SLIP_SPEED) - force * MU_DYNAMIC;
    let c = SLIP_SPEED * (2.0 * a - force * MU_STATIC);
    let disc = b * b + 8.0 * c;
    if holds && (b <= 0.0 || disc < 0.0) {
        // No slipping solution: the bow catches the string.
        return (dv, true);
    }
    let x = ((b + disc.max(0.0).sqrt()) / 4.0).clamp(0.0, a);
    (dv.signum() * (a - x), false)
}

/// The phase delay of `y += a (x - y)` at `freq`, in samples.
fn one_pole_delay(a: f32, freq: f32, sr: f32) -> f32 {
    let w = std::f32::consts::TAU * freq.max(MIN_FREQ) / sr;
    let b = 1.0 - a;
    let phase = (b * w.sin()).atan2(1.0 - b * w.cos());
    if w > 0.0 { (phase / w).max(0.0) } else { 0.0 }
}

/// The string's loss per period: a stopped string rings for a second or two low down and a few
/// tenths of a second high up (the finger damps it); an open one rings on for `open_ring` seconds.
fn loop_gain(freq: f32, open: bool, open_ring: f32) -> f32 {
    let f = freq.max(MIN_FREQ);
    let stopped = curve(&[(5.0, 2.5), (6.0, 2.0), (7.6, 1.0), (8.8, 0.6), (10.8, 0.3)], f.log2());
    let ring = if open { open_ring.max(stopped) } else { stopped };
    10f32.powf(-3.0 / (ring * f))
}

/// High partials die away in about this long, whatever the note: losses in the string and at the
/// bridge are per second, so a low string, with fewer periods a second, loses more per period.
const HF_T60: f32 = 0.08;
const HF_AT: f32 = 4000.0;

/// The coefficient of the bridge's one-pole low-pass (`y += a (x - y)`) that takes `HF_AT` down
/// by 60 dB in `HF_T60` seconds at a fundamental of `freq`.
fn corner_filter(freq: f32, sr: f32) -> f32 {
    let w = std::f32::consts::TAU * HF_AT.min(sr * 0.4) / sr;
    let g = 10f32.powf(-3.0 / (HF_T60 * freq.max(MIN_FREQ))).clamp(0.01, 0.999);
    let g2 = g * g;
    let p = 1.0 - g2 * w.cos();
    let q = 1.0 - g2;
    // |a / (1 - b z^-1)| = g at w with b = 1 - a, the root in 0..1.
    let b = (p - (p * p - q * q).max(0.0).sqrt()) / q;
    (1.0 - b).clamp(0.02, 1.0)
}

/// A value moving from `from` to `to` along a raised cosine over `len` samples.
#[derive(Clone, Copy, Debug)]
struct Ramp {
    from: f32,
    to: f32,
    len: f32,
    t: f32,
}

impl Ramp {
    fn hold(x: f32) -> Ramp {
        Ramp { from: x, to: x, len: 1.0, t: 1.0 }
    }

    fn value(&self) -> f32 {
        let x = (self.t / self.len.max(1.0)).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * (0.5 - 0.5 * (x * std::f32::consts::PI).cos())
    }

    fn to(&mut self, to: f32, len: f32) {
        self.from = self.value();
        self.to = to;
        self.len = len.max(1.0);
        self.t = 0.0;
    }

    /// Moves `n` samples on and returns the value there.
    fn advance(&mut self, n: f32) -> f32 {
        self.t = (self.t + n).min(self.len);
        self.value()
    }
}

/// One instrument's playing: its vibrato and the size of its strokes.
#[derive(Clone, Copy, Debug)]
struct Player {
    /// Vibrato depth (semitones either way) and rate (Hz).
    depth: f32,
    rate: f32,
    /// Seconds for an open string to fall 60 dB once the bow leaves it.
    ring: f32,
}

fn player(body: BodyKind) -> Player {
    match body {
        BodyKind::Violin | BodyKind::Piano => Player { depth: 0.2, rate: 5.8, ring: 1.6 },
        BodyKind::Viola => Player { depth: 0.18, rate: 5.6, ring: 2.0 },
        BodyKind::Cello => Player { depth: 0.16, rate: 5.4, ring: 3.0 },
        BodyKind::DoubleBass => Player { depth: 0.13, rate: 5.0, ring: 4.0 },
    }
}

/// How a note is bowed.
#[derive(Clone, Copy, Debug)]
pub struct Stroke {
    pub freq: f32,
    /// 0..1 from the score.
    pub velocity: f32,
    /// The note's length in seconds, for the shape of its stroke and whether it has vibrato.
    pub seconds: f32,
    /// Short and separated: a quick bite and a quick stop.
    pub short: bool,
    /// Played on the open string: no vibrato, longer ring.
    pub open: bool,
    /// Tremolo: the bow changes direction many times a second.
    pub tremolo: bool,
    /// Swelling slowly into the note.
    pub slow: bool,
    /// The bow's direction: down-bow 1, up-bow -1. A new stroke on a sounding string the other
    /// way is a bow change, through standstill.
    pub direction: f32,
    /// The bow arrives from another string within a slur: no bite.
    pub crossing: bool,
    /// The attack's length against the usual: a section's players ease into a long note.
    pub ease: f32,
}

/// A stroke or a slur a player of a section starts a moment after the others.
#[derive(Clone, Copy, Debug)]
enum Cue {
    Bow(Stroke),
    Slur { freq: f32, open: bool, seconds: f32 },
}

/// Pitch, the loop's length and the bow's speed and force change slowly next to the string's
/// motion: they are worked out every `CONTROL` samples and followed in straight lines between,
/// which saves most of a voice's work (a section has many) and is far finer than a vibrato or a
/// bow stroke moves.
const CONTROL: u32 = 16;

pub struct BowedVoice {
    pub track: usize,
    pub string: u8,
    pub body: BodyKind,
    /// Which player of a section, and which of the track's bodies it sounds through.
    pub seat: usize,
    pub slot: usize,
    /// Samples before this player comes in: a section never starts quite together.
    pub wait: u32,
    /// The player's share of a section's level.
    pub level_scale: f32,
    neck: Line,
    bridge: Line,
    log_freq: f32,
    target: f32,
    glide_per_sample: f32,
    pub bend_semis: f32,
    /// A vibrato marked in the score, and wider.
    pub vibrato: bool,
    pub wide: bool,
    /// The player's own small detuning, in semitones (sections are never exactly in tune).
    pub detune: f32,
    beta: f32,
    lp: f32,
    lp_state: f32,
    loop_gain: f32,
    /// The stroke's direction: down-bow 1, up-bow -1.
    direction: f32,
    speed: Ramp,
    force: Ramp,
    /// The force the bow's speed calls for, as a share of the most it may have (Schelleng's
    /// upper limit).
    pressure: f32,
    bow_on: bool,
    release_len: f32,
    style: Player,
    vib_phase: f32,
    vib_rate: f32,
    vib_depth: f32,
    /// Samples into the note, and when its vibrato starts and is full.
    note_t: f32,
    vib_start: f32,
    vib_full: f32,
    /// Intonation: the finger lands a little off and settles.
    settle: f32,
    settle_mul: f32,
    tremolo: Option<(f32, f32)>,
    /// Intonation by ear: the player hears the pitch and moves the finger. The stick-slip cycle
    /// runs a little off the string's own period (the slip comes early as the corner of the
    /// Helmholtz wave rounds off), so the voice measures the time between slips against the
    /// intended pitch and corrects the stopped length, slowly.
    phase: f32,
    stuck: bool,
    tune: f32,
    /// Tone by ear: how often a slip comes early (a second slip within a period), and the extra
    /// weight the player puts on the bow while the string keeps slipping twice a period.
    rough: f32,
    press: f32,
    /// The slip under way: whether it came early, and how long it has lasted.
    slip_early: bool,
    slip_run: f32,
    /// How far the last sample was from static friction's limit (above zero, past it).
    grip: f32,
    noise: Noise,
    hiss: f32,
    hiss_amount: f32,
    wander: f32,
    wander_target: f32,
    /// Samples to the next control point, and control points so far.
    countdown: u32,
    blocks: u32,
    /// The loop's length before the ear's correction, the bow's speed and force and the pitch's
    /// phase per sample, each with its step towards the next control point.
    total: f32,
    total_step: f32,
    speed_now: f32,
    speed_step: f32,
    force_now: f32,
    force_step: f32,
    phase_step: f32,
    /// A stroke or slur waiting its turn.
    cue: Option<(u32, Cue)>,
    pub released: bool,
    pub let_ring: bool,
    pub note_start: f64,
    pub note_end: f64,
    pub bend: Option<guitario_score::Bend>,
    pub whammy: Option<crate::synth::WhammyTime>,
    pub slide: Option<crate::synth::SlideTime>,
    pub fade: Option<guitario_score::Fade>,
    pub gain: f32,
    level: f32,
    age: u32,
    sr: f32,
}

/// The bow's distance from the bridge (a share of the string) and its force (a share of the most
/// the Helmholtz motion survives), softly and how much nearer and harder loudly.
const BETA: f32 = 0.15;
const BETA_LOUD: f32 = 0.03;
const PRESSURE: f32 = 0.35;
const PRESSURE_LOUD: f32 = 0.2;
/// The rosin's grip varies this much, sample to sample.
const HISS: f32 = 0.06;

/// Overall level, so a mezzo-forte bowed note sits near a plucked string's level.
const OUTPUT: f32 = 0.22;

impl BowedVoice {
    pub fn new(track: usize, string: u8, body: BodyKind, stroke: &Stroke, seed: u32, sr: f32) -> BowedVoice {
        let sr = if sr.is_finite() { sr.clamp(8_000.0, 384_000.0) } else { 48_000.0 };
        let stroke = &Stroke { freq: if stroke.freq.is_finite() { stroke.freq.clamp(MIN_FREQ, sr * 0.2) } else { 440.0 }, ..*stroke };
        let len = (sr / MIN_FREQ) as usize + 8;
        let style = player(body);
        let mut noise = Noise::new(seed.wrapping_mul(2_654_435_761).max(1));
        let jitter = |n: &mut Noise| n.next();
        let vib_rate = style.rate * (1.0 + 0.06 * jitter(&mut noise));
        let vib_depth = style.depth * (1.0 + 0.2 * jitter(&mut noise));
        let lf = stroke.freq.max(MIN_FREQ).log2();
        let mut v = BowedVoice {
            track,
            string,
            body,
            seat: 0,
            slot: 0,
            wait: 0,
            level_scale: 1.0,
            neck: Line::new(len),
            bridge: Line::new(len),
            log_freq: lf,
            target: lf,
            glide_per_sample: 0.0,
            bend_semis: 0.0,
            vibrato: false,
            wide: false,
            detune: 0.0,
            beta: 0.12,
            lp: 0.0,
            lp_state: 0.0,
            loop_gain: 0.99,
            direction: 1.0,
            speed: Ramp::hold(0.0),
            force: Ramp::hold(0.0),
            pressure: 0.3,
            bow_on: true,
            release_len: 0.08 * sr,
            style,
            vib_phase: (jitter(&mut noise) + 1.0) * 0.5,
            vib_rate,
            vib_depth,
            note_t: 0.0,
            vib_start: f32::MAX,
            vib_full: f32::MAX,
            settle: 0.0,
            settle_mul: 1.0,
            tremolo: None,
            phase: 0.0,
            stuck: true,
            tune: 1.0,
            rough: 0.0,
            press: 1.0,
            slip_early: false,
            slip_run: 0.0,
            grip: 0.0,
            noise,
            hiss: 0.0,
            hiss_amount: HISS,
            wander: 0.0,
            wander_target: 0.0,
            countdown: 0,
            blocks: 0,
            total: 0.0,
            total_step: 0.0,
            speed_now: 0.0,
            speed_step: 0.0,
            force_now: 0.0,
            force_step: 0.0,
            phase_step: 0.0,
            cue: None,
            released: false,
            let_ring: false,
            note_start: 0.0,
            note_end: f64::MAX,
            bend: None,
            whammy: None,
            slide: None,
            fade: None,
            gain: 1.0,
            level: 0.0,
            age: 0,
            sr,
        };
        v.bow(stroke);
        // The loop starts at its length rather than growing into it.
        v.total = v.loop_length(v.pitch_now());
        v
    }

    /// The player's own vibrato: its rate and depth against the instrument's usual.
    pub fn personalise(&mut self, rate: f32, depth: f32) {
        if rate.is_finite() && depth.is_finite() {
            self.vib_rate *= rate.clamp(0.5, 2.0);
            self.vib_depth *= depth.clamp(0.0, 3.0);
        }
    }

    /// A new stroke `delay` samples from now (a player of a section a moment after the others).
    pub fn bow_after(&mut self, s: &Stroke, delay: u32) {
        if delay == 0 {
            self.bow(s);
        } else {
            self.cue = Some((delay, Cue::Bow(*s)));
        }
    }

    /// A slurred note `delay` samples from now.
    pub fn slur_after(&mut self, freq: f32, open: bool, seconds: f32, delay: u32) {
        if delay == 0 {
            self.slur_to(freq, open, seconds);
        } else {
            self.cue = Some((delay, Cue::Slur { freq, open, seconds }));
        }
    }

    /// A new stroke, from rest or on the sounding string.
    pub fn bow(&mut self, s: &Stroke) {
        self.cue = None;
        self.countdown = 0;
        let s = &Stroke { freq: if s.freq.is_finite() { s.freq.clamp(MIN_FREQ, self.sr * 0.2) } else { 440.0 }, ..*s };
        let vel = s.velocity.clamp(0.0, 1.0);
        self.target = s.freq.max(MIN_FREQ).log2();
        self.log_freq = self.target;
        self.glide_per_sample = 0.0;
        // Louder is a faster bow, nearer the bridge, pressed harder.
        self.beta = BETA - BETA_LOUD * vel;
        let speed = 0.06 + 0.3 * vel;
        self.pressure = PRESSURE + PRESSURE_LOUD * vel;
        let force = self.pressure * 2.0 * speed / (self.beta * (MU_STATIC - MU_DYNAMIC));
        let ease = if s.ease.is_finite() { s.ease.clamp(0.5, 4.0) } else { 1.0 };
        let attack = (match (s.short, s.slow) {
            (true, _) => 0.025f32,
            (false, false) => 0.07,
            (false, true) => 0.35,
        } * ease)
            .min(s.seconds.max(0.01) * 0.35)
            * self.sr;
        let change = s.direction != self.direction && self.speed.value().abs() > 1e-4;
        self.direction = if s.direction < 0.0 { -1.0 } else { 1.0 };
        let attack = if s.crossing { attack.min(0.03 * self.sr) } else { attack };
        self.speed.to(self.direction * speed, if change { attack.min(0.045 * self.sr) } else { attack });
        // The bow bites into the string, then eases to the force the stroke needs.
        let bite = if s.crossing { 1.0 } else { 1.35 };
        let from = if s.crossing { self.force.value().max(force * 0.5) } else { force * bite };
        self.force = Ramp { from, to: force, len: attack * 2.0, t: 0.0 };
        self.bow_on = true;
        self.released = false;
        self.release_len = if s.short { 0.04 } else { 0.09 } * self.sr;
        self.loop_gain = loop_gain(s.freq, s.open, self.style.ring);
        self.lp = corner_filter(s.freq, self.sr);
        self.note_t = 0.0;
        let long = s.seconds > 0.25;
        (self.vib_start, self.vib_full) = if s.open || !long {
            (f32::MAX, f32::MAX)
        } else {
            let start = (0.12 + 0.12 * (self.noise.next() + 1.0) * 0.5).min(s.seconds * 0.3) * self.sr;
            (start, start + 0.3 * self.sr)
        };
        self.settle = 0.06 * self.noise.next() / 12.0;
        self.settle_mul = (-1.0 / (0.05 * self.sr)).exp();
        self.tremolo = s.tremolo.then_some((0.0, self.sr / (2.0 * 7.5)));
    }

    /// A slurred note: the finger moves under the same stroke.
    pub fn slur_to(&mut self, freq: f32, open: bool, seconds: f32) {
        self.cue = None;
        self.countdown = 0;
        let freq = if freq.is_finite() { freq.clamp(MIN_FREQ, self.sr * 0.2) } else { 440.0 };
        self.target = freq.max(MIN_FREQ).log2();
        let samples = 0.03 * self.sr;
        self.glide_per_sample = (self.target - self.log_freq) / samples;
        self.released = false;
        self.bow_on = true;
        self.loop_gain = loop_gain(freq, open, self.style.ring);
        self.note_t = 0.0;
        (self.vib_start, self.vib_full) = if open || seconds <= 0.25 { (f32::MAX, f32::MAX) } else { (0.08 * self.sr, 0.3 * self.sr) };
        // Keep the stroke going if it was ending.
        let speed = self.speed.to;
        if speed.abs() < 1e-4 {
            self.speed.to(self.direction * 0.15, 0.03 * self.sr);
        }
    }

    pub fn release(&mut self) {
        if !self.released {
            self.released = true;
            self.cue = None;
            self.countdown = 0;
            self.speed.to(0.0, self.release_len);
            self.force.to(0.0, self.release_len);
        }
    }

    /// Another note takes the string: the bow leaves this one at once.
    pub fn damp(&mut self) {
        self.released = true;
        self.cue = None;
        self.countdown = 0;
        self.speed.to(0.0, 0.004 * self.sr);
        self.force.to(0.0, 0.004 * self.sr);
        self.loop_gain = self.loop_gain.min(0.9);
    }

    /// Whether it has fallen silent: the bow is off and what it adds to the sound (a section
    /// player's share is smaller) is far below hearing, or it has rung for a minute.
    pub fn finished(&self) -> bool {
        let quiet = self.released && self.force.value() <= 0.0 && self.level * self.level_scale.min(1.0) < 1e-5;
        quiet || self.age > (self.sr * MAX_RING) as u32
    }

    /// The pitch now, in Hz: the note, the score's bends, the player's vibrato and intonation.
    fn pitch_now(&self) -> f32 {
        let mut semis = self.bend_semis + self.detune + self.settle * 12.0;
        let natural = ((self.note_t - self.vib_start) / (self.vib_full - self.vib_start).max(1.0)).clamp(0.0, 1.0);
        // A vibrato marked in the score is there from the start, and deeper.
        let depth = match (self.vibrato, self.wide) {
            (false, _) => self.vib_depth * natural,
            (true, false) => self.vib_depth * 1.5,
            (true, true) => self.vib_depth * 2.4,
        };
        if depth > 0.0 {
            semis += depth * (self.vib_phase * std::f32::consts::TAU).sin();
        }
        (self.log_freq + semis / 12.0).exp2()
    }

    /// The loop's length in samples for `freq`: both lines, less the bridge low-pass's own delay
    /// (before the ear's correction, which follows each slip at once).
    fn loop_length(&self, freq: f32) -> f32 {
        self.sr / freq - one_pole_delay(self.lp, freq, self.sr)
    }

    /// The next control point: everything that moves slowly, `CONTROL` samples on.
    fn control(&mut self) {
        let n = CONTROL as f32;
        if self.glide_per_sample != 0.0 {
            self.log_freq += self.glide_per_sample * n;
            if (self.glide_per_sample > 0.0) == (self.log_freq >= self.target) {
                self.log_freq = self.target;
                self.glide_per_sample = 0.0;
            }
        }
        self.note_t += n;
        self.settle *= self.settle_mul.powi(CONTROL as i32);
        if self.vibrato || self.note_t > self.vib_start {
            self.vib_phase += self.vib_rate * (1.0 + 0.04 * self.wander) * n / self.sr;
            self.vib_phase -= self.vib_phase.floor();
        }
        let freq = self.pitch_now();
        self.total_step = (self.loop_length(freq) - self.total) / n;
        self.phase_step = freq / self.sr;

        // The bow: slow wander in its speed (a player's arm).
        if self.blocks.is_multiple_of(64 / CONTROL) {
            if self.blocks.is_multiple_of(4096 / CONTROL) {
                self.wander_target = self.noise.next();
            }
            self.wander += (self.wander_target - self.wander) * 0.02;
        }
        self.blocks = self.blocks.wrapping_add(1);
        if let Some((t, half)) = self.tremolo.as_mut() {
            *t += n;
            if *t >= *half && !self.released {
                *t = 0.0;
                self.direction = -self.direction;
                let s = self.speed.to.abs();
                self.speed.to(self.direction * s, *half * 0.5);
            }
        }
        if self.released && self.force.value() <= 0.0 && self.force_now <= 1e-6 {
            self.bow_on = false;
        }
        self.speed_step = (self.speed.advance(n) - self.speed_now) / n;
        self.force_step = (self.force.advance(n) - self.force_now) / n;
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        if self.wait > 0 {
            self.wait -= 1;
            return 0.0;
        }
        if let Some((delay, cue)) = self.cue.as_mut() {
            if *delay > 0 {
                *delay -= 1;
            } else {
                match *cue {
                    Cue::Bow(s) => self.bow(&s),
                    Cue::Slur { freq, open, seconds } => self.slur_to(freq, open, seconds),
                }
            }
        }
        if self.countdown == 0 {
            self.control();
            self.countdown = CONTROL;
        }
        self.countdown -= 1;
        self.total += self.total_step;
        self.speed_now += self.speed_step;
        self.force_now += self.force_step;
        self.phase += self.phase_step;
        let total = (self.total * self.tune).max(4.5);
        let to_bridge = (total * self.beta).max(2.0);
        let to_neck = (total - to_bridge).max(2.0);

        // Rosin noise in the bow's grip.
        self.hiss += 0.35 * (self.noise.next() - self.hiss);
        let speed = self.speed_now * (1.0 + 0.04 * self.wander);
        let force = self.force_now.max(0.0) * self.press * (1.0 + self.hiss_amount * self.hiss);

        let from_bridge = self.bridge.read(to_bridge);
        let from_neck = self.neck.read(to_neck);
        self.lp_state += self.lp * (from_bridge - self.lp_state);
        let bridge_reflection = -self.lp_state * self.loop_gain;
        let nut_reflection = -from_neck;
        let incoming = bridge_reflection + nut_reflection;
        let dv = speed - incoming;
        let (added, stuck) = if self.bow_on && force > 0.0 { friction(dv, force, self.stuck) } else { (0.0, true) };
        let grip = 2.0 * dv.abs() - MU_STATIC * force;
        let mut onset = 0.0;
        if self.stuck && !stuck && self.bow_on {
            // The slip began between the last sample and this one, where the bow's grip gave
            // way: spread the jump over the two as a band-limited step does, so slips do not snap
            // to the sample clock (a jitter heard as hiss between the harmonics).
            let d = if grip > 0.0 && self.grip <= 0.0 { (grip / (grip - self.grip)).clamp(0.0, 1.0) } else { 0.5 };
            let jump = added - dv;
            self.neck.adjust_last(0.5 * jump * d * d);
            self.bridge.adjust_last(0.5 * jump * d * d);
            onset = -0.5 * jump * (1.0 - d) * (1.0 - d);
        }
        self.grip = grip;
        if self.stuck && !stuck && self.bow_on {
            // A slip starts: one period of the stick-slip cycle has passed.
            // A second slip within a period (the attack, a raucous stroke) is not a period.
            let error = self.phase - 1.0;
            self.slip_early = error < -0.08;
            self.slip_run = 0.0;
            if error.abs() < 0.08 {
                self.tune = (self.tune * (1.0 - 0.15 * error)).clamp(0.9, 1.1);
                self.phase = 0.0;
            } else if error > 0.0 {
                self.phase = 0.0;
            }
        } else if !stuck {
            self.slip_run += 1.0;
        } else if !self.stuck && self.bow_on {
            // A slip ends. One as long as half the Helmholtz motion's (not a ripple while the
            // string sticks) that came early is a second slip in the period. A string that keeps
            // slipping twice a period has too little weight on it for the bow's speed (below
            // Schelleng's lower limit, likeliest low down and softly, and after a bow change): the
            // player hears it whistle an octave up and presses until it speaks, half as hard again
            // within a twentieth of a second (more turns it raucous), then eases off. The attack
            // is left alone until its bite has eased (that is too much weight, not too little), and
            // so are notes above 1 kHz, where early slips come from a loud stroke near the bridge:
            // too much weight again.
            if self.slip_run + 1.0 >= 0.5 * self.beta * total {
                self.rough += 0.1 * (if self.slip_early { 1.0 } else { 0.0 } - self.rough);
                let periods = 0.05 * self.phase_step * self.sr;
                let settled = self.force.t >= self.force.len && self.speed.t >= self.speed.len;
                if self.rough > 0.4 && settled && periods < 50.0 {
                    self.press = (self.press * 1.5f32.powf(1.0 / periods.max(1.0))).min(1.6);
                } else if self.rough < 0.1 {
                    self.press = 1.0 + (self.press - 1.0) * (1.0 - 1.0 / (6.0 * periods).max(1.0));
                }
            }
        }
        self.stuck = stuck;
        self.neck.push(bridge_reflection + added + onset);
        self.bridge.push(nut_reflection + added + onset);

        self.age = self.age.saturating_add(1);
        self.level = self.level * 0.999 + from_bridge.abs() * 0.001;
        let y = from_bridge * OUTPUT * self.gain * self.level_scale;
        if y.is_finite() { y } else { 0.0 }
    }
}

/// How one player of a section plays a note, against the others: a little out of tune and late,
/// with their own vibrato and bow. Studies of orchestral sections put the players' intonation a
/// few cents apart and their onsets some tens of milliseconds; the many independent vibratos,
/// tunings and bodies are what turn one instrument's line into a section's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seat {
    /// Semitones off the note.
    pub detune: f32,
    /// Seconds after the section's beat this player starts the note, and after its end stops.
    pub late: f32,
    pub lingers: f32,
    /// The player's vibrato rate and depth against the instrument's usual.
    pub rate: f32,
    pub depth: f32,
    /// The player's bow against the dynamic: a faster bow is louder and brighter.
    pub bow: f32,
    /// How much longer than usual the player takes to reach the note's level.
    pub ease: f32,
    /// Level: the back desks are further away.
    pub level: f32,
}

impl Seat {
    /// A soloist: on the beat, in tune, as the instrument plays.
    pub const SOLO: Seat = Seat { detune: 0.0, late: 0.0, lingers: 0.0, rate: 1.0, depth: 1.0, bow: 1.0, ease: 1.0, level: 1.0 };

    /// Player `i` of a section of `kind`, two to a desk from the front, on a note marked `note`
    /// (any number, new each note) lasting `seconds`: the player's habits come from `track` and
    /// the seat, the rest changes from note to note.
    pub fn of(i: usize, track: u32, note: u32, kind: BodyKind, seconds: f32) -> Seat {
        let i = i as u32;
        let habit = |k: u32| crate::piano::hash01(track.wrapping_mul(7919).wrapping_add(i), k) * 2.0 - 1.0;
        let now = |k: u32| crate::piano::hash01(note.wrapping_add(i.wrapping_mul(104_729)), k) * 2.0 - 1.0;
        let desk = (i / 2) as f32;
        // Entries: a short note is placed together, within 16 ms; a long one is eased into, the
        // players a little apart and each taking longer (recordings of sections reach a sustained
        // note's level in 0.1 to 0.5 s, a soloist in as little as 50 ms).
        let spread = curve(&[(0.3, 0.016), (1.0, 0.04)], seconds);
        let ease = curve(&[(0.3, 1.0), (1.0, 2.0)], seconds) * (1.0 + 0.3 * now(6));
        Seat {
            // About 4.5 cents either way (a triangular spread, as its standard deviation).
            detune: 0.055 * (now(1) + now(2)),
            // The back desks a little later (further from the leader, and from the listener).
            late: 0.002 * desk + 0.25 * spread * (now(3) + now(4) + 2.0),
            lingers: 0.012 * (now(5) + 1.0),
            rate: 1.0 + 0.08 * habit(1),
            depth: section_vibrato(kind) * (1.0 + 0.3 * habit(2)),
            bow: 1.0 + 0.1 * habit(3),
            ease,
            level: 1.0 / (1.0 + 0.04 * desk),
        }
    }
}

/// How much vibrato a section's players use against a soloist on the same instrument: measured
/// on recordings of sections and soloists playing the same sustained notes, the partials of a
/// section are spread over 14 cents (violins), 11 (violas and cellos) and 6 (double basses),
/// which their tuning and these vibratos give.
fn section_vibrato(kind: BodyKind) -> f32 {
    match kind {
        BodyKind::Violin | BodyKind::Viola | BodyKind::Piano => 0.75,
        BodyKind::Cello => 0.95,
        BodyKind::DoubleBass => 0.55,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn stroke(freq: f32, velocity: f32) -> Stroke {
        Stroke { freq, velocity, seconds: 0.2, short: false, open: false, tremolo: false, slow: false, direction: 1.0, crossing: false, ease: 1.0 }
    }

    fn render(freq: f32, velocity: f32, seconds: f32) -> Vec<f32> {
        let mut v = BowedVoice::new(0, 0, BodyKind::Violin, &stroke(freq, velocity), 1, SR);
        (0..(seconds * SR) as usize).map(|_| v.tick()).collect()
    }

    /// The pitch by autocorrelation, between 50 and 3000 Hz.
    pub(crate) fn pitch(x: &[f32], sr: f32) -> f32 {
        let lo = (sr / 3000.0) as usize;
        let hi = (sr / 50.0) as usize;
        let n = x.len().saturating_sub(hi);
        let ac = |lag: usize| x.iter().take(n).zip(x.iter().skip(lag)).map(|(a, b)| a * b).sum::<f32>();
        let e = ac(0).max(1e-12);
        // The first lag past the first dip whose correlation is near the best.
        let values: Vec<f32> = (lo..hi).map(ac).collect();
        let best = values.iter().copied().fold(f32::MIN, f32::max);
        let mut lag = lo;
        let mut dipped = false;
        for (i, &v) in values.iter().enumerate() {
            if v < 0.3 * e {
                dipped = true;
            }
            if dipped && v > 0.9 * best {
                // Climb to the local peak.
                let mut j = i;
                while values.get(j + 1).is_some_and(|&w| w > values[j]) {
                    j += 1;
                }
                lag = lo + j;
                break;
            }
        }
        // Parabolic refinement.
        let (a, b, c) = (ac(lag - 1), ac(lag), ac(lag + 1));
        let shift = 0.5 * (a - c) / (a - 2.0 * b + c);
        sr / (lag as f32 + if shift.is_finite() { shift } else { 0.0 })
    }

    #[test]
    fn friction_sticks_then_slips() {
        assert_eq!(friction(0.1, 1.0, true), (0.1, true));
        let (slip, stuck) = friction(1.0, 1.0, true);
        assert!(slip > 0.0 && slip < 1.0 && !stuck, "{slip}");
        assert!(friction(-1.0, 1.0, true).0 < 0.0);
        assert_eq!(friction(f32::NAN, 1.0, true), (0.0, true));
        // Below the static limit, a slipping string slips on while it can.
        assert!(!friction(0.38, 1.0, false).1);
        assert!(friction(0.01, 1.0, false).1);
    }

    #[test]
    fn a_bowed_note_speaks_at_its_pitch() {
        for f in [196.0, 440.0, 880.0, 1760.0] {
            let x = render(f, 0.6, 1.2);
            assert!(x.iter().all(|s| s.is_finite()));
            let steady = &x[(0.5 * SR) as usize..(1.0 * SR) as usize];
            let rms = (steady.iter().map(|s| s * s).sum::<f32>() / steady.len() as f32).sqrt();
            let hz = pitch(&steady[..4096], SR);
            let cents = 1200.0 * (hz / f).log2();
            println!("{f} Hz: {hz:.1} Hz ({cents:+.1} cents), rms {rms:.4}");
            assert!(rms > 1e-3, "{f}: silent");
            assert!(cents.abs() < 25.0, "{f}: {hz}");
        }
    }

    /// Slips a period of a note bowed with `seed`, a quarter of a second after it settles.
    fn slips_per_period(f: f32, vel: f32, seed: u32) -> f32 {
        let mut v = BowedVoice::new(0, 0, BodyKind::Violin, &stroke(f, vel), seed, SR);
        let (mut slips, mut run) = (0, 0);
        for i in 0..36_000 {
            v.tick();
            if i < 24_000 {
                continue;
            }
            if !v.stuck {
                run += 1;
            } else {
                if run as f32 >= 0.5 * v.beta * SR / f {
                    slips += 1;
                }
                run = 0;
            }
        }
        slips as f32 / (f * 0.25)
    }

    #[test]
    fn the_bow_sets_up_helmholtz_motion_across_the_range() {
        // One full slip per period (none extra, none missed) from a cello's C to a violin's
        // high E, softly to loudly, for the typical bow. The stick-slip motion is chaotic: one bow
        // in a couple of hundred slips a few extra times below 1.5 kHz (the median of three must
        // hold it), one in two or three above (the median of five).
        for f in [65.4f32, 98.0, 146.8, 196.0, 293.7, 440.0, 659.3, 880.0, 1318.5, 1760.0, 2637.0] {
            for vel in [0.1f32, 0.3, 0.5, 0.7, 0.9] {
                let bows = if f > 1500.0 { 5 } else { 3 };
                let mut each: Vec<f32> = (1..=bows).map(|seed| slips_per_period(f, vel, seed)).collect();
                each.sort_by(f32::total_cmp);
                let per_period = each[bows as usize / 2];
                assert!((per_period - 1.0).abs() < 0.06, "{f} Hz at {vel}: {per_period} slips a period");
            }
        }
    }

    /// The energy between a note's harmonics from 1 to 3 kHz, against all of it, in dB, by
    /// Goertzel filters every 2 Hz over a Hann window.
    fn hiss_db(x: &[f32], f0: f32) -> f32 {
        let n = x.len();
        let (mut between, mut total) = (0.0f64, 0.0f64);
        let mut f = 50.0f32;
        while f < 12_000.0 {
            let c = 2.0 * (std::f32::consts::TAU * f / SR).cos();
            let (mut s1, mut s2) = (0.0f32, 0.0f32);
            for (i, v) in x.iter().enumerate() {
                let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos();
                (s1, s2) = (v * w + c * s1 - s2, s1);
            }
            let p = f64::from(s1 * s1 + s2 * s2 - c * s1 * s2);
            total += p;
            if (1000.0..3000.0).contains(&f) && (f - (f / f0).round() * f0).abs() > 0.25 * f0 {
                between += p;
            }
            f += 2.0;
        }
        (10.0 * (between / total).log10()) as f32
    }

    #[test]
    fn a_bowed_note_is_as_clean_between_its_harmonics_as_a_real_one() {
        // Recordings of violins hold 1 to 3 kHz between the harmonics 34 dB under the note (the
        // whole instrument; sections 30), where the model measured 26: its slips snapped to the
        // sample clock. The bare string had 30 dB here (36.5 at 110 Hz); each slip placed between
        // samples gives 37 (40).
        for (f, most) in [(329.6f32, -33.0f32), (110.0, -38.0)] {
            let mut v = BowedVoice::new(0, 0, BodyKind::Violin, &stroke(f, 0.55), 1, SR);
            for _ in 0..SR as usize {
                v.tick();
            }
            let windows: Vec<f32> = (0..4).map(|_| hiss_db(&(0..4800).map(|_| v.tick()).collect::<Vec<_>>(), f)).collect();
            let mean = windows.iter().sum::<f32>() / 4.0;
            println!("{f} Hz: {mean:.1} dB between the harmonics");
            assert!(mean < most, "{f} Hz: {mean} dB");
        }
    }

    #[test]
    fn a_bow_change_settles_into_helmholtz_motion() {
        // Low strings after a bow change are where the motion can fall into slipping twice a
        // period (an octave-up whistle); the player's ear presses it back into speaking.
        let mut whistling = Vec::new();
        for (from, to) in [(196.0f32, 185.0f32), (130.8, 146.8), (98.0, 92.5), (65.4, 73.4)] {
            for vel in [0.4f32, 0.6, 0.8] {
                for seed in 1..=4 {
                    let s = Stroke { seconds: 1.2, ..stroke(from, vel) };
                    let mut v = BowedVoice::new(0, 0, BodyKind::Cello, &s, seed, SR);
                    for _ in 0..(0.6 * SR) as usize {
                        v.tick();
                    }
                    v.bow(&Stroke { freq: to, direction: -1.0, seconds: 0.45, ..s });
                    let (mut slips, mut run) = (0, 0);
                    for i in 0..(0.4 * SR) as usize {
                        v.tick();
                        if i < (0.15 * SR) as usize {
                            continue;
                        }
                        if !v.stuck {
                            run += 1;
                        } else {
                            if run as f32 >= 0.5 * v.beta * SR / to {
                                slips += 1;
                            }
                            run = 0;
                        }
                    }
                    let per_period = slips as f32 / (to * 0.25);
                    if per_period > 1.15 {
                        whistling.push((to, vel, seed, per_period));
                    }
                }
            }
        }
        assert!(whistling.len() <= 1, "{whistling:?}");
    }

    #[test]
    fn a_bow_change_reverses_the_bow_and_a_slur_keeps_it() {
        let mut v = BowedVoice::new(0, 0, BodyKind::Cello, &stroke(130.8, 0.6), 1, SR);
        for _ in 0..(0.3 * SR) as usize {
            v.tick();
        }
        assert!(v.speed.value() > 0.0);
        v.slur_to(146.8, false, 0.5);
        for _ in 0..(0.1 * SR) as usize {
            v.tick();
        }
        assert!(v.speed.value() > 0.0, "a slur keeps the stroke");
        v.bow(&Stroke { direction: -1.0, ..stroke(164.8, 0.6) });
        for _ in 0..(0.2 * SR) as usize {
            v.tick();
        }
        assert!(v.speed.value() < 0.0, "up-bow");
        let x: Vec<f32> = (0..(0.4 * SR) as usize).map(|_| v.tick()).collect();
        let hz = pitch(&x[(0.2 * SR) as usize..], SR);
        assert!((1200.0 * (hz / 164.8).log2()).abs() < 10.0, "{hz}");
    }

    #[test]
    fn a_released_note_rings_and_dies_away() {
        let mut v = BowedVoice::new(0, 0, BodyKind::Violin, &Stroke { open: true, ..stroke(440.0, 0.6) }, 1, SR);
        for _ in 0..(0.4 * SR) as usize {
            v.tick();
        }
        v.release();
        let ring: Vec<f32> = (0..(0.3 * SR) as usize).map(|_| v.tick()).collect();
        assert!(ring.iter().skip((0.2 * SR) as usize).any(|s| s.abs() > 1e-3), "an open string rings on");
        let mut n = 0;
        while !v.finished() && n < (20.0 * SR) as usize {
            v.tick();
            n += 1;
        }
        assert!(v.finished());
    }

    #[test]
    fn hostile_pitches_and_rates_stay_finite() {
        for freq in [f32::NAN, -5.0, 0.0, 1.0, 30_000.0, f32::INFINITY] {
            for sr in [8_000.0, 384_000.0, f32::NAN] {
                let s = Stroke { velocity: 7.0, seconds: f32::NAN, ..stroke(freq, 0.5) };
                let mut v = BowedVoice::new(0, 0, BodyKind::DoubleBass, &s, 3, sr);
                for _ in 0..4000 {
                    assert!(v.tick().is_finite(), "{freq} {sr}");
                }
                v.slur_to(f32::NAN, true, -1.0);
                v.bow(&s);
                for _ in 0..1000 {
                    assert!(v.tick().is_finite(), "{freq} {sr}");
                }
            }
        }
    }
}
