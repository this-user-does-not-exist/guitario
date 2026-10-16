//! Chord names from notes: the pitch classes sounding together, matched against the chord shapes a
//! guitarist names (triads, power chords, sixths, sevenths, ninths, suspensions), with a slash bass
//! when the lowest note is not the root.

/// Chord shapes as intervals above the root (a bit per semitone) and their suffix, most common
/// first: the first shape that matches names the chord.
const SHAPES: &[(u16, &str)] = &[
    (bits(&[0, 4, 7]), ""),
    (bits(&[0, 3, 7]), "m"),
    (bits(&[0, 7]), "5"),
    (bits(&[0, 4, 7, 10]), "7"),
    (bits(&[0, 3, 7, 10]), "m7"),
    (bits(&[0, 4, 7, 11]), "maj7"),
    (bits(&[0, 2, 7]), "sus2"),
    (bits(&[0, 5, 7]), "sus4"),
    (bits(&[0, 5, 7, 10]), "7sus4"),
    (bits(&[0, 2, 4, 7]), "add9"),
    (bits(&[0, 2, 3, 7]), "madd9"),
    (bits(&[0, 4, 7, 9]), "6"),
    (bits(&[0, 3, 7, 9]), "m6"),
    (bits(&[0, 2, 4, 7, 9]), "6/9"),
    (bits(&[0, 2, 4, 7, 10]), "9"),
    (bits(&[0, 2, 3, 7, 10]), "m9"),
    (bits(&[0, 2, 4, 7, 11]), "maj9"),
    (bits(&[0, 3, 6]), "dim"),
    (bits(&[0, 3, 6, 9]), "dim7"),
    (bits(&[0, 3, 6, 10]), "m7b5"),
    (bits(&[0, 4, 8]), "aug"),
    (bits(&[0, 4, 8, 10]), "7#5"),
    (bits(&[0, 4, 6, 10]), "7b5"),
    (bits(&[0, 1, 4, 7, 10]), "7b9"),
    (bits(&[0, 3, 4, 7, 10]), "7#9"),
    (bits(&[0, 3, 7, 11]), "mMaj7"),
    (bits(&[0, 2, 4, 5, 7, 10]), "11"),
    (bits(&[0, 2, 3, 5, 7, 10]), "m11"),
    // Guitar voicings often leave the fifth out.
    (bits(&[0, 4, 10]), "7"),
    (bits(&[0, 3, 10]), "m7"),
    (bits(&[0, 4, 11]), "maj7"),
    (bits(&[0, 2, 4, 10]), "9"),
    (bits(&[0, 4]), "(no5)"),
    (bits(&[0, 3]), "m(no5)"),
];

const fn bits(intervals: &[u16]) -> u16 {
    match intervals.split_first() {
        Some((first, rest)) => 1 << *first | bits(rest),
        None => 0,
    }
}

const SHARPS: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
const FLATS: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B"];

/// The pitch-class name of `class` (0 is C), spelled with flats or sharps.
pub fn pitch_class_name(class: u8, flats: bool) -> &'static str {
    let names = if flats { &FLATS } else { &SHARPS };
    names.get(usize::from(class % 12)).copied().unwrap_or("?")
}

