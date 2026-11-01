//! The agent task suite (beta gate B8): ten scripted edit tasks an agent runs over MCP, through
//! the real `guitario-cli mcp` binary on stdin and stdout, each checked from what the engine
//! reports back or from the file it saved.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// An MCP client talking to a `guitario-cli mcp` process.
struct Agent {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Agent {
    /// Starts the server on `file` (a new score when None) and runs the MCP handshake.
    fn start(file: Option<&str>) -> Agent {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_guitario-cli"));
        cmd.arg("mcp").args(file).env("GUITARIO_CONFIG_DIR", "").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut a = Agent { child, stdin, stdout, next: 1 };
        let init = a.rpc("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "agent-tasks", "version": "1"}}));
        assert_eq!(init["result"]["serverInfo"]["name"], "guitario", "{init}");
        writeln!(a.stdin, "{}", json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).unwrap();
        a
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        writeln!(self.stdin, "{}", json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).unwrap();
        self.stdin.flush().unwrap();
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad reply {line:?}: {e}"));
        assert_eq!(reply["id"], id);
        reply
    }

    /// Calls a tool; returns its JSON text, panicking with the message if the tool failed.
    fn tool(&mut self, name: &str, args: Value) -> Value {
        let r = self.rpc("tools/call", json!({"name": name, "arguments": args}));
        let text = r["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
        assert_eq!(r["result"]["isError"], false, "{name} {args}: {text}");
        serde_json::from_str(&text).unwrap_or(Value::Null)
    }

    /// Calls a tool that should fail; returns its message.
    fn tool_fails(&mut self, name: &str, args: Value) -> String {
        let r = self.rpc("tools/call", json!({"name": name, "arguments": args}));
        assert_eq!(r["result"]["isError"], true, "{name} {args} should fail: {r}");
        r["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string()
    }

    fn run(&mut self, command: &str, params: Value) -> Value {
        self.tool("run", json!({"command": command, "params": params}))
    }

    fn batch(&mut self, steps: Value) -> Value {
        self.tool("batch", json!({"steps": steps}))
    }

    fn state(&mut self) -> Value {
        self.tool("state", json!({}))
    }

    /// Every note that sounds: (track, measure, tick, pitch).
    fn notes(&mut self) -> Vec<(u64, u64, u64, i64)> {
        let t = self.run("score.timeline", Value::Null);
        t.as_array()
            .into_iter()
            .flatten()
            .map(|n| (n["track"].as_u64().unwrap_or(0), n["measure"].as_u64().unwrap_or(0), n["tick"].as_u64().unwrap_or(0), n["pitch"].as_i64().unwrap_or(-1)))
            .collect()
    }
}

impl Drop for Agent {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn out(name: &str) -> String {
    format!("{}/{name}", env!("CARGO_TARGET_TMPDIR"))
}

/// Four eighths on the low E string, frets 0 3 5 0, entered one command at a time.
fn riff(a: &mut Agent) {
    a.run("beat.setDuration", json!({"value": 8}));
    for fret in [0, 3, 5, 0] {
        a.run("cursor.set", json!({"string": 5}));
        a.run("note.setFret", json!({"fret": fret}));
        a.run("cursor.right", Value::Null);
    }
}

fn pitches(a: &mut Agent) -> Vec<i64> {
    a.notes().iter().map(|n| n.3).collect()
}

#[test]
fn task_01_enter_a_riff() {
    let mut a = Agent::start(None);
    let tools = a.rpc("tools/list", json!({}));
    assert_eq!(tools["result"]["tools"].as_array().map(Vec::len), Some(4));
    let found = a.tool("commands", json!({"search": "fret"}));
    assert!(found.as_array().is_some_and(|c| c.iter().any(|c| c["id"] == "note.setFret")));
    riff(&mut a);
    assert_eq!(pitches(&mut a), [40, 43, 45, 40]);
    let ticks: Vec<u64> = a.notes().iter().map(|n| n.2).collect();
    assert!(ticks.windows(2).all(|w| w[1] > w[0]), "eighths one after another: {ticks:?}");
}

#[test]
fn task_02_title_artist_and_tempo() {
    let mut a = Agent::start(None);
    a.batch(json!([["score.setMeta", {"title": "Night Ride", "artist": "The Agents"}], ["score.setTempo", {"bpm": 168}]]));
    let s = a.state();
    assert_eq!((s["title"].as_str(), s["artist"].as_str(), s["tempo"].as_u64()), (Some("Night Ride"), Some("The Agents"), Some(168)));
    assert_eq!(s["dirty"], true);
}

#[test]
fn task_03_add_a_bass_track() {
    let mut a = Agent::start(None);
    a.run("track.add", json!({"name": "Bass", "tuning": "bass"}));
    let s = a.state();
    let tracks = s["tracks"].as_array().cloned().unwrap_or_default();
    assert_eq!(tracks.len(), 2);
    assert_eq!((tracks[1]["name"].as_str(), tracks[1]["strings"].as_u64()), (Some("Bass"), Some(4)));
    a.batch(json!([["cursor.set", {"track": 1, "measure": 0, "beat": 0, "string": 3}], ["note.setFret", {"fret": 0}]]));
    assert_eq!(a.notes(), [(1, 0, 0, 28)], "the bass's low E");
}

#[test]
fn task_04_power_chords() {
    let mut a = Agent::start(None);
    a.batch(json!([
        ["beat.setDuration", {"value": 4}],
        ["cursor.set", {"string": 5}], ["note.setFret", {"fret": 3}],
        ["cursor.set", {"string": 4}], ["note.setFret", {"fret": 5}],
        ["cursor.set", {"string": 3}], ["note.setFret", {"fret": 5}],
    ]));
    let notes = a.notes();
    let mut chord: Vec<i64> = notes.iter().map(|n| n.3).collect();
    chord.sort_unstable();
    assert_eq!(chord, [43, 50, 55], "G5 with the octave");
    assert!(notes.iter().all(|n| n.2 == 0), "one chord on one beat");
}

#[test]
fn task_05_bend_and_vibrato_saved() {
    let mut a = Agent::start(None);
    a.batch(json!([
        ["cursor.set", {"string": 2}], ["note.setFret", {"fret": 7}],
        ["note.setBend", {"quarterTones": 4}], ["note.toggleEffect", {"effect": "vibrato"}],
    ]));
    let path = out("task05.gt");
    a.run("file.save", json!({"path": path}));
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let note = &saved["score"]["tracks"][0]["measures"][0]["voices"][0]["beats"][0]["notes"][0];
    assert_eq!(note["fret"], 7, "{note}");
    assert_eq!(note["effects"]["bend"], 4, "a full step: {note}");
    assert_eq!(note["effects"]["vibrato"], true);
}

#[test]
fn task_06_repeat_a_bar() {
    let mut a = Agent::start(None);
    riff(&mut a);
    a.batch(json!([["cursor.set", {"measure": 0, "beat": 0}], ["measure.setRepeat", {"open": true, "close": 1}]]));
    assert_eq!(pitches(&mut a), [40, 43, 45, 40, 40, 43, 45, 40], "the bar plays twice");
}

#[test]
fn task_07_copy_a_bar_into_the_next() {
    let mut a = Agent::start(None);
    riff(&mut a);
    a.batch(json!([
        ["cursor.set", {"measure": 0, "beat": 0}], ["select.set", {"fromMeasure": 0, "fromBeat": 0, "toMeasure": 0, "toBeat": 3}], ["edit.copy"],
        // A new bar goes in before the cursor's: the riff moves to bar 2 and the copy fills bar 1.
        ["measure.insert"], ["cursor.set", {"measure": 0, "beat": 0}], ["edit.paste"],
    ]));
    let by_bar = |m: u64, notes: &[(u64, u64, u64, i64)]| notes.iter().filter(|n| n.1 == m).map(|n| n.3).collect::<Vec<_>>();
    let notes = a.notes();
    assert_eq!(by_bar(0, &notes), [40, 43, 45, 40], "{notes:?}");
    assert_eq!(by_bar(1, &notes), by_bar(0, &notes), "{notes:?}");
}

#[test]
fn task_08_transpose_a_selection() {
    let mut a = Agent::start(None);
    riff(&mut a);
    a.batch(json!([["select.set", {"fromMeasure": 0, "fromBeat": 0, "toMeasure": 0, "toBeat": 3}], ["edit.transpose", {"semitones": 2}]]));
    assert_eq!(pitches(&mut a), [42, 45, 47, 42]);
    assert!(a.state()["selection"].is_object());
}

#[test]
fn task_09_a_mistake_is_undone_and_a_bad_batch_changes_nothing() {
    let mut a = Agent::start(None);
    riff(&mut a);
    a.batch(json!([["cursor.set", {"measure": 0, "beat": 1, "string": 5}], ["note.setFret", {"fret": 12}]]));
    assert_eq!(pitches(&mut a), [40, 52, 45, 40]);
    a.run("edit.undo", Value::Null);
    assert_eq!(pitches(&mut a), [40, 43, 45, 40]);
    let before = a.state();
    let msg = a.tool_fails("batch", json!({"steps": [["note.setFret", {"fret": 9}], ["note.setFret", {"fret": 99}]]}));
    assert!(msg.contains("step 1") && msg.contains("nothing in the batch was applied"), "{msg}");
    assert_eq!(a.state(), before);
    assert!(a.tool_fails("run", json!({"command": "no.such"})).contains("unknown command"));
}

#[test]
fn task_10_open_a_guitar_pro_file_and_export() {
    let gp5 = concat!(env!("CARGO_MANIFEST_DIR"), "/../../demos/gp5/03-hall-of-the-mountain-king.gp5");
    let mut a = Agent::start(Some(gp5));
    let s = a.state();
    assert!(s["measures"].as_u64().unwrap_or(0) > 4 && !s["tracks"].as_array().is_some_and(Vec::is_empty), "{s}");
    let (mid, gp, gt) = (out("task10.mid"), out("task10.gp"), out("task10.gt"));
    a.batch(json!([["file.exportMidi", {"path": mid}], ["file.exportGp", {"path": gp}], ["score.setMeta", {"title": "Hall, reworked"}], ["file.save", {"path": gt}]]));
    assert!(std::fs::read(&mid).unwrap().starts_with(b"MThd"));
    assert!(std::fs::read(&gp).unwrap().starts_with(b"PK"), "a .gp is a zip");
    drop(a);
    let mut again = Agent::start(Some(&gt));
    let r = again.state();
    assert_eq!((r["title"].as_str(), r["measures"].as_u64(), r["dirty"].as_bool()), (Some("Hall, reworked"), s["measures"].as_u64(), Some(false)));
}
