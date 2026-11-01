//! One playing region: resampling (4-point Hermite), loops with an optional crossfade, an AHDSR
//! amplitude envelope, velocity curves and a filter with its own envelope.

use super::parse::{Filter, FilterKind, LoopMode, Region};
use super::sample::Sample;
use super::stream::{LEAD_SECS, Tail};
use crate::fx::filter::{Biquad, OnePole, Shape};
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

fn samples(secs: f32, sr: f32) -> f32 {
    (secs * sr).max(0.0)
}

/// An envelope generator: delay, attack, hold, decay to a sustain level, and a release when let go.
#[derive(Clone, Copy, Debug)]
struct Env {
    stage: Stage,
    level: f32,
    delay_left: u32,
    attack_inc: f32,
    hold_left: u32,
    decay_dec: f32,
    sustain: f32,
    release_secs: f32,
    release_dec: f32,
    sr: f32,
    done: bool,
}

impl Env {
    /// Times in seconds, `sustain` 0..=1.
    fn new(delay: f32, attack: f32, hold: f32, decay: f32, sustain: f32, release: f32, sr: f32) -> Env {
        let attack = samples(attack, sr);
        let decay = samples(decay, sr);
        Env {
            stage: if delay > 0.0 { Stage::Delay } else { Stage::Attack },
            level: if attack < 1.0 { 1.0 } else { 0.0 },
            delay_left: samples(delay, sr) as u32,
            attack_inc: if attack >= 1.0 { 1.0 / attack } else { 1.0 },
            hold_left: samples(hold, sr) as u32,
            decay_dec: if decay >= 1.0 { (1.0 - sustain) / decay } else { 1.0 },
            sustain,
            release_secs: release,
            release_dec: 0.0,
            sr,
            done: false,
        }
    }

    /// Lets go: down to silence over `secs`.
    fn release(&mut self, secs: f32) {
        if self.stage != Stage::Release {
            self.stage = Stage::Release;
            self.release_dec = self.level / samples(secs, self.sr).max(1.0);
        }
    }

    #[inline]
    fn tick(&mut self) {
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
}

/// How often a moving filter's coefficients are worked out again, in samples.
const FILTER_STEP: u32 = 32;

/// A region's filter running on one voice, both channels.
struct VoiceFilter {
    kind: FilterKind,
    biquads: [Biquad; 3],
    poles: [OnePole; 2],
    /// Hz after key and velocity tracking, before the envelope.
    base: f32,
    q: f32,
    env: Env,
    depth: f32,
    countdown: u32,
    sr: f32,
}

impl VoiceFilter {
    fn new(f: &Filter, key: u8, velocity: f32, sr: f32) -> VoiceFilter {
        let cents = f.keytrack * (f32::from(key) - f32::from(f.keycenter)) + f.veltrack * velocity;
        let [delay, attack, hold, decay, release] = f.eg;
        let mut vf = VoiceFilter {
            kind: f.kind,
            biquads: [Biquad::default(); 3],
            poles: [OnePole::default(); 2],
            base: (f.cutoff * 2f32.powf(cents / 1200.0)).clamp(10.0, sr * 0.45),
            // 0 dB of resonance is a Butterworth response.
            q: std::f32::consts::FRAC_1_SQRT_2 * 10f32.powf(f.resonance / 20.0),
            env: Env::new(delay, attack, hold, decay, f.eg_sustain, release, sr),
            depth: if f.eg_depth.is_finite() { f.eg_depth } else { 0.0 },
            countdown: 0,
            sr,
        };
        vf.retune();
        vf
    }

    /// New coefficients for the cutoff the envelope has moved to (the filters keep their state).
    fn retune(&mut self) {
        let hz = (self.base * 2f32.powf(self.depth * self.env.level / 1200.0)).clamp(10.0, self.sr * 0.45);
        let (shape, stages) = match self.kind {
            FilterKind::LowPass1 | FilterKind::HighPass1 => {
                for p in &mut self.poles {
                    p.set(hz, self.sr);
                }
                return;
            }
            FilterKind::LowPass { stages } => (Shape::LowPass, stages),
            FilterKind::HighPass { stages } => (Shape::HighPass, stages),
            FilterKind::BandPass => (Shape::BandPass, 1),
            FilterKind::BandReject => (Shape::Notch, 1),
        };
        for b in self.biquads.iter_mut().take(usize::from(stages)) {
            b.set(shape, hz, self.q, 0.0, self.sr);
        }
    }

