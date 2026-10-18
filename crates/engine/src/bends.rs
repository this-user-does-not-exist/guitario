//! Bends: the curve a note's string is pulled along (Effects › Bend…).

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{BEND_END, BEND_MAX, Bend, BendKind, BendPoint};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd(
        "note.setBend",
        "Bend",
        "Note/Effects",
        None,
        "quarterTones (0 clears, max 12; 4 is a full step) with kind? (bend|release|bend_release|hold|prebend|prebend_bend|prebend_release; default bend), \
         or points ([{pos: 0..=60, value: 0..=12}, ...], 2 to 8 points along the note; null clears)",
        true,
        set_bend,
    )]
}

fn set_bend(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    if s.score.tracks.get(s.cursor.track).is_some_and(guitario_score::Track::is_drums) {
        return Err(EngineError::invalid("a drum hit cannot be bent"));
    }
    let bend = match params.get("points") {
        Some(Value::Null) => None,
        Some(points) => Some(curve(points)?),
        None => {
            let q = p::uint(params, "quarterTones", u64::from(BEND_MAX))? as u8;
            let kind = match p::opt_string(params, "kind") {
                None => BendKind::Bend,
                Some(name) => BendKind::from_name(name).ok_or_else(|| {
                    EngineError::BadParam("`kind` must be bend, release, bend_release, hold, prebend, prebend_bend or prebend_release".into())
                })?,
            };
            (q > 0).then(|| Bend::preset(kind, q))
        }
    };
    s.note_mut()?.effects.bend = bend;
    Ok(json!({"ok": true, "points": bend.map(|b| b.points().iter().map(|pt| json!({"pos": pt.pos, "value": pt.value})).collect::<Vec<_>>())}))
}

/// A curve from `[{pos, value}, ...]`: every position and value must be in range, and the curve
/// must leave the fretted pitch somewhere.
fn curve(points: &Value) -> Result<Bend, EngineError> {
    let list = points.as_array().ok_or_else(|| EngineError::BadParam("`points` must be a list of {pos, value} or null".into()))?;
    if list.len() > guitario_score::BEND_MAX_POINTS {
        return Err(EngineError::BadParam(format!("`points` holds at most {} points", guitario_score::BEND_MAX_POINTS)));
    }
    let mut out = Vec::with_capacity(list.len());
    for item in list {
        out.push(BendPoint::new(p::uint(item, "pos", u64::from(BEND_END))? as u8, p::uint(item, "value", u64::from(BEND_MAX))? as u8));
    }
    Bend::from_points(&out).ok_or_else(|| EngineError::invalid("a bend curve needs a point above the fretted pitch"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_note() -> Session {
        let mut s = Session::new();
        s.run("note.setFret", &json!({"fret": 7})).unwrap();
        s
    }

    fn bend_of(s: &Session) -> Option<Bend> {
        s.score().tracks[0].measures[0].voices[0].beats[0].notes[0].effects.bend
    }

    #[test]
    fn a_height_sets_the_plain_bend_and_zero_clears_it() {
        let mut s = session_with_note();
        s.run("note.setBend", &json!({"quarterTones": 4})).unwrap();
        assert_eq!(bend_of(&s), Some(Bend::to(4)));
        s.run("note.setBend", &json!({"quarterTones": 0})).unwrap();
        assert_eq!(bend_of(&s), None);
    }

    #[test]
    fn every_type_is_a_curve_of_the_chosen_height() {
        let mut s = session_with_note();
        for kind in BendKind::ALL {
            let r = s.run("note.setBend", &json!({"quarterTones": 6, "kind": kind.name()})).unwrap();
            assert_eq!(bend_of(&s), Some(Bend::preset(kind, 6)), "{kind:?}");
            assert!(r["points"].as_array().is_some_and(|a| a.len() >= 2));
        }
        s.run("note.setBend", &json!({"quarterTones": 4, "kind": "bend_release"})).unwrap();
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(bend_of(&s), Some(Bend::preset(BendKind::PrebendRelease, 6)), "one undo step per edit");
    }

    #[test]
    fn points_set_a_free_curve_and_null_clears_it() {
        let mut s = session_with_note();
        s.run("note.setBend", &json!({"points": [{"pos": 0, "value": 0}, {"pos": 20, "value": 4}, {"pos": 40, "value": 2}, {"pos": 60, "value": 6}]})).unwrap();
        let b = bend_of(&s).unwrap();
        assert_eq!((b.peak(), b.end(), b.points().len()), (6, 6, 4));
        assert_eq!(b.kind(false), None);
        s.run("note.setBend", &json!({"points": null})).unwrap();
        assert_eq!(bend_of(&s), None);
    }

    #[test]
    fn bad_curves_and_heights_are_refused_and_change_nothing() {
        let mut s = session_with_note();
        s.run("note.setBend", &json!({"quarterTones": 2})).unwrap();
        for bad in [
            json!({"quarterTones": 13}),
            json!({"quarterTones": 4, "kind": "wobble"}),
            json!({"points": "up"}),
            json!({"points": []}),
            json!({"points": [{"pos": 0, "value": 0}, {"pos": 60, "value": 0}]}),
            json!({"points": [{"pos": 61, "value": 4}]}),
            json!({"points": [{"pos": 0, "value": 13}]}),
            json!({"points": [{"pos": 0}]}),
            json!({"points": (0..9).map(|i| json!({"pos": i * 5, "value": 1 + i % 3})).collect::<Vec<_>>()}),
            json!({}),
        ] {
            assert!(s.run("note.setBend", &bad).is_err(), "{bad}");
            assert_eq!(bend_of(&s), Some(Bend::to(2)), "{bad} must change nothing");
        }
    }

    #[test]
    fn a_rest_and_a_drum_hit_take_no_bend() {
        let mut empty = Session::new();
        assert!(empty.run("note.setBend", &json!({"quarterTones": 4})).is_err(), "no note at the cursor");
        let mut s = Session::new();
        s.run("track.addDrums", &json!({})).unwrap();
        let last = s.score().tracks.len() - 1;
        s.run("cursor.set", &json!({"track": last})).unwrap();
        assert!(s.run("note.setBend", &json!({"quarterTones": 4})).is_err());
    }
}
