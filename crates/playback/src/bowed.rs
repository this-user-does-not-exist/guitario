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
use crate::fx::filter::{Biquad, Shape};
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

    fn push(&mut self, x: f32) {
        if let Some(s) = self.buf.get_mut(self.w) {
            *s = if x.is_finite() && x.abs() > 1e-20 { x } else { 0.0 };
        }
        self.w = (self.w + 1) % self.buf.len().max(1);
    }

    /// Adds `x` to the sample pushed last.
    fn nudge_last(&mut self, x: f32) {
        let n = self.buf.len();
        if let Some(s) = self.buf.get_mut((self.w + n - 1) % n.max(1)) {
            *s += if x.is_finite() { x } else { 0.0 };
        }
    }

    /// The sample pushed `d` samples ago (1 is the last one), `d` at least 2.
    fn read(&self, d: f32) -> f32 {
        let n = self.buf.len();
        let d = d.clamp(2.0, (n - 3) as f32);
        let i = d.floor();
        let f = d - i;
        let at = |k: usize| self.buf.get((self.w + n * 2 - k) % n).copied().unwrap_or(0.0);
        let k = i as usize;
        let (xm, x0, x1, x2) = (at(k - 1), at(k), at(k + 1), at(k + 2));
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

/// How far the junction is from changing state, positive while it keeps it: for a sticking
/// string the static friction to spare, for a slipping one the discriminant of its slip (which
/// goes negative where the slipping solution vanishes and the bow catches the string).
fn margin(dv: f32, force: f32, stuck: bool) -> f32 {
    let a = dv.abs();
    if stuck {
        MU_STATIC * force - 2.0 * a
    } else {
        let b = 2.0 * (a - SLIP_SPEED) - force * MU_DYNAMIC;
        let c = SLIP_SPEED * (2.0 * a - force * MU_STATIC);
        b * b + 8.0 * c
    }
}

/// The coefficient of `y += a (x - y)` cornered at `hz`.
fn one_pole(hz: f32, sr: f32) -> f32 {
    1.0 - (-std::f32::consts::TAU * hz / sr.max(1.0)).exp()
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
/// by 60 dB in `HF_T60` seconds at a fundamental of `freq`. The lowest strings lose them a
/// little faster: a long string keeps ripples on its Helmholtz corner that, over-sharp, let the
/// bow slip twice in a period or stick for one, now and then; this much loss holds one slip a
/// period from 46 Hz up at every dynamic.
fn corner_filter(freq: f32, sr: f32) -> f32 {
    let w = std::f32::consts::TAU * HF_AT.min(sr * 0.4) / sr;
    let t60 = HF_T60 * curve(&[(5.3, 0.75), (6.0, 0.6), (7.3, 0.7), (8.2, 1.0)], freq.max(MIN_FREQ).log2());
    let g = 10f32.powf(-3.0 / (t60 * freq.max(MIN_FREQ))).clamp(0.01, 0.999);
    let g2 = g * g;
    let p = 1.0 - g2 * w.cos();
    let q = 1.0 - g2;
    // |a / (1 - b z^-1)| = g at w with b = 1 - a, the root in 0..1.
    let b = (p - (p * p - q * q).max(0.0).sqrt()) / q;
    (1.0 - b).clamp(0.02, 1.0)
}

/// How sharp the bow keeps the corner of the Helmholtz wave: the harder and faster it is drawn,
/// the sharper, and the stronger the string's upper harmonics. Recorded violins at forte have
/// their 4th harmonic 2 dB, their 7th and 8th 5.5 dB and everything from the 9th up 7 dB
/// stronger against their fundamental than at piano. Modelled as a shelf on the bridge's force
/// (`y = lp + gain (x - lp)`, the low-pass cornered at the fifth harmonic), even at
/// mezzo-forte: its coefficient and its gain on the harmonics above.
fn corner(freq: f32, velocity: f32, sr: f32) -> (f32, f32) {
    let fc = (3.5 * freq.max(MIN_FREQ)).min(sr * 0.4);
    let a = 1.0 - (-std::f32::consts::TAU * fc / sr).exp();
    let db = CORNER_DB_PER_VELOCITY * (velocity.clamp(0.0, 1.0) - 0.63);
    (a, 10f32.powf(db / 20.0))
}

/// The corner's gain on the upper harmonics, in dB per unit of velocity.
const CORNER_DB_PER_VELOCITY: f32 = 26.0;

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
        if self.t >= self.len {
            return self.to;
        }
        let x = (self.t / self.len.max(1.0)).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * (0.5 - 0.5 * (x * std::f32::consts::PI).cos())
    }

    fn to(&mut self, to: f32, len: f32) {
        self.from = self.value();
        self.to = to;
        self.len = len.max(1.0);
        self.t = 0.0;
    }

    fn tick(&mut self) -> f32 {
        let v = self.value();
        if self.t < self.len {
            self.t += 1.0;
        }
        v
    }
}

