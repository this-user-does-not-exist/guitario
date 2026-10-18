//! One playing region: resampling (4-point Hermite), loops and an AHDSR amplitude envelope.

use super::parse::{LoopMode, Region};
use super::sample::Sample;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    Delay,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
}

pub struct SampleVoice {
    pub track: usize,
    pub string: u8,
    pub group: u32,
    pub off_by: u32,
    pub off_fast: bool,
    pub released: bool,
    pub let_ring: bool,
    pub vibrato: bool,
    /// Song time of the note driving this voice, for its end and bend.
    pub note_start: f64,
    pub note_end: f64,
    /// The bend curve of the note, followed from its start to its end.
    pub bend: Option<guitario_score::Bend>,
    /// The whammy bar's curve moving the note.
    pub whammy: Option<crate::synth::WhammyTime>,
    pub bend_semis: f32,
    /// Vibrato depth in semitones (wide vibrato is deeper).
    pub vibrato_depth: f32,
    /// Output gain for fades and swells, 0..=1.
    pub gain: f32,
    pub fade: Option<guitario_score::Fade>,
    sample: Arc<Sample>,
    pos: f64,
    /// Source frames per output frame before bend.
    step: f64,
    end: f64,
    loop_start: f64,
    loop_end: f64,
    looping: bool,
    sustain_loop: bool,
    one_shot: bool,
    gain_l: f32,
    gain_r: f32,
    stage: Stage,
    level: f32,
    delay_left: u32,
    attack_inc: f32,
    hold_left: u32,
    decay_dec: f32,
    sustain: f32,
    release_secs: f32,
    release_dec: f32,
    vib_phase: f32,
    sr: f32,
    done: bool,
}

fn samples(secs: f32, sr: f32) -> f32 {
    (secs * sr).max(0.0)
}

impl SampleVoice {
    /// Starts `region` for `key` at `velocity` (0..=1), resampled to the output rate `sr`.
    pub fn new(track: usize, string: u8, region: &Region, sample: &Arc<Sample>, key: u8, velocity: f32, sr: f32) -> SampleVoice {
        let semis = (f32::from(key) - f32::from(region.keycenter)) * region.keytrack / 100.0 + region.tune_cents / 100.0;
        let step = f64::from(sample.rate) / f64::from(sr.max(1.0)) * 2f64.powf(f64::from(semis) / 12.0);
        let frames = sample.frames() as f64;
        let end = region.end.map_or(frames, |e| (e as f64 + 1.0).min(frames));
        let (loop_start, loop_end) = (region.loop_start.map_or(0.0, |s| s as f64), region.loop_end.map_or(end, |e| (e as f64 + 1.0).min(end)));
        let looping = matches!(region.loop_mode, LoopMode::LoopContinuous | LoopMode::LoopSustain) && loop_end - loop_start > 4.0;
        let v = velocity.clamp(0.0, 1.0);
        let vel_gain = 1.0 - region.veltrack + region.veltrack * v * v;
        let gain = 10f32.powf(region.volume_db / 20.0) * vel_gain.max(0.0);
        // Equal-power pan, unity at the centre.
        let angle = (region.pan + 1.0) * std::f32::consts::FRAC_PI_4;
        let (gain_l, gain_r) = (gain * angle.cos() * std::f32::consts::SQRT_2, gain * angle.sin() * std::f32::consts::SQRT_2);
        let attack = samples(region.attack, sr);
        let decay = samples(region.decay, sr);
        // Never snap to silence in less than ~2 ms: that clicks.
        let release_secs = region.release.max(0.002);
        SampleVoice {
            track,
            string,
            group: region.group,
            off_by: region.off_by,
            off_fast: region.off_fast,
            released: false,
            let_ring: false,
            vibrato: false,
            note_start: 0.0,
            note_end: f64::MAX,
            bend: None,
            whammy: None,
            bend_semis: 0.0,
            vibrato_depth: 0.12,
            gain: 1.0,
            fade: None,
            sample: Arc::clone(sample),
            pos: (region.offset as f64).min(frames),
            step,
            end,
            loop_start,
            loop_end,
            looping,
            sustain_loop: region.loop_mode == LoopMode::LoopSustain,
            one_shot: region.loop_mode == LoopMode::OneShot,
            gain_l,
            gain_r,
            stage: if region.delay > 0.0 { Stage::Delay } else { Stage::Attack },
            level: if attack < 1.0 { 1.0 } else { 0.0 },
            delay_left: samples(region.delay, sr) as u32,
            attack_inc: if attack >= 1.0 { 1.0 / attack } else { 1.0 },
            hold_left: samples(region.hold, sr) as u32,
            decay_dec: if decay >= 1.0 { (1.0 - region.sustain) / decay } else { 1.0 },
            sustain: region.sustain,
            release_secs,
            release_dec: 0.0,
            vib_phase: 0.0,
            sr,
            done: false,
        }
    }

