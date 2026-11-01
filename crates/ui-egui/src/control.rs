//! The control channel: the running app serves the control protocol
//! (`guitario_engine::control`) on a loopback socket, so an agent or a script can run commands,
//! change the view and take screenshots. Off unless the app is started with `--control <port>`
//! (or `GUITARIO_CONTROL`). A thread per connection reads request lines and hands them to the UI
//! thread, which answers them between frames; a screenshot is answered on the frame after it was
//! asked for, when the window's pixels arrive.

use guitario_engine::control::{self, MAX_REQUEST_BYTES};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::time::Duration;

/// At most this many clients at once; more are turned away with an error.
const MAX_CLIENTS: usize = 8;
/// How long a connection waits for the UI thread to answer (a frame is ~16 ms; a screenshot
/// needs two).
const ANSWER_WAIT: Duration = Duration::from_secs(30);

/// One request waiting for the UI thread.
pub(crate) struct Job {
    pub method: String,
    pub params: Value,
    pub reply: mpsc::Sender<Result<Value, String>>,
}

/// A screenshot asked of the window, answered when its frame arrives.
pub(crate) struct Shot {
    pub id: u64,
    pub score_only: bool,
    pub path: Option<String>,
    pub reply: mpsc::Sender<Result<Value, String>>,
}

pub(crate) struct Server {
    pub addr: SocketAddr,
    jobs: mpsc::Receiver<Job>,
    /// The UI's context, so a connection can wake the app when it is idle.
    ctx: Arc<OnceLock<egui::Context>>,
    pub shots: Vec<Shot>,
    pub next_shot: u64,
}

impl Server {
    /// Listens on 127.0.0.1:`port` (0 picks a free port; `addr` says which).
    pub fn listen(port: u16) -> std::io::Result<Server> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        let addr = listener.local_addr()?;
        let (tx, jobs) = mpsc::channel();
        let ctx: Arc<OnceLock<egui::Context>> = Arc::new(OnceLock::new());
        let wake = Arc::clone(&ctx);
        let clients = Arc::new(AtomicUsize::new(0));
        std::thread::Builder::new().name("guitario-control".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                let (tx, wake, clients) = (tx.clone(), Arc::clone(&wake), Arc::clone(&clients));
                if clients.fetch_add(1, Ordering::SeqCst) >= MAX_CLIENTS {
                    clients.fetch_sub(1, Ordering::SeqCst);
                    let mut s = stream;
                    let _ = writeln!(s, "{}", control::error_reply(&Value::Null, "too many control connections"));
                    continue;
                }
                let leaving = Arc::clone(&clients);
                let spawned = std::thread::Builder::new().name("guitario-control-client".into()).spawn(move || {
                    serve(stream, &tx, &wake);
                    leaving.fetch_sub(1, Ordering::SeqCst);
                });
                if spawned.is_err() {
                    clients.fetch_sub(1, Ordering::SeqCst);
                }
            }
        })?;
        Ok(Server { addr, jobs, ctx, shots: Vec::new(), next_shot: 0 })
    }

    /// The requests that arrived since the last frame.
    pub fn take_jobs(&mut self, ctx: &egui::Context) -> Vec<Job> {
        let _ = self.ctx.set(ctx.clone());
        self.jobs.try_iter().collect()
    }
}

/// Answers one client until it hangs up or sends something that is not a request.
fn serve(stream: TcpStream, jobs: &mpsc::Sender<Job>, wake: &OnceLock<egui::Context>) {
    let Ok(mut out) = stream.try_clone() else { return };
    let mut reader = BufReader::new(stream);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        // One byte over the limit tells a long line from one that fits exactly.
        match reader.by_ref().take(MAX_REQUEST_BYTES as u64 + 1).read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if buf.len() > MAX_REQUEST_BYTES {
            let _ = writeln!(out, "{}", control::error_reply(&Value::Null, &format!("a request may be at most {MAX_REQUEST_BYTES} bytes")));
            return;
        }
        let Ok(line) = std::str::from_utf8(&buf) else {
            let _ = writeln!(out, "{}", control::error_reply(&Value::Null, "a request is UTF-8 JSON"));
            return;
        };
        if line.trim().is_empty() {
            continue;
        }
        let req = match control::parse_request(line) {
            Ok(r) => r,
            Err(reply) => {
                // Not the protocol (an HTTP request from a web page, say): stop reading.
                let _ = writeln!(out, "{reply}");
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        if jobs.send(Job { method: req.method, params: req.params, reply: tx }).is_err() {
            return;
        }
        if let Some(ctx) = wake.get() {
            ctx.request_repaint();
        }
        let reply = match rx.recv_timeout(ANSWER_WAIT) {
            Ok(Ok(v)) => json!({"id": req.id, "result": v}),
            Ok(Err(m)) => control::error_reply(&req.id, &m),
            Err(_) => control::error_reply(&req.id, "the app did not answer in time (is a dialog waiting?)"),
        };
        if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
            return;
        }
    }
}

