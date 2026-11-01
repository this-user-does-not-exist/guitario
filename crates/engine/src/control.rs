//! The control protocol: how a program outside Guitario drives a session.
//!
//! One request per line, a JSON object `{"id": 1, "method": "run", "params": {…}}`; one reply per
//! line, `{"id": 1, "result": …}` or `{"id": 1, "error": {"message": "…"}}`. The running app
//! serves it on a loopback socket (`guitario --control <port>`), `guitario-cli mcp` turns MCP
//! tool calls into it, and a headless [`Session`] answers it directly through [`Host`]. Nothing
//! here does I/O, so it builds for wasm. The methods are documented in `docs/control.md`.

use crate::{EngineError, Session, registry};
use serde_json::{Map, Value, json};

/// Bumped when a method changes in a way an old client would misread.
pub const PROTOCOL: u32 = 1;
/// The longest request line a server reads; anything longer is refused and the connection closed.
pub const MAX_REQUEST_BYTES: usize = 8 << 20;
/// The most steps one `batch` runs.
pub const MAX_BATCH: usize = 10_000;

/// Something that answers control methods: a headless [`Session`], the running app, or a client
/// connected to the app.
pub trait Host {
    fn call(&mut self, method: &str, params: &Value) -> Result<Value, EngineError>;
}

impl Host for Session {
    fn call(&mut self, method: &str, params: &Value) -> Result<Value, EngineError> {
        self.control(method, params)
    }
}

/// One parsed request line.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// Echoed in the reply; any JSON value (null when the client sent none).
    pub id: Value,
    pub method: String,
    pub params: Value,
}

/// Reads one request line. A line that is not a JSON object with a string `method` is refused
/// with the reply to send; servers then close the connection, so a stray HTTP request (a web page
/// posting to the port) never reaches a body line.
pub fn parse_request(line: &str) -> Result<Request, Value> {
    if line.len() > MAX_REQUEST_BYTES {
        return Err(error_reply(&Value::Null, &format!("a request may be at most {MAX_REQUEST_BYTES} bytes")));
    }
    let v: Value = serde_json::from_str(line.trim()).map_err(|e| error_reply(&Value::Null, &format!("not a JSON request: {e}")))?;
    let Value::Object(mut o) = v else {
        return Err(error_reply(&Value::Null, "a request is a JSON object {\"id\", \"method\", \"params\"}"));
    };
    let id = o.remove("id").unwrap_or(Value::Null);
    let Some(Value::String(method)) = o.remove("method") else {
        return Err(error_reply(&id, "a request needs a string \"method\""));
    };
    Ok(Request { id, method, params: o.remove("params").unwrap_or(Value::Null) })
}

/// The reply line for a request.
pub fn reply(id: &Value, result: Result<Value, EngineError>) -> Value {
    match result {
        Ok(v) => json!({"id": id, "result": v}),
        Err(e) => error_reply(id, &e.to_string()),
    }
}

pub fn error_reply(id: &Value, message: &str) -> Value {
    json!({"id": id, "error": {"message": message}})
}

/// The error for a method nobody answers.
pub fn unknown_method(method: &str) -> EngineError {
    EngineError::invalid(format!("unknown method `{}`; try `hello`", method.chars().take(80).collect::<String>()))
}

impl Session {
    /// Answers the methods a session can answer on its own: `hello`, `commands`, `run`, `batch`
    /// and `state`. The app adds `view` and `screenshot`.
    pub fn control(&mut self, method: &str, params: &Value) -> Result<Value, EngineError> {
        match method {
            "hello" => Ok(hello(&[])),
            "commands" => Ok(commands(params)),
            "run" => {
                let (id, p) = command_of(params)?;
                self.run(id, &p)
            }
            "batch" => self.batch(params),
            "state" => self.state(),
            _ => Err(unknown_method(method)),
        }
    }

    /// The score summary, cursor, selection and history, as `score.info` plus the selection.
    pub fn state(&mut self) -> Result<Value, EngineError> {
        let mut v = self.run("score.info", &Value::Null)?;
        if let Some(o) = v.as_object_mut() {
            let sel = self.selection().map(|s| {
                let ((m0, b0), (m1, b1)) = s.ordered();
                json!({"track": s.track, "fromMeasure": m0, "fromBeat": b0, "toMeasure": m1, "toBeat": b1})
            });
            o.insert("selection".into(), sel.unwrap_or(Value::Null));
        }
        Ok(v)
    }

