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

/// Song information, Guitar Pro's fields. Every field may be empty, and files from before a field
/// existed load with it empty.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub title: String,
    pub artist: String,
    pub subtitle: String,
    pub album: String,
    /// Who wrote the words, and the music. When both name the same people the title block says
    /// "Words & Music by".
    pub words: String,
    pub music: String,
    pub copyright: String,
    /// The transcriber (Guitar Pro's "Tabber").
    pub author: String,
    /// Guitar Pro's "Notice".
    pub notes: String,
    pub instructions: String,
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
    /// Tempo automation: the tempo glides from the previous tempo point to this bar's `tempo`
    /// (Guitar Pro's linear automation) instead of jumping to it here.
    #[serde(default)]
    pub tempo_linear: bool,
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
    /// Fermatas over this bar, by where they fall in it; every track holds there.
    #[serde(default)]
    pub fermatas: Vec<Fermata>,
    /// How notes are beamed, as group sizes counted in the time signature's note value (`[3, 2, 2]`
    /// beams 7/8 as 3+2+2, `[4, 4, 5]` beams 13/16 as 4+4+5). Empty is the time signature's usual
    /// grouping ([`default_beaming`]).
    #[serde(default)]
    pub beams: Vec<u8>,
}

/// Most beam groups a bar may list.
pub const MAX_BEAM_GROUPS: usize = 32;

/// The usual beam groups of a time signature, counted in its note value: one beat each when the
/// beat is a quarter or longer (4/4 as 1+1+1+1), dotted quarters in compound time (6/8 as 3+3),
/// twos with a closing three in other eighth meters (5/8 as 2+3, 7/8 as 2+2+3), and quarter notes'
/// worth in sixteenth meters with the rest joining the last group (13/16 as 4+4+5).
pub fn default_beaming(ts: TimeSignature) -> Vec<u8> {
    let num = ts.numerator.max(1);
    let groups: Vec<u8> = match ts.denominator {
        0..=4 => vec![1; usize::from(num)],
        8 if num.is_multiple_of(3) => vec![3; usize::from(num / 3)],
        8 => {
            let mut g = vec![2; usize::from(num / 2)];
            match g.last_mut() {
                Some(last) if num % 2 == 1 => *last = 3,
                Some(_) => {}
                None => g.push(num),
            }
            g
        }
        d => {
            // A quarter note's worth of this value.
            let quarter = (d / 4).max(1);
            let mut g = vec![quarter; usize::from(num / quarter)];
            let rest = num % quarter;
            match g.last_mut() {
                Some(last) if rest > 0 && rest * 2 <= quarter => *last += rest,
                _ if rest > 0 => g.push(rest),
                _ => {}
            }
            g
        }
    };
    groups.into_iter().take(MAX_BEAM_GROUPS).collect()
}

/// A fermata: the beat at `tick` (from the start of the bar) is held longer, in every track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fermata {
    pub tick: u32,
    pub kind: FermataKind,
    /// Guitar Pro's fermata length in hundredths, kept so a file comes back as it was.
    #[serde(default)]
    pub length: u16,
}

/// How long a fermata holds: the three shapes Guitar Pro draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FermataKind {
    Short,
    #[default]
    Medium,
    Long,
}

impl FermataKind {
    pub const ALL: [FermataKind; 3] = [FermataKind::Short, FermataKind::Medium, FermataKind::Long];

    /// How many times its written length the held beat lasts.
    pub fn stretch(self) -> f64 {
        match self {
            FermataKind::Short => 1.5,
            FermataKind::Medium => 2.0,
            FermataKind::Long => 3.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FermataKind::Short => "short",
            FermataKind::Medium => "medium",
            FermataKind::Long => "long",
        }
    }

