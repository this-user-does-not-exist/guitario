//! Scales as Guitar Pro 8's Scale tool shows them: a library of named scales, modes, arpeggios
//! and tetrachords with tags to filter by, their intervals, and a finder that names the scales a
//! passage uses.

use crate::chords::pitch_class_name;

/// Tags the Scale tool filters by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tag {
    Usual,
    Scale,
    Mode,
    Major,
    Minor,
    Harmonic,
    Melodic,
    Pentatonic,
    Dominant,
    Blues,
    Arpeggio,
    Triad,
    Tetrachord,
    Symmetrical,
    Exotic,
    Bebop,
}

impl Tag {
    pub const ALL: [Tag; 16] = [
        Tag::Usual,
        Tag::Scale,
        Tag::Mode,
        Tag::Major,
        Tag::Minor,
        Tag::Harmonic,
        Tag::Melodic,
        Tag::Pentatonic,
        Tag::Dominant,
        Tag::Blues,
        Tag::Arpeggio,
        Tag::Triad,
        Tag::Tetrachord,
        Tag::Symmetrical,
        Tag::Exotic,
        Tag::Bebop,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tag::Usual => "Usual",
            Tag::Scale => "Scale",
            Tag::Mode => "Mode",
            Tag::Major => "Major",
            Tag::Minor => "Minor",
            Tag::Harmonic => "Harmonic",
            Tag::Melodic => "Melodic",
            Tag::Pentatonic => "Pentatonic",
            Tag::Dominant => "Dominant",
            Tag::Blues => "Blues",
            Tag::Arpeggio => "Arpeggio",
            Tag::Triad => "Triad",
            Tag::Tetrachord => "Tetrachord",
            Tag::Symmetrical => "Symmetrical",
            Tag::Exotic => "Exotic",
            Tag::Bebop => "Bebop",
        }
    }

    pub fn from_label(label: &str) -> Option<Tag> {
        Tag::ALL.into_iter().find(|t| t.label().eq_ignore_ascii_case(label))
    }
}

/// A named scale: semitones above its key note, ascending from 0, within an octave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scale {
    pub name: &'static str,
    pub intervals: &'static [u8],
    pub tags: &'static [Tag],
}

impl Scale {
    pub fn has(&self, tag: Tag) -> bool {
        self.tags.contains(&tag)
    }

    /// The scale's pitch classes on `root`, as a bit per class.
    pub fn mask(&self, root: u8) -> u16 {
        self.intervals.iter().fold(0, |m, i| m | 1 << ((root + i) % 12))
    }

    /// The scale's notes on `root`. A seven-note scale gives each letter once from the key note
    /// (F Dorian is F G Ab Bb C D Eb); `flats` spells the key note and the other scales.
    pub fn notes(&self, root: u8, flats: bool) -> Vec<String> {
        const LETTERS: [char; 7] = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];
        const NATURAL: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
        let plain = || self.intervals.iter().map(|i| pitch_class_name((root + i) % 12, flats).to_string()).collect();
        let first = pitch_class_name(root % 12, flats).chars().next().unwrap_or('C');
        let Some(start) = LETTERS.iter().position(|l| *l == first) else { return plain() };
        if self.intervals.len() != 7 {
            return plain();
        }
        let mut out = Vec::with_capacity(7);
        for (k, i) in self.intervals.iter().enumerate() {
            let at = (start + k) % 7;
            let (Some(letter), Some(natural)) = (LETTERS.get(at), NATURAL.get(at)) else { return plain() };
            let pc = (root + i) % 12;
            let up = (pc + 12 - natural) % 12;
            let accidental = match up {
                0 => "",
                1 => "#",
                2 => "##",
                11 => "b",
                10 => "bb",
                _ => return plain(),
            };
            out.push(format!("{letter}{accidental}"));
        }
        out
    }

    /// Interval names from the key note (`P1`, `M2`, `m3` …) and the octave, `P8`.
    pub fn interval_names(&self) -> Vec<&'static str> {
        let has = |s: u8| self.intervals.contains(&s);
        let mut names: Vec<&'static str> = self
            .intervals
            .iter()
            .map(|i| match i {
                0 => "P1",
                1 => "m2",
                2 => "M2",
                // With a major third beside it, the raised second.
                3 => {
                    if has(4) {
                        "A2"
                    } else {
                        "m3"
                    }
                }
                4 => "M3",
                5 => "P4",
                6 => {
                    if has(7) {
                        "A4"
                    } else {
                        "d5"
                    }
                }
                7 => "P5",
                8 => {
                    if has(7) || !has(4) {
                        "m6"
                    } else {
                        "A5"
                    }
                }
                9 => {
                    if has(10) || has(11) || !has(8) {
                        "M6"
                    } else {
                        "d7"
                    }
                }
                10 => "m7",
                11 => "M7",
                _ => "?",
            })
            .collect();
        names.push("P8");
        names
    }

    /// Semitones from each note to the next, the octave closing the list.
    pub fn steps(&self) -> Vec<u8> {
        let mut out: Vec<u8> = self
            .intervals
            .windows(2)
            .map(|w| match w {
                [a, b] => b.saturating_sub(*a),
                _ => 0,
            })
            .collect();
        if let Some(last) = self.intervals.last() {
            out.push(12u8.saturating_sub(*last));
        }
        out
    }
}

