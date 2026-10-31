use crate::articulation::{Articulation, legato_flags, sample_key, used_keys_by_articulation};
use crate::body::{Body, BodyKind, curve};
use crate::bowed::{BowedVoice, Stroke, section_player};
use crate::fx::Chain;
use crate::instrument::Voicing;
use crate::piano::{PianoTone, PianoVoice, key_of};
use crate::sfz::{Instrument, SampleVoice};
use crate::synth::{Click, DrumVoice, Noise, Pluck, SlideTime, StringVoice, WhammyTime, curve_semis};
use crate::tempo::{BeatTime, TempoMap, locate};
use guitario_score::{BeatTechnique, Effect, Fade, Master, PlayingStyle, Score, Slide, default_chain, timeline};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub metronome: bool,
    pub count_in: bool,
    pub looping: bool,
    /// Playback speed, 1.0 = written tempo (pitch is unaffected).
    pub speed: f32,
    /// Relative tonality: pitched tracks sound this many semitones higher (lower if negative),
    /// clamped to an octave either way; the score is unchanged and drums are not affected.
    pub transpose: i8,
}

impl Default for Options {
    fn default() -> Self {
        Options { metronome: false, count_in: false, looping: false, speed: 1.0, transpose: 0 }
    }
}

impl Options {
    /// The relative tonality as a frequency ratio.
    fn pitch_ratio(&self) -> f32 {
        2f32.powf(f32::from(self.transpose.clamp(-12, 12)) / 12.0)
    }
}

#[derive(Clone, Debug)]
struct NoteEvent {
    start: f64,
    end: f64,
    track: usize,
    string: u8,
    /// The pitch of the length of string that vibrates (a harmonic sounds a partial of it).
    freq: f32,
    /// The pitch heard, in MIDI notes with a fraction: a harmonic's partial.
    pitch: f32,
    velocity: f32,
    dead: bool,
    /// Played on the open string.
    open: bool,
    /// Under a slur from an earlier note: one bow stroke.
    slurred: bool,
    /// Shortened: staccato.
    staccato: bool,
    palm_mute: bool,
    /// How hard the track's palm mutes damp, 0 to 1 (its interpretation).
    mute: f32,
    let_ring: bool,
    vibrato: bool,
    /// How the string is pulled while the note rings.
    bend: Option<guitario_score::Bend>,
    /// The whammy bar's curve over the note's beat.
    whammy: Option<WhammyTime>,
    /// A slide into the note, out of it, or on to the next note on its string.
    slide: Option<SlideTime>,
    /// The MIDI note: the drum number on a drum track.
    midi: u8,
    /// The key it plays on a sampled instrument, and which of its programs plays it.
    key: u8,
    articulation: Articulation,
    /// This note continues the previous one on its string without a pluck.
    legato_from_prev: bool,
    /// A finger, not the pick, sets the string going (tapping): no pick click.
    tapped: bool,
    /// What a finger hammering on, pulling off or tapping puts back into the ringing string, as
    /// a share of a pluck (a legato slide adds nothing).
    energy: f32,
    /// A harmonic: the partial heard (1 for a plain note) and how much of the plain note sounds
    /// under it.
    partial: u8,
    fundamental: f32,
    /// How much brighter than the track's tone it is picked: harder notes, slaps and pops are
    /// brighter.
    bright: f32,
    wide_vibrato: bool,
    fade: Option<Fade>,
}

/// How long a half-open hi-hat rings at most before the hat closes, in seconds.
const HALF_HAT: f64 = 0.18;

/// When a drum hit is cut short: a choked cymbal at the end of its note, a half-open hi-hat
/// soon after it is struck.
fn drum_cut(n: &NoteEvent) -> Option<f64> {
    let piece = guitario_theory::drums::piece(n.midi)?;
    if piece.choked() {
        Some(n.end.max(n.start + 0.01))
    } else if piece.head == guitario_theory::drums::Head::CircleSlash {
        Some(n.end.min(n.start + HALF_HAT).max(n.start + 0.01))
    } else {
        None
    }
}

/// How far a slide into a note starts from it, and how far a slide out of one goes, in semitones.
const SLIDE_IN: f32 = 3.0;
const SLIDE_OUT: f32 = 8.0;

/// The slide a note makes, in song time: into it at its start, or over its last part out of it or
/// on to `next_pitch` (the next note on its string, for a shift or legato slide).
fn slide_time(slide: Option<Slide>, start: f64, end: f64, pitch: f32, next_pitch: Option<f32>) -> Option<SlideTime> {
    let len = (end - start).max(0.0);
    let tail = |share: f64, lo: f64, hi: f64| end - (len * share).clamp(lo, hi).min(len);
    let into = |from: f32| SlideTime { start, end: start + (len * 0.4).clamp(0.03, 0.12).min(len), from, to: 0.0, fade: false };
    let out = |to: f32| SlideTime { start: tail(0.5, 0.05, 0.35), end, from: 0.0, to, fade: true };
    match slide? {
        Slide::Shift | Slide::Legato => next_pitch.map(|to| SlideTime { start: tail(0.3, 0.03, 0.18), end, from: 0.0, to: to - pitch, fade: false }),
        Slide::IntoFromBelow => Some(into(-SLIDE_IN)),
        Slide::IntoFromAbove => Some(into(SLIDE_IN)),
        Slide::OutDown => Some(out(-SLIDE_OUT)),
        Slide::OutUp => Some(out(SLIDE_OUT)),
    }
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

/// One track's sound after the instrument: its effect chain (none plays the instrument's own) and
/// the mixer's equalizer.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackSound {
    pub effects: Option<Vec<Effect>>,
    pub eq: Effect,
}

impl TrackSound {
    pub fn of(t: &guitario_score::Track) -> TrackSound {
        TrackSound { effects: t.effects.clone(), eq: t.eq.clone() }
    }
}

/// The master's left and right gains: volume (100 is unity) and a balance that only turns down
/// the side away from the pan.
fn master_gains(volume: u8, pan: i8) -> (f32, f32) {
    let v = f32::from(volume.min(127)) / 100.0;
    let pan = f32::from(pan.clamp(-63, 63)) / 63.0;
    (v * (1.0 - pan).min(1.0), v * (1.0 + pan).min(1.0))
}

/// An automation curve (per written bar) laid on playback time, bar by bar, repeats included.
fn lay(map: &TempoMap, curve: &[Option<(f64, f64)>]) -> Vec<Ramp> {
    map.segments
        .iter()
        .filter_map(|seg| curve.get(seg.measure).copied().flatten().map(|(a, b)| (seg.start_sec, seg.start_sec + seg.duration(), a, b)))
        .collect()
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
    tone: f32,
    lp: f32,
    hp: f32,
    hp_prev: f32,
    brightness: f32,
    /// A new note on a string stops the one ringing there (off: notes ring on, as on a piano).
    stringed: bool,
    program: u8,
    /// The effect chain, then the mixer's equalizer; `fx` is whether either changes the sound.
    chain: Chain,
    eq: Chain,
    fx: bool,
    /// Plays a sampled (stereo) instrument rather than the mono synth.
    sampled: bool,
    /// The sound is stereo after the chain (sampled, or the chain widens it).
    wide: bool,
    /// The modelled instrument the track plays when it has no sampled one.
    voicing: Voicing,
    /// Its bodies, each with what its voices put into it this sample.
    bodies: Vec<(Body, (f32, f32))>,
    /// A piano's sustain pedal is down: a let-ring note is sounding.
    pedal: bool,
}

/// A note on a sampled instrument with release samples (`trigger=release`): they start when it
/// ends, quieter the longer it was held.
struct PendingRelease {
    at: f64,
    held_from: f64,
    track: usize,
    string: u8,
    key: u8,
    velocity: f32,
    instrument: Arc<Instrument>,
}

/// The string a release sample is filed under: none, so a new note on its string does not cut it.
const NO_STRING: u8 = u8::MAX;

const MAX_VOICES: usize = 96;
/// Piano keys sounding at once (a held pedal through a run can ring many).
const MAX_PIANO_VOICES: usize = 64;
const MAX_BOWED_VOICES: usize = 128;
/// A section's level against a solo player's, each player at 1/sqrt(players) of it: a section is
/// fuller than one player, not as much louder as its numbers.
const SECTION_LEVEL: f32 = 0.8;
/// Players a section may have.
const MAX_SEATS: usize = 8;
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
    pianos: Vec<PianoVoice>,
    bowed: Vec<BowedVoice>,
    /// Each track's bow direction, per player of a section.
    bow_dir: Vec<[f32; MAX_SEATS]>,
    /// Sampled (SFZ) programs per track, one per articulation: the sustain program plays every
    /// articulation without one of its own; a track without it uses the built-in synth.
    instruments: Vec<[Option<Arc<Instrument>>; Articulation::COUNT]>,
    sampled: Vec<SampleVoice>,
    /// Release samples waiting for their notes to end.
    releases: Vec<PendingRelease>,
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
    /// Each track's effects and equalizer as last set.
    sound: Vec<TrackSound>,
    /// The master's compressor, reverb and equalizer, and its volume and pan as gains.
    master: Chain,
    master_fx: bool,
    master_gain: (f32, f32),
    master_target: (f32, f32),
    /// The master's volume and pan as last set, and their automation in playback time.
    master_level: (u8, i8),
    master_automation: (Vec<Ramp>, Vec<Ramp>),
    /// Frames in a row the output has been silent, to know when effect tails have died away.
    quiet: u32,
    /// Rendering for the sound card: streamed sample tails are never waited for.
    live: bool,
}

/// The sorted, distinct MIDI keys `track` plays, so a sampled instrument can load only the
/// samples the score needs: harmonics at the pitch heard, dead notes at the open string's.
pub fn used_keys(score: &Score, track: usize) -> Vec<u8> {
    used_keys_by_track(score).into_iter().nth(track).unwrap_or_default()
}

/// [`used_keys`] for every track, from one pass over the timeline.
pub fn used_keys_by_track(score: &Score) -> Vec<Vec<u8>> {
    used_keys_by_articulation(score)
        .into_iter()
        .map(|by_art| {
            let mut keys: Vec<u8> = by_art.into_iter().flatten().collect();
            keys.sort_unstable();
            keys.dedup();
            keys
        })
        .collect()
}

/// The output's ceiling: straight through below `LIMIT_KNEE`, rounding off smoothly above it to
/// never pass 1. A plain `tanh` bends the whole range, and a loud, pedalled piano's dozens of
/// sustained partials then intermodulate into clangorous, bell-like tones (-36 dB in a forte
/// passage); straight below the knee, nothing does until the mix nears full scale.
fn soft_limit(x: f32) -> f32 {
    let a = x.abs();
    if !a.is_finite() {
        return 0.0;
    }
    if a <= LIMIT_KNEE {
        return x;
    }
    let room = 1.0 - LIMIT_KNEE;
    (LIMIT_KNEE + room * ((a - LIMIT_KNEE) / room).tanh()).copysign(x)
}