    pub fn from_name(name: &str) -> Option<FermataKind> {
        FermataKind::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// A stretch of a bar that plays slower because of a fermata: from `at` ticks into the bar, `ticks`
/// long, lasting `stretch` times as long as written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hold {
    pub at: u32,
    pub ticks: u32,
    pub stretch: f64,
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
            tempo_linear: false,
            repeat_open: false,
            repeat_close: 0,
            marker: None,
            double_bar: false,
            free_time: false,
            triplet_feel: TripletFeel::None,
            endings: 0,
            targets: Vec::new(),
            jump: None,
            fermatas: Vec::new(),
            beams: Vec::new(),
        }
    }

    pub fn capacity(&self) -> u32 {
        self.time_signature.measure_ticks()
    }

    /// The beam groups this bar uses, in its time signature's note value: its own, or the usual ones.
    pub fn beaming(&self) -> Vec<u8> {
        if self.beams.is_empty() { default_beaming(self.time_signature) } else { self.beams.clone() }
    }

    /// Where each beam group starts, in ticks from the start of the bar; the first is 0. What is
    /// left after the groups is a group of its own.
    pub fn beam_starts(&self) -> Vec<u32> {
        let cap = self.capacity();
        let mut starts = vec![0u32];
        let unit = 4 * guitario_theory::TICKS_PER_QUARTER / u32::from(self.time_signature.denominator.max(1));
        let mut at = 0u32;
        for g in self.beaming() {
            at = at.saturating_add(u32::from(g).saturating_mul(unit));
            if at >= cap {
                break;
            }
            starts.push(at);
        }
        starts
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
    /// Up to [`MAX_LYRIC_LINES`] lines of lyrics, sung on the track's first voice.
    #[serde(default)]
    pub lyrics: Vec<LyricLine>,
    /// The name at the start of each system after the first (`s.guit.`); empty lets Guitario
    /// abbreviate the name.
    #[serde(default)]
    pub short_name: String,
    /// The track's colour as red, green, blue; none takes the next of Guitar Pro's track colours.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// The notations the track shows.
    #[serde(default)]
    pub notation: Notations,
    /// How playback interprets the track (Guitar Pro's Interpretation section).
    #[serde(default)]
    pub interpretation: Interpretation,
    /// Which engine sounds the track.
    #[serde(default)]
    pub engine: SoundEngine,
}

/// Guitar Pro's audio engines: RSE plays recorded samples, MIDI a lighter synthesised sound.
/// Guitario's RSE is the sampled sound bank (the synth where the bank has no such instrument).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SoundEngine {
    #[default]
    Rse,
    Midi,
}

/// How the track is played: Guitar Pro's playing styles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayingStyle {
    /// The instrument's own (a pick for a guitar).
    #[default]
    Default,
    Pick,
    Fingers,
    /// Fingerpicking.
    Picking,
    BassSlap,
}

/// Guitar Pro's track interpretation: the playing style, how hard palm mutes damp, how much the
/// strong beats are stressed, and whether every note rings, chords are strummed and the strings
/// are kept apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Interpretation {
    pub style: PlayingStyle,
    /// 0..=100: 0 is a light mute, 100 a dead thud.
    pub palm_mute: u8,
    /// 0..=100: how much louder the first beat of a bar, and the other beats, are played.
    pub accentuation: u8,
    /// Every note rings until a rest (or its string is played again).
    pub auto_let_ring: bool,
    /// Chords are strummed slightly, low string first (high first after an upstroke).
    pub auto_brush: bool,
    /// A stringed instrument: a new note on a string stops the one ringing there. Off, notes ring
    /// on as on a piano.
    pub stringed: bool,
}

impl Default for Interpretation {
    fn default() -> Self {
        Interpretation { style: PlayingStyle::Default, palm_mute: 0, accentuation: 0, auto_let_ring: false, auto_brush: false, stringed: true }
    }
}

/// The notations a track shows, any together, as Guitar Pro's notation buttons choose them.
/// Slash and numbered are kept for exchange; Guitario draws standard notation and tablature.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notations {
    pub standard: bool,
    pub tab: bool,
    pub slash: bool,
    pub numbered: bool,
}

impl Default for Notations {
    fn default() -> Self {
        Notations { standard: true, tab: true, slash: false, numbered: false }
    }
}

