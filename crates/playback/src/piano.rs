//! A grand piano, modelled rather than sampled.
//!
//! Each key's strings are a bank of modes: stiff strings, so the partials run progressively sharp;
//! two or three strings per key tuned a fraction of a cent apart, so they beat; and a slow
//! aftersound under the prompt sound, so the note decays in two stages. A felt hammer strikes
//! them, and its contact with the moving strings is simulated sample by sample: the felt is
//! nonlinear (`F = K δ^p`), so a harder blow compresses it into its stiffer range, the contact
//! gets shorter and the note brighter as well as louder. A key struck again hits strings that are
//! still moving. The voice's output is the force the strings push into the bridge; the
//! soundboard and case ([`crate::body`]) colour it per track.
//!
//! The string and hammer data follow published measurements of concert grands (Chaigne and
//! Askenfelt's hammer and string parameters, Conklin's scaling, Fletcher's inharmonicity) and
//! are interpolated across the keyboard.

use crate::body::curve;

/// Modes per block: the loop over a block is unrolled and vectorised.
const LANES: usize = 8;

/// The lowest and highest keys of a grand piano: A0 and C8.
const LOWEST: f32 = 21.0;
const HIGHEST: f32 = 108.0;
/// Keys above this have no damper and ring on when let go (F#6 and up on most grands).
const LAST_DAMPER: f32 = 89.0;
/// Partials stop below this, or at 45 % of the sample rate.
const TOP_PARTIAL: f32 = 15_000.0;
const MAX_PARTIALS: usize = 120;
/// Longest a note may sound.
const MAX_RING: f32 = 45.0;

#[derive(Clone, Copy, Default)]
struct Block {
    /// Each mode's state as a complex number: displacement is the real part.
    re: [f32; LANES],
    im: [f32; LANES],
    /// The pole: one sample's decay and rotation.
    cr: [f32; LANES],
    ci: [f32; LANES],
    /// The pole with the damper on the strings, and without it.
    dr: [f32; LANES],
    di: [f32; LANES],
    ur: [f32; LANES],
    ui: [f32; LANES],
    /// What an impulse from the hammer (newton-seconds) adds to the imaginary part.
    kick: [f32; LANES],
    /// The mode's share of the strings' displacement under the hammer (0 for the aftersound).
    contact: [f32; LANES],
    /// The mode's share of the force on the bridge.
    out: [f32; LANES],
}

/// The tone of a piano program.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PianoTone {
    /// Felt stiffness relative to a concert grand: harder felt is brighter.
    pub hardness: f32,
    /// How far apart a key's strings are tuned, in cents: a fraction for a tuned grand, many
    /// for a honky-tonk upright.
    pub unison: f32,
}

impl PianoTone {
    pub const GRAND: PianoTone = PianoTone { hardness: 1.0, unison: 0.25 };
    pub const BRIGHT: PianoTone = PianoTone { hardness: 2.2, unison: 0.3 };
    pub const HONKY_TONK: PianoTone = PianoTone { hardness: 1.6, unison: 14.0 };
}

/// One key's strings and hammer, in SI units.
#[derive(Clone, Copy, Debug)]
struct Key {
    f0: f32,
    strings: usize,
    /// The inharmonicity coefficient: partial n sounds at `n f0 sqrt(1 + B n²)`.
    b: f32,
    length: f32,
    tension: f32,
    /// Mass per metre.
    mu: f32,
    /// Where the hammer strikes, as a share of the speaking length from the agraffe.
    strike: f32,
    hammer_mass: f32,
    felt_p: f32,
    felt_k: f32,
    /// Seconds for the fundamental to fall by 60 dB: the prompt sound and the aftersound.
    t60: f32,
    t60_after: f32,
    after_level: f32,
    /// Partials this far above the fundamental decay twice as fast, three times this far three
    /// times as fast and so on (decay rates rise in proportion to frequency, as measured on real
    /// strings): a bass note starts bright and darkens within a second or two, a middle C keeps its
    /// tenth partial for two or three. Steeper, the upper partials vanish and leave a bell-like
    /// tone: an electric piano.
    loss_corner: f32,
    /// How far apart the aftersound's partials sit from the prompt sound's, in hertz: the string's
    /// two planes of motion and the bridge pull them apart, so each partial beats slowly at its own
    /// rate (the growl of the bass), whatever its frequency.
    polarisation_hz: f32,
    damped_t60: f32,
    damper: bool,
    /// Loudness voicing, so the keyboard sounds even.
    voicing: f32,
}