const LIMIT_KNEE: f32 = 0.75;

/// The frequency of a MIDI note (with a fraction).
fn freq_of(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

impl Player {
    pub fn new(score: &Score, sample_rate: u32, opts: Options) -> Player {
        let sr = (sample_rate as f32).clamp(8_000.0, 384_000.0);
        let map = TempoMap::new(score);
        let any_solo = score.tracks.iter().any(|t| t.solo);

        let mut notes: Vec<NoteEvent> = Vec::new();
        let mut played = timeline(score);
        // In string order, so each note knows the one before and after it on its string.
        let legato = legato_flags(&mut played);
        // The written pitch of a note, in MIDI notes; a dead note takes its open string's.
        let fretted = |p: &guitario_score::PlayedBeat| match (p.pitch, p.dead) {
            (Some(pt), _) => Some(f32::from(pt.0)),
            (None, true) => score.tracks.get(p.track).and_then(|t| t.tuning.strings.get(usize::from(p.string)).map(|m| f32::from(m.saturating_add(t.capo)))),
            (None, false) => None,
        };
        // Every track's notes are kept: muting and soloing silence a track's bus, so the mixer can
        // change them while the song plays.
        for (i, p) in played.iter().enumerate() {
            let start = map.tick_to_sec(p.start_tick);
            let mut end = map.tick_to_sec(p.start_tick + u64::from(p.ticks));
            if start > MAX_SECONDS {
                continue;
            }
            let Some(written) = fretted(p) else { continue };
            let next = played.get(i + 1).filter(|q| q.track == p.track && q.string == p.string && p.start_tick + u64::from(p.ticks) == q.start_tick && !q.dead);
            let flows_on = legato.get(i + 1).copied().unwrap_or(false) && next.is_some();
            // A staccato note sounds for half its length, unless it flows into the next.
            if p.effects.staccato && !flows_on {
                end = start + (end - start) * 0.5;
            }
            let (base, partial, fundamental) = match p.harmonic {
                Some(h) => (f32::from(h.base.0), h.partial, h.fundamental),
                None => (written, 1, 1.0),
            };
            let pitch = p.harmonic.map_or(written, |h| h.semitones());
            let legato_from_prev = legato.get(i).copied().unwrap_or(false);
            let velocity = f32::from(p.velocity) / 127.0;
            let technique = match p.technique {
                Some(BeatTechnique::Slap) => 1.5,
                Some(BeatTechnique::Pop) => 1.9,
                None => 1.0,
            };
            notes.push(NoteEvent {
                midi: p.pitch.map_or(0, |pt| pt.0),
                start,
                end,
                track: p.track,
                string: p.string,
                freq: freq_of(base),
                pitch,
                velocity,
                dead: p.dead,
                open: p.harmonic.is_none()
                    && score.tracks.get(p.track).and_then(|t| t.tuning.strings.get(usize::from(p.string)).map(|m| f32::from(m.saturating_add(t.capo))))
                        == Some(written),
                staccato: p.effects.staccato && !flows_on,
                slurred: p.slurred,
                palm_mute: p.effects.palm_mute,
                mute: score.tracks.get(p.track).map_or(0.0, |t| f32::from(t.interpretation.palm_mute.min(100)) / 100.0),
                let_ring: p.effects.let_ring,
                vibrato: p.effects.vibrato || p.effects.wide_vibrato,
                wide_vibrato: p.effects.wide_vibrato,
                fade: p.fade,
                bend: p.effects.bend,
                whammy: p.whammy.map(|w| WhammyTime {
                    curve: w.curve,
                    start: map.tick_to_sec(w.from_tick),
                    end: map.tick_to_sec(w.from_tick + u64::from(w.ticks)),
                }),
                slide: slide_time(p.effects.slide, start, end, written, next.and_then(fretted)),
                key: sample_key(score, p).unwrap_or(0),
                articulation: Articulation::of(p, legato_from_prev),
                legato_from_prev,
                tapped: p.effects.tapped || p.effects.left_hand_tapped,
                energy: match i.checked_sub(1).and_then(|j| played.get(j)) {
                    _ if !legato_from_prev => 0.0,
                    _ if p.effects.tapped || p.effects.left_hand_tapped => 0.5,
                    Some(q) if q.effects.hammer => 0.35,
                    _ => 0.0,
                },
                partial,
                fundamental,
                bright: technique * (0.75 + 0.35 * velocity),
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
                // The overdriven and distorted programs get their amp from the effect chain.
                let (tone, brightness) = match t.program {
                    29 => (0.55, 0.8),
                    30 => (0.55, 0.85),
                    _ if t.is_drums() => (0.95, 1.0),
                    32..=39 => (0.25, 0.35),
                    24 | 25 => (0.7, 0.75),
                    _ => (0.55, 0.65),
                };
                // Fingers are warmer than a pick; a slapped bass is brighter.
                let brightness = brightness
                    * match t.interpretation.style {
                        PlayingStyle::Default | PlayingStyle::Pick => 1.0,
                        PlayingStyle::Fingers => 0.75,
                        PlayingStyle::Picking => 0.85,
                        PlayingStyle::BassSlap => 1.3,
                    };
                let stringed = t.interpretation.stringed;
                let voicing = if t.is_drums() { Voicing::Plucked } else { Voicing::of(t.program) };
                TrackBus {
                    gain_l,
                    gain_r,
                    target_l: gain_l,
                    target_r: gain_r,
                    tone,
                    lp: 0.0,
                    hp: 0.0,
                    hp_prev: 0.0,
                    brightness,
                    stringed,
                    program: t.program,
                    chain: Chain::new(sr),
                    eq: Chain::new(sr),
                    fx: false,
                    sampled: false,
                    wide: false,
                    voicing,
                    bodies: voicing.bodies().into_iter().map(|(k, desk)| (Body::at_desk(k, sr, desk), (0.0, 0.0))).collect(),
                    pedal: false,
                }
            })
            .collect::<Vec<_>>();
        let beats = (0..score.tracks.len()).map(|ti| map.beat_times(score, ti)).collect();
        // Volume and pan automation, laid on playback time bar by bar (repeats included).
        let automation = (0..score.tracks.len())
            .map(|ti| {
                let (vol, pan) = score.mix_curves(ti);
                (lay(&map, &vol), lay(&map, &pan))
            })
            .collect();
        let (master_vol, master_pan) = score.master_curves();
        let master_automation = (lay(&map, &master_vol), lay(&map, &master_pan));
        let mix = score.tracks.iter().map(Mix::of).collect();
        let drum_track = score.tracks.iter().map(|t| t.is_drums()).collect();
        let end = map.total_sec;
        let sound = score.tracks.iter().map(TrackSound::of).collect();
        let mut player = Player {
            sr,
            map,
            notes,
            clicks,
            next_note: 0,
            next_click: 0,
            voices: Vec::new(),
            drums: Vec::new(),
            pianos: Vec::new(),
            bowed: Vec::new(),
            bow_dir: vec![[-1.0; MAX_SEATS]; score.tracks.len()],
            instruments: (0..score.tracks.len()).map(|_| Default::default()).collect(),
            sampled: Vec::new(),
            releases: Vec::new(),
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
            sound,
            master: Chain::new(sr),
            master_fx: false,
            master_gain: (1.0, 1.0),
            master_target: (1.0, 1.0),
            master_level: (100, 0),
            master_automation,
            quiet: 0,
            live: false,
        };
        for ti in 0..player.buses.len() {
            player.configure_track(ti);
        }
        player.set_master(&score.master);
        player.master_gain = player.master_target;
        player
    }

    /// Builds track `ti`'s chain and equalizer from its sound. A track that has not set its own
    /// chain plays the instrument's: the synth's amp for its program, nothing for a sampled
    /// instrument (its recording already has its amp).
    fn configure_track(&mut self, ti: usize) {
        let sampled = self.is_sampled(ti);
        let (Some(bus), Some(sound)) = (self.buses.get_mut(ti), self.sound.get(ti)) else { return };
        match &sound.effects {
            Some(chain) => bus.chain.configure(chain),
            None if sampled => bus.chain.configure(&[]),
            None => bus.chain.configure(&default_chain(bus.program)),
        }
        bus.eq.configure(std::slice::from_ref(&sound.eq));
        bus.fx = bus.chain.active() || bus.eq.active();
        bus.sampled = sampled;
        bus.wide = sampled || !bus.bodies.is_empty() || bus.chain.widens();
    }

    /// Takes each track's effects and equalizer (one per track, in order) while playing: an effect
    /// that stays keeps its state, so a reverb's tail survives a knob move. Other lengths are
    /// ignored.
    pub fn set_sound(&mut self, sound: &[TrackSound]) {
        if sound.len() != self.buses.len() || sound == self.sound.as_slice() {
            return;
        }
        self.sound = sound.to_vec();
        for ti in 0..self.buses.len() {
            self.configure_track(ti);
        }
    }

    /// Takes the master's volume, pan and mastering effects (its automation, where it has a point,
    /// goes over the volume and pan).
    pub fn set_master(&mut self, master: &Master) {
        self.master.configure(&master.slots().map(Clone::clone));
        self.master_fx = self.master.active();
        self.master_level = (master.volume, master.pan);
        self.retarget();
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
        let (mut volume, mut pan) = self.master_level;
        if let Some(v) = ramp_at(&self.master_automation.0, t) {
            volume = v.round().clamp(0.0, 127.0) as u8;
        }
        if let Some(p) = ramp_at(&self.master_automation.1, t) {
            pan = p.round().clamp(-63.0, 63.0) as i8;
        }
        self.master_target = master_gains(volume, pan);
    }

    /// Plays `track` from a sampled instrument instead of the built-in synth.
    pub fn set_instrument(&mut self, track: usize, instrument: Option<Arc<Instrument>>) {
        self.set_articulation(track, Articulation::Sustain, instrument);
    }

    /// Plays `track`'s notes of one articulation (palm mutes, harmonics …) from their own sampled
    /// program; without one they play from the sustain program.
    pub fn set_articulation(&mut self, track: usize, articulation: Articulation, instrument: Option<Arc<Instrument>>) {
        if let Some(slot) = self.instruments.get_mut(track).and_then(|a| a.get_mut(articulation.index())) {
            *slot = instrument;
        }
        self.configure_track(track);
    }

    /// Whether `track` plays sampled programs rather than the synth.
    fn is_sampled(&self, track: usize) -> bool {
        self.instruments.get(track).is_some_and(|a| a.iter().any(Option::is_some))
    }

    /// The program `track` has of its own for `articulation`.
    fn instrument(&self, track: usize, articulation: Articulation) -> Option<&Arc<Instrument>> {
        self.instruments.get(track)?.get(articulation.index())?.as_ref()
    }

    pub fn sample_rate(&self) -> u32 {
        self.sr as u32
    }

    /// For the sound card: the tails of long samples are read from disk ahead of need by the
    /// streamer thread and never waited for (a late one plays as silence and counts in
    /// [`stream::underruns`](crate::sfz::stream::underruns)). Off (the default, for exports and
    /// tests), a tail is loaded when a note reaches it, so the result is exact.
    pub fn set_live(&mut self, live: bool) {
        self.live = live;
        if live {
            crate::sfz::stream::start();
        }
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
        self.pianos.clear();
        self.bowed.clear();
        self.sampled.clear();
        self.releases.clear();
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
        self.master_gain = self.master_target;
    }

    /// The song played to its end: what is held ends, but let-ring notes and cymbals ring out
    /// on their own (unlike [`Player::stop`]).
    fn finish(&mut self) {
        self.playing = false;
        for v in self.voices.iter_mut().filter(|v| !v.let_ring) {
            v.release();
        }
        for v in self.pianos.iter_mut().filter(|v| !v.let_ring) {
            v.release();
        }
        for v in self.bowed.iter_mut().filter(|v| !v.let_ring) {
            v.release();
        }
        for v in self.sampled.iter_mut().filter(|v| !v.let_ring) {
            v.release();
        }
    }

    /// Whether anything still sounds (a note ringing out after the end, a cymbal decaying).
    pub fn is_sounding(&self) -> bool {
        let tails = self.master_fx || self.buses.iter().any(|b| b.fx);
        !self.voices.is_empty()
            || !self.drums.is_empty()
            || !self.pianos.is_empty()
            || !self.bowed.is_empty()
            || self.buses.iter().any(|b| b.bodies.iter().any(|(body, _)| !body.is_idle()))
            || !self.sampled.is_empty()
            || !self.active_clicks.is_empty()
            || (tails && (self.quiet as f32) < self.sr * 0.25)
    }

    pub fn stop(&mut self) {
        self.playing = false;
        for v in &mut self.voices {
            v.release();
        }
        for v in &mut self.pianos {
            v.let_ring = false;
            v.release();
        }
        for v in &mut self.bowed {
            v.release();
        }
        for d in &mut self.drums {
            d.choke();
        }
        for v in &mut self.sampled {
            v.release();
        }
        self.releases.clear();
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
        let mut d = DrumVoice::new(n.track, n.midi, n.velocity, self.sr);
        if let Some(at) = drum_cut(n) {
            d.cut_after((at - n.start) as f32);
        }
        self.drums.push(d);
    }

    fn start_sampled(&mut self, n: &NoteEvent) {
        let drum = self.drum_track.get(n.track).copied().unwrap_or(false);
        let art = if drum { Articulation::Sustain } else { n.articulation };
        let own = self.instrument(n.track, art).cloned();
        let Some(inst) = own.clone().or_else(|| self.instrument(n.track, Articulation::Sustain).cloned()) else { return };
        // Without legato samples, a hammer-on, pull-off or legato slide moves the sample already
        // sounding on the string to the new pitch, with no new attack (and the finger lifts at the
        // end of this note, not the one before).
        if art == Articulation::Legato && own.is_none() && !inst.has_legato() {
            let moved_from = self.releases.iter().position(|r| r.track == n.track && r.string == n.string && r.at >= n.start - 1e-9);
            if let Some(r) = moved_from.and_then(|i| self.releases.get_mut(i)) {
                (r.at, r.key) = (n.end, n.key);
            }
            let mut moved = false;
            for v in self.sampled.iter_mut().filter(|v| v.track == n.track && v.string == n.string && !v.released) {
                v.legato_to(n.pitch);
                v.vibrato = n.vibrato;
                v.vibrato_depth = if n.wide_vibrato { 0.45 } else { 0.12 };
                v.let_ring = n.let_ring;
                v.fade = n.fade;
                v.gain = fade_gain(n.fade, 0.0);
                (v.note_start, v.note_end, v.bend, v.whammy, v.slide) = (n.start, n.end, n.bend, n.whammy, n.slide);
                v.bend_semis = curve_semis(n.bend, n.whammy, n.slide, n.start, n.start, n.end);
                moved = true;
            }
            if moved {
                return;
            }
        }
        // The written key picks the sample, so the preloaded keys still cover the score; the
        // relative tonality then re-pitches it.
        let ratio = if drum { 1.0 } else { self.opts.pitch_ratio() };
        // A recorded articulation sounds as recorded; borrowed from the sustain program, dead notes
        // and legato notes are softer, palm mutes and dead notes cut short.
        let borrowed = own.is_none() && !drum;
        let soft = match art {
            Articulation::Dead if borrowed => 0.6,
            Articulation::Legato if borrowed => 0.75,
            _ if n.tapped => 0.85,
            _ => 1.0,
        };
        let velocity = (n.velocity * soft).clamp(0.0, 1.0);
        // A string sounds one note at a time: a new note stops the one ringing there, and the
        // release noise of one struck again before its end.
        if !drum && self.buses.get(n.track).is_none_or(|b| b.stringed) {
            for v in self.sampled.iter_mut().filter(|v| v.track == n.track && v.string == n.string && !v.finished()) {
                v.damp();
            }
            self.releases.retain(|r| r.track != n.track || r.string != n.string || r.at <= n.start + 1e-9);
        }
        let damped = borrowed && (n.palm_mute || n.dead);
        let cut = if drum { drum_cut(n) } else { None };
        // A heavier palm mute cuts the sample sooner.
        let muted_for = 0.22 - 0.12 * n.mute.clamp(0.0, 1.0);
        let end = if damped { n.end.min(n.start + if n.dead { 0.08 } else { f64::from(muted_for) }) } else { n.end };
        // A semi-harmonic sounds the plain note under the partial.
        let under = n.partial > 1 && n.fundamental >= 0.5 && !drum;
        let plain = self.instrument(n.track, Articulation::Sustain).cloned().unwrap_or_else(|| Arc::clone(&inst));
        let keys = [Some((n.key, velocity, Arc::clone(&inst))), under.then_some((n.midi, velocity * n.fundamental, plain))];
        let mut voices = Vec::new();
        for (key, velocity, inst) in keys.into_iter().flatten() {
            let counter = self.seq.get_mut(n.track).map_or(0, |c| {
                *c = c.wrapping_add(1);
                *c
            });
            let rand = (self.noise.next() + 1.0) * 0.5;
            let hits = inst.select(key, (velocity * 127.0).round() as u8, counter, rand, n.legato_from_prev);
            // A sound with a group chokes the sounds it is `off_by`.
            for h in &hits {
                if h.region.group != 0 {
                    for v in self.sampled.iter_mut().filter(|v| v.track == n.track && v.off_by == h.region.group) {
                        v.choke(v.off_fast);
                    }
                }
            }
            for h in hits {
                let mut v = SampleVoice::new(n.track, n.string, h.region, h.sample, key, velocity, self.sr);
                v.live(self.live);
                v.repitch(ratio);
                voices.push(v);
            }
        }
        for mut v in voices {
            v.vibrato = n.vibrato;
            if n.wide_vibrato {
                v.vibrato_depth = 0.45;
            }
            v.fade = n.fade;
            v.gain = fade_gain(n.fade, 0.0);
            v.let_ring = n.let_ring && !damped;
            v.cut_at = cut.unwrap_or(f64::MAX);
            (v.note_start, v.note_end, v.bend, v.whammy, v.slide) = (n.start, end, n.bend, n.whammy, n.slide);
            v.bend_semis = curve_semis(n.bend, n.whammy, n.slide, n.start, n.start, end);
            if self.sampled.len() >= MAX_SAMPLE_VOICES {
                self.sampled.remove(0);
            }
            self.sampled.push(v);
        }
        // Lifting the finger at the end makes the instrument's release noise; a let-ring note
        // rings on instead.
        if inst.has_release() && !n.let_ring {
            self.releases.push(PendingRelease { at: end, held_from: n.start, track: n.track, string: n.string, key: n.key, velocity, instrument: inst });
        }
    }

    /// Starts the release samples of the notes that have ended by now.
    fn start_releases(&mut self) {
        let t = self.t;
        let mut due = Vec::new();
        self.releases.retain(|r| {
            let now = r.at <= t;
            if now {
                due.push((r.track, r.key, r.velocity, (t - r.held_from).max(0.0) as f32, Arc::clone(&r.instrument)));
            }
            !now
        });
        let ratio = self.opts.pitch_ratio();
        for (track, key, velocity, held, inst) in due {
            let counter = self.seq.get(track).copied().unwrap_or(0);
            let rand = (self.noise.next() + 1.0) * 0.5;
            for h in inst.select_release(key, (velocity * 127.0).round() as u8, counter, rand) {
                let mut v = SampleVoice::new(track, NO_STRING, h.region, h.sample, key, velocity, self.sr);
                v.live(self.live);
                v.repitch(ratio);
                v.attenuate(h.region.rt_decay * held);
                if self.sampled.len() >= MAX_SAMPLE_VOICES {
                    self.sampled.remove(0);
                }
                self.sampled.push(v);
            }
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
        if self.is_sampled(n.track) {
            self.start_sampled(n);
            return;
        }
        if self.drum_track.get(n.track).copied().unwrap_or(false) {
            self.start_drum(n);
            return;
        }
        let voicing = self.buses.get(n.track).map_or(Voicing::Plucked, |b| b.voicing);
        match voicing {
            Voicing::Piano(tone) => return self.start_piano(n, tone),
            Voicing::Bowed { body, players, tremolo, slow } => return self.start_bowed(n, body, players, tremolo, slow),
            Voicing::Pizzicato { body, players } => return self.start_pizzicato(n, body, players),
            Voicing::Plucked => {}
        }
        let freq = n.freq * self.opts.pitch_ratio();
        // A hammer-on, pull-off, legato slide or tap carries on the vibration already on the
        // string (the slide has brought it to this pitch), with what the finger adds. A harmonic
        // is a new vibration of the string.
        if n.legato_from_prev
            && n.partial == 1
            && let Some(v) = self.voices.iter_mut().find(|v| v.track == n.track && v.string == n.string && !v.released)
        {
            v.glide_to(freq, 0.0);
            if n.energy > 0.0 {
                v.excite(n.velocity * n.energy, &mut self.noise);
            }
            v.vibrato = n.vibrato;
            v.vibrato_depth = if n.wide_vibrato { 0.04 } else { 0.012 };
            v.let_ring = n.let_ring;
            v.fade = n.fade;
            v.gain = fade_gain(n.fade, 0.0);
            (v.note_start, v.note_end, v.bend, v.whammy, v.slide) = (n.start, n.end, n.bend, n.whammy, n.slide);
            v.bend_semis = curve_semis(n.bend, n.whammy, n.slide, n.start, n.start, n.end);
            return;
        }
        // A string sounds one note at a time: a new note stops the one ringing there (a let-ring
        // note too); notes on other strings ring on. Not stringed, notes ring on as on a piano.
        if self.buses.get(n.track).is_none_or(|b| b.stringed) {
            for v in self.voices.iter_mut().filter(|v| v.track == n.track && v.string == n.string) {
                v.damp();
            }
        }
        let Some(bus) = self.buses.get(n.track) else { return };
        let mut pluck = Pluck::plain(freq, n.velocity, (bus.brightness * n.bright).min(1.0));
        if n.dead {
            // A click with hardly any pitch: the hand stops the string at once.
            (pluck.velocity, pluck.brightness, pluck.decay, pluck.damping) = (n.velocity * 0.6, 0.9, Some(0.045), 0.8);
        } else if n.palm_mute {
            // The palm takes the highs off and stops the string soon; a heavier mute more so.
            let k = n.mute.clamp(0.0, 1.0);
            pluck.brightness *= 0.6 - 0.3 * k;
            pluck.decay = Some(0.7 - 0.55 * k);
            pluck.damping = 0.5 + 0.4 * k;
        }
        if n.tapped {
            // A finger, not a pick: no click, a little softer.
            pluck.brightness *= 0.6;
            pluck.velocity *= 0.85;
        }
        if n.partial > 1 {
            // The partial is high up the string's spectrum: excite it brightly.
            (pluck.partial, pluck.fundamental) = (n.partial, n.fundamental);
            pluck.brightness = pluck.brightness.max(0.85);
        }
        let mut v = StringVoice::new(n.track, n.string, self.sr, &pluck, &mut self.noise);
        v.vibrato = n.vibrato;
        if n.wide_vibrato {
            v.vibrato_depth = 0.04;
        }
        v.fade = n.fade;
        v.gain = fade_gain(n.fade, 0.0);
        v.let_ring = n.let_ring;
        (v.note_start, v.note_end, v.bend, v.whammy, v.slide) = (n.start, n.end, n.bend, n.whammy, n.slide);
        v.bend_semis = curve_semis(n.bend, n.whammy, n.slide, n.start, n.start, n.end);
        if self.voices.len() >= MAX_VOICES {
            self.voices.remove(0);
        }
        self.voices.push(v);
    }

    /// The track's body of `kind` (at a section's `desk`), by its place among the track's bodies.
    fn body_slot(&self, track: usize, kind: BodyKind, desk: Option<u8>) -> Option<usize> {
        let bodies = &self.buses.get(track)?.bodies;
        bodies.iter().position(|(b, _)| b.kind == kind && b.desk == desk).or_else(|| bodies.iter().position(|(b, _)| b.kind == kind))
    }

    fn start_piano(&mut self, n: &NoteEvent, tone: PianoTone) {
        let freq = n.freq * self.opts.pitch_ratio();
        // A dead note on a piano is a key struck softly and let go at once.
        let (velocity, end) = if n.dead { (n.velocity * 0.35, n.start + 0.06) } else { (n.velocity, n.end) };
        let key = key_of(freq);
        // The same key again: its hammer strikes the strings while they still move.
        if let Some(v) = self.pianos.iter_mut().find(|v| v.track == n.track && v.key == key && !v.is_choked()) {
            v.let_ring = n.let_ring;
            v.note_end = end;
            v.strike(velocity);
            return;
        }
        let mut v = PianoVoice::new(n.track, freq, velocity, tone, self.sr);
        v.let_ring = n.let_ring;
        v.note_end = end;
        if self.pianos.len() >= MAX_PIANO_VOICES {
            // The oldest released key gives way, or else the oldest.
            let i = self.pianos.iter().position(|v| v.released).unwrap_or(0);
            if let Some(old) = self.pianos.get_mut(i) {
                old.choke();
            }
            if self.pianos.len() >= MAX_PIANO_VOICES + 8 {
                self.pianos.remove(0);
            }
        }
        self.pianos.push(v);
    }

    fn start_bowed(&mut self, n: &NoteEvent, body: Option<BodyKind>, players: usize, tremolo: bool, slow: bool) {
        let ratio = self.opts.pitch_ratio();
        // A harmonic sounds its partial, flute-like, from a lighter bow.
        let freq = freq_of(n.pitch) * ratio;
        let kind = body.unwrap_or_else(|| BodyKind::for_pitch(n.pitch + 12.0 * ratio.log2()));
        let Some(first) = self.body_slot(n.track, kind, None) else { return };
        let seconds = (n.end - n.start).max(0.0) as f32;
        let velocity = if n.dead {
            n.velocity * 0.3
        } else if n.partial > 1 {
            n.velocity * 0.6
        } else {
            n.velocity
        };
        let mut stroke = Stroke {
            freq,
            velocity,
            seconds,
            short: n.staccato || n.dead || seconds < 0.12,
            open: n.open,
            tremolo,
            slow,
            direction: 1.0,
            crossing: false,
            section: players > 1,
        };
        let seed = ((self.noise.next() + 1.0) * 1.0e6) as u32;
        let end = if n.dead { n.start + 0.08 } else { n.end };
        // A slur keeps the stroke going (the notes of a chord share one too); anything else is a
        // new stroke, the other way.
        let slur = n.legato_from_prev || n.slurred;
        let chord = self.bowed.iter().any(|v| v.track == n.track && v.note_start == n.start && v.string != n.string);
        for seat in 0..players.clamp(1, MAX_SEATS) {
            let (detune, onset, place) = if players > 1 { section_player(seat, seed) } else { (0.0, 0.0, 0.0) };
            // A section's players on the left sound through one desk's instruments, those on the
            // right through the other's.
            let slot = self.body_slot(n.track, kind, (players > 1).then_some(u8::from(place >= 0.0))).unwrap_or(first);
            if let Some(dir) = self.bow_dir.get_mut(n.track).and_then(|d| d.get_mut(seat)) {
                if !slur && !chord {
                    *dir = -*dir;
                }
                stroke.direction = *dir;
            }
            stroke.crossing = n.slurred && !n.legato_from_prev;
            let sounding = self.bowed.iter().position(|v| v.track == n.track && v.string == n.string && v.seat == seat && !v.finished());
            let reuse = sounding.and_then(|i| self.bowed.get_mut(i)).filter(|v| v.body == kind && v.slot == slot && v.fits(freq));
            let v = match reuse {
                Some(v) => {
                    // A slur moves the finger under the same stroke; anything else is a bow change.
                    if slur && !v.released {
                        v.slur_to(freq, n.open, seconds);
                    } else {
                        v.bow(&stroke);
                    }
                    v
                }
                None => {
                    if let Some(old) = sounding.and_then(|i| self.bowed.get_mut(i)) {
                        old.damp();
                    }
                    let mut v = BowedVoice::new(n.track, n.string, kind, &stroke, seed.wrapping_add(seat as u32 * 101), self.sr);
                    v.seat = seat;
                    v.slot = slot;
                    // A section is fuller than one player, not as much louder as its numbers.
                    v.level_scale = SECTION_LEVEL / (players.max(1) as f32).sqrt();
                    v.detune = detune;
                    // Together on short notes, a little apart on long ones.
                    v.wait = (onset * (seconds / 0.6).clamp(0.3, 1.0) * self.sr) as u32;
                    let angle = (place + 1.0) * std::f32::consts::FRAC_PI_4;
                    v.pan = (angle.cos() * std::f32::consts::SQRT_2, angle.sin() * std::f32::consts::SQRT_2);
                    if self.bowed.len() >= MAX_BOWED_VOICES {
                        self.bowed.remove(0);
                    }
                    self.bowed.push(v);
                    let Some(v) = self.bowed.last_mut() else { return };
                    v
                }
            };
            v.vibrato = n.vibrato;
            v.wide = n.wide_vibrato;
            v.let_ring = n.let_ring;
            v.fade = n.fade;
            v.gain = fade_gain(n.fade, 0.0);
            (v.note_start, v.note_end, v.bend, v.whammy, v.slide) = (n.start, end, n.bend, n.whammy, n.slide);
            v.bend_semis = curve_semis(n.bend, n.whammy, n.slide, n.start, n.start, end);
        }
    }

    /// A string plucked with a finger on a bowed instrument: soft, short, and coloured by its body.
    fn start_pizzicato(&mut self, n: &NoteEvent, body: Option<BodyKind>, players: usize) {
        let ratio = self.opts.pitch_ratio();
        let freq = n.freq * ratio;
        let kind = body.unwrap_or_else(|| BodyKind::for_pitch(n.pitch + 12.0 * ratio.log2()));
        for v in self.voices.iter_mut().filter(|v| v.track == n.track && v.string == n.string) {
            v.damp();
        }
        // A stopped string under the finger rings briefly; an open one longer.
        let ring = curve(&[(5.0, 1.6), (8.0, 0.8), (11.0, 0.3)], freq.max(20.0).log2()) * if n.open { 2.0 } else { 1.0 };
        let seed = ((self.noise.next() + 1.0) * 1.0e6) as u32;
        for seat in 0..players.max(1) {
            let (detune, _, place) = if players > 1 { section_player(seat, seed) } else { (0.0, 0.0, 0.0) };
            let slot = self.body_slot(n.track, kind, (players > 1).then_some(u8::from(place >= 0.0)));
            let level = if players > 1 { SECTION_LEVEL / (players as f32).sqrt() } else { 1.0 };
            let mut pluck = Pluck::plain(freq * 2f32.powf(detune / 12.0), n.velocity * level, 0.3);
            pluck.decay = Some(ring);
            pluck.damping = 0.35;
            let mut v = StringVoice::new(n.track, n.string, self.sr, &pluck, &mut self.noise);
            v.slot = slot;
            v.let_ring = true;
            (v.note_start, v.note_end) = (n.start, n.end);
            if self.voices.len() >= MAX_VOICES {
                self.voices.remove(0);
            }
            self.voices.push(v);
        }
    }

    /// Fills interleaved `out` with `channels` channels. Silence when stopped (releasing voices
    /// still ring out).
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        let ch = channels.max(1);
        let dt = f64::from(self.opts.speed.clamp(0.25, 2.0)) / f64::from(self.sr);
        let glide = 1.0 - (-1.0 / (0.01 * self.sr)).exp();
        let automated = self.automation.iter().chain([&self.master_automation]).any(|(v, p)| !v.is_empty() || !p.is_empty());
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
            self.master_gain.0 += (self.master_target.0 - self.master_gain.0) * glide;
            self.master_gain.1 += (self.master_target.1 - self.master_gain.1) * glide;
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
                    if !self.releases.is_empty() {
                        self.start_releases();
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
                    if v.fade.is_some() || v.slide.is_some_and(|s| s.fade) {
                        v.gain = fade_gain(v.fade, (t - v.note_start) / (v.note_end - v.note_start).max(1e-3)) * v.slide.map_or(1.0, |s| s.gain_at(t));
                    }
                    if v.bend.is_some() || v.whammy.is_some() || v.slide.is_some() {
                        v.bend_semis = curve_semis(v.bend, v.whammy, v.slide, t, v.note_start, v.note_end);
                    }
                }
                for v in &mut self.pianos {
                    if !v.let_ring && !v.released && t >= v.note_end {
                        v.release();
                    }
                }
                for v in &mut self.bowed {
                    if !v.let_ring && !v.released && t >= v.note_end {
                        v.release();
                    }
                    if v.fade.is_some() || v.slide.is_some_and(|s| s.fade) {
                        v.gain = fade_gain(v.fade, (t - v.note_start) / (v.note_end - v.note_start).max(1e-3)) * v.slide.map_or(1.0, |s| s.gain_at(t));
                    }
                    if v.bend.is_some() || v.whammy.is_some() || v.slide.is_some() {
                        v.bend_semis = curve_semis(v.bend, v.whammy, v.slide, t, v.note_start, v.note_end);
                    }
                }
                for v in &mut self.sampled {
                    if !v.let_ring && !v.released && t >= v.note_end {
                        v.release();
                    }
                    if t >= v.cut_at {
                        v.choke(true);
                        v.cut_at = f64::MAX;
                    }
                    if v.fade.is_some() || v.slide.is_some_and(|s| s.fade) {
                        v.gain = fade_gain(v.fade, (t - v.note_start) / (v.note_end - v.note_start).max(1e-3)) * v.slide.map_or(1.0, |s| s.gain_at(t));
                    }
                    if v.bend.is_some() || v.whammy.is_some() || v.slide.is_some() {
                        v.bend_semis = curve_semis(v.bend, v.whammy, v.slide, t, v.note_start, v.note_end);
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
                match v.slot.and_then(|s| self.buses.get_mut(v.track).and_then(|b| b.bodies.get_mut(s))) {
                    Some((_, into)) => {
                        into.0 += y;
                        into.1 += y;
                    }
                    None => {
                        if let Some(a) = self.acc.get_mut(v.track) {
                            *a += y;
                        }
                    }
                }
            }
            for v in &mut self.pianos {
                let (yl, yr) = v.tick();
                if let Some(bus) = self.buses.get_mut(v.track) {
                    bus.pedal |= v.let_ring && !v.released;
                    if let Some((_, into)) = bus.bodies.first_mut() {
                        into.0 += yl;
                        into.1 += yr;
                    }
                }
            }
            for v in &mut self.bowed {
                let y = v.tick();
                if let Some((_, into)) = self.buses.get_mut(v.track).and_then(|b| b.bodies.get_mut(v.slot)) {
                    into.0 += y * v.pan.0;
                    into.1 += y * v.pan.1;
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
            // Each track: the synth's voicing, the sampled instrument beside it, then the effect
            // chain, the equalizer, volume and pan.
            for ((bus, a), s) in self.buses.iter_mut().zip(&self.acc).zip(&self.sample_acc) {
                bus.lp += bus.tone * (*a * 0.85 - bus.lp);
                // DC blocker.
                bus.hp = bus.lp - bus.hp_prev + 0.995 * bus.hp;
                bus.hp_prev = bus.lp;
                let (mut tl, mut tr) = (bus.hp + s.0 * 0.85, bus.hp + s.1 * 0.85);
                // A modelled instrument's strings through its body.
                for (body, into) in &mut bus.bodies {
                    body.set_pedal(bus.pedal);
                    let (bl, br) = body.tick(into.0, into.1);
                    tl += bl;
                    tr += br;
                    *into = (0.0, 0.0);
                }
                bus.pedal = false;
                if bus.fx {
                    // The synth is mono, so its effects run once until one widens the sound.
                    let stereo = bus.sampled || !bus.bodies.is_empty();
                    (tl, tr) = if stereo { bus.chain.tick(tl, tr) } else { bus.chain.tick_mono(tl) };
                    (tl, tr) = if bus.wide { bus.eq.tick(tl, tr) } else { bus.eq.tick_mono(tl) };
                }
                l += tl * bus.gain_l;
                r += tr * bus.gain_r;
            }
            if self.master_fx {
                (l, r) = self.master.tick(l, r);
            }
            (l, r) = (l * self.master_gain.0, r * self.master_gain.1);
            self.quiet = if l.abs().max(r.abs()) > 1e-4 { 0 } else { self.quiet.saturating_add(1) };
            for c in &mut self.active_clicks {
                let y = c.tick();
                l += y;
                r += y;
            }
            self.active_clicks.retain(|c| !c.done());
            self.voices.retain(|v| !v.finished());
            self.drums.retain(|d| !d.finished());
            self.pianos.retain(|v| !v.finished());
            self.bowed.retain(|v| !v.finished());
            self.sampled.retain(|v| !v.finished());
            let (l, r) = (soft_limit(l * 1.4), soft_limit(r * 1.4));
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

    /// The autocorrelation pitch of the first second of `p` (80 to 800 Hz).
    fn measured_hz(mut p: Player) -> f32 {
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 48_000];
        p.render(&mut buf, 1);
        let seg = buf.get(12_000..20_000).unwrap_or_default();
        let ac = |lag: usize| seg.iter().zip(seg.iter().skip(lag)).map(|(a, b)| a * b).sum::<f32>();
        let best = (60..600).max_by(|a, b| ac(*a).total_cmp(&ac(*b))).unwrap();
        48_000.0 / best as f32
    }

    #[test]
    fn the_whammy_bar_moves_the_pitch() {
        // Open high E (329.6 Hz) held two whole tones down by the bar is C (261.6 Hz).
        let mut s = one_note(0);
        s.tracks[0].measures[0].voices[0].beats[0].whammy = Some(guitario_score::Whammy::preset(guitario_score::WhammyKind::Predive, -8));
        let hz = measured_hz(Player::new(&s, 48_000, Options::default()));
        assert!((hz - 261.6).abs() < 4.0, "measured {hz} Hz");
        // The curve in song time: at rest before it, held after it.
        let w = WhammyTime { curve: guitario_score::Whammy::preset(guitario_score::WhammyKind::Dive, -4), start: 1.0, end: 2.0 };
        assert_eq!((curve_semis(None, Some(w), None, 0.5, 0.0, 4.0), curve_semis(None, Some(w), None, 9.0, 0.0, 4.0)), (0.0, -2.0));
        let up = guitario_score::Bend::to(4);
        assert_eq!(curve_semis(Some(up), Some(w), None, 9.0, 0.0, 4.0), 0.0, "a full-step bend and a full-step dive cancel out");
    }

    #[test]
    fn relative_tonality_shifts_the_pitch() {
        // Open high E (329.6 Hz) two semitones down is D (293.7 Hz), and the score is untouched.
        let s = one_note(0);
        let hz = measured_hz(Player::new(&s, 48_000, Options { transpose: -2, ..Options::default() }));
        assert!((hz - 293.7).abs() < 4.0, "measured {hz} Hz");
        assert_eq!(s, one_note(0));
        // Out-of-range values stop at an octave.
        let hz = measured_hz(Player::new(&one_note(0), 48_000, Options { transpose: -100, ..Options::default() }));
        assert!((hz - 164.8).abs() < 3.0, "measured {hz} Hz");
    }

    #[test]
    fn relative_tonality_changes_while_playing() {
        let mut p = Player::new(&one_note(0), 48_000, Options::default());
        p.opts.transpose = 12;
        let hz = measured_hz(p);
        assert!((hz - 659.3).abs() < 12.0, "measured {hz} Hz");
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

    /// The rms of a crash hit as an eighth note in each of its windows: the first eighth (0.25 s at
    /// 120 bpm) and the following quarter second.
    fn crash_rms(midi: u8, sampled: bool) -> (f32, f32) {
        let mut s = Score::new();
        s.tracks = vec![guitario_score::Track::drums("Drums", &s.headers)];
        s.tracks[0].measures[0].voices[0].beats = vec![
            Beat { rest: false, notes: vec![Note::new(0, midi)], ..Beat::rest(Duration::new(guitario_theory::NoteValue::Eighth)) },
            Beat::rest(Duration::new(guitario_theory::NoteValue::Eighth)),
            Beat::rest(Duration::new(guitario_theory::NoteValue::Half)),
            Beat::rest(Duration::QUARTER),
        ];
        assert!(s.validate().is_ok());
        let mut p = Player::new(&s, 44_100, Options::default());
        if sampled {
            p.set_instrument(0, Some(test_instrument("<region> sample=lo.wav key=49 loop_mode=one_shot", 44_100)));
        }
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 44_100 / 4 * 2];
        p.render(&mut buf, 2);
        // Skip the first 20 ms after the note's end, while the cut fades.
        let mut after = vec![0.0f32; 44_100 / 4 * 2];
        p.render(&mut after, 2);
        (rms(&buf), rms(after.get(1764..).unwrap_or_default()))
    }

    #[test]
    fn a_choked_cymbal_stops_at_the_end_of_its_note() {
        for sampled in [false, true] {
            let (hit, rings) = crash_rms(49, sampled);
            let (choked, cut) = crash_rms(97, sampled);
            assert!(hit > 0.01 && (choked - hit).abs() < hit * 0.2, "same attack: {hit} {choked} (sampled {sampled})");
            assert!(rings > hit * 0.2, "a plain crash rings on: {rings} (sampled {sampled})");
            // What is left is the kit's room.
            assert!(cut < rings * 0.25, "the choke cuts it: {cut} vs {rings} (sampled {sampled})");
        }
        // The voice itself is silent within 20 ms of the cut.
        let mut v = crate::synth::DrumVoice::new(0, 97, 1.0, 44_100.0);
        v.cut_after(0.1);
        let mut n = crate::synth::Noise::new(3);
        let out: Vec<f32> = (0..44_100 / 2).map(|_| v.tick(&mut n)).collect();
        assert!(rms(out.get(..4410).unwrap_or_default()) > 0.02);
        assert!(rms(out.get(44_100 * 12 / 100..).unwrap_or_default()) < 1e-3);
        // Sampled, a choke plays the crash's sample.
        let mut s = Score::new();
        s.tracks = vec![guitario_score::Track::drums("Drums", &s.headers)];
        s.tracks[0].measures[0].voices[0].beats[0] = Beat { rest: false, notes: vec![Note::new(0, 97), Note::new(2, 92)], ..Beat::rest(Duration::QUARTER) };
        assert_eq!(used_keys(&s, 0), vec![46, 49]);
    }

    fn test_instrument(sfz: &str, rate: u32) -> Arc<Instrument> {
        let tone = |hz: f32| {
            let left: Vec<f32> = (0..rate as usize * 3).map(|n| (n as f32 / rate as f32 * hz * std::f32::consts::TAU).sin() * 0.8).collect();
            crate::sfz::Sample::from_f32(rate, &left, None)
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
    fn sampled_track_follows_the_relative_tonality() {
        // The written key (open high E) picks the sample; five semitones up it sounds A4.
        let inst = test_instrument("<region> sample=hi.wav lokey=0 hikey=127 pitch_keycenter=69 ampeg_release=0.05", 44_100);
        let mut p = Player::new(&one_note(0), 44_100, Options { transpose: 5, ..Options::default() });
        p.set_instrument(0, Some(inst));
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 44_100];
        p.render(&mut buf, 1);
        let seg = buf.get(8_000..16_000).unwrap_or_default();
        let ac = |lag: usize| seg.iter().zip(seg.iter().skip(lag)).map(|(a, b)| a * b).sum::<f32>();
        let best = (60..600).max_by(|a, b| ac(*a).total_cmp(&ac(*b))).unwrap();
        let hz = 44_100.0 / best as f32;
        assert!((hz - 440.0).abs() < 8.0, "measured {hz} Hz");
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
    fn master_automation_plays_over_the_master_level() {
        // Two bars of a held note at 240 bpm (a bar is a second): the master silent in the first,
        // then back to unity and panned hard left.
        let mut s = Score::new();
        s.tempo = 240;
        let h = s.headers[0].clone();
        s.headers.push(h.clone());
        s.tracks[0].measures.push(guitario_score::Measure::empty_for(&h));
        for m in &mut s.tracks[0].measures {
            m.voices[0].beats = vec![Beat { rest: false, notes: vec![Note::new(0, 5)], ..Beat::rest(Duration::WHOLE) }];
        }
        s.headers[0].master.volume = Some(0);
        s.headers[1].master.volume = Some(100);
        s.headers[1].master.pan = Some(-63);
        let mut p = Player::new(&s, 22_050, Options::default());
        p.play_from(0.0);
        let mut channel_rms = |secs: f32| {
            let mut buf = vec![0f32; (22_050.0 * secs) as usize * 2];
            p.render(&mut buf, 2);
            let ch = |c: usize| (buf.iter().skip(c).step_by(2).map(|x| x * x).sum::<f32>() / (buf.len() / 2) as f32).sqrt();
            (ch(0), ch(1))
        };
        let first = channel_rms(0.95);
        channel_rms(0.1);
        let second = channel_rms(0.5);
        assert!(first.0 < 1e-4 && first.1 < 1e-4, "silent by the master in bar 1: {first:?}");
        assert!(second.0 > 0.01 && second.1 < 0.01 * second.0, "full and hard left in bar 2: {second:?}");
        // Starting in bar 2 starts at its values; changing the master's own level keeps the points.
        let mut q = Player::new(&s, 22_050, Options::default());
        q.play_from(1.5);
        assert!(q.master_gain.1 < 1e-6 && q.master_gain.0 > 0.9, "{:?}", q.master_gain);
        let mut m = s.master.clone();
        m.volume = 50;
        q.set_master(&m);
        assert!(q.master_target.0 > 0.9, "the automation point still rules: {:?}", q.master_target);
    }

    #[test]
    fn not_stringed_a_restruck_string_lets_the_old_note_ring_on() {
        // Two let-ring notes on the low E string, a beat apart.
        let q = Duration::QUARTER;
        let note = |fret: u8| {
            let mut n = Note::new(5, fret);
            n.effects.let_ring = true;
            Beat { rest: false, notes: vec![n], ..Beat::rest(q) }
        };
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![note(0), note(3), Beat::rest(q), Beat::rest(q)];
        let sounding = |stringed: bool| {
            let mut s = s.clone();
            s.tracks[0].interpretation.stringed = stringed;
            let mut p = Player::new(&s, 22_050, Options::default());
            p.play_from(0.0);
            let mut buf = vec![0f32; (0.8 * 22_050.0) as usize * 2];
            p.render(&mut buf, 2);
            p.voices.iter().filter(|v| v.string == 5 && !v.released).count()
        };
        assert_eq!(sounding(true), 1, "a string sounds one note at a time");
        assert_eq!(sounding(false), 2, "as on a piano, both ring");
    }

    #[test]
    fn a_heavier_palm_mute_is_quieter_after_the_attack() {
        let mut n = Note::new(5, 0);
        n.effects.palm_mute = true;
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![Beat { rest: false, notes: vec![n], ..Beat::rest(Duration::WHOLE) }];
        let tail = |weight: u8| {
            let mut s = s.clone();
            s.tracks[0].interpretation.palm_mute = weight;
            let mut p = Player::new(&s, 22_050, Options::default());
            p.play_from(0.0);
            let mut buf = vec![0f32; 22_050 * 2];
            p.render(&mut buf, 2);
            rms(buf.get(22_050 / 4 * 2..).unwrap_or_default())
        };
        let (light, heavy) = (tail(0), tail(100));
        assert!(heavy < light * 0.8, "light {light}, heavy {heavy}");
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

    /// The pitch of `x` at `sr`: the shortest lag whose autocorrelation peaks within 10 % of the
    /// strongest between 40 Hz and 2.4 kHz, so a harmonic is not taken for the string under it.
    fn pitch_hz(x: &[f32], sr: f32) -> f32 {
        let ac = |lag: usize| x.iter().zip(x.iter().skip(lag)).map(|(a, b)| a * b).sum::<f32>();
        let (lo, hi) = ((sr / 2400.0) as usize, (sr / 40.0) as usize);
        let acs: Vec<f32> = (lo..hi).map(ac).collect();
        let max = acs.iter().copied().fold(0.0, f32::max);
        let peak = (1..acs.len() - 1).find(|&i| acs[i] >= 0.9 * max && acs[i] >= acs[i - 1] && acs[i] >= acs[i + 1]).unwrap_or(0);
        sr / (lo + peak) as f32
    }

    /// One whole note on `string` at `fret`, shaped by `f`.
    fn note_score(string: u8, fret: u8, f: impl Fn(&mut Note)) -> Score {
        let mut n = Note::new(string, fret);
        f(&mut n);
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![Beat { rest: false, notes: vec![n], ..Beat::rest(Duration::WHOLE) }];
        s
    }

    /// `secs` of `score` played from the start at 48 kHz, mono.
    fn play(score: &Score, secs: f32) -> (Player, Vec<f32>) {
        let mut p = Player::new(score, 48_000, Options::default());
        p.play_from(0.0);
        let mut buf = vec![0.0f32; (48_000.0 * secs) as usize];
        p.render(&mut buf, 1);
        (p, buf)
    }

    #[test]
    fn harmonics_sound_their_partial() {
        use guitario_score::Harmonic;
        let heard = |fret: u8, h: Harmonic| pitch_hz(&play(&note_score(0, fret, |n| n.effects.harmonic = Some(h)), 0.3).1[4800..12_000], 48_000.0);
        // Over the open high E (329.6 Hz): the 12th fret's octave, the 7th's octave and a fifth.
        for (fret, want) in [(12, 659.3), (7, 988.9), (5, 1318.5)] {
            let hz = heard(fret, Harmonic::Natural);
            assert!((hz / want - 1.0).abs() < 0.012, "natural at {fret}: {hz} Hz, want {want}");
        }
        // An artificial harmonic on A (440 Hz, the 5th fret) sounds its octave; a natural one at
        // a fret without a node sounds the plain note.
        let hz = heard(5, Harmonic::Artificial);
        assert!((hz / 880.0 - 1.0).abs() < 0.012, "artificial: {hz} Hz");
        let hz = heard(1, Harmonic::Natural);
        assert!((hz / 349.2 - 1.0).abs() < 0.012, "no node: {hz} Hz");
        // A harmonic rings like a bell: longer than the fretted note at the same pitch.
        let tail = |s: &Score| {
            let buf = play(s, 1.25).1;
            rms(&buf[48_000..60_000]) / rms(&buf[1000..13_000])
        };
        let (bell, fretted) = (tail(&note_score(0, 12, |n| n.effects.harmonic = Some(Harmonic::Natural))), tail(&note_score(0, 12, |_| {})));
        assert!(bell > 3.0 * fretted, "harmonic {bell}, fretted {fretted}");
    }

    #[test]
    fn palm_mutes_and_dead_notes_stop_soon_and_keep_their_pitch() {
        let plain = play(&note_score(5, 0, |_| {}), 1.0).1;
        let muted = play(&note_score(5, 0, |n| n.effects.palm_mute = true), 1.0).1;
        let dead = play(&note_score(5, 0, |n| n.kind = guitario_score::NoteKind::Dead), 1.0).1;
        let late = |b: &[f32]| rms(&b[24_000..48_000]);
        assert!(late(&muted) < 0.15 * late(&plain), "a palm mute dies away: {} vs {}", late(&muted), late(&plain));
        assert!(late(&dead) < 1e-3 && rms(&dead[..2400]) > 0.01, "a dead note clicks and stops");
        // The palm's damping is taken out of the string's length: still a low E.
        let mut heavy = note_score(5, 0, |n| n.effects.palm_mute = true);
        heavy.tracks[0].interpretation.palm_mute = 100;
        let hz = pitch_hz(&play(&heavy, 0.2).1[1000..8000], 48_000.0);
        assert!((hz / 82.41 - 1.0).abs() < 0.012, "palm-muted low E at {hz} Hz");
        let hz = pitch_hz(&muted[1000..12_000], 48_000.0);
        assert!((hz / 82.41 - 1.0).abs() < 0.012, "palm-muted low E at {hz} Hz");
    }

    #[test]
    fn staccato_plays_half_the_note() {
        let tail = |s: &Score| rms(&play(s, 1.6).1[60_000..76_800]);
        let (long, short) = (tail(&note_score(0, 5, |_| {})), tail(&note_score(0, 5, |n| n.effects.staccato = true)));
        assert!(short < 0.05 * long, "the staccato note has stopped by 1.25 s of its 2: {short} vs {long}");
    }

    /// Two quarter notes on the high E string, the first shaped by `f`, then a half rest.
    fn pair(from: u8, to: u8, f: impl Fn(&mut Note)) -> Score {
        let mut first = Note::new(0, from);
        f(&mut first);
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![
            Beat { rest: false, notes: vec![first], ..Beat::rest(Duration::QUARTER) },
            Beat { rest: false, notes: vec![Note::new(0, to)], ..Beat::rest(Duration::QUARTER) },
            Beat::rest(Duration::new(guitario_theory::NoteValue::Half)),
        ];
        s
    }

    #[test]
    fn slides_glide_over_the_end_of_the_first_note_and_only_a_shift_slide_picks_again() {
        // A (440 Hz, fret 5) slides to B (493.9 Hz, fret 7); a beat is half a second.
        for (slide, picks) in [(Slide::Legato, 1usize), (Slide::Shift, 2)] {
            let s = pair(5, 7, |n| n.effects.slide = Some(slide));
            let mut p = Player::new(&s, 48_000, Options::default());
            p.play_from(0.0);
            let mut first = vec![0.0f32; 16_800];
            p.render(&mut first, 1);
            let hz = pitch_hz(&first[4800..14_400], 48_000.0);
            assert!((hz / 440.0 - 1.0).abs() < 0.012, "{slide:?}: the first note holds its pitch until the slide: {hz}");
            // 0.35 s to 0.5 s: the slide; by the end of the first note it is at B.
            let mut sliding = vec![0.0f32; 7_000];
            p.render(&mut sliding, 1);
            let bend = p.voices.iter().find(|v| !v.released).map(|v| v.bend_semis);
            assert!(bend.is_some_and(|b| b > 1.0), "{slide:?}: on the way up at 0.49 s: {bend:?}");
            // Just after B starts: a shift slide has plucked again (the old vibration still dying).
            let mut second = vec![0.0f32; 12_000];
            p.render(&mut second[..1000], 1);
            assert_eq!(p.voices.len(), picks, "{slide:?}: strings plucked");
            p.render(&mut second[1000..], 1);
            let hz = pitch_hz(&second[2400..12_000], 48_000.0);
            assert!((hz / 493.9 - 1.0).abs() < 0.012, "{slide:?}: then B: {hz}");
        }
        // Without a next note on the string, a shift slide does nothing.
        let s = note_score(0, 5, |n| n.effects.slide = Some(Slide::Shift));
        let (p, _) = play(&s, 1.9);
        assert!(p.voices.iter().all(|v| v.bend_semis == 0.0));
    }

    #[test]
    fn slides_into_and_out_of_a_note() {
        let start_bend = |slide: Slide| {
            let (p, _) = play(&note_score(0, 5, |n| n.effects.slide = Some(slide)), 0.01);
            p.voices.first().map(|v| v.bend_semis)
        };
        assert!(start_bend(Slide::IntoFromBelow).is_some_and(|b| b < -2.0 && b > -3.1));
        assert!(start_bend(Slide::IntoFromAbove).is_some_and(|b| b > 2.0 && b < 3.1));
        // Into from below reaches the note within the first 0.12 s.
        let (p, _) = play(&note_score(0, 5, |n| n.effects.slide = Some(Slide::IntoFromBelow)), 0.2);
        assert!(p.voices.first().is_some_and(|v| v.bend_semis == 0.0));
        // Out of the note: falling (or rising) and fading over its last 0.35 s, silent at its end.
        for (slide, sign) in [(Slide::OutDown, -1.0f32), (Slide::OutUp, 1.0)] {
            let (p, buf) = play(&note_score(0, 5, |n| n.effects.slide = Some(slide)), 1.9);
            assert!(p.voices.first().is_some_and(|v| v.bend_semis * sign > 2.0 && v.gain < 0.2), "{slide:?}");
            let (_, plain) = play(&note_score(0, 5, |_| {}), 1.9);
            assert!(rms(&buf[86_400..91_200]) < 0.5 * rms(&plain[86_400..91_200]), "{slide:?} fades");
        }
    }

    #[test]
    fn a_hammer_on_puts_energy_back_and_a_legato_slide_does_not() {
        // The second note's loudness after a hammer-on and after a legato slide from the same note.
        let second = |f: &dyn Fn(&mut Note)| rms(&play(&pair(5, 7, f), 1.0).1[26_400..48_000]);
        let (hammer, slide) = (second(&|n| n.effects.hammer = true), second(&|n| n.effects.slide = Some(Slide::Legato)));
        assert!(hammer > 1.1 * slide, "hammer {hammer}, slide {slide}");
        // A hammer-on onto a harmonic sounds the harmonic, not the string under it.
        let mut s = pair(5, 12, |n| n.effects.hammer = true);
        s.tracks[0].measures[0].voices[0].beats[1].notes[0].effects.harmonic = Some(guitario_score::Harmonic::Natural);
        let hz = pitch_hz(&play(&s, 0.62).1[26_400..29_760], 48_000.0);
        assert!((hz / 659.3 - 1.0).abs() < 0.012, "{hz} Hz");
        // A tap on a silent string plucks it without a pick, softer than a picked note.
        let tap = rms(&play(&note_score(0, 12, |n| n.effects.tapped = true), 0.5).1);
        let picked = rms(&play(&note_score(0, 12, |_| {}), 0.5).1);
        assert!(tap > 0.1 * picked && tap < picked, "tap {tap}, picked {picked}");
    }

    #[test]
    fn sampled_legato_moves_the_sounding_sample_unless_the_bank_has_legato_samples() {
        let sfz = "<region> sample=hi.wav lokey=0 hikey=127 pitch_keycenter=69 ampeg_release=0.05";
        let s = pair(5, 7, |n| n.effects.hammer = true);
        let mut p = Player::new(&s, 48_000, Options::default());
        p.set_instrument(0, Some(test_instrument(sfz, 48_000)));
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 36_000];
        p.render(&mut buf, 1);
        assert_eq!(p.sampled.len(), 1, "one sample carries on");
        let hz = pitch_hz(&buf[26_400..36_000], 48_000.0);
        assert!((hz / 493.9 - 1.0).abs() < 0.012, "at B: {hz}");
        // With a legato program, the second note starts a sample of its own.
        let mut p = Player::new(&s, 48_000, Options::default());
        p.set_instrument(0, Some(test_instrument(sfz, 48_000)));
        p.set_articulation(0, Articulation::Legato, Some(test_instrument(sfz, 48_000)));
        p.play_from(0.0);
        p.render(&mut buf, 1);
        assert_eq!(p.sampled.iter().filter(|v| !v.released).count(), 1);
        assert!(p.sampled.iter().any(|v| v.note_start == 0.5 && v.pitch == 71.0));
    }

    #[test]
    fn sampled_articulations_play_from_their_own_programs() {
        // The sustain program is a 440 Hz recording, the palm-mute one a 110 Hz recording, both at A4.
        let sustain = test_instrument("<region> sample=hi.wav lokey=0 hikey=127 pitch_keycenter=69", 48_000);
        let muted = test_instrument("<region> sample=lo.wav lokey=0 hikey=127 pitch_keycenter=69", 48_000);
        let s = note_score(0, 5, |n| n.effects.palm_mute = true);
        let mut p = Player::new(&s, 48_000, Options::default());
        p.set_instrument(0, Some(sustain));
        p.set_articulation(0, Articulation::PalmMute, Some(muted));
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 24_000];
        p.render(&mut buf, 1);
        let hz = pitch_hz(&buf[4800..14_400], 48_000.0);
        assert!((hz / 110.0 - 1.0).abs() < 0.012, "from the palm-mute program: {hz}");
        assert!(rms(&buf[14_400..24_000]) > 0.05, "a recorded palm mute is not cut short again");
        // A semi-harmonic sounds its partial and the plain note under it.
        let s = note_score(0, 5, |n| n.effects.harmonic = Some(guitario_score::Harmonic::Semi));
        let mut p = Player::new(&s, 48_000, Options::default());
        p.set_instrument(0, Some(test_instrument("<region> sample=hi.wav lokey=0 hikey=127 pitch_keycenter=69", 48_000)));
        p.play_from(0.0);
        p.render(&mut buf[..100], 1);
        let mut pitches: Vec<f32> = p.sampled.iter().map(|v| v.pitch).collect();
        pitches.sort_by(f32::total_cmp);
        assert_eq!(pitches, vec![69.0, 93.0]);
        assert_eq!(used_keys(&s, 0), vec![69, 93]);
    }

    #[test]
    fn release_samples_start_when_a_note_ends_and_fade_with_how_long_it_was_held() {
        // A 440 Hz attack that stops at once when let go, and a 110 Hz release noise losing 20 dB
        // a second the note was held.
        let sfz = "<region> sample=hi.wav lokey=0 hikey=127 pitch_keycenter=69 ampeg_release=0.005\n<region> sample=lo.wav lokey=0 hikey=127 pitch_keycenter=69 trigger=release rt_decay=20";
        let noise_after = |value: guitario_theory::NoteValue, ring: bool| {
            let mut s = note_score(0, 5, |n| n.effects.let_ring = ring);
            s.tracks[0].measures[0].voices[0].beats[0].duration = Duration::new(value);
            let mut p = Player::new(&s, 48_000, Options::default());
            p.set_instrument(0, Some(test_instrument(sfz, 48_000)));
            p.play_from(0.0);
            let note = 2.0 / f64::from(value.divisor());
            let mut before = vec![0.0f32; (48_000.0 * note) as usize];
            p.render(&mut before, 1);
            let mut after = vec![0.0f32; 9600];
            p.render(&mut after, 1);
            let noises = p.sampled.iter().filter(|v| v.string == NO_STRING).count();
            (noises, rms(&after[4800..]), pitch_hz(&after[2400..], 48_000.0))
        };
        let (n, quarter, hz) = noise_after(guitario_theory::NoteValue::Quarter, false);
        assert!(n == 1 && (hz / 110.0 - 1.0).abs() < 0.02, "one release noise at the note's key (A4, the sample's root): {n}, {hz} Hz");
        let (_, whole, _) = noise_after(guitario_theory::NoteValue::Whole, false);
        assert!(whole < 0.1 * quarter, "held 2 s instead of 0.5 s: 30 dB quieter ({whole} vs {quarter})");
        assert_eq!(noise_after(guitario_theory::NoteValue::Quarter, true).0, 0, "a let-ring note rings on");
        // Stop forgets the noises still to come.
        let s = note_score(0, 5, |_| {});
        let mut p = Player::new(&s, 48_000, Options::default());
        p.set_instrument(0, Some(test_instrument(sfz, 48_000)));
        p.play_from(0.0);
        p.render(&mut [0.0f32; 480], 1);
        assert_eq!(p.releases.len(), 1);
        p.stop();
        assert!(p.releases.is_empty());
    }

    #[test]
    fn a_library_s_own_legato_samples_play_hammer_ons() {
        // The attack is a 440 Hz recording, the legato one 110 Hz, both rooted at A4.
        let sfz = "<region> sample=hi.wav lokey=0 hikey=127 pitch_keycenter=69\n<region> sample=lo.wav lokey=0 hikey=127 pitch_keycenter=69 trigger=legato";
        let s = pair(5, 7, |n| n.effects.hammer = true);
        let mut p = Player::new(&s, 48_000, Options::default());
        p.set_instrument(0, Some(test_instrument(sfz, 48_000)));
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 36_000];
        p.render(&mut buf, 1);
        let first = pitch_hz(&buf[4800..14_400], 48_000.0);
        let second = pitch_hz(&buf[26_400..36_000], 48_000.0);
        assert!((first / 440.0 - 1.0).abs() < 0.012, "the first note from the attack sample: {first}");
        assert!((second / 123.5 - 1.0).abs() < 0.012, "the hammer-on from the legato sample: {second}");
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

    /// Renders the whole song at 48 kHz, stereo, with its tails.
    fn render_all(score: &Score) -> Vec<f32> {
        let mut p = Player::new(score, 48_000, Options::default());
        crate::render_player(&mut p)
    }

    #[test]
    fn the_overdriven_programs_play_through_an_amp_and_a_chain_of_their_own_replaces_it() {
        // How much of its attack a note keeps after a second: an amp's clipping holds it up.
        let bright = |score: &Score| {
            let mut p = Player::new(score, 48_000, Options::default());
            p.play_from(0.0);
            let mut buf = vec![0.0f32; 72_000];
            p.render(&mut buf, 1);
            rms(&buf[48_000..]) / rms(&buf[..4800])
        };
        let mut clean = one_note(3);
        clean.tracks[0].program = 27;
        let mut dist = clean.clone();
        dist.tracks[0].program = 30;
        assert!(bright(&dist) > 1.5 * bright(&clean), "{} vs {}", bright(&dist), bright(&clean));
        // An empty chain of its own: the same program plays clean.
        dist.tracks[0].effects = Some(Vec::new());
        assert!((bright(&dist) - bright(&clean)).abs() < 0.2 * bright(&clean));
    }

    #[test]
    fn a_reverb_rings_on_after_the_song_and_the_export_keeps_its_tail() {
        use guitario_score::{Effect, EffectType};
        let mut s = one_note(5);
        s.tracks[0].measures[0].voices[0].beats[0].duration = Duration::EIGHTH;
        let dry = render_all(&s);
        s.tracks[0].effects = Some(vec![Effect::preset(EffectType::Reverb, "Hall").unwrap()]);
        let wet = render_all(&s);
        assert!(wet.len() > dry.len(), "{} vs {}", wet.len(), dry.len());
        // The master's reverb is off by default; on, it adds a tail to every track.
        let mut m = one_note(5);
        m.tracks[0].measures[0].voices[0].beats[0].duration = Duration::EIGHTH;
        m.master.reverb = Effect::preset(EffectType::Reverb, "Hall").unwrap();
        assert!(render_all(&m).len() > dry.len());
        assert!(wet.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn the_master_volume_and_pan_follow_the_track_mix_and_glide_while_playing() {
        let level = |p: &mut Player| {
            let mut buf = vec![0.0f32; 9600];
            p.render(&mut buf, 2);
            let l: Vec<f32> = buf.iter().step_by(2).copied().collect();
            let r: Vec<f32> = buf.iter().skip(1).step_by(2).copied().collect();
            (rms(&l), rms(&r))
        };
        let s = one_note(5);
        let mut p = Player::new(&s, 48_000, Options::default());
        p.play_from(0.0);
        let (l0, r0) = level(&mut p);
        let mut m = s.master.clone();
        m.pan = 63;
        p.set_master(&m);
        level(&mut p);
        let (l1, r1) = level(&mut p);
        assert!((l0 - r0).abs() < 1e-6 && l1 < 0.001 * r1, "{l1} {r1} after {l0} {r0}");
        m.volume = 0;
        p.set_master(&m);
        level(&mut p);
        assert!(level(&mut p).1 < 1e-4);
    }

    #[test]
    fn effects_and_the_equalizer_change_while_playing() {
        use guitario_score::{Effect, EffectType};
        let s = one_note(5);
        let mut p = Player::new(&s, 48_000, Options::default());
        p.play_from(0.0);
        let mut buf = vec![0.0f32; 4800];
        p.render(&mut buf, 2);
        let before = rms(&buf);
        let mut sound: Vec<TrackSound> = s.tracks.iter().map(TrackSound::of).collect();
        sound[0].eq.set("low", -12.0).unwrap();
        sound[0].eq.set("lowMid", -12.0).unwrap();
        sound[0].eq.set("mid", -12.0).unwrap();
        sound[0].eq.set("highMid", -12.0).unwrap();
        sound[0].eq.set("high", -12.0).unwrap();
        p.set_sound(&sound);
        p.render(&mut buf, 2);
        assert!(rms(&buf) < 0.5 * before, "{} vs {before}", rms(&buf));
        sound[0].eq.on = false;
        sound[0].effects = Some(vec![Effect::new(EffectType::Tremolo)]);
        p.set_sound(&sound);
        p.render(&mut buf, 2);
        assert!(buf.iter().all(|x| x.is_finite()));
        // The wrong number of tracks is ignored.
        p.set_sound(&[]);
    }

    fn on_instrument(program: u8, tuning: guitario_theory::Tuning, beats: Vec<Beat>) -> Score {
        let mut s = Score::new();
        s.tracks[0].program = program;
        s.tracks[0].tuning = tuning;
        s.tracks[0].measures[0].voices[0].beats = beats;
        s
    }

    fn play_for(s: &Score, seconds: f32) -> (Player, Vec<f32>) {
        let mut p = Player::new(s, 44_100, Options::default());
        p.play_from(0.0);
        let mut out = vec![0.0; (44_100.0 * seconds) as usize];
        p.render(&mut out, 1);
        (p, out)
    }

    #[test]
    fn a_piano_track_plays_the_modelled_piano_and_a_key_struck_again_restrikes() {
        let q = Duration::QUARTER;
        let note = |string, fret| Beat { rest: false, notes: vec![Note::new(string, fret)], ..Beat::rest(q) };
        let s = on_instrument(0, guitario_theory::Tuning::standard(), vec![note(0, 0), note(0, 0), note(1, 0), Beat::rest(q)]);
        let (p, out) = play_for(&s, 0.9);
        assert!(p.voices.is_empty() && p.bowed.is_empty());
        assert_eq!(p.pianos.len(), 1, "one key, struck twice");
        assert!(rms(&out) > 0.01 && out.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn a_slur_is_one_bow_stroke_and_the_next_note_changes_bow() {
        let q = Duration::QUARTER;
        let note = |fret| Beat { rest: false, notes: vec![Note::new(1, fret)], ..Beat::rest(q) };
        let s = on_instrument(40, guitario_theory::Tuning::violin(), vec![Beat { slur: Some(1), ..note(0) }, note(2), note(4), Beat::rest(q)]);
        let mut p = Player::new(&s, 44_100, Options::default());
        p.play_from(0.0);
        let mut directions = Vec::new();
        for _ in 0..3 {
            let mut out = vec![0.0; 22_050];
            p.render(&mut out, 1);
            directions.push(p.bow_dir[0][0]);
            assert_eq!(p.bowed.len(), 1, "one string, one voice");
        }
        assert_eq!(directions[0], directions[1], "the slurred note keeps the stroke");
        assert_eq!(directions[2], -directions[1], "the next note is a bow change");
    }

    #[test]
    fn a_string_section_plays_each_note_with_several_players_and_pizzicato_through_a_body() {
        let note = Beat { rest: false, notes: vec![Note::new(0, 0), Note::new(3, 0)], ..Beat::rest(Duration::QUARTER) };
        let (p, out) = play_for(&on_instrument(48, guitario_theory::Tuning::violin(), vec![note.clone()]), 0.3);
        assert_eq!(p.bowed.len(), 8, "two notes, four players each");
        assert!(rms(&out) > 0.005);
        let (p, out) = play_for(&on_instrument(45, guitario_theory::Tuning::violin(), vec![note]), 0.3);
        assert!(p.bowed.is_empty() && p.voices.len() == 6 && p.voices.iter().all(|v| v.slot.is_some()));
        assert!(rms(&out) > 0.005);
    }

    #[test]
    fn a_string_section_sounds_wide_and_a_soloist_narrower() {
        // Recorded violin sections are as good as uncorrelated between left and right (0.05); a
        // violin through its one body much less so. Dry, without the hall strings default to, once
        // the section has swelled in.
        let correlation = |program: u8| {
            let notes = (0..4).map(|string| Note::new(string, 3)).collect();
            let note = Beat { rest: false, notes, ..Beat::rest(Duration::WHOLE) };
            let mut s = on_instrument(program, guitario_theory::Tuning::violin(), vec![note]);
            s.tracks[0].effects = Some(Vec::new());
            let mut p = Player::new(&s, 44_100, Options::default());
            p.play_from(0.0);
            let mut out = vec![0.0; 44_100 * 19 / 10 * 2];
            p.render(&mut out, 2);
            let (mut lr, mut ll, mut rr) = (0.0f64, 0.0f64, 0.0f64);
            for f in out.chunks(2).skip(44_100) {
                lr += f64::from(f[0] * f[1]);
                ll += f64::from(f[0] * f[0]);
                rr += f64::from(f[1] * f[1]);
            }
            lr / (ll * rr).sqrt().max(1e-12)
        };
        let (section, solo) = (correlation(48), correlation(40));
        println!("left/right correlation: section {section:.2}, solo {solo:.2}");
        assert!(section.abs() < 0.4 && solo > section + 0.2, "section {section}, solo {solo}");
    }

    #[test]
    fn modelled_instruments_survive_hostile_scores_at_any_rate() {
        use guitario_score::{Bend, Harmonic, Slide};
        let e = Duration::EIGHTH;
        let mut beats = Vec::new();
        for (i, fret) in [0u8, 36, 12, 36, 0, 7, 24, 36].into_iter().enumerate() {
            let mut n = Note::new((i % 6) as u8, fret);
            match i {
                1 => n.effects.bend = Some(Bend::to(8)),
                2 => n.effects.harmonic = Some(Harmonic::Natural),
                3 => n.effects.slide = Some(Slide::OutUp),
                4 => n.kind = guitario_score::NoteKind::Dead,
                5 => n.effects.let_ring = true,
                6 => n.effects.wide_vibrato = true,
                _ => n.effects.staccato = true,
            }
            beats.push(Beat {
                rest: false,
                notes: vec![n, Note::new(((i + 3) % 6) as u8, 36)],
                slur: Some(2),
                tremolo: (i == 7).then_some(guitario_score::TremoloSpeed::ThirtySecond),
                ..Beat::rest(e)
            });
        }
        let tuning = guitario_theory::Tuning { name: "wide".into(), strings: vec![100, 80, 60, 40, 20, 0] };
        for program in [0u8, 1, 3, 40, 41, 42, 43, 44, 45, 48, 49, 110] {
            let s = on_instrument(program, tuning.clone(), beats.clone());
            for rate in [8_000u32, 192_000] {
                let mut p = Player::new(&s, rate, Options { transpose: 12, ..Options::default() });
                p.play_from(0.0);
                let mut out = vec![0.0; rate as usize * 4];
                p.render(&mut out, 2);
                assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 1.0), "program {program} at {rate}");
            }
        }
    }

    #[test]
    fn the_output_is_linear_until_it_nears_full_scale() {
        for x in [0.0f32, 0.1, -0.5, 0.75, -0.75] {
            assert_eq!(soft_limit(x), x);
        }
        let mut last = 0.0;
        for i in 0..2000 {
            let y = soft_limit(i as f32 * 0.005);
            assert!(y >= last && y <= 1.0, "monotonic and never past 1");
            last = y;
        }
        assert!((soft_limit(0.7501) - 0.7501).abs() < 1e-4, "continuous at the knee");
        assert_eq!(soft_limit(f32::NAN), 0.0);
        assert!(soft_limit(-50.0) >= -1.0);
    }
}
