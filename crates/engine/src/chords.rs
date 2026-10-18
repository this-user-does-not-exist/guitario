//! Chord symbols above beats: Guitar Pro's chord names, with an optional fingering diagram.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{Chord, Track};
use guitario_theory::{Alteration, ChordSpec, ChordType, Extension};
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
            "name (empty clears; with `built`, empty takes the built name), frets? (array per string, highest first: 0..=36 or null for not played), firstFret? (1..=36), fingers? (array per string: 0 thumb, 1..=4, or null), built? (object as chord.build takes: the chord keeps its structure and transposes)",
            true,
            set_chord,
        ),
        cmd(
            "chord.build",
            "Build Chord",
            "",
            None,
            "root (C, C#, Db … B, or 0..=11), flats?, type? (M, 7, 7M, 6, m, m7, m7M, m6, sus2, sus4, 7sus2, 7sus4, dim, dim7, m7b5, aug, 5), extension? (9, 11, 13, add9, add11, add13), fifth?/ninth?/eleventh?/thirteenth? (\"flat\" or \"sharp\"), bass? (a note name) or inversion? (0..=3), omit? (tone labels such as [\"5\"]), limit? (1..=100, default 24): the name, tones and the diagrams that play it on the cursor's track, easiest first",
            false,
            build,
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
    beat.chord = Some(Chord { name: name.clone(), frets, first_fret, ..Chord::default() });
    Ok(json!({"name": name}))
}

/// A note name (`C`, `F#`, `Bb`) or a pitch class number as a pitch class.
pub(crate) fn pitch_class(v: &Value, key: &str) -> Result<u8, EngineError> {
    if let Some(n) = v.as_u64() {
        return u8::try_from(n).ok().filter(|n| *n < 12).ok_or_else(|| EngineError::BadParam(format!("`{key}` must be 0..=11 or a note name")));
    }
    let text = v.as_str().ok_or_else(|| EngineError::BadParam(format!("`{key}` must be a note name such as C, F# or Bb")))?.trim();
    let mut chars = text.chars();
    let base: i16 = match chars.next().map(|c| c.to_ascii_uppercase()) {
        Some('C') => 0,
        Some('D') => 2,
        Some('E') => 4,
        Some('F') => 5,
        Some('G') => 7,
        Some('A') => 9,
        Some('B') => 11,
        _ => return Err(EngineError::BadParam(format!("`{key}` {text:?} is not a note name"))),
    };
    let shift: i16 = match chars.as_str() {
        "" => 0,
        "#" | "♯" => 1,
        "b" | "♭" => -1,
        other => return Err(EngineError::BadParam(format!("`{key}` has an unknown accidental {other:?}"))),
    };
    Ok((base + shift).rem_euclid(12) as u8)
}

/// A chord spec from `chord.build`'s parameters.
pub(crate) fn spec_of(params: &Value) -> Result<ChordSpec, EngineError> {
    let root = pitch_class(params.get("root").ok_or_else(|| EngineError::BadParam("`root` is required".into()))?, "root")?;
    let kind = match p::opt_string(params, "type") {
        None => ChordType::Major,
        Some(t) => ChordType::from_label(t).ok_or_else(|| {
            EngineError::BadParam(format!("`type` {t:?} is not one of M, 7, 7M, 6, m, m7, m7M, m6, sus2, sus4, 7sus2, 7sus4, dim, dim7, m7b5, aug, 5"))
        })?,
    };
    let mut spec = ChordSpec::new(root, kind);
    if let Some(e) = p::opt_string(params, "extension").filter(|e| !e.is_empty()) {
        spec.extension =
            Extension::from_label(e).ok_or_else(|| EngineError::BadParam(format!("`extension` {e:?} is not one of 9, 11, 13, add9, add11, add13")))?;
    }
    if params.get("flats").is_some_and(|v| !v.is_null()) {
        spec.flats = p::boolean(params, "flats")?;
    } else {
        spec.flats = params.get("root").and_then(Value::as_str).is_some_and(|r| r.ends_with('b') || r.ends_with('♭'));
    }
    let alterable = spec.alterable();
    for (i, key) in ["fifth", "ninth", "eleventh", "thirteenth"].into_iter().enumerate() {
        let alt = match p::opt_string(params, key) {
            None => continue,
            Some("flat") => Alteration::Flat,
            Some("sharp") => Alteration::Sharp,
            Some(other) => return Err(EngineError::BadParam(format!("`{key}` must be \"flat\" or \"sharp\", not {other:?}"))),
        };
        if !alterable.get(i).copied().unwrap_or(false) {
            return Err(EngineError::BadParam(format!("this chord has no {key} to alter")));
        }
        match i {
            0 => spec.fifth = alt,
            1 => spec.ninth = alt,
            2 => spec.eleventh = alt,
            _ => spec.thirteenth = alt,
        }
    }
    if let Some(b) = params.get("bass").filter(|v| !v.is_null()) {
        let b = pitch_class(b, "bass")?;
        spec.bass = (b != spec.root).then_some(b);
    } else if let Some(n) = p::opt_uint(params, "inversion", 3)? {
        if n as usize >= spec.tones().len() {
            return Err(EngineError::BadParam("`inversion` is beyond the chord's tones".into()));
        }
        spec.invert(n as usize);
    }
    if let Some(list) = params.get("omit").filter(|v| !v.is_null()) {
        let list = list.as_array().ok_or_else(|| EngineError::BadParam("`omit` must be an array of tone labels".into()))?;
        let tones = spec.tones();
        for v in list {
            let label = v.as_str().ok_or_else(|| EngineError::BadParam("`omit` holds tone labels such as \"5\"".into()))?;
            let i = tones.iter().position(|t| t.label == label).ok_or_else(|| EngineError::BadParam(format!("the chord has no tone {label:?} to omit")))?;
            spec.omit |= 1 << i;
        }
        if spec.pitch_classes().is_empty() {
            return Err(EngineError::BadParam("every tone is omitted".into()));
        }
    }
    Ok(spec)
}

