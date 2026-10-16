//! A plucked string (Karplus–Strong with a fractional, time-varying delay) and a metronome click.

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
    /// Per-sample loss while sounding, close to 1.
    pub sustain: f32,
}

pub struct StringVoice {
    pub track: usize,
    pub string: u8,
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
    pub released: bool,
    pub let_ring: bool,
    /// Song time of the note driving this voice, for its end and bend.
    pub note_start: f64,
    pub note_end: f64,
    /// Bend height in semitones, reached over the first half of the note.
    pub bend_to: f32,
    level: f32,
    age: u32,
    sr: f32,
}

const MIN_FREQ: f32 = 25.0;

impl StringVoice {
    pub fn new(track: usize, string: u8, sr: f32, p: &Pluck, noise: &mut Noise) -> StringVoice {
        let len = ((sr / MIN_FREQ) as usize + 4).max(8);
        let mut buf = vec![0.0f32; len];
        let period = (sr / p.freq.max(MIN_FREQ)).max(2.0) as usize;
        // Excite with filtered noise: darker picks lowpass it more.
        let a = 0.15 + 0.8 * p.brightness.clamp(0.0, 1.0);
        let mut lp = 0.0f32;
        for x in buf.iter_mut().take(period.min(len)) {
            lp += a * (noise.next() - lp);
            *x = lp * p.velocity;
        }
        // Remove DC so the string does not thump.
        let n = period.min(len).max(1) as f32;
        let mean = buf.iter().take(period.min(len)).sum::<f32>() / n;
        for x in buf.iter_mut().take(period.min(len)) {
            *x -= mean;
        }
        let lf = p.freq.max(MIN_FREQ).log2();
        StringVoice {
            track,
            string,
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
            loss: p.sustain,
            released: false,
            let_ring: false,
            note_start: 0.0,
            note_end: f64::MAX,
            bend_to: 0.0,
            level: 1.0,
            age: 0,
            sr,
        }
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
    }

    pub fn release(&mut self) {
        self.released = true;
    }

    pub fn finished(&self) -> bool {
        (self.released && self.level < 1e-4) || self.age > (self.sr * 12.0) as u32
    }

    #[inline]
    pub fn tick(&mut self) -> f32 {
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
        // The two-point average delays half a sample; compensate.
        let delay = (self.sr / lf.exp2() - 0.5).clamp(1.0, (len - 2) as f32);
        let read = self.w as f32 + len as f32 - delay;
        let i0 = read.floor() as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = read - read.floor();
        let y = self.buf.get(i0).copied().unwrap_or(0.0) * (1.0 - frac) + self.buf.get(i1).copied().unwrap_or(0.0) * frac;
        let loss = if self.released { 0.985 } else { self.loss };
        let next = 0.5 * (y + self.prev) * loss;
        self.prev = y;
        if let Some(slot) = self.buf.get_mut(self.w) {
            *slot = next;
        }
        self.w = (self.w + 1) % len;
        self.level = self.level * 0.999 + y.abs() * 0.001;
        self.age = self.age.saturating_add(1);
        y * self.gain
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
    match midi {
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
        DrumVoice {
            track,
            midi,
            sound,
            vel: velocity.clamp(0.0, 1.0),
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
        }
    }

    /// An open hi-hat is cut short when the hi-hat closes.
    pub fn is_open_hat(&self) -> bool {
        self.midi == 46
    }

    pub fn choke(&mut self) {
        self.choke = true;
    }

    pub fn finished(&self) -> bool {
        self.env < 1e-4 || self.t > (self.sr * 4.0) as u32
    }

    pub fn tick(&mut self, noise: &mut Noise) -> f32 {
        let secs = self.t as f32 / self.sr;
        self.t = self.t.saturating_add(1);
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
