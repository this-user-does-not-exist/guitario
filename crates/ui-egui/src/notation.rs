//! Standard-notation maths with no drawing: pitch spelling, staff positions, key signatures and
//! accidentals.

/// A clef, with the MIDI pitch of its bottom line as *written*. Guitar is written an octave above
/// sounding pitch (treble clef), bass guitar likewise in bass clef.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clef {
    Treble,
    Bass,
}

impl Clef {
    /// Diatonic index (octave × 7 + step) of the bottom staff line: E4 for treble, G2 for bass.
    fn bottom_line(self) -> i32 {
        match self {
            Clef::Treble => 4 * 7 + 2,
            Clef::Bass => 2 * 7 + 4,
        }
    }

    /// Chosen from the *highest* open string: a bass guitar tops out at G2 (43) or C3 (48), while
    /// even a 7-string in drop A still has a high E on top and is written in treble clef.
    pub fn for_tuning(highest_open: u8) -> Clef {
        if highest_open < 52 { Clef::Bass } else { Clef::Treble }
    }

    /// Staff positions of the key-signature accidentals (sharps or flats) in order.
    pub fn key_positions(self, sharps: bool) -> [i32; 7] {
        let treble_sharps = [8, 5, 9, 6, 3, 7, 4];
        let treble_flats = [4, 7, 3, 6, 2, 5, 1];
        let base = if sharps { treble_sharps } else { treble_flats };
        match self {
            Clef::Treble => base,
            Clef::Bass => base.map(|p| p - 2),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spelled {
    /// 0 = C … 6 = B.
    pub step: u8,
    pub octave: i32,
    /// -1 flat, 0 natural, +1 sharp.
    pub alter: i8,
}

/// Spells a MIDI pitch, preferring sharps in sharp keys (and C) and flats in flat keys.
pub fn spell(midi: u8, key: i8) -> Spelled {
    const SHARPS: [(u8, i8); 12] = [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (3, 0), (3, 1), (4, 0), (4, 1), (5, 0), (5, 1), (6, 0)];
    const FLATS: [(u8, i8); 12] = [(0, 0), (1, -1), (1, 0), (2, -1), (2, 0), (3, 0), (4, -1), (4, 0), (5, -1), (5, 0), (6, -1), (6, 0)];
    let pc = usize::from(midi % 12);
    let (step, alter) = if key < 0 { FLATS.get(pc) } else { SHARPS.get(pc) }.copied().unwrap_or((0, 0));
    Spelled { step, octave: i32::from(midi / 12) - 1, alter }
}

/// Half-space steps above the bottom staff line (0 = bottom line, 8 = top line).
pub fn staff_pos(s: Spelled, clef: Clef) -> i32 {
    s.octave * 7 + i32::from(s.step) - clef.bottom_line()
}

/// The alteration the key signature gives a step.
pub fn key_alter(key: i8, step: u8) -> i8 {
    const SHARP_ORDER: [u8; 7] = [3, 0, 4, 1, 5, 2, 6]; // F C G D A E B
    const FLAT_ORDER: [u8; 7] = [6, 2, 5, 1, 4, 0, 3]; // B E A D G C F
    let n = usize::from(key.unsigned_abs().min(7));
    if key > 0 && SHARP_ORDER.iter().take(n).any(|s| *s == step) {
        1
    } else if key < 0 && FLAT_ORDER.iter().take(n).any(|s| *s == step) {
        -1
    } else {
        0
    }
}

/// Tracks accidentals within one measure so each altered note shows a sign only when needed.
#[derive(Default)]
pub struct Accidentals {
    seen: Vec<((u8, i32), i8)>,
}

impl Accidentals {
    /// The sign to draw (`Some(-1|0|1)`) or `None` when the current state already implies it.
    pub fn next(&mut self, s: Spelled, key: i8) -> Option<i8> {
        let k = (s.step, s.octave);
        let current = self.seen.iter().find(|(kk, _)| *kk == k).map_or(key_alter(key, s.step), |(_, a)| *a);
        if current == s.alter {
            return None;
        }
        self.seen.retain(|(kk, _)| *kk != k);
        self.seen.push((k, s.alter));
        Some(s.alter)
    }
}

/// Beam groups for one measure: runs of two or more consecutive sounding beats of an eighth or
/// shorter that start in the same beam group of the bar (its time signature's usual grouping, or
/// the bar's own; see `MeasureHeader::beam_starts`). Returns beat indices.
pub fn beam_groups(beats: &[guitario_score::Beat], header: &guitario_score::MeasureHeader) -> Vec<Vec<usize>> {
    let starts = header.beam_starts();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut tick = 0u32;
    let mut prev_unit = None;
    for (i, b) in beats.iter().enumerate() {
        let start = tick;
        tick = tick.saturating_add(b.duration.ticks());
        let beamable = !b.rest && !b.notes.is_empty() && b.duration.value.divisor() >= 8;
        if !beamable {
            prev_unit = None;
            continue;
        }
        let u = starts.partition_point(|s| *s <= start);
        match groups.last_mut() {
            Some(g) if prev_unit == Some(u) && g.last().is_some_and(|&j| j + 1 == i) => g.push(i),
            _ => groups.push(vec![i]),
        }
        prev_unit = Some(u);
    }
    groups.retain(|g| g.len() > 1);
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spelling_and_positions() {
        // Written pitch of open low E on guitar is E3 + 12 = E3 written as E3? Written = sounding + 12.
        let e4 = spell(64, 0);
        assert_eq!((e4.step, e4.octave, e4.alter), (2, 4, 0));
        assert_eq!(staff_pos(e4, Clef::Treble), 0);
        assert_eq!(staff_pos(spell(77, 0), Clef::Treble), 8); // F5 top line
        assert_eq!(spell(61, 0).alter, 1);
        assert_eq!(spell(61, -2), Spelled { step: 1, octave: 4, alter: -1 });
        assert_eq!(staff_pos(spell(43, 0), Clef::Bass), 0); // G2
    }

    #[test]
    fn clef_follows_the_top_string() {
        use guitario_theory::Tuning;
        let top = |t: Tuning| t.strings.iter().copied().max().unwrap_or(0);
        assert_eq!(Clef::for_tuning(top(Tuning::standard())), Clef::Treble);
        assert_eq!(Clef::for_tuning(top(Tuning::seven_string_drop_a())), Clef::Treble);
        assert_eq!(Clef::for_tuning(top(Tuning::drop_b())), Clef::Treble);
        assert_eq!(Clef::for_tuning(top(Tuning::bass())), Clef::Bass);
    }

    #[test]
    fn beams_group_by_beat() {
        use guitario_score::{Beat, Note};
        use guitario_theory::{Duration, NoteValue, TimeSignature};
        let n = |v| Beat { rest: false, notes: vec![Note::new(0, 0)], ..Beat::rest(Duration::new(v)) };
        let beats = vec![
            n(NoteValue::Eighth),
            n(NoteValue::Eighth),
            n(NoteValue::Eighth),
            Beat::rest(Duration::EIGHTH),
            n(NoteValue::Quarter),
            n(NoteValue::Sixteenth),
            n(NoteValue::Sixteenth),
            n(NoteValue::Eighth),
        ];
        let bar = |n, d| guitario_score::MeasureHeader::new(TimeSignature::new(n, d).unwrap());
        assert_eq!(beam_groups(&beats, &bar(4, 4)), vec![vec![0, 1], vec![5, 6, 7]]);
        assert_eq!(beam_groups(&beats[..3], &bar(6, 8)), vec![vec![0, 1, 2]]);
        // 7/8 beams 2+2+3 unless the bar says otherwise.
        let seven: Vec<Beat> = (0..7).map(|_| n(NoteValue::Eighth)).collect();
        assert_eq!(beam_groups(&seven, &bar(7, 8)), vec![vec![0, 1], vec![2, 3], vec![4, 5, 6]]);
        let mut three_first = bar(7, 8);
        three_first.beams = vec![3, 2, 2];
        assert_eq!(beam_groups(&seven, &three_first), vec![vec![0, 1, 2], vec![3, 4], vec![5, 6]]);
    }

    #[test]
    fn key_signatures() {
        assert_eq!(key_alter(1, 3), 1); // F# in G major
        assert_eq!(key_alter(1, 0), 0);
        assert_eq!(key_alter(-1, 6), -1); // Bb in F major
        assert_eq!(key_alter(7, 6), 1);
        assert_eq!(key_alter(i8::MIN, 6), -1);
    }

    #[test]
    fn accidentals_persist_in_a_measure() {
        let mut a = Accidentals::default();
        assert_eq!(a.next(spell(66, 0), 0), Some(1)); // F#
        assert_eq!(a.next(spell(66, 0), 0), None); // F# again
        assert_eq!(a.next(spell(65, 0), 0), Some(0)); // F natural
        let mut g = Accidentals::default();
        assert_eq!(g.next(spell(66, 1), 1), None); // F# in G major
        assert_eq!(g.next(spell(65, 1), 1), Some(0));
    }
}