/// The strings' inharmonicity coefficient at key `m`: partial n sounds at `n f0 sqrt(1 + B n²)`.
/// Lowest in the wound strings of the tenor, rising steeply to the short treble strings.
fn inharmonicity(m: f32) -> f32 {
    10f32.powf(curve(&[(21.0, -3.45), (36.0, -3.7), (48.0, -3.78), (60.0, -3.4), (72.0, -3.0), (84.0, -2.6), (96.0, -2.2), (108.0, -1.8)], m))
}

/// The stretched tuning a piano tuner sets, in cents from equal temperament, worked out from the
/// strings' own inharmonicity the way a tuner works by ear: the temperament octave A3 to A4 is set
/// first, its octave just wide enough not to beat, then every other note against the note an
/// octave from it: above, its fundamental on the lower note's second partial; below, its fourth
/// partial on the upper note's second (the bass is tuned on the partials heard most). Tuned
/// against the model's strings, octaves do not beat; a fixed Railsback curve stretched further
/// than these strings need, and their octaves beat.
fn stretch_cents(m: f32) -> f32 {
    // How much wider than 2:1 the octave from `k` up must be for the lower note's partial `a` to
    // meet the upper note's partial `b`, each string with its own inharmonicity.
    let widening = |k: f32, a: f32, b: f32| {
        let (lower, upper) = (inharmonicity(k), inharmonicity(k + 12.0));
        1200.0 * ((1.0 + a * a * lower).sqrt() / (1.0 + b * b * upper).sqrt()).log2()
    };
    let m = m.clamp(LOWEST, HIGHEST);
    if m > 69.0 {
        stretch_cents(m - 12.0) + widening(m - 12.0, 2.0, 1.0)
    } else if m < 57.0 {
        stretch_cents(m + 12.0) - widening(m, 4.0, 2.0)
    } else {
        (m - 69.0) / 12.0 * widening(57.0, 2.0, 1.0)
    }
}

/// The frequency a key of the piano is tuned to, stretched.
pub fn tuned(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0 + stretch_cents(midi) / 1200.0)
}

