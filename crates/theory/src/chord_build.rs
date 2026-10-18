//! Chords built as Guitar Pro 8's Chord window builds them: a root, a type, an extension,
//! altered fifth, ninth, eleventh and thirteenth, a bass note or an inversion, and omitted tones.
//! [`ChordSpec::tones`] gives the chord's tones and [`ChordSpec::name`] its symbol; [`voicings`]
//! lists the diagrams that play it on any tuning, easiest first.

use crate::chords::pitch_class_name;
use serde::{Deserialize, Serialize};

/// The chord types of Guitar Pro's Type list, plus the diminished seventh and half-diminished.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChordType {
    #[default]
    Major,
    Seventh,
    MajorSeventh,
    Sixth,
    Minor,
    MinorSeventh,
    MinorMajorSeventh,
    MinorSixth,
    Sus2,
    Sus4,
    SeventhSus2,
    SeventhSus4,
    Diminished,
    DiminishedSeventh,
    HalfDiminished,
    Augmented,
    Power,
}

impl ChordType {
    pub const ALL: [ChordType; 17] = [
        ChordType::Major,
        ChordType::Seventh,
        ChordType::MajorSeventh,
        ChordType::Sixth,
        ChordType::Minor,
        ChordType::MinorSeventh,
        ChordType::MinorMajorSeventh,
        ChordType::MinorSixth,
        ChordType::Sus2,
        ChordType::Sus4,
        ChordType::SeventhSus2,
        ChordType::SeventhSus4,
        ChordType::Diminished,
        ChordType::DiminishedSeventh,
        ChordType::HalfDiminished,
        ChordType::Augmented,
        ChordType::Power,
    ];

    /// The label in the Type list.
    pub fn label(self) -> &'static str {
        match self {
            ChordType::Major => "M",
            ChordType::Seventh => "7",
            ChordType::MajorSeventh => "7M",
            ChordType::Sixth => "6",
            ChordType::Minor => "m",
            ChordType::MinorSeventh => "m7",
            ChordType::MinorMajorSeventh => "m7M",
            ChordType::MinorSixth => "m6",
            ChordType::Sus2 => "sus2",
            ChordType::Sus4 => "sus4",
            ChordType::SeventhSus2 => "7sus2",
            ChordType::SeventhSus4 => "7sus4",
            ChordType::Diminished => "dim",
            ChordType::DiminishedSeventh => "dim7",
            ChordType::HalfDiminished => "m7b5",
            ChordType::Augmented => "aug",
            ChordType::Power => "5",
        }
    }

    pub fn from_label(label: &str) -> Option<ChordType> {
        ChordType::ALL.into_iter().find(|t| t.label() == label)
    }

    /// The tones as (label, semitones above the root).
    fn tones(self) -> &'static [(&'static str, u8)] {
        match self {
            ChordType::Major => &[("T", 0), ("M3", 4), ("5", 7)],
            ChordType::Seventh => &[("T", 0), ("M3", 4), ("5", 7), ("7", 10)],
            ChordType::MajorSeventh => &[("T", 0), ("M3", 4), ("5", 7), ("M7", 11)],
            ChordType::Sixth => &[("T", 0), ("M3", 4), ("5", 7), ("6", 9)],
            ChordType::Minor => &[("T", 0), ("m3", 3), ("5", 7)],
            ChordType::MinorSeventh => &[("T", 0), ("m3", 3), ("5", 7), ("7", 10)],
            ChordType::MinorMajorSeventh => &[("T", 0), ("m3", 3), ("5", 7), ("M7", 11)],
            ChordType::MinorSixth => &[("T", 0), ("m3", 3), ("5", 7), ("6", 9)],
            ChordType::Sus2 => &[("T", 0), ("2", 2), ("5", 7)],
            ChordType::Sus4 => &[("T", 0), ("4", 5), ("5", 7)],
            ChordType::SeventhSus2 => &[("T", 0), ("2", 2), ("5", 7), ("7", 10)],
            ChordType::SeventhSus4 => &[("T", 0), ("4", 5), ("5", 7), ("7", 10)],
            ChordType::Diminished => &[("T", 0), ("m3", 3), ("b5", 6)],
            ChordType::DiminishedSeventh => &[("T", 0), ("m3", 3), ("b5", 6), ("dim7", 9)],
            ChordType::HalfDiminished => &[("T", 0), ("m3", 3), ("b5", 6), ("7", 10)],
            ChordType::Augmented => &[("T", 0), ("M3", 4), ("#5", 8)],
            ChordType::Power => &[("T", 0), ("5", 7)],
        }
    }

    fn has_seventh(self) -> bool {
        self.tones().iter().any(|t| t.1 == 10 || t.1 == 11) || self == ChordType::DiminishedSeventh
    }
}