/// The JSON of a built chord's parameters, as `chord.build` and `beat.setChord` take them.
pub fn spec_json(spec: &ChordSpec) -> Value {
    let alt = |a: Alteration| match a {
        Alteration::None => Value::Null,
        Alteration::Flat => json!("flat"),
        Alteration::Sharp => json!("sharp"),
    };
    let omit: Vec<&str> = spec.tones().iter().filter(|t| t.omitted).map(|t| t.label).collect();
    json!({
        "root": spec.root, "flats": spec.flats, "type": spec.kind.label(), "extension": spec.extension.label(),
        "fifth": alt(spec.fifth), "ninth": alt(spec.ninth), "eleventh": alt(spec.eleventh), "thirteenth": alt(spec.thirteenth),
        "bass": spec.bass, "omit": omit,
    })
}

fn build(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let spec = spec_of(params)?;
    let limit = p::opt_uint(params, "limit", 100)?.unwrap_or(24).max(1) as usize;
    let track = s.score.tracks.get(s.cursor.track).ok_or_else(|| EngineError::invalid("no such track"))?;
    if track.is_drums() {
        return Err(EngineError::invalid("a drum track has no chord diagrams"));
    }
    let tones: Vec<Value> = spec
        .tones()
        .iter()
        .map(|t| json!({"label": t.label, "note": guitario_theory::pitch_class_name(t.pitch_class(spec.root), spec.flats), "omitted": t.omitted}))
        .collect();
    let voicings: Vec<Value> = guitario_theory::voicings(&spec, &track.tuning.strings, track.capo, limit)
        .iter()
        .map(|v| json!({"frets": v.frets, "fingers": v.fingers, "firstFret": v.first_fret(), "barre": v.barre, "difficulty": v.difficulty}))
        .collect();
    Ok(
        json!({"name": spec.name(), "tones": tones, "inversion": spec.inversion(), "alterable": spec.alterable(), "voicings": voicings, "spec": spec_json(&spec)}),
    )
}

/// A built chord `semitones` away: its name follows, and its diagram moves along the neck, or is
/// played again from the easiest voicing when it would leave the neck or has open strings.
pub(crate) fn transpose_chord(chord: &mut Chord, semitones: i16, open: &[u8], capo: u8) {
    let Some(spec) = chord.built else { return };
    let moved = spec.transposed(semitones);
    chord.built = Some(moved);
    chord.name = moved.name();
    if chord.frets.iter().all(Option::is_none) {
        return;
    }
    let shifted: Option<Vec<Option<u8>>> = chord
        .frets
        .iter()
        .map(|f| match f {
            None => Some(None),
            Some(0) => None,
            Some(f) => u8::try_from(i16::from(*f) + semitones).ok().filter(|f| (1..=24).contains(f)).map(Some),
        })
        .collect();
    match shifted {
        Some(frets) => {
            let first = i16::from(chord.first_fret) + semitones;
            chord.first_fret = u8::try_from(first).ok().filter(|f| *f >= 1).unwrap_or(1);
            chord.frets = frets;
        }
        None => {
            if let Some(v) = guitario_theory::voicings(&moved, open, capo, 1).into_iter().next() {
                chord.first_fret = v.first_fret();
                chord.frets = v.frets;
                chord.fingers = v.fingers;
            }
        }
    }
}

