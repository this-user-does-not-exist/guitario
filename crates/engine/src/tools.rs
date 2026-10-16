//! Tools menu commands that need no audio: bar-duration checks.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd("tools.checkBarDurations", "Check Bar Durations", "Tools", None, "", false, check)]
}

/// Lists every bar whose voice does not fill its time signature (free-time bars excepted).
fn check(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let mut out = Vec::new();
    for (ti, t) in s.score.tracks.iter().enumerate() {
        for (mi, (m, h)) in t.measures.iter().zip(&s.score.headers).enumerate() {
            if h.free_time {
                continue;
            }
            for (vi, v) in m.voices.iter().enumerate() {
                let (have, want) = (v.ticks(), h.capacity());
                if have != want && !(vi > 0 && v.beats.is_empty()) {
                    out.push(json!({"track": ti, "measure": mi, "voice": vi, "ticks": have, "expected": want}));
                }
                if out.len() >= 10_000 {
                    return Ok(json!({"bars": out, "truncated": true}));
                }
            }
        }
    }
    Ok(json!({"bars": out}))
}