impl Notations {
    /// Guitar Pro's flags: 1 standard, 2 tablature, 4 slash, 8 numbered.
    pub fn bits(self) -> u8 {
        u8::from(self.standard) | u8::from(self.tab) << 1 | u8::from(self.slash) << 2 | u8::from(self.numbered) << 3
    }

    pub fn from_bits(bits: u8) -> Notations {
        Notations { standard: bits & 1 != 0, tab: bits & 2 != 0, slash: bits & 4 != 0, numbered: bits & 8 != 0 }
    }

    /// What Guitario draws: standard notation and tablature as chosen, and the tablature when
    /// neither is (a track always shows something).
    pub fn drawn(self) -> (bool, bool) {
        (self.standard, self.tab || !self.standard)
    }
}

/// The colours new tracks take in turn, as Guitar Pro gives them.
pub const TRACK_COLORS: [[u8; 3]; 8] = [
    [0xe0, 0x4f, 0x4f],
    [0x3f, 0x9b, 0xe8],
    [0x5c, 0xbd, 0x5c],
    [0xe8, 0xa2, 0x3a],
    [0xa8, 0x6b, 0xe0],
    [0x3c, 0xc4, 0xb8],
    [0xe0, 0x6b, 0xb5],
    [0xc4, 0xc4, 0x48],
];

/// Lines of lyrics a track may hold, as in Guitar Pro.
pub const MAX_LYRIC_LINES: usize = 5;

/// One line of lyrics as it is typed: the text and the bar its first syllable falls in. The text is
/// split into syllables by [`crate::syllables`] and laid one per beat by [`crate::dispatch_lyrics`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLine {
    /// 0-based bar of the first syllable.
    #[serde(default)]
    pub measure: usize,
    #[serde(default)]
    pub text: String,
}

impl Track {
    /// The track's colour: its own, or the one a track at `index` takes by default.
    pub fn color_at(&self, index: usize) -> [u8; 3] {
        self.color.unwrap_or_else(|| TRACK_COLORS.get(index % TRACK_COLORS.len()).copied().unwrap_or([0xe0, 0x4f, 0x4f]))
    }

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
            lyrics: Vec::new(),
            short_name: String::new(),
            color: None,
            notation: Notations::default(),
            interpretation: Interpretation::default(),
            engine: SoundEngine::Rse,
        }
    }

    /// A drum-kit track: eight kit lanes, General MIDI drums, the GM percussion program.
    pub fn drums(name: impl Into<String>, headers: &[MeasureHeader]) -> Track {
        let notation = Notations { tab: false, ..Notations::default() };
        Track { kind: TrackKind::Drums, program: 0, notation, ..Track::new(name, Tuning::drum_kit(), headers) }
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Measure {
    pub voices: Vec<Voice>,
    /// Volume automation: the track's volume (0..=127) from this bar on, in place of the mixer's.
    #[serde(default)]
    pub volume: Option<u8>,
    /// The volume glides here from the point before instead of jumping.
    #[serde(default)]
    pub volume_linear: bool,
    /// Pan automation: -63 (left) ..= 63 (right) from this bar on.
    #[serde(default)]
    pub pan: Option<i8>,
    #[serde(default)]
    pub pan_linear: bool,
}

impl Measure {
    /// A measure holding one voice with a single rest as long as the header allows.
    pub fn empty_for(header: &MeasureHeader) -> Measure {
        Measure { voices: vec![Voice { beats: vec![Beat::rest(rest_for(header.capacity()))] }], ..Measure::default() }
    }
}

/// An automation per bar: its value at the bar's start and end, or `None` before its first point.
pub type Curve = Vec<Option<(f64, f64)>>;

/// The value of an automation at each bar's start and end, from its points `(bar, value, linear)`
/// in bar order: from a point on the value holds, or, when the next point is linear, glides to it
/// in proportion to the ticks played. Before the first point it is `before` (`None` for "not
/// automated yet"). `starts` holds each bar's first tick and, last, the end of the score.
pub fn automation_curves(starts: &[u64], before: Option<f64>, points: &[(usize, f64, bool)]) -> Curve {
    let n = starts.len().saturating_sub(1);
    let mut out: Vec<Option<(f64, f64)>> = vec![before.map(|v| (v, v)); n];
    let mut prev: Option<(usize, f64)> = before.map(|v| (0, v));
    for &(b, value, linear) in points {
        if b >= n {
            continue;
        }
        if let Some((a, from)) = prev.filter(|(a, _)| linear && b > *a) {
            let (t0, t1) = (starts.get(a).copied().unwrap_or(0), starts.get(b).copied().unwrap_or(0));
            let span = t1.saturating_sub(t0).max(1) as f64;
            let at = |t: u64| from + (value - from) * (t.saturating_sub(t0) as f64 / span);
            for m in a..b {
                if let (Some(slot), Some(s), Some(e)) = (out.get_mut(m), starts.get(m), starts.get(m + 1)) {
                    *slot = Some((at(*s), at(*e)));
                }
            }
        }
        for slot in out.iter_mut().skip(b) {
            *slot = Some((value, value));
        }
        prev = Some((b, value));
    }
    out
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
    /// Tremolo picking: every note of the beat is picked again and again at this speed (slashes on the stem).
    #[serde(default)]
    pub tremolo: Option<TremoloSpeed>,
    /// Overrides the bar's beaming at this beat: start a new beam here, or join the one before.
    #[serde(default)]
    pub beam: Option<BeamOverride>,
    /// A slur drawn from this beat to the one this many beats later in the same bar and voice
    /// (notation only; it does not change how the notes play).
    #[serde(default)]
    pub slur: Option<u8>,
}

/// A per-beat beam choice that beats the bar's grouping (Guitar Pro's beam break and join).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeamOverride {
    /// Never beamed to the beat before.
    Break,
    /// Beamed to the beat before even across the bar's group boundary.
    Join,
}

