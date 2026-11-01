//! A track's sound after the instrument and the song's master section: the effect chain (Guitar
//! Pro's inspector, up to six effects), the mixer's equalizer, master volume and pan, and the three
//! mastering slots. Every effect is described by `sound.effectTypes`, so an agent can find each
//! type's parameters, ranges and presets.

use crate::commands::{CommandSpec, cmd, mixer_track};
use crate::{EngineError, Session, params as p};
use guitario_score::{Effect, EffectType, MASTER_SLOTS, MAX_TRACK_EFFECTS, Unit};
use serde_json::{Map, Value, json};
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd(
            "track.setEffect",
            "Effect",
            "Sound",
            None,
            "slot (0-based; the chain's length adds an effect), type? (\"amp\", \"cabinet\", \"reverb\" …; a new type replaces the slot's effect), \
             preset? (a preset name of the type), on?, params? ({name: value}; see sound.effectTypes), track? (index; default the cursor's). \
             A track that plays its instrument's own chain gets a copy of it first.",
            true,
            set_effect,
        ),
        cmd("track.removeEffect", "Delete Effect", "Sound", None, "slot (0-based), track? (index; default the cursor's)", true, remove_effect),
        cmd("track.moveEffect", "Move Effect", "Sound", None, "slot (0-based), to (0-based), track? (index; default the cursor's)", true, move_effect),
        cmd(
            "track.resetEffects",
            "Reset Effect Chain",
            "Sound",
            None,
            "track? (index; default the cursor's): plays the instrument's own chain again",
            true,
            reset_effects,
        ),
        cmd("track.setEq", "Track Equalizer", "Sound", None, EQ_PARAMS, true, set_eq),
        cmd(
            "score.setMaster",
            "Master",
            "Sound",
            None,
            "volume? (0..=127; 100 is unity), pan? (-63..=63), focus? (-100..=100 percent: above zero the current track stands out, below it sits back)",
            true,
            set_master,
        ),
        cmd(
            "score.setMastering",
            "Sound Mastering",
            "Sound",
            None,
            "slot (\"compressor\", \"reverb\" or \"equalizer\", or 0..=2), preset? (a preset name of the type), on?, params? ({name: value}; see sound.effectTypes)",
            true,
            set_mastering,
        ),
        cmd(
            "track.sound",
            "Track Sound",
            "Sound",
            None,
            "track? (index; default the cursor's): the chain that plays (own: false when it is the instrument's), the equalizer and the master",
            false,
            track_sound,
        ),
        cmd(
            "sound.effectTypes",
            "Effect Types",
            "Sound",
            None,
            "type? (one type's id): each type's parameters, ranges, units and presets",
            false,
            effect_types,
        ),
    ]
}

const EQ_PARAMS: &str = "on?, preset? (\"Flat\", \"Electric Guitar\", \"Distorted Guitar\", \"Acoustic Guitar\", \"Bass\", \"Drums\"), \
    params? ({low, lowMid, lowMidFreq, midQ, mid, midFreq, highMid, highMidFreq, high}; gains in dB -12..=12), track? (index; default the cursor's)";

fn bad(msg: String) -> EngineError {
    EngineError::BadParam(msg)
}

fn effect_type(params: &Value) -> Result<Option<EffectType>, EngineError> {
    match params.get("type") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let id = v.as_str().ok_or_else(|| bad("`type` must be an effect type such as \"reverb\"".into()))?;
            let kind = EffectType::from_id(id).filter(|k| *k != EffectType::MixerEq).ok_or_else(|| {
                let ids: Vec<&str> = EffectType::CHAIN.iter().map(|k| k.id()).collect();
                bad(format!("`type` {id:?} must be one of {}", ids.join(", ")))
            })?;
            Ok(Some(kind))
        }
    }
}

/// Applies `preset`, `params` and `on` from a command's parameters to `e`.
fn apply(e: &mut Effect, params: &Value) -> Result<(), EngineError> {
    if let Some(name) = params.get("preset").filter(|v| !v.is_null()) {
        let name = name.as_str().ok_or_else(|| bad("`preset` must be a preset's name".into()))?;
        let on = e.on;
        *e = Effect::preset(e.kind, name).ok_or_else(|| {
            let names: Vec<&str> = e.kind.presets().iter().map(|(n, _)| *n).collect();
            bad(format!("{} has no preset {name:?}; it has {}", e.kind.label(), names.join(", ")))
        })?;
        e.on = on;
    }
    match params.get("params") {
        None | Some(Value::Null) => {}
        Some(Value::Object(map)) => {
            for (name, v) in map {
                let v = v.as_f64().filter(|v| v.is_finite()).ok_or_else(|| bad(format!("`params.{name}` must be a number")))?;
                e.set(name, v as f32).map_err(bad)?;
            }
        }
        Some(_) => return Err(bad("`params` must be an object of {name: value}".into())),
    }
    if params.get("on").is_some_and(|v| !v.is_null()) {
        e.on = p::boolean(params, "on")?;
    }
    Ok(())
}

