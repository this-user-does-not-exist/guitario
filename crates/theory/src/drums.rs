//! The General MIDI drum kit as Guitario uses it: which lane of a drum track each piece sits on,
//! where standard drum notation places it, and whether it takes a cross notehead.
//!
//! A drum track stores each hit as a note whose fret is the General MIDI drum number and whose
//! string is the piece's lane, so two pieces that are played together (kick, snare, hi-hat, crash)
//! always land on different lanes. Staff positions follow the common drum-set notation used by
//! Guitar Pro and most method books (kick in the bottom space, snare on the third space, hi-hat
//! above the staff).

/// Lanes of a drum track, top to bottom, like the lines of Guitar Pro's drum tab.
pub const LANES: [&str; 8] = ["Cymbals", "Ride", "Hi-Hat", "Toms", "Snare", "Floor Tom", "Pedal Hi-Hat", "Kick"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrumPiece {
    pub midi: u8,
    pub name: &'static str,
    /// Two- or three-letter label for tab and pads.
    pub short: &'static str,
    pub lane: u8,
    /// Half-space steps above the bottom line of a five-line staff.
    pub staff_pos: i8,
    /// Cymbals and hi-hats take a cross notehead.
    pub cross: bool,
}

const fn p(midi: u8, name: &'static str, short: &'static str, lane: u8, staff_pos: i8, cross: bool) -> DrumPiece {
    DrumPiece { midi, name, short, lane, staff_pos, cross }
}

/// The pieces a metal or rock kit uses, in pad order.
pub const KIT: [DrumPiece; 20] = [
    p(36, "Kick", "BD", 7, 1, false),
    p(35, "Kick 2", "BD2", 7, 0, false),
    p(38, "Snare", "SD", 4, 5, false),
    p(37, "Side Stick", "SS", 4, 5, true),
    p(40, "Snare (rim)", "SR", 4, 5, false),
    p(42, "Hi-Hat Closed", "HH", 2, 9, true),
    p(46, "Hi-Hat Open", "OH", 2, 9, true),
    p(44, "Hi-Hat Pedal", "PH", 6, -1, true),
    p(49, "Crash", "CR", 0, 10, true),
    p(57, "Crash 2", "CR2", 0, 11, true),
    p(52, "China", "CH", 0, 11, true),
    p(55, "Splash", "SP", 0, 10, true),
    p(51, "Ride", "RD", 1, 8, true),
    p(53, "Ride Bell", "RB", 1, 8, true),
    p(50, "High Tom", "T1", 3, 7, false),
    p(48, "Mid Tom", "T2", 3, 6, false),
    p(47, "Low-Mid Tom", "T3", 3, 6, false),
    p(45, "Low Tom", "T4", 5, 4, false),
    p(43, "Floor Tom", "FT", 5, 3, false),
    p(41, "Low Floor Tom", "FT2", 5, 2, false),
];

/// The piece for a General MIDI drum number, if the kit knows it.
pub fn piece(midi: u8) -> Option<&'static DrumPiece> {
    KIT.iter().find(|p| p.midi == midi)
}

/// The lane a drum number goes on. Unknown percussion (cowbell, claps …) shares the toms lane.
pub fn lane_for(midi: u8) -> u8 {
    piece(midi).map_or(3, |p| p.lane)
}

/// A piece by its short label or name, case-insensitively (`"bd"`, `"snare"`).
pub fn by_name(name: &str) -> Option<&'static DrumPiece> {
    KIT.iter().find(|p| p.short.eq_ignore_ascii_case(name) || p.name.eq_ignore_ascii_case(name))
}

/// Highest General MIDI drum number a note may hold.
pub const MAX_DRUM: u8 = 127;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_basic_beat_never_collides() {
        // Kick, snare, hi-hat and crash are played together; they must be on different lanes.
        let lanes: Vec<u8> = [36, 38, 42, 49, 44, 51].iter().map(|m| lane_for(*m)).collect();
        let mut sorted = lanes.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), lanes.len(), "{lanes:?}");
    }

    #[test]
    fn lookups() {
        assert_eq!(piece(36).map(|p| p.name), Some("Kick"));
        assert_eq!(by_name("sd").map(|p| p.midi), Some(38));
        assert_eq!(by_name("Ride").map(|p| p.midi), Some(51));
        assert_eq!(lane_for(56), 3);
        assert!(KIT.iter().all(|p| usize::from(p.lane) < LANES.len()));
        assert!(piece(42).is_some_and(|p| p.cross) && piece(38).is_some_and(|p| !p.cross));
    }

    #[test]
    fn short_names_are_unique() {
        for (i, a) in KIT.iter().enumerate() {
            assert!(KIT.iter().skip(i + 1).all(|b| !b.short.eq_ignore_ascii_case(a.short) && b.midi != a.midi), "{}", a.name);
        }
    }
}
