//! Beat techniques and rhythm: tuplets, pick strokes, slap and pop, fades, brush and arpeggio.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{Beat, BeatTechnique, Brush, BrushKind, Fade, NoteKind, StrokeDirection, TremoloSpeed, Trill, TrillSpeed};
use guitario_theory::{Duration, TICKS_PER_QUARTER};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd(
            "beat.setTuplet",
            "Tuplet",
            "Note/Duration",
            None,
            "enters (1..=13; 1 clears), times? (default: the largest power of two below enters)",
            true,
            set_tuplet,
        ),
        cmd("beat.setStroke", "Pick Stroke", "Effects", None, "stroke: up|down|none", true, set_stroke),
        cmd("beat.setTechnique", "Slap / Pop", "Effects", None, "technique: slap|pop|none", true, set_technique),
        cmd("beat.setFade", "Fade", "Effects", None, "fade: in|out|swell|none", true, set_fade),
        cmd(
            "beat.setBrush",
            "Brush / Arpeggio",
            "Effects",
            None,
            "kind: brush|arpeggio|rasgueado|none, direction?: down|up (default down), duration? (ticks first to last string; default a 32nd)",
            true,
            set_brush,
        ),
        cmd(
            "beat.setTremolo",
            "Tremolo Picking",
            "Effects",
            None,
            "speed: eighth|sixteenth|thirty_second|none (the notes of the beat, or the selection, are picked again and again at that speed)",
            true,
            set_tremolo,
        ),
        cmd(
            "note.setTrill",
            "Trill",
            "Effects",
            None,
            "fret (the other fret of the cursor note's string; null clears), speed? (sixteenth|thirty_second|sixty_fourth; default what the note has, else sixteenth)",
            true,
            set_trill,
        ),
    ]
}

/// Applies `f` to every selected beat, or to the cursor beat.
fn each_beat(s: &mut Session, mut f: impl FnMut(&mut Beat)) -> Result<Value, EngineError> {
    if s.selection.is_some() {
        for b in s.selected_beats_mut()? {
            f(b);
        }
    } else {
        f(s.beat_mut()?);
    }
    Ok(json!({"ok": true}))
}

fn set_tuplet(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let enters = p::uint(params, "enters", 13)? as u8;
    if enters == 0 {
        return Err(EngineError::BadParam("`enters` must be 1..=13".into()));
    }
    // The largest power of two below `enters`: 3:2, 5:4, 7:4, 9:8 …
    let mut default_times = 1u8;
    while default_times.saturating_mul(2) < enters {
        default_times *= 2;
    }
    let times = p::opt_uint(params, "times", 16)?.map_or(default_times, |t| t as u8).max(1);
    let tuplet = if enters == 1 { (1, 1) } else { (enters, times) };
    let probe = Duration { tuplet, ..s.beat_mut()?.duration };
    let base = Duration { tuplet: (1, 1), ..probe }.ticks();
    if (u64::from(base) * u64::from(tuplet.1)) % u64::from(tuplet.0) != 0 {
        return Err(EngineError::BadParam(format!(
            "a {}:{} tuplet of this note value is not exact at {TICKS_PER_QUARTER} ticks per quarter",
            tuplet.0, tuplet.1
        )));
    }
    let cap = s.capacity();
    let idx = s.cursor.beat;
    let voice = s.voice_mut()?;
    let used = voice.ticks();
    let beat = voice.beats.get_mut(idx).ok_or_else(|| EngineError::invalid("the cursor is not on a beat"))?;
    if used - beat.duration.ticks() + probe.ticks() > cap {
        return Err(EngineError::invalid("that duration does not fit in the measure"));
    }
    beat.duration = probe;
    Ok(json!({"tuplet": [tuplet.0, tuplet.1]}))
}

fn opt_enum<T: Copy>(params: &Value, key: &str, table: &[(&str, T)]) -> Result<Option<T>, EngineError> {
    let name = p::string(params, key)?.to_ascii_lowercase();
    if name == "none" {
        return Ok(None);
    }
    table.iter().find(|(n, _)| *n == name).map(|(_, v)| Some(*v)).ok_or_else(|| EngineError::BadParam(format!("unknown `{key}` value `{name}`")))
}

fn set_stroke(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let v = opt_enum(params, "stroke", &[("up", StrokeDirection::Up), ("down", StrokeDirection::Down)])?;
    each_beat(s, |b| b.stroke = v)
}

fn set_technique(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let v = opt_enum(params, "technique", &[("slap", BeatTechnique::Slap), ("pop", BeatTechnique::Pop)])?;
    each_beat(s, |b| b.technique = v)
}

fn set_fade(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let v = opt_enum(params, "fade", &[("in", Fade::In), ("out", Fade::Out), ("swell", Fade::Swell)])?;
    each_beat(s, |b| b.fade = v)
}

fn set_brush(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let kind = opt_enum(params, "kind", &[("brush", BrushKind::Brush), ("arpeggio", BrushKind::Arpeggio), ("rasgueado", BrushKind::Rasgueado)])?;
    let down = match p::opt_string(params, "direction") {
        None | Some("down") => true,
        Some("up") => false,
        Some(other) => return Err(EngineError::BadParam(format!("`direction` must be up or down, not `{other}`"))),
    };
    let ticks = p::opt_uint(params, "duration", u64::from(TICKS_PER_QUARTER) * 4)?.map_or(TICKS_PER_QUARTER / 8, |t| t as u32);
    each_beat(s, |b| b.brush = kind.map(|kind| Brush { kind, down, ticks }))
}

