use crate::EngineError;
use guitario_score::{Beat, Measure, MeasureHeader, Score, Voice};
use guitario_theory::{Duration, NoteValue};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub track: usize,
    pub measure: usize,
    pub beat: usize,
    pub string: usize,
}

type Snapshot = (Arc<Score>, Cursor);
const MAX_HISTORY: usize = 500;

pub struct Session {
    pub(crate) score: Arc<Score>,
    pub(crate) cursor: Cursor,
    pub(crate) undo: Vec<Snapshot>,
    pub(crate) redo: Vec<Snapshot>,
    pub(crate) path: Option<PathBuf>,
    pub(crate) dirty: bool,
    pub(crate) selection: Option<crate::Selection>,
    pub(crate) clipboard: Option<crate::Clip>,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Session {
        Session::with_score(Score::new())
    }

    pub fn with_score(score: Score) -> Session {
        let mut s = Session {
            score: Arc::new(score),
            cursor: Cursor::default(),
            undo: vec![],
            redo: vec![],
            path: None,
            dirty: false,
            selection: None,
            clipboard: None,
        };
        s.clamp_cursor();
        s
    }

    /// An immutable snapshot, cheap to hold while the UI draws.
    pub fn score(&self) -> &Arc<Score> {
        &self.score
    }

    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        (Arc::clone(&self.score), self.cursor)
    }

    pub(crate) fn push_undo(&mut self, snap: Snapshot) {
        self.undo.push(snap);
        if self.undo.len() > MAX_HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    pub(crate) fn restore(&mut self, snap: Snapshot) {
        self.score = snap.0;
        self.cursor = snap.1;
        self.clamp_cursor();
        self.validate_selection();
    }

    pub(crate) fn replace_score(&mut self, score: Score, path: Option<PathBuf>) {
        self.score = Arc::new(score);
        self.cursor = Cursor::default();
        self.undo.clear();
        self.redo.clear();
        self.path = path;
        self.dirty = false;
        self.selection = None;
        self.clamp_cursor();
    }

    /// Keeps the cursor on a real beat and string after anything structural.
    pub(crate) fn clamp_cursor(&mut self) {
        let s = &self.score;
        let c = &mut self.cursor;
        c.track = c.track.min(s.tracks.len().saturating_sub(1));
        c.measure = c.measure.min(s.headers.len().saturating_sub(1));
        let track = s.tracks.get(c.track);
        let beats = track.and_then(|t| t.measures.get(c.measure)).and_then(|m| m.voices.first()).map_or(1, |v| v.beats.len());
        c.beat = c.beat.min(beats.saturating_sub(1));
        c.string = c.string.min(track.map_or(0, |t| t.string_count()).saturating_sub(1));
    }

    pub(crate) fn beat_mut(&mut self) -> Result<&mut Beat, EngineError> {
        let c = self.cursor;
        Arc::make_mut(&mut self.score)
            .tracks
            .get_mut(c.track)
            .and_then(|t| t.measures.get_mut(c.measure))
            .and_then(|m| m.voices.first_mut())
            .and_then(|v| v.beats.get_mut(c.beat))
            .ok_or_else(|| EngineError::invalid("the cursor is not on a beat"))
    }

    pub(crate) fn voice_mut(&mut self) -> Result<&mut Voice, EngineError> {
        let c = self.cursor;
        Arc::make_mut(&mut self.score)
            .tracks
            .get_mut(c.track)
            .and_then(|t| t.measures.get_mut(c.measure))
            .and_then(|m| m.voices.first_mut())
            .ok_or_else(|| EngineError::invalid("the cursor is not in a measure"))
    }

    pub(crate) fn capacity(&self) -> u32 {
        self.score.headers.get(self.cursor.measure).map_or(0, MeasureHeader::capacity)
    }

    /// Moves one beat right. Pads the current measure with a rest while it has room, then crosses
    /// into the next measure, appending one at the very end.
    pub fn move_right(&mut self) -> Result<(), EngineError> {
        let cap = self.capacity();
        let (len, used, prefer) = {
            let v = self.voice_mut()?;
            (v.beats.len(), v.ticks(), v.beats.last().map_or(Duration::QUARTER, |b| b.duration))
        };
        if self.cursor.beat + 1 < len {
            self.cursor.beat += 1;
        } else if used < cap {
            let d = fit_duration(cap - used, prefer);
            self.voice_mut()?.beats.push(Beat::rest(d));
            self.cursor.beat += 1;
        } else if self.cursor.measure + 1 < self.score.headers.len() {
            self.cursor.measure += 1;
            self.cursor.beat = 0;
        } else {
            let header = self.score.headers.last().cloned().unwrap_or_else(|| MeasureHeader::new(Default::default()));
            append_measure(Arc::make_mut(&mut self.score), header);
            self.cursor.measure += 1;
            self.cursor.beat = 0;
        }
        Ok(())
    }

    pub fn move_left(&mut self) {
        if self.cursor.beat > 0 {
            self.cursor.beat -= 1;
        } else if self.cursor.measure > 0 {
            self.cursor.measure -= 1;
            let last = self
                .score
                .tracks
                .get(self.cursor.track)
                .and_then(|t| t.measures.get(self.cursor.measure))
                .and_then(|m| m.voices.first())
                .map_or(1, |v| v.beats.len());
            self.cursor.beat = last.saturating_sub(1);
        }
    }

    pub fn move_string(&mut self, delta: i64) {
        let max = self.score.tracks.get(self.cursor.track).map_or(0, |t| t.string_count()).saturating_sub(1);
        self.cursor.string = (self.cursor.string as i64 + delta).clamp(0, max as i64) as usize;
    }
}

/// `prefer` if it fits in `remaining` ticks, otherwise the longest plain value that does.
pub(crate) fn fit_duration(remaining: u32, prefer: Duration) -> Duration {
    if prefer.ticks() <= remaining {
        return prefer;
    }
    let plain = Duration::new(prefer.value);
    if plain.ticks() <= remaining {
        return plain;
    }
    NoteValue::ALL.into_iter().map(Duration::new).find(|d| d.ticks() <= remaining).unwrap_or(Duration::new(NoteValue::SixtyFourth))
}

pub(crate) fn append_measure(score: &mut Score, header: MeasureHeader) {
    let fresh = MeasureHeader { tempo: None, repeat_open: false, repeat_close: 0, marker: None, ..header };
    for t in &mut score.tracks {
        t.measures.push(Measure::empty_for(&fresh));
    }
    score.headers.push(fresh);
}