    #[inline]
    fn tick(&mut self, l: f32, r: f32) -> (f32, f32) {
        if self.depth != 0.0 {
            self.env.tick();
            if self.countdown == 0 {
                self.retune();
                self.countdown = FILTER_STEP;
            }
            self.countdown -= 1;
        }
        match self.kind {
            FilterKind::LowPass1 | FilterKind::HighPass1 => {
                let [pl, pr] = &mut self.poles;
                let (lo_l, lo_r) = (pl.tick(l), pr.tick(r));
                if self.kind == FilterKind::LowPass1 { (lo_l, lo_r) } else { (l - lo_l, r - lo_r) }
            }
            FilterKind::LowPass { stages } | FilterKind::HighPass { stages } => {
                let (mut l, mut r) = (l, r);
                for b in self.biquads.iter_mut().take(usize::from(stages)) {
                    (l, r) = (b.tick(0, l), b.tick(1, r));
                }
                (l, r)
            }
            FilterKind::BandPass | FilterKind::BandReject => {
                let [b, ..] = &mut self.biquads;
                (b.tick(0, l), b.tick(1, r))
            }
        }
    }
}

/// The gain of a velocity (0..=127) on a curve of (velocity, gain) points, straight between them.
fn on_curve(curve: &[(u8, f32)], velocity: f32) -> f32 {
    let i = curve.partition_point(|p| f32::from(p.0) <= velocity);
    match (i.checked_sub(1).and_then(|j| curve.get(j)), curve.get(i)) {
        (Some(&(v0, g0)), Some(&(v1, g1))) if v1 > v0 => g0 + (g1 - g0) * (velocity - f32::from(v0)) / f32::from(v1 - v0),
        (Some(&(_, g)), _) | (None, Some(&(_, g))) => g,
        (None, None) => 1.0,
    }
}

pub struct SampleVoice {
    pub track: usize,
    pub string: u8,
    pub group: u32,
    pub off_by: u32,
    pub off_fast: bool,
    pub released: bool,
    pub let_ring: bool,
    /// Song time at which the voice is cut, whatever its region says (a choked cymbal).
    pub cut_at: f64,
    pub vibrato: bool,
    /// Song time of the note driving this voice, for its end and bend.
    pub note_start: f64,
    pub note_end: f64,
    /// The bend curve of the note, followed from its start to its end.
    pub bend: Option<guitario_score::Bend>,
    /// The whammy bar's curve moving the note.
    pub whammy: Option<crate::synth::WhammyTime>,
    /// A slide moving the note, in song time.
    pub slide: Option<crate::synth::SlideTime>,
    pub bend_semis: f32,
    /// How far legato notes have moved the voice from the key it started on, in semitones.
    legato_semis: f32,
    /// The pitch heard now, before bends, in MIDI notes: where the next legato note moves from.
    pub pitch: f32,
    /// Vibrato depth in semitones (wide vibrato is deeper).
    pub vibrato_depth: f32,
    /// Output gain for fades and swells, 0..=1.
    pub gain: f32,
    pub fade: Option<guitario_score::Fade>,
    sample: Arc<Sample>,
    /// A streamed sample's tail, once in memory; the voice holds it until it ends.
    tail: Option<Arc<Tail>>,
    /// The source position at which to look for the tail (again).
    tail_at: f64,
    /// Offline: load a missing tail right away instead of asking the streamer.
    wait_for_tail: bool,
    underrun: bool,
    pos: f64,
    /// Source frames per output frame before bend.
    step: f64,
    end: f64,
    loop_start: f64,
    loop_end: f64,
    /// Source frames before the loop's end that fade into the ones before its start.
    crossfade: f64,
    looping: bool,
    sustain_loop: bool,
    one_shot: bool,
    gain_l: f32,
    gain_r: f32,
    amp: Env,
    filter: Option<VoiceFilter>,
    vib_phase: f32,
    sr: f32,
}

impl SampleVoice {
    /// Starts `region` for `key` at `velocity` (0..=1), resampled to the output rate `sr`.
    pub fn new(track: usize, string: u8, region: &Region, sample: &Arc<Sample>, key: u8, velocity: f32, sr: f32) -> SampleVoice {
        let semis = (f32::from(key) - f32::from(region.keycenter)) * region.keytrack / 100.0 + region.tune_cents / 100.0;
        let step = f64::from(sample.rate) / f64::from(sr.max(1.0)) * 2f64.powf(f64::from(semis) / 12.0);
        let frames = sample.frames() as f64;
        let end = region.end.map_or(frames, |e| (e as f64 + 1.0).min(frames));
        // The file's own loop counts where the region names none, and makes it loop by default.
        let (own_start, own_end) = sample.loop_points.map_or((None, None), |(a, b)| (Some(a), Some(b)));
        let loop_mode = region.loop_mode.unwrap_or(if sample.loop_points.is_some() { LoopMode::LoopContinuous } else { LoopMode::NoLoop });
        let loop_start = region.loop_start.or(own_start).map_or(0.0, |s| s as f64);
        let loop_end = region.loop_end.or(own_end).map_or(end, |e| (e as f64 + 1.0).min(end));
        let looping = matches!(loop_mode, LoopMode::LoopContinuous | LoopMode::LoopSustain) && loop_end - loop_start > 4.0;
        let crossfade = (f64::from(region.loop_crossfade) * f64::from(sample.rate)).min(loop_start).min((loop_end - loop_start) / 2.0).max(0.0);
        let v = if velocity.is_finite() { velocity.clamp(0.0, 1.0) } else { 0.0 };
        let vel_gain = if region.velcurve.is_empty() { 1.0 - region.veltrack + region.veltrack * v * v } else { on_curve(&region.velcurve, v * 127.0) };
        let gain = 10f32.powf(region.volume_db / 20.0) * vel_gain.max(0.0);
        // Equal-power pan, unity at the centre.
        let angle = (region.pan + 1.0) * std::f32::consts::FRAC_PI_4;
        let (gain_l, gain_r) = (gain * angle.cos() * std::f32::consts::SQRT_2, gain * angle.sin() * std::f32::consts::SQRT_2);
        // Never snap to silence in less than ~2 ms: that clicks.
        let release = region.release.max(0.002);
        SampleVoice {
            track,
            string,
            group: region.group,
            off_by: region.off_by,
            off_fast: region.off_fast,
            released: false,
            let_ring: false,
            cut_at: f64::MAX,
            vibrato: false,
            note_start: 0.0,
            note_end: f64::MAX,
            bend: None,
            whammy: None,
            slide: None,
            bend_semis: 0.0,
            legato_semis: 0.0,
            pitch: f32::from(key),
            vibrato_depth: 0.12,
            gain: 1.0,
            fade: None,
            sample: Arc::clone(sample),
            tail: None,
            tail_at: if sample.stream.is_some() { sample.head_frames() as f64 - LEAD_SECS * f64::from(sample.rate) } else { f64::MAX },
            wait_for_tail: true,
            underrun: false,
            pos: (region.offset as f64).min(frames),
            step,
            end,
            loop_start,
            loop_end,
            crossfade,
            looping,
            sustain_loop: loop_mode == LoopMode::LoopSustain,
            one_shot: loop_mode == LoopMode::OneShot,
            gain_l,
            gain_r,
            amp: Env::new(region.delay, region.attack, region.hold, region.decay, region.sustain, release, sr),
            filter: region.filter.as_ref().map(|f| VoiceFilter::new(f, key, v, sr)),
            vib_phase: 0.0,
            sr,
        }
    }