impl BeamOverride {
    pub fn from_name(name: &str) -> Option<BeamOverride> {
        match name {
            "break" => Some(BeamOverride::Break),
            "join" => Some(BeamOverride::Join),
            _ => None,
        }
    }
}

/// How fast a tremolo-picked beat repeats its notes; the slashes drawn on the stem are the same count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TremoloSpeed {
    Eighth,
    Sixteenth,
    ThirtySecond,
}

impl TremoloSpeed {
    pub const ALL: [TremoloSpeed; 3] = [TremoloSpeed::Eighth, TremoloSpeed::Sixteenth, TremoloSpeed::ThirtySecond];

    /// Ticks between two picks.
    pub fn ticks(self) -> u32 {
        guitario_theory::TICKS_PER_QUARTER / self.per_quarter()
    }

    /// Picks in a quarter note.
    pub fn per_quarter(self) -> u32 {
        match self {
            TremoloSpeed::Eighth => 2,
            TremoloSpeed::Sixteenth => 4,
            TremoloSpeed::ThirtySecond => 8,
        }
    }

    /// Slashes through the stem: one for eighths, two for sixteenths, three for thirty-seconds.
    pub fn slashes(self) -> u8 {
        match self {
            TremoloSpeed::Eighth => 1,
            TremoloSpeed::Sixteenth => 2,
            TremoloSpeed::ThirtySecond => 3,
        }
    }

    pub fn from_slashes(n: u8) -> Option<TremoloSpeed> {
        TremoloSpeed::ALL.into_iter().find(|s| s.slashes() == n)
    }