/// Guitar Pro's Extension list: stacked ninths, elevenths and thirteenths, or one added tone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Extension {
    #[default]
    None,
    Ninth,
    Eleventh,
    Thirteenth,
    Add9,
    Add11,
    Add13,
}

impl Extension {
    pub const ALL: [Extension; 7] =
        [Extension::None, Extension::Ninth, Extension::Eleventh, Extension::Thirteenth, Extension::Add9, Extension::Add11, Extension::Add13];

    pub fn label(self) -> &'static str {
        match self {
            Extension::None => "",
            Extension::Ninth => "9",
            Extension::Eleventh => "11",
            Extension::Thirteenth => "13",
            Extension::Add9 => "add9",
            Extension::Add11 => "add11",
            Extension::Add13 => "add13",
        }
    }

    pub fn from_label(label: &str) -> Option<Extension> {
        Extension::ALL.into_iter().find(|e| e.label() == label)
    }

    fn stacked(self) -> bool {
        matches!(self, Extension::Ninth | Extension::Eleventh | Extension::Thirteenth)
    }
}

/// A tone lowered or raised a half step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Alteration {
    #[default]
    None,
    Flat,
    Sharp,
}

impl Alteration {
    fn shift(self) -> i8 {
        match self {
            Alteration::None => 0,
            Alteration::Flat => -1,
            Alteration::Sharp => 1,
        }
    }

    fn mark(self) -> &'static str {
        match self {
            Alteration::None => "",
            Alteration::Flat => "b",
            Alteration::Sharp => "#",
        }
    }
}

/// One tone of a built chord.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChordTone {
    /// Its degree as the Omissions row labels it: `T`, `m3`, `5`, `b9` …
    pub label: &'static str,
    /// Semitones above the root (an octave or more for 9, 11 and 13).
    pub interval: u8,
    /// Left out of the chord.
    pub omitted: bool,
}

impl ChordTone {
    pub fn pitch_class(&self, root: u8) -> u8 {
        (root + self.interval) % 12
    }
}

/// A chord as the Chord window builds it. Transposing moves its root and bass and keeps the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChordSpec {
    /// Pitch class of the root, 0 for C.
    pub root: u8,
    /// Spell names with flats.
    #[serde(default)]
    pub flats: bool,
    #[serde(default)]
    pub kind: ChordType,
    #[serde(default)]
    pub extension: Extension,
    #[serde(default)]
    pub fifth: Alteration,
    #[serde(default)]
    pub ninth: Alteration,
    #[serde(default)]
    pub eleventh: Alteration,
    #[serde(default)]
    pub thirteenth: Alteration,
    /// A bass note other than the root (pitch class), for a slash chord.
    #[serde(default)]
    pub bass: Option<u8>,
    /// Bit `i` set leaves out tone `i` of [`ChordSpec::tones`].
    #[serde(default)]
    pub omit: u16,
}

impl ChordSpec {
    pub fn new(root: u8, kind: ChordType) -> ChordSpec {
        ChordSpec { root: root % 12, kind, ..ChordSpec::default() }
    }

    /// Which alterations make sense: the fifth, ninth, eleventh and thirteenth when the chord has them.
    pub fn alterable(&self) -> [bool; 4] {
        let base = self.base_intervals();
        [base.contains(&7), base.contains(&14), base.contains(&17), base.contains(&21)]
    }

    fn base_intervals(&self) -> Vec<u8> {
        self.base_tones().iter().map(|t| t.1).collect()
    }

