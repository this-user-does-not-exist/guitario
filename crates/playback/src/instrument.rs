//! Which modelled instrument plays a track, from its General MIDI program and its players: the
//! plucked string for guitars, basses and everything without a model of its own; the grand piano;
//! the violin family, bowed or plucked, by one player, a section of one instrument, or a string
//! orchestra.

use crate::body::BodyKind;
use crate::piano::PianoTone;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Voicing {
    Plucked,
    Piano(PianoTone),
    /// Bowed by one player (`players` 1) or a section of `players`, or, when `body` is `None`, by
    /// a string orchestra whose sections each take the notes in their range. A tremolo section
    /// changes bow many times a second; a slow one swells into each note.
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

/// The most players a section may have: a symphony orchestra's first violins.
pub(crate) const MAX_PLAYERS: usize = 16;

/// A section on the stage: how many play, and where it sits from the hall, its middle and
/// half its width (-1 is the left of the stereo image, 1 the right).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Section {
    pub players: usize,
    pub centre: f32,
    pub width: f32,
}

impl Section {
    /// A section on a track of its own, across the middle (the track's pan places it).
    pub(crate) fn alone(players: usize) -> Section {
        Section { players: players.clamp(1, MAX_PLAYERS), centre: 0.0, width: 0.5 }
    }

    /// A symphony orchestra's string sections, seated the American way: sixteen violins on the
    /// left, twelve violas in the middle, ten cellos to the right and eight double basses behind
    /// them.
    pub(crate) fn orchestral(kind: BodyKind) -> Section {
        let (players, centre, width) = match kind {
            BodyKind::Violin | BodyKind::Piano => (16, -0.45, 0.4),
            BodyKind::Viola => (12, 0.05, 0.25),
            BodyKind::Cello => (10, 0.4, 0.25),
            BodyKind::DoubleBass => (8, 0.65, 0.2),
        };
        Section { players, centre, width }
    }

    /// How many instruments are heard apart: each its own body, at its own place. Players beyond
    /// them share one (a body costs as much as several strings); four spread a section across
    /// its width and blur any one body's peaks.
    pub(crate) fn desks(self) -> usize {
        match self.players {
            0 | 1 => 1,
            2 | 3 => 2,
            4..=7 => 3,
            _ => 4,
        }
    }

    /// Where its `i`th of `n` instruments sits, spread evenly across it.
    pub(crate) fn place(self, i: usize, n: usize) -> f32 {
        if n <= 1 {
            return self.centre;
        }
        let x = (i as f32 + 0.5) / n as f32 * 2.0 - 1.0;
        (self.centre + self.width * x).clamp(-1.0, 1.0)
    }
}

/// A body on a track: its kind, which instrument of that kind, and its place across the stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Desk {
    pub kind: BodyKind,
    pub which: u32,
    pub place: f32,
}

/// The kinds a string orchestra's notes go to, low to high.
const ORCHESTRA: [BodyKind; 4] = [BodyKind::DoubleBass, BodyKind::Cello, BodyKind::Viola, BodyKind::Violin];

impl Voicing {
    /// The voicing of a General MIDI program played by `players` (0 or 1 is one player; more is a
    /// section on the violin family's programs, and means nothing elsewhere).
    pub(crate) fn of(program: u8, players: u8) -> Voicing {
        let players = usize::from(players).clamp(1, MAX_PLAYERS);
        let one = |body| Voicing::Bowed { body: Some(body), players, tremolo: false, slow: false };
        let orchestra = |tremolo, slow| Voicing::Bowed { body: None, players: 0, tremolo, slow };
        match program {
            0 => Voicing::Piano(PianoTone::GRAND),
            // A bright grand; an electric grand is a piano with strings and pickups.
            1 | 2 => Voicing::Piano(PianoTone::BRIGHT),
            3 => Voicing::Piano(PianoTone::HONKY_TONK),
            40 | 110 => one(BodyKind::Violin),
            41 => one(BodyKind::Viola),
            42 => one(BodyKind::Cello),
            43 => one(BodyKind::DoubleBass),
            44 => orchestra(true, false),
            45 => Voicing::Pizzicato { body: None, players: 0 },
            48 => orchestra(false, false),
            49 => orchestra(false, true),
            _ => Voicing::Plucked,
        }
    }