    /// On the audio thread: never wait for a streamed tail; ask the streamer ahead of need, and
    /// play silence if it is late. Off (the default), a missing tail is loaded when needed, so
    /// offline renders are exact.
    pub fn live(&mut self, live: bool) {
        self.wait_for_tail = !live;
    }

    /// Takes the tail if it is in memory (or loads it, offline), else asks for it and looks again
    /// a few milliseconds of sample later.
    #[cold]
    fn fetch_tail(&mut self) {
        let Some(stream) = &self.sample.stream else {
            self.tail_at = f64::MAX;
            return;
        };
        self.tail = if self.wait_for_tail { stream.tail() } else { stream.try_tail() };
        if self.tail.is_some() || stream.failed() {
            self.tail_at = f64::MAX;
            return;
        }
        stream.request();
        if !self.underrun && self.pos >= self.sample.head_frames() as f64 - 2.0 {
            self.underrun = true;
            super::stream::note_underrun();
        }
        self.tail_at = self.pos + 0.005 * f64::from(self.sample.rate);
    }

    /// Plays the sample `ratio` times higher (the relative tonality); 1.0 leaves it as it is.
    pub fn repitch(&mut self, ratio: f32) {
        if ratio.is_finite() && ratio > 0.0 {
            self.step *= f64::from(ratio);
        }
    }