fn set_effect(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let slot = p::uint(params, "slot", MAX_TRACK_EFFECTS as u64)? as usize;
    let kind = effect_type(params)?;
    let t = mixer_track(s, params)?;
    let mut chain = t.chain();
    if slot > chain.len() {
        return Err(bad(format!("`slot` {slot}: the chain has {} effects", chain.len())));
    }
    let mut e = match (chain.get(slot), kind) {
        (Some(old), None) => old.clone(),
        (Some(old), Some(k)) if old.kind == k => old.clone(),
        (_, Some(k)) => Effect::new(k),
        (None, None) => return Err(bad("`type` is needed to add an effect".into())),
    };
    if slot >= MAX_TRACK_EFFECTS {
        return Err(bad(format!("a chain holds at most {MAX_TRACK_EFFECTS} effects")));
    }
    apply(&mut e, params)?;
    let out = serde_json::to_value(&e).map_err(|err| bad(err.to_string()))?;
    match chain.get_mut(slot) {
        Some(old) => *old = e,
        None => chain.push(e),
    }
    t.effects = Some(chain);
    Ok(json!({"slot": slot, "effect": out}))
}

fn remove_effect(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let slot = p::uint(params, "slot", 1_000)? as usize;
    let t = mixer_track(s, params)?;
    let mut chain = t.chain();
    if slot >= chain.len() {
        return Err(bad(format!("`slot` {slot}: the chain has {} effects", chain.len())));
    }
    chain.remove(slot);
    t.effects = Some(chain);
    Ok(json!({"ok": true}))
}

fn move_effect(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let slot = p::uint(params, "slot", 1_000)? as usize;
    let to = p::uint(params, "to", 1_000)? as usize;
    let t = mixer_track(s, params)?;
    let mut chain = t.chain();
    if slot >= chain.len() || to >= chain.len() {
        return Err(bad(format!("`slot` and `to` must be below {}, the chain's length", chain.len())));
    }
    let e = chain.remove(slot);
    chain.insert(to, e);
    t.effects = Some(chain);
    Ok(json!({"ok": true}))
}

fn reset_effects(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    mixer_track(s, params)?.effects = None;
    Ok(json!({"ok": true}))
}

fn set_eq(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let t = mixer_track(s, params)?;
    let mut eq = t.eq.clone();
    apply(&mut eq, params)?;
    t.eq = eq;
    serde_json::to_value(&t.eq).map_err(|err| bad(err.to_string()))
}

fn set_master(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let volume = p::opt_uint(params, "volume", 127)?;
    let pan = params.get("pan").filter(|v| !v.is_null()).map(|_| p::int(params, "pan", -63, 63)).transpose()?;
    let focus = params.get("focus").filter(|v| !v.is_null()).map(|_| p::int(params, "focus", -100, 100)).transpose()?;
    let master = &mut Arc::make_mut(&mut s.score).master;
    if let Some(v) = volume {
        master.volume = v as u8;
    }
    if let Some(v) = pan {
        master.pan = v as i8;
    }
    if let Some(v) = focus {
        master.focus = v as i8;
    }
    Ok(json!({"volume": master.volume, "pan": master.pan, "focus": master.focus}))
}

fn set_mastering(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let names: Vec<&str> = MASTER_SLOTS.iter().map(|(n, _)| *n).collect();
    let index = match params.get("slot") {
        Some(Value::String(name)) => MASTER_SLOTS.iter().position(|(n, _)| n.eq_ignore_ascii_case(name)),
        Some(v) => v.as_u64().and_then(|i| usize::try_from(i).ok()).filter(|i| *i < MASTER_SLOTS.len()),
        None => None,
    }
    .ok_or_else(|| bad(format!("`slot` must be {} or 0..=2", names.join(", "))))?;
    let master = &mut Arc::make_mut(&mut s.score).master;
    let slot = master.slot_mut(index).ok_or_else(|| bad(format!("`slot` {index}: no such slot")))?;
    let mut e = slot.clone();
    apply(&mut e, params)?;
    let out = serde_json::to_value(&e).map_err(|err| bad(err.to_string()))?;
    *slot = e;
    Ok(out)
}

