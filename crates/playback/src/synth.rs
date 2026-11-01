//! A plucked string (Karplus–Strong with a fractional, time-varying delay) and a metronome click.

/// A whammy curve in song time: it runs from `start` to `end` seconds. Before it the bar is at
/// rest; after it, it stays where the curve ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WhammyTime {
    pub curve: guitario_score::Whammy,
    pub start: f64,
    pub end: f64,
}

/// How far a note's pitch is moved at `t` (song seconds), in semitones: its bend, followed from
/// `start` to `end`, plus the whammy bar and a slide.
pub fn curve_semis(bend: Option<guitario_score::Bend>, whammy: Option<WhammyTime>, slide: Option<SlideTime>, t: f64, start: f64, end: f64) -> f32 {
    let b = bend.map_or(0.0, |b| b.semitones_at(((t - start) / (end - start).max(1e-3)) as f32));
    let w = whammy.filter(|w| t >= w.start).map_or(0.0, |w| w.curve.semitones_at(((t - w.start) / (w.end - w.start).max(1e-3)) as f32));
    b + w + slide.map_or(0.0, |s| s.semitones_at(t))
}

/// Deterministic noise so renders are reproducible.
pub struct Noise(u32);

impl Noise {
    pub fn new(seed: u32) -> Noise {
        Noise(seed.max(1))
    }

    pub fn next(&mut self) -> f32 {
        // xorshift32
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

pub struct Pluck {
    pub freq: f32,
    pub velocity: f32,
    /// 0 = dark (palm mute, bass), 1 = bright.
    pub brightness: f32,
    /// Seconds to fade by 60 dB when that is sooner than the string would on its own (a palm
    /// mute, a dead note); `None` rings naturally.
    pub decay: Option<f32>,
    /// How much the highs die away faster than the lows while it rings: 0 as plucked, 1 with a
    /// palm resting on the string.
    pub damping: f32,
    /// The partial heard, for a harmonic (1 for a plain note), and how much of the plain note
    /// still sounds under it (0 to 1).
    pub partial: u8,
    pub fundamental: f32,
}

impl Pluck {
    /// A plain note: natural decay, undamped, no harmonic.
    pub fn plain(freq: f32, velocity: f32, brightness: f32) -> Pluck {
        Pluck { freq, velocity, brightness, decay: None, damping: 0.0, partial: 1, fundamental: 1.0 }
    }
}

pub struct StringVoice {
    pub track: usize,
    pub string: u8,
    /// The track's instrument body it sounds through (a pizzicato), if any.
    pub slot: Option<usize>,
    /// Samples before the string is plucked: the players of a section never pluck together.
    pub wait: u32,
    buf: Vec<f32>,
    w: usize,
    prev: f32,
    pub base_freq: f32,
    /// Glide target and per-sample step, in log2 frequency.
    target: f32,
    glide_per_sample: f32,
    log_freq: f32,
    pub bend_semis: f32,
    pub vibrato: bool,
    /// Vibrato depth in octaves (wide vibrato is deeper).
    pub vibrato_depth: f32,
    /// Output gain for fades and swells, 0..=1.
    pub gain: f32,
    pub fade: Option<guitario_score::Fade>,
    vib_phase: f32,
    loss: f32,
    /// The loss once released: the finger lifts and the string stops in a fraction of a second.
    release_loss: f32,
    /// The loop's extra lowpass (1 leaves it open) and its state, and the delay it adds at the
    /// note's pitch, taken off the delay line so the pitch stays true.
    loop_lp: f32,
    loop_state: f32,
    loop_delay: f32,
    pub released: bool,
    pub let_ring: bool,
    /// Song time of the note driving this voice, for its end and bend.
    pub note_start: f64,
    pub note_end: f64,
    /// The bend curve of the note, followed from its start to its end.
    pub bend: Option<guitario_score::Bend>,
    /// The whammy bar's curve moving the note.
    pub whammy: Option<WhammyTime>,
    /// A slide moving the note, in song time.
    pub slide: Option<SlideTime>,
    level: f32,
    age: u32,
    sr: f32,
    /// The string was struck again: the old vibration dies within milliseconds.
    damped: bool,
    damp_gain: f32,
}

/// A slide in song time: the pitch moves from `from` to `to` semitones (off the note's pitch)
/// between `start` and `end` seconds, and the note fades out over it when `fade` is set (sliding
/// off the string).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlideTime {
    pub start: f64,
    pub end: f64,
    pub from: f32,
    pub to: f32,
    pub fade: bool,
}

impl SlideTime {
    /// The slide's pitch offset at `t`: `from` before it, `to` after it.
    pub fn semitones_at(&self, t: f64) -> f32 {
        let x = if self.end > self.start { ((t - self.start) / (self.end - self.start)).clamp(0.0, 1.0) as f32 } else { f32::from(u8::from(t >= self.start)) };
        self.from + (self.to - self.from) * x
    }

