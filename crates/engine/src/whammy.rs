//! The whammy bar: the curve the bridge is moved along over a beat (Effects › Whammy Bar…), and
//! vibrato played with the bar.

use crate::commands::{CommandSpec, cmd};
use crate::techniques::each_beat;
use crate::{EngineError, Session, params as p};
use guitario_score::{BarVibrato, WHAMMY_END, WHAMMY_MAX, WHAMMY_MAX_POINTS, WHAMMY_MIN, Whammy, WhammyKind, WhammyPoint};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd(
            "beat.setWhammy",
            "Whammy Bar",
            "Effects",
            None,
            "depth (quarter tones, -24..=24: negative dives, 4 is a whole tone; 0 clears) with kind? (dive|dip|return|hold|predive|predive_dive; default dive), \
             or points ([{pos: 0..=60, value: -24..=24}, ...], 2 to 8 points along the beat; null clears). The beat, or every beat of the selection",
            true,
            set_whammy,
        ),
        cmd(
            "beat.setBarVibrato",
            "Vibrato with Whammy Bar",
            "Effects",
            None,
            "vibrato: slight|wide|none (the beat, or every beat of the selection)",
            true,
            set_bar_vibrato,
        ),
    ]
}

/// The bar only moves strings that sound: a drum track has none, and a rest needs a note.
fn check_beat(s: &Session) -> Result<(), EngineError> {
    if s.score.tracks.get(s.cursor.track).is_some_and(guitario_score::Track::is_drums) {
        return Err(EngineError::invalid("a drum track has no whammy bar"));
    }
    let c = s.cursor;
    let beat = s.score.tracks.get(c.track).and_then(|t| t.measures.get(c.measure)).and_then(|m| m.voices.get(c.voice)).and_then(|v| v.beats.get(c.beat));
    if s.selection.is_none() && beat.is_none_or(|b| b.rest) {
        return Err(EngineError::invalid("put the cursor on a beat with notes"));
    }
    Ok(())
}

fn set_whammy(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let whammy = match params.get("points") {
        Some(Value::Null) => None,
        Some(points) => Some(curve(points)?),
        None => {
            let depth = p::int(params, "depth", i64::from(WHAMMY_MIN), i64::from(WHAMMY_MAX))? as i8;
            let kind = match p::opt_string(params, "kind") {
                None => WhammyKind::Dive,
                Some(name) => WhammyKind::from_name(name)
                    .ok_or_else(|| EngineError::BadParam("`kind` must be dive, dip, return, hold, predive or predive_dive".into()))?,
            };
            (depth != 0).then(|| Whammy::preset(kind, depth))
        }
    };
    check_beat(s)?;
    each_beat(s, |b| b.whammy = if b.rest { None } else { whammy })?;
    Ok(json!({"ok": true, "points": whammy.map(|w| w.points().iter().map(|pt| json!({"pos": pt.pos, "value": pt.value})).collect::<Vec<_>>())}))
}

/// A curve from `[{pos, value}, ...]`: every position and value must be in range, and the curve
/// must leave the written pitch somewhere.
fn curve(points: &Value) -> Result<Whammy, EngineError> {
    let list = points.as_array().ok_or_else(|| EngineError::BadParam("`points` must be a list of {pos, value} or null".into()))?;
    if list.len() > WHAMMY_MAX_POINTS {
        return Err(EngineError::BadParam(format!("`points` holds at most {WHAMMY_MAX_POINTS} points")));
    }
    let mut out = Vec::with_capacity(list.len());
    for item in list {
        let pos = p::uint(item, "pos", u64::from(WHAMMY_END))? as u8;
        let value = p::int(item, "value", i64::from(WHAMMY_MIN), i64::from(WHAMMY_MAX))? as i8;
        out.push(WhammyPoint::new(pos, value));
    }
    Whammy::from_points(&out).ok_or_else(|| EngineError::invalid("a whammy curve needs a point away from the written pitch"))
}