fn track_sound(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let ti = p::opt_uint(params, "track", 10_000)?.map_or(s.cursor.track, |t| t as usize);
    let score = s.score();
    let t = score.tracks.get(ti).ok_or_else(|| bad(format!("`track` {ti}: no such track")))?;
    Ok(json!({
        "track": ti,
        "own": t.effects.is_some(),
        "effects": serde_json::to_value(t.chain()).unwrap_or(Value::Null),
        "eq": serde_json::to_value(&t.eq).unwrap_or(Value::Null),
        "master": serde_json::to_value(&score.master).unwrap_or(Value::Null),
    }))
}

fn unit_name(u: Unit) -> &'static str {
    match u {
        Unit::Knob => "knob",
        Unit::Decibels => "dB",
        Unit::Hertz => "Hz",
        Unit::Milliseconds => "ms",
        Unit::Percent => "%",
        Unit::Ratio => "ratio",
        Unit::Q => "Q",
        Unit::Choice => "choice",
    }
}

fn describe(kind: EffectType) -> Value {
    let params: Vec<Value> = kind
        .params()
        .iter()
        .map(|spec| {
            let mut m = Map::new();
            m.insert("name".into(), json!(spec.name));
            m.insert("label".into(), json!(spec.label));
            m.insert("min".into(), json!(spec.min));
            m.insert("max".into(), json!(spec.max));
            m.insert("default".into(), json!(spec.default));
            m.insert("unit".into(), json!(unit_name(spec.unit)));
            if !spec.choices.is_empty() {
                m.insert("choices".into(), json!(spec.choices));
            }
            Value::Object(m)
        })
        .collect();
    let presets: Vec<&str> = kind.presets().iter().map(|(n, _)| *n).collect();
    json!({"type": kind.id(), "label": kind.label(), "params": params, "presets": presets})
}

