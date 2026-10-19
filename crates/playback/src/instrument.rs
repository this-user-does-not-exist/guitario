//! Which modelled instrument plays a track, from its General MIDI program: the plucked string
//! for guitars, basses and everything without a model of its own; the grand piano; the violin
//! family, bowed or plucked, alone or as a section.

use crate::body::BodyKind;
use crate::piano::PianoTone;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Voicing {
    Plucked,
    Piano(PianoTone),
    /// Bowed by one player (`players` 1) or a section, its notes on the body that suits their
    /// pitch when `body` is `None`. A tremolo section changes bow many times a second; a slow
    /// one swells into each note.
    Bowed {
        body: Option<BodyKind>,
        players: usize,
        tremolo: bool,
        slow: bool,
    },
    /// Plucked with a finger on a bowed instrument.
    Pizzicato {
        body: Option<BodyKind>,
        players: usize,
    },
}

/// Players in a section: each note is played by this many, a little apart in time and tuning.
const SECTION: usize = 4;

impl Voicing {
    pub(crate) fn of(program: u8) -> Voicing {
        let solo = |body| Voicing::Bowed { body: Some(body), players: 1, tremolo: false, slow: false };
        let section = |tremolo, slow| Voicing::Bowed { body: None, players: SECTION, tremolo, slow };
        match program {
            0 => Voicing::Piano(PianoTone::GRAND),
            // A bright grand; an electric grand is a piano with strings and pickups.
            1 | 2 => Voicing::Piano(PianoTone::BRIGHT),
            3 => Voicing::Piano(PianoTone::HONKY_TONK),
            40 | 110 => solo(BodyKind::Violin),
            41 => solo(BodyKind::Viola),
            42 => solo(BodyKind::Cello),
            43 => solo(BodyKind::DoubleBass),
            44 => section(true, false),
            45 => Voicing::Pizzicato { body: None, players: 3 },
            48 => section(false, false),
            49 => section(false, true),
            _ => Voicing::Plucked,
        }
    }

    /// The bodies a track of this instrument needs, with their desks: a section has two of each,
    /// its left and right halves on instruments of their own.
    pub(crate) fn bodies(self) -> Vec<(BodyKind, Option<u8>)> {
        let kinds = match self {
            Voicing::Plucked => Vec::new(),
            Voicing::Piano(_) => vec![BodyKind::Piano],
            Voicing::Bowed { body: Some(b), .. } | Voicing::Pizzicato { body: Some(b), .. } => vec![b],
            Voicing::Bowed { body: None, .. } | Voicing::Pizzicato { body: None, .. } => {
                vec![BodyKind::Violin, BodyKind::Viola, BodyKind::Cello, BodyKind::DoubleBass]
            }
        };
        let section = matches!(self, Voicing::Bowed { players: 2.., .. } | Voicing::Pizzicato { players: 2.., .. });
        if section { kinds.into_iter().flat_map(|k| [(k, Some(0)), (k, Some(1))]).collect() } else { kinds.into_iter().map(|k| (k, None)).collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_choose_their_models() {
        assert_eq!(Voicing::of(0), Voicing::Piano(PianoTone::GRAND));
        assert_eq!(Voicing::of(42), Voicing::Bowed { body: Some(BodyKind::Cello), players: 1, tremolo: false, slow: false });
        assert!(matches!(Voicing::of(48), Voicing::Bowed { body: None, players: 4, .. }));
        assert!(matches!(Voicing::of(45), Voicing::Pizzicato { .. }));
        for p in [24, 25, 29, 30, 33, 105] {
            assert_eq!(Voicing::of(p), Voicing::Plucked, "{p}");
        }
        assert_eq!(Voicing::of(48).bodies().len(), 8, "two desks of four instruments");
        assert_eq!(Voicing::of(40).bodies(), vec![(BodyKind::Violin, None)]);
        assert!(Voicing::Plucked.bodies().is_empty());
    }
}