    /// Runs `[["id", {params}], …]` all or nothing: if a step fails, the session is put back as
    /// it was before the first and the error names the step. Each step that changed the score
    /// stays its own undo step.
    fn batch(&mut self, params: &Value) -> Result<Value, EngineError> {
        let steps = params
            .get("steps")
            .unwrap_or(params)
            .as_array()
            .ok_or_else(|| EngineError::BadParam("`steps` is an array of [\"command.id\", {params}]".into()))?;
        if steps.len() > MAX_BATCH {
            return Err(EngineError::BadParam(format!("a batch runs at most {MAX_BATCH} steps")));
        }
        let saved = (self.snapshot(), self.undo.clone(), self.redo.clone(), self.path.clone(), self.dirty, self.selection);
        let mut results = Vec::with_capacity(steps.len());
        for (i, step) in steps.iter().enumerate() {
            let outcome = match step_of(step) {
                Ok((id, p)) => self.run(id, &p).map_err(|e| format!("step {i} ({id}): {e}")),
                Err(e) => Err(format!("step {i}: {e}")),
            };
            match outcome {
                Ok(v) => results.push(v),
                Err(m) => {
                    let (snap, undo, redo, path, dirty, selection) = saved;
                    (self.undo, self.redo, self.path, self.dirty, self.selection) = (undo, redo, path, dirty, selection);
                    self.restore(snap);
                    return Err(EngineError::Invalid(format!("{m}; nothing in the batch was applied")));
                }
            }
        }
        Ok(json!({"results": results}))
    }
}

/// `{"command": "id", "params": {…}}` → the id and params.
fn command_of(params: &Value) -> Result<(&str, Value), EngineError> {
    let id = params.get("command").and_then(Value::as_str).ok_or_else(|| EngineError::BadParam("`command` is a command id such as \"note.setFret\"".into()))?;
    Ok((id, params.get("params").cloned().unwrap_or(Value::Null)))
}

/// One batch step: `["id", {params}]` or `{"command": "id", "params": {…}}`.
fn step_of(step: &Value) -> Result<(&str, Value), EngineError> {
    if let Some(a) = step.as_array() {
        let id = a.first().and_then(Value::as_str).ok_or_else(|| EngineError::BadParam("a step is [\"command.id\", {params}]".into()))?;
        return Ok((id, a.get(1).cloned().unwrap_or(Value::Null)));
    }
    command_of(step)
}

/// The `hello` reply: who answers and which methods beyond the session's it has.
pub fn hello(extra: &[&str]) -> Value {
    let mut methods = vec!["hello", "commands", "run", "batch", "state"];
    methods.extend_from_slice(extra);
    json!({"name": "Guitario", "version": env!("CARGO_PKG_VERSION"), "protocol": PROTOCOL, "methods": methods})
}

/// Every command, or those whose id starts with `prefix` or whose label or menu contains `search`.
pub fn commands(params: &Value) -> Value {
    let prefix = params.get("prefix").and_then(Value::as_str).unwrap_or("");
    let search = params.get("search").and_then(Value::as_str).map(str::to_lowercase).unwrap_or_default();
    let list: Vec<Value> = registry()
        .iter()
        .filter(|c| c.id.starts_with(prefix))
        .filter(|c| {
            search.is_empty() || c.id.to_lowercase().contains(&search) || c.label.to_lowercase().contains(&search) || c.menu.to_lowercase().contains(&search)
        })
        .map(|c| {
            let mut o = Map::new();
            o.insert("id".into(), json!(c.id));
            o.insert("label".into(), json!(c.label));
            o.insert("params".into(), json!(c.params));
            o.insert("mutates".into(), json!(c.mutates));
            if !c.menu.is_empty() {
                o.insert("menu".into(), json!(c.menu));
            }
            if let Some(k) = c.shortcut {
                o.insert("shortcut".into(), json!(k));
            }
            Value::Object(o)
        })
        .collect();
    Value::Array(list)
}