/// A slow, smooth random wander between -1 and 1: a player's arm never draws the bow quite
/// evenly. It heads for a new random value every few tenths of a second, easing towards it
/// every few samples (a step every few dozen would buzz).
#[derive(Clone, Copy, Debug)]
struct Drift {
    value: f32,
    target: f32,
    /// Samples until the next target.
    left: u32,
}

impl Drift {
    const STILL: Drift = Drift { value: 0.0, target: 0.0, left: 0 };

    /// One sample on: `ease` is the share of the way to the target, `every` the samples between
    /// targets on average.
    fn step(&mut self, noise: &mut Noise, ease: f32, every: f32) {
        if self.left == 0 {
            self.target = noise.next();
            self.left = (every * (1.0 + 0.4 * noise.next())).max(1.0) as u32;
        }
        self.left -= 1;
        self.value += (self.target - self.value) * ease;
    }
}

/// How far a player's bow wanders, slowly (under 2.5 Hz): its speed, its distance from the
/// bridge and its weight on the string, as shares of each. Recorded violins played without
/// vibrato still swing each harmonic's level by almost 2 dB (a standard deviation), nearly all
/// of it this slowly, and with vibrato a fifth of their swing is this slow.
const ARM_SPEED: f32 = 0.05;
const ARM_PLACE: f32 = 0.15;
const ARM_WEIGHT: f32 = 0.06;
/// Pitch and the arm's wander are worked out every this many samples.
const CONTROL: u32 = 8;

/// A drift's new target comes every so often, and it eases towards it this quickly.
const DRIFT_EVERY: f32 = 0.35;
const DRIFT_EASE: f32 = 0.12;

/// Forte's velocity (96 of 127), and how much of each step past it the bow still answers.
const FORTE: f32 = 96.0 / 127.0;
const PAST_FORTE: f32 = 0.35;

/// The furthest along the string the bow goes.
const BETA_MAX: f32 = 0.14;