    /// The section that plays a note on a body of `kind`.
    pub(crate) fn section(self, kind: BodyKind) -> Section {
        match self {
            Voicing::Bowed { body: Some(_), players, .. } | Voicing::Pizzicato { body: Some(_), players } => Section::alone(players),
            _ => Section::orchestral(kind),
        }
    }

    /// The bodies a track of this instrument needs: one for a soloist, a few spread across a
    /// section, a few for each section of an orchestra.
    pub(crate) fn desks(self) -> Vec<Desk> {
        let spread = |kind: BodyKind, section: Section, most: usize| {
            let n = section.desks().min(most);
            let instruments = kind.section_instruments();
            (0..n).map(move |i| Desk {
                kind,
                which: if section.players > 1 { instruments.get(i).copied().unwrap_or(i as u32 + 1) } else { 0 },
                place: section.place(i, n),
            })
        };
        match self {
            Voicing::Plucked => Vec::new(),
            Voicing::Piano(_) => vec![Desk { kind: BodyKind::Piano, which: 0, place: 0.0 }],
            Voicing::Bowed { body: Some(b), .. } | Voicing::Pizzicato { body: Some(b), .. } => spread(b, self.section(b), usize::MAX).collect(),
            // Three a section: an orchestra has four of them on one track.
            Voicing::Bowed { body: None, .. } | Voicing::Pizzicato { body: None, .. } => {
                ORCHESTRA.into_iter().flat_map(|k| spread(k, Section::orchestral(k), 3)).collect()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_choose_their_models() {
        assert_eq!(Voicing::of(0, 0), Voicing::Piano(PianoTone::GRAND));
        assert_eq!(Voicing::of(42, 0), Voicing::Bowed { body: Some(BodyKind::Cello), players: 1, tremolo: false, slow: false });
        assert_eq!(Voicing::of(42, 10), Voicing::Bowed { body: Some(BodyKind::Cello), players: 10, tremolo: false, slow: false });
        assert_eq!(Voicing::of(40, 200), Voicing::Bowed { body: Some(BodyKind::Violin), players: MAX_PLAYERS, tremolo: false, slow: false });
        assert!(matches!(Voicing::of(48, 0), Voicing::Bowed { body: None, .. }));
        assert!(matches!(Voicing::of(45, 0), Voicing::Pizzicato { .. }));
        for p in [24, 25, 29, 30, 33, 105] {
            assert_eq!(Voicing::of(p, 0), Voicing::Plucked, "{p}");
            assert_eq!(Voicing::of(p, 16), Voicing::Plucked, "{p}");
        }
        assert!(Voicing::Plucked.desks().is_empty());
    }

    #[test]
    fn a_soloist_has_one_body_and_a_section_several_across_the_stage() {
        let solo = Voicing::of(40, 1).desks();
        assert_eq!(solo, vec![Desk { kind: BodyKind::Violin, which: 0, place: 0.0 }], "the soloist's own instrument, in the middle");
        let section = Voicing::of(40, 16).desks();
        assert_eq!(section.len(), 4);
        assert!(section.iter().all(|d| d.which > 0 && d.place.abs() <= 0.5));
        assert!(section.windows(2).all(|w| w[0].place < w[1].place && w[0].which != w[1].which));
        // An orchestra: every section, violins on the left and basses on the right.
        let orchestra = Voicing::of(48, 0).desks();
        assert_eq!(orchestra.len(), 12);
        let middle = |k: BodyKind| {
            let places: Vec<f32> = orchestra.iter().filter(|d| d.kind == k).map(|d| d.place).collect();
            places.iter().sum::<f32>() / places.len() as f32
        };
        assert!(middle(BodyKind::Violin) < middle(BodyKind::Viola) && middle(BodyKind::Viola) < middle(BodyKind::Cello));
        assert!(middle(BodyKind::DoubleBass) > 0.3);
        assert_eq!(Section::orchestral(BodyKind::Violin).players, 16);
        assert_eq!(Section::orchestral(BodyKind::DoubleBass).players, 8);
    }
}