fn set_bar_vibrato(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let name = p::string(params, "vibrato")?;
    let vibrato = if name == "none" {
        None
    } else {
        Some(BarVibrato::from_name(name).ok_or_else(|| EngineError::BadParam(format!("`vibrato` must be slight, wide or none, not `{name}`")))?)
    };
    check_beat(s)?;
    each_beat(s, |b| b.bar_vibrato = if b.rest { None } else { vibrato })
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::Beat;

    fn session_with_note() -> Session {
        let mut s = Session::new();
        s.run("note.setFret", &json!({"fret": 7})).unwrap();
        s
    }

    fn first_beat(s: &Session) -> &Beat {
        &s.score().tracks[0].measures[0].voices[0].beats[0]
    }

    #[test]
    fn a_depth_sets_a_dive_and_zero_clears_it() {
        let mut s = session_with_note();
        s.run("beat.setWhammy", &json!({"depth": -8})).unwrap();
        assert_eq!(first_beat(&s).whammy, Some(Whammy::preset(WhammyKind::Dive, -8)));
        s.run("beat.setWhammy", &json!({"depth": 0})).unwrap();
        assert_eq!(first_beat(&s).whammy, None);
    }

    #[test]
    fn every_kind_is_a_curve_of_the_chosen_depth_in_one_undo_step() {
        let mut s = session_with_note();
        for kind in WhammyKind::ALL {
            let r = s.run("beat.setWhammy", &json!({"depth": -6, "kind": kind.name()})).unwrap();
            assert_eq!(first_beat(&s).whammy, Some(Whammy::preset(kind, -6)), "{kind:?}");
            assert!(r["points"].as_array().is_some_and(|a| a.len() >= 2));
        }
        s.run("beat.setWhammy", &json!({"depth": 3, "kind": "dip"})).unwrap();
        s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(first_beat(&s).whammy, Some(Whammy::preset(WhammyKind::PrediveDive, -6)));
    }

    #[test]
    fn points_set_a_free_curve_and_null_clears_it() {
        let mut s = session_with_note();
        s.run("beat.setWhammy", &json!({"points": [{"pos": 0, "value": 0}, {"pos": 20, "value": -12}, {"pos": 40, "value": 4}, {"pos": 60, "value": 0}]}))
            .unwrap();
        let w = first_beat(&s).whammy.unwrap();
        assert_eq!((w.depth(), w.points().len(), w.kind(false)), (-12, 4, None));
        s.run("beat.setWhammy", &json!({"points": null})).unwrap();
        assert_eq!(first_beat(&s).whammy, None);
    }

    #[test]
    fn bad_curves_and_depths_are_refused_and_change_nothing() {
        let mut s = session_with_note();
        s.run("beat.setWhammy", &json!({"depth": -2})).unwrap();
        let before = first_beat(&s).whammy;
        for bad in [
            json!({"depth": -25}),
            json!({"depth": 25}),
            json!({"depth": "deep"}),
            json!({"depth": -4, "kind": "bomb"}),
            json!({"points": "down"}),
            json!({"points": []}),
            json!({"points": [{"pos": 0, "value": 0}, {"pos": 60, "value": 0}]}),
            json!({"points": [{"pos": 61, "value": -4}]}),
            json!({"points": [{"pos": 0, "value": -25}]}),
            json!({"points": [{"pos": 0}]}),
            json!({"points": (0..9).map(|i| json!({"pos": i * 5, "value": -1 - i % 3})).collect::<Vec<_>>()}),
            json!({}),
        ] {
            assert!(s.run("beat.setWhammy", &bad).is_err(), "{bad}");
            assert_eq!(first_beat(&s).whammy, before, "{bad} must change nothing");
        }
    }

    #[test]
    fn a_selection_sets_every_beat_with_notes_and_skips_rests() {
        let mut s = Session::new();
        for fret in [3, 5] {
            s.run("beat.setDuration", &json!({"value": 4})).unwrap();
            s.run("note.setFret", &json!({"fret": fret})).unwrap();
            s.run("cursor.right", &json!({})).unwrap();
        }
        s.run("select.set", &json!({"fromMeasure": 0, "fromBeat": 0, "toMeasure": 0, "toBeat": 2})).unwrap();
        s.run("beat.setWhammy", &json!({"depth": -4, "kind": "dip"})).unwrap();
        s.run("beat.setBarVibrato", &json!({"vibrato": "wide"})).unwrap();
        let beats = &s.score().tracks[0].measures[0].voices[0].beats;
        assert!(beats[0].whammy.is_some() && beats[1].whammy.is_some());
        assert_eq!(beats[1].bar_vibrato, Some(BarVibrato::Wide));
        assert!(beats.iter().filter(|b| b.rest).all(|b| b.whammy.is_none() && b.bar_vibrato.is_none()));
    }

    #[test]
    fn vibrato_with_the_bar_is_set_and_cleared() {
        let mut s = session_with_note();
        s.run("beat.setBarVibrato", &json!({"vibrato": "slight"})).unwrap();
        assert_eq!(first_beat(&s).bar_vibrato, Some(BarVibrato::Slight));
        assert!(s.run("beat.setBarVibrato", &json!({"vibrato": "shaky"})).is_err());
        assert!(s.run("beat.setBarVibrato", &json!({})).is_err());
        s.run("beat.setBarVibrato", &json!({"vibrato": "none"})).unwrap();
        assert_eq!(first_beat(&s).bar_vibrato, None);
    }

    #[test]
    fn a_rest_and_a_drum_track_take_no_whammy() {
        let mut empty = Session::new();
        assert!(empty.run("beat.setWhammy", &json!({"depth": -4})).is_err(), "the cursor beat is a rest");
        assert!(empty.run("beat.setBarVibrato", &json!({"vibrato": "wide"})).is_err());
        let mut s = Session::new();
        s.run("track.addDrums", &json!({})).unwrap();
        let last = s.score().tracks.len() - 1;
        s.run("cursor.set", &json!({"track": last})).unwrap();
        s.run("note.setFret", &json!({"fret": 36})).ok();
        assert!(s.run("beat.setWhammy", &json!({"depth": -4})).is_err());
    }
}
