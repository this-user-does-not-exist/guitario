//! Bar-level commands: clear, double bar, free time, triplet feel.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{Measure, TripletFeel};
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
    ]
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