/// A deterministic value in 0..1 from two integers, for each key's small irregularities.
pub(crate) fn hash01(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

fn key_model(freq: f32, tone: PianoTone) -> Key {
    let m = (69.0 + 12.0 * (freq.max(8.0) / 440.0).log2()).clamp(LOWEST - 12.0, HIGHEST + 12.0);
    let f0 = freq * 2f32.powf(stretch_cents(m.clamp(LOWEST, HIGHEST)) / 1200.0);
    let length = curve(&[(21.0, 2.0), (36.0, 1.85), (48.0, 1.15), (60.0, 0.62), (72.0, 0.32), (84.0, 0.165), (96.0, 0.09), (108.0, 0.052)], m);
    let tension = curve(&[(21.0, 1100.0), (40.0, 850.0), (60.0, 700.0), (108.0, 750.0)], m);
    let mu = tension / (2.0 * length * f0).powi(2);
    let strings = if m <= 28.5 {
        1
    } else if m <= 40.5 {
        2
    } else {
        3
    };
    Key {
        f0,
        strings,
        b: inharmonicity(m),
        length,
        tension,
        mu,
        strike: curve(&[(21.0, 0.12), (72.0, 0.12), (84.0, 0.1), (96.0, 0.075), (108.0, 0.06)], m),
        hammer_mass: curve(&[(21.0, 6.5e-3), (36.0, 4.9e-3), (60.0, 2.97e-3), (96.0, 2.2e-3), (108.0, 2.0e-3)], m),
        felt_p: curve(&[(21.0, 2.2), (36.0, 2.3), (60.0, 2.5), (96.0, 3.0), (108.0, 3.1)], m),
        felt_k: 10f32.powf(curve(&[(21.0, 8.3), (36.0, 8.6), (60.0, 9.65), (96.0, 11.0), (108.0, 11.4)], m)) * tone.hardness,
        t60: curve(&[(21.0, 14.0), (36.0, 11.0), (48.0, 9.0), (60.0, 6.5), (72.0, 4.5), (84.0, 3.2), (96.0, 2.0), (108.0, 0.9)], m),
        t60_after: curve(&[(21.0, 42.0), (36.0, 36.0), (60.0, 23.0), (84.0, 9.0), (96.0, 4.5), (108.0, 1.6)], m),
        after_level: curve(&[(21.0, 0.2), (48.0, 0.17), (60.0, 0.13), (96.0, 0.1), (108.0, 0.08)], m),
        loss_corner: curve(&[(21.0, 250.0), (48.0, 500.0), (60.0, 900.0), (72.0, 1400.0), (84.0, 2200.0), (96.0, 3200.0), (108.0, 4500.0)], m),
        polarisation_hz: curve(&[(21.0, 0.3), (48.0, 0.22), (72.0, 0.15), (108.0, 0.1)], m),
        damped_t60: curve(&[(21.0, 0.9), (48.0, 0.45), (72.0, 0.25), (88.0, 0.18)], m),
        damper: m < LAST_DAMPER,
        voicing: curve(&[(21.0, 1.0), (108.0, 1.0)], m),
    }
}

/// The felt hammer while it touches the strings (and a few milliseconds after it leaves them).
#[derive(Clone, Copy, Debug)]
struct Hammer {
    /// Position and velocity, metres and metres per second, along the strings' motion.
    y: f32,
    v: f32,
    mass: f32,
    k: f32,
    p: f32,
    strings: f32,
    /// The felt's compression a sample ago, for its hysteresis.
    last_compression: f32,
    /// Samples since the blow.
    t: u32,
    /// Samples it may stay in flight.
    limit: u32,
}

/// Felt is hysteretic: it pushes back harder while compressed than while it recovers.
const FELT_DAMPING: f32 = 0.1;

pub struct PianoVoice {
    pub track: usize,
    /// The key, as a MIDI note: a key struck again restrikes its own strings.
    pub key: u8,
    blocks: Vec<Block>,
    hammer: Option<Hammer>,
    /// The hammer's knock in the case: two short resonances driven by its force (state, then
    /// pole).
    knock: [(f32, f32, f32, f32); 2],
    knock_gain: f32,
    /// The felt's thud and the action's noise: noise shaped by the hammer's force, low-passed
    /// twice (state, coefficient, level).
    thump: (f32, f32),
    thump_lp: f32,
    thump_gain: f32,
    noise: crate::synth::Noise,
    /// Phantom partials: the strings stretch as they swing, so their tension (and the force on
    /// the bridge along them) moves at sums of the partials' frequencies, louder the harder the
    /// blow; strong in the bass, gone in the treble. The square of the bridge force, its DC taken
    /// off (coefficient, then the blocker's state).
    phantom: f32,
    phantom_state: (f32, f32),
    model: Key,
    /// Gains to the left and right: bass to the left, as the pianist hears it.
    pan: (f32, f32),
    pub released: bool,
    /// Rings on when let go, as with the sustain pedal down.
    pub let_ring: bool,
    pub note_end: f64,
    level: f32,
    age: u32,
    sr: f32,
    /// A new note on the same key: the old strings fall silent within milliseconds.
    choked: bool,
    choke_gain: f32,
    out_gain: f32,
}

/// The key a frequency is played on.
pub fn key_of(freq: f32) -> u8 {
    (69.0 + 12.0 * (freq.max(8.0) / 440.0).log2()).round().clamp(0.0, 127.0) as u8
}

/// How fast a hammer travels for a score velocity (0..1): about 0.4 m/s at ppp, 5 m/s at fff.
fn hammer_speed(velocity: f32) -> f32 {
    0.3 * 16f32.powf(velocity.clamp(0.0, 1.0))
}

/// The pole for a mode at `freq` falling 60 dB in `t60` seconds.
fn pole(freq: f32, t60: f32, sr: f32) -> (f32, f32) {
    let r = (-6.907_755 / (t60.max(1e-3) * sr)).exp();
    let w = std::f32::consts::TAU * freq / sr;
    (r * w.cos(), r * w.sin())
}

/// The hammer's thud per newton of force, and the phantom partials' strength in the bass.
const THUMP: f32 = 0.025;
const PHANTOM: f32 = 0.3;

/// Overall level, so a mezzo-forte middle C peaks around a plucked string's level.
const OUTPUT: f32 = 2.75e-2;

impl PianoVoice {
    pub fn new(track: usize, freq: f32, velocity: f32, tone: PianoTone, sr: f32) -> PianoVoice {
        let freq = if freq.is_finite() { freq.clamp(8.0, 20_000.0) } else { 440.0 };
        let sr = if sr.is_finite() { sr.clamp(8_000.0, 384_000.0) } else { 48_000.0 };
        let model = key_model(freq, tone);
        let key = key_of(freq);
        let nyquist = (sr * 0.45).min(TOP_PARTIAL);
        // The unison strings' detuning in cents: a fixed pattern per key, scaled by the tone.
        let spread = tone.unison * (0.6 + 0.8 * hash01(u32::from(key), 7));
        let detune: &[f32] = match model.strings {
            1 => &[0.0],
            2 => &[0.5, -0.5],
            _ => &[0.0, 1.0, -0.8],
        };
        let mode_mass = model.mu * model.length / 2.0;
        let mut modes: Vec<(f32, f32, f32, f32, f32, f32)> = Vec::new();
        for n in 1..=MAX_PARTIALS {
            let nf = n as f32;
            let fn_ = nf * model.f0 * (1.0 + model.b * nf * nf).sqrt();
            if fn_ >= nyquist {
                break;
            }
            let shape = (std::f32::consts::PI * nf * model.strike).sin();
            if shape.abs() < 0.02 {
                continue;
            }
            // Losses rise with frequency (air, the felt-covered ends, the soundboard), with some
            // irregularity per partial as the soundboard couples to each differently.
            let irregular = 0.8 + 0.45 * hash01(u32::from(key), n as u32);
            let rise = 1.0 + (fn_ - model.f0).max(0.0) / model.loss_corner;
            let t60 = model.t60 * irregular / rise;
            let t60_after = (model.t60_after * irregular / rise).max(t60);
            let damped = model.damped_t60.min(t60);
            // A newton-second of impulse at the striking point moves the mode this much.
            let omega = std::f32::consts::TAU * fn_;
            let kick = -shape / (mode_mass * omega);
            // The string's slope at the bridge drives it: n pi / L, alternating in sign.
            let sign = if n.is_multiple_of(2) { 1.0 } else { -1.0 };
            let out = sign * nf * std::f32::consts::PI * model.tension / model.length * OUTPUT * model.voicing;
            let share = 1.0 / model.strings as f32;
            for (s, cents) in detune.iter().enumerate() {
                let f = fn_ * 2f32.powf(cents * spread / 1200.0);
                // Each string's prompt decay differs a little.
                let t = t60 * (0.9 + 0.2 * hash01(u32::from(key) * 3 + s as u32, n as u32));
                modes.push((f, t, damped, kick, shape * share, out * share));
            }
            // The aftersound: the strings' slow motion, as the bridge lets it through.
            let f = fn_ + (hash01(u32::from(key), n as u32 + 999) * 2.0 - 1.0) * model.polarisation_hz;
            modes.push((f, t60_after, damped, kick * model.after_level, 0.0, out));
        }
        let mut blocks = Vec::with_capacity(modes.len().div_ceil(LANES));
        for chunk in modes.chunks(LANES) {
            let mut b = Block::default();
            for (lane, &(f, t60, damped, kick, contact, out)) in chunk.iter().enumerate() {
                let (ur, ui) = pole(f, t60, sr);
                let (dr, di) = pole(f, damped, sr);
                if let (Some(a), Some(b2), Some(c), Some(d), Some(e), Some(g), Some(h), Some(i), Some(j)) = (
                    b.cr.get_mut(lane),
                    b.ci.get_mut(lane),
                    b.ur.get_mut(lane),
                    b.ui.get_mut(lane),
                    b.dr.get_mut(lane),
                    b.di.get_mut(lane),
                    b.kick.get_mut(lane),
                    b.contact.get_mut(lane),
                    b.out.get_mut(lane),
                ) {
                    (*a, *b2, *c, *d, *e, *g, *h, *i, *j) = (ur, ui, ur, ui, dr, di, kick, contact, out);
                }
            }
            blocks.push(b);
        }
        let m = f32::from(key).clamp(LOWEST, HIGHEST);
        let pan = ((m - 64.5) / 43.5 * 0.55).clamp(-0.6, 0.6);
        let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
        let knock = [(95.0, 0.05), (240.0, 0.03)].map(|(f, t60)| {
            let (cr, ci) = pole(f, t60, sr);
            (0.0, 0.0, cr, ci)
        });
        let mut v = PianoVoice {
            track,
            key,
            blocks,
            hammer: None,
            knock,
            knock_gain: curve(&[(21.0, 0.002), (60.0, 0.004), (108.0, 0.012)], m),
            thump: (0.0, 0.0),
            thump_lp: 1.0 - (-std::f32::consts::TAU * curve(&[(21.0, 900.0), (60.0, 1800.0), (108.0, 3500.0)], m) / sr).exp(),
            thump_gain: curve(&[(21.0, THUMP * 0.5), (60.0, THUMP), (108.0, THUMP * 2.5)], m),
            noise: crate::synth::Noise::new(u32::from(key).wrapping_mul(2_654_435_761) | 1),
            phantom: curve(&[(21.0, PHANTOM), (48.0, PHANTOM * 0.6), (72.0, PHANTOM * 0.15), (84.0, 0.0)], m),
            phantom_state: (0.0, 0.0),
            model,
            pan: (angle.cos() * std::f32::consts::SQRT_2, angle.sin() * std::f32::consts::SQRT_2),
            released: false,
            let_ring: false,
            note_end: f64::MAX,
            level: 0.0,
            age: 0,
            sr,
            choked: false,
            choke_gain: 1.0,
            out_gain: 1.0,
        };
        v.strike(velocity);
        v
    }

    /// The hammer strikes (again): it meets the strings where they are now.
    pub fn strike(&mut self, velocity: f32) {
        let y = self.displacement();
        self.hammer = Some(Hammer {
            y,
            v: hammer_speed(velocity),
            mass: self.model.hammer_mass,
            k: self.model.felt_k,
            p: self.model.felt_p,
            strings: self.model.strings as f32,
            last_compression: 0.0,
            t: 0,
            limit: (0.03 * self.sr) as u32,
        });
        // The key is down again: the damper lifts.
        for b in &mut self.blocks {
            (b.cr, b.ci) = (b.ur, b.ui);
        }
        self.released = false;
        self.age = 0;
        self.level = self.level.max(1e-3);
    }

    /// The strings' mean displacement under the hammer.
    fn displacement(&self) -> f32 {
        self.blocks.iter().map(|b| b.contact.iter().zip(&b.re).map(|(c, r)| c * r).sum::<f32>()).sum()
    }

    /// The key is let go: the damper falls on the strings, unless the note rings on or the key
    /// has none.
    pub fn release(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        if self.model.damper && !self.let_ring {
            for b in &mut self.blocks {
                (b.cr, b.ci) = (b.dr, b.di);
            }
        }
    }

    /// Another voice takes over this key: this one stops quickly.
    pub fn choke(&mut self) {
        self.released = true;
        self.choked = true;
    }

    /// Being cut short for another voice.
    pub fn is_choked(&self) -> bool {
        self.choked
    }

    pub fn finished(&self) -> bool {
        let settled = self.hammer.is_none() && self.age > (self.sr * 0.05) as u32;
        (settled && self.level < 2e-5) || self.choke_gain < 1e-4 || self.age > (self.sr * MAX_RING) as u32
    }

    /// One sample, left and right.
    #[inline]
    pub fn tick(&mut self) -> (f32, f32) {
        let dt = 1.0 / self.sr;
        let mut force = 0.0;
        if let Some(h) = self.hammer.as_mut() {
            let y = self.blocks.iter().map(|b| b.contact.iter().zip(&b.re).map(|(c, r)| c * r).sum::<f32>()).sum::<f32>();
            let compression = h.y - y;
            if compression > 0.0 {
                let rate = (compression - h.last_compression) / dt;
                force = (h.k * compression.powf(h.p) * (1.0 + FELT_DAMPING * rate)).clamp(0.0, 5_000.0);
            }
            h.last_compression = compression.max(0.0);
            h.v -= h.strings * force / h.mass * dt;
            h.y += h.v * dt;
            h.t += 1;
            // The hammer has left the strings and is falling back, or it is out of time.
            if (compression < -2e-4 && h.v < 0.0) || h.t > h.limit || !h.y.is_finite() {
                self.hammer = None;
            }
        }
        let impulse = force * dt;
        let mut acc = [0.0f32; LANES];
        for b in &mut self.blocks {
            for ((((re, im), (cr, ci)), (kick, out)), a) in
                b.re.iter_mut().zip(b.im.iter_mut()).zip(b.cr.iter().zip(&b.ci)).zip(b.kick.iter().zip(&b.out)).zip(acc.iter_mut())
            {
                let r = *re * cr - *im * ci;
                let i = *re * ci + *im * cr + kick * impulse;
                *re = r;
                *im = i;
                *a += out * r;
            }
        }
        let mut y: f32 = acc.iter().sum();
        if self.phantom > 0.0 {
            let p = y * y * self.phantom;
            let (last, out) = self.phantom_state;
            let hp = p - last + 0.995 * out;
            self.phantom_state = (p, if hp.is_finite() { hp } else { 0.0 });
            y += self.phantom_state.1;
        }
        if force > 0.0 || self.thump.1.abs() > 1e-9 {
            let n = self.noise.next() * force;
            self.thump.0 += self.thump_lp * (n - self.thump.0);
            self.thump.1 += self.thump_lp * (self.thump.0 - self.thump.1);
            y += self.thump.1 * self.thump_gain;
        }
        // The knock of the hammer in the case.
        for (y1, y2, cr, ci) in &mut self.knock {
            let r = *y1 * *cr - *y2 * *ci;
            let i = *y1 * *ci + *y2 * *cr + force * self.knock_gain * dt * 1e3;
            (*y1, *y2) = (r, i);
            y += r;
        }
        if !y.is_finite() {
            y = 0.0;
            self.blocks.clear();
        }
        self.age = self.age.saturating_add(1);
        self.level = self.level * 0.9995 + y.abs() * 0.0005;
        if self.choked {
            self.choke_gain *= (-1.0 / (0.012 * self.sr)).exp();
        }
        // Drop modes that have died away, now and then: high partials go quickly.
        if self.age.is_multiple_of(2048) && self.hammer.is_none() {
            self.blocks.retain(|b| b.re.iter().zip(&b.im).zip(&b.out).any(|((r, i), o)| (r.abs() + i.abs()) * o.abs() > 1e-7));
        }
        let y = y * self.choke_gain * self.out_gain;
        (y * self.pan.0, y * self.pan.1)
    }

    /// Modes still sounding, for tests and budgets.
    #[cfg(test)]
    fn modes(&self) -> usize {
        self.blocks.len() * LANES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn render(freq: f32, velocity: f32, seconds: f32) -> Vec<f32> {
        let mut v = PianoVoice::new(0, freq, velocity, PianoTone::GRAND, SR);
        (0..(seconds * SR) as usize).map(|_| v.tick()).map(|(l, r)| 0.5 * (l + r)).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn hammer_contact_is_milliseconds_and_shorter_when_harder() {
        let contact = |freq: f32, velocity: f32| {
            let mut v = PianoVoice::new(0, freq, velocity, PianoTone::GRAND, SR);
            let mut n = 0;
            while v.hammer.is_some() && n < 10_000 {
                v.tick();
                n += 1;
            }
            n as f32 / SR * 1000.0
        };
        for f in [55.0, 261.6, 2093.0] {
            let (soft, hard) = (contact(f, 0.3), contact(f, 1.0));
            println!("{f} Hz: pp {soft:.2} ms, ff {hard:.2} ms");
            assert!(hard < soft, "{f}: ff {hard} ms, pp {soft} ms");
            assert!((0.2..12.0).contains(&soft) && (0.2..12.0).contains(&hard));
        }
    }

    #[test]
    fn notes_decay_and_stay_finite() {
        for f in [27.5, 65.4, 261.6, 1046.5, 4186.0] {
            let x = render(f, 0.8, 3.0);
            assert!(x.iter().all(|s| s.is_finite()));
            let early = rms(&x[..(0.3 * SR) as usize]);
            let late = rms(&x[(2.5 * SR) as usize..]);
            println!("{f} Hz: early {early:.4}, late {late:.4}");
            assert!(early > 1e-3 && late < early, "{f}: {early} {late}");
        }
    }

    /// Where the spectrum's energy sits, in Hz, over the first `seconds`.
    fn centroid(x: &[f32]) -> f32 {
        let n = 8192.min(x.len());
        let (mut num, mut den) = (0.0f32, 0.0f32);
        for k in 1..n / 2 {
            let w = std::f32::consts::TAU * k as f32 / n as f32;
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (i, s) in x.iter().take(n).enumerate() {
                re += s * (w * i as f32).cos();
                im += s * (w * i as f32).sin();
            }
            let p = re * re + im * im;
            num += p * k as f32 * SR / n as f32;
            den += p;
        }
        num / den.max(1e-20)
    }

    #[test]
    fn a_harder_blow_is_brighter_as_well_as_louder() {
        let (soft, hard) = (render(261.6, 0.3, 0.2), render(261.6, 0.95, 0.2));
        let (cs, ch) = (centroid(&soft), centroid(&hard));
        println!("centroid pp {cs:.0} Hz, ff {ch:.0} Hz; rms {:.4} {:.4}", rms(&soft), rms(&hard));
        assert!(rms(&hard) > 3.0 * rms(&soft));
        assert!(ch > cs * 1.3, "{cs} {ch}");
    }

    #[test]
    fn the_damper_stops_a_key_let_go_but_the_top_keys_ring_on() {
        let after_release = |freq: f32| {
            let mut v = PianoVoice::new(0, freq, 0.7, PianoTone::GRAND, SR);
            let held: Vec<f32> = (0..(0.5 * SR) as usize).map(|_| v.tick().0).collect();
            v.release();
            let tail: Vec<f32> = (0..(0.6 * SR) as usize).map(|_| v.tick().0).collect();
            20.0 * (rms(&tail[(0.4 * SR) as usize..]) / rms(&held[(0.3 * SR) as usize..])).log10()
        };
        let (damped, undamped) = (after_release(261.6), after_release(2093.0));
        println!("after release: middle C {damped:.1} dB, C7 {undamped:.1} dB");
        assert!(damped < -30.0, "{damped}");
        assert!(undamped > damped + 30.0, "{undamped}");
    }

    #[test]
    fn a_key_struck_again_sets_the_moving_strings_going_again() {
        let mut v = PianoVoice::new(0, 196.0, 0.5, PianoTone::GRAND, SR);
        let first: Vec<f32> = (0..(1.0 * SR) as usize).map(|_| v.tick().0).collect();
        v.release();
        for _ in 0..(0.3 * SR) as usize {
            v.tick();
        }
        v.strike(0.9);
        let again: Vec<f32> = (0..(0.3 * SR) as usize).map(|_| v.tick().0).collect();
        assert!(rms(&again) > rms(&first[(0.7 * SR) as usize..]) * 2.0);
        assert!(!v.released);
    }

    #[test]
    fn partials_that_have_died_away_stop_costing_anything() {
        let mut v = PianoVoice::new(0, 55.0, 0.9, PianoTone::GRAND, SR);
        let start = v.modes();
        for _ in 0..(4.0 * SR) as usize {
            v.tick();
        }
        println!("modes: {start} at the blow, {} after 4 s", v.modes());
        assert!(v.modes() < start);
    }

    #[test]
    fn hostile_pitches_and_rates_stay_finite() {
        for freq in [f32::NAN, -5.0, 0.0, 1.0, 30_000.0, f32::INFINITY] {
            for sr in [8_000.0, 384_000.0, f32::NAN] {
                let mut v = PianoVoice::new(0, freq, 2.0, PianoTone::HONKY_TONK, sr);
                for _ in 0..2000 {
                    let (l, r) = v.tick();
                    assert!(l.is_finite() && r.is_finite(), "{freq} {sr}");
                }
            }
        }
    }

    #[test]
    fn octaves_are_tuned_to_the_strings_partials_so_they_do_not_beat() {
        // A tuner's octave: the upper note's fundamental on the lower note's second partial (in
        // the bass, its second on the lower's fourth), within a fraction of a cent.
        for lower in [24.0f32, 36.0, 48.0, 60.0, 72.0, 84.0] {
            let (a, b) = (
                key_model(tuned(lower) / 2f32.powf(stretch_cents(lower) / 1200.0), PianoTone::GRAND),
                key_model(440.0 * 2f32.powf((lower + 12.0 - 69.0) / 12.0), PianoTone::GRAND),
            );
            let partial = |k: &Key, n: f32| n * k.f0 * (1.0 + k.b * n * n).sqrt();
            let (lo, hi) = if lower >= 57.0 { (partial(&a, 2.0), partial(&b, 1.0)) } else { (partial(&a, 4.0), partial(&b, 2.0)) };
            let cents = 1200.0 * (hi / lo).log2();
            println!("octave from {lower}: {cents:+.2} cents");
            assert!(cents.abs() < 0.5, "{lower}: {cents}");
        }
        assert!(stretch_cents(108.0) > 10.0 && stretch_cents(21.0) < -5.0, "stretched both ways");
    }

    #[test]
    fn partials_decay_as_a_real_strings_do() {
        // Middle C's partials ring for seconds, the higher ones less: about 2-3 s for the tenth and
        // about 1 s for the twentieth (prompt sound), not a fraction of a second (an electric piano).
        let k = key_model(261.6, PianoTone::GRAND);
        let t60 = |n: f32| {
            let f = n * k.f0 * (1.0 + k.b * n * n).sqrt();
            k.t60 / (1.0 + (f - k.f0) / k.loss_corner)
        };
        println!("C4 T60: p5 {:.1} s, p10 {:.1} s, p20 {:.1} s", t60(5.0), t60(10.0), t60(20.0));
        assert!((1.5..4.0).contains(&t60(10.0)) && (0.6..1.6).contains(&t60(20.0)));
        // The bass darkens faster: A0's partial near 3 kHz is gone within about a second.
        let a0 = key_model(27.5, PianoTone::GRAND);
        let n = 60.0;
        let f = n * a0.f0 * (1.0 + a0.b * n * n).sqrt();
        assert!(a0.t60 / (1.0 + (f - a0.f0) / a0.loss_corner) < 1.5);
    }
}