fn set_chord(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let built = match params.get("built").filter(|v| !v.is_null()) {
        Some(b) => Some(spec_of(b)?),
        None => None,
    };
    let mut name = p::string(params, "name")?.trim().to_string();
    if name.is_empty()
        && let Some(b) = &built
    {
        name = b.name();
    }
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
    let fingers = match params.get("fingers") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) => {
            if list.len() > frets.len() {
                return Err(EngineError::BadParam("`fingers` lists more strings than `frets`".into()));
            }
            list.iter()
                .map(|f| match f {
                    Value::Null => Ok(None),
                    v => v
                        .as_u64()
                        .filter(|f| *f <= 4)
                        .map(|f| Some(f as u8))
                        .ok_or_else(|| EngineError::BadParam("each finger is 0 (thumb) to 4, or null".into())),
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        Some(_) => return Err(EngineError::BadParam("`fingers` must be an array".into())),
    };
    let chord = (!name.is_empty() || frets.iter().any(Option::is_some)).then_some(Chord { name, frets, first_fret, fingers, built });
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
    fn a_chord_is_built_listed_applied_and_transposed() {
        let mut s = Session::new();
        let r = s.run("chord.build", &json!({"root": "D#", "type": "m7", "extension": "add11"})).unwrap();
        assert_eq!(r["name"], "D#m7add11");
        let notes: Vec<&str> = r["tones"].as_array().unwrap().iter().map(|t| t["note"].as_str().unwrap()).collect();
        assert_eq!(notes, ["D#", "F#", "A#", "C#", "G#"]);
        assert!(r["voicings"].as_array().unwrap().len() >= 3);
        let r = s.run("chord.build", &json!({"root": "E", "limit": 3})).unwrap();
        assert_eq!(r["voicings"].as_array().unwrap().len(), 3);
        assert_eq!(r["voicings"][0]["frets"], json!([0, 0, 1, 2, 2, 0]), "open E first");
        // Apply the second voicing of a built G/B, then transpose the beat up a whole step.
        let r = s.run("chord.build", &json!({"root": "G", "inversion": 1})).unwrap();
        assert_eq!(r["name"], "G/B");
        let v = r["voicings"][0].clone();
        s.run("beat.setChord", &json!({"name": "", "frets": v["frets"], "fingers": v["fingers"], "firstFret": v["firstFret"], "built": r["spec"]})).unwrap();
        let chord = s.score().tracks[0].measures[0].voices[0].beats[0].chord.clone().unwrap();
        assert_eq!(chord.name, "G/B");
        assert!(chord.built.is_some() && !chord.fingers.is_empty());
        s.run("cursor.set", &json!({"string": 0})).unwrap();
        s.run("note.setFret", &json!({"fret": 3})).unwrap();
        s.run("track.transpose", &json!({"semitones": 2})).unwrap();
        let chord = s.score().tracks[0].measures[0].voices[0].beats[0].chord.clone().unwrap();
        assert_eq!(chord.name, "A/C#");
        let pitches: Vec<u8> = chord.frets.iter().zip([64u8, 59, 55, 50, 45, 40]).filter_map(|(f, o)| f.map(|f| (o + f) % 12)).collect();
        assert!(pitches.iter().all(|p| [9, 1, 4].contains(p)), "the diagram plays A C# E: {:?}", chord.frets);
        // A chord only named stays as it is.
        s.run("beat.setChord", &json!({"name": "Riff"})).unwrap();
        s.run("track.transpose", &json!({"semitones": 1})).unwrap();
        assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].chord.as_ref().map(|c| c.name.as_str()), Some("Riff"));
    }

    #[test]
    fn hostile_chord_builds_are_refused() {
        let mut s = Session::new();
        for bad in [
            json!({}),
            json!({"root": "H"}),
            json!({"root": 12}),
            json!({"root": "C##"}),
            json!({"root": "C", "type": "mega"}),
            json!({"root": "C", "extension": "15"}),
            json!({"root": "C", "ninth": "sharp"}),
            json!({"root": "C", "fifth": "up"}),
            json!({"root": "C", "inversion": 3}),
            json!({"root": "C", "omit": ["9"]}),
            json!({"root": "C", "type": "5", "omit": ["T", "5"]}),
            json!({"root": "C", "limit": 1000}),
        ] {
            assert!(s.run("chord.build", &bad).is_err(), "{bad}");
        }
        assert!(s.run("beat.setChord", &json!({"name": "C", "frets": [0, 1], "fingers": [0, 1, 2]})).is_err());
        assert!(s.run("beat.setChord", &json!({"name": "C", "frets": [0, 1], "fingers": [9, 1]})).is_err());
        assert!(s.run("beat.setChord", &json!({"name": "", "built": {"root": "Q"}})).is_err());
        let mut d = Session::new();
        d.run("track.addDrums", &json!({"name": "Drums"})).unwrap();
        d.run("track.select", &json!({"index": 1})).unwrap();
        assert!(d.run("chord.build", &json!({"root": "C"})).is_err());
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