    /// Tones before alterations.
    fn base_tones(&self) -> Vec<(&'static str, u8)> {
        let mut tones: Vec<(&'static str, u8)> = self.kind.tones().to_vec();
        let ext = self.extension;
        if ext.stacked() && !self.kind.has_seventh() && matches!(self.kind, ChordType::Major | ChordType::Minor | ChordType::Sus2 | ChordType::Sus4) {
            tones.push(("7", 10));
        }
        match ext {
            Extension::None => {}
            Extension::Ninth => tones.push(("9", 14)),
            Extension::Eleventh => tones.extend([("9", 14), ("11", 17)]),
            Extension::Thirteenth => tones.extend([("9", 14), ("13", 21)]),
            Extension::Add9 => tones.push(("9", 14)),
            Extension::Add11 => tones.push(("11", 17)),
            Extension::Add13 => tones.push(("13", 21)),
        }
        tones
    }

    /// The chord's tones in order, with alterations applied and omissions marked.
    pub fn tones(&self) -> Vec<ChordTone> {
        let altered = |label: &'static str, interval: u8| -> (&'static str, u8) {
            let (alt, flat, sharp) = match interval {
                7 => (self.fifth, "b5", "#5"),
                14 => (self.ninth, "b9", "#9"),
                17 => (self.eleventh, "b11", "#11"),
                21 => (self.thirteenth, "b13", "#13"),
                _ => return (label, interval),
            };
            match alt {
                Alteration::None => (label, interval),
                Alteration::Flat => (flat, interval.saturating_add_signed(alt.shift())),
                Alteration::Sharp => (sharp, interval.saturating_add_signed(alt.shift())),
            }
        };
        self.base_tones()
            .into_iter()
            .enumerate()
            .map(|(i, (label, interval))| {
                let (label, interval) = altered(label, interval);
                ChordTone { label, interval, omitted: i < 16 && self.omit & (1 << i) != 0 }
            })
            .collect()
    }

    /// Pitch classes that sound (not omitted), the bass's included.
    pub fn pitch_classes(&self) -> Vec<u8> {
        let mut pcs: Vec<u8> = self.tones().iter().filter(|t| !t.omitted).map(|t| t.pitch_class(self.root)).collect();
        if let Some(b) = self.bass {
            pcs.push(b % 12);
        }
        pcs.sort_unstable();
        pcs.dedup();
        pcs
    }

    /// The note the chord stands on: the bass, or the root.
    pub fn bass_class(&self) -> u8 {
        self.bass.map_or(self.root, |b| b % 12)
    }

    /// Inversion `n`: the bass becomes the chord's `n`th tone above the root (1 the third, 2 the
    /// fifth, 3 the seventh); 0 puts the root back.
    pub fn invert(&mut self, n: usize) {
        self.bass = if n == 0 { None } else { self.tones().get(n).map(|t| t.pitch_class(self.root)) };
    }

    /// Which inversion the bass makes, if it is a chord tone other than the root.
    pub fn inversion(&self) -> usize {
        let Some(b) = self.bass else { return 0 };
        self.tones().iter().position(|t| t.pitch_class(self.root) == b % 12).unwrap_or(0)
    }

    /// The suffix after the root: `m7`, `maj9`, `7b5`, `sus4`, `6/9`, `madd11` …
    pub fn suffix(&self) -> String {
        let t = self.kind;
        let e = self.extension;
        let mut s = if e.stacked() {
            // An altered ninth is written on the seventh: C7#9, not C9#9.
            let n = if e == Extension::Ninth && self.ninth != Alteration::None { "7" } else { e.label() };
            match t {
                ChordType::Major | ChordType::Seventh => n.to_string(),
                ChordType::MajorSeventh => format!("maj{n}"),
                ChordType::Minor | ChordType::MinorSeventh => format!("m{n}"),
                ChordType::MinorMajorSeventh => format!("mMaj{n}"),
                ChordType::Sus2 | ChordType::SeventhSus2 => format!("{n}sus2"),
                ChordType::Sus4 | ChordType::SeventhSus4 => format!("{n}sus4"),
                ChordType::HalfDiminished => format!("m{n}b5"),
                ChordType::Sixth if e == Extension::Ninth => "6/9".to_string(),
                ChordType::MinorSixth if e == Extension::Ninth => "m6/9".to_string(),
                _ => format!("{}add{n}", base_suffix(t)),
            }
        } else {
            format!("{}{}", base_suffix(t), e.label())
        };
        let fifth_written = matches!(t, ChordType::Diminished | ChordType::DiminishedSeventh | ChordType::HalfDiminished | ChordType::Augmented);
        if !fifth_written {
            s.push_str(&format!("{}{}", self.fifth.mark(), if self.fifth == Alteration::None { "" } else { "5" }));
        }
        for (alt, n) in [(self.ninth, "9"), (self.eleventh, "11"), (self.thirteenth, "13")] {
            if alt != Alteration::None {
                s.push_str(alt.mark());
                s.push_str(n);
            }
        }
        s
    }

