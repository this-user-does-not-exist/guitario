//! Beat techniques and rhythm: tuplets, pick strokes, slap and pop, fades, brush and arpeggio;
//! tremolo picking, trills and grace notes.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{
    BeamOverride, Beat, BeatTechnique, Brush, BrushKind, Fade, Finger, Grace, GraceDuration, GraceTransition, NoteKind, StrokeDirection, TremoloSpeed, Trill,
    TrillSpeed,
};
use guitario_theory::{Duration, TICKS_PER_QUARTER};
use serde_json::{Value, json};
use std::sync::Arc;

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
            "beat.setBeam",
            "Beam",
            "Note/Duration",
            None,
            "mode: break|join|auto (break starts a new beam at the beat, join beams it to the beat before across the bar's grouping, auto follows the bar; the beat or the selection)",
            true,
            set_beam,
        ),
        cmd(
            "beat.setSlur",
            "Slur",
            "Note/Duration",
            None,
            "beats (1..=32: the slur runs from the cursor beat to the beat that many beats later in its voice, across bar lines too; 0 or null removes the slur the cursor beat is under)",
            true,
            set_slur,
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
        cmd(
            "note.setGrace",
            "Grace Note",
            "Note",
            None,
            "fret (on the cursor note's string; on a drum track a drum number or kit name, a flam; null clears), duration? (sixteenth|thirty_second|sixty_fourth), transition? (none|slide|bend|hammer), onBeat? (bool), dead? (bool); what is left out keeps what the grace note has, else thirty_second, none, before the beat, not dead",
            true,
            set_grace,
        ),
        cmd(
            "note.setFingering",
            "Fingering",
            "Note",
            None,
            "left? and right?: thumb|index|middle|ring|little|none (the hand left out keeps what it has)",
            true,
            set_fingering,
        ),
    ]
}

fn set_fingering(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let finger = |key: &str| -> Result<Option<Option<Finger>>, EngineError> {
        match params.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(n)) if n == "none" => Ok(Some(None)),
            Some(Value::String(n)) => Finger::from_name(n)
                .map(|f| Some(Some(f)))
                .ok_or_else(|| EngineError::BadParam(format!("`{key}` must be thumb, index, middle, ring, little or none, not `{n}`"))),
            Some(_) => Err(EngineError::BadParam(format!("`{key}` must be a finger name"))),
        }
    };
    let (left, right) = (finger("left")?, finger("right")?);
    if left.is_none() && right.is_none() {
        return Err(EngineError::BadParam("give `left`, `right` or both".into()));
    }
    if s.score.tracks.get(s.cursor.track).is_some_and(|t| t.is_drums()) {
        return Err(EngineError::invalid("a drum hit has no fingering"));
    }
    let note = s.note_mut()?;
    if let Some(l) = left {
        note.effects.left_finger = l;
    }
    if let Some(r) = right {
        note.effects.right_finger = r;
    }
    ok_json()
}

/// Applies `f` to every selected beat, or to the cursor beat.
pub(crate) fn each_beat(s: &mut Session, mut f: impl FnMut(&mut Beat)) -> Result<Value, EngineError> {
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

fn set_beam(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let name = p::string(params, "mode")?;
    let mode = if name == "auto" {
        None
    } else {
        Some(BeamOverride::from_name(name).ok_or_else(|| EngineError::BadParam(format!("`mode` must be break, join or auto, not `{name}`")))?)
    };
    each_beat(s, |b| b.beam = mode)
}

/// Every beat of the cursor's voice in written order, as `(measure, beat)`.
fn voice_beats(s: &Session) -> Vec<(usize, usize)> {
    let (t, v) = (s.cursor.track, s.cursor.voice);
    let Some(track) = s.score.tracks.get(t) else { return Vec::new() };
    track.measures.iter().enumerate().flat_map(|(mi, m)| (0..m.voices.get(v).map_or(0, |x| x.beats.len())).map(move |bi| (mi, bi))).collect()
}

/// Beat `i` of `order` (from [`voice_beats`]) in voice `v` of track `t`.
fn nth<'a>(score: &'a mut guitario_score::Score, order: &[(usize, usize)], (t, v): (usize, usize), i: usize) -> Option<&'a mut Beat> {
    let &(m, b) = order.get(i)?;
    score.tracks.get_mut(t)?.measures.get_mut(m)?.voices.get_mut(v)?.beats.get_mut(b)
}

