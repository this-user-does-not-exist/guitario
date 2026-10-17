//! Beat selection, clipboard and transposition, as in Guitar Pro: Shift+arrows, drag or ⌘A
//! select a range of beats on the current track; copy, cut and paste work on whole beats and
//! paste overwrites from the cursor, flowing into following measures.

use crate::commands::{CommandSpec, cmd};
use crate::session::append_measure;
use crate::{EngineError, Session, params as p};
use guitario_score::{Beat, MeasureHeader, Voice};
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;

/// A range of beats on one track, from `anchor` (where it started) to `head` (the cursor end).
/// Positions are `(measure, beat)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Selection {
    pub track: usize,
    pub anchor: (usize, usize),
    pub head: (usize, usize),
}

impl Selection {
    /// `(first, last)` in score order, both inclusive.
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.head { (self.anchor, self.head) } else { (self.head, self.anchor) }
    }

    pub fn contains(&self, measure: usize, beat: usize) -> bool {
        let (a, b) = self.ordered();
        (measure, beat) >= a && (measure, beat) <= b
    }
}

/// Copied beats, one fragment per measure the selection touched.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub fragments: Vec<Vec<Beat>>,
}

const MAX_PASTE_MEASURES: usize = 10_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd("select.extendLeft", "Extend Selection Left", "", Some("Shift+Left"), "", false, |s, _| extend(s, false)),
        cmd("select.extendRight", "Extend Selection Right", "", Some("Shift+Right"), "", false, |s, _| extend(s, true)),
        cmd("select.set", "Select Range", "", None, "fromMeasure, fromBeat, toMeasure, toBeat", false, select_set),
        cmd("select.all", "Select All", "Edit", Some("Cmd+A"), "", false, select_all),
        cmd("select.clear", "Deselect", "Edit", Some("Escape"), "", false, |s, _| {
            s.selection = None;
            Ok(json!({"ok": true}))
        }),
        cmd("edit.copy", "Copy", "Edit", Some("Cmd+C"), "", false, copy),
        cmd("edit.cut", "Cut", "Edit", Some("Cmd+X"), "", true, cut),
        cmd("edit.paste", "Paste", "Edit", Some("Cmd+V"), "", true, paste),
        cmd("edit.transposeUp", "Transpose Up a Semitone", "Edit", Some("Cmd+Up"), "", true, |s, _| transpose(s, 1)),
        cmd("edit.transposeDown", "Transpose Down a Semitone", "Edit", Some("Cmd+Down"), "", true, |s, _| transpose(s, -1)),
        cmd("edit.transpose", "Transpose…", "Edit", None, "semitones (-24..=24): the selection, or the note at the cursor", true, |s, p| {
            let n = p::int(p, "semitones", -24, 24)?;
            transpose(s, i16::try_from(n).unwrap_or(0))
        }),
    ]
}

impl Session {
    pub fn selection(&self) -> Option<Selection> {
        self.selection
    }

    pub fn has_clipboard(&self) -> bool {
        self.clipboard.is_some()
    }

    fn position_exists(&self, track: usize, (m, b): (usize, usize)) -> bool {
        // A voice the bar lacks reads as one bar-long rest.
        self.score.tracks.get(track).and_then(|t| t.measures.get(m)).is_some_and(|x| b < x.voices.get(self.cursor.voice).map_or(1, |v| v.beats.len()))
    }

    /// Drops a selection that no longer points at real beats (after undo, deletes …).
    pub(crate) fn validate_selection(&mut self) {
        if let Some(sel) = self.selection
            && (!self.position_exists(sel.track, sel.anchor) || !self.position_exists(sel.track, sel.head) || sel.track != self.cursor.track)
        {
            self.selection = None;
        }
    }

    /// Every selected beat, mutably, in score order. Errors when nothing is selected.
    pub(crate) fn selected_beats_mut(&mut self) -> Result<Vec<&mut Beat>, EngineError> {
        let sel = self.selection.ok_or_else(|| EngineError::invalid("nothing is selected"))?;
        let ((m0, b0), (m1, b1)) = sel.ordered();
        let voice = self.cursor.voice;
        let track = Arc::make_mut(&mut self.score).tracks.get_mut(sel.track).ok_or_else(|| EngineError::invalid("no such track"))?;
        Ok(track
            .measures
            .iter_mut()
            .enumerate()
            .filter(|(m, _)| (m0..=m1).contains(m))
            .flat_map(|(m, meas)| {
                meas.voices
                    .get_mut(voice)
                    .into_iter()
                    .flat_map(move |v| v.beats.iter_mut().enumerate().filter(move |(b, _)| (m > m0 || *b >= b0) && (m < m1 || *b <= b1)).map(|(_, beat)| beat))
            })
            .collect())
    }

