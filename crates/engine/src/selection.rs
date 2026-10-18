//! Beat selection and transposition, as in Guitar Pro: Shift+arrows, drag or ⌘A select a range
//! of beats on the current track (the clipboard that works on it is `clipboard.rs`).

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::Beat;
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

fn transpose(s: &mut Session, semis: i16) -> Result<Value, EngineError> {
    if s.score.tracks.get(s.cursor.track).is_some_and(guitario_score::Track::is_drums) {
        // On a drum track the number is the instrument: transposing would swap the kit pieces.
        return Err(EngineError::invalid("drum hits can't be transposed"));
    }
    let (strings, capo) = s.score.tracks.get(s.cursor.track).map(|t| (t.tuning.strings.clone(), t.capo)).unwrap_or_default();
    let shift = |b: &mut Beat| -> Result<(), EngineError> {
        // A chord built in the Chord window moves with its notes.
        if let Some(chord) = b.chord.as_mut() {
            crate::chords::transpose_chord(chord, semis, &strings, capo);
        }
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