    pub fn name(self) -> &'static str {
        match self {
            TremoloSpeed::Eighth => "eighth",
            TremoloSpeed::Sixteenth => "sixteenth",
            TremoloSpeed::ThirtySecond => "thirty_second",
        }
    }

    pub fn from_name(name: &str) -> Option<TremoloSpeed> {
        TremoloSpeed::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// A trill: the note alternates with another fret of the same string, at this speed, through its beat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trill {
    pub fret: u8,
    pub speed: TrillSpeed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrillSpeed {
    #[default]
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl TrillSpeed {
    pub const ALL: [TrillSpeed; 3] = [TrillSpeed::Sixteenth, TrillSpeed::ThirtySecond, TrillSpeed::SixtyFourth];

    /// Ticks one note of the trill lasts.
    pub fn ticks(self) -> u32 {
        guitario_theory::TICKS_PER_QUARTER / self.per_quarter()
    }

    pub fn per_quarter(self) -> u32 {
        match self {
            TrillSpeed::Sixteenth => 4,
            TrillSpeed::ThirtySecond => 8,
            TrillSpeed::SixtyFourth => 16,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TrillSpeed::Sixteenth => "sixteenth",
            TrillSpeed::ThirtySecond => "thirty_second",
            TrillSpeed::SixtyFourth => "sixty_fourth",
        }
    }

    pub fn from_name(name: &str) -> Option<TrillSpeed> {
        TrillSpeed::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// A grace note: a quick note on the same string as the note it ornaments, drawn small. It sounds
/// just before the beat, or on the beat taking its time from the note. Guitar Pro 3-5 keep one per
/// note, as here; GPIF files write grace notes as beats of their own before the beat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grace {
    pub fret: u8,
    #[serde(default)]
    pub duration: GraceDuration,
    /// How the grace note moves into the note.
    #[serde(default)]
    pub transition: GraceTransition,
    /// On the beat: the grace note starts with the beat and delays the note. Otherwise it sounds just before.
    #[serde(default)]
    pub on_beat: bool,
    /// A muted (x) grace note.
    #[serde(default)]
    pub dead: bool,
}

impl Grace {
    pub fn new(fret: u8) -> Grace {
        Grace { fret, duration: GraceDuration::default(), transition: GraceTransition::default(), on_beat: false, dead: false }
    }
}

/// How long a grace note sounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraceDuration {
    Sixteenth,
    #[default]
    ThirtySecond,
    SixtyFourth,
}

impl GraceDuration {
    pub const ALL: [GraceDuration; 3] = [GraceDuration::Sixteenth, GraceDuration::ThirtySecond, GraceDuration::SixtyFourth];

    pub fn ticks(self) -> u32 {
        guitario_theory::TICKS_PER_QUARTER / self.per_quarter()
    }

    pub fn per_quarter(self) -> u32 {
        match self {
            GraceDuration::Sixteenth => 4,
            GraceDuration::ThirtySecond => 8,
            GraceDuration::SixtyFourth => 16,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            GraceDuration::Sixteenth => "sixteenth",
            GraceDuration::ThirtySecond => "thirty_second",
            GraceDuration::SixtyFourth => "sixty_fourth",
        }
    }

    pub fn from_name(name: &str) -> Option<GraceDuration> {
        GraceDuration::ALL.into_iter().find(|d| d.name() == name)
    }
}

/// How a grace note moves into its note: struck again, or slid, bent or hammered without a new pluck.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraceTransition {
    #[default]
    None,
    Slide,
    Bend,
    Hammer,
}

impl GraceTransition {
    pub const ALL: [GraceTransition; 4] = [GraceTransition::None, GraceTransition::Slide, GraceTransition::Bend, GraceTransition::Hammer];

    pub fn name(self) -> &'static str {
        match self {
            GraceTransition::None => "none",
            GraceTransition::Slide => "slide",
            GraceTransition::Bend => "bend",
            GraceTransition::Hammer => "hammer",
        }
    }

    pub fn from_name(name: &str) -> Option<GraceTransition> {
        GraceTransition::ALL.into_iter().find(|t| t.name() == name)
    }
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
            tremolo: None,
            beam: None,
            slur: None,
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
    /// The note alternates with another fret of its string.
    #[serde(default)]
    pub trill: Option<Trill>,
    /// A small, quick note on the same string just before this one (or on its beat).
    #[serde(default)]
    pub grace: Option<Grace>,
    /// The left-hand finger that frets the note (drawn T, 1-4).
    #[serde(default)]
    pub left_finger: Option<Finger>,
    /// The right-hand finger that plucks it (drawn p, i, m, a, c).
    #[serde(default)]
    pub right_finger: Option<Finger>,
}

