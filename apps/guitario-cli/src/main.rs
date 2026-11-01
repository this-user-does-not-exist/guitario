//! Headless Guitario: inspect, convert and script scores through the same command registry the
//! app uses.

use guitario_engine::control::Host;
use guitario_engine::mcp::McpServer;
use guitario_engine::{EngineError, Session, registry};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
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
  mcp [file] [--connect <port>]  an MCP server on stdin and stdout: drives a headless session on [file],
                                 or with --connect the running app (`guitario --control <port>`),
                                 which adds the view and screenshot tools. See docs/control.md
  send <port> <method> [params]  one control request to the running app; prints the reply
";

/// The running app, reached over its control channel.
struct App {
    out: TcpStream,
    replies: std::io::Lines<BufReader<TcpStream>>,
    next: u64,
}

impl App {
    fn connect(port: &str) -> Result<App, String> {
        let port: u16 = port.parse().map_err(|_| format!("`{port}` is not a port number"))?;
        let s = TcpStream::connect(("127.0.0.1", port))
            .map_err(|e| format!("no Guitario control channel on port {port} ({e}); start the app with --control {port}"))?;
        let out = s.try_clone().map_err(|e| e.to_string())?;
        Ok(App { out, replies: BufReader::new(s).lines(), next: 1 })
    }
}

impl Host for App {
    fn call(&mut self, method: &str, params: &Value) -> Result<Value, EngineError> {
        let id = self.next;
        self.next += 1;
        writeln!(self.out, "{}", json!({"id": id, "method": method, "params": params}))?;
        self.out.flush()?;
        let line = self.replies.next().ok_or_else(|| EngineError::invalid("the app closed the control channel"))??;
        let mut reply: Value = serde_json::from_str(&line).map_err(|e| EngineError::invalid(format!("the app sent a bad reply: {e}")))?;
        if let Some(m) = reply.pointer("/error/message").and_then(Value::as_str) {
            return Err(EngineError::invalid(m));
        }
        Ok(reply.get_mut("result").map(Value::take).unwrap_or(Value::Null))
    }
}

/// Serves MCP over stdin and stdout, one JSON-RPC message per line, until stdin closes.
fn serve_mcp(host: &mut dyn Host, app: bool) -> Result<(), String> {
    let mut server = McpServer::new(app);
    let mut stdout = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = server.handle_line(host, &line) {
            writeln!(stdout, "{reply}").and_then(|()| stdout.flush()).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

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
        "mcp" => {
            if let Some(pos) = args.iter().position(|x| x == "--connect") {
                let mut app = App::connect(a(pos + 1)?)?;
                app.call("hello", &Value::Null).map_err(e)?;
                serve_mcp(&mut app, true)?;
            } else {
                if let Some(file) = args.get(2).filter(|f| !f.starts_with("--")) {
                    open(&mut s, file).map_err(e)?;
                }
                serve_mcp(&mut s, false)?;
            }
        }
        "send" => {
            let mut app = App::connect(a(2)?)?;
            let params = match args.get(4) {
                Some(p) => serde_json::from_str(p).map_err(|err| format!("params must be JSON: {err}"))?,
                None => Value::Null,
            };
            let r = app.call(a(3)?, &params).map_err(e)?;
            println!("{}", serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?);
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
