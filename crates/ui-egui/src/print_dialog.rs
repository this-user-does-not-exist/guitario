//! File › Print…: choose a printer and options, then send the pages to the spooler.
//!
//! Looking up printers and sending a job can both block (a network queue, the Windows print
//! pipeline), so each runs on a worker thread and the dialog polls for the answer. The job itself
//! is the engine's `file.print`, prepared on the UI thread and run on the worker, so what the
//! dialog does is what an agent gets from the command.

use crate::GuitarioApp;
use guitario_engine::{DuplexMode, PrintReceipt, PrinterInfo, Spooler};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

pub(crate) struct PrintDialog {
    pub open: bool,
    /// `None` while the list is being fetched.
    printers: Option<Vec<PrinterInfo>>,
    lookup: Option<Receiver<Vec<PrinterInfo>>>,
    /// `None` is the system's default printer.
    printer: Option<String>,
    copies: u32,
    collate: bool,
    duplex: DuplexMode,
    grayscale: bool,
    all_tracks: bool,
    notation: bool,
    sending: Option<Receiver<Result<PrintReceipt, String>>>,
    error: Option<String>,
}

impl Default for PrintDialog {
    fn default() -> Self {
        PrintDialog {
            open: false,
            printers: None,
            lookup: None,
            printer: None,
            copies: 1,
            collate: true,
            duplex: DuplexMode::Default,
            grayscale: false,
            all_tracks: false,
            notation: true,
            sending: None,
            error: None,
        }
    }
}

impl PrintDialog {
    /// The `file.print` request for what is chosen. `track` is the one on screen.
    pub(crate) fn params(&self, track: usize) -> Value {
        let tracks = if self.all_tracks { Value::Null } else { json!([track]) };
        let duplex = match self.duplex {
            DuplexMode::Default => "default",
            DuplexMode::Off => "off",
            DuplexMode::LongEdge => "long",
            DuplexMode::ShortEdge => "short",
        };
        json!({
            "printer": self.printer,
            "copies": self.copies,
            "collate": self.collate,
            "duplex": duplex,
            "grayscale": self.grayscale,
            "tracks": tracks,
            "notation": self.notation,
        })
    }

    fn start(&mut self, notation: bool, spooler: Option<Arc<dyn Spooler>>) {
        *self = PrintDialog { open: true, notation, ..PrintDialog::default() };
        let Some(spooler) = spooler else {
            self.printers = Some(Vec::new());
            return;
        };
        let (tx, rx) = channel();
        match std::thread::Builder::new().name("printers".into()).spawn(move || {
            let _ = tx.send(spooler.printers());
        }) {
            Ok(_) => self.lookup = Some(rx),
            Err(_) => self.printers = Some(Vec::new()),
        }
    }

    fn duplex_label(d: DuplexMode) -> &'static str {
        match d {
            DuplexMode::Default => "Printer setting",
            DuplexMode::Off => "One-sided",
            DuplexMode::LongEdge => "Two-sided (long edge)",
            DuplexMode::ShortEdge => "Two-sided (short edge)",
        }
    }
}

impl GuitarioApp {
    /// File › Print…: opens the dialog and starts looking for printers.
    pub fn open_print(&mut self) {
        let notation = self.view.notation;
        self.open_print_with(false, notation);
    }

    /// The dialog with the track choice and notation setting already made (the preview's).
    pub(crate) fn open_print_with(&mut self, all_tracks: bool, notation: bool) {
        let spooler = self.session.spooler().cloned();
        self.print.start(notation, spooler);
        self.print.all_tracks = all_tracks && self.session.score().tracks.len() > 1;
    }

    /// Hands the chosen job to a worker thread.
    fn send_print(&mut self) {
        let track = self.session.cursor().track;
        let task = match self.session.prepare_print(&self.print.params(track)) {
            Ok(task) => task,
            Err(e) => {
                self.print.error = Some(e.to_string());
                return;
            }
        };
        let (tx, rx) = channel();
        match std::thread::Builder::new().name("print".into()).spawn(move || {
            let _ = tx.send(task.run());
        }) {
            Ok(_) => {
                self.print.error = None;
                self.print.sending = Some(rx);
            }
            Err(e) => self.print.error = Some(format!("could not start printing: {e}")),
        }
    }

