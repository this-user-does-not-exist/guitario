use crate::EngineError;
use guitario_theory::{Duration, NoteValue};
use serde_json::Value;

fn bad(key: &str, want: &str) -> EngineError {
    EngineError::BadParam(format!("`{key}` must be {want}"))
}

pub fn uint(p: &Value, key: &str, max: u64) -> Result<u64, EngineError> {
    let v = p.get(key).and_then(Value::as_u64).ok_or_else(|| bad(key, "a non-negative integer"))?;
    if v > max { Err(bad(key, &format!("at most {max}"))) } else { Ok(v) }
}

pub fn opt_uint(p: &Value, key: &str, max: u64) -> Result<Option<u64>, EngineError> {
    if p.get(key).is_none_or(Value::is_null) { Ok(None) } else { uint(p, key, max).map(Some) }
}

pub fn int(p: &Value, key: &str, min: i64, max: i64) -> Result<i64, EngineError> {
    let v = p.get(key).and_then(Value::as_i64).ok_or_else(|| bad(key, "an integer"))?;
    if (min..=max).contains(&v) { Ok(v) } else { Err(bad(key, &format!("between {min} and {max}"))) }
}

pub fn string<'a>(p: &'a Value, key: &str) -> Result<&'a str, EngineError> {
    p.get(key).and_then(Value::as_str).ok_or_else(|| bad(key, "a string"))
}

pub fn opt_string<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str)
}

pub fn boolean(p: &Value, key: &str) -> Result<bool, EngineError> {
    p.get(key).and_then(Value::as_bool).ok_or_else(|| bad(key, "true or false"))
}

/// `value` is a divisor (`4`) or a name (`"quarter"`); `dots` defaults to 0; `tuplet` is `[3, 2]`.
pub fn duration(p: &Value) -> Result<Duration, EngineError> {
    let v = p.get("value").ok_or_else(|| bad("value", "a note value"))?;
    let value = match v {
        Value::Number(n) => n.as_u64().and_then(|d| u32::try_from(d).ok()).and_then(NoteValue::from_divisor),
        other => serde_json::from_value::<NoteValue>(other.clone()).ok(),
    }
    .ok_or_else(|| bad("value", "1, 2, 4, 8, 16, 32, 64 or a name such as \"quarter\""))?;
    let dots = opt_uint(p, "dots", 2)?.unwrap_or(0) as u8;
    let tuplet = match p.get("tuplet") {
        None | Some(Value::Null) => (1, 1),
        Some(t) => {
            let a = t.get(0).and_then(Value::as_u64).filter(|n| (1..=16).contains(n));
            let b = t.get(1).and_then(Value::as_u64).filter(|n| (1..=16).contains(n));
            match (a, b) {
                (Some(a), Some(b)) => (a as u8, b as u8),
                _ => return Err(bad("tuplet", "[enters, times] with both in 1..=16")),
            }
        }
    };
    Ok(Duration { value, dots, tuplet })
}