    /// The gain left at `t`: 1 until a fading slide starts, then down to silence at its end.
    pub fn gain_at(&self, t: f64) -> f32 {
        if !self.fade || t <= self.start {
            return 1.0;
        }
        let x = if self.end > self.start { ((t - self.start) / (self.end - self.start)).clamp(0.0, 1.0) as f32 } else { 1.0 };
        (1.0 - x) * (1.0 - x)
    }
}

const MIN_FREQ: f32 = 25.0;

/// The loss per period that lets a plucked string at `freq` fade by 60 dB in about the time a
/// real one does: some 7 s around A2, sooner up the neck, never more than 10 s.
fn natural_loss(freq: f32) -> f32 {
    let f = freq.max(MIN_FREQ);
    let t60 = (7.0 * (110.0 / f).sqrt()).clamp(1.5, 10.0);
    10f32.powf(-3.0 / (t60 * f))
}

/// The loss per period that fades a string at `freq` by 60 dB in `t60` seconds.
fn loss_for(freq: f32, t60: f32) -> f32 {
    10f32.powf(-3.0 / (t60.max(0.005) * freq.max(MIN_FREQ)))
}

/// The delay, in samples, a one-pole lowpass `y += a (x - y)` adds at `freq`: its phase over the
/// angular frequency.
fn one_pole_delay(a: f32, freq: f32, sr: f32) -> f32 {
    let w = std::f32::consts::TAU * freq.max(MIN_FREQ) / sr;
    let b = 1.0 - a;
    let phase = (b * w.sin()).atan2(1.0 - b * w.cos());
    if w > 0.0 { (phase / w).max(0.0) } else { 0.0 }
}

/// How long a string's old vibration lasts once it is struck again, in seconds.
const RESTRIKE: f32 = 0.015;
/// How long a note takes to fade by 60 dB after it ends, in seconds: the fretting finger lifts.
const RELEASE: f32 = 0.25;
/// Longest a voice may sound, whatever its decay.
const MAX_RING: f32 = 30.0;

impl StringVoice {
    pub fn new(track: usize, string: u8, sr: f32, p: &Pluck, noise: &mut Noise) -> StringVoice {
        let len = ((sr / MIN_FREQ) as usize + 4).max(8);
        let mut buf = vec![0.0f32; len];
        let period = (sr / p.freq.max(MIN_FREQ)).max(2.0) as usize;
        let n = period.min(len).max(1);
        // Excite with filtered noise: darker picks lowpass it more.
        let a = 0.15 + 0.8 * p.brightness.clamp(0.0, 1.0);
        let mut lp = 0.0f32;
        for x in buf.iter_mut().take(n) {
            lp += a * (noise.next() - lp);
            *x = lp * p.velocity;
        }
        // A harmonic: the string is touched at a node, which silences every partial without one
        // there. Averaging copies of the excitation shifted by 1/k of the period keeps only the
        // multiples of the k-th partial; some of the plain note stays for pinch and semi-harmonics.
        let k = usize::from(p.partial.max(1));
        if k > 1 {
            let src: Vec<f32> = buf.iter().take(n).copied().collect();
            let shift = n as f32 / k as f32;
            let keep = p.fundamental.clamp(0.0, 1.0);
            // The comb passes a k-th of the energy; a harmonic is softer than the note, but not that much.
            let lift = (k as f32).sqrt() * 0.8;
            for (i, x) in buf.iter_mut().take(n).enumerate() {
                let mut acc = 0.0;
                for j in 0..k {
                    let pos = i as f32 + j as f32 * shift;
                    let (i0, frac) = (pos.floor() as usize, pos - pos.floor());
                    let at = |m: usize| src.get(m % n).copied().unwrap_or(0.0);
                    acc += at(i0) * (1.0 - frac) + at(i0 + 1) * frac;
                }
                *x = keep * *x + (1.0 - keep) * acc / k as f32 * lift;
            }
        }
        // Remove DC so the string does not thump.
        let mean = buf.iter().take(n).sum::<f32>() / n as f32;
        for x in buf.iter_mut().take(n) {
            *x -= mean;
        }
        let lf = p.freq.max(MIN_FREQ).log2();
        let natural = natural_loss(p.freq);
        let loss = p.decay.map_or(natural, |t60| loss_for(p.freq, t60).min(natural));
        // A palm on the string: a lowpass in the loop takes the highs off sooner, down to some 900 Hz.
        let damping = p.damping.clamp(0.0, 1.0);
        let loop_lp = if damping > 0.0 { 1.0 - (-std::f32::consts::TAU * (6000.0 - 5100.0 * damping) / sr).exp() } else { 1.0 };
        let loop_delay = if loop_lp < 1.0 { one_pole_delay(loop_lp, p.freq, sr) } else { 0.0 };
        StringVoice {
            track,
            string,
            slot: None,
            wait: 0,
            buf,
            w: period.min(len.saturating_sub(1)),
            prev: 0.0,
            base_freq: p.freq,
            target: lf,
            glide_per_sample: 0.0,
            log_freq: lf,
            bend_semis: 0.0,
            vibrato: false,
            vibrato_depth: 0.012,
            gain: 1.0,
            fade: None,
            vib_phase: 0.0,
            loss,
            release_loss: loss,
            loop_lp,
            loop_state: 0.0,
            loop_delay,
            released: false,
            let_ring: false,
            note_start: 0.0,
            note_end: f64::MAX,
            bend: None,
            whammy: None,
            slide: None,
            level: 1.0,
            age: 0,
            sr,
            damped: false,
            damp_gain: 1.0,
        }
    }

