//! Sending pages to a printer. The engine draws nothing and talks to no spooler itself: the shell
//! registers a [`PageRenderer`](crate::PageRenderer) for the pages and a [`Spooler`] for the
//! system's print queues, and `file.print` joins the two. A [`PrintTask`] owns everything a job
//! needs, so a UI can run it on a worker thread while the editor stays responsive.

use crate::commands::{CommandSpec, cmd};
use crate::render::tracks_param;
use crate::{EngineError, PageRenderer, PdfRequest, Session, params as p};
use guitario_score::Score;
use serde_json::{Value, json};
use std::sync::Arc;

/// Most copies one job may ask for.
pub const MAX_COPIES: u64 = 999;

/// Two-sided printing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DuplexMode {
    /// Whatever the printer is set to.
    #[default]
    Default,
    Off,
    LongEdge,
    ShortEdge,
}

/// One print job, as the spooler needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintJob {
    /// `None` is the system's default printer.
    pub printer: Option<String>,
    pub copies: u32,
    pub collate: bool,
    pub duplex: DuplexMode,
    pub grayscale: bool,
    /// The document name the spooler shows.
    pub title: String,
}

/// A printer the system offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrinterInfo {
    pub name: String,
    pub default: bool,
}

/// The system's print queues. Implemented by the shell; calls may block, so run them off the UI thread.
pub trait Spooler: Send + Sync {
    fn printers(&self) -> Vec<PrinterInfo>;
    /// Sends a finished PDF to the queue. The message is the spooler's own ("request id is …").
    fn submit(&self, pdf: &[u8], job: &PrintJob) -> Result<String, String>;
}

/// What a finished job reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintReceipt {
    pub message: String,
    pub pages: usize,
    pub warnings: Vec<String>,
}

/// A job ready to run: the score as it was when the job was made, the pages to draw and where to send them.
pub struct PrintTask {
    score: Arc<Score>,
    request: PdfRequest,
    job: PrintJob,
    renderer: Arc<dyn PageRenderer>,
    spooler: Arc<dyn Spooler>,
}

impl PrintTask {
    /// Draws the pages and submits them. Blocks until the spooler has the job.
    pub fn run(self) -> Result<PrintReceipt, String> {
        let doc = self.renderer.render_pdf(&self.score, &self.request)?;
        let message = self.spooler.submit(&doc.bytes, &self.job)?;
        Ok(PrintReceipt { message, pages: doc.pages, warnings: doc.warnings })
    }

    pub fn job(&self) -> &PrintJob {
        &self.job
    }
}

impl Session {
    /// Registers the print queues `file.print` and `file.printers` use.
    pub fn set_spooler(&mut self, spooler: Arc<dyn Spooler>) {
        self.spooler = Some(spooler);
    }

    pub fn spooler(&self) -> Option<&Arc<dyn Spooler>> {
        self.spooler.as_ref()
    }

