//! Chord symbols above beats: Guitar Pro's chord names, with an optional fingering diagram.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{Chord, Track};
use serde_json::{Value, json};

/// Longest chord name accepted, in characters.
const MAX_NAME: usize = 64;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd(
            "beat.setChord",
            "Chord",
            "Beat",
            None,
            "name (empty clears), frets? (array per string, highest first: 0..=36 or null for not played), firstFret? (1..=36)",
            true,
            set_chord,
        ),
        cmd(
            "beat.chordFromNotes",
            "Chord from Notes",
            "Beat",
            None,
            "name? (default: named from the notes, e.g. E5, Am7, D/F#); the diagram is the beat's notes",
            true,
            chord_from_notes,
        ),
    ]
}

/// The diagram of the beat under the cursor (its played notes, each string's fret), named from its
/// pitches unless a name is given. The diagram starts at the nut when it fits in five frets.
fn chord_from_notes(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let given = match params.get("name") {
        None | Some(Value::Null) => None,
        Some(_) => Some(p::string(params, "name")?.trim().to_string()),
    };
    if given.as_ref().is_some_and(|n| n.chars().count() > MAX_NAME) {
        return Err(EngineError::BadParam(format!("`name` is longer than {MAX_NAME} characters")));
    }
    let track = s.score.tracks.get(s.cursor.track).ok_or_else(|| EngineError::invalid("no such track"))?;
    if track.is_drums() {
        return Err(EngineError::invalid("a drum beat has no chord diagram"));
    }
    let (tuning, capo) = (track.tuning.clone(), track.capo);
    let key = s.score.headers.get(s.cursor.measure).map_or(0, |h| h.key);
    let beat = s.beat_mut()?;
    let mut frets: Vec<Option<u8>> = vec![None; tuning.string_count()];
    let mut pitches = Vec::new();
    for n in beat.notes.iter().filter(|n| n.kind != guitario_score::NoteKind::Dead) {
        if let Some(slot) = frets.get_mut(usize::from(n.string)) {
            *slot = Some(n.fret);
            pitches.extend(tuning.pitch(usize::from(n.string), n.fret, capo).map(|p| p.0));
        }
    }
    if pitches.is_empty() {
        return Err(EngineError::invalid("the beat has no played notes to make a chord of"));
    }
    let fretted = frets.iter().flatten().copied().filter(|f| *f > 0);
    let first_fret = if fretted.clone().max().unwrap_or(0) <= 5 { 1 } else { fretted.min().unwrap_or(1) };
    let name = given.unwrap_or_else(|| guitario_theory::chord_name(&pitches, key < 0).unwrap_or_default());
    beat.chord = Some(Chord { name: name.clone(), frets, first_fret });
    Ok(json!({"name": name}))
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
    fn a_chord_is_made_from_the_beat_and_named_from_its_notes() {
        let mut s = Session::new();
        assert!(s.run("beat.chordFromNotes", &json!({})).is_err(), "a rest has no chord");
        // An A minor shape: x 0 2 2 1 0, entered string by string (string 0 is the high E).
        for (string, fret) in [(4, 0), (3, 2), (2, 2), (1, 1), (0, 0)] {
            s.run("cursor.set", &json!({"string": string})).unwrap();
            s.run("note.setFret", &json!({"fret": fret})).unwrap();
        }
        let r = s.run("beat.chordFromNotes", &json!({})).unwrap();
        assert_eq!(r["name"], "Am");
        let c = s.score().tracks[0].measures[0].voices[0].beats[0].chord.clone().unwrap();
        assert_eq!((c.frets, c.first_fret), (vec![Some(0), Some(1), Some(2), Some(2), Some(0), None], 1));
        // A given name wins; up the neck the diagram starts at the lowest fret.
        let mut s = Session::new();
        for (string, fret) in [(5, 7), (4, 9), (3, 9)] {
            s.run("cursor.set", &json!({"string": string})).unwrap();
            s.run("note.setFret", &json!({"fret": fret})).unwrap();
        }
        assert_eq!(s.run("beat.chordFromNotes", &json!({})).unwrap()["name"], "B5");
        assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].chord.as_ref().map(|c| c.first_fret), Some(7));
        s.run("beat.chordFromNotes", &json!({"name": "Riff chord"})).unwrap();
        assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].chord.as_ref().map(|c| c.name.as_str()), Some("Riff chord"));
        for bad in [json!({"name": 5}), json!({"name": "x".repeat(65)})] {
            assert!(s.run("beat.chordFromNotes", &bad).is_err(), "{bad}");
        }
        // Drums take none.
        let mut d = Session::new();
        d.run("track.addDrums", &json!({"name": "Drums"})).unwrap();
        d.run("track.select", &json!({"index": 1})).unwrap();
        assert!(d.run("beat.chordFromNotes", &json!({})).is_err());
    }

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
