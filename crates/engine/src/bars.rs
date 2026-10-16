//! Bar-level commands: clear, double bar, free time, triplet feel.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{DirectionTarget, Jump, JumpEnd, Measure, TripletFeel};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd("measure.clear", "Clear Bar", "Bar", None, "all? (every track; default the current track)", true, clear),
        cmd("measure.toggleDoubleBar", "Double Bar", "Bar", None, "", true, |s, _| toggle(s, |h| &mut h.double_bar)),
        cmd("measure.toggleFreeTime", "Free Time", "Bar", None, "", true, |s, _| toggle(s, |h| &mut h.free_time)),
        cmd(
            "measure.setTripletFeel",
            "Triplet Feel",
            "Bar",
            None,
            "feel: none|triplet8th|triplet16th|dotted8th|dotted16th|scottish8th|scottish16th; to? (last bar index, default this bar only)",
            true,
            set_feel,
        ),
        cmd("measure.setEndings", "Alternate Endings", "Bar", None, "endings: array of ending numbers 1..=8 (empty clears)", true, set_endings),
        cmd(
            "measure.setDirections",
            "Directions",
            "Bar",
            None,
            "targets?: array of segno|segnoSegno|coda|doubleCoda|fine; jump?: daCapo|daCapoAlFine|daCapoAlCoda|daCapoAlDoubleCoda|dalSegno|dalSegnoAlFine|dalSegnoAlCoda|dalSegnoAlDoubleCoda|dalSegnoSegno|dalSegnoSegnoAlFine|dalSegnoSegnoAlCoda|dalSegnoSegnoAlDoubleCoda|toCoda|toDoubleCoda|none",
            true,
            set_directions,
        ),
    ]
}

fn set_endings(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let list = params.get("endings").and_then(Value::as_array).ok_or_else(|| EngineError::BadParam("`endings` must be an array of numbers 1..=8".into()))?;
    let mut bits = 0u8;
    for v in list {
        let n = v.as_u64().filter(|n| (1..=8).contains(n)).ok_or_else(|| EngineError::BadParam("ending numbers are 1..=8".into()))?;
        bits |= 1 << (n - 1);
    }
    s.header_mut()?.endings = bits;
    Ok(json!({"ok": true}))
}

pub(crate) const TARGETS: [(&str, DirectionTarget); 5] = [
    ("segno", DirectionTarget::Segno),
    ("segnoSegno", DirectionTarget::SegnoSegno),
    ("coda", DirectionTarget::Coda),
    ("doubleCoda", DirectionTarget::DoubleCoda),
    ("fine", DirectionTarget::Fine),
];

pub(crate) const JUMPS: [(&str, Jump); 14] = [
    ("daCapo", Jump::DaCapo(JumpEnd::End)),
    ("daCapoAlFine", Jump::DaCapo(JumpEnd::Fine)),
    ("daCapoAlCoda", Jump::DaCapo(JumpEnd::Coda)),
    ("daCapoAlDoubleCoda", Jump::DaCapo(JumpEnd::DoubleCoda)),
    ("dalSegno", Jump::DalSegno(JumpEnd::End)),
    ("dalSegnoAlFine", Jump::DalSegno(JumpEnd::Fine)),
    ("dalSegnoAlCoda", Jump::DalSegno(JumpEnd::Coda)),
    ("dalSegnoAlDoubleCoda", Jump::DalSegno(JumpEnd::DoubleCoda)),
    ("dalSegnoSegno", Jump::DalSegnoSegno(JumpEnd::End)),
    ("dalSegnoSegnoAlFine", Jump::DalSegnoSegno(JumpEnd::Fine)),
    ("dalSegnoSegnoAlCoda", Jump::DalSegnoSegno(JumpEnd::Coda)),
    ("dalSegnoSegnoAlDoubleCoda", Jump::DalSegnoSegno(JumpEnd::DoubleCoda)),
    ("toCoda", Jump::ToCoda),
    ("toDoubleCoda", Jump::ToDoubleCoda),
];

/// The command names of direction targets, for menus.
pub fn direction_targets() -> &'static [(&'static str, DirectionTarget)] {
    &TARGETS
}

fn set_directions(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let targets = match params.get("targets") {
        None => None,
        Some(Value::Array(list)) => {
            let mut out: Vec<DirectionTarget> = Vec::new();
            for v in list {
                let name = v.as_str().unwrap_or_default();
                let t = TARGETS.iter().find(|(n, _)| *n == name).map(|(_, t)| *t).ok_or_else(|| EngineError::BadParam(format!("unknown target `{name}`")))?;
                if !out.contains(&t) {
                    out.push(t);
                }
            }
            Some(out)
        }
        Some(_) => return Err(EngineError::BadParam("`targets` must be an array".into())),
    };
    let jump = match params.get("jump") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(n)) if n == "none" => Some(None),
        Some(Value::String(n)) => {
            Some(Some(JUMPS.iter().find(|(k, _)| k == n).map(|(_, j)| *j).ok_or_else(|| EngineError::BadParam(format!("unknown jump `{n}`")))?))
        }
        Some(_) => return Err(EngineError::BadParam("`jump` must be a string or null".into())),
    };
    if targets.is_none() && jump.is_none() {
        return Err(EngineError::BadParam("give `targets`, `jump` or both".into()));
    }
    let h = s.header_mut()?;
    if let Some(t) = targets {
        h.targets = t;
    }
    if let Some(j) = jump {
        h.jump = j;
    }
    Ok(json!({"ok": true}))
}

fn clear(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let all = params.get("all").map(|_| p::boolean(params, "all")).transpose()?.unwrap_or(false);
    let (mi, ti) = (s.cursor.measure, s.cursor.track);
    let score = Arc::make_mut(&mut s.score);
    let header = score.headers.get(mi).cloned().ok_or_else(|| EngineError::invalid("no such measure"))?;
    for (i, t) in score.tracks.iter_mut().enumerate() {
        if (all || i == ti)
            && let Some(m) = t.measures.get_mut(mi)
        {
            *m = Measure::empty_for(&header);
        }
    }
    s.cursor.beat = 0;
    Ok(json!({"ok": true}))
}

fn toggle(s: &mut Session, field: fn(&mut guitario_score::MeasureHeader) -> &mut bool) -> Result<Value, EngineError> {
    let f = field(s.header_mut()?);
    *f = !*f;
    Ok(json!({"on": *f}))
}

fn set_feel(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let name = p::string(params, "feel")?;
    let feel = match name.to_ascii_lowercase().as_str() {
        "none" => TripletFeel::None,
        "triplet8th" => TripletFeel::Triplet8th,
        "triplet16th" => TripletFeel::Triplet16th,
        "dotted8th" => TripletFeel::Dotted8th,
        "dotted16th" => TripletFeel::Dotted16th,
        "scottish8th" => TripletFeel::Scottish8th,
        "scottish16th" => TripletFeel::Scottish16th,
        _ => return Err(EngineError::BadParam(format!("unknown triplet feel `{name}`"))),
    };
    let from = s.cursor.measure;
    let count = s.score.headers.len();
    let to = p::opt_uint(params, "to", 1_000_000)?.map_or(from, |t| t as usize);
    if to < from || to >= count {
        return Err(EngineError::BadParam("`to` must be a bar at or after the cursor".into()));
    }
    let score = Arc::make_mut(&mut s.score);
    for h in score.headers.iter_mut().skip(from).take(to - from + 1) {
        h.triplet_feel = feel;
    }
    Ok(json!({"ok": true}))
}