    /// Collects the answers of the two worker threads.
    fn poll_print(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.print.lookup {
            match rx.try_recv() {
                Ok(list) => {
                    self.print.printers = Some(list);
                    self.print.lookup = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
                Err(TryRecvError::Disconnected) => {
                    self.print.printers = Some(Vec::new());
                    self.print.lookup = None;
                }
            }
        }
        let Some(rx) = &self.print.sending else { return };
        match rx.try_recv() {
            Ok(Ok(receipt)) => {
                let noun = if receipt.pages == 1 { "page" } else { "pages" };
                self.status = format!("Sent {} {noun} to the printer. {}", receipt.pages, receipt.message);
                if let Some(w) = receipt.warnings.first() {
                    self.status = format!("{} (warning: {w})", self.status);
                }
                self.status_is_error = false;
                self.print.sending = None;
                self.print.open = false;
            }
            Ok(Err(e)) => {
                self.print.error = Some(e);
                self.print.sending = None;
            }
            Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
            Err(TryRecvError::Disconnected) => {
                self.print.error = Some("Printing stopped unexpectedly.".into());
                self.print.sending = None;
            }
        }
    }

    pub(crate) fn print_dialog(&mut self, ctx: &egui::Context) {
        if !self.print.open {
            return;
        }
        self.poll_print(ctx);
        let tracks = self.session.score().tracks.len();
        let current = self.session.score().tracks.get(self.session.cursor().track).map(|t| t.name.clone()).unwrap_or_default();
        let busy = self.print.sending.is_some();
        let page = self.session.score().page;
        let paper = format!("{} {}", page.paper.name(), if page.landscape { "landscape" } else { "portrait" });
        let mut print = false;
        let mut cancel = false;
        let d = &mut self.print;
        egui::Modal::new(egui::Id::new("print-dialog")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Print");
            ui.add_space(6.0);
            egui::Grid::new("print-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Printer");
                let shown = match (&d.printer, &d.printers) {
                    (Some(name), _) => name.clone(),
                    (None, None) => "Looking for printers…".into(),
                    (None, Some(list)) => list.iter().find(|p| p.default).map_or_else(|| "System default".into(), |p| format!("{} (default)", p.name)),
                };
                ui.add_enabled_ui(!busy, |ui| {
                    egui::ComboBox::from_id_salt("print-printer").selected_text(shown).width(250.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut d.printer, None, "System default");
                        for p in d.printers.iter().flatten() {
                            ui.selectable_value(&mut d.printer, Some(p.name.clone()), &p.name);
                        }
                    });
                });
                ui.end_row();

                ui.label("Copies");
                ui.add_enabled_ui(!busy, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut d.copies).range(1..=guitario_engine::MAX_COPIES as u32));
                        ui.add_enabled(d.copies > 1, egui::Checkbox::new(&mut d.collate, "Collate"));
                    });
                });
                ui.end_row();

                ui.label("Sides");
                ui.add_enabled_ui(!busy, |ui| {
                    egui::ComboBox::from_id_salt("print-duplex").selected_text(PrintDialog::duplex_label(d.duplex)).width(250.0).show_ui(ui, |ui| {
                        for mode in [DuplexMode::Default, DuplexMode::Off, DuplexMode::LongEdge, DuplexMode::ShortEdge] {
                            ui.selectable_value(&mut d.duplex, mode, PrintDialog::duplex_label(mode));
                        }
                    });
                });
                ui.end_row();

                ui.label("Tracks");
                ui.add_enabled_ui(!busy, |ui| {
                    ui.vertical(|ui| {
                        ui.radio_value(&mut d.all_tracks, false, format!("This track ({current})"));
                        ui.add_enabled_ui(tracks > 1, |ui| ui.radio_value(&mut d.all_tracks, true, format!("All tracks ({tracks})")));
                    });
                });
                ui.end_row();

                ui.label("Show");
                ui.add_enabled_ui(!busy, |ui| {
                    ui.vertical(|ui| {
                        ui.checkbox(&mut d.notation, "Standard notation above the tab");
                        ui.checkbox(&mut d.grayscale, "Black and white");
                    });
                });
                ui.end_row();
            });
            ui.add_space(6.0);
            ui.label(egui::RichText::new(format!("Pages are {paper} (File › Page Setup); other paper in the printer shrinks them to fit.")).small());
            if let Some(e) = &d.error {
                ui.add_space(4.0);
                ui.colored_label(egui::Color32::from_rgb(0xff, 0x7a, 0x6a), e);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let nothing_to_print = d.printers.as_ref().is_some_and(Vec::is_empty);
                if busy {
                    ui.spinner();
                    ui.label("Sending to the printer…");
                } else if ui.add_enabled(!nothing_to_print, egui::Button::new("Print")).clicked() {
                    print = true;
                }
                if ui.add_enabled(!busy, egui::Button::new("Cancel")).clicked() || (!busy && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
                    cancel = true;
                }
                if nothing_to_print && !busy {
                    ui.label("No printers found. Export a PDF instead.");
                }
            });
        });
        if cancel {
            self.print.open = false;
        } else if print {
            self.send_print();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::{PageRenderer, PdfDocument, PdfRequest, PrintJob, Session};
    use std::sync::Mutex;

    struct Pages;
    impl PageRenderer for Pages {
        fn render_pdf(&self, _: &guitario_score::Score, r: &PdfRequest) -> Result<PdfDocument, String> {
            Ok(PdfDocument { bytes: b"%PDF".to_vec(), pages: r.tracks.len(), warnings: vec![] })
        }
    }

    #[derive(Default)]
    struct Queue(Mutex<Vec<PrintJob>>, bool);
    impl Spooler for Queue {
        fn printers(&self) -> Vec<PrinterInfo> {
            vec![PrinterInfo { name: "Office".into(), default: true }]
        }
        fn submit(&self, _: &[u8], job: &PrintJob) -> Result<String, String> {
            if self.1 {
                return Err("the queue is paused".into());
            }
            if let Ok(mut jobs) = self.0.lock() {
                jobs.push(job.clone());
            }
            Ok("queued".into())
        }
    }

    /// Runs frames until `done` or about two seconds have passed.
    fn pump(app: &mut GuitarioApp, done: impl Fn(&GuitarioApp) -> bool) {
        let ctx = egui::Context::default();
        for _ in 0..200 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.print_dialog(ui.ctx())).textures_delta.clear();
            if done(app) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn app_with(queue: &Arc<Queue>) -> GuitarioApp {
        let mut s = Session::new();
        s.run("track.add", &json!({"name": "Bass", "tuning": "bass"})).unwrap();
        let mut app = GuitarioApp::new(s, None);
        app.session.set_renderer(Arc::new(Pages));
        app.session.set_spooler(queue.clone());
        app
    }

    #[test]
    fn the_request_follows_the_choices() {
        let mut d =
            PrintDialog { printer: Some("Office".into()), copies: 3, collate: false, duplex: DuplexMode::ShortEdge, grayscale: true, ..PrintDialog::default() };
        assert_eq!(
            d.params(1),
            json!({"printer": "Office", "copies": 3, "collate": false, "duplex": "short", "grayscale": true, "tracks": [1], "notation": true})
        );
        d.all_tracks = true;
        d.printer = None;
        let p = d.params(1);
        assert_eq!((p["tracks"].clone(), p["printer"].clone()), (Value::Null, Value::Null));
    }

    #[test]
    fn opening_looks_up_printers_and_printing_sends_one_job_then_closes() {
        let queue = Arc::new(Queue::default());
        let mut app = app_with(&queue);
        app.open_print();
        assert!(app.print.open && app.print.printers.is_none());
        pump(&mut app, |a| a.print.printers.is_some());
        assert_eq!(app.print.printers.as_ref().map(Vec::len), Some(1));

        app.print.copies = 2;
        app.print.all_tracks = true;
        app.send_print();
        assert!(app.print.sending.is_some() && app.print.error.is_none());
        pump(&mut app, |a| !a.print.open);
        assert!(!app.print.open, "the dialog should close when the job is accepted");
        assert!(app.status.contains("Sent 2 pages"), "{}", app.status);
        let jobs = queue.0.lock().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs.first().map(|j| j.copies), Some(2));
    }

    #[test]
    fn a_refused_job_keeps_the_dialog_open_with_the_reason() {
        let queue = Arc::new(Queue(Mutex::default(), true));
        let mut app = app_with(&queue);
        app.open_print();
        app.send_print();
        pump(&mut app, |a| a.print.sending.is_none());
        assert!(app.print.open, "the dialog stays open so the options can be changed");
        assert!(app.print.error.as_deref().is_some_and(|e| e.contains("the queue is paused")), "{:?}", app.print.error);
        assert!(queue.0.lock().unwrap().is_empty());
    }
}
