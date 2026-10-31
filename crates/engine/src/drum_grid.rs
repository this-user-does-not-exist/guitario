//! The step grid for drum tracks: a bar seen as kit pieces × equal steps (sixteen sixteenths in
//! 4/4 by default), and a command that switches one cell on or off. Switching a step on where no
//! beat starts splits the beat that covers it, so a bar entered on the grid reads as ordinary
//! beats in the tab and the notation.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{Beat, Note, Track, Voice};
use guitario_theory::{Duration, NoteValue, drums};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd(
            "drums.grid",
            "Drum Step Grid",
            "",
            None,
            "measure?: bar (default the cursor's); resolution?: 8, 16 (default), 32 or 64; triplet?: bool. Returns the steps and which pieces sound on each, in the cursor's voice",
            false,
            grid_cmd,
        ),
        cmd(
            "drums.setStep",
            "Drum Step",
            "",
            None,
            "drum: General MIDI number or kit name; step: 0-based; on?: bool (default toggles); measure?, resolution?, triplet? as for drums.grid",
            true,
            set_step,
        ),
    ]
}

/// One bar of a drum voice on a grid of equal steps.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DrumGrid {
    pub steps: usize,
    /// `(General MIDI drum, step)` for every hit that starts on a step.
    pub hits: Vec<(u8, usize)>,
    /// Hits that start between steps (a finer rhythm than the grid shows).
    pub off_grid: usize,
}

impl DrumGrid {
    pub fn has(&self, midi: u8, step: usize) -> bool {
        self.hits.contains(&(midi, step))
    }
}

/// A step's length for a grid of `resolution` (8, 16, 32 or 64), as triplets or not.
pub fn step_duration(resolution: u32, triplet: bool) -> Option<Duration> {
    let value = NoteValue::from_divisor(resolution).filter(|v| v.divisor() >= 8)?;
    Some(Duration { tuplet: if triplet { (3, 2) } else { (1, 1) }, ..Duration::new(value) })
}

/// The grid of `voice` in a bar of `capacity` ticks. `None` when the bar does not divide into
/// whole steps of `step`.
pub fn grid(voice: Option<&Voice>, capacity: u32, step: Duration) -> Option<DrumGrid> {
    let len = step.ticks();
    if len == 0 || !capacity.is_multiple_of(len) {
        return None;
    }
    let mut g = DrumGrid { steps: (capacity / len) as usize, ..DrumGrid::default() };
    let mut at = 0u32;
    for beat in voice.map_or(&[][..], |v| v.beats.as_slice()) {
        if !beat.rest {
            for n in &beat.notes {
                if at.is_multiple_of(len) && at < capacity {
                    g.hits.push((n.fret, (at / len) as usize));
                } else {
                    g.off_grid += 1;
                }
            }
        }
        at = at.saturating_add(beat.duration.ticks());
    }
    Some(g)
}

/// `ticks` as a run of beat lengths, longest first: plain and dotted values, and triplets when
/// `triplet`. `None` when no run adds up exactly.
fn lengths(mut ticks: u32, triplet: bool) -> Option<Vec<Duration>> {
    let mut choices: Vec<Duration> = NoteValue::ALL
        .into_iter()
        .flat_map(|v| {
            let plain = Duration::new(v);
            let mut ds = vec![plain, plain.dotted(1)];
            if triplet {
                ds.push(Duration { tuplet: (3, 2), ..plain });
            }
            ds
        })
        .collect();
    choices.sort_by_key(|d| std::cmp::Reverse(d.ticks()));
    let mut out = Vec::new();
    while ticks > 0 {
        let d = choices.iter().find(|d| d.ticks() <= ticks)?;
        ticks -= d.ticks();
        out.push(*d);
    }
    Some(out)
}

/// Beats for a span of `ticks`: the first is `first` cut to the first length, the rest are rests.
fn span(first: Beat, ticks: u32, triplet: bool) -> Result<Vec<Beat>, EngineError> {
    let ds = lengths(ticks, triplet).ok_or_else(|| EngineError::invalid("the bar's rhythm does not fit this grid"))?;
    let mut ds = ds.into_iter();
    let Some(d0) = ds.next() else { return Ok(Vec::new()) };
    let mut out = vec![Beat { duration: d0, ..first }];
    out.extend(ds.map(Beat::rest));
    Ok(out)
}

/// Makes a beat start at `at` in `voice`, splitting or padding as needed, and returns its index.
fn beat_at(voice: &mut Voice, at: u32, step: u32, triplet: bool) -> Result<usize, EngineError> {
    let mut start = 0u32;
    for i in 0..voice.beats.len() {
        let Some(len) = voice.beats.get(i).map(|b| b.duration.ticks()) else { break };
        if start == at {
            return Ok(i);
        }
        if at < start + len {
            let Some(old) = voice.beats.get(i).cloned() else { break };
            let mut head = span(old, at - start, triplet)?;
            let tail = span(Beat::rest(Duration::QUARTER), start + len - at, triplet)?;
            let index = i + head.len();
            head.extend(tail);
            voice.beats.splice(i..=i, head);
            return Ok(index);
        }
        start += len;
    }
    // Past the last beat of a bar that is not full: rests up to the step, then one step's beat.
    let pad = span(Beat::rest(Duration::QUARTER), at - start, triplet)?;
    let index = voice.beats.len() + pad.len();
    voice.beats.extend(pad);
    voice.beats.extend(span(Beat::rest(Duration::QUARTER), step, triplet)?);
    Ok(index)
}

