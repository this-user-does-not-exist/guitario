use guitario_theory::{Duration, TimeSignature, Tuning};
use serde::{Deserialize, Serialize};

pub const DEFAULT_TEMPO: u16 = 120;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub meta: Meta,
    /// Tempo in quarter notes per minute at the start. A header's `tempo` overrides it from there.
    pub tempo: u16,
    pub headers: Vec<MeasureHeader>,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub author: String,
    pub notes: String,
}

/// The master bar: everything shared by all tracks at one measure position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeasureHeader {
    pub time_signature: TimeSignature,
    /// Key signature as sharps (positive) or flats (negative), -7..=7.
    #[serde(default)]
    pub key: i8,
    #[serde(default)]
    pub tempo: Option<u16>,
    #[serde(default)]
    pub repeat_open: bool,
    /// Extra passes: `1` plays the section twice. `0` is no repeat close.
    #[serde(default)]
    pub repeat_close: u8,
    #[serde(default)]
    pub marker: Option<String>,
}

impl MeasureHeader {
    pub fn new(time_signature: TimeSignature) -> MeasureHeader {
        MeasureHeader { time_signature, key: 0, tempo: None, repeat_open: false, repeat_close: 0, marker: None }
    }

    pub fn capacity(&self) -> u32 {
        self.time_signature.measure_ticks()
    }
}

/// What a track holds. Drum tracks use the kit lanes of [`Tuning::drum_kit`] and store each hit's
/// General MIDI drum number as the note's fret.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    #[default]
    Strings,
    Drums,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    #[serde(default)]
    pub kind: TrackKind,
    pub tuning: Tuning,
    #[serde(default)]
    pub capo: u8,
    /// General MIDI program, 0..=127. 25 is nylon guitar, 29 overdriven, 33 acoustic bass.
    pub program: u8,
    pub volume: u8,
    /// -63..=63, 0 is centre.
    #[serde(default)]
    pub pan: i8,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
    pub measures: Vec<Measure>,
}

impl Track {
    pub fn new(name: impl Into<String>, tuning: Tuning, headers: &[MeasureHeader]) -> Track {
        let program = if tuning.string_count() <= 4 && tuning.strings.first().is_some_and(|p| *p < 50) { 33 } else { 25 };
        Track {
            name: name.into(),
            kind: TrackKind::Strings,
            tuning,
            capo: 0,
            program,
            volume: 100,
            pan: 0,
            muted: false,
            solo: false,
            measures: headers.iter().map(Measure::empty_for).collect(),
        }
    }

    /// A drum-kit track: eight kit lanes, General MIDI drums, the GM percussion program.
    pub fn drums(name: impl Into<String>, headers: &[MeasureHeader]) -> Track {
        Track { kind: TrackKind::Drums, program: 0, ..Track::new(name, Tuning::drum_kit(), headers) }
    }

    pub fn string_count(&self) -> usize {
        self.tuning.string_count()
    }

    pub fn is_drums(&self) -> bool {
        self.kind == TrackKind::Drums
    }

