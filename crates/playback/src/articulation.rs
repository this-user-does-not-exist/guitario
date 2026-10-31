//! How each note is played: which of an instrument's recorded articulations sounds it, the key it
//! plays, and whether it carries on a string already ringing instead of being picked.

use guitario_score::{PlayedBeat, Score, Slide};

/// A way of playing a note that a sampled library may record separately (`bank.json` maps each
/// to a program; one that is not mapped plays from the sustain program).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Articulation {
    /// A plain picked note, and every articulation without a program of its own.
    Sustain,
    PalmMute,
    /// A muted string: a click without pitch.
    Dead,
    /// Natural, artificial, pinch, tapped and semi-harmonics, played at the pitch heard.
    Harmonic,
    /// Hammer-ons, pull-offs and legato slides: the note sounds without a pick attack.
    Legato,
}

impl Articulation {
    pub const ALL: [Articulation; 5] = [Articulation::Sustain, Articulation::PalmMute, Articulation::Dead, Articulation::Harmonic, Articulation::Legato];
    pub const COUNT: usize = Articulation::ALL.len();

    /// Its name in `bank.json`.
    pub fn key(self) -> &'static str {
        match self {
            Articulation::Sustain => "sustain",
            Articulation::PalmMute => "palm_mute",
            Articulation::Dead => "dead",
            Articulation::Harmonic => "harmonic",
            Articulation::Legato => "legato",
        }
    }

    pub fn from_key(key: &str) -> Option<Articulation> {
        Articulation::ALL.into_iter().find(|a| a.key() == key)
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The articulation of `note`; `legato` says it carries on its string (see [`legato_flags`]).
    pub fn of(note: &PlayedBeat, legato: bool) -> Articulation {
        if note.dead {
            Articulation::Dead
        } else if note.harmonic.is_some() {
            Articulation::Harmonic
        } else if legato {
            Articulation::Legato
        } else if note.effects.palm_mute {
            Articulation::PalmMute
        } else {
            Articulation::Sustain
        }
    }
}

/// Whether the note before `next` on its string flows into it without a new pick: a hammer-on or
/// pull-off, a legato slide, or a tap (a shift slide picks the second note).
pub fn flows_into(prev: &PlayedBeat, next: &PlayedBeat) -> bool {
    !prev.dead && !next.dead && (prev.effects.hammer || prev.effects.slide == Some(Slide::Legato) || next.effects.tapped || next.effects.left_hand_tapped)
}

/// Sorts `played` by track, string and time (the order a string is played in) and returns, for
/// each note, whether it carries on the note just before it on its string.
pub fn legato_flags(played: &mut [PlayedBeat]) -> Vec<bool> {
    played.sort_by_key(|p| (p.track, p.string, p.start_tick));
    let mut flags = vec![false; played.len()];
    for (i, flag) in flags.iter_mut().enumerate().skip(1) {
        let (Some(prev), Some(p)) = (played.get(i - 1), played.get(i)) else { continue };
        *flag = prev.track == p.track && prev.string == p.string && prev.start_tick + u64::from(prev.ticks) == p.start_tick && flows_into(prev, p);
    }
    flags
}

/// [`legato_flags`] for `played` in the timeline's own order (by time), without sorting it: each
/// note is checked against the last one seen on its string.
pub fn legato_in_order(played: &[PlayedBeat]) -> Vec<bool> {
    let mut last: std::collections::HashMap<(usize, u8), usize> = std::collections::HashMap::new();
    played
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let prev = last.insert((p.track, p.string), i).and_then(|j| played.get(j));
            prev.is_some_and(|q| q.start_tick + u64::from(q.ticks) == p.start_tick && flows_into(q, p))
        })
        .collect()
}

/// The key a note plays on a sampled instrument: the drum number (the General MIDI drum an
/// articulation varies), the pitch heard (a harmonic's partial), or for a dead note the open
/// string's pitch. None for a note without one.
pub fn sample_key(score: &Score, note: &PlayedBeat) -> Option<u8> {
    let track = score.tracks.get(note.track)?;
    if track.is_drums() {
        return note.pitch.map(|p| guitario_theory::drums::plays(p.0));
    }
    if let Some(h) = note.harmonic {
        return Some(h.key());
    }
    match note.pitch {
        Some(p) => Some(p.0),
        None if note.dead => track.tuning.strings.get(usize::from(note.string)).map(|open| open.saturating_add(track.capo).min(127)),
        None => None,
    }
}

