use crate::{Fade, NoteKind, Score};
use guitario_theory::TICKS_PER_QUARTER;

/// How long an automatic brush takes across a chord: a 64th note, about 30 ms at 120 bpm.
const AUTO_BRUSH: u32 = TICKS_PER_QUARTER / 16;

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
    /// The whammy bar's curve moving this note.
    pub whammy: Option<PlayedWhammy>,
}

/// A whammy curve in playback time: it runs over `ticks` from `from_tick`. Before that the bar is
/// at rest; after it, it stays where the curve ends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayedWhammy {
    pub curve: crate::Whammy,
    pub from_tick: u64,
    pub ticks: u32,
}

impl PlayedWhammy {
    /// The pitch at `tick`, in semitones.
    pub fn semitones_at(&self, tick: f64) -> f32 {
        let from = self.from_tick as f64;
        if tick < from {
            return 0.0;
        }
        self.curve.semitones_at(((tick - from) / f64::from(self.ticks.max(1))) as f32)
    }
}

/// Most attacks one note makes inside one beat, so a hostile duration cannot make a huge timeline.
const MAX_ATTACKS: u32 = 512;

/// Where the attacks of one note fall inside a beat of `total` ticks, as `(start, length)` from the
/// beat's start: one attack, or a new one every `period` ticks (tremolo picking, a trill). `lead` delays
/// the first (a brush). A period too long for two attacks to fit gives the single attack.
fn attacks(period: Option<u32>, total: u32, lead: u32) -> Vec<(u32, u32)> {
    let total = total.max(1);
    let lead = lead.min(total - 1);
    let span = total - lead;
    let Some(period) = period.filter(|p| *p > 0 && *p < span) else { return vec![(lead, span)] };
    let wanted = span.div_ceil(period);
    let (n, step) = if wanted > MAX_ATTACKS { (MAX_ATTACKS, (span / MAX_ATTACKS).max(1)) } else { (wanted, period) };
    (0..n).map(|i| (lead + i * step, if i + 1 == n { span - i * step } else { step })).collect()
}