    fn beats_in(&self, m: usize) -> usize {
        self.voice_in(m).map_or(1, |v| v.beats.len())
    }
}

fn extend(s: &mut Session, right: bool) -> Result<Value, EngineError> {
    let c = s.cursor;
    let anchor = s.selection.filter(|x| x.track == c.track).map_or((c.measure, c.beat), |x| x.anchor);
    // Step over existing beats only; extending never creates beats.
    let (m, b) = if right {
        if c.beat + 1 < s.beats_in(c.measure) {
            (c.measure, c.beat + 1)
        } else if c.measure + 1 < s.score.headers.len() {
            (c.measure + 1, 0)
        } else {
            (c.measure, c.beat)
        }
    } else if c.beat > 0 {
        (c.measure, c.beat - 1)
    } else if c.measure > 0 {
        (c.measure - 1, s.beats_in(c.measure - 1).saturating_sub(1))
    } else {
        (c.measure, c.beat)
    };
    s.cursor.measure = m;
    s.cursor.beat = b;
    s.selection = Some(Selection { track: c.track, anchor, head: (m, b) });
    Ok(json!({"selection": s.selection}))
}

fn select_set(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let from = (p::uint(params, "fromMeasure", 1_000_000)? as usize, p::uint(params, "fromBeat", 1_000_000)? as usize);
    let to = (p::uint(params, "toMeasure", 1_000_000)? as usize, p::uint(params, "toBeat", 1_000_000)? as usize);
    let track = s.cursor.track;
    if !s.position_exists(track, from) || !s.position_exists(track, to) {
        return Err(EngineError::invalid("the selection is outside the track"));
    }
    s.selection = Some(Selection { track, anchor: from, head: to });
    s.cursor.measure = to.0;
    s.cursor.beat = to.1;
    Ok(json!({"selection": s.selection}))
}

fn select_all(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let last = s.score.headers.len().saturating_sub(1);
    let head = (last, s.beats_in(last).saturating_sub(1));
    s.selection = Some(Selection { track: s.cursor.track, anchor: (0, 0), head });
    Ok(json!({"selection": s.selection}))
}

/// The selection, or the beat under the cursor.
fn range(s: &Session) -> ((usize, usize), (usize, usize)) {
    s.selection.filter(|x| x.track == s.cursor.track).map_or(((s.cursor.measure, s.cursor.beat), (s.cursor.measure, s.cursor.beat)), |x| x.ordered())
}

fn copy(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let ((m0, b0), (m1, b1)) = range(s);
    let track = s.score.tracks.get(s.cursor.track).ok_or_else(|| EngineError::invalid("no such track"))?;
    let mut fragments = Vec::new();
    for m in m0..=m1 {
        let beats = track.measures.get(m).and_then(|x| x.voices.get(s.cursor.voice)).map(|v| v.beats.as_slice()).unwrap_or_default();
        let start = if m == m0 { b0 } else { 0 };
        let end = if m == m1 { b1.saturating_add(1) } else { beats.len() };
        fragments.push(beats.get(start.min(beats.len())..end.min(beats.len())).unwrap_or_default().to_vec());
    }
    let count: usize = fragments.iter().map(Vec::len).sum();
    if count == 0 {
        return Err(EngineError::invalid("nothing to copy"));
    }
    s.clipboard = Some(Clip { fragments });
    Ok(json!({"beats": count}))
}

fn cut(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let r = copy(s, params)?;
    clear_range(s)?;
    Ok(r)
}

/// Turns the selected beats (or the cursor beat) into rests, keeping their durations.
pub(crate) fn clear_range(s: &mut Session) -> Result<(), EngineError> {
    if s.selection.is_some() {
        for b in s.selected_beats_mut()? {
            b.notes.clear();
            b.rest = true;
        }
    } else {
        let b = s.beat_mut()?;
        b.notes.clear();
        b.rest = true;
    }
    Ok(())
}