/// Standard base64 with padding, for PNGs in replies.
pub fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let sym = |n: u32| TABLE.get((n & 63) as usize).map_or('A', |&c| c as char);
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = |i: usize| chunk.get(i).map_or(0, |&x| u32::from(x));
        let n = (b(0) << 16) | (b(1) << 8) | b(2);
        out.push(sym(n >> 18));
        out.push(sym(n >> 12));
        out.push(if chunk.len() > 1 { sym(n >> 6) } else { '=' });
        out.push(if chunk.len() > 2 { sym(n) } else { '=' });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(s: &mut Session, line: &str) -> Value {
        match parse_request(line) {
            Ok(r) => reply(&r.id, s.call(&r.method, &r.params)),
            Err(e) => e,
        }
    }

    #[test]
    fn runs_a_command_by_id_and_reports_state() {
        let mut s = Session::new();
        let r = call(&mut s, r#"{"id":7,"method":"run","params":{"command":"note.setFret","params":{"fret":5}}}"#);
        assert_eq!(r["id"], 7);
        assert!(r.get("error").is_none(), "{r}");
        let st = call(&mut s, r#"{"id":"x","method":"state"}"#);
        assert_eq!(st["id"], "x");
        assert_eq!(st["result"]["canUndo"], true);
        assert_eq!(st["result"]["selection"], Value::Null);
    }

    #[test]
    fn hostile_lines_get_an_error_not_a_panic() {
        let mut s = Session::new();
        for line in [
            "",
            "POST / HTTP/1.1",
            "[1,2,3]",
            "null",
            "{\"method\": 5}",
            "{\"id\": {}, \"method\": \"\"}",
            "{\"method\": \"run\"}",
            "{\"method\": \"run\", \"params\": {\"command\": 3}}",
            "{\"method\": \"run\", \"params\": {\"command\": \"nope.nope\"}}",
            "{\"method\": \"batch\", \"params\": {\"steps\": 4}}",
            "{\"method\": \"batch\", \"params\": {\"steps\": [[], [7], {\"x\": 1}]}}",
            "{\"method\": \"screenshot\"}",
            &format!("{{\"method\": \"{}\"}}", "x".repeat(100_000)),
            &"[".repeat(10_000),
        ] {
            let r = call(&mut s, line);
            assert!(r["error"]["message"].is_string(), "{line:.60} → {r}");
            assert!(r["error"]["message"].as_str().is_some_and(|m| m.len() < 400), "messages stay short");
        }
        let long = "x".repeat(MAX_REQUEST_BYTES + 1);
        assert!(parse_request(&long).is_err());
    }

    #[test]
    fn a_failing_batch_changes_nothing() {
        let mut s = Session::new();
        let before = s.score().clone();
        let r = s.call("batch", &json!({"steps": [["note.setFret", {"fret": 3}], ["cursor.right"], ["note.setFret", {"fret": "x"}]]}));
        let msg = r.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(msg.starts_with("step 2 (note.setFret)"), "{msg}");
        assert!(msg.contains("nothing in the batch was applied"), "{msg}");
        assert_eq!(**s.score(), *before);
        assert!(!s.can_undo() && !s.is_dirty());
        assert_eq!(s.cursor().beat, 0);
        let ok = s.call("batch", &json!([["note.setFret", {"fret": 3}], {"command": "cursor.right"}])).unwrap();
        assert_eq!(ok["results"].as_array().map(Vec::len), Some(2));
        assert!(s.can_undo());
    }

    #[test]
    fn commands_filter_by_prefix_and_search() {
        let all = commands(&Value::Null);
        assert_eq!(all.as_array().map(Vec::len), Some(registry().len()));
        let notes = commands(&json!({"prefix": "note."}));
        assert!(notes.as_array().is_some_and(|a| !a.is_empty() && a.iter().all(|c| c["id"].as_str().is_some_and(|i| i.starts_with("note.")))));
        let bends = commands(&json!({"search": "BEND"}));
        assert!(bends.as_array().is_some_and(|a| a.iter().any(|c| c["id"] == "note.setBend")));
    }

    #[test]
    fn base64_matches_the_rfc_vectors() {
        for (input, out) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64(input.as_bytes()), out);
        }
    }
}