    /// Plays `db` decibels quieter (a release sample after a long note).
    pub fn attenuate(&mut self, db: f32) {
        if db.is_finite() && db > 0.0 {
            let g = 10f32.powf(-db.min(120.0) / 20.0);
            self.gain_l *= g;
            self.gain_r *= g;
        }
    }

    /// Carries on at `pitch` (MIDI notes) without a new attack: a hammer-on, pull-off or legato
    /// slide moves the sounding sample instead of starting another.
    pub fn legato_to(&mut self, pitch: f32) {
        if pitch.is_finite() {
            self.legato_semis += pitch - self.pitch;
            self.pitch = pitch;
        }
        self.bend_semis = 0.0;
    }

    /// Note off. One-shot regions play out unless forced.
    pub fn release(&mut self) {
        self.released = true;
        if !self.one_shot {
            self.amp.release(self.amp.release_secs);
            if let Some(f) = &mut self.filter {
                f.env.release(f.env.release_secs);
            }
        }
    }

    /// Another note on the same string: the old one stops within milliseconds, whatever the
    /// region's own release (a let-ring note too).
    pub fn damp(&mut self) {
        self.released = true;
        self.amp.release(0.025);
    }

    /// Cuts the voice (choke group, `off_by`): releases even a one-shot.
    pub fn choke(&mut self, fast: bool) {
        self.released = true;
        self.amp.release(if fast { 0.006 } else { self.amp.release_secs });
    }

    pub fn finished(&self) -> bool {
        self.amp.done
    }

    /// Frame `i`, from memory or the tail (silence while a streamed tail is not there yet).
    #[inline]
    fn frame(&self, i: isize) -> (f32, f32) {
        let head = self.sample.head_frames() as isize;
        if i < head {
            return self.sample.get(i);
        }
        match (&self.tail, usize::try_from(i - head)) {
            (Some(t), Ok(at)) => t.get(at),
            _ => (0.0, 0.0),
        }
    }

    /// The sample at source position `pos`, both channels, by 4-point Hermite interpolation.
    #[inline]
    fn read(&self, pos: f64) -> (f32, f32) {
        let i = pos.floor();
        let t = (pos - i) as f32;
        let i = i as isize;
        let (a, b, c, d) = (self.frame(i - 1), self.frame(i), self.frame(i + 1), self.frame(i + 2));
        let h = |y0: f32, y1: f32, y2: f32, y3: f32| {
            let c1 = 0.5 * (y2 - y0);
            let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
            let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
            ((c3 * t + c2) * t + c1) * t + y1
        };
        (h(a.0, b.0, c.0, d.0), h(a.1, b.1, c.1, d.1))
    }