/// Overwrites `voice` from beat `at` with `clip`: whole beats covering the clip's length are
/// replaced, and any time left over becomes rests. Fails if the measure would overflow.
fn paste_into(voice: &mut Voice, at: usize, clip: &[Beat], cap: u32) -> Result<(), EngineError> {
    let at = at.min(voice.beats.len());
    let clip_ticks: u32 = clip.iter().map(|b| b.duration.ticks()).sum();
    let mut end = at;
    let mut removed = 0u32;
    while removed < clip_ticks
        && let Some(b) = voice.beats.get(end)
    {
        removed += b.duration.ticks();
        end += 1;
    }
    let before: u32 = voice.beats.iter().take(at).map(|b| b.duration.ticks()).sum();
    let after: u32 = voice.beats.iter().skip(end).map(|b| b.duration.ticks()).sum();
    let mut pad = removed.saturating_sub(clip_ticks);
    if before + clip_ticks.max(removed) + after > cap {
        return Err(EngineError::invalid("the clipboard does not fit in the measure"));
    }
    let mut fill = Vec::new();
    let mut guard = 0;
    while pad > 0 && guard < 64 {
        // The longest plain rest that fits what is left.
        let Some(d) = guitario_theory::NoteValue::ALL.into_iter().map(guitario_theory::Duration::new).find(|x| x.ticks() <= pad) else { break };
        pad -= d.ticks();
        fill.push(Beat::rest(d));
        guard += 1;
    }
    voice.beats.drain(at..end.min(voice.beats.len()));
    let insert: Vec<Beat> = clip.iter().cloned().chain(fill).collect();
    voice.beats.splice(at..at, insert);
    if voice.beats.is_empty() {
        return Err(EngineError::invalid("paste left an empty measure"));
    }
    Ok(())
}

fn paste(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let clip = s.clipboard.clone().ok_or_else(|| EngineError::invalid("the clipboard is empty"))?;
    if clip.fragments.len() > MAX_PASTE_MEASURES {
        return Err(EngineError::invalid("the clipboard is too large"));
    }
    let c = s.cursor;
    let strings = s.score.tracks.get(c.track).map_or(0, |t| t.string_count());
    if clip.fragments.iter().flatten().flat_map(|b| &b.notes).any(|n| usize::from(n.string) >= strings) {
        return Err(EngineError::invalid("the clipboard uses strings this track does not have"));
    }
    let score = Arc::make_mut(&mut s.score);
    let mut beats = 0;
    for (i, frag) in clip.fragments.iter().enumerate() {
        if frag.is_empty() {
            continue;
        }
        let m = c.measure + i;
        while score.headers.len() <= m {
            let h = score.headers.last().cloned().unwrap_or_else(|| MeasureHeader::new(Default::default()));
            append_measure(score, h);
        }
        let cap = score.headers.get(m).map_or(0, MeasureHeader::capacity);
        let voice = crate::session::voice_at_mut(score, c.track, m, c.voice).ok_or_else(|| EngineError::invalid("no such measure"))?;
        paste_into(voice, if i == 0 { c.beat } else { 0 }, frag, cap)?;
        beats += frag.len();
    }
    s.selection = None;
    Ok(json!({"beats": beats}))
}

fn transpose(s: &mut Session, semis: i16) -> Result<Value, EngineError> {
    if s.score.tracks.get(s.cursor.track).is_some_and(guitario_score::Track::is_drums) {
        // On a drum track the number is the instrument: transposing would swap the kit pieces.
        return Err(EngineError::invalid("drum hits can't be transposed"));
    }
    let shift = |b: &mut Beat| -> Result<(), EngineError> {
        for n in &mut b.notes {
            let f = i16::from(n.fret) + semis;
            if !(0..=36).contains(&f) {
                return Err(EngineError::invalid("a note would leave the fretboard (0–36)"));
            }
            n.fret = f as u8;
        }
        Ok(())
    };
    if s.selection.is_some() {
        for b in s.selected_beats_mut()? {
            shift(b)?;
        }
    } else {
        let string = s.cursor.string as u8;
        let b = s.beat_mut()?;
        let n = b.notes.iter_mut().find(|n| n.string == string).ok_or_else(|| EngineError::invalid("no note at the cursor"))?;
        let f = i16::from(n.fret) + semis;
        if !(0..=36).contains(&f) {
            return Err(EngineError::invalid("the note would leave the fretboard (0–36)"));
        }
        n.fret = f as u8;
    }
    Ok(json!({"ok": true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_theory::Duration;

    fn beat(f: u8, d: Duration) -> Beat {
        Beat { rest: false, notes: vec![guitario_score::Note::new(0, f)], ..Beat::rest(d) }
    }

    #[test]
    fn paste_overwrites_and_pads() {
        let mut v =
            Voice { beats: vec![Beat::rest(Duration::new(guitario_theory::NoteValue::Half)), Beat::rest(Duration::new(guitario_theory::NoteValue::Half))] };
        let q = guitario_theory::TICKS_PER_QUARTER;
        paste_into(&mut v, 0, &[beat(3, Duration::QUARTER)], 4 * q).unwrap();
        // A quarter replaced the first half; a quarter rest fills the gap.
        assert_eq!(v.beats.iter().map(|b| b.duration.ticks()).collect::<Vec<_>>(), vec![q, q, 2 * q]);
        assert_eq!(v.ticks(), 4 * q);
        assert!(paste_into(&mut v, 2, &[beat(1, Duration::WHOLE)], 4 * q).is_err());
    }
}