/// Names the chord `pitches` (MIDI numbers, in any order and octave) make: `E5`, `Am7`, `D/F#`.
/// The lowest note is tried as the root first; another root gets a slash bass. `None` for fewer
/// than two pitch classes or a set no shape matches.
pub fn chord_name(pitches: &[u8], flats: bool) -> Option<String> {
    let bass = pitches.iter().min()? % 12;
    let set = pitches.iter().fold(0u16, |b, p| b | 1 << (p % 12));
    if set.count_ones() < 2 {
        return None;
    }
    // The set seen from `root`: bit k is the pitch class k semitones above it.
    let from = |root: u8| -> u16 { (set >> root | set << (12 - u16::from(root))) & 0x0FFF };
    let name = |root: u8, suffix: &str| format!("{}{suffix}", pitch_class_name(root, flats));
    if let Some((_, suffix)) = SHAPES.iter().find(|(shape, _)| *shape == from(bass)) {
        return Some(name(bass, suffix));
    }
    for (shape, suffix) in SHAPES {
        if let Some(root) = (0..12u8).map(|k| (bass + k) % 12).find(|r| set & 1 << r != 0 && from(*r) == *shape) {
            return Some(format!("{}/{}", name(root, suffix), pitch_class_name(bass, flats)));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Standard tuning, low E to high E.
    const OPEN: [u8; 6] = [40, 45, 50, 55, 59, 64];

    /// Pitches of a shape written low string first, `None` for a string not played.
    fn shape(frets: [Option<u8>; 6]) -> Vec<u8> {
        OPEN.iter().zip(frets).filter_map(|(o, f)| f.map(|f| o + f)).collect()
    }

    #[test]
    fn open_chords_power_chords_and_barres() {
        let n = None;
        assert_eq!(chord_name(&shape([Some(0), Some(2), Some(2), Some(1), Some(0), Some(0)]), false).as_deref(), Some("E"));
        assert_eq!(chord_name(&shape([n, Some(0), Some(2), Some(2), Some(1), Some(0)]), false).as_deref(), Some("Am"));
        assert_eq!(chord_name(&shape([n, n, Some(0), Some(2), Some(3), Some(2)]), false).as_deref(), Some("D"));
        assert_eq!(chord_name(&shape([n, Some(3), Some(2), Some(0), Some(1), Some(0)]), false).as_deref(), Some("C"));
        assert_eq!(chord_name(&shape([Some(3), Some(2), Some(0), Some(0), Some(0), Some(3)]), false).as_deref(), Some("G"));
        assert_eq!(chord_name(&shape([Some(0), Some(2), Some(2), n, n, n]), false).as_deref(), Some("E5"));
        assert_eq!(chord_name(&shape([n, Some(5), Some(7), Some(7), Some(7), Some(5)]), false).as_deref(), Some("D"));
        assert_eq!(chord_name(&shape([Some(1), Some(3), Some(3), Some(2), Some(1), Some(1)]), false).as_deref(), Some("F"));
        assert_eq!(chord_name(&shape([n, Some(0), Some(2), Some(0), Some(2), Some(0)]), false).as_deref(), Some("A7"));
        assert_eq!(chord_name(&shape([n, Some(0), Some(2), Some(1), Some(2), Some(0)]), false).as_deref(), Some("Amaj7"));
        assert_eq!(chord_name(&shape([n, n, Some(0), Some(2), Some(3), Some(3)]), false).as_deref(), Some("Dsus4"));
        assert_eq!(chord_name(&shape([n, Some(3), Some(2), Some(0), Some(3), Some(0)]), false).as_deref(), Some("Cadd9"));
    }

    #[test]
    fn the_bass_note_decides_between_names_of_one_set() {
        // C E G A: C6 over C, Am7 over A.
        assert_eq!(chord_name(&[48, 52, 55, 57], false).as_deref(), Some("C6"));
        assert_eq!(chord_name(&[45, 48, 52, 55], false).as_deref(), Some("Am7"));
        // D/F#: a first inversion gets a slash bass.
        assert_eq!(chord_name(&[42, 50, 57, 62, 66], false).as_deref(), Some("D/F#"));
        // An inverted power chord.
        assert_eq!(chord_name(&[45, 50], false).as_deref(), Some("D5/A"));
    }

    #[test]
    fn flats_and_what_cannot_be_named() {
        assert_eq!(chord_name(&[46, 53, 58], true).as_deref(), Some("Bb5"));
        assert_eq!(chord_name(&[46, 53, 58], false).as_deref(), Some("A#5"));
        assert_eq!(chord_name(&[], false), None);
        assert_eq!(chord_name(&[40, 52, 64], false), None, "one pitch class");
        assert_eq!(chord_name(&[60, 61, 62], false), None, "a cluster");
        // Any byte is a pitch class; nothing panics.
        for p in 0..=255u8 {
            let _ = chord_name(&[p, p.wrapping_add(7), 255], p % 2 == 0);
        }
    }
}
