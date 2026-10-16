use guitario_theory::{Duration, TimeSignature, Tuning};
use serde::{Deserialize, Serialize};

pub const DEFAULT_TEMPO: u16 = 120;
/// Voices a bar may hold, as in Guitar Pro.
pub const MAX_VOICES: usize = 4;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub meta: Meta,
    /// Tempo in quarter notes per minute at the start. A header's `tempo` overrides it from there.
    pub tempo: u16,
    pub headers: Vec<MeasureHeader>,
    pub tracks: Vec<Track>,
    /// Paper, orientation and margins for the page view, PDF and print.
    #[serde(default)]
    pub page: PageSetup,
}

/// A paper size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Paper {
    #[default]
    A4,
    A5,
    Letter,
    Legal,
}

impl Paper {
    pub const ALL: [Paper; 4] = [Paper::A4, Paper::A5, Paper::Letter, Paper::Legal];

    /// Width and height upright, in millimetres.
    pub fn size_mm(self) -> (f32, f32) {
        match self {
            Paper::A4 => (210.0, 297.0),
            Paper::A5 => (148.0, 210.0),
            Paper::Letter => (215.9, 279.4),
            Paper::Legal => (215.9, 355.6),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Paper::A4 => "A4",
            Paper::A5 => "A5",
            Paper::Letter => "Letter",
            Paper::Legal => "Legal",
        }
    }

    /// The paper called `name`, in any case.
    pub fn from_name(name: &str) -> Option<Paper> {
        Paper::ALL.into_iter().find(|p| p.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// How pages are laid out: the paper, its orientation and the margins in millimetres. The defaults
/// are Guitar Pro's stylesheet: A4 upright, 10 mm at the sides, 15 mm above, 12 mm below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PageSetup {
    pub paper: Paper,
    pub landscape: bool,
    pub margin_left: u8,
    pub margin_right: u8,
    pub margin_top: u8,
    pub margin_bottom: u8,
}

impl Default for PageSetup {
    fn default() -> Self {
        PageSetup { paper: Paper::A4, landscape: false, margin_left: 10, margin_right: 10, margin_top: 15, margin_bottom: 12 }
    }
}

impl PageSetup {
    /// No margin is wider than this, in millimetres.
    pub const MAX_MARGIN: u8 = 60;
    /// What is left of the page inside the margins is at least this wide and this tall, in millimetres.
    pub const MIN_PRINTABLE: f32 = 100.0;

    /// The page's width and height as it lies, in millimetres.
    pub fn size_mm(&self) -> (f32, f32) {
        let (w, h) = self.paper.size_mm();
        if self.landscape { (h, w) } else { (w, h) }
    }

    /// Why this setup cannot be used, if it cannot.
    pub fn check(&self) -> Result<(), String> {
        let margins = [("left", self.margin_left), ("right", self.margin_right), ("top", self.margin_top), ("bottom", self.margin_bottom)];
        if let Some((side, v)) = margins.into_iter().find(|(_, v)| *v > Self::MAX_MARGIN) {
            return Err(format!("the {side} margin is {v} mm; at most {} mm", Self::MAX_MARGIN));
        }
        let (w, h) = self.size_mm();
        let printable_w = w - f32::from(self.margin_left) - f32::from(self.margin_right);
        let printable_h = h - f32::from(self.margin_top) - f32::from(self.margin_bottom);
        if printable_w < Self::MIN_PRINTABLE || printable_h < Self::MIN_PRINTABLE {
            return Err(format!(
                "the margins leave {printable_w:.0} x {printable_h:.0} mm of {} paper; at least {:.0} x {:.0} mm is needed",
                self.paper.name(),
                Self::MIN_PRINTABLE,
                Self::MIN_PRINTABLE
            ));
        }
        Ok(())
    }
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
    /// A double bar line closes this measure.
    #[serde(default)]
    pub double_bar: bool,
    /// Free time: no time signature is shown and the metronome is silent.
    #[serde(default)]
    pub free_time: bool,
    #[serde(default)]
    pub triplet_feel: TripletFeel,
    /// Alternate endings this bar belongs to, as a bit set: bit 0 is the first ending. 0 is none.
    #[serde(default)]
    pub endings: u8,
    /// Segno, coda and fine marks at this bar.
    #[serde(default)]
    pub targets: Vec<DirectionTarget>,
    /// A jump (D.C., D.S., To Coda …) taken at the end of this bar.
    #[serde(default)]
    pub jump: Option<Jump>,
}

/// A place a jump can go to, or Fine where a jump "al Fine" stops.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectionTarget {
    Segno,
    SegnoSegno,
    Coda,
    DoubleCoda,
    Fine,
}

/// Where a D.C. or D.S. jump ends: at Fine, or by taking the next To Coda.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JumpEnd {
    End,
    Fine,
    Coda,
    DoubleCoda,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Jump {
    /// Da Capo: back to the start.
    DaCapo(JumpEnd),
    /// Dal Segno: back to the segno.
    DalSegno(JumpEnd),
    /// Dal Segno Segno: back to the double segno.
    DalSegnoSegno(JumpEnd),
    /// To Coda: taken only after a jump "al Coda".
    ToCoda,
    ToDoubleCoda,
}

impl Jump {
    /// The text Guitar Pro prints for this jump.
    pub fn label(self) -> &'static str {
        match self {
            Jump::DaCapo(JumpEnd::End) => "D.C.",
            Jump::DaCapo(JumpEnd::Fine) => "D.C. al Fine",
            Jump::DaCapo(JumpEnd::Coda) => "D.C. al Coda",
            Jump::DaCapo(JumpEnd::DoubleCoda) => "D.C. al Double Coda",
            Jump::DalSegno(JumpEnd::End) => "D.S.",
            Jump::DalSegno(JumpEnd::Fine) => "D.S. al Fine",
            Jump::DalSegno(JumpEnd::Coda) => "D.S. al Coda",
            Jump::DalSegno(JumpEnd::DoubleCoda) => "D.S. al Double Coda",
            Jump::DalSegnoSegno(JumpEnd::End) => "D.S.S.",
            Jump::DalSegnoSegno(JumpEnd::Fine) => "D.S.S. al Fine",
            Jump::DalSegnoSegno(JumpEnd::Coda) => "D.S.S. al Coda",
            Jump::DalSegnoSegno(JumpEnd::DoubleCoda) => "D.S.S. al Double Coda",
            Jump::ToCoda => "To Coda",
            Jump::ToDoubleCoda => "To Double Coda",
        }
    }
}