use Tag::*;

const fn s(name: &'static str, intervals: &'static [u8], tags: &'static [Tag]) -> Scale {
    Scale { name, intervals, tags }
}

/// The library, the most used first.
pub const SCALES: &[Scale] = &[
    s("Major", &[0, 2, 4, 5, 7, 9, 11], &[Usual, Scale, Major]),
    s("Natural Minor", &[0, 2, 3, 5, 7, 8, 10], &[Usual, Scale, Minor]),
    s("Ionian (1st mode of major)", &[0, 2, 4, 5, 7, 9, 11], &[Usual, Scale, Mode, Major]),
    s("Dorian (2nd mode of major)", &[0, 2, 3, 5, 7, 9, 10], &[Usual, Scale, Mode, Minor]),
    s("Phrygian (3rd mode of major)", &[0, 1, 3, 5, 7, 8, 10], &[Usual, Scale, Mode, Minor]),
    s("Lydian (4th mode of major)", &[0, 2, 4, 6, 7, 9, 11], &[Usual, Scale, Mode, Major]),
    s("Mixolydian (5th mode of major)", &[0, 2, 4, 5, 7, 9, 10], &[Usual, Scale, Mode, Major, Dominant]),
    s("Aeolian (6th mode of major)", &[0, 2, 3, 5, 7, 8, 10], &[Usual, Scale, Mode, Minor]),
    s("Locrian (7th mode of major)", &[0, 1, 3, 5, 6, 8, 10], &[Usual, Scale, Mode, Minor]),
    s("Major Pentatonic", &[0, 2, 4, 7, 9], &[Usual, Scale, Pentatonic, Major]),
    s("Minor Pentatonic", &[0, 3, 5, 7, 10], &[Usual, Scale, Pentatonic, Minor]),
    s("Blues", &[0, 3, 5, 6, 7, 10], &[Usual, Scale, Blues, Minor]),
    s("Major Blues", &[0, 2, 3, 4, 7, 9], &[Usual, Scale, Blues, Major]),
    s("Harmonic Minor", &[0, 2, 3, 5, 7, 8, 11], &[Usual, Scale, Minor, Harmonic]),
    s("Melodic Minor (ascending)", &[0, 2, 3, 5, 7, 9, 11], &[Usual, Scale, Minor, Melodic]),
    s("Locrian Natural 6 (2nd mode of harmonic minor)", &[0, 1, 3, 5, 6, 9, 10], &[Scale, Mode, Harmonic]),
    s("Ionian #5 (3rd mode of harmonic minor)", &[0, 2, 4, 5, 8, 9, 11], &[Scale, Mode, Harmonic, Major]),
    s("Dorian #4 (4th mode of harmonic minor)", &[0, 2, 3, 6, 7, 9, 10], &[Scale, Mode, Harmonic, Minor]),
    s("Phrygian Dominant (5th mode of harmonic minor)", &[0, 1, 4, 5, 7, 8, 10], &[Usual, Scale, Mode, Harmonic, Dominant, Exotic]),
    s("Lydian #2 (6th mode of harmonic minor)", &[0, 3, 4, 6, 7, 9, 11], &[Scale, Mode, Harmonic, Major]),
    s("Ultralocrian (7th mode of harmonic minor)", &[0, 1, 3, 4, 6, 8, 9], &[Scale, Mode, Harmonic]),
    s("Dorian b2 (2nd mode of melodic minor)", &[0, 1, 3, 5, 7, 9, 10], &[Scale, Mode, Melodic, Minor]),
    s("Lydian Augmented (3rd mode of melodic minor)", &[0, 2, 4, 6, 8, 9, 11], &[Scale, Mode, Melodic, Major]),
    s("Lydian Dominant (4th mode of melodic minor)", &[0, 2, 4, 6, 7, 9, 10], &[Usual, Scale, Mode, Melodic, Dominant]),
    s("Mixolydian b6 (5th mode of melodic minor)", &[0, 2, 4, 5, 7, 8, 10], &[Scale, Mode, Melodic, Dominant]),
    s("Locrian Natural 2 (6th mode of melodic minor)", &[0, 2, 3, 5, 6, 8, 10], &[Scale, Mode, Melodic, Minor]),
    s("Altered (7th mode of melodic minor)", &[0, 1, 3, 4, 6, 8, 10], &[Usual, Scale, Mode, Melodic, Dominant]),
    s("Harmonic Major", &[0, 2, 4, 5, 7, 8, 11], &[Scale, Major, Harmonic]),
    s("Double Harmonic", &[0, 1, 4, 5, 7, 8, 11], &[Scale, Harmonic, Exotic]),
    s("Hungarian Minor", &[0, 2, 3, 6, 7, 8, 11], &[Scale, Minor, Exotic]),
    s("Hungarian Major", &[0, 3, 4, 6, 7, 9, 10], &[Scale, Major, Exotic]),
    s("Neapolitan Minor", &[0, 1, 3, 5, 7, 8, 11], &[Scale, Minor, Exotic]),
    s("Neapolitan Major", &[0, 1, 3, 5, 7, 9, 11], &[Scale, Major, Exotic]),
    s("Romanian Minor", &[0, 2, 3, 6, 7, 9, 10], &[Scale, Minor, Exotic]),
    s("Persian", &[0, 1, 4, 5, 6, 8, 11], &[Scale, Exotic]),
    s("Enigmatic", &[0, 1, 4, 6, 8, 10, 11], &[Scale, Exotic]),
    s("Spanish 8-Tone", &[0, 1, 3, 4, 5, 6, 8, 10], &[Scale, Exotic]),
    s("Whole Tone", &[0, 2, 4, 6, 8, 10], &[Usual, Scale, Symmetrical]),
    s("Diminished (half-whole)", &[0, 1, 3, 4, 6, 7, 9, 10], &[Usual, Scale, Symmetrical, Dominant]),
    s("Diminished (whole-half)", &[0, 2, 3, 5, 6, 8, 9, 11], &[Scale, Symmetrical, Minor]),
    s("Augmented", &[0, 3, 4, 7, 8, 11], &[Scale, Symmetrical]),
    s("Chromatic", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], &[Scale, Symmetrical]),
    s("Bebop Dominant", &[0, 2, 4, 5, 7, 9, 10, 11], &[Scale, Bebop, Dominant]),
    s("Bebop Major", &[0, 2, 4, 5, 7, 8, 9, 11], &[Scale, Bebop, Major]),
    s("Bebop Dorian", &[0, 2, 3, 4, 5, 7, 9, 10], &[Scale, Bebop, Minor]),
    s("Bebop Melodic Minor", &[0, 2, 3, 5, 7, 8, 9, 11], &[Scale, Bebop, Minor, Melodic]),
    s("Suspended Pentatonic (Egyptian)", &[0, 2, 5, 7, 10], &[Scale, Pentatonic, Exotic]),
    s("Blues Minor Pentatonic (Man Gong)", &[0, 3, 5, 8, 10], &[Scale, Pentatonic, Minor]),
    s("Blues Major Pentatonic (Ritsusen)", &[0, 2, 5, 7, 9], &[Scale, Pentatonic, Major]),
    s("Dominant Pentatonic", &[0, 2, 4, 7, 10], &[Scale, Pentatonic, Dominant]),
    s("Hirajoshi", &[0, 2, 3, 7, 8], &[Scale, Pentatonic, Exotic]),
    s("In Sen", &[0, 1, 5, 7, 10], &[Scale, Pentatonic, Exotic]),
    s("Iwato", &[0, 1, 5, 6, 10], &[Scale, Pentatonic, Exotic]),
    s("Kumoi", &[0, 2, 3, 7, 9], &[Scale, Pentatonic, Exotic]),
    s("Prometheus", &[0, 2, 4, 6, 9, 10], &[Scale, Exotic]),
    s("Major Triad", &[0, 4, 7], &[Usual, Arpeggio, Triad, Major]),
    s("Minor Triad", &[0, 3, 7], &[Usual, Arpeggio, Triad, Minor]),
    s("Diminished Triad", &[0, 3, 6], &[Arpeggio, Triad, Symmetrical]),
    s("Augmented Triad", &[0, 4, 8], &[Arpeggio, Triad, Symmetrical]),
    s("Major 7th Arpeggio", &[0, 4, 7, 11], &[Usual, Arpeggio, Major]),
    s("Dominant 7th Arpeggio", &[0, 4, 7, 10], &[Usual, Arpeggio, Dominant]),
    s("Minor 7th Arpeggio", &[0, 3, 7, 10], &[Usual, Arpeggio, Minor]),
    s("Half-Diminished Arpeggio", &[0, 3, 6, 10], &[Arpeggio, Minor]),
    s("Diminished 7th Arpeggio", &[0, 3, 6, 9], &[Arpeggio, Symmetrical]),
    s("Major Tetrachord", &[0, 2, 4, 5], &[Tetrachord, Major]),
    s("Minor Tetrachord", &[0, 2, 3, 5], &[Tetrachord, Minor]),
    s("Phrygian Tetrachord", &[0, 1, 3, 5], &[Tetrachord, Minor]),
    s("Harmonic Tetrachord", &[0, 1, 4, 5], &[Tetrachord, Harmonic]),
];