fn set_tremolo(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let name = p::string(params, "speed")?;
    let speed = if name == "none" {
        None
    } else {
        Some(
            TremoloSpeed::from_name(name)
                .ok_or_else(|| EngineError::BadParam(format!("`speed` must be eighth, sixteenth, thirty_second or none, not `{name}`")))?,
        )
    };
    each_beat(s, |b| b.tremolo = if b.rest { None } else { speed })
}

fn set_trill(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let max = {
        let track = s.score.tracks.get(s.cursor.track).ok_or_else(|| EngineError::invalid("the cursor is not on a track"))?;
        if track.is_drums() {
            return Err(EngineError::invalid("a drum hit cannot be trilled"));
        }
        track.max_fret()
    };
    let fret = match params.get("fret") {
        None => return Err(EngineError::BadParam("`fret` must be a fret number or null".into())),
        Some(Value::Null) => None,
        Some(_) => Some(p::uint(params, "fret", u64::from(max))? as u8),
    };
    let speed = match p::opt_string(params, "speed") {
        None => None,
        Some(name) => Some(TrillSpeed::from_name(name).ok_or_else(|| EngineError::BadParam(format!("unknown `speed` value `{name}`")))?),
    };
    if params.get("speed").is_some_and(|v| !v.is_null() && !v.is_string()) {
        return Err(EngineError::BadParam("`speed` must be sixteenth, thirty_second or sixty_fourth".into()));
    }
    let note = s.note_mut()?;
    note.effects.trill = match fret {
        None => None,
        Some(_) if note.kind != NoteKind::Normal => return Err(EngineError::invalid("only a played note can be trilled, not a tie or a dead note")),
        Some(f) if f == note.fret => return Err(EngineError::BadParam("a trill needs another fret than the note's own".into())),
        Some(fret) => Some(Trill { fret, speed: speed.or(note.effects.trill.map(|t| t.speed)).unwrap_or_default() }),
    };
    ok_json()
}

fn ok_json() -> Result<Value, EngineError> {
    Ok(json!({"ok": true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tuplet_times() {
        let mut s = Session::new();
        s.run("beat.setDuration", &json!({"value": 8})).unwrap();
        for (enters, times) in [(3u8, 2u8), (5, 4), (6, 4), (7, 4), (9, 8)] {
            let r = s.run("beat.setTuplet", &json!({"enters": enters})).unwrap();
            assert_eq!(r["tuplet"], json!([enters, times]), "{enters}");
        }
        s.run("beat.setTuplet", &json!({"enters": 1})).unwrap();
        assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].duration.tuplet, (1, 1));
        // 11 is not exact at this resolution.
        assert!(s.run("beat.setTuplet", &json!({"enters": 11})).is_err());
    }

    #[test]
    fn tremolo_picking_is_set_per_beat_or_selection_and_never_on_a_rest() {
        let mut s = Session::new();
        // A rest takes none.
        s.run("beat.setTremolo", &json!({"speed": "sixteenth"})).unwrap();
        assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].tremolo, None);
        s.run("note.setFret", &json!({"fret": 3})).unwrap();
        s.run("beat.setTremolo", &json!({"speed": "thirty_second"})).unwrap();
        assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].tremolo, Some(TremoloSpeed::ThirtySecond));
        s.run("beat.setTremolo", &json!({"speed": "none"})).unwrap();
        assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].tremolo, None);
        for bad in [json!({"speed": "fast"}), json!({"speed": 3}), json!({}), json!({"speed": null})] {
            assert!(s.run("beat.setTremolo", &bad).is_err(), "{bad}");
        }
        // Undo restores the beat.
        s.run("beat.setTremolo", &json!({"speed": "eighth"})).unwrap();
        s.run("edit.undo", &Value::Null).unwrap();
        assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].tremolo, None);
    }

    #[test]
    fn a_trill_needs_a_played_note_another_fret_and_a_real_fret() {
        let mut s = Session::new();
        assert!(s.run("note.setTrill", &json!({"fret": 5})).is_err(), "no note yet");
        s.run("note.setFret", &json!({"fret": 3})).unwrap();
        s.run("note.setTrill", &json!({"fret": 5})).unwrap();
        let note = |s: &Session| s.score().tracks[0].measures[0].voices[0].beats[0].notes[0];
        assert_eq!(note(&s).effects.trill, Some(Trill { fret: 5, speed: TrillSpeed::Sixteenth }));
        // A second call keeps the speed it has unless one is given.
        s.run("note.setTrill", &json!({"fret": 6, "speed": "sixty_fourth"})).unwrap();
        s.run("note.setTrill", &json!({"fret": 4})).unwrap();
        assert_eq!(note(&s).effects.trill, Some(Trill { fret: 4, speed: TrillSpeed::SixtyFourth }));
        s.run("note.setTrill", &json!({"fret": null})).unwrap();
        assert_eq!(note(&s).effects.trill, None);
        for bad in [
            json!({"fret": 3}),
            json!({"fret": 200}),
            json!({"fret": -1}),
            json!({"fret": "5"}),
            json!({}),
            json!({"fret": 5, "speed": "slow"}),
            json!({"fret": 5, "speed": 2}),
        ] {
            assert!(s.run("note.setTrill", &bad).is_err(), "{bad}");
        }
        assert_eq!(note(&s).effects.trill, None);
        // Dead notes, ties and drums cannot be trilled.
        s.run("note.toggleDead", &Value::Null).unwrap();
        assert!(s.run("note.setTrill", &json!({"fret": 5})).is_err());
        let mut drums = Session::new();
        drums.run("track.addDrums", &json!({"name": "Drums"})).unwrap();
        drums.run("track.select", &json!({"index": 1})).unwrap();
        let e = drums.run("note.setTrill", &json!({"fret": 5})).unwrap_err().to_string();
        assert!(e.contains("drum"), "{e}");
    }
}