fn effect_types(_s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    match p::opt_string(params, "type") {
        Some(id) => EffectType::from_id(id).map(describe).ok_or_else(|| bad(format!("`type` {id:?}: no such effect type"))),
        None => Ok(Value::Array(EffectType::CHAIN.into_iter().chain([EffectType::MixerEq]).map(describe).collect())),
    }
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::{Value, json};

    fn guitar(program: u8) -> Session {
        let mut s = Session::new();
        s.run("track.setProgram", &json!({"program": program})).unwrap();
        s
    }

    #[test]
    fn a_chain_is_built_edited_reordered_and_reset() {
        let mut s = guitar(30);
        let sound = s.run("track.sound", &Value::Null).unwrap();
        assert_eq!((sound["own"].as_bool(), sound["effects"].as_array().map(Vec::len)), (Some(false), Some(3)));
        // Editing the instrument's chain copies it first.
        s.run("track.setEffect", &json!({"slot": 1, "params": {"gain": 9}})).unwrap();
        let t = &s.score().tracks[0];
        let chain = t.effects.as_ref().unwrap();
        assert_eq!((chain.len(), chain[1].value("gain"), chain[1].value("model")), (3, 9.0, 2.0));
        s.run("track.setEffect", &json!({"slot": 3, "type": "reverb", "preset": "Hall"})).unwrap();
        s.run("track.setEffect", &json!({"slot": 3, "on": false})).unwrap();
        let chain = s.score().tracks[0].effects.clone().unwrap();
        assert_eq!((chain[3].preset_name(), chain[3].on), (Some("Hall"), false));
        // A new type in a slot replaces the effect.
        s.run("track.setEffect", &json!({"slot": 0, "type": "compressor"})).unwrap();
        assert_eq!(s.score().tracks[0].effects.as_ref().unwrap()[0].kind, guitario_score::EffectType::Compressor);
        s.run("track.moveEffect", &json!({"slot": 3, "to": 0})).unwrap();
        assert_eq!(s.score().tracks[0].effects.as_ref().unwrap()[0].kind, guitario_score::EffectType::Reverb);
        s.run("track.removeEffect", &json!({"slot": 0})).unwrap();
        assert_eq!(s.score().tracks[0].effects.as_ref().unwrap().len(), 3);
        s.run("edit.undo", &Value::Null).unwrap();
        assert_eq!(s.score().tracks[0].effects.as_ref().unwrap().len(), 4, "one undo step each");
        s.run("track.resetEffects", &Value::Null).unwrap();
        assert!(s.score().tracks[0].effects.is_none());
        assert!(s.score().validate().is_ok());
    }

    #[test]
    fn bad_effects_are_refused_and_change_nothing() {
        let mut s = guitar(25);
        let before = s.score().clone();
        for bad in [
            json!({"slot": 0}),
            json!({"slot": 1, "type": "reverb"}),
            json!({"slot": 0, "type": "mixerEq"}),
            json!({"slot": 0, "type": "flanger"}),
            json!({"slot": 0, "type": "delay", "params": {"time": 99999}}),
            json!({"slot": 0, "type": "delay", "params": {"nope": 1}}),
            json!({"slot": 0, "type": "delay", "params": [1]}),
            json!({"slot": 0, "type": "delay", "preset": "Nope"}),
            json!({"slot": 0, "type": "delay", "track": 9}),
            json!({"slot": 6, "type": "delay"}),
        ] {
            assert!(s.run("track.setEffect", &bad).is_err(), "{bad}");
        }
        for _ in 0..6 {
            s.run("track.setEffect", &json!({"slot": s.score().tracks[0].effects.as_ref().map_or(0, Vec::len), "type": "tremolo"})).unwrap();
        }
        assert!(s.run("track.setEffect", &json!({"slot": 6, "type": "delay"})).is_err(), "six at most");
        assert!(s.run("track.removeEffect", &json!({"slot": 6})).is_err());
        assert!(s.run("track.moveEffect", &json!({"slot": 0, "to": 6})).is_err());
        assert_ne!(s.score(), &before);
        assert!(s.score().validate().is_ok());
    }

    #[test]
    fn the_equalizer_master_and_mastering_are_set_and_saved() {
        let mut s = guitar(25);
        s.run("track.setEq", &json!({"preset": "Distorted Guitar"})).unwrap();
        s.run("track.setEq", &json!({"params": {"mid": 6, "midQ": 2.5}})).unwrap();
        let eq = &s.score().tracks[0].eq;
        assert_eq!((eq.value("mid"), eq.value("midQ"), eq.value("low"), eq.on), (6.0, 2.5, -4.0, true));
        assert!(s.run("track.setEq", &json!({"params": {"mid": 13}})).is_err());
        s.run("score.setMaster", &json!({"volume": 80, "pan": -10})).unwrap();
        assert!(s.run("score.setMaster", &json!({"volume": 128})).is_err());
        assert_eq!(s.run("score.setMaster", &json!({"focus": -40})).unwrap()["focus"], -40);
        assert!(s.run("score.setMaster", &json!({"focus": 101})).is_err());
        assert_eq!((s.score().master.volume, s.score().master.focus), (80, -40), "focus leaves the volume");
        s.run("score.setMastering", &json!({"slot": "reverb", "on": true, "preset": "Hall"})).unwrap();
        s.run("score.setMastering", &json!({"slot": 2, "params": {"b1k": -3}})).unwrap();
        assert!(s.run("score.setMastering", &json!({"slot": "chorus"})).is_err());
        let m = &s.score().master;
        assert_eq!((m.volume, m.pan, m.reverb.on, m.reverb.preset_name(), m.equalizer.value("b1k")), (80, -10, true, Some("Hall"), -3.0));
        // Saved and read back.
        let text = serde_json::to_string(s.score()).unwrap();
        let back: guitario_score::Score = serde_json::from_str(&text).unwrap();
        assert_eq!(&back, &**s.score());
        back.validate().unwrap();
    }

    #[test]
    fn effect_types_describe_every_parameter() {
        let mut s = Session::new();
        let all = s.run("sound.effectTypes", &Value::Null).unwrap();
        assert_eq!(all.as_array().map(Vec::len), Some(12));
        let amp = s.run("sound.effectTypes", &json!({"type": "amp"})).unwrap();
        assert_eq!(amp["params"][0]["choices"][2], "Lead");
        assert!(amp["presets"].as_array().is_some_and(|p| p.iter().any(|n| n == "Metal Rhythm")));
        assert!(s.run("sound.effectTypes", &json!({"type": "x"})).is_err());
    }
}
