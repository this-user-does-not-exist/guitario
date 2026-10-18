//! Headless Guitario: inspect, convert and script scores through the same command registry the
//! app uses.

use guitario_engine::{EngineError, Session, registry};
use serde_json::{Value, json};
use std::process::ExitCode;

const USAGE: &str = "guitario-cli <command>

  commands                       list every command id with its parameters (JSON)
  info <file>                    summarise a .gt or Guitar Pro (.gp, .gpx, .gp5, .gp4, .gp3) file
  convert <in> <out> [--sounds <dir>] [--tracks 0,2] [--tab-only] [--score]
                                 .gt or Guitar Pro in; .gt, .gp, .gp5, .pdf, .mid, .wav, .flac or .txt
                                 (ASCII tab) out. For .wav and .flac, --sounds renders drums, bass and guitars from the
                                 SFZ bank in <dir>. For .pdf, --tracks picks tracks by index (default
                                 every track), --tab-only leaves out the standard notation and
                                 --score stacks the tracks in every system as a full score
  run <file|-> <script.json>     run [[\"command.id\", {params}], ...] on a score; `-` starts a new one;
                                 prints each result as JSON. Add --save <path> to write the result.
";

fn open(s: &mut Session, path: &str) -> Result<(), EngineError> {
    if path == "-" {
        return Ok(());
    }
    let r = s.run("file.open", &json!({"path": path}))?;
    if let Some(w) = r.get("warnings").and_then(Value::as_array) {
        for w in w {
            eprintln!("warning: {}", w.as_str().unwrap_or_default());
        }
    }
    Ok(())
}

fn real_main(args: &[String]) -> Result<(), String> {
    let e = |e: EngineError| e.to_string();
    let a = |i: usize| args.get(i).map(String::as_str).ok_or_else(|| USAGE.to_string());
    let mut s = Session::new();
    #[cfg(feature = "pdf")]
    {
        s.set_renderer(std::sync::Arc::new(guitario_ui_egui::PdfRenderer));
        s.set_spooler(std::sync::Arc::new(guitario_ui_egui::SystemSpooler));
    }
    match a(1)? {
        "commands" => {
            let list: Vec<Value> = registry()
                .iter()
                .map(|c| json!({"id": c.id, "label": c.label, "menu": c.menu, "shortcut": c.shortcut, "params": c.params, "mutates": c.mutates}))
                .collect();
            println!("{}", serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?);
        }
        "info" => {
            open(&mut s, a(2)?).map_err(e)?;
            println!("{}", serde_json::to_string_pretty(&s.run("score.info", &Value::Null).map_err(e)?).map_err(|e| e.to_string())?);
        }
        "convert" => {
            open(&mut s, a(2)?).map_err(e)?;
            let out = a(3)?;
            let lower = out.to_lowercase();
            let id = if lower.ends_with(".mid") || lower.ends_with(".midi") {
                "file.exportMidi"
            } else if lower.ends_with(".wav") || lower.ends_with(".flac") {
                "file.exportAudio"
            } else if lower.ends_with(".txt") {
                "file.exportAscii"
            } else if lower.ends_with(".pdf") {
                "file.exportPdf"
            } else if lower.ends_with(".gp5") {
                "file.exportGp5"
            } else if lower.ends_with(".gp") {
                "file.exportGp"
            } else if lower.ends_with(".musicxml") || lower.ends_with(".xml") {
                "file.exportMusicXml"
            } else if lower.ends_with(".gt") {
                "file.save"
            } else {
                return Err("output must end in .gt, .gp, .gp5, .musicxml, .pdf, .mid, .wav, .flac or .txt".into());
            };
            let mut params = serde_json::Map::new();
            params.insert("path".into(), json!(out));
            if let Some(pos) = args.iter().position(|x| x == "--sounds") {
                params.insert("sounds".into(), json!(a(pos + 1)?));
            }
            if id == "file.exportPdf" {
                if let Some(pos) = args.iter().position(|x| x == "--tracks") {
                    let list: Result<Vec<u64>, _> = a(pos + 1)?.split(',').map(|t| t.trim().parse::<u64>()).collect();
                    params.insert("tracks".into(), json!(list.map_err(|_| "--tracks takes track indexes such as 0,2".to_string())?));
                }
                if args.iter().any(|x| x == "--tab-only") {
                    params.insert("notation".into(), json!(false));
                }
                if args.iter().any(|x| x == "--score") {
                    params.insert("together".into(), json!(true));
                }
            }
            let params = Value::Object(params);
            let r = s.run(id, &params).map_err(e)?;
            if let Some(w) = r.get("warnings").and_then(Value::as_array) {
                for w in w {
                    eprintln!("warning: {}", w.as_str().unwrap_or_default());
                }
            }
        }
        "run" => {
            open(&mut s, a(2)?).map_err(e)?;
            let script: Value = serde_json::from_str(&std::fs::read_to_string(a(3)?).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            for (i, step) in script.as_array().ok_or("script must be a JSON array")?.iter().enumerate() {
                let id = step.get(0).and_then(Value::as_str).ok_or_else(|| format!("step {i}: expected [\"id\", {{params}}]"))?;
                let params = step.get(1).cloned().unwrap_or(Value::Null);
                let r = s.run(id, &params).map_err(|err| format!("step {i} ({id}): {err}"))?;
                println!("{r}");
            }
            if let Some(pos) = args.iter().position(|x| x == "--save") {
                s.run("file.save", &json!({"path": a(pos + 1)?})).map_err(e)?;
            }
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match real_main(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}