/// Guitar Pro's triplet feels: how pairs of eighths or sixteenths are played.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TripletFeel {
    #[default]
    None,
    /// Pairs of eighths swing as a quarter-eighth triplet.
    Triplet8th,
    Triplet16th,
    /// Pairs played dotted eighth + sixteenth.
    Dotted8th,
    Dotted16th,
    /// Scotch snap: the short note first.
    Scottish8th,
    Scottish16th,
}

const EIGHTH: u32 = guitario_theory::TICKS_PER_QUARTER / 2;

impl TripletFeel {
    /// `(pair length in ticks, where the second note of a pair starts as a fraction of the pair)`.
    pub fn swing(self) -> Option<(u32, f64)> {
        let (unit, split) = match self {
            TripletFeel::None => return None,
            TripletFeel::Triplet8th => (EIGHTH, 2.0 / 3.0),
            TripletFeel::Triplet16th => (EIGHTH / 2, 2.0 / 3.0),
            TripletFeel::Dotted8th => (EIGHTH, 0.75),
            TripletFeel::Dotted16th => (EIGHTH / 2, 0.75),
            TripletFeel::Scottish8th => (EIGHTH, 0.25),
            TripletFeel::Scottish16th => (EIGHTH / 2, 0.25),
        };
        Some((unit * 2, split))
    }

    /// Moves a tick offset within a measure according to the feel.
    pub fn warp(self, tick: u64) -> u64 {
        let Some((pair, split)) = self.swing() else { return tick };
        if pair == 0 {
            return tick;
        }
        let pair = u64::from(pair);
        let (base, x) = (tick - tick % pair, (tick % pair) as f64);
        let half = pair as f64 / 2.0;
        let target = split * pair as f64;
        let y = if x < half { x * target / half } else { target + (x - half) * (pair as f64 - target) / half };
        base + y.round() as u64
    }
}

impl MeasureHeader {
    pub fn new(time_signature: TimeSignature) -> MeasureHeader {
        MeasureHeader {
            time_signature,
            key: 0,
            tempo: None,
            repeat_open: false,
            repeat_close: 0,
            marker: None,
            double_bar: false,
            free_time: false,
            triplet_feel: TripletFeel::None,
            endings: 0,
            targets: Vec::new(),
            jump: None,
        }
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
    /// Pick stroke direction (Effects › Pick Stroke Up / Down).
    #[serde(default)]
    pub stroke: Option<StrokeDirection>,
    /// Slap or pop on a bass beat.
    #[serde(default)]
    pub technique: Option<BeatTechnique>,
    /// Fade in or volume swell over the beat.
    #[serde(default)]
    pub fade: Option<Fade>,
    /// Brush (strum) or arpeggio across the strings of this beat.
    #[serde(default)]
    pub brush: Option<Brush>,
    /// A chord name (and diagram) shown above the beat.
    #[serde(default)]
    pub chord: Option<Chord>,
}

/// A chord symbol, with the fingering diagram Guitar Pro draws above the staff when it has one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chord {
    pub name: String,
    /// Fret per string, highest string first; `None` is a string not played. Empty without a diagram.
    #[serde(default)]
    pub frets: Vec<Option<u8>>,
    /// The fret the diagram starts at (1 is the nut).
    #[serde(default = "first_fret")]
    pub first_fret: u8,
}

fn first_fret() -> u8 {
    1
}

impl Chord {
    pub fn named(name: impl Into<String>) -> Chord {
        Chord { name: name.into(), frets: Vec::new(), first_fret: 1 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeatTechnique {
    Slap,
    Pop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fade {
    In,
    Out,
    Swell,
}

/// A strum or arpeggio: the strings sound one after another instead of together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Brush {
    pub kind: BrushKind,
    /// `true` strums from the lowest string up (a downstroke on a guitar).
    pub down: bool,
    /// Time from the first string to the last, in ticks (`TICKS_PER_QUARTER` per quarter).
    pub ticks: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrushKind {
    Brush,
    Arpeggio,
    Rasgueado,
}

impl Beat {
    pub fn rest(duration: Duration) -> Beat {
        Beat {
            duration,
            rest: true,
            notes: Vec::new(),
            dynamic: Dynamic::default(),
            text: None,
            stroke: None,
            technique: None,
            fade: None,
            brush: None,
            chord: None,
        }
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
    #[serde(default)]
    pub heavy_accent: bool,
    #[serde(default)]
    pub wide_vibrato: bool,
    /// Right-hand tapping.
    #[serde(default)]
    pub tapped: bool,
    #[serde(default)]
    pub left_hand_tapped: bool,
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
        Score { meta: Meta::default(), tempo: DEFAULT_TEMPO, headers, tracks, page: PageSetup::default() }
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
