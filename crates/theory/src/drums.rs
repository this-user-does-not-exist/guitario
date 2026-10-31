//! The General MIDI drum kit as Guitario uses it: which lane of a drum track each piece sits on,
//! where standard drum notation places it, which notehead it takes, and the articulations Guitar
//! Pro 7 adds to it (rim shot, half-open hi-hat, ride edge, chokes).
//!
//! A drum track stores each hit as a note whose fret is the General MIDI drum number and whose
//! string is the piece's lane, so two pieces that are played together (kick, snare, hi-hat, crash)
//! always land on different lanes. Staff positions follow the common drum-set notation used by
//! Guitar Pro and most method books (kick in the bottom space, snare on the third space, hi-hat
//! above the staff).

/// Lanes of a drum track, top to bottom, like the lines of Guitar Pro's drum tab.
pub const LANES: [&str; 8] = ["Cymbals", "Ride", "Hi-Hat", "Toms", "Snare", "Floor Tom", "Pedal Hi-Hat", "Kick"];

/// A drum notehead, as Guitar Pro's kit map draws each articulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Head {
    Normal,
    /// A cross: hi-hats, rides, the side stick.
    X,
    /// A heavier cross for the crashes.
    HeavyX,
    /// A cross with a hat over it: the china.
    HeavyXHat,
    /// A cross in a circle: the open hi-hat.
    CircleX,
    /// A slashed circle: the half-open hi-hat.
    CircleSlash,
    /// An open diamond: the ride bell, the rim shot.
    Diamond,
    /// A triangle: cowbells.
    Triangle,
}

/// A sign Guitar Pro prints beside an articulation's head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// A choked cymbal: a staccato dot above the head, as Guitar Pro writes it.
    Choke,
    /// The edge of a cymbal, above the staff.
    Edge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrumPiece {
    /// The General MIDI drum number, or the number Guitar Pro 7 gives an articulation that General
    /// MIDI lacks (91 and up: rim shot, half-open hi-hat, ride edge, the chokes).
    pub midi: u8,
    pub name: &'static str,
    /// Two- or three-letter label for tab and pads.
    pub short: &'static str,
    pub lane: u8,
    /// Half-space steps above the bottom line of a five-line staff (Guitar Pro's `StaffLine` is
    /// `8 - staff_pos`).
    pub staff_pos: i8,
    pub head: Head,
    pub mark: Option<Mark>,
    /// The General MIDI drum that sounds it (itself for a General MIDI piece).
    pub plays: u8,
    /// The kit piece this is an articulation of (a choke of its crash), if it is one.
    pub variant_of: Option<u8>,
}

impl DrumPiece {
    /// Cymbals and hi-hats take a cross-like head.
    pub fn cross(&self) -> bool {
        matches!(self.head, Head::X | Head::HeavyX | Head::HeavyXHat | Head::CircleX | Head::CircleSlash)
    }

    /// A choked cymbal stops at the end of its note.
    pub fn choked(&self) -> bool {
        self.mark == Some(Mark::Choke)
    }
}

const fn p(midi: u8, name: &'static str, short: &'static str, lane: u8, staff_pos: i8, head: Head) -> DrumPiece {
    DrumPiece { midi, name, short, lane, staff_pos, head, mark: None, plays: midi, variant_of: None }
}

/// An articulation Guitar Pro 7 numbers outside General MIDI: played as `of` with `mark`.
const fn art(midi: u8, name: &'static str, short: &'static str, of: DrumPiece, head: Head, mark: Option<Mark>) -> DrumPiece {
    DrumPiece { midi, name, short, lane: of.lane, staff_pos: of.staff_pos, head, mark, plays: of.midi, variant_of: Some(of.midi) }
}

const SNARE: DrumPiece = p(38, "Snare", "SD", 4, 5, Head::Normal);
const HAT_OPEN: DrumPiece = p(46, "Hi-Hat Open", "OH", 2, 9, Head::CircleX);
const CRASH: DrumPiece = p(49, "Crash", "CR", 0, 10, Head::HeavyX);
const CRASH_2: DrumPiece = p(57, "Crash 2", "CR2", 0, 9, Head::HeavyX);
const CHINA: DrumPiece = p(52, "China", "CH", 0, 11, Head::HeavyXHat);
const SPLASH: DrumPiece = p(55, "Splash", "SP", 0, 10, Head::X);
const RIDE: DrumPiece = p(51, "Ride", "RD", 1, 8, Head::X);

/// How many pieces have a pad of their own (the rest are their articulations).
pub const PADS: usize = 20;