    /// The highest fret (or drum number, on a drum track) a note may hold.
    pub fn max_fret(&self) -> u8 {
        if self.is_drums() { guitario_theory::drums::MAX_DRUM } else { 36 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    pub voices: Vec<Voice>,
}

impl Measure {
    /// A measure holding one voice with a single rest as long as the header allows.
    pub fn empty_for(header: &MeasureHeader) -> Measure {
        Measure { voices: vec![Voice { beats: vec![Beat::rest(rest_for(header.capacity()))] }] }
    }
}

/// The longest plain (undotted) duration that fits in `ticks`, or a dotted/whole one at best.
fn rest_for(ticks: u32) -> Duration {
    use guitario_theory::NoteValue;
    for value in NoteValue::ALL {
        let d = Duration::new(value);
        if d.ticks() == ticks {
            return d;
        }
        let dotted = d.dotted(1);
        if dotted.ticks() == ticks {
            return dotted;
        }
    }
    NoteValue::ALL.into_iter().map(Duration::new).find(|d| d.ticks() <= ticks).unwrap_or(Duration::WHOLE)
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Voice {
    pub beats: Vec<Beat>,
}

impl Voice {
    pub fn ticks(&self) -> u32 {
        self.beats.iter().map(|b| b.duration.ticks()).sum()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dynamic {
    Ppp,
    Pp,
    P,
    Mp,
    #[default]
    Mf,
    F,
    Ff,
    Fff,
}

impl Dynamic {
    /// MIDI velocity.
    pub fn velocity(self) -> u8 {
        match self {
            Dynamic::Ppp => 22,
            Dynamic::Pp => 36,
            Dynamic::P => 49,
            Dynamic::Mp => 64,
            Dynamic::Mf => 80,
            Dynamic::F => 96,
            Dynamic::Ff => 112,
            Dynamic::Fff => 126,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Beat {
    pub duration: Duration,
    /// A rest has no sounding notes. Kept explicit so an empty beat is never ambiguous.
    pub rest: bool,
    #[serde(default)]
    pub notes: Vec<Note>,
    #[serde(default)]
    pub dynamic: Dynamic,
    #[serde(default)]
    pub text: Option<String>,
}

impl Beat {
    pub fn rest(duration: Duration) -> Beat {
        Beat { duration, rest: true, notes: Vec::new(), dynamic: Dynamic::default(), text: None }
    }

    pub fn note_on(&self, string: u8) -> Option<&Note> {
        self.notes.iter().find(|n| n.string == string)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    #[default]
    Normal,
    /// Muted `x`: a percussive click without pitch.
    Dead,
    /// Tied to the previous note on the same string; does not re-attack.
    Tie,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slide {
    Shift,
    Legato,
    IntoFromBelow,
    IntoFromAbove,
    OutDown,
    OutUp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Harmonic {
    Natural,
    Artificial,
    Pinch,
    Tapped,
    Semi,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteEffects {
    /// Hammer-on or pull-off into the next note on this string.
    #[serde(default)]
    pub hammer: bool,
    #[serde(default)]
    pub slide: Option<Slide>,
    #[serde(default)]
    pub harmonic: Option<Harmonic>,
    /// Bend height in quarter tones (4 is a full step).
    #[serde(default)]
    pub bend: Option<u8>,
    #[serde(default)]
    pub vibrato: bool,
    #[serde(default)]
    pub palm_mute: bool,
    #[serde(default)]
    pub let_ring: bool,
    #[serde(default)]
    pub ghost: bool,
    #[serde(default)]
    pub accent: bool,
    #[serde(default)]
    pub staccato: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// 0 is the highest string.
    pub string: u8,
    pub fret: u8,
    #[serde(default)]
    pub kind: NoteKind,
    #[serde(default)]
    pub effects: NoteEffects,
}

impl Note {
    pub fn new(string: u8, fret: u8) -> Note {
        Note { string, fret, kind: NoteKind::Normal, effects: NoteEffects::default() }
    }
}

impl Score {
    /// One empty measure of 4/4 and one standard-tuned guitar track.
    pub fn new() -> Score {
        let headers = vec![MeasureHeader::new(TimeSignature::COMMON)];
        let tracks = vec![Track::new("Guitar", Tuning::standard(), &headers)];
        Score { meta: Meta::default(), tempo: DEFAULT_TEMPO, headers, tracks }
    }

    pub fn measure_count(&self) -> usize {
        self.headers.len()
    }

    /// The tempo in force at `measure`, following earlier overrides.
    pub fn tempo_at(&self, measure: usize) -> u16 {
        self.headers.iter().take(measure.saturating_add(1)).filter_map(|h| h.tempo).next_back().unwrap_or(self.tempo)
    }
}

impl Default for Score {
    fn default() -> Self {
        Score::new()
    }
}
