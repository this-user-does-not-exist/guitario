use crate::sfz::{Instrument, SampleVoice};
use crate::synth::{Click, DrumVoice, Noise, Pluck, StringVoice};
use crate::tempo::{BeatTime, TempoMap, locate};
use guitario_score::{Score, Slide, timeline};
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
}

struct TrackBus {
    gain_l: f32,
    gain_r: f32,
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
    beats: Vec<Vec<BeatTime>>,
}

/// The sorted, distinct MIDI keys `track` plays, so a sampled instrument can load only the
/// samples the score needs. Dead notes count as the open string's pitch.
pub fn used_keys(score: &Score, track: usize) -> Vec<u8> {
    let Some(t) = score.tracks.get(track) else { return Vec::new() };
    let mut keys: Vec<u8> = timeline(score)
        .iter()
        .filter(|p| p.track == track)
        .filter_map(|p| match p.pitch {
            Some(pt) => Some(pt.0),
            None if p.dead => t.tuning.strings.get(usize::from(p.string)).copied(),
            None => None,
        })
        .collect();
    keys.sort_unstable();
    keys.dedup();
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
        let audible = |ti: usize| score.tracks.get(ti).is_some_and(|t| !t.muted && (!any_solo || t.solo));

        let mut notes: Vec<NoteEvent> = Vec::new();
        let mut played = timeline(score);
        played.sort_by_key(|p| (p.track, p.string, p.start_tick));
        for (i, p) in played.iter().enumerate() {
            if !audible(p.track) {
                continue;
            }
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
                vibrato: p.effects.vibrato,
                bend: p.effects.bend.map_or(0.0, |q| f32::from(q) / 2.0),
                legato_from_prev: legato,
                slide_glide,
            });
        }
        notes.sort_by(|a, b| a.start.total_cmp(&b.start));

        // Metronome: one click per denominator beat, accent on the downbeat.
        let mut clicks = Vec::new();
        for seg in &map.segments {
            let Some(h) = score.headers.get(seg.measure) else { continue };
            let n = u32::from(h.time_signature.numerator.max(1));
            let step = seg.ticks / n;
            for k in 0..n {
                clicks.push((seg.start_sec + f64::from(k * step) * seg.sec_per_tick, k == 0));
            }
        }

        let buses = score
            .tracks
            .iter()
            .map(|t| {
                let pan = (f32::from(t.pan.clamp(-63, 63)) / 63.0 + 1.0) * std::f32::consts::FRAC_PI_4;
                let vol = f32::from(t.volume.min(127)) / 127.0;
                let (drive, tone, brightness, sustain) = match t.program {
                    29 => (6.0, 0.35, 0.8, 0.9993),
                    30 => (14.0, 0.28, 0.85, 0.9995),
                    _ if t.is_drums() => (1.0, 0.95, 1.0, 1.0),
                    32..=39 => (1.0, 0.25, 0.35, 0.9992),
                    24 | 25 => (1.0, 0.7, 0.75, 0.9985),
                    _ => (1.0, 0.55, 0.65, 0.9988),
                };
                TrackBus { gain_l: vol * pan.cos(), gain_r: vol * pan.sin(), drive, tone, lp: 0.0, hp: 0.0, hp_prev: 0.0, brightness, sustain }
            })
            .collect::<Vec<_>>();
        let beats = (0..score.tracks.len()).map(|ti| map.beat_times(score, ti)).collect();
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
            beats,
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
            // One bar of clicks at the tempo where playback starts.
            let tick = self
                .map
                .segments
                .iter()
                .find(|s| s.start_sec + f64::from(s.ticks) * s.sec_per_tick > sec)
                .or(self.map.segments.last())
                .map_or(0, |s| s.start_tick);
            let rate = self.map.rate_at(tick);
            let beat = 960.0 * rate;
            for k in 0..4 {
                self.count_in_clicks.push((sec - (4 - k) as f64 * beat, k == 0));
            }
            self.t = sec - 4.0 * beat;
        }
        self.next_note = self.notes.partition_point(|n| n.start < sec);
        self.next_click = self.clicks.partition_point(|c| c.0 < sec);
        self.playing = true;
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
        if !drum {
            for v in self.sampled.iter_mut().filter(|v| v.track == n.track && v.string == n.string && !v.released) {
                v.release();
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
            v.let_ring = n.let_ring && !damped;
            let end = if damped { n.end.min(n.start + if n.dead { 0.08 } else { 0.22 }) } else { n.end };
            (v.note_start, v.note_end, v.bend_to) = (n.start, end, n.bend);
            if self.sampled.len() >= MAX_SAMPLE_VOICES {
                self.sampled.remove(0);
            }
            self.sampled.push(v);
        }
    }

    fn start_note(&mut self, n: &NoteEvent) {
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
        for v in self.voices.iter_mut().filter(|v| v.track == n.track && v.string == n.string) {
            v.release();
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
        for frame in out.chunks_mut(ch) {
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
                    if v.bend_to > 0.0 {
                        let half = ((v.note_end - v.note_start) * 0.5).max(1e-3);
                        v.bend_semis = v.bend_to * ((t - v.note_start) / half).clamp(0.0, 1.0) as f32;
                    }
                }
                for v in &mut self.sampled {
                    if !v.let_ring && !v.released && t >= v.note_end {
                        v.release();
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
                        self.stop();
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
        let mut load = |name: &str| Ok(tone(if name.starts_with("lo") { 110.0 } else { 440.0 }));
        Arc::new(Instrument::build("test", sfz, &mut |_| None, &mut load, None).unwrap())
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
    fn hostile_rates_and_buffers() {
        let mut p = Player::new(&one_note(36), 0, Options { speed: f32::NAN, ..Options::default() });
        p.play_from(f64::NAN);
        let mut buf = vec![0.0f32; 1001];
        p.render(&mut buf, 0);
        p.render(&mut [], 2);
        assert!(buf.iter().all(|x| x.is_finite()));
    }
}
