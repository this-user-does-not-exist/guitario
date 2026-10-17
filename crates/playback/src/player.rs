use crate::sfz::{Instrument, SampleVoice};
use crate::synth::{Click, DrumVoice, Noise, Pluck, StringVoice};
use crate::tempo::{BeatTime, TempoMap, locate};
use guitario_score::{Fade, Score, Slide, timeline};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub metronome: bool,
    pub count_in: bool,
    pub looping: bool,
    /// Playback speed, 1.0 = written tempo (pitch is unaffected).
    pub speed: f32,
}

impl Default for Options {
    fn default() -> Self {
        Options { metronome: false, count_in: false, looping: false, speed: 1.0 }
    }
}

#[derive(Clone, Debug)]
struct NoteEvent {
    start: f64,
    end: f64,
    track: usize,
    string: u8,
    freq: f32,
    velocity: f32,
    dead: bool,
    palm_mute: bool,
    let_ring: bool,
    vibrato: bool,
    /// Bend height in semitones, reached over the first half of the note.
    bend: f32,
    /// The MIDI note: the drum number on a drum track.
    midi: u8,
    /// This note continues the previous one on its string without a pluck.
    legato_from_prev: bool,
    slide_glide: bool,
    wide_vibrato: bool,
    fade: Option<Fade>,
}

/// A stretch of an automation in playback time: from and to seconds, from and to values.
type Ramp = (f64, f64, f64, f64);

/// An automation's value at `t`: inside a stretch, interpolated; after the last, its end value;
/// before the first, none (the mixer's value applies).
fn ramp_at(ramps: &[Ramp], t: f64) -> Option<f64> {
    let i = ramps.partition_point(|r| r.0 <= t);
    let &(s0, s1, a, b) = ramps.get(i.checked_sub(1)?)?;
    if t >= s1 {
        return Some(b);
    }
    let x = if s1 > s0 { (t - s0) / (s1 - s0) } else { 1.0 };
    Some(a + (b - a) * x.clamp(0.0, 1.0))
}

/// One track's mixer settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mix {
    /// 0..=127.
    pub volume: u8,
    /// -63 (left) ..= 63 (right).
    pub pan: i8,
    pub muted: bool,
    pub solo: bool,
}

impl Mix {
    pub fn of(t: &guitario_score::Track) -> Mix {
        Mix { volume: t.volume, pan: t.pan, muted: t.muted, solo: t.solo }
    }

    /// Left and right gains: equal-power pan and volume; zero when muted or another track is soloed.
    fn gains(self, any_solo: bool) -> (f32, f32) {
        if self.muted || (any_solo && !self.solo) {
            return (0.0, 0.0);
        }
        let pan = (f32::from(self.pan.clamp(-63, 63)) / 63.0 + 1.0) * std::f32::consts::FRAC_PI_4;
        let vol = f32::from(self.volume.min(127)) / 127.0;
        (vol * pan.cos(), vol * pan.sin())
    }
}

fn mix_gains(t: &guitario_score::Track, any_solo: bool) -> (f32, f32) {
    Mix::of(t).gains(any_solo)
}

/// Gain of a fade at `x`, the fraction of the note played (0..=1).
fn fade_gain(fade: Option<Fade>, x: f64) -> f32 {
    let x = if x.is_nan() { 1.0 } else { x.clamp(0.0, 1.0) as f32 };
    match fade {
        None => 1.0,
        Some(Fade::In) => x,
        Some(Fade::Out) => 1.0 - x,
        Some(Fade::Swell) => (x * std::f32::consts::PI).sin().max(if x > 0.5 { 0.0 } else { x }),
    }
}

struct TrackBus {
    gain_l: f32,
    gain_r: f32,
    /// Where the gains glide to after a mixer change (volume, pan, mute, solo).
    target_l: f32,
    target_r: f32,
    drive: f32,
    tone: f32,
    lp: f32,
    hp: f32,
    hp_prev: f32,
    brightness: f32,
    sustain: f32,
}

const MAX_VOICES: usize = 96;
/// Sampled kits layer several microphones per hit, so they get a bigger pool.
const MAX_SAMPLE_VOICES: usize = 256;
const MAX_SECONDS: f64 = 60.0 * 60.0;

pub struct Player {
    sr: f32,
    map: TempoMap,
    notes: Vec<NoteEvent>,
    clicks: Vec<(f64, bool)>,
    next_note: usize,
    next_click: usize,
    voices: Vec<StringVoice>,
    drums: Vec<DrumVoice>,
    /// Sampled (SFZ) instrument per track; a track without one uses the built-in synth.
    instruments: Vec<Option<Arc<Instrument>>>,
    sampled: Vec<SampleVoice>,
    sample_acc: Vec<(f32, f32)>,
    /// Note counter per track, for round-robin sample selection.
    seq: Vec<u32>,
    /// Which tracks are drum kits.
    drum_track: Vec<bool>,
    active_clicks: Vec<Click>,
    buses: Vec<TrackBus>,
    acc: Vec<f32>,
    noise: Noise,
    t: f64,
    start: f64,
    end: f64,
    count_in_end: f64,
    count_in_clicks: Vec<(f64, bool)>,
    playing: bool,
    pub opts: Options,
    /// The mixer as last set (volume, pan, mute, solo per track).
    mix: Vec<Mix>,
    /// Per track, volume and pan automation in playback time: `(from sec, to sec, from, to)`.
    automation: Vec<(Vec<Ramp>, Vec<Ramp>)>,
    /// Samples until automations are looked up again.
    automation_countdown: u32,
    beats: Vec<Vec<BeatTime>>,
    /// Beats per bar of each tempo-map segment, for the count-in.
    meter: Vec<u32>,
}

/// The sorted, distinct MIDI keys `track` plays, so a sampled instrument can load only the
/// samples the score needs. Dead notes count as the open string's pitch.
pub fn used_keys(score: &Score, track: usize) -> Vec<u8> {
    used_keys_by_track(score).into_iter().nth(track).unwrap_or_default()
}