    /// Checks a `file.print` request and packs it into a job that can run anywhere.
    pub fn prepare_print(&self, params: &Value) -> Result<PrintTask, EngineError> {
        let renderer = self.renderer.clone().ok_or_else(|| EngineError::invalid("this build has no page renderer, so it cannot print"))?;
        let spooler = self.spooler.clone().ok_or_else(|| EngineError::invalid("this build cannot reach a printer; export a PDF instead"))?;
        let request = PdfRequest {
            tracks: tracks_param(&self.score, params)?,
            notation: match params.get("notation") {
                None | Some(Value::Null) => true,
                Some(v) => v.as_bool().ok_or_else(|| EngineError::BadParam("`notation` must be true or false".into()))?,
            },
        };
        let printer = match p::opt_string(params, "printer") {
            None => None,
            Some(name) if name.is_empty() || name.chars().count() > 256 || name.chars().any(char::is_control) => {
                return Err(EngineError::BadParam("`printer` must be a printer name".into()));
            }
            Some(name) => Some(name.to_string()),
        };
        if params.get("printer").is_some_and(|v| !v.is_null() && !v.is_string()) {
            return Err(EngineError::BadParam("`printer` must be a printer name".into()));
        }
        let copies = match p::opt_uint(params, "copies", MAX_COPIES)? {
            None => 1,
            Some(0) => return Err(EngineError::BadParam("`copies` must be at least 1".into())),
            Some(n) => u32::try_from(n).unwrap_or(1),
        };
        let flag = |key: &str, default: bool| match params.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(v) => v.as_bool().ok_or_else(|| EngineError::BadParam(format!("`{key}` must be true or false"))),
        };
        let duplex = match params.get("duplex") {
            None | Some(Value::Null) => DuplexMode::Default,
            Some(v) => match v.as_str() {
                Some("default") => DuplexMode::Default,
                Some("off") => DuplexMode::Off,
                Some("long") => DuplexMode::LongEdge,
                Some("short") => DuplexMode::ShortEdge,
                _ => return Err(EngineError::BadParam("`duplex` must be \"default\", \"off\", \"long\" or \"short\"".into())),
            },
        };
        let title = self.score.meta.title.trim();
        let job = PrintJob {
            printer,
            copies,
            collate: flag("collate", true)?,
            duplex,
            grayscale: flag("grayscale", false)?,
            title: if title.is_empty() { "Guitario".to_string() } else { title.chars().filter(|c| !c.is_control()).take(120).collect() },
        };
        Ok(PrintTask { score: self.score.clone(), request, job, renderer, spooler })
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd(
            "file.print",
            "Print…",
            "File",
            None,
            "printer? (name from file.printers; default the system default), copies? (1..=999), collate? (default true), \
             duplex? (\"default\", \"off\", \"long\", \"short\"), grayscale? (default false), tracks? (indexes, default every track), \
             notation? (default true)",
            false,
            print,
        ),
        cmd("file.printers", "Printers", "", None, "", false, printers),
    ]
}

fn print(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let receipt = s.prepare_print(params)?.run().map_err(EngineError::invalid)?;
    Ok(json!({"message": receipt.message, "pages": receipt.pages, "warnings": receipt.warnings}))
}

