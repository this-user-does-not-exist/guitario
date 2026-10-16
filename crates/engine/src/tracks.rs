//! Track tools: duplicate and reorder.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd("track.duplicate", "Duplicate Track", "Track", None, "", true, duplicate),
        cmd("track.moveUp", "Move Track Up", "Track", None, "", true, |s, _| shift(s, -1)),
        cmd("track.moveDown", "Move Track Down", "Track", None, "", true, |s, _| shift(s, 1)),
    ]
}

fn duplicate(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let i = s.cursor.track;
    let score = Arc::make_mut(&mut s.score);
    if score.tracks.len() >= 15 {
        return Err(EngineError::invalid("a score holds at most 15 tracks"));
    }
    let mut copy = score.tracks.get(i).cloned().ok_or_else(|| EngineError::invalid("no such track"))?;
    copy.name = format!("{} (copy)", copy.name).chars().take(100).collect();
    copy.solo = false;
    score.tracks.insert(i + 1, copy);
    s.cursor.track = i + 1;
    s.selection = None;
    Ok(json!({"track": i + 1}))
}

fn shift(s: &mut Session, d: i64) -> Result<Value, EngineError> {
    let i = s.cursor.track;
    let n = s.score.tracks.len();
    let j = i as i64 + d;
    if i >= n || j < 0 || j as usize >= n {
        return Err(EngineError::invalid(if d < 0 { "the track is already first" } else { "the track is already last" }));
    }
    Arc::make_mut(&mut s.score).tracks.swap(i, j as usize);
    s.cursor.track = j as usize;
    s.selection = None;
    Ok(json!({"track": j}))
}