/// The sorted, distinct keys each track plays in each articulation, from one pass over the
/// timeline: `[track][articulation]`.
pub fn used_keys_by_articulation(score: &Score) -> Vec<[Vec<u8>; Articulation::COUNT]> {
    let mut keys: Vec<[Vec<u8>; Articulation::COUNT]> = (0..score.tracks.len()).map(|_| Default::default()).collect();
    let played = guitario_score::timeline(score);
    let legato = legato_in_order(&played);
    for (p, l) in played.iter().zip(legato) {
        let (Some(key), Some(slots), Some(track)) = (sample_key(score, p), keys.get_mut(p.track), score.tracks.get(p.track)) else { continue };
        // A drum kit plays every hit from its one program.
        let art = if track.is_drums() { Articulation::Sustain } else { Articulation::of(p, l) };
        if let Some(list) = slots.get_mut(art.index()) {
            list.push(key);
        }
        // A semi-harmonic sounds the plain note under it.
        if let (Some(h), Some(pitch), Some(list)) = (p.harmonic, p.pitch, slots.get_mut(Articulation::Sustain.index()))
            && h.fundamental >= 0.5
        {
            list.push(pitch.0);
        }
    }
    for list in keys.iter_mut().flatten() {
        list.sort_unstable();
        list.dedup();
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::{Beat, Harmonic, Note, NoteKind};
    use guitario_theory::Duration;

    fn beats(notes: Vec<Note>) -> Score {
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = notes.into_iter().map(|n| Beat { rest: false, notes: vec![n], ..Beat::rest(Duration::QUARTER) }).collect();
        s
    }

    #[test]
    fn hammers_legato_slides_and_taps_flow_on_and_shift_slides_pick_again() {
        let fx = |fret: u8, f: &dyn Fn(&mut Note)| {
            let mut n = Note::new(0, fret);
            f(&mut n);
            n
        };
        let s = beats(vec![
            fx(5, &|n| n.effects.hammer = true),
            fx(7, &|n| n.effects.slide = Some(Slide::Shift)),
            fx(9, &|n| n.effects.slide = Some(Slide::Legato)),
            fx(12, &|_| {}),
        ]);
        let mut played = guitario_score::timeline(&s);
        assert_eq!(legato_in_order(&played), vec![false, true, false, true]);
        assert_eq!(legato_flags(&mut played), vec![false, true, false, true]);
        let mut tapped = beats(vec![Note::new(0, 5), fx(12, &|n| n.effects.tapped = true)]);
        let mut played = guitario_score::timeline(&tapped);
        assert_eq!(legato_flags(&mut played), vec![false, true]);
        // A dead note never flows.
        tapped.tracks[0].measures[0].voices[0].beats[0].notes[0].kind = NoteKind::Dead;
        let mut played = guitario_score::timeline(&tapped);
        assert_eq!(legato_flags(&mut played), vec![false, false]);
    }

    #[test]
    fn keys_are_sorted_by_articulation() {
        let fx = |string: u8, fret: u8, f: &dyn Fn(&mut Note)| {
            let mut n = Note::new(string, fret);
            f(&mut n);
            n
        };
        let s = beats(vec![
            fx(5, 3, &|n| n.effects.palm_mute = true),
            fx(5, 3, &|n| n.kind = NoteKind::Dead),
            fx(0, 12, &|n| n.effects.harmonic = Some(Harmonic::Natural)),
            fx(1, 5, &|n| n.effects.harmonic = Some(Harmonic::Semi)),
        ]);
        let keys = &used_keys_by_articulation(&s)[0];
        let at = |a: Articulation| keys[a.index()].clone();
        assert_eq!(at(Articulation::PalmMute), vec![43]);
        assert_eq!(at(Articulation::Dead), vec![40], "the open string");
        assert_eq!(at(Articulation::Harmonic), vec![76, 88], "an octave over the open E; two octaves over the B string's 5th fret");
        assert_eq!(at(Articulation::Sustain), vec![64], "the semi-harmonic's plain note");
        assert!(at(Articulation::Legato).is_empty());
        assert!(Articulation::ALL.iter().all(|a| Articulation::from_key(a.key()) == Some(*a)));
    }
}