/// Flattens the score into timed notes, following repeats. Tied notes extend the previous
/// attack on the same string instead of re-attacking. Triplet feel moves notes inside each
/// measure, and a brush or arpeggio staggers the strings of a beat. Tremolo picking picks a
/// beat's notes again and again, and a trill alternates its note with another fret. A grace note
/// sounds just before its note (cutting the note before it short), or on the beat and delays it.
pub fn timeline(score: &Score) -> Vec<PlayedBeat> {
    let order = playback_order(score);
    let mut out: Vec<PlayedBeat> = Vec::new();
    for (ti, track) in score.tracks.iter().enumerate() {
        let mut clock: u64 = 0;
        // The attack a tied note extends, per voice and string.
        let mut open_tie: Vec<Vec<Option<usize>>> = Vec::new();
        let feel_of = track.interpretation;
        // Automatic let ring: the notes of each voice still ringing, until a rest stops them.
        let mut ringing: Vec<Vec<usize>> = Vec::new();
        for &mi in &order {
            let Some(measure) = track.measures.get(mi) else { continue };
            let capacity = score.headers.get(mi).map_or(0, |h| h.capacity());
            let feel = score.headers.get(mi).map_or(crate::TripletFeel::None, |h| h.triplet_feel);
            // The beat the accentuation stresses: a quarter in 4/4, an eighth in 6/8.
            let unit = score.headers.get(mi).map_or(TICKS_PER_QUARTER, |h| TICKS_PER_QUARTER * 4 / u32::from(h.time_signature.denominator.max(1)));
            for (vi, voice) in measure.voices.iter().enumerate() {
                if open_tie.len() <= vi {
                    open_tie.resize_with(vi + 1, || vec![None; track.string_count()]);
                }
                if ringing.len() <= vi {
                    ringing.resize_with(vi + 1, Vec::new);
                }
                let Some(ties) = open_tie.get_mut(vi) else { continue };
                let mut at = clock;
                for beat in &voice.beats {
                    let ticks = beat.duration.ticks();
                    // Swung start and end of this beat.
                    let (s0, s1) = (clock + feel.warp(at - clock), clock + feel.warp(at - clock + u64::from(ticks)));
                    let ticks_swung = u32::try_from(s1.saturating_sub(s0)).unwrap_or(ticks).max(1);
                    // A rest stops what the automatic let ring holds.
                    if beat.rest
                        && let Some(held) = ringing.get_mut(vi)
                    {
                        for idx in held.drain(..) {
                            if let Some(n) = out.get_mut(idx) {
                                n.ticks = n.ticks.max(u32::try_from(s0.saturating_sub(n.start_tick)).unwrap_or(u32::MAX));
                            }
                        }
                    }
                    if !beat.rest {
                        // Strings in strum order: a downstroke starts on the lowest string.
                        let count = beat.notes.len();
                        for note in &beat.notes {
                            let s = usize::from(note.string);
                            if note.kind == NoteKind::Tie
                                && let Some(&Some(idx)) = ties.get(s)
                            {
                                if let Some(prev) = out.get_mut(idx) {
                                    // A bend carries on through the tie: held, or followed by the tied note's own curve.
                                    prev.effects.bend = crate::Bend::join(prev.effects.bend, prev.ticks, note.effects.bend, ticks_swung);
                                    // So does the whammy: the tied beat's curve follows the one before, if any.
                                    prev.whammy = match (prev.whammy, beat.whammy) {
                                        (Some(w), Some(next)) => {
                                            let before = u32::try_from(s0.saturating_sub(w.from_tick)).unwrap_or(u32::MAX);
                                            Some(PlayedWhammy {
                                                curve: crate::Whammy::join(Some(w.curve), before, Some(next), ticks_swung).unwrap_or(w.curve),
                                                ticks: before.saturating_add(ticks_swung),
                                                ..w
                                            })
                                        }
                                        (None, Some(next)) => Some(PlayedWhammy { curve: next, from_tick: s0, ticks: ticks_swung }),
                                        (held, None) => held,
                                    };
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
                            // Accentuation stresses the bar's first beat, and less the others.
                            let pos = at - clock;
                            let stress = if pos == 0 {
                                25
                            } else if unit > 0 && pos.is_multiple_of(u64::from(unit)) {
                                12
                            } else {
                                0
                            };
                            velocity = velocity.saturating_add(u8::try_from(u32::from(feel_of.accentuation) * stress / 100).unwrap_or(0)).min(127);
                            // An automatic brush strums a chord quickly, low string first unless it is an upstroke.
                            let auto = (feel_of.auto_brush && beat.brush.is_none()).then(|| crate::Brush {
                                kind: crate::BrushKind::Brush,
                                down: beat.stroke != Some(crate::StrokeDirection::Up),
                                ticks: AUTO_BRUSH,
                            });
                            let offset = match beat.brush.or(auto) {
                                Some(b) if count > 1 => {
                                    let rank = beat.notes.iter().filter(|o| if b.down { o.string > note.string } else { o.string < note.string }).count();
                                    let span = u64::from(b.ticks.min(ticks_swung.saturating_sub(1)));
                                    span * rank as u64 / (count as u64 - 1)
                                }
                                _ => 0,
                            };
                            // A grace note sounds just before the note, or on the beat and delays it.
                            let mut lead = u32::try_from(offset).unwrap_or(0);
                            if let Some(grace) = note.effects.grace {
                                let at = s0 + u64::from(lead);
                                let len = grace.duration.ticks().min(ticks_swung.saturating_sub(lead).saturating_sub(1)).max(1);
                                // Before the very first beat there is no time to take it from: it goes on the beat.
                                let start = if grace.on_beat || at < u64::from(len) {
                                    lead = lead.saturating_add(len);
                                    at
                                } else {
                                    at - u64::from(len)
                                };
                                // The note before it on this string stops for it.
                                if let Some(prev) = ties.get(s).copied().flatten().and_then(|i| out.get_mut(i))
                                    && prev.start_tick < start
                                {
                                    let room = u32::try_from(start - prev.start_tick).unwrap_or(u32::MAX);
                                    prev.ticks = prev.ticks.min(room);
                                }
                                let grace_pitch = if grace.dead { None } else { track.tuning.pitch(s, grace.fret, track.capo) };
                                let mut effects = crate::NoteEffects { palm_mute: note.effects.palm_mute, ..crate::NoteEffects::default() };
                                match grace.transition {
                                    crate::GraceTransition::None => {}
                                    crate::GraceTransition::Hammer => effects.hammer = true,
                                    crate::GraceTransition::Slide => effects.slide = Some(crate::Slide::Legato),
                                    crate::GraceTransition::Bend => {
                                        // Bent up to the note's pitch, which then carries on without a new pluck.
                                        let up = match (grace_pitch, pitch) {
                                            (Some(g), Some(p)) if p.0 > g.0 => p.0 - g.0,
                                            _ => 0,
                                        };
                                        effects.bend = (up > 0).then(|| crate::Bend::to(up.saturating_mul(2)));
                                        effects.hammer = true;
                                    }
                                }
                                out.push(PlayedBeat {
                                    track: ti,
                                    measure: mi,
                                    start_tick: start,
                                    ticks: len,
                                    string: note.string,
                                    pitch: grace_pitch,
                                    velocity,
                                    dead: grace.dead || grace_pitch.is_none(),
                                    effects,
                                    fade: None,
                                    whammy: None,
                                });
                            }
                            // A trill alternates with another fret; tremolo picking repeats the note.
                            let trill = note.effects.trill.filter(|_| !dead);
                            let period = match (trill, beat.tremolo) {
                                (Some(t), _) => Some(t.speed.ticks()),
                                (None, Some(speed)) => Some(speed.ticks()),
                                (None, None) => None,
                            };
                            let other = trill.and_then(|t| track.tuning.pitch(s, t.fret, track.capo));
                            let parts = attacks(period, ticks_swung, lead);
                            let picks = parts.len();
                            let whammy = beat.whammy.map(|curve| PlayedWhammy { curve, from_tick: s0, ticks: ticks_swung });
                            for (k, (start, len)) in parts.into_iter().enumerate() {
                                let mut effects = note.effects;
                                // Vibrato with the bar shakes every note of the beat.
                                match beat.bar_vibrato {
                                    Some(crate::BarVibrato::Slight) => effects.vibrato = true,
                                    Some(crate::BarVibrato::Wide) => effects.wide_vibrato = true,
                                    None => {}
                                }
                                // Each trill note flows into the next without a new pluck.
                                effects.hammer |= other.is_some() && k + 1 < picks;
                                if k + 1 == picks
                                    && let Some(slot) = ties.get_mut(s)
                                {
                                    *slot = Some(out.len());
                                }
                                if k + 1 == picks
                                    && feel_of.auto_let_ring
                                    && !dead
                                    && !note.effects.palm_mute
                                    && let Some(held) = ringing.get_mut(vi)
                                {
                                    held.push(out.len());
                                }
                                out.push(PlayedBeat {
                                    track: ti,
                                    measure: mi,
                                    start_tick: s0 + u64::from(start),
                                    ticks: len,
                                    string: note.string,
                                    pitch: if k % 2 == 1 { other.or(pitch) } else { pitch },
                                    velocity,
                                    dead,
                                    effects,
                                    fade: beat.fade,
                                    whammy,
                                });
                            }
                        }
                    }
                    at += u64::from(ticks);
                }
            }
            clock += u64::from(capacity);
        }
        // What the automatic let ring still holds rings to the end of the track.
        for idx in ringing.into_iter().flatten() {
            if let Some(n) = out.get_mut(idx) {
                n.ticks = n.ticks.max(u32::try_from(clock.saturating_sub(n.start_tick)).unwrap_or(u32::MAX));
            }
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
    fn beam_groups_follow_the_time_signature_or_the_bar() {
        use crate::{MeasureHeader, default_beaming};
        let ts = |n, d| TimeSignature::new(n, d).unwrap();
        assert_eq!(default_beaming(TimeSignature::COMMON), vec![1, 1, 1, 1]);
        assert_eq!(default_beaming(ts(3, 4)), vec![1, 1, 1]);
        assert_eq!(default_beaming(ts(2, 2)), vec![1, 1]);
        assert_eq!(default_beaming(ts(6, 8)), vec![3, 3]);
        assert_eq!(default_beaming(ts(12, 8)), vec![3, 3, 3, 3]);
        assert_eq!(default_beaming(ts(5, 8)), vec![2, 3]);
        assert_eq!(default_beaming(ts(7, 8)), vec![2, 2, 3]);
        assert_eq!(default_beaming(ts(13, 16)), vec![4, 4, 5]);
        assert_eq!(default_beaming(ts(15, 16)), vec![4, 4, 4, 3]);
        assert_eq!(default_beaming(ts(1, 8)), vec![1]);
        let e = guitario_theory::TICKS_PER_QUARTER / 2;
        let mut h = MeasureHeader::new(ts(7, 8));
        assert_eq!(h.beam_starts(), vec![0, 2 * e, 4 * e]);
        h.beams = vec![3, 2, 2];
        assert_eq!(h.beam_starts(), vec![0, 3 * e, 5 * e]);
        // Groups past the bar are cut; what the groups leave beams on its own.
        h.beams = vec![4, 4, 4];
        assert_eq!(h.beam_starts(), vec![0, 4 * e]);
        h.beams = vec![2];
        assert_eq!(h.beam_starts(), vec![0, 2 * e]);
        assert_eq!(MeasureHeader::new(ts(13, 16)).beam_starts(), vec![0, 2 * e, 4 * e]);
        assert_eq!(MeasureHeader::new(TimeSignature::COMMON).beam_starts(), vec![0, 2 * e, 4 * e, 6 * e]);
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

    #[test]
    fn a_bend_carries_on_through_a_tie() {
        use crate::{Beat, Bend, BendKind, Note};
        use guitario_theory::Duration;
        let tied_after = |bend: Option<Bend>| {
            let mut s = score_with(1);
            let mut head = Note::new(0, 5);
            head.effects.bend = Some(Bend::to(4));
            let mut tail = Note::new(0, 5);
            tail.kind = NoteKind::Tie;
            tail.effects.bend = bend;
            s.tracks[0].measures[0].voices[0].beats = vec![
                Beat { rest: false, notes: vec![head], ..Beat::rest(Duration::QUARTER) },
                Beat { rest: false, notes: vec![tail], ..Beat::rest(Duration::QUARTER) },
                Beat::rest(Duration::new(guitario_theory::NoteValue::Half)),
            ];
            let t = timeline(&s);
            assert_eq!(t.len(), 1, "the tie adds no attack");
            t[0].effects.bend.unwrap()
        };
        // Held: up by the middle of the first half, then kept.
        let held = tied_after(None);
        assert_eq!((held.value_at(7.5), held.value_at(30.0), held.value_at(60.0)), (2.0, 4.0, 4.0));
        // Let down on the tied note.
        let released = tied_after(Some(Bend::preset(BendKind::Release, 4)));
        assert_eq!((released.value_at(30.0), released.value_at(60.0)), (4.0, 0.0));
    }

    #[test]
    fn the_whammy_moves_every_note_of_its_beat_and_carries_on_through_a_tie() {
        use crate::{BarVibrato, Beat, Note, Whammy, WhammyKind};
        use guitario_theory::Duration;
        let q = Duration::QUARTER;
        let dive = Whammy::preset(WhammyKind::Dive, -8);
        let mut s = score_with(1);
        let mut tail = Note::new(1, 5);
        tail.kind = NoteKind::Tie;
        s.tracks[0].measures[0].voices[0].beats = vec![
            Beat { rest: false, notes: vec![Note::new(0, 3), Note::new(1, 5)], whammy: Some(dive), ..Beat::rest(q) },
            Beat { rest: false, notes: vec![tail], whammy: Some(Whammy::preset(WhammyKind::Return, -8)), ..Beat::rest(q) },
            Beat { rest: false, notes: vec![Note::new(2, 0)], bar_vibrato: Some(BarVibrato::Wide), ..Beat::rest(q) },
            Beat { rest: false, notes: vec![Note::new(2, 0)], tremolo: Some(crate::TremoloSpeed::Eighth), whammy: Some(dive), ..Beat::rest(q) },
        ];
        let t = timeline(&s);
        let on = |string: u8, tick: u64| t.iter().find(|p| p.string == string && p.start_tick == tick).unwrap();
        let qt = u64::from(q.ticks());
        // The untied string follows the dive and stays down.
        let w = on(0, 0).whammy.unwrap();
        assert_eq!((w.from_tick, w.ticks, w.semitones_at(qt as f64), w.semitones_at(4.0 * qt as f64)), (0, q.ticks(), -4.0, -4.0));
        // The tied one dives over the first beat, then comes back over the second.
        let w = on(1, 0).whammy.unwrap();
        assert_eq!((w.ticks, w.semitones_at(qt as f64), w.semitones_at(2.0 * qt as f64)), (2 * q.ticks(), -4.0, 0.0));
        // Vibrato with the bar shakes the note; no whammy there.
        let n = on(2, 2 * qt);
        assert!(n.effects.wide_vibrato && n.whammy.is_none());
        // Tremolo picking: every pick follows the one curve of the beat.
        let picks: Vec<&PlayedBeat> = t.iter().filter(|p| p.start_tick >= 3 * qt).collect();
        assert!(picks.len() > 1);
        assert!(picks.iter().all(|p| p.whammy.is_some_and(|w| w.from_tick == 3 * qt && w.semitones_at(0.0) == 0.0)));
        assert_eq!(picks[0].whammy.unwrap().semitones_at(2.0 * qt as f64), 0.0, "before its beat the bar is at rest");
    }

    /// Quarter notes on the top string: `frets`, `None` for a rest.
    fn quarters(s: &mut Score, frets: &[Option<u8>]) {
        use crate::{Beat, Note};
        use guitario_theory::Duration;
        let q = Duration::QUARTER;
        s.tracks[0].measures[0].voices[0].beats =
            frets.iter().map(|f| f.map_or_else(|| Beat::rest(q), |f| Beat { rest: false, notes: vec![Note::new(0, f)], ..Beat::rest(q) })).collect();
    }

    #[test]
    fn accentuation_stresses_the_downbeat_most_and_the_other_beats_less() {
        let mut s = score_with(1);
        quarters(&mut s, &[Some(1), Some(2), Some(3), Some(4)]);
        let plain: Vec<u8> = timeline(&s).iter().map(|p| p.velocity).collect();
        s.tracks[0].interpretation.accentuation = 100;
        let stressed: Vec<u8> = timeline(&s).iter().map(|p| p.velocity).collect();
        assert_eq!(stressed[0], plain[0] + 25);
        assert_eq!(stressed[1], plain[1] + 12);
        // An off-beat eighth is not stressed.
        let mut e = score_with(1);
        eighths(&mut e);
        e.tracks[0].interpretation.accentuation = 100;
        let v: Vec<u8> = timeline(&e).iter().map(|p| p.velocity).collect();
        assert!(v[0] > v[2] && v[2] > v[1], "{v:?}");
    }

    #[test]
    fn an_automatic_brush_strums_chords_low_string_first_and_leaves_single_notes() {
        use crate::{Beat, Note, StrokeDirection};
        use guitario_theory::Duration;
        let mut s = score_with(1);
        let chord = Beat { rest: false, notes: vec![Note::new(0, 0), Note::new(1, 0), Note::new(2, 0)], ..Beat::rest(Duration::WHOLE) };
        s.tracks[0].measures[0].voices[0].beats = vec![chord.clone()];
        assert!(timeline(&s).iter().all(|p| p.start_tick == 0));
        s.tracks[0].interpretation.auto_brush = true;
        let start = |t: &[PlayedBeat], string: u8| t.iter().find(|p| p.string == string).map(|p| p.start_tick);
        let t = timeline(&s);
        assert_eq!(start(&t, 2), Some(0), "the lowest string first");
        assert!(start(&t, 0) > start(&t, 1) && start(&t, 0) <= Some(u64::from(AUTO_BRUSH)));
        // After an upstroke the high string leads.
        s.tracks[0].measures[0].voices[0].beats = vec![Beat { stroke: Some(StrokeDirection::Up), ..chord }];
        assert_eq!(start(&timeline(&s), 0), Some(0));
        // One note is not strummed.
        quarters(&mut s, &[Some(5), None, None, None]);
        assert_eq!(timeline(&s)[0].start_tick, 0);
    }

    #[test]
    fn automatic_let_ring_holds_notes_until_a_rest_or_the_end() {
        let q = TICKS_PER_QUARTER;
        let mut s = score_with(1);
        quarters(&mut s, &[Some(1), Some(2), None, Some(3)]);
        s.tracks[0].interpretation.auto_let_ring = true;
        let t = timeline(&s);
        // The first two ring up to the rest, the last to the end of the bar.
        assert_eq!(t.iter().map(|p| (p.start_tick, p.ticks)).collect::<Vec<_>>(), vec![(0, 2 * q), (u64::from(q), q), (3 * u64::from(q), q)]);
        // Palm-muted notes stay short.
        s.tracks[0].measures[0].voices[0].beats[0].notes[0].effects.palm_mute = true;
        assert_eq!(timeline(&s)[0].ticks, q);
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

    #[test]
    fn attacks_tile_the_beat_exactly_and_are_bounded() {
        assert_eq!(attacks(None, 100, 0), vec![(0, 100)]);
        // A period that does not leave room for two attacks is one attack.
        assert_eq!(attacks(Some(100), 100, 0), vec![(0, 100)]);
        assert_eq!(attacks(Some(40), 100, 0), vec![(0, 40), (40, 40), (80, 20)]);
        // A lead (a brush) delays the first and shortens the whole.
        assert_eq!(attacks(Some(40), 100, 10), vec![(10, 40), (50, 40), (90, 10)]);
        assert_eq!(attacks(Some(40), 100, 500), vec![(99, 1)]);
        assert_eq!(attacks(Some(0), 100, 0), vec![(0, 100)]);
        assert_eq!(attacks(Some(5), 0, 0), vec![(0, 1)]);
        for (period, total, lead) in [(1, u32::MAX, 0), (1, 1_000_000, 7), (3, 1_000_001, 0), (7, 8, 0), (u32::MAX, u32::MAX, 0)] {
            let parts = attacks(Some(period), total, lead);
            assert!(parts.len() <= MAX_ATTACKS as usize, "{period} {total}: {}", parts.len());
            let lead = lead.min(total.max(1) - 1);
            assert_eq!(parts.first().map(|p| p.0), Some(lead));
            assert_eq!(parts.iter().map(|p| u64::from(p.1)).sum::<u64>(), u64::from(total.max(1) - lead));
            assert!(parts.windows(2).all(|w| w[0].0 + w[0].1 == w[1].0), "gaps or overlaps");
        }
    }

    fn one_beat(s: &mut Score, beat: crate::Beat) {
        s.tracks[0].measures[0].voices[0].beats = vec![beat];
    }

    #[test]
    fn tremolo_picking_repeats_every_note_of_the_beat() {
        use crate::{Beat, Note, TremoloSpeed};
        use guitario_theory::{Duration, TICKS_PER_QUARTER};
        let mut s = score_with(1);
        one_beat(
            &mut s,
            Beat { rest: false, notes: vec![Note::new(0, 0), Note::new(1, 2)], tremolo: Some(TremoloSpeed::Sixteenth), ..Beat::rest(Duration::QUARTER) },
        );
        let t = timeline(&s);
        let q = TICKS_PER_QUARTER;
        assert_eq!(t.len(), 8, "two strings, four picks each");
        for string in [0u8, 1] {
            let mine: Vec<_> = t.iter().filter(|p| p.string == string).collect();
            assert_eq!(mine.iter().map(|p| p.start_tick).collect::<Vec<_>>(), vec![0, u64::from(q / 4), u64::from(q / 2), u64::from(3 * q / 4)]);
            assert!(mine.iter().all(|p| p.ticks == q / 4 && p.pitch == mine[0].pitch));
        }
        // Slower speeds pick less often.
        s.tracks[0].measures[0].voices[0].beats[0].tremolo = Some(TremoloSpeed::Eighth);
        assert_eq!(timeline(&s).len(), 4);
        // A beat shorter than the period is picked once; no tremolo is the plain beat.
        one_beat(
            &mut s,
            Beat {
                rest: false,
                notes: vec![Note::new(0, 0)],
                tremolo: Some(TremoloSpeed::Eighth),
                ..Beat::rest(Duration::new(guitario_theory::NoteValue::ThirtySecond))
            },
        );
        assert_eq!(timeline(&s).len(), 1);
        s.tracks[0].measures[0].voices[0].beats[0].tremolo = None;
        assert_eq!(timeline(&s).len(), 1);
    }

    #[test]
    fn a_trill_alternates_two_frets_legato_and_a_dead_note_does_not_trill() {
        use crate::{Beat, Note, NoteKind, Trill, TrillSpeed};
        use guitario_theory::{Duration, TICKS_PER_QUARTER};
        let mut s = score_with(1);
        let mut n = Note::new(2, 5);
        n.effects.trill = Some(Trill { fret: 7, speed: TrillSpeed::ThirtySecond });
        one_beat(&mut s, Beat { rest: false, notes: vec![n], ..Beat::rest(Duration::QUARTER) });
        let t = timeline(&s);
        let tuning = &s.tracks[0].tuning;
        let (low, high) = (tuning.pitch(2, 5, 0), tuning.pitch(2, 7, 0));
        assert_eq!(t.len(), 8, "a quarter at 32nd speed");
        assert_eq!(t.iter().map(|p| p.pitch).collect::<Vec<_>>(), (0..8).map(|i| if i % 2 == 0 { low } else { high }).collect::<Vec<_>>());
        assert!(t.iter().all(|p| p.ticks == TICKS_PER_QUARTER / 8));
        // Each note flows into the next; the last does not, since the note itself is no hammer-on.
        assert_eq!(t.iter().map(|p| p.effects.hammer).collect::<Vec<_>>(), vec![true, true, true, true, true, true, true, false]);
        // A dead note is struck once; the trill does not apply.
        let mut dead = n;
        dead.kind = NoteKind::Dead;
        one_beat(&mut s, Beat { rest: false, notes: vec![dead], ..Beat::rest(Duration::QUARTER) });
        assert_eq!(timeline(&s).len(), 1);
    }

    #[test]
    fn grace_notes_sound_before_the_beat_or_on_it() {
        use crate::{Beat, Grace, GraceDuration, GraceTransition, Note};
        use guitario_theory::{Duration, TICKS_PER_QUARTER};
        let q = TICKS_PER_QUARTER;
        let mut s = score_with(1);
        let mut main = Note::new(2, 7);
        main.effects.grace = Some(Grace { transition: GraceTransition::Hammer, ..Grace::new(5) });
        s.tracks[0].measures[0].voices[0].beats = vec![
            Beat { rest: false, notes: vec![Note::new(2, 3)], ..Beat::rest(Duration::QUARTER) },
            Beat { rest: false, notes: vec![main], ..Beat::rest(Duration::QUARTER) },
            Beat::rest(Duration::new(guitario_theory::NoteValue::Half)),
        ];
        let t = timeline(&s);
        let tuning = &s.tracks[0].tuning;
        let g = q / 8;
        assert_eq!(t.len(), 3, "{t:?}");
        // The first note stops for the grace note, which leads legato into the beat.
        assert_eq!((t[0].start_tick, t[0].ticks), (0, q - g));
        assert_eq!((t[1].start_tick, t[1].ticks, t[1].pitch, t[1].effects.hammer), (u64::from(q - g), g, tuning.pitch(2, 5, 0), true));
        assert_eq!((t[2].start_tick, t[2].ticks, t[2].pitch), (u64::from(q), q, tuning.pitch(2, 7, 0)));
        // On the beat it takes its time from the note.
        if let Some(gr) = s.tracks[0].measures[0].voices[0].beats[1].notes[0].effects.grace.as_mut() {
            gr.on_beat = true;
            gr.duration = GraceDuration::Sixteenth;
        }
        let t = timeline(&s);
        assert_eq!((t[0].start_tick, t[0].ticks), (0, q), "nothing is cut short");
        assert_eq!((t[1].start_tick, t[1].ticks), (u64::from(q), q / 4));
        assert_eq!((t[2].start_tick, t[2].ticks), (u64::from(q + q / 4), q - q / 4));
        // On the first beat of the piece there is nothing to take time from: it goes on the beat.
        let mut first = Note::new(0, 2);
        first.effects.grace = Some(Grace { dead: true, ..Grace::new(0) });
        one_beat(&mut s, Beat { rest: false, notes: vec![first], ..Beat::rest(Duration::WHOLE) });
        let t = timeline(&s);
        assert_eq!(t.len(), 2);
        assert!(t[0].dead && t[0].pitch.is_none() && t[0].start_tick == 0);
        assert_eq!(t[1].start_tick, u64::from(g));
    }

    #[test]
    fn a_bent_grace_note_rises_to_its_note_and_a_tiny_beat_keeps_both() {
        use crate::{Beat, Grace, GraceDuration, GraceTransition, Note};
        use guitario_theory::{Duration, NoteValue};
        let mut s = score_with(1);
        let mut main = Note::new(1, 9);
        main.effects.grace = Some(Grace { transition: GraceTransition::Bend, on_beat: true, duration: GraceDuration::Sixteenth, ..Grace::new(7) });
        one_beat(&mut s, Beat { rest: false, notes: vec![main], ..Beat::rest(Duration::new(NoteValue::SixtyFourth)) });
        let t = timeline(&s);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].effects.bend, t[0].effects.hammer), (Some(crate::Bend::to(4)), true), "a whole tone up");
        assert!(t.iter().all(|p| p.ticks >= 1));
        let total = Duration::new(NoteValue::SixtyFourth).ticks();
        assert_eq!(t[0].ticks + t[1].ticks, total, "{t:?}");
    }

    #[test]
    fn a_tie_extends_the_last_pick_and_a_rest_never_picks() {
        use crate::{Beat, Measure, Note, NoteKind, TremoloSpeed, Voice};
        use guitario_theory::{Duration, TICKS_PER_QUARTER};
        let mut s = score_with(1);
        let picked = Beat { rest: false, notes: vec![Note::new(0, 3)], tremolo: Some(TremoloSpeed::Sixteenth), ..Beat::rest(Duration::QUARTER) };
        let mut tied = Note::new(0, 3);
        tied.kind = NoteKind::Tie;
        let tie = Beat { rest: false, notes: vec![tied], ..Beat::rest(Duration::QUARTER) };
        let rest = Beat { tremolo: Some(TremoloSpeed::Eighth), ..Beat::rest(Duration::QUARTER) };
        let m: &mut Measure = &mut s.tracks[0].measures[0];
        m.voices = vec![Voice { beats: vec![picked, tie, rest] }];
        let t = timeline(&s);
        assert_eq!(t.len(), 4, "four picks; the tie adds none and the rest nothing");
        let q = TICKS_PER_QUARTER;
        assert_eq!(t.last().map(|p| p.ticks), Some(q / 4 + q), "the tie holds the last pick");
    }
}