/// [`used_keys`] for every track, from one pass over the timeline.
pub fn used_keys_by_track(score: &Score) -> Vec<Vec<u8>> {
    let mut keys: Vec<Vec<u8>> = vec![Vec::new(); score.tracks.len()];
    for p in timeline(score) {
        let (Some(t), Some(list)) = (score.tracks.get(p.track), keys.get_mut(p.track)) else { continue };
        let key = match p.pitch {
            Some(pt) => Some(pt.0),
            None if p.dead => t.tuning.strings.get(usize::from(p.string)).copied(),
            None => None,
        };
        list.extend(key);
    }
    for list in &mut keys {
        list.sort_unstable();
        list.dedup();
    }
    keys
}

fn freq_of(midi: u8) -> f32 {
    440.0 * 2f32.powf((f32::from(midi) - 69.0) / 12.0)
}

impl Player {
    pub fn new(score: &Score, sample_rate: u32, opts: Options) -> Player {
        let sr = (sample_rate as f32).clamp(8_000.0, 384_000.0);
        let map = TempoMap::new(score);
        let any_solo = score.tracks.iter().any(|t| t.solo);

        let mut notes: Vec<NoteEvent> = Vec::new();
        let mut played = timeline(score);
        played.sort_by_key(|p| (p.track, p.string, p.start_tick));
        // Every track's notes are kept: muting and soloing silence a track's bus, so the mixer can
        // change them while the song plays.
        for (i, p) in played.iter().enumerate() {
            let start = map.tick_to_sec(p.start_tick);
            let end = map.tick_to_sec(p.start_tick + u64::from(p.ticks));
            if start > MAX_SECONDS {
                continue;
            }
            let prev = i
                .checked_sub(1)
                .and_then(|j| played.get(j))
                .filter(|q| q.track == p.track && q.string == p.string && q.start_tick + u64::from(q.ticks) == p.start_tick);
            let legato = prev.is_some_and(|q| q.effects.hammer || matches!(q.effects.slide, Some(Slide::Legato | Slide::Shift)));
            let slide_glide = prev.is_some_and(|q| matches!(q.effects.slide, Some(Slide::Legato | Slide::Shift)));
            let freq = match (p.pitch, p.dead) {
                (Some(pt), _) => freq_of(pt.0),
                (None, true) => score.tracks.get(p.track).and_then(|t| t.tuning.strings.get(usize::from(p.string))).map_or(110.0, |m| freq_of(*m)),
                (None, false) => continue,
            };
            notes.push(NoteEvent {
                midi: p.pitch.map_or(0, |pt| pt.0),
                start,
                end,
                track: p.track,
                string: p.string,
                freq,
                velocity: f32::from(p.velocity) / 127.0,
                dead: p.dead,
                palm_mute: p.effects.palm_mute,
                let_ring: p.effects.let_ring,
                vibrato: p.effects.vibrato || p.effects.wide_vibrato,
                wide_vibrato: p.effects.wide_vibrato,
                fade: p.fade,
                bend: p.effects.bend.map_or(0.0, |q| f32::from(q) / 2.0),
                legato_from_prev: legato,
                slide_glide,
            });
        }
        notes.sort_by(|a, b| a.start.total_cmp(&b.start));

        // Metronome: one click per denominator beat, accent on the downbeat; silent in free time.
        let mut clicks = Vec::new();
        let meter: Vec<u32> =
            map.segments.iter().map(|seg| score.headers.get(seg.measure).map_or(4, |h| u32::from(h.time_signature.numerator.max(1)))).collect();
        for seg in &map.segments {
            let Some(h) = score.headers.get(seg.measure) else { continue };
            if h.free_time {
                continue;
            }
            let n = u32::from(h.time_signature.numerator.max(1));
            let step = seg.ticks / n;
            for k in 0..n {
                clicks.push((seg.start_sec + seg.sec_at(k * step), k == 0));
            }
        }

        let buses = score
            .tracks
            .iter()
            .map(|t| {
                let (gain_l, gain_r) = mix_gains(t, any_solo);
                let (drive, tone, brightness, sustain) = match t.program {
                    29 => (6.0, 0.35, 0.8, 0.9993),
                    30 => (14.0, 0.28, 0.85, 0.9995),
                    _ if t.is_drums() => (1.0, 0.95, 1.0, 1.0),
                    32..=39 => (1.0, 0.25, 0.35, 0.9992),
                    24 | 25 => (1.0, 0.7, 0.75, 0.9985),
                    _ => (1.0, 0.55, 0.65, 0.9988),
                };
                TrackBus { gain_l, gain_r, target_l: gain_l, target_r: gain_r, drive, tone, lp: 0.0, hp: 0.0, hp_prev: 0.0, brightness, sustain }
            })
            .collect::<Vec<_>>();
        let beats = (0..score.tracks.len()).map(|ti| map.beat_times(score, ti)).collect();
        // Volume and pan automation, laid on playback time bar by bar (repeats included).
        let automation = (0..score.tracks.len())
            .map(|ti| {
                let (vol, pan) = score.mix_curves(ti);
                let lay = |curve: &[Option<(f64, f64)>]| -> Vec<Ramp> {
                    map.segments
                        .iter()
                        .filter_map(|seg| curve.get(seg.measure).copied().flatten().map(|(a, b)| (seg.start_sec, seg.start_sec + seg.duration(), a, b)))
                        .collect()
                };
                (lay(&vol), lay(&pan))
            })
            .collect();
        let mix = score.tracks.iter().map(Mix::of).collect();
        let drum_track = score.tracks.iter().map(|t| t.is_drums()).collect();
        let end = map.total_sec;
        Player {
            sr,
            map,
            notes,
            clicks,
            next_note: 0,
            next_click: 0,
            voices: Vec::new(),
            drums: Vec::new(),
            instruments: vec![None; score.tracks.len()],
            sampled: Vec::new(),
            sample_acc: vec![(0.0, 0.0); score.tracks.len()],
            seq: vec![0; score.tracks.len()],
            drum_track,
            active_clicks: Vec::new(),
            acc: vec![0.0; buses.len()],
            buses,
            noise: Noise::new(0x9E37_79B9),
            t: 0.0,
            start: 0.0,
            end,
            count_in_end: 0.0,
            count_in_clicks: Vec::new(),
            playing: false,
            opts,
            mix,
            automation,
            automation_countdown: 0,
            beats,
            meter,
        }
    }