/// A slur from the cursor beat to the beat `beats` on in its voice: every beat from the first to
/// the one before the last is slurred on to the next, as Guitar Pro chains `Legato`.
fn set_slur(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let beats = p::opt_uint(params, "beats", 32)?.unwrap_or(0) as usize;
    let order = voice_beats(s);
    let here =
        order.iter().position(|&(m, b)| (m, b) == (s.cursor.measure, s.cursor.beat)).ok_or_else(|| EngineError::invalid("the cursor is not on a beat"))?;
    let (t, v) = (s.cursor.track, s.cursor.voice);
    let score = Arc::make_mut(&mut s.score);
    if beats == 0 {
        // The run the cursor beat is in: back to its first slurred beat, on to its last.
        let mut first = here;
        while first > 0 && nth(score, &order, (t, v), first - 1).is_some_and(|b| b.legato) {
            first -= 1;
        }
        let mut i = first;
        while let Some(b) = nth(score, &order, (t, v), i).filter(|b| b.legato) {
            b.legato = false;
            i += 1;
        }
        return ok_json();
    }
    match nth(score, &order, (t, v), here + beats) {
        None => return Err(EngineError::invalid("there is no beat that far on in this voice")),
        Some(end) if end.rest => return Err(EngineError::invalid("a slur ends on a note, not a rest")),
        Some(_) => {}
    }
    if nth(score, &order, (t, v), here).is_none_or(|b| b.rest) {
        return Err(EngineError::invalid("a slur starts on a note, not a rest"));
    }
    for i in here..here + beats {
        if let Some(b) = nth(score, &order, (t, v), i) {
            b.legato = true;
        }
    }
    ok_json()
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

/// An optional named value: `None` when `key` is absent or null, an error when it names nothing in `from`.
fn opt_named<T>(params: &Value, key: &str, from: fn(&str) -> Option<T>, want: &str) -> Result<Option<T>, EngineError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(name)) => from(name).map(Some).ok_or_else(|| EngineError::BadParam(format!("`{key}` must be {want}, not `{name}`"))),
        Some(_) => Err(EngineError::BadParam(format!("`{key}` must be {want}"))),
    }
}

fn opt_bool(params: &Value, key: &str) -> Result<Option<bool>, EngineError> {
    if params.get(key).is_none_or(Value::is_null) { Ok(None) } else { p::boolean(params, key).map(Some) }
}