/// How much shorter than its open length the string sounding `freq` is stopped: players take a
/// note on the highest string it can be played on (on a violin, everything from E5 up is on the
/// E string), so it is at most a fifth short until the top string, which goes on up.
fn stopped(freq: f32, body: BodyKind) -> f32 {
    let open = body
        .open_strings()
        .iter()
        .map(|&m| 440.0 * 2f32.powf((f32::from(m) - 69.0) / 12.0))
        .filter(|&f| f <= freq * 1.001)
        .fold(None, |best: Option<f32>, f| Some(best.map_or(f, |b| b.max(f))));
    open.map_or(1.0, |open| (freq / open).clamp(1.0, 4.0))
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

/// At mezzo-forte in the middle of the instrument's range, measured on recordings (see
/// [`vibrato`]): the larger instruments narrower, the double bass much slower and narrower
/// (3.8 times a second, 3 to 5 cents either way).
fn player(body: BodyKind) -> Player {
    match body {
        BodyKind::Violin | BodyKind::Piano => Player { depth: 0.14, rate: 5.0, ring: 1.6 },
        BodyKind::Viola => Player { depth: 0.13, rate: 5.0, ring: 2.0 },
        BodyKind::Cello => Player { depth: 0.11, rate: 4.9, ring: 3.0 },
        BodyKind::DoubleBass => Player { depth: 0.055, rate: 3.7, ring: 4.0 },
    }
}

/// How a player's vibrato changes with the dynamic and the register, as shares of its
/// mezzo-forte depth and rate. Recorded violins (VSCO 2 and Sonatina's solo violins, every
/// note, with and without vibrato) vibrate about 5.2 times a second, slower and narrower softly
/// (4.7 Hz, 8 cents either way at piano) than loudly (5.4 Hz, 14 cents at forte), and wider
/// high on the instrument than low down.
fn vibrato(velocity: f32, freq: f32, body: BodyKind) -> (f32, f32) {
    let low = 440.0 * 2f32.powf((f32::from(body.open_strings().first().copied().unwrap_or(55)) - 69.0) / 12.0);
    let octaves = (freq.max(MIN_FREQ) / low).log2();
    let depth = (0.2 + 1.3 * velocity) * (0.8 + 0.2 * octaves).clamp(0.75, 1.4);
    let rate = 0.87 + 0.38 * velocity;
    (depth, rate)
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
    /// One player of a section: blending in, each note swells in more slowly and vibrates
    /// narrower (see [`SECTION_SWELL`]).
    pub section: bool,
}

/// How much more slowly a section's players swell into a held note, and how much narrower they
/// vibrate, than a soloist. Recorded violin sections take 0.5 to 0.7 s to reach a held note where
/// solo violins take 0.15 to 0.4, and their vibrato, heard as one, is 5 to 6 cents either way
/// where a soloist's is 9 to 14: a dozen players' vibratos, never together, blur; four, here,
/// vibrate narrower to blur as much.
const SECTION_SWELL: f32 = 2.5;
const SECTION_VIBRATO: f32 = 0.7;

pub struct BowedVoice {
    pub track: usize,
    pub string: u8,
    pub body: BodyKind,
    /// Which player of a section, and which of the track's bodies it sounds through.
    pub seat: usize,
    pub slot: usize,
    /// Gains into the body's left and right: players of a section sit apart.
    pub pan: (f32, f32),
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
    lp_delay: f32,
    /// The corner's shelf (see [`corner`]) and its state.
    corner: (f32, f32),
    corner_state: f32,
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
    /// This stroke's vibrato against the player's own: (depth, rate).
    vib_scale: (f32, f32),
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
    /// The junction's margin and the string's lag behind the bow (`dv` less what the bow put
    /// in) at the last sample, to place the next change of state between samples.
    margin: f32,
    offset: f32,
    tune: f32,
    noise: Noise,
    /// The rosin's noise, smoothed twice: its band is the difference.
    hiss: f32,
    hiss_low: f32,
    hiss_coef: (f32, f32),
    hiss_amount: f32,
    /// The bow's slow wander in speed, place and weight.
    arm: Drift,
    place: Drift,
    weight: Drift,
    drift_ease: f32,
    /// Samples until `control` next runs, and what it worked out: the pitch's step in phase and
    /// the two lines' lengths.
    ctl: u32,
    phase_step: f32,
    to_bridge: f32,
    to_neck: f32,
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
    /// The string's own rate, `os` times the output's.
    sr: f32,
    os: u32,
    out_sr: f32,
    /// Under the fundamental, at the output's rate (see [`UNDER`]).
    under: Biquad,
}

/// A string cannot vibrate below its fundamental, but the stick-slip, worked out sample by
/// sample, leaks a little there: irregular by a fraction of a sample from period to period, it
/// leaves a broad hump of noise from DC up, which the body's strong low modes turn into a growl
/// under high notes. A high-pass this far under the fundamental takes it away and leaves the
/// harmonics be. Recorded violins have 35 to 40 dB less under their fundamentals than in them.
const UNDER: f32 = 0.55;

/// The string runs at a multiple of the output rate, enough for this many samples a period: with
/// fewer, the moment of each slip is too coarse and the bow now and then slips twice in a period,
/// a burst of noise that the body turns into a growl under high notes (a violin's A6, 27
/// samples a period at 48 kHz, slipped twice about eight times a second; at 54 not once).
const SAMPLES_PER_PERIOD: f32 = 40.0;
const MAX_OVERSAMPLE: u32 = 4;

fn oversample(freq: f32, sr: f32) -> u32 {
    let n = (SAMPLES_PER_PERIOD * freq.max(MIN_FREQ) / sr.max(1.0)).ceil();
    if n.is_finite() { (n as u32).clamp(1, MAX_OVERSAMPLE) } else { 1 }
}

/// The bow's distance from the bridge (a share of the string) at the softest and the loudest, its
/// speed (in the waveguide's units) at mezzo-forte and how it grows, and its force (a share of
/// the most the Helmholtz motion survives), softly and how much harder loudly. Players move
/// towards the bridge and draw faster as they play louder: from about a sixth of the string at
/// pianissimo to a twentieth at fortissimo. Together these give the 12 to 13 dB between piano
/// and forte of recorded violins, and the forte's brighter top.
const BETA_SOFT: f32 = 0.16;
const BETA_LOUD: f32 = 0.075;
const SPEED_MF: f32 = 0.146;
const SPEED_GROWTH: f32 = 2.8;
const PRESSURE: f32 = 0.3;
const PRESSURE_LOUD: f32 = 0.3;

/// How hard a player presses for a note at `freq`, as a share of the stroke's force: as hard as
/// the string needs low down (lighter and a long string slips twice a period, an octave up),
/// lighter high up, where a heavy bow chokes a short string into slipping in a pattern of two or
/// four periods (anomalous low frequencies: a growl an octave or two under the note). Held to
/// what recorded violins show, nothing within 35 dB under the fundamental of a high note.
fn touch(freq: f32) -> f32 {
    curve(&[(6.0, 1.15), (7.6, 1.0), (8.8, 1.0), (10.8, 0.45)], freq.max(MIN_FREQ).log2())
}
/// The rosin's grip varies this much, sample to sample.
const HISS: f32 = 0.1;
/// The rosin's noise lies between the harmonics, where recorded violins have it about 40 dB
/// under their strongest harmonic up to 1 kHz and 45 to 60 dB under above, and hardly at all
/// under a high note's fundamental, where the string does not resonate. A band of noise, its
/// edges in hertz.
const HISS_LOW_HZ: f32 = 350.0;
const HISS_HIGH_HZ: f32 = 1700.0;

/// Overall level, so a forte bowed note sits near a plucked string's level and a piano one
/// 12 dB under it (as recorded), leaving room for a section's fortissimo chords.
const OUTPUT: f32 = 0.156;

impl BowedVoice {
    pub fn new(track: usize, string: u8, body: BodyKind, stroke: &Stroke, seed: u32, sr: f32) -> BowedVoice {
        let out_sr = if sr.is_finite() { sr.clamp(8_000.0, 384_000.0) } else { 48_000.0 };
        let stroke = &Stroke { freq: if stroke.freq.is_finite() { stroke.freq.clamp(MIN_FREQ, out_sr * 0.2) } else { 440.0 }, ..*stroke };
        // Room for the string to be stopped a little shorter later in the voice.
        let os = oversample(stroke.freq * 1.2, out_sr);
        let sr = out_sr * os as f32;
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
            pan: (1.0, 1.0),
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
            lp_delay: 0.0,
            corner: (1.0, 1.0),
            corner_state: 0.0,
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
            vib_scale: (1.0, 1.0),
            note_t: 0.0,
            vib_start: f32::MAX,
            vib_full: f32::MAX,
            settle: 0.0,
            settle_mul: 1.0,
            tremolo: None,
            phase: 0.0,
            stuck: true,
            margin: 0.0,
            offset: 0.0,
            tune: 1.0,
            noise,
            hiss: 0.0,
            hiss_low: 0.0,
            hiss_coef: (one_pole(HISS_HIGH_HZ, sr), one_pole(HISS_LOW_HZ, sr)),
            hiss_amount: HISS,
            arm: Drift::STILL,
            place: Drift::STILL,
            weight: Drift::STILL,
            drift_ease: 1.0 - (-(CONTROL as f32) / (DRIFT_EASE * sr)).exp(),
            ctl: 0,
            phase_step: 0.0,
            to_bridge: 2.0,
            to_neck: 2.0,
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
            os,
            out_sr,
            under: Biquad::default(),
        };
        v.bow(stroke);
        v
    }

    /// A new stroke, from rest or on the sounding string.
    pub fn bow(&mut self, s: &Stroke) {
        let s = &Stroke { freq: if s.freq.is_finite() { s.freq.clamp(MIN_FREQ, self.out_sr * 0.2) } else { 440.0 }, ..*s };
        let vel = s.velocity.clamp(0.0, 1.0);
        // Past forte a player has little more to give: fortissimo is a few dB over forte, not
        // as much again as forte is over piano.
        let vel = if vel > FORTE { FORTE + (vel - FORTE) * PAST_FORTE } else { vel };
        self.target = s.freq.max(MIN_FREQ).log2();
        self.log_freq = self.target;
        self.glide_per_sample = 0.0;
        // Louder is a faster bow, nearer the bridge, pressed harder. The bow stays a few
        // centimetres from the bridge whatever the note, so on a string stopped short (high on
        // it) it is further along the sounding length: a twentieth of an open E string is a
        // fifth of the string under an A two octaves above it.
        // A long, low string has a narrow window between too light a bow (it slips twice a
        // period) and too heavy (raucous); drawn loudly it stays a little further out.
        let loud = BETA_LOUD * curve(&[(6.0, 1.4), (7.6, 1.15), (8.8, 1.0)], s.freq.max(MIN_FREQ).log2());
        self.beta = (BETA_SOFT * (loud / BETA_SOFT).powf(vel) * stopped(s.freq, self.body)).min(BETA_MAX);
        let speed = SPEED_MF * (SPEED_GROWTH * (vel - 0.63)).exp();
        self.pressure = (PRESSURE + PRESSURE_LOUD * vel) * touch(s.freq);
        let force = self.pressure * 2.0 * speed / (self.beta * (MU_STATIC - MU_DYNAMIC));
        // A long soft note is drawn in gently, a loud one bitten into: recorded violins take
        // half a second or more to reach a held piano, and a few hundredths to reach forte.
        // Shorter notes have no time to swell.
        let swell = (0.04 + 1.2 * (1.0 - vel) * (1.0 - vel)) * (s.seconds / 2.5).clamp(0.25, 1.0) * if s.section { SECTION_SWELL } else { 1.0 };
        let attack = match (s.short, s.slow) {
            (true, _) => 0.025f32,
            (false, false) => swell,
            (false, true) => swell.max(0.35),
        }
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
        self.under.set(Shape::HighPass, s.freq * UNDER, 0.707, 0.0, self.out_sr);
        self.lp = corner_filter(s.freq, self.sr);
        self.lp_delay = one_pole_delay(self.lp, s.freq, self.sr);
        self.corner = corner(s.freq, vel, self.sr);
        self.note_t = 0.0;
        let long = s.seconds > 0.25;
        (self.vib_start, self.vib_full) = if s.open || !long {
            (f32::MAX, f32::MAX)
        } else {
            let start = (0.12 + 0.12 * (self.noise.next() + 1.0) * 0.5).min(s.seconds * 0.3) * self.sr;
            (start, start + 0.3 * self.sr)
        };
        self.ctl = 0;
        self.vib_scale = vibrato(vel, s.freq, self.body);
        if s.section {
            self.vib_scale.0 *= SECTION_VIBRATO;
        }
        self.settle = 0.06 * self.noise.next() / 12.0;
        self.settle_mul = (-1.0 / (0.05 * self.sr)).exp();
        self.tremolo = s.tremolo.then_some((0.0, self.sr / (2.0 * 7.5)));
    }

    /// A slurred note: the finger moves under the same stroke.
    pub fn slur_to(&mut self, freq: f32, open: bool, seconds: f32) {
        let freq = if freq.is_finite() { freq.clamp(MIN_FREQ, self.out_sr * 0.2) } else { 440.0 };
        self.target = freq.max(MIN_FREQ).log2();
        let samples = 0.03 * self.sr;
        self.glide_per_sample = (self.target - self.log_freq) / samples;
        self.ctl = 0;
        self.released = false;
        self.bow_on = true;
        self.loop_gain = loop_gain(freq, open, self.style.ring);
        self.corner.0 = corner(freq, 0.63, self.sr).0;
        self.under.set(Shape::HighPass, freq.min(2f32.powf(self.log_freq)) * UNDER, 0.707, 0.0, self.out_sr);
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
            self.speed.to(0.0, self.release_len);
            self.force.to(0.0, self.release_len);
        }
    }

    /// Another note takes the string: the bow leaves this one at once.
    pub fn damp(&mut self) {
        self.released = true;
        self.speed.to(0.0, 0.004 * self.sr);
        self.force.to(0.0, 0.004 * self.sr);
        self.loop_gain = self.loop_gain.min(0.9);
    }

    pub fn finished(&self) -> bool {
        let quiet = self.released && self.force.value() <= 0.0 && self.level < 1e-5;
        quiet || self.age > (self.sr * MAX_RING) as u32
    }

    /// Whether this voice's string runs fast enough for a note at `freq`.
    pub fn fits(&self, freq: f32) -> bool {
        oversample(freq, self.out_sr) <= self.os
    }

    /// One sample at the output rate.
    #[inline]
    pub fn tick(&mut self) -> f32 {
        if self.wait > 0 {
            self.wait -= 1;
            return 0.0;
        }
        let y = if self.os <= 1 {
            self.step()
        } else {
            let mut y = 0.0;
            for _ in 0..self.os {
                y += self.step();
            }
            y / self.os as f32
        };
        self.under.tick(0, y)
    }

    /// Pitch (a slur's glide, the score's bends, the player's vibrato and intonation) and the
    /// arm's slow wander move smoothly: worked out every [`CONTROL`] samples, not every one.
    fn control(&mut self) {
        let n = CONTROL as f32;
        self.ctl = CONTROL;
        if self.glide_per_sample != 0.0 {
            self.log_freq += self.glide_per_sample * n;
            if (self.glide_per_sample > 0.0) == (self.log_freq >= self.target) {
                self.log_freq = self.target;
                self.glide_per_sample = 0.0;
            }
        }
        self.note_t += n;
        self.settle *= self.settle_mul.powi(CONTROL as i32);
        let mut semis = self.bend_semis + self.detune + self.settle * 12.0;
        let natural = ((self.note_t - self.vib_start) / (self.vib_full - self.vib_start).max(1.0)).clamp(0.0, 1.0);
        // A vibrato marked in the score is there from the start, and deeper.
        let own = self.vib_depth * self.vib_scale.0;
        let depth = match (self.vibrato, self.wide) {
            (false, _) => own * natural,
            (true, false) => own * 1.5,
            (true, true) => own * 2.4,
        };
        if depth > 0.0 {
            self.vib_phase += n * self.vib_rate * self.vib_scale.1 * (1.0 + 0.04 * self.arm.value) / self.sr;
            if self.vib_phase >= 1.0 {
                self.vib_phase -= 1.0;
            }
            semis += depth * (self.vib_phase * std::f32::consts::TAU).sin();
        }
        let freq = (self.log_freq + semis / 12.0).exp2();
        // The loop: both lines, the bridge's low-pass, less the low-pass's own delay.
        self.lp_delay = one_pole_delay(self.lp, freq, self.sr);
        let total = ((self.sr / freq - self.lp_delay) * self.tune).max(4.5);
        self.phase_step = freq / self.sr;
        self.to_bridge = (total * self.beta * (1.0 + ARM_PLACE * self.place.value)).max(2.0);
        self.to_neck = (total - self.to_bridge).max(2.0);
        // The arm: slow wander in the bow's speed, place and weight.
        let every = DRIFT_EVERY * self.sr / n;
        let ease = self.drift_ease;
        self.arm.step(&mut self.noise, ease, every);
        self.place.step(&mut self.noise, ease, every);
        self.weight.step(&mut self.noise, ease, every);
    }

    /// One sample at the string's rate.
    #[inline]
    fn step(&mut self) -> f32 {
        if self.ctl == 0 {
            self.control();
        }
        self.ctl -= 1;
        self.phase += self.phase_step;
        let (to_bridge, to_neck) = (self.to_bridge, self.to_neck);

        if let Some((t, half)) = self.tremolo.as_mut() {
            *t += 1.0;
            if *t >= *half && !self.released {
                *t = 0.0;
                self.direction = -self.direction;
                let s = self.speed.to.abs();
                self.speed.to(self.direction * s, *half * 0.5);
            }
        }
        // The bow: rosin noise in its grip, and the arm's slow wander (in `control`).
        self.hiss += self.hiss_coef.0 * (self.noise.next() - self.hiss);
        self.hiss_low += self.hiss_coef.1 * (self.hiss - self.hiss_low);
        let speed = self.speed.tick() * (1.0 + ARM_SPEED * self.arm.value);
        let force = self.force.tick() * (1.0 + ARM_WEIGHT * self.weight.value) * (1.0 + self.hiss_amount * (self.hiss - self.hiss_low));
        if self.released && self.force.value() <= 0.0 {
            self.bow_on = false;
        }

        let from_bridge = self.bridge.read(to_bridge);
        let from_neck = self.neck.read(to_neck);
        self.lp_state += self.lp * (from_bridge - self.lp_state);
        let bridge_reflection = -self.lp_state * self.loop_gain;
        let nut_reflection = -from_neck;
        let incoming = bridge_reflection + nut_reflection;
        let dv = speed - incoming;
        let bowing = self.bow_on && force > 0.0;
        let (mut added, stuck) = if bowing { friction(dv, force, self.stuck) } else { (0.0, true) };
        // The string changes state at some moment between two samples, not on one: where the
        // margin crossed zero. Placing the step there (a band-limited step, spread over this
        // sample and the last) keeps the period from jittering by up to a sample, which is
        // heard as hiss between the harmonics.
        let mut since = 0.0;
        if bowing && stuck != self.stuck && self.margin > 0.0 {
            let now = margin(dv, force, self.stuck);
            let tau = if self.margin - now > 1e-12 { (self.margin / (self.margin - now)).clamp(0.0, 1.0) } else { 1.0 };
            since = 1.0 - tau;
            let step = added - (dv - self.offset);
            added -= step * 0.5 * (1.0 - since) * (1.0 - since);
            let earlier = step * 0.5 * since * since;
            self.neck.nudge_last(earlier);
            self.bridge.nudge_last(earlier);
        }
        if self.stuck && !stuck && self.bow_on {
            // A slip starts: one period of the stick-slip cycle has passed.
            // A second slip within a period (the attack, a raucous stroke) is not a period.
            let error = self.phase - 1.0;
            let reset = since * self.phase_step;
            if error.abs() < 0.08 {
                self.tune = (self.tune * (1.0 - 0.15 * error)).clamp(0.9, 1.1);
                self.phase = reset;
            } else if error > 0.0 {
                self.phase = reset;
            }
        }
        self.stuck = stuck;
        self.margin = if bowing { margin(dv, force, stuck) } else { 0.0 };
        self.offset = dv - added;
        self.neck.push(bridge_reflection + added);
        self.bridge.push(nut_reflection + added);

        self.age = self.age.saturating_add(1);
        self.level = self.level * 0.999 + from_bridge.abs() * 0.001;
        let (a, gain) = self.corner;
        self.corner_state += a * (from_bridge - self.corner_state);
        let shaped = self.corner_state + gain * (from_bridge - self.corner_state);
        let y = shaped * OUTPUT * self.gain * self.level_scale;
        if y.is_finite() { y } else { 0.0 }
    }
}