/// A finger, as Guitar Pro names them for both hands: thumb, index, middle, ring (annular) and little.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Finger {
    Thumb,
    Index,
    Middle,
    Ring,
    Little,
}

impl Finger {
    pub const ALL: [Finger; 5] = [Finger::Thumb, Finger::Index, Finger::Middle, Finger::Ring, Finger::Little];

    pub fn name(self) -> &'static str {
        match self {
            Finger::Thumb => "thumb",
            Finger::Index => "index",
            Finger::Middle => "middle",
            Finger::Ring => "ring",
            Finger::Little => "little",
        }
    }

    pub fn from_name(name: &str) -> Option<Finger> {
        Finger::ALL.into_iter().find(|f| f.name() == name)
    }

    /// The left-hand mark: T for the thumb, 1 to 4 for the fingers.
    pub fn left_mark(self) -> &'static str {
        match self {
            Finger::Thumb => "T",
            Finger::Index => "1",
            Finger::Middle => "2",
            Finger::Ring => "3",
            Finger::Little => "4",
        }
    }

    /// The right-hand mark: p, i, m, a, c.
    pub fn right_mark(self) -> &'static str {
        match self {
            Finger::Thumb => "p",
            Finger::Index => "i",
            Finger::Middle => "m",
            Finger::Ring => "a",
            Finger::Little => "c",
        }
    }

    /// Guitar Pro's number: 0 for the thumb to 4 for the little finger.
    pub fn index(self) -> u8 {
        match self {
            Finger::Thumb => 0,
            Finger::Index => 1,
            Finger::Middle => 2,
            Finger::Ring => 3,
            Finger::Little => 4,
        }
    }

    pub fn from_index(i: i64) -> Option<Finger> {
        Finger::ALL.into_iter().find(|f| i64::from(f.index()) == i)
    }
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

    /// Where bar `measure` is held by its fermatas: each from the fermata to the next moment a beat
    /// starts in any track or voice (or the bar's end), in order.
    pub fn holds(&self, measure: usize) -> Vec<Hold> {
        let Some(h) = self.headers.get(measure).filter(|h| !h.fermatas.is_empty()) else { return Vec::new() };
        let cap = h.capacity();
        let mut onsets: Vec<u32> = vec![cap];
        for v in self.tracks.iter().filter_map(|t| t.measures.get(measure)).flat_map(|m| &m.voices) {
            let mut at = 0u32;
            for b in &v.beats {
                onsets.push(at);
                at = at.saturating_add(b.duration.ticks());
            }
        }
        onsets.sort_unstable();
        onsets.dedup();
        let mut out: Vec<Hold> = h
            .fermatas
            .iter()
            .filter(|f| f.tick < cap)
            .filter_map(|f| {
                let end = onsets.iter().copied().find(|o| *o > f.tick)?;
                Some(Hold { at: f.tick, ticks: end - f.tick, stretch: f.kind.stretch() })
            })
            .collect();
        out.sort_by_key(|h| h.at);
        out.dedup_by_key(|h| h.at);
        out
    }

    /// The tempo in force at `measure`, following earlier overrides.
    pub fn tempo_at(&self, measure: usize) -> u16 {
        self.headers.iter().take(measure.saturating_add(1)).filter_map(|h| h.tempo).next_back().unwrap_or(self.tempo)
    }

    /// Each bar's tempo at its start and at its end, in written order. A bar is flat at
    /// [`tempo_at`] unless a later tempo point is linear: then the tempo glides, in proportion to
    /// the ticks played, from the point before (or the song tempo) to that point's tempo.
    pub fn tempo_curves(&self) -> Vec<(f64, f64)> {
        let points: Vec<(usize, f64, bool)> = self.headers.iter().enumerate().filter_map(|(b, h)| Some((b, f64::from(h.tempo?), h.tempo_linear))).collect();
        let song = f64::from(self.tempo);
        automation_curves(&self.bar_starts(), Some(song), &points).into_iter().map(|c| c.unwrap_or((song, song))).collect()
    }

    /// Each bar's first tick in written order, then the end of the score.
    pub fn bar_starts(&self) -> Vec<u64> {
        let mut starts = Vec::with_capacity(self.headers.len() + 1);
        let mut tick = 0u64;
        for h in &self.headers {
            starts.push(tick);
            tick += u64::from(h.capacity());
        }
        starts.push(tick);
        starts
    }

    /// A track's volume and pan automation at each bar's start and end; `None` where it has no
    /// point yet (the mixer's value applies).
    pub fn mix_curves(&self, track: usize) -> (Curve, Curve) {
        let starts = self.bar_starts();
        let Some(t) = self.tracks.get(track) else { return (Vec::new(), Vec::new()) };
        let volume: Vec<(usize, f64, bool)> = t.measures.iter().enumerate().filter_map(|(b, m)| Some((b, f64::from(m.volume?), m.volume_linear))).collect();
        let pan: Vec<(usize, f64, bool)> = t.measures.iter().enumerate().filter_map(|(b, m)| Some((b, f64::from(m.pan?), m.pan_linear))).collect();
        (automation_curves(&starts, None, &volume), automation_curves(&starts, None, &pan))
    }
}

