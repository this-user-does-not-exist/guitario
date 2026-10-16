//! Chord symbols above beats: Guitar Pro's chord names, with an optional fingering diagram.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{Chord, Track};
use serde_json::{Value, json};

/// Longest chord name accepted, in characters.
const MAX_NAME: usize = 64;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd(
        "beat.setChord",
        "Chord",
        "Beat",
        None,
        "name (empty clears), frets? (array per string, highest first: 0..=36 or null for not played), firstFret? (1..=36)",
        true,
        set_chord,
    )]
}

fn set_chord(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let name = p::string(params, "name")?.trim().to_string();
    if name.chars().count() > MAX_NAME {
        return Err(EngineError::BadParam(format!("`name` is longer than {MAX_NAME} characters")));
    }
    let track = s.score.tracks.get(s.cursor.track).ok_or_else(|| EngineError::invalid("no such track"))?;
    let strings = if track.is_drums() { 0 } else { track.string_count() };
    let frets = match params.get("frets") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) => {
            if list.len() > strings {
                return Err(EngineError::BadParam(format!("`frets` lists {} strings; the track has {strings}", list.len())));
            }
            list.iter()
                .map(|f| match f {
                    Value::Null => Ok(None),
                    v => v
                        .as_u64()
                        .filter(|f| *f <= u64::from(Track::max_fret(track)))
                        .map(|f| Some(f as u8))
                        .ok_or_else(|| EngineError::BadParam("each fret is 0..=36 or null".into())),
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        Some(_) => return Err(EngineError::BadParam("`frets` must be an array".into())),
    };
    let first_fret = p::opt_uint(params, "firstFret", 36)?.map_or(1, |f| f.max(1) as u8);
    let chord = (!name.is_empty() || frets.iter().any(Option::is_some)).then_some(Chord { name, frets, first_fret });
    s.beat_mut()?.chord = chord;
    Ok(json!({"ok": true}))
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::json;

    #[test]
    fn chords_are_named_drawn_from_frets_and_cleared() {
        let mut s = Session::new();
        s.run("beat.setChord", &json!({"name": "Em", "frets": [0, 0, 0, 2, 2, 0]})).unwrap();
        let c = s.score().tracks[0].measures[0].voices[0].beats[0].chord.clone().unwrap();
        assert_eq!((c.name.as_str(), c.frets.len(), c.first_fret), ("Em", 6, 1));
        s.run("beat.setChord", &json!({"name": "D5", "frets": [null, null, null, 7, 5, null], "firstFret": 5})).unwrap();
        let c = s.score().tracks[0].measures[0].voices[0].beats[0].chord.clone().unwrap();
        assert_eq!((c.frets[3], c.frets[0], c.first_fret), (Some(7), None, 5));
        assert!(s.run("beat.setChord", &json!({"name": "X", "frets": [0, 0, 0, 0, 0, 0, 0]})).is_err());
        assert!(s.run("beat.setChord", &json!({"name": "X", "frets": [99]})).is_err());
        assert!(s.run("beat.setChord", &json!({"name": "x".repeat(65)})).is_err());
        s.run("beat.setChord", &json!({"name": ""})).unwrap();
        assert!(s.score().tracks[0].measures[0].voices[0].beats[0].chord.is_none());
        s.run("edit.undo", &json!(null)).unwrap();
        assert!(s.score().tracks[0].measures[0].voices[0].beats[0].chord.is_some());
    }
}