    /// The chord symbol: `D#m7add11`, `Cmaj7/E`.
    pub fn name(&self) -> String {
        let mut n = format!("{}{}", pitch_class_name(self.root, self.flats), self.suffix());
        if let Some(b) = self.bass.filter(|b| b % 12 != self.root) {
            n.push('/');
            n.push_str(pitch_class_name(b, self.flats));
        }
        n
    }

    /// The same chord `semitones` higher (or lower).
    pub fn transposed(&self, semitones: i16) -> ChordSpec {
        let shift = |pc: u8| (i16::from(pc) + semitones).rem_euclid(12) as u8;
        ChordSpec { root: shift(self.root), bass: self.bass.map(shift), ..*self }
    }
}

fn base_suffix(t: ChordType) -> &'static str {
    match t {
        ChordType::Major => "",
        ChordType::Seventh => "7",
        ChordType::MajorSeventh => "maj7",
        ChordType::Sixth => "6",
        ChordType::Minor => "m",
        ChordType::MinorSeventh => "m7",
        ChordType::MinorMajorSeventh => "mMaj7",
        ChordType::MinorSixth => "m6",
        ChordType::Sus2 => "sus2",
        ChordType::Sus4 => "sus4",
        ChordType::SeventhSus2 => "7sus2",
        ChordType::SeventhSus4 => "7sus4",
        ChordType::Diminished => "dim",
        ChordType::DiminishedSeventh => "dim7",
        ChordType::HalfDiminished => "m7b5",
        ChordType::Augmented => "aug",
        ChordType::Power => "5",
    }
}

/// One way to play a chord: a fret per string (highest string first, `None` not played), the
/// finger on each fretted string (1 index to 4 little), and how hard it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Voicing {
    pub frets: Vec<Option<u8>>,
    pub fingers: Vec<Option<u8>>,
    /// The fret a first-finger barre lies across, if any.
    pub barre: Option<u8>,
    /// Higher is harder: stretch, fingers, barre, muted strings inside the chord, position.
    pub difficulty: u32,
}

impl Voicing {
    /// The fret a diagram of it starts at: the nut when it fits in the first five frets.
    pub fn first_fret(&self) -> u8 {
        let fretted = self.frets.iter().flatten().copied().filter(|f| *f > 0);
        if fretted.clone().max().unwrap_or(0) <= 5 { 1 } else { fretted.min().unwrap_or(1) }
    }
}

/// Fingers for fretted strings: a first-finger barre across the lowest fret when it holds two or
/// more strings with nothing open between them, then one finger per remaining note in fret order.
/// `None` when it needs more than four fingers.
pub fn fingering(frets: &[Option<u8>]) -> Option<(Vec<Option<u8>>, Option<u8>)> {
    let n = frets.len();
    let mut fingers = vec![None; n];
    let fretted: Vec<(usize, u8)> = frets.iter().enumerate().filter_map(|(i, f)| f.filter(|f| *f > 0).map(|f| (i, f))).collect();
    let Some(low) = fretted.iter().map(|x| x.1).min() else { return Some((fingers, None)) };
    let at_low: Vec<usize> = fretted.iter().filter(|x| x.1 == low).map(|x| x.0).collect();
    let (lo_s, hi_s) = (at_low.iter().copied().min().unwrap_or(0), at_low.iter().copied().max().unwrap_or(0));
    // A barre: no open string under it.
    let barre = at_low.len() >= 2 && (lo_s..=hi_s).all(|i| frets.get(i).copied().flatten() != Some(0));
    let mut next = 1u8;
    if barre {
        for &i in &at_low {
            if let Some(f) = fingers.get_mut(i) {
                *f = Some(1);
            }
        }
        next = 2;
    }
    // Remaining notes by fret, then from the lowest string up (strings are stored highest first).
    let mut rest: Vec<(usize, u8)> = fretted.into_iter().filter(|x| !(barre && x.1 == low)).collect();
    rest.sort_by_key(|x| (x.1, std::cmp::Reverse(x.0)));
    let count = rest.len();
    for (k, (i, fret)) in rest.into_iter().enumerate() {
        // A note further up the neck takes a later finger, as a hand in position does, as long
        // as enough fingers are left for the notes after it.
        let left_after = (count - k - 1) as u8;
        let wanted = 1 + fret.saturating_sub(low);
        let finger = next.max(wanted.min(4u8.saturating_sub(left_after)));
        if finger > 4 {
            return None;
        }
        if let Some(f) = fingers.get_mut(i) {
            *f = Some(finger);
        }
        next = finger + 1;
    }
    Some((fingers, barre.then_some(low)))
}

