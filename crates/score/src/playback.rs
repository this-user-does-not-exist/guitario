use crate::{NoteKind, Score};

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
}

/// Flattens the score into timed notes, following repeats. Tied notes extend the previous
/// attack on the same string instead of re-attacking.
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
            let mut at = clock;
            for beat in &voice.beats {
                let ticks = beat.duration.ticks();
                if !beat.rest {
                    for note in &beat.notes {
                        let s = usize::from(note.string);
                        if note.kind == NoteKind::Tie
                            && let Some(&Some(idx)) = open_tie.get(s)
                        {
                            if let Some(prev) = out.get_mut(idx) {
                                prev.ticks += ticks;
                            }
                            continue;
                        }
                        let dead = note.kind == NoteKind::Dead;
                        let pitch = if dead { None } else { track.tuning.pitch(s, note.fret, track.capo) };
                        let mut velocity = beat.dynamic.velocity();
                        if note.effects.accent {
                            velocity = velocity.saturating_add(15).min(127);
                        }
                        if note.effects.ghost {
                            velocity = velocity.saturating_sub(25).max(1);
                        }
                        if let Some(slot) = open_tie.get_mut(s) {
                            *slot = Some(out.len());
                        }
                        out.push(PlayedBeat {
                            track: ti,
                            measure: mi,
                            start_tick: at,
                            ticks,
                            string: note.string,
                            pitch,
                            velocity,
                            dead,
                            effects: note.effects,
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
        assert_eq!(t[0].ticks, 1920);
        assert_eq!(t[0].pitch.map(|p| p.0), Some(69));
    }
}