fn set_grace(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let max = {
        let track = s.score.tracks.get(s.cursor.track).ok_or_else(|| EngineError::invalid("the cursor is not on a track"))?;
        (track.max_fret(), track.is_drums())
    };
    let (max, drums) = max;
    // On a drum track the grace note is a flam: a drum number or kit name, usually the hit's own.
    let fret = match params.get("fret") {
        None => return Err(EngineError::BadParam("`fret` must be a fret number or null".into())),
        Some(Value::Null) => None,
        Some(Value::String(_)) if drums => Some(crate::drums::drum_param(&json!({"drum": params.get("fret")}))?),
        Some(_) => Some(p::uint(params, "fret", u64::from(max))? as u8),
    };
    let duration = opt_named(params, "duration", GraceDuration::from_name, "sixteenth, thirty_second or sixty_fourth")?;
    let transition = opt_named(params, "transition", GraceTransition::from_name, "none, slide, bend or hammer")?;
    let on_beat = opt_bool(params, "onBeat")?;
    let dead = opt_bool(params, "dead")?;
    let note = s.note_mut()?;
    note.effects.grace = match fret {
        None => None,
        Some(_) if note.kind == NoteKind::Tie => return Err(EngineError::invalid("a tied note is not struck again, so it takes no grace note")),
        Some(fret) => {
            let had = note.effects.grace.unwrap_or(Grace::new(fret));
            Some(Grace {
                fret,
                duration: duration.unwrap_or(had.duration),
                transition: transition.unwrap_or(had.transition),
                on_beat: on_beat.unwrap_or(had.on_beat),
                dead: dead.unwrap_or(had.dead),
            })
        }
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
    fn beam_overrides_are_set_per_beat_or_selection_and_reject_unknown_modes() {
        let mut s = Session::new();
        s.run("beat.setBeam", &json!({"mode": "break"})).unwrap();
        assert_eq!(s.beat_mut().unwrap().beam, Some(BeamOverride::Break));
        s.run("beat.setBeam", &json!({"mode": "join"})).unwrap();
        assert_eq!(s.beat_mut().unwrap().beam, Some(BeamOverride::Join));
        s.run("beat.setBeam", &json!({"mode": "auto"})).unwrap();
        assert_eq!(s.beat_mut().unwrap().beam, None);
        assert!(s.run("beat.setBeam", &json!({"mode": "sideways"})).is_err());
        assert!(s.run("beat.setBeam", &json!({})).is_err());
    }

    #[test]
    fn a_slur_spans_notes_across_bars_and_never_starts_or_ends_on_a_rest() {
        let mut s = Session::new();
        s.run("beat.setDuration", &json!({"value": 2})).unwrap();
        // Two half notes a bar, over two bars.
        for fret in [0, 2, 3, 5] {
            s.run("note.setFret", &json!({"fret": fret})).unwrap();
            s.move_right().unwrap();
        }
        let legato = |s: &Session| -> Vec<bool> { s.score().tracks[0].measures.iter().flat_map(|m| &m.voices[0].beats).map(|b| b.legato).collect() };
        s.cursor.measure = 0;
        s.cursor.beat = 1;
        // From the second beat of bar 1 over the bar line to the second of bar 2.
        s.run("beat.setSlur", &json!({"beats": 2})).unwrap();
        assert_eq!(&legato(&s)[..4], [false, true, true, false]);
        assert!(s.run("beat.setSlur", &json!({"beats": 9})).is_err(), "past the last beat");
        assert!(s.run("beat.setSlur", &json!({"beats": 33})).is_err());
        // Removing it from any beat it covers clears the whole run.
        s.cursor.measure = 1;
        s.cursor.beat = 0;
        s.run("beat.setSlur", &json!({"beats": null})).unwrap();
        assert!(legato(&s).iter().all(|l| !l));
        s.cursor.beat = 2;
        assert!(s.run("beat.setSlur", &json!({"beats": 1})).is_err(), "a rest cannot start one");
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
    fn a_grace_note_keeps_what_it_has_unless_told_and_needs_a_struck_note() {
        let mut s = Session::new();
        assert!(s.run("note.setGrace", &json!({"fret": 5})).is_err(), "no note yet");
        s.run("note.setFret", &json!({"fret": 7})).unwrap();
        s.run("note.setGrace", &json!({"fret": 5})).unwrap();
        let grace = |s: &Session| s.score().tracks[0].measures[0].voices[0].beats[0].notes[0].effects.grace;
        assert_eq!(grace(&s), Some(Grace::new(5)));
        s.run("note.setGrace", &json!({"fret": 9, "duration": "sixteenth", "transition": "hammer", "onBeat": true, "dead": true})).unwrap();
        s.run("note.setGrace", &json!({"fret": 4, "dead": false})).unwrap();
        assert_eq!(grace(&s), Some(Grace { fret: 4, duration: GraceDuration::Sixteenth, transition: GraceTransition::Hammer, on_beat: true, dead: false }));
        // The same fret as the note is allowed: Guitar Pro's default grace note repeats it.
        s.run("note.setGrace", &json!({"fret": 7})).unwrap();
        s.run("note.setGrace", &json!({"fret": null})).unwrap();
        assert_eq!(grace(&s), None);
        for bad in [
            json!({}),
            json!({"fret": 37}),
            json!({"fret": -1}),
            json!({"fret": "5"}),
            json!({"fret": 5, "duration": "eighth"}),
            json!({"fret": 5, "duration": 8}),
            json!({"fret": 5, "transition": "pull"}),
            json!({"fret": 5, "onBeat": "yes"}),
            json!({"fret": 5, "dead": 1}),
        ] {
            assert!(s.run("note.setGrace", &bad).is_err(), "{bad}");
        }
        assert_eq!(grace(&s), None, "a failed call changes nothing");
        // Undo removes it in one step.
        s.run("note.setGrace", &json!({"fret": 2})).unwrap();
        s.run("edit.undo", &Value::Null).unwrap();
        assert_eq!(grace(&s), None);
        // A tied note takes none.
        s.run("note.toggleTie", &Value::Null).unwrap();
        assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].notes[0].kind, NoteKind::Tie);
        assert!(s.run("note.setGrace", &json!({"fret": 5})).is_err());
    }

    #[test]
    fn a_drum_hit_takes_a_flam() {
        let mut s = Session::new();
        s.run("track.addDrums", &json!({"name": "Drums"})).unwrap();
        s.run("track.select", &json!({"index": 1})).unwrap();
        s.run("note.setDrum", &json!({"drum": "snare"})).unwrap();
        let grace = |s: &Session| s.score().tracks[1].measures[0].voices[0].beats[0].notes[0].effects.grace;
        s.run("note.setGrace", &json!({"fret": "snare"})).unwrap();
        assert_eq!(grace(&s).map(|g| g.fret), Some(38));
        s.run("note.setGrace", &json!({"fret": 40})).unwrap();
        assert_eq!(grace(&s).map(|g| g.fret), Some(40));
        assert!(s.run("note.setGrace", &json!({"fret": "cowbell"})).is_err() && s.run("note.setGrace", &json!({"fret": 128})).is_err());
        assert!(s.score().validate().is_ok());
        s.run("note.setGrace", &json!({"fret": null})).unwrap();
        assert_eq!(grace(&s), None);
        // On a guitar a name is not a fret.
        let mut g = Session::new();
        g.run("note.setFret", &json!({"fret": 5})).unwrap();
        assert!(g.run("note.setGrace", &json!({"fret": "snare"})).is_err());
    }

    #[test]
    fn fingering_is_set_per_hand_and_cleared() {
        let mut s = Session::new();
        assert!(s.run("note.setFingering", &json!({"left": "index"})).is_err(), "no note yet");
        s.run("note.setFret", &json!({"fret": 5})).unwrap();
        let e = |s: &Session| s.score().tracks[0].measures[0].voices[0].beats[0].notes[0].effects;
        s.run("note.setFingering", &json!({"left": "ring", "right": "index"})).unwrap();
        assert_eq!((e(&s).left_finger, e(&s).right_finger), (Some(Finger::Ring), Some(Finger::Index)));
        s.run("note.setFingering", &json!({"right": "none"})).unwrap();
        assert_eq!((e(&s).left_finger, e(&s).right_finger), (Some(Finger::Ring), None), "the left hand is kept");
        for bad in [json!({}), json!({"left": "toe"}), json!({"right": 3})] {
            assert!(s.run("note.setFingering", &bad).is_err(), "{bad}");
        }
        s.run("edit.undo", &Value::Null).unwrap();
        assert_eq!(e(&s).right_finger, Some(Finger::Index));
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