/// Highest fret a voicing reaches for.
const TOP: u8 = 15;
/// Frets one hand position covers: a diagram's five rows.
const SPAN: u8 = 5;

/// Every playable diagram of `spec` on strings tuned to `open` (MIDI numbers, highest string
/// first) with a capo at `capo`, easiest first, at most `limit`. Each one has every tone that is
/// not omitted, the bass (or root) as its lowest note, a stretch of at most five frets, and at most
/// four fingers.
pub fn voicings(spec: &ChordSpec, open: &[u8], capo: u8, limit: usize) -> Vec<Voicing> {
    let n = open.len();
    if n == 0 || n > crate::Tuning::MAX_STRINGS {
        return Vec::new();
    }
    let pcs = spec.pitch_classes();
    let want: u16 = pcs.iter().fold(0, |m, pc| m | 1 << pc);
    let bass = spec.bass_class();
    let min_strings = pcs.len().clamp(2, 3);
    let mut found: Vec<Voicing> = Vec::new();
    let mut frets = vec![None; n];
    for base in 0..=TOP.saturating_sub(SPAN - 1) {
        let window: Vec<u8> = (base.max(1)..base.max(1) + SPAN).collect();
        search(open, capo, &window, want, bass, min_strings, n, &mut frets, &mut found);
        if found.len() > 4000 {
            break;
        }
    }
    found.sort_by(|a, b| a.difficulty.cmp(&b.difficulty).then_with(|| a.first_fret().cmp(&b.first_fret())).then_with(|| a.frets.cmp(&b.frets)));
    found.dedup_by(|a, b| a.frets == b.frets);
    found.truncate(limit);
    found
}

/// Depth-first from the lowest string (the last) to the highest.
#[allow(clippy::too_many_arguments)]
fn search(open: &[u8], capo: u8, window: &[u8], want: u16, bass: u8, min_strings: usize, at: usize, frets: &mut Vec<Option<u8>>, found: &mut Vec<Voicing>) {
    if at == 0 {
        finish(open, capo, want, bass, min_strings, frets, found);
        return;
    }
    let s = at - 1;
    let Some(&o) = open.get(s) else { return };
    let pitch = |f: u8| (u16::from(o) + u16::from(capo) + u16::from(f)) % 12;
    // Nothing sounds below this string yet: the first sounding note must be the bass.
    let first = frets.iter().skip(at).all(Option::is_none);
    let mut options: Vec<Option<u8>> = vec![None];
    for f in std::iter::once(0).chain(window.iter().copied()) {
        let pc = pitch(f);
        if want & (1 << pc) != 0 && (!first || pc == u16::from(bass)) {
            options.push(Some(f));
        }
    }
    for opt in options {
        if let Some(slot) = frets.get_mut(s) {
            *slot = opt;
        }
        search(open, capo, window, want, bass, min_strings, s, frets, found);
    }
    if let Some(slot) = frets.get_mut(s) {
        *slot = None;
    }
}