    /// Plays the sample `ratio` times higher (the relative tonality); 1.0 leaves it as it is.
    pub fn repitch(&mut self, ratio: f32) {
        if ratio.is_finite() && ratio > 0.0 {
            self.step *= f64::from(ratio);
        }
    }

    /// Note off. One-shot regions play out unless forced.
    pub fn release(&mut self) {
        self.released = true;
        if !self.one_shot {
            self.begin_release(self.release_secs);
        }
    }

    /// Another note on the same string: the old one stops within milliseconds, whatever the
    /// region's own release (a let-ring note too).
    pub fn damp(&mut self) {
        self.released = true;
        self.begin_release(0.025);
    }

    /// Cuts the voice (choke group, `off_by`): releases even a one-shot.
    pub fn choke(&mut self, fast: bool) {
        self.released = true;
        self.begin_release(if fast { 0.006 } else { self.release_secs });
    }

    fn begin_release(&mut self, secs: f32) {
        if self.stage != Stage::Release {
            self.stage = Stage::Release;
            self.release_dec = self.level / samples(secs, self.sr).max(1.0);
        }
    }

    pub fn finished(&self) -> bool {
        self.done
    }

    #[inline]
    fn envelope(&mut self) {
        match self.stage {
            Stage::Delay => {
                if self.delay_left == 0 {
                    self.stage = Stage::Attack;
                } else {
                    self.delay_left -= 1;
                }
            }
            Stage::Attack => {
                self.level += self.attack_inc;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Hold;
                }
            }
            Stage::Hold => {
                if self.hold_left == 0 {
                    self.stage = Stage::Decay;
                } else {
                    self.hold_left -= 1;
                }
            }
            Stage::Decay => {
                self.level -= self.decay_dec;
                if self.level <= self.sustain {
                    self.level = self.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {}
            Stage::Release => {
                self.level -= self.release_dec;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.done = true;
                }
            }
        }
    }