    /// Puts energy back into the ringing string without a pick: a finger hammering on, pulling
    /// off or tapping. `amount` is like a pluck's velocity; the burst is dark (no pick click).
    pub fn excite(&mut self, amount: f32, noise: &mut Noise) {
        let len = self.buf.len();
        let period = ((self.sr / self.target.exp2()) as usize).clamp(2, len.saturating_sub(2).max(2));
        let mut lp = 0.0f32;
        let mut burst: Vec<f32> = (0..period)
            .map(|_| {
                lp += 0.3 * (noise.next() - lp);
                lp * amount.clamp(0.0, 1.0)
            })
            .collect();
        let mean = burst.iter().sum::<f32>() / period as f32;
        burst.iter_mut().for_each(|x| *x -= mean);
        // Into the period about to be read.
        for (j, b) in burst.into_iter().enumerate() {
            if let Some(slot) = self.buf.get_mut((self.w + len - period + j) % len) {
                *slot += b;
            }
        }
    }

    /// Another note on the same string: this one stops at once (a let-ring note too).
    pub fn damp(&mut self) {
        self.release();
        self.damped = true;
    }

    /// Legato change of pitch (hammer-on, pull-off, slide) without a new pluck.
    pub fn glide_to(&mut self, freq: f32, seconds: f32) {
        self.base_freq = freq;
        self.target = freq.max(MIN_FREQ).log2();
        let samples = (seconds * self.sr).max(1.0);
        self.glide_per_sample = (self.target - self.log_freq) / samples;
        if seconds <= 0.0 {
            self.log_freq = self.target;
        }
        self.bend_semis = 0.0;
        self.released = false;
        if self.loop_lp < 1.0 {
            self.loop_delay = one_pole_delay(self.loop_lp, freq, self.sr);
        }
    }

    pub fn release(&mut self) {
        if !self.released {
            self.released = true;
            self.release_loss = loss_for(self.log_freq.exp2(), RELEASE).min(self.loss);
        }
    }

