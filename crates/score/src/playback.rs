use crate::{Fade, NoteKind, Score};

/// The order measures are played in with repeats unrolled. Each repeat close replays from the
/// nearest earlier repeat open (or the start) `repeat_close` extra times. Total output is capped so
/// a hostile file cannot make this allocate without bound.
pub fn playback_order(score: &Score) -> Vec<usize> {
    const CAP: usize = 100_000;
    let mut order = Vec::new();
    let mut section_start = 0usize;
    let mut remaining: Vec<(usize, u8)> = Vec::new(); // (close measure, passes left)
    let mut i = 0usize;
    while let Some(header) = score.headers.get(i) {
        if order.len() >= CAP {
            break;
        }
        if header.repeat_open {
            section_start = i;
        }
        order.push(i);
        if header.repeat_close > 0 {
            let left = match remaining.iter().position(|(m, _)| *m == i) {
                Some(p) => remaining.get(p).map_or(0, |r| r.1),
                None => {
                    remaining.push((i, header.repeat_close));
                    header.repeat_close
                }
            };
            if left > 0 {
                if let Some(entry) = remaining.iter_mut().find(|(m, _)| *m == i) {
                    entry.1 = left - 1;
                }
                i = section_start;
                continue;
            }
            remaining.retain(|(m, _)| *m != i);
            section_start = i + 1;
        }
        i += 1;
    }
    order
}

/// One attack in playback time.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayedBeat {
    pub track: usize,
    pub measure: usize,
    pub start_tick: u64,
    pub ticks: u32,
    pub string: u8,
    pub pitch: Option<guitario_theory::Pitch>,
    pub velocity: u8,
    pub dead: bool,
    /// The note's effects, for synthesis (palm mute, bends, legato …).
    pub effects: crate::NoteEffects,
    /// The beat's fade in, fade out or swell.
    pub fade: Option<Fade>,
}

