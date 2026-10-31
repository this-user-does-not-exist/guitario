//! Drum tracks: adding a kit and entering hits. A hit is a note whose fret is the General MIDI drum
//! number, on the lane the kit map gives it (`guitario_theory::drums`).

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{Note, Track};
use guitario_theory::drums;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd("track.addDrums", "Add Drum Track", "Track", None, "name?", true, add_drums),
        cmd(
            "note.setDrum",
            "Drum Hit",
            "",
            None,
            "drum: General MIDI number (35-81 …), a Guitar Pro articulation (91 rim shot, 92 half-open hi-hat, 93 ride edge, 94-98 ride, splash, china, crash and crash 2 choked) or a kit name such as \"kick\", \"snare\", \"hh\", \"crc\"; toggles the hit at the cursor beat, and another piece on its lane takes its place",
            true,
            set_drum,
        ),
    ]
}

fn add_drums(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let score = Arc::make_mut(&mut s.score);
    if score.tracks.len() >= 15 {
        return Err(EngineError::invalid("a score holds at most 15 tracks"));
    }
    let name = p::opt_string(params, "name").map_or_else(|| "Drums".to_string(), |n| n.chars().take(100).collect());
    let track = Track::drums(name, &score.headers);
    score.tracks.push(track);
    s.cursor.track = s.score.tracks.len() - 1;
    s.cursor.string = usize::from(drums::lane_for(36));
    s.selection = None;
    Ok(json!({"track": s.cursor.track}))
}

pub(crate) fn drum_param(params: &Value) -> Result<u8, EngineError> {
    match params.get("drum") {
        Some(Value::Number(n)) => n
            .as_u64()
            .filter(|m| (1..=u64::from(drums::MAX_DRUM)).contains(m))
            .map(|m| m as u8)
            .ok_or_else(|| EngineError::BadParam("`drum` must be a General MIDI drum number 1..=127".into())),
        Some(Value::String(name)) => drums::by_name(name).map(|p| p.midi).ok_or_else(|| EngineError::BadParam(format!("unknown drum `{name}`"))),
        _ => Err(EngineError::BadParam("`drum` must be a number or a kit name".into())),
    }
}

fn set_drum(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let midi = drum_param(params)?;
    let c = s.cursor;
    if !s.score.tracks.get(c.track).is_some_and(Track::is_drums) {
        return Err(EngineError::invalid("the current track is not a drum track"));
    }
    let lane = drums::lane_for(midi);
    let beat = s.beat_mut()?;
    match beat.notes.iter().position(|n| n.string == lane) {
        Some(i) if beat.notes.get(i).is_some_and(|n| n.fret == midi) => {
            beat.notes.remove(i);
        }
        // Another piece or articulation on the lane (a crash choked): the hit keeps its accent,
        // ghost, flam and the rest.
        Some(i) => {
            if let Some(n) = beat.notes.get_mut(i) {
                n.fret = midi;
                n.kind = guitario_score::NoteKind::Normal;
                if let Some(g) = n.effects.grace.as_mut().filter(|g| drums::lane_for(g.fret) == lane) {
                    g.fret = midi;
                }
            }
        }
        None => {
            beat.notes.push(Note::new(lane, midi));
            beat.notes.sort_by_key(|n| n.string);
        }
    }
    beat.rest = beat.notes.is_empty();
    s.cursor.string = usize::from(lane);
    Ok(json!({"lane": lane, "on": beat_has(s, lane, midi)}))
}

fn beat_has(s: &Session, lane: u8, midi: u8) -> bool {
    s.voice_in(s.cursor.measure).and_then(|v| v.beats.get(s.cursor.beat)).is_some_and(|b| b.notes.iter().any(|n| n.string == lane && n.fret == midi))
}