    /// Done when released and quiet, or, ringing on, once it has decayed to silence on its own.
    pub fn finished(&self) -> bool {
        let settled = self.age > (self.sr * 0.2) as u32;
        (self.released && (self.level < 1e-4 || self.damp_gain < 1e-4)) || (settled && self.level < 1e-5) || self.age > (self.sr * MAX_RING) as u32
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
        if self.wait > 0 {
            self.wait -= 1;
            return 0.0;
        }
        // Pitch: glide, bend and vibrato.
        if self.glide_per_sample != 0.0 {
            self.log_freq += self.glide_per_sample;
            if (self.glide_per_sample > 0.0 && self.log_freq >= self.target) || (self.glide_per_sample < 0.0 && self.log_freq <= self.target) {
                self.log_freq = self.target;
                self.glide_per_sample = 0.0;
            }
        }
        let mut lf = self.log_freq + self.bend_semis / 12.0;
        if self.vibrato {
            self.vib_phase += 5.5 / self.sr;
            if self.vib_phase >= 1.0 {
                self.vib_phase -= 1.0;
            }
            lf += self.vibrato_depth * (self.vib_phase * std::f32::consts::TAU).sin();
        }
        let len = self.buf.len();
        // The two-point average delays half a sample, the damping lowpass some more; compensate.
        let delay = (self.sr / lf.exp2() - 0.5 - self.loop_delay).clamp(1.0, (len - 2) as f32);
        let read = self.w as f32 + len as f32 - delay;
        let i0 = read.floor() as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = read - read.floor();
        let y = self.buf.get(i0).copied().unwrap_or(0.0) * (1.0 - frac) + self.buf.get(i1).copied().unwrap_or(0.0) * frac;
        let loss = if self.released { self.release_loss } else { self.loss };
        let mut next = 0.5 * (y + self.prev) * loss;
        self.prev = y;
        if self.loop_lp < 1.0 {
            self.loop_state += self.loop_lp * (next - self.loop_state);
            next = self.loop_state;
        }
        if let Some(slot) = self.buf.get_mut(self.w) {
            *slot = next;
        }
        self.w = (self.w + 1) % len;
        self.level = self.level * 0.999 + y.abs() * 0.001;
        self.age = self.age.saturating_add(1);
        if self.damped {
            self.damp_gain *= (-1.0 / (RESTRIKE * self.sr)).exp();
        }
        y * self.gain * self.damp_gain
    }
}

/// A short sine blip for the metronome.
pub struct Click {
    phase: f32,
    freq: f32,
    left: u32,
    total: u32,
    sr: f32,
}

impl Click {
    pub fn new(accent: bool, sr: f32) -> Click {
        let total = (sr * 0.03) as u32;
        Click { phase: 0.0, freq: if accent { 1900.0 } else { 1300.0 }, left: total, total: total.max(1), sr }
    }

    pub fn done(&self) -> bool {
        self.left == 0
    }

    pub fn tick(&mut self) -> f32 {
        if self.left == 0 {
            return 0.0;
        }
        self.left -= 1;
        self.phase += self.freq / self.sr;
        let env = self.left as f32 / self.total as f32;
        0.35 * env * env * (self.phase * std::f32::consts::TAU).sin()
    }
}

/// What kind of sound a General MIDI drum number makes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum DrumSound {
    Kick,
    Snare,
    SideStick,
    HiHat { decay: f32 },
    Cymbal { decay: f32, noise: f32, base: f32 },
    Tom { from: f32, to: f32 },
    Click,
}

fn drum_sound(midi: u8) -> DrumSound {
    // Guitar Pro's articulations: the half-open hi-hat is shorter than the open one; the others
    // sound as the piece they vary (a choke is cut by the player, a rim shot is louder).
    if midi == 92 {
        return DrumSound::HiHat { decay: 0.14 };
    }
    match guitario_theory::drums::plays(midi) {
        35 | 36 => DrumSound::Kick,
        38 | 40 => DrumSound::Snare,
        37 => DrumSound::SideStick,
        42 => DrumSound::HiHat { decay: 0.045 },
        44 => DrumSound::HiHat { decay: 0.03 },
        46 => DrumSound::HiHat { decay: 0.4 },
        49 | 57 => DrumSound::Cymbal { decay: 1.4, noise: 0.55, base: 1.0 },
        52 => DrumSound::Cymbal { decay: 1.0, noise: 0.75, base: 0.8 },
        55 => DrumSound::Cymbal { decay: 0.5, noise: 0.5, base: 1.3 },
        51 | 59 => DrumSound::Cymbal { decay: 1.1, noise: 0.25, base: 1.15 },
        53 => DrumSound::Cymbal { decay: 1.3, noise: 0.1, base: 1.6 },
        50 => DrumSound::Tom { from: 210.0, to: 175.0 },
        48 => DrumSound::Tom { from: 180.0, to: 150.0 },
        47 => DrumSound::Tom { from: 160.0, to: 132.0 },
        45 => DrumSound::Tom { from: 130.0, to: 105.0 },
        43 => DrumSound::Tom { from: 108.0, to: 88.0 },
        41 => DrumSound::Tom { from: 92.0, to: 74.0 },
        _ => DrumSound::Click,
    }
}

/// One drum hit, synthesised: no samples, deterministic.
pub struct DrumVoice {
    pub track: usize,
    pub midi: u8,
    sound: DrumSound,
    vel: f32,
    t: u32,
    sr: f32,
    env: f32,
    env_mul: f32,
    tone_env: f32,
    tone_mul: f32,
    phase: f32,
    partials: [f32; 6],
    lp: f32,
    prev: f32,
    choke: bool,
    /// The sample at which the hit is choked.
    cut_at: u32,
}