/// The part of `image` inside `rect` (points), clamped to the image.
pub(crate) fn crop(image: &egui::ColorImage, rect: egui::Rect, pixels_per_point: f32) -> egui::ColorImage {
    let [w, h] = image.size;
    let px = |v: f32, max: usize| ((v * pixels_per_point).max(0.0) as usize).min(max);
    let (x0, x1) = (px(rect.min.x, w), px(rect.max.x, w));
    let (y0, y1) = (px(rect.min.y, h), px(rect.max.y, h));
    let mut pixels = Vec::with_capacity(x1.saturating_sub(x0) * y1.saturating_sub(y0));
    for y in y0..y1 {
        if let Some(row) = image.pixels.get(y * w + x0..y * w + x1) {
            pixels.extend_from_slice(row);
        }
    }
    let (cw, ch) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    if pixels.len() != cw * ch {
        return image.clone();
    }
    egui::ColorImage::new([cw, ch], pixels)
}

/// The image as PNG bytes.
pub(crate) fn png(image: &egui::ColorImage) -> Result<Vec<u8>, String> {
    let [w, h] = image.size;
    let (w32, h32) = (u32::try_from(w).map_err(|e| e.to_string())?, u32::try_from(h).map_err(|e| e.to_string())?);
    if w == 0 || h == 0 {
        return Err("the area to capture is empty".into());
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w32, h32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    let data: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
    writer.write_image_data(&data).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

impl crate::GuitarioApp {
    /// Serves the control protocol on 127.0.0.1:`port` (0 picks a free port) and returns the
    /// address it listens on.
    pub fn listen(&mut self, port: u16) -> std::io::Result<SocketAddr> {
        let server = Server::listen(port)?;
        let addr = server.addr;
        self.control = Some(server);
        Ok(addr)
    }

    /// Once a frame: answers the screenshots whose pixels arrived, then the requests waiting.
    pub(crate) fn control_tick(&mut self, ctx: &egui::Context) {
        let Some(server) = self.control.as_mut() else { return };
        let shots: Vec<(Option<u64>, Arc<egui::ColorImage>)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Screenshot { user_data, image, .. } => {
                        Some((user_data.data.as_ref().and_then(|d| d.downcast_ref::<u64>()).copied(), Arc::clone(image)))
                    }
                    _ => None,
                })
                .collect()
        });
        for (id, image) in shots {
            let Some(at) = server.shots.iter().position(|s| Some(s.id) == id) else { continue };
            let shot = server.shots.remove(at);
            let image = match (shot.score_only, self.score_rect) {
                (true, Some(r)) => crop(&image, r, ctx.pixels_per_point()),
                _ => (*image).clone(),
            };
            let _ = shot.reply.send(answer_shot(&image, shot.path.as_deref()));
        }
        let jobs = server.take_jobs(ctx);
        for job in jobs {
            if let Some(answer) = self.control_call(ctx, &job) {
                let _ = job.reply.send(answer);
            }
        }
    }

    /// One request; `None` when the answer comes on a later frame (a screenshot).
    fn control_call(&mut self, ctx: &egui::Context, job: &Job) -> Option<Result<Value, String>> {
        let p = &job.params;
        Some(match job.method.as_str() {
            "hello" => {
                let mut h = control::hello(&["view", "screenshot"]);
                if let Some(o) = h.as_object_mut() {
                    o.insert("app".into(), json!(true));
                }
                Ok(h)
            }
            "view" => self.control_view(p),
            "screenshot" => {
                let score_only = match p.get("area").and_then(Value::as_str) {
                    None | Some("window") => false,
                    Some("score") => true,
                    Some(_) => return Some(Err("`area` is \"window\" or \"score\"".into())),
                };
                let path = p.get("path").and_then(Value::as_str).map(str::to_string);
                let server = self.control.as_mut()?;
                let id = server.next_shot;
                server.next_shot = server.next_shot.wrapping_add(1);
                server.shots.push(Shot { id, score_only, path, reply: job.reply.clone() });
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(id)));
                ctx.request_repaint();
                return None;
            }
            // Opening and starting a document go through the tabs, as File › Open and New do.
            "run" if p.get("command").and_then(Value::as_str) == Some("file.open") => {
                let Some(path) = p.get("params").and_then(|q| q.get("path")).and_then(Value::as_str) else {
                    return Some(Err("bad parameter: `path` is the file to open".into()));
                };
                self.status_is_error = false;
                self.open_path(path);
                if self.status_is_error { Err(self.status.clone()) } else { self.session.state().map_err(|e| e.to_string()) }
            }
            "run" if p.get("command").and_then(Value::as_str) == Some("file.new") => {
                self.start_document("file.new", Value::Null);
                self.session.state().map_err(|e| e.to_string())
            }
            method => {
                let r = self.session.control(method, p).map_err(|e| e.to_string());
                if r.is_ok() {
                    self.follow_cursor = true;
                }
                r
            }
        })
    }

    /// The `view` method: zoom, layout, multitrack, multivoice and the Homepage; every field
    /// is checked before any is applied.
    fn control_view(&mut self, p: &Value) -> Result<Value, String> {
        let zoom = match p.get("zoom") {
            None => None,
            Some(z) => Some(z.as_f64().filter(|z| (0.25..=4.0).contains(z)).ok_or("`zoom` is a number from 0.25 to 4")? as f32),
        };
        let layout = match p.get("layout") {
            None => None,
            Some(l) => Some(match l.as_str() {
                Some("page") => crate::layout::LayoutMode::Page,
                Some("screen") => crate::layout::LayoutMode::Screen,
                Some("horizontal") => crate::layout::LayoutMode::Horizontal,
                _ => return Err("`layout` is \"page\", \"screen\" or \"horizontal\"".into()),
            }),
        };
        let flag = |k: &str| match p.get(k) {
            None => Ok(None),
            Some(v) => v.as_bool().map(Some).ok_or(format!("`{k}` is true or false")),
        };
        let (multitrack, multivoice, home) = (flag("multitrack")?, flag("multivoice")?, flag("home")?);
        if let Some(z) = zoom {
            self.view.zoom = z;
        }
        if let Some(l) = layout {
            self.view.layout = l;
        }
        if let Some(m) = multitrack {
            self.view.multitrack = m;
        }
        if let Some(m) = multivoice {
            self.view.multivoice = m;
        }
        if let Some(h) = home {
            self.home.open = h;
        }
        let layout = match self.view.layout {
            crate::layout::LayoutMode::Page => "page",
            crate::layout::LayoutMode::Screen => "screen",
            crate::layout::LayoutMode::Horizontal => "horizontal",
        };
        Ok(json!({"zoom": self.view.zoom, "layout": layout, "multitrack": self.view.multitrack, "multivoice": self.view.multivoice, "home": self.home.open}))
    }
}