    /// One stereo frame.
    #[inline]
    pub fn tick(&mut self) -> (f32, f32) {
        if self.done {
            return (0.0, 0.0);
        }
        self.envelope();
        if self.stage == Stage::Delay {
            return (0.0, 0.0);
        }
        let mut bend = self.bend_semis;
        if self.vibrato {
            self.vib_phase += 5.5 / self.sr;
            if self.vib_phase >= 1.0 {
                self.vib_phase -= 1.0;
            }
            bend += self.vibrato_depth * (self.vib_phase * std::f32::consts::TAU).sin();
        }
        let i = self.pos.floor();
        let t = (self.pos - i) as f32;
        let i = i as isize;
        let (a, b, c, d) = (self.sample.get(i - 1), self.sample.get(i), self.sample.get(i + 1), self.sample.get(i + 2));
        let h = |y0: f32, y1: f32, y2: f32, y3: f32| {
            let c1 = 0.5 * (y2 - y0);
            let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
            let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
            ((c3 * t + c2) * t + c1) * t + y1
        };
        let (l, r) = (h(a.0, b.0, c.0, d.0), h(a.1, b.1, c.1, d.1));
        self.pos += self.step * 2f64.powf(f64::from(bend) / 12.0);
        if self.looping && (!self.sustain_loop || !self.released) {
            if self.pos >= self.loop_end {
                self.pos -= self.loop_end - self.loop_start;
            }
        } else if self.pos >= self.end {
            self.done = true;
        }
        (l * self.level * self.gain_l * self.gain, r * self.level * self.gain_r * self.gain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sfz::parse::parse;

    fn region(text: &str) -> Region {
        parse(text, &mut |_| None).regions.remove(0)
    }

    fn sine(rate: u32, hz: f32, frames: usize) -> Arc<Sample> {
        let left = (0..frames).map(|n| (n as f32 / rate as f32 * hz * std::f32::consts::TAU).sin()).collect();
        Arc::new(Sample { rate, left, right: None })
    }

    fn zero_crossings(x: &[f32]) -> usize {
        x.windows(2).filter(|w| matches!(w, [a, b] if *a < 0.0 && *b >= 0.0)).count()
    }

    fn render(v: &mut SampleVoice, n: usize) -> Vec<f32> {
        (0..n).map(|_| v.tick().0).collect()
    }

    #[test]
    fn transposes_and_resamples() {
        // A 220 Hz sample recorded at 22.05 kHz, played one octave up at 44.1 kHz = 440 Hz.
        let s = sine(22_050, 220.0, 22_050);
        let r = region("<region> sample=a.wav pitch_keycenter=57 key=57");
        let mut v = SampleVoice::new(0, 0, &r, &s, 57, 1.0, 44_100.0);
        let x = render(&mut v, 44_100);
        assert!((zero_crossings(&x) as i32 - 220).abs() <= 2, "{}", zero_crossings(&x));
        let r = region("<region> sample=a.wav lokey=0 hikey=127 pitch_keycenter=57");
        let mut up = SampleVoice::new(0, 0, &r, &s, 69, 1.0, 22_050.0);
        let x = render(&mut up, 11_000);
        // Octave up at the native rate: 440 Hz over 0.5 s = 220 cycles.
        assert!((zero_crossings(&x) as i32 - 220).abs() <= 3, "{}", zero_crossings(&x));
    }

    #[test]
    fn envelope_release_and_finish() {
        let s = sine(44_100, 200.0, 44_100 * 2);
        let r = region("<region> sample=a.wav key=60 ampeg_attack=0.01 ampeg_release=0.05");
        let mut v = SampleVoice::new(0, 0, &r, &s, 60, 1.0, 44_100.0);
        let x = render(&mut v, 4410);
        assert!(x.first().copied().unwrap_or(1.0).abs() < 0.05);
        assert!(x.iter().any(|a| a.abs() > 0.9));
        v.release();
        render(&mut v, 3000);
        assert!(v.finished());
        assert!(render(&mut v, 10).iter().all(|a| *a == 0.0));
    }

    #[test]
    fn one_shot_ignores_note_off_but_not_choke() {
        let s = sine(44_100, 200.0, 44_100);
        let r = region("<region> sample=a.wav key=60 loop_mode=one_shot");
        let mut v = SampleVoice::new(0, 0, &r, &s, 60, 1.0, 44_100.0);
        render(&mut v, 100);
        v.release();
        render(&mut v, 4410);
        assert!(!v.finished());
        v.choke(true);
        render(&mut v, 1000);
        assert!(v.finished());
    }

    #[test]
    fn loops_hold_past_the_sample_and_non_loops_end() {
        let s = sine(44_100, 200.0, 4410);
        let r = region("<region> sample=a.wav key=60 loop_mode=loop_continuous loop_start=0 loop_end=4409");
        let mut v = SampleVoice::new(0, 0, &r, &s, 60, 1.0, 44_100.0);
        render(&mut v, 44_100);
        assert!(!v.finished());
        let r = region("<region> sample=a.wav key=60");
        let mut v = SampleVoice::new(0, 0, &r, &s, 60, 1.0, 44_100.0);
        render(&mut v, 5000);
        assert!(v.finished());
    }

    #[test]
    fn velocity_pan_and_hostile_regions_stay_finite() {
        let s = sine(44_100, 200.0, 1000);
        let r = region("<region> sample=a.wav key=60 pan=100");
        let mut v = SampleVoice::new(0, 0, &r, &s, 60, 1.0, 44_100.0);
        let (l, rr) = (0..200).map(|_| v.tick()).fold((0.0f32, 0.0f32), |a, f| (a.0.max(f.0.abs()), a.1.max(f.1.abs())));
        assert!(l < 0.01 && rr > 0.5);
        let loud = region("<region> sample=a.wav key=60 volume=1e30 offset=99999999 end=0 loop_mode=loop_continuous loop_start=5 loop_end=1");
        let mut v = SampleVoice::new(0, 0, &loud, &s, 200, f32::NAN, 0.0);
        assert!((0..2000).all(|_| {
            let (a, b) = v.tick();
            a.is_finite() && b.is_finite()
        }));
    }
}