impl Default for Score {
    fn default() -> Self {
        Score::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(n: usize) -> Score {
        let mut s = Score::new();
        for _ in 1..n {
            let h = MeasureHeader::new(TimeSignature::COMMON);
            s.tracks[0].measures.push(Measure::empty_for(&h));
            s.headers.push(h);
        }
        s
    }

    #[test]
    fn a_linear_tempo_point_glides_from_the_point_before() {
        let mut s = bars(6);
        s.tempo = 100;
        // A jump to 120 at bar 2, then a ramp reaching 160 at bar 4: bars 2 and 3 glide.
        s.headers[1].tempo = Some(120);
        s.headers[3].tempo = Some(160);
        s.headers[3].tempo_linear = true;
        let c = s.tempo_curves();
        assert_eq!(c[0], (100.0, 100.0));
        assert_eq!(c[1], (120.0, 140.0));
        assert_eq!(c[2], (140.0, 160.0));
        assert_eq!(c[3], (160.0, 160.0));
        assert_eq!(c[5], (160.0, 160.0));
        // Volume automation starts nowhere (the mixer's value) until its first point.
        s.tracks[0].measures[2].volume = Some(40);
        s.tracks[0].measures[4].volume = Some(120);
        s.tracks[0].measures[4].volume_linear = true;
        s.tracks[0].measures[1].pan = Some(-63);
        let (vol, pan) = s.mix_curves(0);
        assert_eq!(
            (vol[0], vol[1], vol[2], vol[3], vol[4], vol[5]),
            (None, None, Some((40.0, 80.0)), Some((80.0, 120.0)), Some((120.0, 120.0)), Some((120.0, 120.0)))
        );
        assert_eq!((pan[0], pan[1], pan[5]), (None, Some((-63.0, -63.0)), Some((-63.0, -63.0))));
        assert!(s.mix_curves(9).0.is_empty());
        s.tracks[0].measures[4].volume = Some(200);
        assert!(s.validate().is_err());
        s.tracks[0].measures[4].volume = Some(120);
        // Without the flag it is a jump.
        s.headers[3].tempo_linear = false;
        assert_eq!(s.tempo_curves()[2], (120.0, 120.0));
        // A ramp from the song tempo, over bars of different lengths, in proportion to their ticks.
        let mut s = bars(3);
        s.tempo = 60;
        s.headers[0].time_signature = TimeSignature::new(2, 4).unwrap();
        s.headers[2].tempo = Some(120);
        s.headers[2].tempo_linear = true;
        let c = s.tempo_curves();
        assert!((c[0].1 - 80.0).abs() < 1e-9 && (c[1].0 - 80.0).abs() < 1e-9 && (c[1].1 - 120.0).abs() < 1e-9, "{c:?}");
        // A linear point on the first bar has nothing to glide from.
        let mut s = bars(2);
        s.headers[0].tempo = Some(90);
        s.headers[0].tempo_linear = true;
        assert_eq!(s.tempo_curves()[0], (90.0, 90.0));
    }
}