    /// One stereo frame.
    #[inline]
    pub fn tick(&mut self) -> (f32, f32) {
        if self.amp.done {
            return (0.0, 0.0);
        }
        self.amp.tick();
        if self.amp.stage == Stage::Delay {
            return (0.0, 0.0);
        }
        let mut bend = self.bend_semis + self.legato_semis;
        if self.vibrato {
            self.vib_phase += 5.5 / self.sr;
            if self.vib_phase >= 1.0 {
                self.vib_phase -= 1.0;
            }
            bend += self.vibrato_depth * (self.vib_phase * std::f32::consts::TAU).sin();
        }
        let looping = self.looping && (!self.sustain_loop || !self.released);
        if self.pos >= self.tail_at {
            self.fetch_tail();
        }
        let (mut l, mut r) = self.read(self.pos);
        // Near the loop's end, fade into what comes before its start, so the seam is smooth.
        let fade_from = self.loop_end - self.crossfade;
        if looping && self.crossfade > 0.0 && self.pos >= fade_from {
            let a = ((self.pos - fade_from) / self.crossfade).clamp(0.0, 1.0) as f32;
            let (el, er) = self.read(self.pos - (self.loop_end - self.loop_start));
            (l, r) = (l * (1.0 - a) + el * a, r * (1.0 - a) + er * a);
        }
        self.pos += self.step * 2f64.powf(f64::from(bend) / 12.0);
        if looping {
            if self.pos >= self.loop_end {
                self.pos -= self.loop_end - self.loop_start;
            }
        } else if self.pos >= self.end {
            self.amp.done = true;
        }
        if let Some(f) = &mut self.filter {
            (l, r) = f.tick(l, r);
        }
        let level = self.amp.level * self.gain;
        (l * level * self.gain_l, r * level * self.gain_r)
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
        let left: Vec<f32> = (0..frames).map(|n| (n as f32 / rate as f32 * hz * std::f32::consts::TAU).sin()).collect();
        Arc::new(Sample::from_f32(rate, &left, None))
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

    /// The loudest output of `region` over `sample` from `from` to `to` output frames.
    fn peak(region: &str, sample: &Arc<Sample>, velocity: f32, from: usize, to: usize) -> f32 {
        let mut v = SampleVoice::new(0, 0, &self::region(region), sample, 60, velocity, 44_100.0);
        render(&mut v, to).iter().skip(from).fold(0.0f32, |m, x| m.max(x.abs()))
    }

    #[test]
    fn filters_shape_the_sound_and_their_envelope_moves_them() {
        let high = sine(44_100, 4000.0, 44_100);
        let low = sine(44_100, 200.0, 44_100);
        let open = peak("<region> sample=a.wav key=60", &high, 1.0, 4410, 8820);
        let lpf = peak("<region> sample=a.wav key=60 cutoff=500", &high, 1.0, 4410, 8820);
        assert!(open > 0.9 && lpf < 0.05 * open, "low-pass at 500 Hz on 4 kHz: {lpf}");
        let one_pole = peak("<region> sample=a.wav key=60 cutoff=500 fil_type=lpf_1p", &high, 1.0, 4410, 8820);
        assert!(one_pole < 0.2 && one_pole > lpf, "a one-pole is gentler: {one_pole}");
        let hpf = peak("<region> sample=a.wav key=60 cutoff=2000 fil_type=hpf_2p", &low, 1.0, 4410, 8820);
        assert!(hpf < 0.05, "high-pass at 2 kHz on 200 Hz: {hpf}");
        let steeper = peak("<region> sample=a.wav key=60 cutoff=1000 fil_type=hpf_6p", &low, 1.0, 4410, 8820);
        assert!(steeper < 1e-3, "{steeper}");
        let band = peak("<region> sample=a.wav key=60 cutoff=4000 fil_type=bpf_2p", &high, 1.0, 4410, 8820);
        assert!(band > 0.9, "band-pass at its centre: {band}");
        // Velocity opens it: four octaves up at full velocity.
        let soft = peak("<region> sample=a.wav key=60 cutoff=250 fil_veltrack=4800", &high, 0.0, 4410, 8820);
        let hard = peak("<region> sample=a.wav key=60 cutoff=250 fil_veltrack=4800", &high, 1.0, 4410, 8820);
        assert!(hard > 0.5 && soft < 0.01, "soft {soft}, hard {hard}");
        // The envelope starts four octaves up and decays to the base cutoff in half a second.
        let eg = "<region> sample=a.wav key=60 cutoff=250 fileg_depth=4800 fileg_decay=0.5 fileg_sustain=0";
        let (early, late) = (peak(eg, &high, 1.0, 100, 2000), peak(eg, &high, 1.0, 30_000, 34_000));
        assert!(early > 0.5 && late < 0.01, "early {early}, late {late}");
    }

    #[test]
    fn velocity_curves_set_the_gain() {
        let s = sine(44_100, 200.0, 44_100);
        let at = |region: &str, v: f32| peak(region, &s, v, 1000, 2000);
        assert!((at("<region> sample=a.wav key=60 amp_velcurve_127=0.5", 1.0) - 0.5).abs() < 0.02);
        // Between points the curve is straight: half-way from (64, 0.8) to (127, 1).
        let curved = "<region> sample=a.wav key=60 amp_velcurve_64=0.8";
        assert!((at(curved, 64.0 / 127.0) - 0.8).abs() < 0.02);
        assert!((at(curved, 95.5 / 127.0) - 0.9).abs() < 0.02);
        // Without a curve, amp_veltrack: full at velocity 1, a quarter at half.
        assert!((at("<region> sample=a.wav key=60", 0.5) - 0.25).abs() < 0.02);
        assert_eq!(on_curve(&[], 50.0), 1.0);
    }

    #[test]
    fn a_sample_loops_where_its_file_says_and_the_seam_can_be_crossfaded() {
        // A ramp whose loop jumps from 0.9 back to 0.1: a click without a crossfade.
        let ramp = |looped: bool| {
            let left: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
            let mut s = Sample::from_f32(44_100, &left, None);
            s.loop_points = looped.then_some((100, 899));
            Arc::new(s)
        };
        let mut v = SampleVoice::new(0, 0, &region("<region> sample=a.wav key=60"), &ramp(true), 60, 1.0, 44_100.0);
        render(&mut v, 5000);
        assert!(!v.finished(), "loops on the file's own loop");
        let mut v = SampleVoice::new(0, 0, &region("<region> sample=a.wav key=60 loop_mode=no_loop"), &ramp(true), 60, 1.0, 44_100.0);
        render(&mut v, 5000);
        assert!(v.finished(), "unless told not to");
        let mut v = SampleVoice::new(0, 0, &region("<region> sample=a.wav key=60"), &ramp(false), 60, 1.0, 44_100.0);
        render(&mut v, 5000);
        assert!(v.finished(), "a file without a loop plays once");
        let jump = |region_text: &str| {
            let mut v = SampleVoice::new(0, 0, &region(region_text), &ramp(true), 60, 1.0, 44_100.0);
            let x = render(&mut v, 3000);
            x.windows(2).skip(1000).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max)
        };
        let (hard, smooth) = (jump("<region> sample=a.wav key=60"), jump("<region> sample=a.wav key=60 loop_crossfade=0.002"));
        assert!(hard > 0.5 && smooth < 0.02, "seam {hard} without, {smooth} with a crossfade");
    }