struct Target {
    measure: usize,
    step: Duration,
    triplet: bool,
}

fn target(s: &Session, params: &Value) -> Result<Target, EngineError> {
    let measure = p::opt_uint(params, "measure", u64::MAX)?.map_or(s.cursor.measure, |m| m as usize);
    if measure >= s.score.headers.len() {
        return Err(EngineError::invalid("no such bar"));
    }
    let resolution = p::opt_uint(params, "resolution", 64)?.unwrap_or(16) as u32;
    let triplet = if params.get("triplet").is_none_or(Value::is_null) { false } else { p::boolean(params, "triplet")? };
    let step = step_duration(resolution, triplet).ok_or_else(|| EngineError::BadParam("`resolution` must be 8, 16, 32 or 64".into()))?;
    Ok(Target { measure, step, triplet })
}

fn drum_track(s: &Session) -> Result<&Track, EngineError> {
    s.score.tracks.get(s.cursor.track).filter(|t| t.is_drums()).ok_or_else(|| EngineError::invalid("the current track is not a drum track"))
}

fn grid_json(g: &DrumGrid) -> Value {
    let mut rows: Vec<(u8, Vec<usize>)> = Vec::new();
    for (midi, step) in &g.hits {
        match rows.iter_mut().find(|(m, _)| m == midi) {
            Some((_, steps)) => steps.push(*step),
            None => rows.push((*midi, vec![*step])),
        }
    }
    rows.sort_by_key(|(m, _)| (drums::lane_for(*m), *m));
    let rows: Vec<Value> =
        rows.into_iter().map(|(midi, steps)| json!({"drum": midi, "name": drums::piece(midi).map_or("Percussion", |p| p.name), "steps": steps})).collect();
    json!({"steps": g.steps, "rows": rows, "offGrid": g.off_grid})
}

fn grid_cmd(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let t = target(s, params)?;
    let track = drum_track(s)?;
    let capacity = s.score.headers.get(t.measure).map_or(0, |h| h.capacity());
    let voice = track.measures.get(t.measure).and_then(|m| m.voices.get(s.cursor.voice));
    let g = grid(voice, capacity, t.step).ok_or_else(|| EngineError::invalid("the bar does not divide into whole steps of this grid"))?;
    Ok(grid_json(&g))
}

fn set_step(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let midi = crate::drums::drum_param(params)?;
    let t = target(s, params)?;
    drum_track(s)?;
    let capacity = s.score.headers.get(t.measure).map_or(0, |h| h.capacity());
    let len = t.step.ticks();
    if len == 0 || !capacity.is_multiple_of(len) {
        return Err(EngineError::invalid("the bar does not divide into whole steps of this grid"));
    }
    let steps = (capacity / len) as usize;
    let step = p::uint(params, "step", steps.saturating_sub(1) as u64)? as usize;
    let want = if params.get("on").is_none_or(Value::is_null) { None } else { Some(p::boolean(params, "on")?) };
    let at = step as u32 * len;
    let lane = drums::lane_for(midi);
    let (track, voice) = (s.cursor.track, s.cursor.voice);
    let was = grid(s.score.tracks.get(track).and_then(|tr| tr.measures.get(t.measure)).and_then(|m| m.voices.get(voice)), capacity, t.step)
        .is_some_and(|g| g.has(midi, step));
    let on = want.unwrap_or(!was);
    if on == was {
        return Ok(json!({"on": on, "steps": steps}));
    }
    let v = crate::session::voice_at_mut(Arc::make_mut(&mut s.score), track, t.measure, voice).ok_or_else(|| EngineError::invalid("no such bar"))?;
    let index = beat_at(v, at, len, t.triplet)?;
    let beat = v.beats.get_mut(index).ok_or_else(|| EngineError::invalid("the step has no beat"))?;
    if on {
        beat.notes.retain(|n| n.string != lane);
        beat.notes.push(Note::new(lane, midi));
        beat.notes.sort_by_key(|n| n.string);
    } else {
        beat.notes.retain(|n| n.fret != midi);
    }
    beat.rest = beat.notes.is_empty();
    s.cursor.measure = t.measure;
    s.cursor.beat = index;
    s.cursor.string = usize::from(lane);
    Ok(json!({"on": on, "steps": steps, "beat": index}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_split_into_plain_dotted_and_triplet_lengths() {
        let q = Duration::QUARTER.ticks();
        assert_eq!(lengths(q * 3 / 4, false), Some(vec![Duration::EIGHTH.dotted(1)]));
        assert_eq!(lengths(q * 5 / 4, false), Some(vec![Duration::QUARTER, Duration::new(NoteValue::Sixteenth)]));
        assert_eq!(lengths(q / 3, false), None);
        assert_eq!(lengths(q / 3, true), Some(vec![Duration { tuplet: (3, 2), ..Duration::EIGHTH }]));
        assert_eq!(lengths(0, false), Some(vec![]));
    }

    #[test]
    fn grids_that_do_not_divide_the_bar_are_refused() {
        // 4/4 in eighth-note triplets is twelve steps; 5/8 in them is not whole.
        let tri = step_duration(8, true);
        assert_eq!(tri.and_then(|d| grid(None, Duration::WHOLE.ticks(), d)).map(|g| g.steps), Some(12));
        assert!(tri.and_then(|d| grid(None, Duration::EIGHTH.ticks() * 5, d)).is_none());
        assert!(step_duration(4, false).is_none() && step_duration(12, false).is_none());
    }
}
