use crate::{Fade, NoteKind, Score};

/// The order measures are played in, following repeats, alternate endings and directions.
///
/// - A repeat close replays from the nearest earlier repeat open (or the start) `repeat_close`
///   extra times. Inside a repeat, a bar with alternate endings plays only on the passes it lists.
/// - A D.C. or D.S. jump at the end of a bar is taken once. After it, repeats are not taken again
///   and only the last ending of each ending group plays. "al Fine" stops at the end of a Fine bar;
///   "al Coda" takes the next To Coda to the coda.
///
/// Total output is capped so a hostile file cannot make this allocate without bound.
pub fn playback_order(score: &Score) -> Vec<usize> {
    use crate::{DirectionTarget, Jump, JumpEnd};
    const CAP: usize = 100_000;
    let headers = &score.headers;
    let find = |t: DirectionTarget| headers.iter().position(|h| h.targets.contains(&t));
    // The last ending of the group each bar belongs to: the highest ending number in its run of
    // consecutive ending bars.
    let mut last_ending = vec![0u8; headers.len()];
    let mut i = 0;
    while i < headers.len() {
        if headers.get(i).is_some_and(|h| h.endings != 0) {
            let mut j = i;
            let mut bits = 0u8;
            while let Some(h) = headers.get(j).filter(|h| h.endings != 0) {
                bits |= h.endings;
                j += 1;
            }
            let top = 1u8 << (7 - bits.leading_zeros().min(7));
            for slot in last_ending.iter_mut().take(j).skip(i) {
                *slot = top;
            }
            i = j;
        } else {
            i += 1;
        }
    }

    let mut order = Vec::new();
    let mut section_start = 0usize;
    let mut remaining: Vec<(usize, u8)> = Vec::new(); // (close measure, passes left)
    let mut pass = 1u32; // 1-based pass through the current repeat section
    let mut taken: Vec<usize> = Vec::new(); // jumps already taken
    let mut after_jump: Option<JumpEnd> = None;
    let mut i = 0usize;
    while let Some(header) = headers.get(i) {
        if order.len() >= CAP {
            break;
        }
        if header.repeat_open && section_start != i {
            section_start = i;
            pass = 1;
        }
        // Alternate endings: skip bars that are not for this pass.
        if header.endings != 0 {
            let plays = match after_jump {
                Some(_) => header.endings & last_ending.get(i).copied().unwrap_or(0) != 0,
                None => pass <= 8 && header.endings & (1u8 << (pass - 1)) != 0,
            };
            if !plays {
                i += 1;
                continue;
            }
        }
        order.push(i);
        // Fine ends the piece after a jump "al Fine".
        if after_jump == Some(JumpEnd::Fine) && header.targets.contains(&DirectionTarget::Fine) {
            break;
        }
        // Repeats (not after a D.C./D.S.).
        if header.repeat_close > 0 && after_jump.is_none() {
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
                pass += 1;
                i = section_start;
                continue;
            }
            remaining.retain(|(m, _)| *m != i);
        }
        // Jumps at the end of the bar.
        if let Some(jump) = header.jump
            && !taken.contains(&i)
        {
            let target = match jump {
                Jump::DaCapo(end) => Some((0, Some(end))),
                Jump::DalSegno(end) => find(DirectionTarget::Segno).map(|t| (t, Some(end))),
                Jump::DalSegnoSegno(end) => find(DirectionTarget::SegnoSegno).map(|t| (t, Some(end))),
                Jump::ToCoda if after_jump == Some(JumpEnd::Coda) => find(DirectionTarget::Coda).map(|t| (t, after_jump)),
                Jump::ToDoubleCoda if after_jump == Some(JumpEnd::DoubleCoda) => find(DirectionTarget::DoubleCoda).map(|t| (t, after_jump)),
                Jump::ToCoda | Jump::ToDoubleCoda => None,
            };
            if let Some((t, end)) = target {
                taken.push(i);
                after_jump = end;
                section_start = t;
                pass = 1;
                i = t;
                continue;
            }
        }
        if header.repeat_close > 0 {
            section_start = i + 1;
            pass = 1;
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
        // The attack a tied note extends, per voice and string.
        let mut open_tie: Vec<Vec<Option<usize>>> = Vec::new();
        for &mi in &order {
            let Some(measure) = track.measures.get(mi) else { continue };
            let capacity = score.headers.get(mi).map_or(0, |h| h.capacity());
            let feel = score.headers.get(mi).map_or(crate::TripletFeel::None, |h| h.triplet_feel);
            for (vi, voice) in measure.voices.iter().enumerate() {
                if open_tie.len() <= vi {
                    open_tie.resize_with(vi + 1, || vec![None; track.string_count()]);
                }
                let Some(ties) = open_tie.get_mut(vi) else { continue };
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
                                && let Some(&Some(idx)) = ties.get(s)
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
                            if let Some(slot) = ties.get_mut(s) {
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
    fn every_voice_sounds_and_ties_stay_inside_their_voice() {
        use crate::{Beat, Measure, Note, NoteKind, Voice};
        use guitario_theory::Duration;
        let mut s = score_with(1);
        let note = |string: u8, fret: u8, kind: NoteKind, d: Duration| {
            let mut n = Note::new(string, fret);
            n.kind = kind;
            Beat { rest: false, notes: vec![n], ..Beat::rest(d) }
        };
        let (q, h) = (Duration::QUARTER, Duration::new(guitario_theory::NoteValue::Half));
        // Voice 1: a whole-note G on string 2 held as two tied halves. Voice 2: the same string,
        // two quarters then a half rest, so its attacks must not extend voice 1's note.
        let m: &mut Measure = &mut s.tracks[0].measures[0];
        m.voices = vec![
            Voice { beats: vec![note(2, 0, NoteKind::Normal, h), note(2, 0, NoteKind::Tie, h)] },
            Voice { beats: vec![note(2, 5, NoteKind::Normal, q), note(2, 5, NoteKind::Tie, q), Beat::rest(h)] },
        ];
        let out = timeline(&s);
        assert_eq!(out.len(), 2, "{out:?}");
        let first = out.iter().find(|p| p.pitch == s.tracks[0].tuning.pitch(2, 0, 0)).unwrap();
        let second = out.iter().find(|p| p.pitch == s.tracks[0].tuning.pitch(2, 5, 0)).unwrap();
        assert_eq!((first.start_tick, first.ticks), (0, 2 * h.ticks()));
        assert_eq!((second.start_tick, second.ticks), (0, 2 * q.ticks()));
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
    fn alternate_endings() {
        // |: 0 1 [1. 2 :] [2. 3] 4
        let mut s = score_with(5);
        s.headers[0].repeat_open = true;
        s.headers[2].endings = 0b01;
        s.headers[2].repeat_close = 1;
        s.headers[3].endings = 0b10;
        assert_eq!(playback_order(&s), vec![0, 1, 2, 0, 1, 3, 4]);
        // Three passes: endings 1 and 2 share a bar.
        let mut s = score_with(4);
        s.headers[0].repeat_open = true;
        s.headers[1].endings = 0b011;
        s.headers[1].repeat_close = 2;
        s.headers[2].endings = 0b100;
        assert_eq!(playback_order(&s), vec![0, 1, 0, 1, 0, 2, 3]);
    }

    #[test]
    fn da_capo_al_fine() {
        use crate::{DirectionTarget, Jump, JumpEnd};
        let mut s = score_with(4);
        s.headers[1].targets = vec![DirectionTarget::Fine];
        s.headers[3].jump = Some(Jump::DaCapo(JumpEnd::Fine));
        assert_eq!(playback_order(&s), vec![0, 1, 2, 3, 0, 1]);
    }

    #[test]
    fn dal_segno_al_coda() {
        use crate::{DirectionTarget, Jump, JumpEnd};
        // 0 §1 2(To Coda) 3(D.S. al Coda) ⊕4 5
        let mut s = score_with(6);
        s.headers[1].targets = vec![DirectionTarget::Segno];
        s.headers[2].jump = Some(Jump::ToCoda);
        s.headers[3].jump = Some(Jump::DalSegno(JumpEnd::Coda));
        s.headers[4].targets = vec![DirectionTarget::Coda];
        assert_eq!(playback_order(&s), vec![0, 1, 2, 3, 1, 2, 4, 5]);
    }

    #[test]
    fn repeats_are_not_taken_after_a_jump_and_the_last_ending_plays() {
        use crate::{Jump, JumpEnd};
        // |: 0 [1. 1 :] [2. 2] 3(D.C.)
        let mut s = score_with(4);
        s.headers[0].repeat_open = true;
        s.headers[1].endings = 0b01;
        s.headers[1].repeat_close = 1;
        s.headers[2].endings = 0b10;
        s.headers[3].jump = Some(Jump::DaCapo(JumpEnd::End));
        assert_eq!(playback_order(&s), vec![0, 1, 0, 2, 3, 0, 2, 3]);
    }

    #[test]
    fn a_jump_without_its_target_is_ignored_and_loops_terminate() {
        use crate::{Jump, JumpEnd};
        let mut s = score_with(2);
        s.headers[1].jump = Some(Jump::DalSegno(JumpEnd::Coda));
        assert_eq!(playback_order(&s), vec![0, 1]);
        let mut s = score_with(2);
        s.headers[0].jump = Some(Jump::DaCapo(JumpEnd::End));
        s.headers[1].jump = Some(Jump::DaCapo(JumpEnd::End));
        assert!(playback_order(&s).len() < 10);
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