    /// Bus targets from the mixer, with each track's volume and pan automation at the play time.
    fn retarget(&mut self) {
        let t = self.t;
        let any_solo = self.mix.iter().any(|m| m.solo);
        for (i, bus) in self.buses.iter_mut().enumerate() {
            let Some(mut m) = self.mix.get(i).copied() else { continue };
            if let Some((vol, pan)) = self.automation.get(i) {
                if let Some(v) = ramp_at(vol, t) {
                    m.volume = v.round().clamp(0.0, 127.0) as u8;
                }
                if let Some(p) = ramp_at(pan, t) {
                    m.pan = p.round().clamp(-63.0, 63.0) as i8;
                }
            }
            (bus.target_l, bus.target_r) = m.gains(any_solo);
        }
    }

    /// Plays `track` from a sampled instrument instead of the built-in synth.
    pub fn set_instrument(&mut self, track: usize, instrument: Option<Arc<Instrument>>) {
        if let Some(slot) = self.instruments.get_mut(track) {
            *slot = instrument;
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sr as u32
    }

    pub fn duration(&self) -> f64 {
        self.end
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Song time in seconds (negative during the count-in).
    pub fn position(&self) -> f64 {
        self.t
    }

    /// The first time a beat sounds, to start playback from the cursor.
    pub fn time_of(&self, track: usize, measure: usize, beat: usize) -> Option<f64> {
        self.beats.get(track)?.iter().find(|b| b.measure == measure && b.beat == beat).map(|b| b.sec)
    }

    /// When the beat after `(measure, beat)` starts (its first pass), or the song's end: the end
    /// of a selection for looping.
    pub fn time_after(&self, track: usize, measure: usize, beat: usize) -> Option<f64> {
        let beats = self.beats.get(track)?;
        let i = beats.iter().position(|b| b.measure == measure && b.beat == beat)?;
        Some(beats.get(i + 1).map_or(self.end, |b| b.sec))
    }

    /// Limits playback (and looping) to end at `end` seconds.
    pub fn set_end(&mut self, end: f64) {
        if end.is_finite() {
            self.end = end.clamp(0.0, self.map.total_sec);
        }
    }

    /// The beat of `track` sounding now.
    pub fn current_beat(&self, track: usize) -> Option<BeatTime> {
        locate(self.beats.get(track)?, self.t.max(0.0))
    }

    pub fn play_from(&mut self, sec: f64) {
        let sec = sec.clamp(0.0, self.end);
        self.start = sec;
        self.voices.clear();
        self.drums.clear();
        self.sampled.clear();
        self.active_clicks.clear();
        self.count_in_clicks.clear();
        self.count_in_end = sec;
        self.t = sec;
        if self.opts.count_in {
            // One bar of the time signature where playback starts, at its tempo.
            let idx = self.map.segments.iter().position(|s| s.start_sec + s.duration() > sec).unwrap_or(self.map.segments.len().saturating_sub(1));
            let n = self.meter.get(idx).copied().unwrap_or(4).max(1);
            let beat = self.map.segments.get(idx).map_or(0.5, |s| f64::from(s.ticks) / f64::from(n) * s.sec_per_tick);
            for k in 0..n {
                self.count_in_clicks.push((sec - f64::from(n - k) * beat, k == 0));
            }
            self.t = sec - f64::from(n) * beat;
        }
        self.next_note = self.notes.partition_point(|n| n.start < sec);
        self.next_click = self.clicks.partition_point(|c| c.0 < sec);
        self.playing = true;
        // Start at the automation's values there, without gliding from elsewhere.
        let saved = self.t;
        self.t = sec;
        self.retarget();
        self.t = saved;
        for bus in &mut self.buses {
            (bus.gain_l, bus.gain_r) = (bus.target_l, bus.target_r);
        }
    }

    /// The song played to its end: what is held ends, but let-ring notes and cymbals ring out
    /// on their own (unlike [`Player::stop`]).
    fn finish(&mut self) {
        self.playing = false;
        for v in self.voices.iter_mut().filter(|v| !v.let_ring) {
            v.release();
        }
        for v in self.sampled.iter_mut().filter(|v| !v.let_ring) {
            v.release();
        }
    }

    /// Whether anything still sounds (a note ringing out after the end, a cymbal decaying).
    pub fn is_sounding(&self) -> bool {
        !self.voices.is_empty() || !self.drums.is_empty() || !self.sampled.is_empty() || !self.active_clicks.is_empty()
    }

    pub fn stop(&mut self) {
        self.playing = false;
        for v in &mut self.voices {
            v.release();
        }
        for d in &mut self.drums {
            d.choke();
        }
        for v in &mut self.sampled {
            v.release();
        }
    }

    fn start_drum(&mut self, n: &NoteEvent) {
        // A closing hi-hat chokes an open one.
        if matches!(n.midi, 42 | 44) {
            for d in self.drums.iter_mut().filter(|d| d.track == n.track && d.is_open_hat()) {
                d.choke();
            }
        }
        if self.drums.len() >= MAX_VOICES {
            self.drums.remove(0);
        }
        self.drums.push(DrumVoice::new(n.track, n.midi, n.velocity, self.sr));
    }

    /// The key a note plays on a sampled instrument: the drum number, the pitch, or for a dead
    /// note the open string's pitch.
    fn sample_key(n: &NoteEvent, drum: bool) -> u8 {
        if drum || !n.dead {
            return n.midi;
        }
        let midi = (69.0 + 12.0 * (n.freq.max(1.0) / 440.0).log2()).round();
        u8::try_from(midi as i32).unwrap_or(40)
    }

    fn start_sampled(&mut self, n: &NoteEvent, inst: &Instrument) {
        let drum = self.drum_track.get(n.track).copied().unwrap_or(false);
        let key = Self::sample_key(n, drum);
        // Legato notes are softer: no new pick attack. A string sounds one note at a time.
        let mut velocity = n.velocity
            * if n.dead {
                0.6
            } else if n.legato_from_prev {
                0.75
            } else {
                1.0
            };
        velocity = velocity.clamp(0.0, 1.0);
        // A string sounds one note at a time: a new note stops the one ringing there.
        if !drum {
            for v in self.sampled.iter_mut().filter(|v| v.track == n.track && v.string == n.string && !v.finished()) {
                v.damp();
            }
        }
        let counter = self.seq.get_mut(n.track).map_or(0, |c| {
            *c = c.wrapping_add(1);
            *c
        });
        let rand = (self.noise.next() + 1.0) * 0.5;
        let hits = inst.select(key, (velocity * 127.0).round() as u8, counter, rand);
        // A sound with a group chokes the sounds it is `off_by`.
        for h in &hits {
            if h.region.group != 0 {
                for v in self.sampled.iter_mut().filter(|v| v.track == n.track && v.off_by == h.region.group) {
                    v.choke(v.off_fast);
                }
            }
        }
        // Palm mutes and dead notes are damped quickly whatever the sample's own tail.
        let damped = n.palm_mute || n.dead;
        for h in hits {
            let mut v = SampleVoice::new(n.track, n.string, h.region, h.sample, key, velocity, self.sr);
            v.vibrato = n.vibrato;
            if n.wide_vibrato {
                v.vibrato_depth = 0.45;
            }
            v.fade = n.fade;
            v.gain = fade_gain(n.fade, 0.0);
            v.let_ring = n.let_ring && !damped;
            let end = if damped { n.end.min(n.start + if n.dead { 0.08 } else { 0.22 }) } else { n.end };
            (v.note_start, v.note_end, v.bend_to) = (n.start, end, n.bend);
            if self.sampled.len() >= MAX_SAMPLE_VOICES {
                self.sampled.remove(0);
            }
            self.sampled.push(v);
        }
    }

    /// Takes the score's mixer settings (volume, pan, mute, solo) while playing; the gains glide
    /// there over about 10 ms so a fader move does not click. A score with other tracks is ignored.
    pub fn update_mix(&mut self, score: &Score) {
        let mix: Vec<Mix> = score.tracks.iter().map(Mix::of).collect();
        self.set_mix(&mix);
    }

    /// The same from the mixer's values directly (one per track, in order), so a fader being
    /// dragged can be heard before it is let go.
    pub fn set_mix(&mut self, mix: &[Mix]) {
        if mix.len() != self.buses.len() {
            return;
        }
        self.mix = mix.to_vec();
        self.retarget();
    }

    fn start_note(&mut self, n: &NoteEvent) {
        // A muted (or not soloed) track starts no voices; when it is heard again its next notes
        // sound. Volume, even an automation at zero, only scales what plays.
        let any_solo = self.mix.iter().any(|m| m.solo);
        if self.mix.get(n.track).is_none_or(|m| m.muted || (any_solo && !m.solo)) {
            return;
        }
        if let Some(inst) = self.instruments.get(n.track).cloned().flatten() {
            self.start_sampled(n, &inst);
            return;
        }
        if self.drum_track.get(n.track).copied().unwrap_or(false) {
            self.start_drum(n);
            return;
        }
        // A string sounds one note at a time.
        if n.legato_from_prev
            && let Some(v) = self.voices.iter_mut().find(|v| v.track == n.track && v.string == n.string && !v.released)
        {
            v.glide_to(n.freq, if n.slide_glide { 0.06 } else { 0.0 });
            v.vibrato = n.vibrato;
            v.let_ring = n.let_ring;
            (v.note_start, v.note_end, v.bend_to) = (n.start, n.end, n.bend);
            return;
        }
        // A string sounds one note at a time: a new note stops the one ringing there (a let-ring
        // note too); notes on other strings ring on.
        for v in self.voices.iter_mut().filter(|v| v.track == n.track && v.string == n.string) {
            v.damp();
        }
        let Some(bus) = self.buses.get(n.track) else { return };
        let (brightness, sustain) = if n.dead {
            (0.9, 0.95)
        } else if n.palm_mute {
            (bus.brightness * 0.45, 0.996)
        } else {
            (bus.brightness, bus.sustain)
        };
        let pluck = Pluck { freq: n.freq, velocity: n.velocity * if n.dead { 0.6 } else { 1.0 }, brightness, sustain };
        let mut v = StringVoice::new(n.track, n.string, self.sr, &pluck, &mut self.noise);
        v.vibrato = n.vibrato;
        if n.wide_vibrato {
            v.vibrato_depth = 0.04;
        }
        v.fade = n.fade;
        v.gain = fade_gain(n.fade, 0.0);
        v.let_ring = n.let_ring;
        (v.note_start, v.note_end, v.bend_to) = (n.start, n.end, n.bend);
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        self.voices.push(v);
    }

    /// Fills interleaved `out` with `channels` channels. Silence when stopped (releasing voices
    /// still ring out).
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        let ch = channels.max(1);
        let dt = f64::from(self.opts.speed.clamp(0.25, 2.0)) / f64::from(self.sr);
        let glide = 1.0 - (-1.0 / (0.01 * self.sr)).exp();
        let automated = self.automation.iter().any(|(v, p)| !v.is_empty() || !p.is_empty());
        for frame in out.chunks_mut(ch) {
            if automated && self.playing {
                if self.automation_countdown == 0 {
                    self.retarget();
                    self.automation_countdown = 64;
                }
                self.automation_countdown -= 1;
            }
            for bus in &mut self.buses {
                bus.gain_l += (bus.target_l - bus.gain_l) * glide;
                bus.gain_r += (bus.target_r - bus.gain_r) * glide;
            }
            if self.playing {
                // Count-in clicks, then song events due at this instant.
                while let Some(&(at, accent)) = self.count_in_clicks.first() {
                    if at > self.t {
                        break;
                    }
                    self.active_clicks.push(Click::new(accent, self.sr));
                    self.count_in_clicks.remove(0);
                }
                if self.t >= self.count_in_end {
                    while let Some(n) = self.notes.get(self.next_note).filter(|n| n.start <= self.t).cloned() {
                        self.start_note(&n);
                        self.next_note += 1;
                    }
                    while let Some(&(at, accent)) = self.clicks.get(self.next_click) {
                        if at > self.t {
                            break;
                        }
                        if self.opts.metronome {
                            self.active_clicks.push(Click::new(accent, self.sr));
                        }
                        self.next_click += 1;
                    }
                }
                // Note ends and bends.
                let t = self.t;
                for v in &mut self.voices {
                    if !v.let_ring && !v.released && t >= v.note_end {
                        v.release();
                    }
                    if v.fade.is_some() {
                        v.gain = fade_gain(v.fade, (t - v.note_start) / (v.note_end - v.note_start).max(1e-3));
                    }
                    if v.bend_to > 0.0 {
                        let half = ((v.note_end - v.note_start) * 0.5).max(1e-3);
                        v.bend_semis = v.bend_to * ((t - v.note_start) / half).clamp(0.0, 1.0) as f32;
                    }
                }
                for v in &mut self.sampled {
                    if !v.let_ring && !v.released && t >= v.note_end {
                        v.release();
                    }
                    if v.fade.is_some() {
                        v.gain = fade_gain(v.fade, (t - v.note_start) / (v.note_end - v.note_start).max(1e-3));
                    }
                    if v.bend_to > 0.0 {
                        let half = ((v.note_end - v.note_start) * 0.5).max(1e-3);
                        v.bend_semis = v.bend_to * ((t - v.note_start) / half).clamp(0.0, 1.0) as f32;
                    }
                }
                self.t += dt;
                if self.t >= self.end {
                    if self.opts.looping && self.end > 0.0 {
                        let opts = self.opts;
                        self.opts.count_in = false;
                        self.play_from(self.start.min(self.end));
                        self.opts = opts;
                    } else {
                        self.finish();
                    }
                }
            }
            // Mix voices per track, then each track through its amp and pan.
            self.acc.iter_mut().for_each(|a| *a = 0.0);
            for v in &mut self.voices {
                let y = v.tick();
                if let Some(a) = self.acc.get_mut(v.track) {
                    *a += y;
                }
            }
            for d in &mut self.drums {
                let y = d.tick(&mut self.noise);
                if let Some(a) = self.acc.get_mut(d.track) {
                    *a += y;
                }
            }
            self.sample_acc.iter_mut().for_each(|a| *a = (0.0, 0.0));
            for v in &mut self.sampled {
                let (sl, sr) = v.tick();
                if let Some(a) = self.sample_acc.get_mut(v.track) {
                    a.0 += sl;
                    a.1 += sr;
                }
            }
            let (mut l, mut r) = (0.0f32, 0.0f32);
            // Sampled tracks are already shaped (amp, cabinet), so only volume and pan apply.
            for (bus, a) in self.buses.iter().zip(&self.sample_acc) {
                l += a.0 * 0.85 * bus.gain_l;
                r += a.1 * 0.85 * bus.gain_r;
            }
            for (bus, a) in self.buses.iter_mut().zip(&self.acc) {
                let mut x = *a * 0.85;
                if bus.drive > 1.0 {
                    x = (x * bus.drive).tanh() * 0.95;
                }
                bus.lp += bus.tone * (x - bus.lp);
                // DC blocker.
                bus.hp = bus.lp - bus.hp_prev + 0.995 * bus.hp;
                bus.hp_prev = bus.lp;
                l += bus.hp * bus.gain_l;
                r += bus.hp * bus.gain_r;
            }
            for c in &mut self.active_clicks {
                let y = c.tick();
                l += y;
                r += y;
            }
            self.active_clicks.retain(|c| !c.done());
            self.voices.retain(|v| !v.finished());
            self.drums.retain(|d| !d.finished());
            self.sampled.retain(|v| !v.finished());
            let (l, r) = ((l * 1.4).tanh(), (r * 1.4).tanh());
            match frame {
                [a] => *a = 0.5 * (l + r),
                [a, b, rest @ ..] => {
                    *a = l;
                    *b = r;
                    rest.iter_mut().for_each(|x| *x = 0.0);
                }
                [] => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::{Beat, Note};
    use guitario_theory::Duration;

    fn one_note(fret: u8) -> Score {
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![Beat { rest: false, notes: vec![Note::new(0, fret)], ..Beat::rest(Duration::WHOLE) }];
        s
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    #[test]
    fn renders_sound_then_stops() {
        let mut p = Player::new(&one_note(0), 44_100, Options::default());
        assert!((p.duration() - 2.0).abs() < 1e-9);
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 44_100 * 2];
        p.render(&mut buf, 2);
        assert!(rms(&buf) > 0.01, "rms {}", rms(&buf));
        assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        let mut rest = vec![0.0f32; 44_100 * 4];
        p.render(&mut rest, 2);
        assert!(!p.is_playing());
        let tail = rest.get(44_100 * 6..).unwrap_or_default();
        assert!(rms(tail) < 1e-3, "tail rms {}", rms(tail));
    }

    #[test]
    fn pitch_is_right() {
        // Open high E: 329.6 Hz. Count zero crossings over half a second.
        let mut p = Player::new(&one_note(0), 48_000, Options::default());
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 48_000];
        p.render(&mut buf, 1);
        let seg = buf.get(12_000..20_000).unwrap_or_default();
        // Autocorrelation peak between 80 Hz and 800 Hz.
        let ac = |lag: usize| seg.iter().zip(seg.iter().skip(lag)).map(|(a, b)| a * b).sum::<f32>();
        let best = (60..600).max_by(|a, b| ac(*a).total_cmp(&ac(*b))).unwrap();
        let hz = 48_000.0 / best as f32;
        assert!((hz - 329.6).abs() < 4.0, "measured {hz} Hz");
    }

    #[test]
    fn silent_when_stopped_and_muted() {
        let mut s = one_note(5);
        s.tracks[0].muted = true;
        let mut p = Player::new(&s, 44_100, Options::default());
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 4410];
        p.render(&mut buf, 2);
        assert!(rms(&buf) < 1e-6);
    }

    #[test]
    fn count_in_and_metronome_click() {
        let opts = Options { metronome: true, count_in: true, ..Options::default() };
        let mut p = Player::new(&Score::new(), 44_100, opts);
        p.play_from(0.0);
        assert!(p.position() < -1.9, "count-in starts before zero");
        let mut buf = vec![0.0f32; 4410];
        p.render(&mut buf, 2);
        assert!(rms(&buf) > 0.005);
    }

    #[test]
    fn follows_the_cursor() {
        let s = one_note(3);
        let mut p = Player::new(&s, 44_100, Options::default());
        assert_eq!(p.time_of(0, 0, 0), Some(0.0));
        p.play_from(1.0);
        assert_eq!(p.current_beat(0).map(|b| (b.measure, b.beat)), Some((0, 0)));
        assert_eq!(p.time_of(3, 0, 0), None);
    }

    #[test]
    fn loops_a_range() {
        let mut s = one_note(3);
        let h = s.headers[0].clone();
        s.headers.push(h.clone());
        s.tracks[0].measures.push(guitario_score::Measure::empty_for(&h));
        let mut p = Player::new(&s, 8_000, Options { looping: true, ..Options::default() });
        assert_eq!(p.time_after(0, 0, 0), Some(2.0));
        p.set_end(1.0);
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 8_000 * 3];
        p.render(&mut buf, 1);
        assert!(p.is_playing());
        assert!(p.position() < 1.0);
    }

    #[test]
    fn drums_sound_and_hats_choke() {
        let mut s = Score::new();
        s.tracks = vec![guitario_score::Track::drums("Drums", &s.headers)];
        let hit = |lane, midi| Note::new(lane, midi);
        s.tracks[0].measures[0].voices[0].beats = vec![
            Beat { rest: false, notes: vec![hit(2, 46), hit(7, 36)], ..Beat::rest(Duration::QUARTER) },
            Beat { rest: false, notes: vec![hit(2, 42), hit(4, 38)], ..Beat::rest(Duration::QUARTER) },
            Beat { rest: false, notes: vec![hit(0, 49)], ..Beat::rest(Duration::new(guitario_theory::NoteValue::Half)) },
        ];
        assert!(s.validate().is_ok());
        let mut p = Player::new(&s, 44_100, Options::default());
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 44_100 * 2 * 2];
        p.render(&mut buf, 2);
        assert!(rms(&buf) > 0.01, "rms {}", rms(&buf));
        assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        // The kick is low: most of its first 50 ms is below 200 Hz, so samples change slowly.
        let first = buf.get(0..4410).unwrap_or_default();
        assert!(rms(first) > 0.02);
        // Every pad sound renders without NaNs.
        for m in 30..=90u8 {
            let mut v = crate::synth::DrumVoice::new(0, m, 1.0, 22_050.0);
            let mut n = crate::synth::Noise::new(7);
            assert!((0..5000).all(|_| v.tick(&mut n).is_finite()), "drum {m}");
        }
    }

