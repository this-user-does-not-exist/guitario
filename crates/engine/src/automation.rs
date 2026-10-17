//! Sound › Automations › Volume and Pan: a track's volume or pan from a bar on, jumping there or
//! gliding from the point before. Tempo automation is `measure.setTempo` (it belongs to every track).

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use serde_json::{Value, json};
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd(
        "track.setAutomation",
        "Volume / Pan Automation",
        "Sound/Automations",
        None,
        "kind (\"volume\" 0..=127 or \"pan\" -63..=63), value (null clears the point), linear? (glide from the point before; default false), \
         measure? (0-based; default the cursor's), track? (index; default the cursor's)",
        true,
        set_automation,
    )]
}

fn set_automation(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let kind = p::string(params, "kind")?;
    let (lo, hi) = match kind {
        "volume" => (0, 127),
        "pan" => (-63, 63),
        other => return Err(EngineError::BadParam(format!("`kind` {other:?} must be \"volume\" or \"pan\""))),
    };
    let value = if params.get("value").is_none_or(Value::is_null) { None } else { Some(p::int(params, "value", lo, hi)?) };
    let linear = params.get("linear").filter(|v| !v.is_null()).map(|_| p::boolean(params, "linear")).transpose()?.unwrap_or(false);
    let track = p::opt_uint(params, "track", 10_000)?.map_or(s.cursor.track, |t| t as usize);
    let bar = p::opt_uint(params, "measure", 1_000_000)?.map_or(s.cursor.measure, |m| m as usize);
    let m = Arc::make_mut(&mut s.score)
        .tracks
        .get_mut(track)
        .ok_or_else(|| EngineError::BadParam(format!("`track` {track}: no such track")))?
        .measures
        .get_mut(bar)
        .ok_or_else(|| EngineError::BadParam(format!("`measure` {bar}: no such bar")))?;
    let glides = linear && value.is_some();
    match kind {
        "volume" => (m.volume, m.volume_linear) = (value.map(|v| v as u8), glides),
        _ => (m.pan, m.pan_linear) = (value.map(|v| v as i8), glides),
    }
    Ok(json!({"track": track, "measure": bar}))
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::{Value, json};

    #[test]
    fn points_are_set_cleared_and_checked() {
        let mut s = Session::new();
        s.run("measure.append", &Value::Null).unwrap();
        s.run("track.add", &json!({"name": "Bass", "tuning": "bass"})).unwrap();
        s.run("track.setAutomation", &json!({"kind": "volume", "value": 30, "track": 1, "measure": 0})).unwrap();
        s.run("track.setAutomation", &json!({"kind": "volume", "value": 110, "linear": true, "track": 1, "measure": 1})).unwrap();
        s.run("track.setAutomation", &json!({"kind": "pan", "value": -40, "track": 1, "measure": 1})).unwrap();
        let m = &s.score().tracks[1].measures;
        assert_eq!((m[0].volume, m[1].volume, m[1].volume_linear, m[1].pan, m[1].pan_linear), (Some(30), Some(110), true, Some(-40), false));
        assert_eq!(s.score().mix_curves(1).0[0], Some((30.0, 110.0)));
        s.run("track.setAutomation", &json!({"kind": "volume", "value": null, "track": 1, "measure": 1})).unwrap();
        assert_eq!((s.score().tracks[1].measures[1].volume, s.score().tracks[1].measures[1].volume_linear), (None, false));
        // One undo step each.
        s.run("edit.undo", &Value::Null).unwrap();
        assert_eq!(s.score().tracks[1].measures[1].volume, Some(110));
        for bad in [
            json!({"kind": "volume", "value": 128}),
            json!({"kind": "pan", "value": 64}),
            json!({"kind": "tempo", "value": 1}),
            json!({"kind": "pan", "value": 0, "track": 5}),
            json!({"kind": "pan", "value": 0, "measure": 9}),
            json!({"value": 3}),
        ] {
            assert!(s.run("track.setAutomation", &bad).is_err(), "{bad}");
        }
    }
}