/// Flattens the score into timed notes, following repeats. Tied notes extend the previous
/// attack on the same string instead of re-attacking. Triplet feel moves notes inside each
/// measure, and a brush or arpeggio staggers the strings of a beat.
pub fn timeline(score: &Score) -> Vec<PlayedBeat> {
    let order = playback_order(score);
    let mut out: Vec<PlayedBeat> = Vec::new();
    for (ti, track) in score.tracks.iter().enumerate() {
        let mut clock: u64 = 0;
        let mut open_tie: Vec<Option<usize>> = vec![None; track.string_count()];
        for &mi in &order {
            let Some(measure) = track.measures.get(mi) else { continue };
            let capacity = score.headers.get(mi).map_or(0, |h| h.capacity());
            let Some(voice) = measure.voices.first() else {
                clock += u64::from(capacity);
                continue;
            };
            let feel = score.headers.get(mi).map_or(crate::TripletFeel::None, |h| h.triplet_feel);
            let mut at = clock;
            for beat in &voice.beats {
                let ticks = beat.duration.ticks();
                // Swung start and end of this beat.
                let (s0, s1) = (clock + feel.warp(at - clock), clock + feel.warp(at - clock + u64::from(ticks)));
                let ticks_swung = u32::try_from(s1.saturating_sub(s0)).unwrap_or(ticks).max(1);
                if !beat.rest {
                    // Strings in strum order: a downstroke starts on the lowest string.
                    let count = beat.notes.len();
                    for note in &beat.notes {
                        let s = usize::from(note.string);
                        if note.kind == NoteKind::Tie
                            && let Some(&Some(idx)) = open_tie.get(s)
                        {
                            if let Some(prev) = out.get_mut(idx) {
                                prev.ticks += ticks_swung;
                            }
                            continue;
                        }
                        let dead = note.kind == NoteKind::Dead;
                        let pitch = if dead { None } else { track.tuning.pitch(s, note.fret, track.capo) };
                        let mut velocity = beat.dynamic.velocity();
                        if note.effects.accent {
                            velocity = velocity.saturating_add(15).min(127);
                        }
                        if note.effects.heavy_accent {
                            velocity = velocity.saturating_add(28).min(127);
                        }
                        if note.effects.ghost {
                            velocity = velocity.saturating_sub(25).max(1);
                        }
                        if beat.technique.is_some() {
                            velocity = velocity.saturating_add(10).min(127);
                        }
                        let offset = match beat.brush {
                            Some(b) if count > 1 => {
                                let rank = beat.notes.iter().filter(|o| if b.down { o.string > note.string } else { o.string < note.string }).count();
                                let span = u64::from(b.ticks.min(ticks_swung.saturating_sub(1)));
                                span * rank as u64 / (count as u64 - 1)
                            }
                            _ => 0,
                        };
                        if let Some(slot) = open_tie.get_mut(s) {
                            *slot = Some(out.len());
                        }
                        out.push(PlayedBeat {
                            track: ti,
                            measure: mi,
                            start_tick: s0 + offset,
                            ticks: ticks_swung.saturating_sub(u32::try_from(offset).unwrap_or(0)).max(1),
                            string: note.string,
                            pitch,
                            velocity,
                            dead,
                            effects: note.effects,
                            fade: beat.fade,
                        });
                    }
                }
                at += u64::from(ticks);
            }
            clock += u64::from(capacity);
        }
    }
    out.sort_by_key(|p| (p.start_tick, p.track, p.string));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MeasureHeader, Track};
    use guitario_theory::{TimeSignature, Tuning};

    fn score_with(n: usize) -> Score {
        let mut s = Score::new();
        s.headers = vec![MeasureHeader::new(TimeSignature::COMMON); n];
        s.tracks = vec![Track::new("g", Tuning::standard(), &s.headers)];
        s
    }

    #[test]
    fn plain_order() {
        assert_eq!(playback_order(&score_with(3)), vec![0, 1, 2]);
    }

    #[test]
    fn repeat_section() {
        let mut s = score_with(4);
        s.headers[1].repeat_open = true;
        s.headers[2].repeat_close = 1;
        assert_eq!(playback_order(&s), vec![0, 1, 2, 1, 2, 3]);
    }

    #[test]
    fn repeat_from_start_twice_extra() {
        let mut s = score_with(2);
        s.headers[0].repeat_close = 2;
        assert_eq!(playback_order(&s), vec![0, 0, 0, 1]);
    }

    #[test]
    fn hostile_repeat_count_is_bounded() {
        let mut s = score_with(2);
        s.headers[0].repeat_close = 255;
        s.headers[1].repeat_close = 255;
        assert!(playback_order(&s).len() <= 100_000 + 1);
    }

    #[test]
    fn timeline_ties_extend() {
        use crate::{Beat, Note};
        use guitario_theory::Duration;
        let mut s = score_with(1);
        let beats = &mut s.tracks[0].measures[0].voices[0].beats;
        let mut tied = Note::new(0, 5);
        tied.kind = NoteKind::Tie;
        *beats = vec![
            Beat { rest: false, notes: vec![Note::new(0, 5)], ..Beat::rest(Duration::QUARTER) },
            Beat { rest: false, notes: vec![tied], ..Beat::rest(Duration::QUARTER) },
            Beat::rest(Duration::new(guitario_theory::NoteValue::Half)),
        ];
        let t = timeline(&s);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].ticks, 2 * guitario_theory::TICKS_PER_QUARTER);
        assert_eq!(t[0].pitch.map(|p| p.0), Some(69));
    }

    fn eighths(s: &mut Score) {
        use crate::{Beat, Note};
        use guitario_theory::{Duration, NoteValue};
        let e = Duration::new(NoteValue::Eighth);
        s.tracks[0].measures[0].voices[0].beats = (0..8).map(|i| Beat { rest: false, notes: vec![Note::new(0, i)], ..Beat::rest(e) }).collect();
    }

    #[test]
    fn triplet_feel_swings_eighths() {
        let mut s = score_with(1);
        eighths(&mut s);
        s.headers[0].triplet_feel = crate::TripletFeel::Triplet8th;
        let t = timeline(&s);
        let q = u64::from(guitario_theory::TICKS_PER_QUARTER);
        let starts: Vec<u64> = t.iter().map(|p| p.start_tick).take(4).collect();
        assert_eq!(starts, vec![0, 2 * q / 3, q, 5 * q / 3]);
        assert_eq!((u64::from(t[0].ticks), u64::from(t[1].ticks)), (2 * q / 3, q / 3));
        assert_eq!(crate::TripletFeel::None.warp(q / 2), q / 2);
        assert_eq!(crate::TripletFeel::Scottish8th.warp(q / 2), q / 4);
        assert_eq!(crate::TripletFeel::Dotted16th.warp(q / 4), 3 * q / 8);
    }

    #[test]
    fn brush_staggers_strings_low_to_high() {
        use crate::{Beat, Brush, BrushKind, Note};
        use guitario_theory::Duration;
        let mut s = score_with(1);
        s.tracks[0].measures[0].voices[0].beats = vec![Beat {
            rest: false,
            notes: vec![Note::new(0, 0), Note::new(1, 0), Note::new(2, 0)],
            brush: Some(Brush { kind: BrushKind::Brush, down: true, ticks: guitario_theory::TICKS_PER_QUARTER / 8 }),
            ..Beat::rest(Duration::WHOLE)
        }];
        let t = timeline(&s);
        let by_string = |st: u8| t.iter().find(|p| p.string == st).map(|p| p.start_tick);
        let e = u64::from(guitario_theory::TICKS_PER_QUARTER / 8);
        assert_eq!((by_string(2), by_string(1), by_string(0)), (Some(0), Some(e / 2), Some(e)));
    }
}