/// Inharmonic ratios for a metallic cymbal tone (square waves at these multiples).
const METAL: [f32; 6] = [1.0, 1.342, 1.693, 2.137, 2.586, 3.073];

impl DrumVoice {
    pub fn new(track: usize, midi: u8, velocity: f32, sr: f32) -> DrumVoice {
        let sound = drum_sound(midi);
        let decay = match sound {
            DrumSound::Kick => 0.32,
            DrumSound::Snare => 0.16,
            DrumSound::SideStick => 0.03,
            DrumSound::HiHat { decay } | DrumSound::Cymbal { decay, .. } => decay,
            DrumSound::Tom { .. } => 0.38,
            DrumSound::Click => 0.04,
        };
        let tone_decay = match sound {
            DrumSound::Snare => 0.07,
            DrumSound::SideStick => 0.02,
            _ => decay,
        };
        let mul = |tau: f32| (-1.0 / (tau.max(1e-3) * sr)).exp();
        let rim_shot = if midi == 91 { 1.3 } else { 1.0 };
        DrumVoice {
            track,
            midi,
            sound,
            vel: (velocity * rim_shot).clamp(0.0, 1.0),
            t: 0,
            sr,
            env: 1.0,
            env_mul: mul(decay),
            tone_env: 1.0,
            tone_mul: mul(tone_decay),
            phase: 0.0,
            partials: [0.0; 6],
            lp: 0.0,
            prev: 0.0,
            choke: false,
            cut_at: u32::MAX,
        }
    }

    /// An open (or half-open) hi-hat is cut short when the hi-hat closes.
    pub fn is_open_hat(&self) -> bool {
        matches!(self.midi, 46 | 92)
    }

    pub fn choke(&mut self) {
        self.choke = true;
    }

    /// Chokes the hit `secs` after it starts (a cymbal grabbed).
    pub fn cut_after(&mut self, secs: f32) {
        self.cut_at = (secs.max(0.0) * self.sr) as u32;
    }

    pub fn finished(&self) -> bool {
        self.env < 1e-4 || self.t > (self.sr * 4.0) as u32
    }

    pub fn tick(&mut self, noise: &mut Noise) -> f32 {
        let secs = self.t as f32 / self.sr;
        self.t = self.t.saturating_add(1);
        if self.t >= self.cut_at {
            self.choke = true;
        }
        if self.choke {
            self.env *= 0.995;
        }
        let n = noise.next();
        let out = match self.sound {
            DrumSound::Kick => {
                let f = 48.0 + 120.0 * (-secs / 0.035).exp();
                self.phase = (self.phase + f / self.sr).fract();
                let body = (self.phase * std::f32::consts::TAU).sin();
                let click = if secs < 0.004 { n * (1.0 - secs / 0.004) * 0.5 } else { 0.0 };
                (body * 1.2 + click) * self.env
            }
            DrumSound::Snare | DrumSound::SideStick => {
                let f = if self.sound == DrumSound::Snare { 185.0 } else { 420.0 };
                self.phase = (self.phase + f / self.sr).fract();
                let tone = (self.phase * std::f32::consts::TAU).sin() * self.tone_env;
                self.tone_env *= self.tone_mul;
                // Brighten the noise: subtract a lowpassed copy.
                self.lp += 0.25 * (n - self.lp);
                let rattle = (n - self.lp) * self.env;
                0.7 * tone + 0.8 * rattle
            }
            DrumSound::HiHat { .. } | DrumSound::Cymbal { .. } => {
                let (noise_amt, base) = match self.sound {
                    DrumSound::Cymbal { noise, base, .. } => (noise, base),
                    _ => (0.8, 1.9),
                };
                let mut metal = 0.0;
                for (ph, r) in self.partials.iter_mut().zip(METAL) {
                    *ph = (*ph + 410.0 * base * r / self.sr).fract();
                    metal += if *ph < 0.5 { 1.0 } else { -1.0 };
                }
                metal /= METAL.len() as f32;
                let mix = noise_amt * n + (1.0 - noise_amt) * metal;
                // A one-pole highpass keeps only the sizzle.
                let hp = mix - self.prev * 0.6;
                self.prev = mix;
                hp * self.env * 0.6
            }
            DrumSound::Tom { from, to } => {
                let f = to + (from - to) * (-secs / 0.08).exp();
                self.phase = (self.phase + f / self.sr).fract();
                let click = if secs < 0.003 { n * 0.3 } else { 0.0 };
                ((self.phase * std::f32::consts::TAU).sin() + click) * self.env
            }
            DrumSound::Click => n * self.env * 0.5,
        };
        self.env *= self.env_mul;
        out * self.vel
    }
}