fn printers(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let spooler = s.spooler.clone().ok_or_else(|| EngineError::invalid("this build cannot reach a printer"))?;
    let list: Vec<Value> = spooler.printers().into_iter().map(|p| json!({"name": p.name, "default": p.default})).collect();
    Ok(json!({"printers": list}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PdfDocument, PdfRequest};
    use std::sync::Mutex;

    struct Pages;
    impl PageRenderer for Pages {
        fn render_pdf(&self, _: &Score, r: &PdfRequest) -> Result<PdfDocument, String> {
            Ok(PdfDocument { bytes: format!("%PDF {:?} {}", r.tracks, r.notation).into_bytes(), pages: 2, warnings: vec!["w".into()] })
        }
    }

    #[derive(Default)]
    struct Queue {
        sent: Mutex<Vec<(String, PrintJob)>>,
        fail: bool,
    }
    impl Spooler for Queue {
        fn printers(&self) -> Vec<PrinterInfo> {
            vec![PrinterInfo { name: "Office".into(), default: true }, PrinterInfo { name: "Plotter".into(), default: false }]
        }
        fn submit(&self, pdf: &[u8], job: &PrintJob) -> Result<String, String> {
            if self.fail {
                return Err("queue is down".into());
            }
            if let Ok(mut sent) = self.sent.lock() {
                sent.push((String::from_utf8_lossy(pdf).into_owned(), job.clone()));
            }
            Ok("request id is Office-7".into())
        }
    }

    fn session(queue: &Arc<Queue>) -> Session {
        let mut s = Session::new();
        s.set_renderer(Arc::new(Pages));
        s.set_spooler(queue.clone());
        s
    }

    #[test]
    fn a_job_carries_its_options_to_the_spooler() {
        let q = Arc::new(Queue::default());
        let mut s = session(&q);
        s.run("score.setMeta", &json!({"title": "Riff"})).unwrap();
        let r =
            s.run("file.print", &json!({"printer": "Plotter", "copies": 3, "collate": false, "duplex": "long", "grayscale": true, "notation": false})).unwrap();
        assert_eq!((r["pages"].clone(), r["message"].clone()), (json!(2), json!("request id is Office-7")));
        let sent = q.sent.lock().unwrap();
        let (pdf, job) = sent.first().unwrap();
        assert_eq!(pdf, "%PDF [0] false");
        assert_eq!(
            *job,
            PrintJob { printer: Some("Plotter".into()), copies: 3, collate: false, duplex: DuplexMode::LongEdge, grayscale: true, title: "Riff".into() }
        );
    }

    #[test]
    fn defaults_print_every_track_once_on_the_default_printer() {
        let q = Arc::new(Queue::default());
        let mut s = session(&q);
        s.run("track.add", &json!({"name": "Bass", "tuning": "bass"})).unwrap();
        s.run("file.print", &json!({})).unwrap();
        let sent = q.sent.lock().unwrap();
        let (pdf, job) = sent.first().unwrap();
        assert_eq!(pdf, "%PDF [0, 1] true");
        assert_eq!(
            (job.printer.clone(), job.copies, job.collate, job.duplex, job.grayscale, job.title.as_str()),
            (None, 1, true, DuplexMode::Default, false, "Guitario")
        );
    }

    #[test]
    fn printers_are_listed_with_the_default_marked() {
        let q = Arc::new(Queue::default());
        let r = session(&q).run("file.printers", &Value::Null).unwrap();
        assert_eq!(r, json!({"printers": [{"name": "Office", "default": true}, {"name": "Plotter", "default": false}]}));
    }

    #[test]
    fn hostile_requests_are_refused_and_nothing_is_sent() {
        let q = Arc::new(Queue::default());
        let mut s = session(&q);
        for bad in [
            json!({"copies": 0}),
            json!({"copies": 1000}),
            json!({"copies": -1}),
            json!({"copies": "two"}),
            json!({"printer": ""}),
            json!({"printer": 5}),
            json!({"printer": "a\nb"}),
            json!({"printer": "x".repeat(300)}),
            json!({"duplex": "sideways"}),
            json!({"duplex": 1}),
            json!({"collate": "yes"}),
            json!({"grayscale": 1}),
            json!({"tracks": [3]}),
            json!({"tracks": []}),
            json!({"notation": 0}),
        ] {
            assert!(s.run("file.print", &bad).is_err(), "{bad}");
        }
        assert!(q.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn without_a_renderer_or_a_spooler_the_commands_explain() {
        let mut s = Session::new();
        assert!(s.run("file.print", &json!({})).unwrap_err().to_string().contains("no page renderer"));
        s.set_renderer(Arc::new(Pages));
        assert!(s.run("file.print", &json!({})).unwrap_err().to_string().contains("printer"));
        assert!(s.run("file.printers", &Value::Null).is_err());
    }

    #[test]
    fn a_spooler_failure_is_an_error_with_its_message() {
        let q = Arc::new(Queue { fail: true, ..Queue::default() });
        let e = session(&q).run("file.print", &json!({})).unwrap_err().to_string();
        assert!(e.contains("queue is down"), "{e}");
    }

    #[test]
    fn a_task_runs_on_another_thread_with_the_score_as_it_was() {
        let q = Arc::new(Queue::default());
        let mut s = session(&q);
        let task = s.prepare_print(&json!({"copies": 2})).unwrap();
        s.run("track.add", &json!({"name": "Late", "tuning": "bass"})).unwrap();
        let receipt = std::thread::spawn(move || task.run()).join().unwrap().unwrap();
        assert_eq!(receipt.pages, 2);
        let sent = q.sent.lock().unwrap();
        assert_eq!(sent.first().map(|(pdf, _)| pdf.as_str()), Some("%PDF [0] true"));
    }

    #[test]
    fn a_title_is_trimmed_of_control_characters() {
        let q = Arc::new(Queue::default());
        let mut s = session(&q);
        s.run("score.setMeta", &json!({"title": "A\u{7}B\u{0}"})).unwrap();
        assert_eq!(s.prepare_print(&json!({})).unwrap().job().title, "AB");
    }
}