    /// A 3 s stereo chirp at 8 kHz, whole, and the same streamed from a loader that takes `delay`.
    fn chirp(delay_ms: u64) -> (Arc<Sample>, Arc<Sample>) {
        let make = || {
            let l: Vec<f32> = (0..24_000).map(|n| (n as f32 * (0.01 + n as f32 * 1e-6)).sin() * 0.5).collect();
            let r: Vec<f32> = l.iter().map(|x| -x).collect();
            Sample::from_f32(8000, &l, Some(&r))
        };
        let streamed = make().streamed(Box::new(move || {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            Ok(make())
        }));
        (Arc::new(make()), Arc::new(streamed))
    }

    fn render_both(v: &mut SampleVoice, n: usize) -> Vec<(f32, f32)> {
        (0..n).map(|_| v.tick()).collect()
    }

    #[test]
    fn a_streamed_sample_sounds_exactly_like_the_whole_one() {
        let (whole, streamed) = chirp(0);
        assert_eq!(streamed.head_frames(), 8000, "one second in memory");
        assert_eq!((streamed.frames(), whole.frames()), (24_000, 24_000));
        assert!(streamed.bytes() * 3 == whole.bytes());
        let r = region("<region> sample=a.wav lokey=0 hikey=127 pitch_keycenter=60");
        // Offline, a fifth up and resampled to 11.025 kHz: the tail is loaded when reached.
        let reference = render_both(&mut SampleVoice::new(0, 0, &r, &whole, 67, 1.0, 11_025.0), 20_000);
        let offline = render_both(&mut SampleVoice::new(0, 0, &r, &streamed, 67, 1.0, 11_025.0), 20_000);
        assert!(reference == offline, "bit for bit");
        // Live, the voice asks ahead of need and takes the tail once the streamer has it.
        let (_, streamed) = chirp(0);
        let mut live = SampleVoice::new(0, 0, &r, &streamed, 67, 1.0, 11_025.0);
        live.live(true);
        let mut out = render_both(&mut live, 4000);
        let stream = streamed.stream.clone().unwrap();
        assert!(stream.queued.load(std::sync::atomic::Ordering::Relaxed) || stream.try_tail().is_some(), "asked for before the head ran out");
        let t0 = std::time::Instant::now();
        while stream.try_tail().is_none() && t0.elapsed().as_secs() < 10 {
            std::thread::yield_now();
        }
        out.extend(render_both(&mut live, 16_000));
        assert!(reference == out, "the same once the tail arrived in time");
    }

    #[test]
    fn a_late_tail_plays_silence_and_is_counted() {
        let (_, streamed) = chirp(400);
        let r = region("<region> sample=a.wav key=60");
        let mut v = SampleVoice::new(0, 0, &r, &streamed, 60, 1.0, 8000.0);
        v.live(true);
        let before = super::super::stream::underruns();
        let out = render_both(&mut v, 12_000);
        assert!(out.iter().take(7990).any(|f| f.0.abs() > 0.1), "the head plays");
        assert!(out.iter().skip(8010).all(|f| *f == (0.0, 0.0)), "silence after it, not a stall");
        assert!(super::super::stream::underruns() > before);
        assert!(!v.finished(), "the voice carries on and picks the tail up when it comes");
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