    fn test_instrument(sfz: &str, rate: u32) -> Arc<Instrument> {
        let tone = |hz: f32| crate::sfz::Sample {
            rate,
            left: (0..rate as usize * 3).map(|n| (n as f32 / rate as f32 * hz * std::f32::consts::TAU).sin() * 0.8).collect(),
            right: None,
        };
        let load = |name: &str| Ok(tone(if name.starts_with("lo") { 110.0 } else { 440.0 }));
        Arc::new(Instrument::build("test", sfz, &mut |_| None, &load, None).unwrap())
    }

    #[test]
    fn sampled_track_plays_the_instrument_at_pitch() {
        // A 440 Hz sample rooted at A4 (69); open high E is 64, so it sounds 329.6 Hz.
        let inst = test_instrument("<region> sample=hi.wav lokey=0 hikey=127 pitch_keycenter=69 ampeg_release=0.05", 44_100);
        let mut p = Player::new(&one_note(0), 44_100, Options::default());
        p.set_instrument(0, Some(inst));
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 44_100];
        p.render(&mut buf, 1);
        assert!(rms(&buf) > 0.05, "rms {}", rms(&buf));
        let seg = buf.get(8_000..16_000).unwrap_or_default();
        let ac = |lag: usize| seg.iter().zip(seg.iter().skip(lag)).map(|(a, b)| a * b).sum::<f32>();
        let best = (60..600).max_by(|a, b| ac(*a).total_cmp(&ac(*b))).unwrap();
        let hz = 44_100.0 / best as f32;
        assert!((hz - 329.6).abs() < 6.0, "measured {hz} Hz");
        // The note ends at 2 s and the release finishes: silence, voices gone.
        let mut rest = vec![0.0f32; 44_100 * 4];
        p.render(&mut rest, 1);
        assert!(rms(rest.get(44_100 * 3..).unwrap_or_default()) < 1e-3);
        assert!(p.sampled.is_empty());
    }

    #[test]
    fn sampled_drums_choke_by_group_and_missing_samples_are_silent() {
        let mut s = Score::new();
        s.tracks = vec![guitario_score::Track::drums("Drums", &s.headers)];
        let hit = |lane, midi| Note::new(lane, midi);
        s.tracks[0].measures[0].voices[0].beats = vec![
            Beat { rest: false, notes: vec![hit(2, 46)], ..Beat::rest(Duration::QUARTER) },
            Beat { rest: false, notes: vec![hit(2, 42)], ..Beat::rest(Duration::QUARTER) },
        ];
        let inst = test_instrument("<group> loop_mode=one_shot group=1 off_by=1\n<region> sample=lo.wav key=46\n<region> sample=lo2.wav key=42", 44_100);
        let mut p = Player::new(&s, 44_100, Options::default());
        p.set_instrument(0, Some(inst));
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 44_100 / 2 + 2000];
        p.render(&mut buf, 2);
        assert!(rms(&buf) > 0.02);
        // The closed hat's group chokes the open hat: the second voice survives, the first is gone.
        let mut more = vec![0.0f32; 4410];
        p.render(&mut more, 2);
        assert_eq!(p.sampled.len(), 1);
        assert_eq!(used_keys(&s, 0), vec![42, 46]);
    }

    #[test]
    fn count_in_follows_the_time_signature() {
        for (num, den, clicks) in [(3u8, 4u8, 3usize), (6, 8, 6), (7, 8, 7), (4, 4, 4)] {
            let mut s = Score::new();
            s.headers[0].time_signature = guitario_theory::TimeSignature::new(num, den).unwrap();
            s.tracks[0].measures[0] = guitario_score::Measure::empty_for(&s.headers[0]);
            let mut p = Player::new(&s, 8_000, Options { count_in: true, ..Options::default() });
            p.play_from(0.0);
            assert_eq!(p.count_in_clicks.len(), clicks, "{num}/{den}");
            // One bar of the signature at 120 bpm.
            let bar = f64::from(num) * 4.0 / f64::from(den) * 0.5;
            assert!((p.position() + bar).abs() < 1e-6, "{num}/{den}: {}", p.position());
        }
    }

    #[test]
    fn the_mixer_changes_volume_pan_mute_and_solo_while_playing() {
        // Two guitars holding long notes.
        let mut s = one_note(5);
        let mut second = s.tracks[0].clone();
        second.measures[0].voices[0].beats[0].notes = vec![Note::new(1, 5)];
        s.tracks.push(second);
        let mut p = Player::new(&s, 22_050, Options::default());
        p.play_from(0.0);
        // Root mean square of each channel over the next `ms`.
        let rms = |p: &mut Player, ms: usize| {
            let mut buf = vec![0f32; 22_050 * ms / 1000 * 2];
            p.render(&mut buf, 2);
            let half = buf.len() / 4;
            let tail = &buf[half * 2..];
            let ch = |c: usize| (tail.iter().skip(c).step_by(2).map(|x| x * x).sum::<f32>() / (tail.len() / 2) as f32).sqrt();
            (ch(0), ch(1))
        };
        let both = rms(&mut p, 200);
        assert!(both.0 > 0.01 && both.1 > 0.01, "{both:?}");
        // Pan the first hard left and mute the second: the right channel goes quiet.
        s.tracks[0].pan = -63;
        s.tracks[1].muted = true;
        p.update_mix(&s);
        let left = rms(&mut p, 200);
        assert!(left.0 > 0.01 && left.1 < left.0 * 0.05, "{left:?}");
        // Solo the muted one: nothing sounds (it is muted, the other is not soloed).
        s.tracks[1].solo = true;
        p.update_mix(&s);
        let none = rms(&mut p, 200);
        assert!(none.0 < 1e-3 && none.1 < 1e-3, "{none:?}");
        // A fader move glides: the first samples after it are close to before.
        s.tracks[1].solo = false;
        s.tracks[0].pan = 0;
        p.update_mix(&s);
        let mut buf = vec![0f32; 2];
        p.render(&mut buf, 2);
        assert!(buf.iter().all(|x| x.abs() < 0.05), "{buf:?}");
        // Another score shape is ignored rather than misapplied.
        p.update_mix(&Score::new());
    }

    #[test]
    fn volume_and_pan_automation_play_over_the_mixer() {
        // A held note over three bars: silent in bar 1, swelling to full over bar 2, panned left in bar 3.
        let mut s = Score::new();
        s.tempo = 240;
        for _ in 0..2 {
            let h = s.headers[0].clone();
            s.headers.push(h.clone());
            s.tracks[0].measures.push(guitario_score::Measure::empty_for(&h));
        }
        for m in &mut s.tracks[0].measures {
            m.voices[0].beats = vec![Beat { rest: false, notes: vec![Note::new(0, 5)], ..Beat::rest(Duration::WHOLE) }];
            m.voices[0].beats[0].notes[0].effects.let_ring = true;
        }
        // Silent from bar 1; still 0 at the start of bar 2, gliding to full by bar 3.
        s.tracks[0].measures[0].volume = Some(0);
        s.tracks[0].measures[1].volume = Some(0);
        s.tracks[0].measures[2].volume = Some(127);
        s.tracks[0].measures[2].volume_linear = true;
        s.tracks[0].measures[2].pan = Some(-63);
        let mut p = Player::new(&s, 22_050, Options::default());
        p.play_from(0.0);
        // One bar is one second at 240 bpm. Root mean square per channel over `secs`.
        let rms = |p: &mut Player, secs: f32| {
            let mut buf = vec![0f32; (22_050.0 * secs) as usize * 2];
            p.render(&mut buf, 2);
            let ch = |c: usize| (buf.iter().skip(c).step_by(2).map(|x| x * x).sum::<f32>() / (buf.len() / 2) as f32).sqrt();
            (ch(0), ch(1))
        };
        // The same notes without automation, at full volume, to compare window by window.
        let mut plain_score = s.clone();
        plain_score.tracks[0].volume = 127;
        for m in &mut plain_score.tracks[0].measures {
            (m.volume, m.pan) = (None, None);
        }
        let mut plain = Player::new(&plain_score, 22_050, Options::default());
        plain.play_from(0.0);
        let mut window = |secs: f32| (rms(&mut p, secs), rms(&mut plain, secs));
        let (bar1, _) = window(1.0);
        let (early, early_plain) = window(0.4);
        window(0.2);
        let (late, late_plain) = window(0.4);
        // Past the pan's 10 ms glide, which bar 3's attack straddles.
        window(0.1);
        let (bar3, _) = window(0.8);
        assert!(bar1.0 < 1e-3, "silent by automation in bar 1: {bar1:?}");
        let (ratio_early, ratio_late) = (early.0 / early_plain.0, late.0 / late_plain.0);
        assert!(ratio_early > 0.05 && ratio_early < 0.4 && ratio_late > 0.6, "swelling through bar 2: {ratio_early} then {ratio_late}");
        assert!(bar3.0 > 0.01 && bar3.1 < bar3.0 * 0.05, "hard left in bar 3: {bar3:?}");
        // Starting in the middle of the swell starts at its level there, not from silence.
        let mut q = Player::new(&s, 22_050, Options::default());
        q.play_from(1.5);
        // Halfway through the swell: volume 63.5 of 127, centred: about 0.5 x 0.707 on each side.
        let g = q.buses[0].gain_l;
        assert!((g - 0.354).abs() < 0.01, "{g}");
        assert_eq!(ramp_at(&[(1.0, 2.0, 0.0, 10.0)], 1.5), Some(5.0));
        assert_eq!((ramp_at(&[(1.0, 2.0, 0.0, 10.0)], 0.5), ramp_at(&[(1.0, 2.0, 0.0, 10.0)], 9.0)), (None, Some(10.0)));
    }

    #[test]
    fn let_ring_rings_out_until_its_string_is_struck_again() {
        // 4/4 at 120 (a beat is half a second): a let-ring low E, a plain note on the A string,
        // a rest, then the low E string struck again (let ring too).
        let q = Duration::QUARTER;
        let note = |string: u8, fret: u8, ring: bool| {
            let mut n = Note::new(string, fret);
            n.effects.let_ring = ring;
            Beat { rest: false, notes: vec![n], ..Beat::rest(q) }
        };
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![note(5, 0, true), note(4, 2, false), Beat::rest(q), note(5, 3, true)];
        let mut p = Player::new(&s, 22_050, Options::default());
        p.play_from(0.0);
        let run = |p: &mut Player, secs: f64| {
            let mut buf = vec![0f32; (secs * 22_050.0) as usize * 2];
            p.render(&mut buf, 2);
        };
        let on = |p: &Player, string: u8| p.voices.iter().filter(|v| v.string == string && !v.released).count();
        // Beat 3: the first low E rings on past its written quarter and through the A string's
        // note; the A string's note, not let ring, has stopped.
        run(&mut p, 1.2);
        assert_eq!((on(&p, 5), on(&p, 4)), (1, 0));
        let first = p.voices.iter().position(|v| v.string == 5).unwrap();
        assert!(p.voices[first].base_freq < 83.0, "still the open E");
        // Beat 4 strikes the low E string again: the old note stops within milliseconds, the new
        // one rings.
        run(&mut p, 0.4);
        let sounding: Vec<f32> = p.voices.iter().filter(|v| v.string == 5 && !v.released).map(|v| v.base_freq).collect();
        assert!(sounding.len() == 1 && sounding[0] > 95.0, "only the new G rings: {sounding:?}");
        run(&mut p, 0.1);
        assert_eq!(p.voices.iter().filter(|v| v.string == 5).count(), 1, "the old E has died away");
        // The song ends at 2 s: the let-ring note rings on after it instead of being cut.
        run(&mut p, 1.0);
        assert!(!p.is_playing() && on(&p, 5) == 1 && p.is_sounding());
        // ... and dies away on its own.
        let mut waited = 0.0;
        while p.is_sounding() && waited < 31.0 {
            run(&mut p, 0.5);
            waited += 0.5;
        }
        // A low G fades out over several seconds, as a real string does, not cut and not endless.
        assert!(!p.is_sounding() && (3.0..12.0).contains(&waited), "rang {waited} s more");
        // Pressing Stop is different: it silences a ringing note.
        let mut p = Player::new(&s, 22_050, Options::default());
        p.play_from(0.0);
        run(&mut p, 0.2);
        p.stop();
        assert_eq!(on(&p, 5), 0);
    }

    #[test]
    fn a_sampled_let_ring_note_stops_quickly_when_its_string_is_struck_again() {
        // A region with a long release (2 s): restriking the string must not wait for it.
        let inst = test_instrument("<region> sample=hi.wav lokey=0 hikey=127 pitch_keycenter=69 ampeg_release=2", 22_050);
        let q = Duration::QUARTER;
        let note = |string: u8, fret: u8| {
            let mut n = Note::new(string, fret);
            n.effects.let_ring = true;
            Beat { rest: false, notes: vec![n], ..Beat::rest(q) }
        };
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![note(0, 0), note(1, 0), note(0, 3), Beat::rest(q)];
        let mut p = Player::new(&s, 22_050, Options::default());
        p.set_instrument(0, Some(inst));
        p.play_from(0.0);
        // Beats are half a second: the high E string is struck again at 1 s; look 0.5 s later.
        let mut buf = vec![0f32; 22_050 * 3];
        p.render(&mut buf, 2);
        let first: Vec<(u8, f64)> = p.sampled.iter().map(|v| (v.string, v.note_start)).collect();
        let strings: Vec<u8> = first.iter().map(|f| f.0).collect();
        let on_e: Vec<f64> = first.iter().filter(|f| f.0 == 0).map(|f| f.1).collect();
        assert_eq!(on_e, vec![1.0], "only the restruck note is left on the high E: {first:?}");
        assert!(strings.contains(&1), "a let-ring note on another string rings on: {strings:?}");
    }

    #[test]
    fn fades_shape_the_gain() {
        assert_eq!(fade_gain(None, 0.3), 1.0);
        assert!(fade_gain(Some(Fade::In), 0.0) < 1e-6 && (fade_gain(Some(Fade::In), 1.0) - 1.0).abs() < 1e-6);
        assert!(fade_gain(Some(Fade::Out), 1.0) < 1e-6);
        assert!(fade_gain(Some(Fade::Swell), 0.5) > 0.99 && fade_gain(Some(Fade::Swell), 1.0) < 1e-3);
        assert!(fade_gain(Some(Fade::In), f64::NAN).is_finite());
    }

    #[test]
    fn hostile_rates_and_buffers() {
        let mut p = Player::new(&one_note(36), 0, Options { speed: f32::NAN, ..Options::default() });
        p.play_from(f64::NAN);
        let mut buf = vec![0.0f32; 1001];
        p.render(&mut buf, 0);
        p.render(&mut [], 2);
        assert!(buf.iter().all(|x| x.is_finite()));
    }
}