/// A scale the finder proposes: its key note, its index in [`SCALES`], and the share of the
/// passage's notes (weighted by length) it holds, in percent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Found {
    pub root: u8,
    pub scale: usize,
    pub percent: f32,
}

/// The scales a passage could use, most likely first: every full scale (no arpeggios, triads,
/// tetrachords or the chromatic scale) on every key note, or only on `key`. `weights` is how long
/// each pitch class sounds. A scale holding more of the passage ranks higher; among equals, the
/// one whose notes the passage uses more, then the usual ones.
pub fn find_scales(weights: &[f32; 12], key: Option<u8>, limit: usize) -> Vec<Found> {
    let total: f32 = weights.iter().sum();
    if total <= 0.0 || !total.is_finite() {
        return Vec::new();
    }
    let used: u16 = weights.iter().enumerate().filter(|(_, w)| **w > 0.0).fold(0, |m, (i, _)| m | 1 << i);
    let mut out: Vec<(Found, f32, bool)> = Vec::new();
    for (si, scale) in SCALES.iter().enumerate() {
        if !scale.has(Tag::Scale) || scale.intervals.len() >= 12 {
            continue;
        }
        for root in 0..12u8 {
            if key.is_some_and(|k| k % 12 != root) {
                continue;
            }
            let mask = scale.mask(root);
            let inside: f32 = weights.iter().enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, w)| *w).sum();
            let fill = (mask & used).count_ones() as f32 / mask.count_ones().max(1) as f32;
            out.push((Found { root, scale: si, percent: (inside / total * 100.0).clamp(0.0, 100.0) }, fill, scale.has(Tag::Usual)));
        }
    }
    out.sort_by(|a, b| {
        b.0.percent
            .total_cmp(&a.0.percent)
            .then_with(|| b.1.total_cmp(&a.1))
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| a.0.scale.cmp(&b.0.scale))
            .then_with(|| a.0.root.cmp(&b.0.root))
    });
    // Ionian and Aeolian repeat Major and Natural Minor note for note.
    let mut seen: Vec<u16> = Vec::new();
    out.into_iter()
        .filter(|(f, ..)| {
            let mask = SCALES.get(f.scale).map_or(0, |s| s.mask(f.root));
            let dup = seen.contains(&mask) && SCALES.get(f.scale).is_some_and(|s| s.has(Tag::Mode));
            seen.push(mask);
            !dup
        })
        .map(|(f, ..)| f)
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> &'static Scale {
        SCALES.iter().find(|s| s.name.starts_with(name)).unwrap()
    }

    #[test]
    fn the_library_is_well_formed() {
        for s in SCALES {
            assert_eq!(s.intervals.first(), Some(&0), "{}", s.name);
            assert!(s.intervals.windows(2).all(|w| w[0] < w[1]) && s.intervals.iter().all(|i| *i < 12), "{}", s.name);
            assert_eq!(s.steps().iter().map(|x| u32::from(*x)).sum::<u32>(), 12, "{}", s.name);
            assert_eq!(s.interval_names().len(), s.intervals.len() + 1);
            assert!(!s.tags.is_empty());
        }
        let names: std::collections::BTreeSet<&str> = SCALES.iter().map(|s| s.name).collect();
        assert_eq!(names.len(), SCALES.len(), "names are unique");
    }

    #[test]
    fn dorian_on_f_reads_as_the_manual_shows_it() {
        let d = named("Dorian (2nd");
        assert_eq!(d.notes(5, true), vec!["F", "G", "Ab", "Bb", "C", "D", "Eb"]);
        assert_eq!(d.notes(5, false), d.notes(5, true), "the preference does not respell a seven-note scale");
        assert_eq!(named("Major").notes(6, false), vec!["F#", "G#", "A#", "B", "C#", "D#", "E#"]);
        assert_eq!(named("Major").notes(6, true), vec!["Gb", "Ab", "Bb", "Cb", "Db", "Eb", "F"]);
        assert_eq!(named("Minor Pentatonic").notes(9, false), vec!["A", "C", "D", "E", "G"]);
        assert_eq!(d.interval_names(), vec!["P1", "M2", "m3", "P4", "P5", "M6", "m7", "P8"]);
        assert_eq!(d.steps(), vec![2, 1, 2, 2, 2, 1, 2]);
        assert_eq!(named("Locrian (7th").interval_names()[4], "d5");
        assert_eq!(named("Lydian (4th").interval_names()[3], "A4");
    }

    #[test]
    fn a_passage_finds_its_scale() {
        // E minor pentatonic riff with a passing F# (E Aeolian / G major also hold it all).
        let mut w = [0.0f32; 12];
        for (pc, len) in [(4, 4.0), (7, 2.0), (9, 2.0), (11, 2.0), (2, 2.0), (6, 0.5)] {
            w[pc] += len;
        }
        let found = find_scales(&w, None, 10);
        assert!(found[0].percent > 99.9);
        let top: Vec<(u8, &str)> = found.iter().take(4).map(|f| (f.root, SCALES[f.scale].name)).collect();
        assert!(top.iter().any(|(r, n)| (*r == 7 && *n == "Major") || (*r == 4 && *n == "Natural Minor")), "{top:?}");
        // Limited to E as the key note.
        let on_e = find_scales(&w, Some(4), 3);
        assert!(on_e.iter().all(|f| f.root == 4));
        assert_eq!(SCALES[on_e[0].scale].name, "Natural Minor");
        // Pure pentatonic notes rank the pentatonic first: every one of its notes is used.
        let mut p = [0.0f32; 12];
        for pc in [4, 7, 9, 11, 2] {
            p[pc] = 1.0;
        }
        let f = find_scales(&p, Some(4), 1);
        assert_eq!(SCALES[f[0].scale].name, "Minor Pentatonic");
        assert!(find_scales(&[0.0; 12], None, 5).is_empty());
        assert!(find_scales(&[f32::NAN; 12], None, 5).is_empty());
    }
}