fn finish(open: &[u8], capo: u8, want: u16, bass: u8, min_strings: usize, frets: &[Option<u8>], found: &mut Vec<Voicing>) {
    let sounding: Vec<(usize, u8)> = frets.iter().enumerate().filter_map(|(i, f)| f.map(|f| (i, f))).collect();
    if sounding.len() < min_strings {
        return;
    }
    // The lowest pitch, not the lowest string, is the bass: a high fret on a low string can sit above an open string.
    let lowest = sounding.iter().map(|(i, f)| u16::from(*open.get(*i).unwrap_or(&0)) + u16::from(*f)).min().unwrap_or(0);
    if (lowest + u16::from(capo)) % 12 != u16::from(bass) {
        return;
    }
    let have = sounding.iter().fold(0u16, |m, (i, f)| m | 1 << ((u16::from(*open.get(*i).unwrap_or(&0)) + u16::from(capo) + u16::from(*f)) % 12));
    if have != want {
        return;
    }
    let fretted: Vec<u8> = sounding.iter().map(|x| x.1).filter(|f| *f > 0).collect();
    let (lo, hi) = (fretted.iter().copied().min().unwrap_or(0), fretted.iter().copied().max().unwrap_or(0));
    if hi.saturating_sub(lo) >= SPAN {
        return;
    }
    let Some((fingers, barre)) = fingering(frets) else { return };
    // Strings not played between sounding ones.
    let (top, bottom) = (sounding.first().map_or(0, |x| x.0), sounding.last().map_or(0, |x| x.0));
    let inner_mutes = (top..=bottom).filter(|i| frets.get(*i).copied().flatten().is_none()).count() as u32;
    if inner_mutes > 1 {
        return;
    }
    let used = fingers.iter().flatten().copied().max().unwrap_or(0) as u32;
    let opens_high = fretted.is_empty() || lo <= 4;
    let stretch = u32::from(hi.saturating_sub(lo));
    let difficulty = stretch * 3
        + if stretch >= 4 { 6 } else { 0 }
        + used * 2
        + if barre.is_some() { 4 } else { 0 }
        + inner_mutes * 6
        + u32::from(lo) / 2
        + if !opens_high && sounding.iter().any(|x| x.1 == 0) { 4 } else { 0 }
        + (open.len() - sounding.len()) as u32 * 3;
    found.push(Voicing { frets: frets.to_vec(), fingers, barre, difficulty });
}

#[cfg(test)]
mod tests {
    use super::*;

    const STANDARD: [u8; 6] = [64, 59, 55, 50, 45, 40];

    fn spec(root: u8, kind: ChordType) -> ChordSpec {
        ChordSpec::new(root, kind)
    }

    #[test]
    fn names_follow_type_extension_alterations_and_bass() {
        assert_eq!(spec(0, ChordType::Major).name(), "C");
        assert_eq!(spec(9, ChordType::Minor).name(), "Am");
        assert_eq!(spec(7, ChordType::Seventh).name(), "G7");
        assert_eq!(spec(5, ChordType::MajorSeventh).name(), "Fmaj7");
        let mut d = spec(3, ChordType::MinorSeventh);
        d.extension = Extension::Add11;
        assert_eq!(d.name(), "D#m7add11", "the manual's example");
        d.flats = true;
        assert_eq!(d.name(), "Ebm7add11");
        let mut c9 = spec(0, ChordType::Seventh);
        c9.extension = Extension::Ninth;
        assert_eq!(c9.name(), "C9");
        c9.ninth = Alteration::Sharp;
        assert_eq!(c9.name(), "C7#9", "an altered ninth is written on the seventh");
        let mut e7 = spec(4, ChordType::Seventh);
        e7.fifth = Alteration::Flat;
        assert_eq!(e7.name(), "E7b5");
        let mut c = spec(0, ChordType::Major);
        c.invert(1);
        assert_eq!((c.name(), c.inversion()), ("C/E".to_string(), 1));
        c.invert(2);
        assert_eq!(c.name(), "C/G");
        c.invert(0);
        assert_eq!(c.name(), "C");
        let mut six9 = spec(0, ChordType::Sixth);
        six9.extension = Extension::Ninth;
        assert_eq!(six9.name(), "C6/9");
        assert_eq!(spec(11, ChordType::HalfDiminished).name(), "Bm7b5");
        assert_eq!(spec(4, ChordType::Power).name(), "E5");
    }