/// A screenshot's reply: the PNG written to `path`, or inline as base64.
fn answer_shot(image: &egui::ColorImage, path: Option<&str>) -> Result<Value, String> {
    let bytes = png(image)?;
    let [w, h] = image.size;
    match path {
        Some(path) => {
            std::fs::write(path, &bytes).map_err(|e| format!("could not write {path}: {e}"))?;
            Ok(json!({"path": path, "width": w, "height": h}))
        }
        None => Ok(json!({"png": control::base64(&bytes), "width": w, "height": h})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_clamps_to_the_image() {
        let img = egui::ColorImage::new([4, 3], (0..12).map(|i| egui::Color32::from_gray(i as u8)).collect());
        let c = crop(&img, egui::Rect::from_min_max(egui::pos2(1.0, 1.0), egui::pos2(99.0, 99.0)), 1.0);
        assert_eq!(c.size, [3, 2]);
        assert_eq!(c.pixels.first(), Some(&egui::Color32::from_gray(5)));
        let none = crop(&img, egui::Rect::from_min_max(egui::pos2(-5.0, -5.0), egui::pos2(-1.0, f32::NAN)), 2.0);
        assert!(png(&none).is_err(), "an empty crop is an error, not a panic");
        assert!(png(&img).is_ok_and(|b| b.starts_with(b"\x89PNG")));
    }

    /// An agent connects to the running app, edits, changes the view and takes screenshots.
    #[test]
    fn an_agent_drives_the_app_end_to_end() {
        let mut app = crate::GuitarioApp::new(guitario_engine::Session::new(), None);
        let addr = app.listen(0).unwrap();
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(900.0, 600.0)).build_ui(move |ui| app.show(ui));
        harness.run_steps(2);
        // Screenshots need a GPU adapter; a CI machine without one checks everything else.
        let gpu = harness.render().is_ok();
        let shot_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/control-test-score.png");
        let client = std::thread::spawn(move || {
            let s = TcpStream::connect(addr).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
            let mut out = s.try_clone().unwrap();
            let mut lines = BufReader::new(s).lines();
            let mut ask = |id: u32, method: &str, params: Value| -> Value {
                writeln!(out, "{}", json!({"id": id, "method": method, "params": params})).unwrap();
                let reply: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
                assert_eq!(reply["id"], id);
                reply
            };
            let hello = ask(1, "hello", Value::Null);
            assert_eq!(hello["result"]["app"], true);
            assert!(hello["result"]["methods"].as_array().is_some_and(|m| m.contains(&json!("screenshot"))));
            assert!(ask(2, "run", json!({"command": "note.setFret", "params": {"fret": 5}}))["result"].is_object());
            assert_eq!(ask(3, "view", json!({"zoom": 2.0, "layout": "screen"}))["result"]["zoom"], 2.0);
            let bad = ask(4, "view", json!({"zoom": 1.0, "layout": "sideways"}));
            assert!(bad["error"]["message"].as_str().is_some_and(|m| m.contains("layout")));
            assert!(ask(5, "batch", json!({"steps": [["cursor.right"], ["no.such"]]}))["error"].is_object());
            let state = ask(6, "state", Value::Null);
            assert_eq!(state["result"]["canUndo"], true);
            assert_eq!(state["result"]["cursor"]["beat"], 0, "the failed batch moved nothing");
            assert!(ask(7, "screenshot", json!({"area": "elsewhere"}))["error"].is_object());
            if gpu {
                let shot = ask(8, "screenshot", Value::Null);
                assert!(shot["result"]["png"].as_str().is_some_and(|p| p.starts_with("iVBORw0KGgo")), "{shot:.200}");
                assert_eq!(shot["result"]["width"], 900);
                let _ = std::fs::remove_file(shot_path);
                let score = ask(9, "screenshot", json!({"area": "score", "path": shot_path}));
                let w = score["result"]["width"].as_u64().unwrap_or(0);
                assert!(w > 100 && w < 900, "the score area alone: {score}");
                assert!(std::fs::read(shot_path).is_ok_and(|b| b.starts_with(b"\x89PNG")));
            }
        });
        for _ in 0..4000 {
            if client.is_finished() {
                break;
            }
            harness.step();
            std::thread::sleep(Duration::from_millis(2));
        }
        if let Err(e) = client.join() {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn hostile_clients_are_answered_or_hung_up_on() {
        let mut server = Server::listen(0).unwrap();
        let addr = server.addr;
        let talk = |send: &[u8]| -> String {
            let mut s = TcpStream::connect(addr).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            s.write_all(send).unwrap();
            let _ = s.shutdown(std::net::Shutdown::Write);
            let mut got = String::new();
            let _ = s.read_to_string(&mut got);
            got
        };
        // An HTTP request is refused at its first line, so its JSON body never runs.
        let http = talk(b"POST / HTTP/1.1\r\nHost: x\r\n\r\n{\"id\":1,\"method\":\"run\",\"params\":{\"command\":\"file.save\"}}\n");
        assert_eq!(http.lines().count(), 1, "{http}");
        assert!(http.contains("not a JSON request"));
        assert!(talk(&[0xff, 0xfe, b'\n']).contains("UTF-8"));
        assert!(talk(b"[1]\n").contains("JSON object"));
        assert!(talk(b"\n\n").is_empty(), "blank lines are skipped");
        // Nothing reached the UI thread.
        let ctx = egui::Context::default();
        assert!(server.take_jobs(&ctx).is_empty());
    }
}