/// How far a section's players are from each other: tuning (semitones), onset (seconds, up to
/// 40 ms on a long note; the player draws them closer on short ones), place.
pub fn section_player(i: usize, seed: u32) -> (f32, f32, f32) {
    let h = |k: u32| crate::piano::hash01(seed.wrapping_add(i as u32 * 7919), k) * 2.0 - 1.0;
    let detune = 0.12 * h(1);
    let onset = 0.02 * (h(2) + 1.0);
    let place = curve(&[(0.0, -0.7), (3.0, 0.7)], i as f32) + 0.15 * h(3);
    (detune, onset, place.clamp(-1.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn stroke(freq: f32, velocity: f32) -> Stroke {
        Stroke { freq, velocity, seconds: 0.2, short: false, open: false, tremolo: false, slow: false, direction: 1.0, crossing: false, section: false }
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

    #[test]
    fn the_bow_sets_up_helmholtz_motion_across_the_range() {
        // One full slip per period (none extra, none missed) from a cello's C to a violin's
        // high E, at every dynamic a score has (ppp to fff), whatever the arm's wander and the
        // rosin's noise do.
        for f in [65.4f32, 98.0, 146.8, 196.0, 293.7, 440.0, 659.3, 880.0, 1318.5, 1760.0, 2637.0] {
            for (vel, seed) in [22.0f32 / 127.0, 0.3, 0.5, 0.7, 0.9, 126.0 / 127.0].into_iter().flat_map(|v| [(v, 1), (v, 5)]) {
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
                let per_period = slips as f32 / (f * 0.25);
                // The lowest strings may slip once more now and then, as a cello's C string does;
                // how often depends on the last bits of the arithmetic, so the test allows it.
                let allowed = if f < 100.0 { 0.12 } else { 0.06 };
                assert!((per_period - 1.0).abs() < allowed, "{f} Hz at {vel} (seed {seed}): {per_period} slips a period");
            }
        }
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
        let bowed = (0..(0.4 * SR) as usize).map(|_| v.tick().abs()).fold(0.0, f32::max);
        v.release();
        let ring: Vec<f32> = (0..(0.3 * SR) as usize).map(|_| v.tick()).collect();
        // Still sounding a fifth of a second after the bow has left (within 50 dB of the bowed
        // note; the bow stopping on the string damps it).
        assert!(ring.iter().skip((0.2 * SR) as usize).any(|s| s.abs() > 0.003 * bowed), "an open string rings on");
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

    /// A held note at a dynamic, its vibrato stopped so its harmonics stay put: the second after
    /// it has spoken.
    fn held(freq: f32, velocity: f32) -> Vec<f32> {
        let mut v = BowedVoice::new(0, 0, BodyKind::Violin, &Stroke { seconds: 4.0, ..stroke(freq, velocity) }, 1, SR);
        v.vib_scale.0 = 0.0;
        (0..(1.6 * SR) as usize).map(|_| v.tick()).skip((0.6 * SR) as usize).collect()
    }

    fn db(x: &[f32]) -> f32 {
        10.0 * (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).max(1e-20).log10()
    }

    /// Through `stages` second-order filters.
    fn filtered(x: &[f32], shape: Shape, freq: f32, stages: usize) -> Vec<f32> {
        let mut fs: Vec<Biquad> = (0..stages).map(|_| Biquad::new(shape, freq, 0.707, 0.0, SR)).collect();
        x.iter().map(|&s| fs.iter_mut().fold(s, |y, f| f.tick(0, y))).collect()
    }

    /// The amplitude of `x` at `freq` (a Hann-windowed DFT bin).
    fn partial(x: &[f32], freq: f32) -> f32 {
        let n = x.len() as f32;
        let w = std::f32::consts::TAU * freq / SR;
        let (re, im) = x.iter().enumerate().fold((0.0f32, 0.0f32), |(re, im), (i, s)| {
            let h = s * (0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n).cos());
            (re + h * (w * i as f32).cos(), im + h * (w * i as f32).sin())
        });
        (re * re + im * im).sqrt()
    }

    #[test]
    fn forte_is_as_much_louder_and_brighter_than_piano_as_on_recorded_violins() {
        // Recorded violins (VSCO 2's solo violin, the same notes at piano and at forte) are
        // 12.7 dB louder at forte, and their harmonics from the ninth up 7 dB stronger against
        // the fundamental.
        for f in [293.7f32, 440.0, 659.3, 987.8] {
            let (p, ff) = (held(f, 49.0 / 127.0), held(f, 96.0 / 127.0));
            let louder = db(&ff) - db(&p);
            let top = |x: &[f32]| db(&filtered(x, Shape::HighPass, 8.0 * f, 2)) - db(&filtered(x, Shape::LowPass, 2.5 * f, 2));
            let brighter = top(&ff) - top(&p);
            println!("{f} Hz: forte {louder:.1} dB louder, its top {brighter:.1} dB brighter");
            assert!((8.0..17.0).contains(&louder), "{f} Hz: {louder} dB");
            assert!(brighter > 3.0, "{f} Hz: {brighter} dB");
        }
    }

    #[test]
    fn high_notes_do_not_growl_under_their_fundamental() {
        // Recorded violins have 35 to 40 dB less under a high note's fundamental than in it. A
        // bow pressed as hard as a low string needs, or a slip timed only to the sample, puts a
        // growl there, an octave or two down.
        for (f, vel) in [(880.0f32, 0.39f32), (1318.5, 0.76), (1760.0, 0.76), (2637.0, 0.63), (1760.0, 0.17)] {
            let x = held(f, vel);
            let under = db(&filtered(&x, Shape::LowPass, 0.5 * f, 4)) - db(&x);
            println!("{f} Hz at {vel}: {under:.1} dB under the fundamental");
            assert!(under < -30.0, "{f} Hz at {vel}: {under} dB");
        }
    }

    #[test]
    fn a_violins_lowest_notes_have_weak_fundamentals() {
        // Below its air mode the box hardly radiates: recorded violins have the fundamental of
        // their open G 21 to 30 dB under their strongest harmonic, of A3 3 to 12 dB.
        for (f, most) in [(196.0f32, -15.0f32), (220.0, -6.0)] {
            let mut body = crate::body::Body::new(BodyKind::Violin, SR);
            let x = held(f, 0.63);
            let y: Vec<f32> = x
                .iter()
                .map(|&s| {
                    let (l, r) = body.tick(s, s);
                    0.5 * (l + r)
                })
                .skip((0.3 * SR) as usize)
                .collect();
            let h: Vec<f32> = (1..=6).map(|k| partial(&y, f * k as f32)).collect();
            let strongest = h.iter().skip(1).copied().fold(0.0, f32::max);
            let under = 20.0 * (h[0] / strongest).log10();
            println!("{f} Hz: fundamental {under:.1} dB under the strongest harmonic");
            assert!(under < most, "{f} Hz: {under} dB");
        }
    }

    #[test]
    fn vibrato_widens_and_quickens_with_the_dynamic_and_the_register() {
        // Recorded violins: 8 cents either way at 4.7 Hz at piano, 14 cents at 5.4 Hz at forte;
        // wider high on the instrument.
        let (pd, pr) = vibrato(49.0 / 127.0, 659.3, BodyKind::Violin);
        let (fd, fr) = vibrato(96.0 / 127.0, 659.3, BodyKind::Violin);
        assert!((1.5..2.1).contains(&(fd / pd)), "{pd} {fd}");
        assert!((1.08..1.25).contains(&(fr / pr)), "{pr} {fr}");
        let (low, _) = vibrato(0.63, 196.0, BodyKind::Violin);
        let (high, _) = vibrato(0.63, 1568.0, BodyKind::Violin);
        assert!(high > low * 1.3, "{low} {high}");
        // A section's players vibrate narrower and swell in more slowly.
        let mut solo = BowedVoice::new(0, 0, BodyKind::Violin, &Stroke { seconds: 4.0, ..stroke(440.0, 0.4) }, 1, SR);
        let mut player = BowedVoice::new(0, 0, BodyKind::Violin, &Stroke { seconds: 4.0, section: true, ..stroke(440.0, 0.4) }, 1, SR);
        assert!(player.vib_scale.0 < solo.vib_scale.0 * 0.8);
        let level = |v: &mut BowedVoice| (0..(0.15 * SR) as usize).map(|_| v.tick().abs()).sum::<f32>();
        assert!(level(&mut player) < level(&mut solo) * 0.8, "a section player is still swelling in");
    }

    #[test]
    fn high_strings_run_faster_than_the_output() {
        assert_eq!(oversample(440.0, SR), 1);
        assert_eq!(oversample(1760.0, SR), 2);
        assert!(oversample(2637.0, SR) >= 3);
        assert_eq!(oversample(f32::NAN, SR), 1);
        let v = BowedVoice::new(0, 0, BodyKind::Violin, &stroke(440.0, 0.5), 1, SR);
        assert!(v.fits(440.0) && !v.fits(2637.0));
    }
}