    #[test]
    fn tones_take_alterations_and_omissions() {
        let mut d = spec(3, ChordType::MinorSeventh);
        d.extension = Extension::Add11;
        let labels: Vec<&str> = d.tones().iter().map(|t| t.label).collect();
        assert_eq!(labels, vec!["T", "m3", "5", "7", "11"]);
        // D# F# A# C# G#, the manual's Omissions row.
        let names: Vec<&str> = d.tones().iter().map(|t| pitch_class_name(t.pitch_class(d.root), false)).collect();
        assert_eq!(names, vec!["D#", "F#", "A#", "C#", "G#"]);
        d.fifth = Alteration::Flat;
        assert_eq!(d.tones()[2].label, "b5");
        assert_eq!(d.tones()[2].interval, 6);
        d.omit = 1 << 2;
        assert!(!d.pitch_classes().contains(&9), "the omitted fifth does not sound");
        assert_eq!(d.alterable(), [true, false, true, false]);
    }

    #[test]
    fn transposing_moves_root_and_bass() {
        let mut d = spec(0, ChordType::Major);
        d.bass = Some(4);
        let up = d.transposed(2);
        assert_eq!(up.name(), "D/F#");
        assert_eq!(d.transposed(-1).name(), "B/D#");
    }

    #[test]
    fn open_chords_come_first() {
        let first = |s: ChordSpec| voicings(&s, &STANDARD, 0, 40).into_iter().next().unwrap().frets;
        let n = None;
        // Strings highest first.
        assert_eq!(first(spec(4, ChordType::Major)), vec![Some(0), Some(0), Some(1), Some(2), Some(2), Some(0)], "open E");
        assert_eq!(first(spec(9, ChordType::Minor)), vec![Some(0), Some(1), Some(2), Some(2), Some(0), n], "open Am");
        assert_eq!(first(spec(2, ChordType::Major)), vec![Some(2), Some(3), Some(2), Some(0), n, n], "open D");
        assert_eq!(first(spec(0, ChordType::Major)), vec![Some(0), Some(1), Some(0), Some(2), Some(3), n], "open C");
    }

    #[test]
    fn every_voicing_plays_the_chord_with_the_bass_lowest() {
        let mut d = spec(0, ChordType::Major);
        d.invert(1);
        let list = voicings(&d, &STANDARD, 0, 60);
        assert!(list.len() > 5);
        for v in &list {
            let sounding: Vec<u8> = v.frets.iter().zip(STANDARD).filter_map(|(f, o)| f.map(|f| o + f)).collect();
            assert_eq!(sounding.iter().min().map(|p| p % 12), Some(4), "{v:?}: E in the bass");
            let pcs: std::collections::BTreeSet<u8> = sounding.iter().map(|p| p % 12).collect();
            assert_eq!(pcs, [0, 4, 7].into_iter().collect());
            assert!(v.fingers.iter().flatten().all(|f| (1..=4).contains(f)));
        }
        assert!(list.windows(2).all(|w| w[0].difficulty <= w[1].difficulty), "easiest first");
    }

    #[test]
    fn barres_are_found_and_fingered() {
        let list = voicings(&spec(5, ChordType::Major), &STANDARD, 0, 40);
        let barre = list.iter().find(|v| v.frets == vec![Some(1), Some(1), Some(2), Some(3), Some(3), Some(1)]).expect("the F barre");
        assert_eq!(barre.barre, Some(1));
        assert_eq!(barre.fingers[0], Some(1));
        assert_eq!(barre.fingers[2], Some(2));
        assert!(barre.fingers[3].is_some_and(|f| f >= 3) && barre.fingers[4].is_some_and(|f| f >= 3));
    }

    #[test]
    fn any_tuning_and_string_count_works() {
        // Drop D power chord and a seven-string E minor.
        assert!(voicings(&spec(2, ChordType::Power), &[64, 59, 55, 50, 45, 38], 0, 10).iter().any(|v| v.frets[5] == Some(0) && v.frets[4] == Some(0)));
        assert!(!voicings(&spec(4, ChordType::Minor), &[64, 59, 55, 50, 45, 40, 35], 0, 10).is_empty());
        // A capo moves every fret down.
        let capo = voicings(&spec(6, ChordType::Major), &STANDARD, 2, 1);
        assert_eq!(capo[0].frets, vec![Some(0), Some(0), Some(1), Some(2), Some(2), Some(0)], "F# with a capo at 2 is an E shape");
        assert!(voicings(&spec(0, ChordType::Major), &[], 0, 10).is_empty());
    }
}