/// The pieces a metal or rock kit uses, in pad order, then the articulations Guitar Pro 7 adds
/// to them. Staff lines and heads follow Guitar Pro 7's drum kit.
pub const KIT: [DrumPiece; 28] = [
    p(36, "Kick", "BD", 7, 1, Head::Normal),
    p(35, "Kick 2", "BD2", 7, 0, Head::Normal),
    SNARE,
    p(37, "Side Stick", "SS", 4, 5, Head::X),
    p(40, "Snare (rim)", "SR", 4, 5, Head::Normal),
    p(42, "Hi-Hat Closed", "HH", 2, 9, Head::X),
    HAT_OPEN,
    p(44, "Hi-Hat Pedal", "PH", 6, -1, Head::X),
    CRASH,
    CRASH_2,
    CHINA,
    SPLASH,
    RIDE,
    p(53, "Ride Bell", "RB", 1, 8, Head::Diamond),
    p(50, "High Tom", "T1", 3, 7, Head::Normal),
    p(48, "Mid Tom", "T2", 3, 6, Head::Normal),
    p(47, "Low-Mid Tom", "T3", 3, 4, Head::Normal),
    p(45, "Low Tom", "T4", 5, 3, Head::Normal),
    p(43, "Floor Tom", "FT", 5, 2, Head::Normal),
    p(41, "Low Floor Tom", "FT2", 5, 3, Head::Normal),
    art(91, "Snare (rim shot)", "RS", SNARE, Head::Diamond, None),
    art(92, "Hi-Hat Half", "HO", HAT_OPEN, Head::CircleSlash, None),
    art(93, "Ride (edge)", "RE", RIDE, Head::X, Some(Mark::Edge)),
    art(94, "Ride (choke)", "RC", RIDE, Head::X, Some(Mark::Choke)),
    art(95, "Splash (choke)", "SPC", SPLASH, Head::X, Some(Mark::Choke)),
    art(96, "China (choke)", "CHC", CHINA, Head::HeavyXHat, Some(Mark::Choke)),
    art(97, "Crash (choke)", "CRC", CRASH, Head::HeavyX, Some(Mark::Choke)),
    art(98, "Crash 2 (choke)", "C2C", CRASH_2, Head::HeavyX, Some(Mark::Choke)),
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

/// The articulations of the piece `midi`, the piece itself first.
pub fn articulations(midi: u8) -> impl Iterator<Item = &'static DrumPiece> {
    KIT.iter().filter(move |p| p.midi == midi || p.variant_of == Some(midi))
}

/// The General MIDI drum that sounds `midi`: itself unless it is one of Guitar Pro's articulations.
pub fn plays(midi: u8) -> u8 {
    piece(midi).map_or(midi, |p| p.plays)
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
        assert!(piece(42).is_some_and(DrumPiece::cross) && piece(38).is_some_and(|p| !p.cross()));
    }

    #[test]
    fn short_names_are_unique() {
        for (i, a) in KIT.iter().enumerate() {
            assert!(KIT.iter().skip(i + 1).all(|b| !b.short.eq_ignore_ascii_case(a.short) && b.midi != a.midi), "{}", a.name);
        }
    }

    #[test]
    fn articulations_sit_with_their_piece_and_sound_as_it() {
        // Guitar Pro 7's numbers, staff lines (8 - staff_pos) and heads from its drum kit.
        for (midi, line, plays, head, mark) in [
            (91, 3, 38, Head::Diamond, None),
            (92, -1, 46, Head::CircleSlash, None),
            (93, 0, 51, Head::X, Some(Mark::Edge)),
            (94, 0, 51, Head::X, Some(Mark::Choke)),
            (95, -2, 55, Head::X, Some(Mark::Choke)),
            (96, -3, 52, Head::HeavyXHat, Some(Mark::Choke)),
            (97, -2, 49, Head::HeavyX, Some(Mark::Choke)),
            (98, -1, 57, Head::HeavyX, Some(Mark::Choke)),
        ] {
            let a = piece(midi).unwrap();
            let of = piece(plays).unwrap();
            assert_eq!((8 - a.staff_pos, a.plays, a.head, a.mark, a.lane), (line, plays, head, mark, of.lane), "{}", a.name);
            assert_eq!(a.variant_of, Some(plays));
            assert!(articulations(plays).any(|x| x.midi == midi) && articulations(plays).next() == Some(of));
        }
        assert_eq!(plays(42), 42);
        assert_eq!(plays(56), 56, "unknown percussion sounds as itself");
        assert!(piece(97).is_some_and(DrumPiece::choked) && !piece(49).is_some_and(DrumPiece::choked));
        // The pads are the pieces; their articulations come after them.
        assert!(KIT.iter().take(PADS).all(|p| p.variant_of.is_none()) && KIT.iter().skip(PADS).all(|p| p.variant_of.is_some()));
    }
}
