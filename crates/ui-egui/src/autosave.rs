//! Autosave and crash recovery. While the score has unsaved changes, a copy is written every
//! [`INTERVAL`] to `recovery/` beside the preferences, on a worker thread with the native format's
//! atomic save. Saving or closing cleanly removes it. Each running Guitario keeps a heartbeat file
//! fresh; a copy whose heartbeat has gone stale was left by a Guitario that stopped without
//! saving, and is offered back at the next start (`file.recover` reopens it). Each open document
//! has its own copy and heartbeat ([`Autosave::sibling`]).

use guitario_engine::Session;
use guitario_score::Score;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

/// How often unsaved work is written, and the heartbeat refreshed.
pub const INTERVAL: Duration = Duration::from_secs(30);
/// A heartbeat older than this belongs to a Guitario that is no longer running.
const STALE: Duration = Duration::from_secs(150);
/// Recovery files looked at, at most, so a folder full of junk cannot stall the start.
const MAX_FOUND: usize = 32;

/// Unsaved work left behind by a Guitario that stopped.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// The autosaved score.
    pub file: PathBuf,
    /// The file it was being edited from, if it had one.
    pub original: Option<PathBuf>,
    /// When it was last autosaved.
    pub saved: Option<SystemTime>,
}

impl Found {
    /// The name shown for it: its file's name, or "Untitled".
    pub fn name(&self) -> String {
        self.original.as_deref().and_then(Path::file_name).map_or_else(|| "Untitled".into(), |n| n.to_string_lossy().into_owned())
    }

    /// Removes the autosaved copy and its notes.
    pub fn discard(&self) {
        for ext in ["gt", "path", "alive"] {
            let _ = std::fs::remove_file(self.file.with_extension(ext));
        }
    }
}

pub struct Autosave {
    /// The recovery folder; `None` turns autosave off (tests, agents, preferences in memory).
    dir: Option<PathBuf>,
    /// This run's file stem.
    id: String,
    /// The score last written, to skip writing it again.
    written: Option<Arc<Score>>,
    last_write: Option<Instant>,
    last_beat: Option<Instant>,
    worker: Option<JoinHandle<Result<(), String>>>,
    /// Why the last write failed, once, for the status bar.
    pub error: Option<String>,
    /// What earlier runs left behind, found at start.
    pub found: Vec<Found>,
}

impl Autosave {
    pub fn off() -> Autosave {
        Autosave { dir: None, id: String::new(), written: None, last_write: None, last_beat: None, worker: None, error: None, found: Vec::new() }
    }

    /// Autosave into `dir`, and look there for what stopped runs left.
    pub fn at(dir: PathBuf) -> Autosave {
        let found = orphans(&dir, SystemTime::now());
        Autosave { dir: Some(dir), id: fresh_id(), found, ..Autosave::off() }
    }

    /// Autosave for another document in the same folder, under its own name; it offers nothing
    /// back (the first one did).
    pub fn sibling(&self) -> Autosave {
        Autosave { dir: self.dir.clone(), id: fresh_id(), ..Autosave::off() }
    }

    fn file(&self, ext: &str) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(&self.id).with_extension(ext))
    }

    /// Call every frame: keeps the heartbeat fresh, writes unsaved work every [`INTERVAL`], and
    /// removes the copy once the score is saved. Returns how soon it wants to run again.
    pub fn tick(&mut self, now: Instant, session: &Session) -> Option<Duration> {
        let dir = self.dir.clone()?;
        if let Some(w) = self.worker.take_if(|w| w.is_finished())
            && let Ok(Err(e)) | Err(e) = w.join().map_err(|_| "the autosave thread stopped".to_string())
        {
            self.error = Some(format!("Autosave failed: {e}"));
        }
        let due = |last: Option<Instant>| last.is_none_or(|t| now.saturating_duration_since(t) >= INTERVAL);
        if due(self.last_beat) {
            if std::fs::create_dir_all(&dir).is_ok()
                && let Some(alive) = self.file("alive")
            {
                let _ = std::fs::write(alive, b"");
            }
            self.last_beat = Some(now);
        }
        if !session.is_dirty() {
            if self.written.take().is_some() {
                self.remove_copy();
            }
            return Some(INTERVAL);
        }
        let changed = self.written.as_ref().is_none_or(|w| !Arc::ptr_eq(w, session.score()));
        if changed && due(self.last_write) && self.worker.is_none() {
            let (Some(file), Some(note)) = (self.file("gt"), self.file("path")) else { return None };
            let score = Arc::clone(session.score());
            let original = session.path().map(|p| p.display().to_string()).unwrap_or_default();
            self.written = Some(Arc::clone(&score));
            self.last_write = Some(now);
            let job = move || -> Result<(), String> {
                std::fs::create_dir_all(file.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
                guitario_format::save(&score, &file).map_err(|e| e.to_string())?;
                std::fs::write(note, original).map_err(|e| e.to_string())
            };
            match std::thread::Builder::new().name("autosave".into()).spawn(job) {
                Ok(h) => self.worker = Some(h),
                Err(e) => self.error = Some(format!("Autosave failed: {e}")),
            }
        }
        Some(if changed { INTERVAL.saturating_sub(self.last_write.map_or(Duration::ZERO, |t| now.saturating_duration_since(t))) } else { INTERVAL })
    }

    /// Waits for a write in progress.
    pub fn wait(&mut self) {
        if let Some(w) = self.worker.take()
            && let Ok(Err(e)) = w.join()
        {
            self.error = Some(format!("Autosave failed: {e}"));
        }
    }

    fn remove_copy(&mut self) {
        self.wait();
        for ext in ["gt", "path"] {
            if let Some(f) = self.file(ext) {
                let _ = std::fs::remove_file(f);
            }
        }
    }

    /// A clean exit: nothing is left to recover.
    pub fn finish(&mut self) {
        self.remove_copy();
        if let Some(f) = self.file("alive") {
            let _ = std::fs::remove_file(f);
        }
        self.written = None;
    }

    /// The autosaved copy of this run, if one is written.
    #[cfg(test)]
    pub fn copy(&self) -> Option<PathBuf> {
        self.file("gt").filter(|f| f.exists())
    }
}

impl crate::GuitarioApp {
    /// Runs autosave for this frame and wakes the app when the next write is due.
    pub(crate) fn autosave_tick(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let shown = self.autosave.tick(now, &self.session);
        if let Some(after) = shown.into_iter().chain(self.autosave_others(now)).min() {
            ctx.request_repaint_after(after);
        }
        if let Some(e) = self.autosave.error.take() {
            self.status = e;
            self.status_is_error = true;
        }
    }

    /// Offers back what a Guitario that stopped without saving left behind, one score at a time.
    pub(crate) fn recovery_dialog(&mut self, ctx: &egui::Context) {
        let Some(found) = self.autosave.found.first().cloned() else { return };
        let more = self.autosave.found.len() - 1;
        let ago = found.saved.and_then(|t| t.elapsed().ok()).map_or_else(String::new, |d| match d.as_secs() / 60 {
            0 => " less than a minute ago".to_string(),
            1 => " a minute ago".to_string(),
            m if m < 120 => format!(" {m} minutes ago"),
            m if m < 48 * 60 => format!(" {} hours ago", m / 60),
            m => format!(" {} days ago", m / (24 * 60)),
        });
        let mut answer = None;
        egui::Modal::new(egui::Id::new("recover-unsaved")).show(ctx, |ui| {
            ui.set_width(420.0);
            ui.heading("Recover unsaved work?");
            ui.label(format!("Guitario stopped before \u{201c}{}\u{201d} was saved. A copy was autosaved{ago}.", found.name()));
            if more > 0 {
                ui.label(egui::RichText::new(format!("{more} more after this one.")).weak());
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Recover").clicked() {
                    answer = Some(Recovery::Recover);
                }
                if ui.button("Discard").clicked() {
                    answer = Some(Recovery::Discard);
                }
                if ui.button("Later").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    answer = Some(Recovery::Later);
                }
            });
        });
        if let Some(answer) = answer {
            self.answer_recovery(answer);
        }
    }

    /// What the recovery prompt was answered.
    pub(crate) fn answer_recovery(&mut self, answer: Recovery) {
        if self.autosave.found.is_empty() {
            return;
        }
        let found = self.autosave.found.remove(0);
        match answer {
            Recovery::Recover => {
                let original = found.original.as_ref().map(|p| p.display().to_string());
                let params = serde_json::json!({"path": found.file.display().to_string(), "original": original});
                // In a tab of its own, unless the one shown is an untouched untitled document.
                if self.open_in_tab(|app| app.run("file.recover", params).is_some()) {
                    // This run autosaves it from now on.
                    found.discard();
                    self.status = format!("Recovered \u{201c}{}\u{201d}. Save it to keep it.", found.name());
                }
            }
            Recovery::Discard => found.discard(),
            // Kept for the next start.
            Recovery::Later => {}
        }
    }
}

/// The answers to the recovery prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Recovery {
    Recover,
    Discard,
    Later,
}

/// A file stem no other document of any run uses: the time, the process and a counter.
fn fresh_id() -> String {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("unsaved-{stamp}-{}-{n}", std::process::id())
}

/// Autosaved scores in `dir` whose run is no longer alive, newest first.
fn orphans(dir: &Path, now: SystemTime) -> Vec<Found> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut found: Vec<Found> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "gt") && p.file_stem().is_some_and(|s| s.to_string_lossy().starts_with("unsaved-")))
        .take(MAX_FOUND)
        .filter(|p| modified(&p.with_extension("alive")).is_none_or(|t| now.duration_since(t).unwrap_or(Duration::ZERO) > STALE))
        .map(|file| {
            let original = std::fs::read_to_string(file.with_extension("path")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).map(PathBuf::from);
            Found { saved: modified(&file), original, file }
        })
        .collect();
    found.sort_by_key(|f| std::cmp::Reverse(f.saved));
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gt-autosave-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn unsaved_work_is_written_every_interval_and_removed_once_saved() {
        let dir = temp("cycle");
        let mut a = Autosave::at(dir.clone());
        let mut s = Session::new();
        let t0 = Instant::now();
        a.tick(t0, &s);
        assert!(a.copy().is_none(), "nothing unsaved, nothing written");
        s.run("note.setFret", &json!({"fret": 5})).unwrap();
        a.tick(t0, &s);
        a.wait();
        let copy = a.copy().unwrap();
        assert_eq!(guitario_format::open(&copy).unwrap(), **s.score());
        // A later edit waits for the interval, then is written.
        s.run("note.setFret", &json!({"fret": 7})).unwrap();
        a.tick(t0 + Duration::from_secs(1), &s);
        a.wait();
        assert_ne!(guitario_format::open(&copy).unwrap(), **s.score());
        a.tick(t0 + INTERVAL, &s);
        a.wait();
        assert_eq!(guitario_format::open(&copy).unwrap(), **s.score());
        // Saved: the copy goes, the heartbeat stays while running; a clean exit removes it all.
        s.run("file.save", &json!({"path": dir.join("riff.gt").display().to_string()})).unwrap();
        a.tick(t0 + INTERVAL * 2, &s);
        assert!(a.copy().is_none() && dir.join(&a.id).with_extension("alive").exists());
        a.finish();
        assert!(!dir.join(&a.id).with_extension("alive").exists());
        assert!(a.error.is_none(), "{:?}", a.error);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_stopped_run_leaves_work_to_recover_and_a_live_one_does_not() {
        let dir = temp("orphans");
        std::fs::create_dir_all(&dir).unwrap();
        let mut crashed = Autosave::at(dir.clone());
        let mut s = Session::new();
        s.run("note.setFret", &json!({"fret": 3})).unwrap();
        s.run("file.save", &json!({"path": dir.join("song.gt").display().to_string()})).unwrap();
        s.run("note.setFret", &json!({"fret": 4})).unwrap();
        crashed.tick(Instant::now(), &s);
        crashed.wait();
        // While it runs, another Guitario finds nothing.
        assert!(Autosave::at(dir.clone()).found.is_empty());
        // It stops without cleaning up: its heartbeat goes stale.
        let alive = dir.join(&crashed.id).with_extension("alive");
        let old = SystemTime::now() - Duration::from_secs(600);
        std::fs::File::options().write(true).open(&alive).unwrap().set_modified(old).unwrap();
        let next = Autosave::at(dir.clone());
        assert_eq!(next.found.len(), 1);
        let found = next.found[0].clone();
        assert_eq!((found.name(), found.original.clone()), ("song.gt".to_string(), Some(dir.join("song.gt"))));
        // Recovering brings back the unsaved edit, to be saved where it came from.
        let mut r = Session::new();
        r.run("file.recover", &json!({"path": found.file.display().to_string(), "original": found.original.as_ref().map(|p| p.display().to_string())}))
            .unwrap();
        assert_eq!(**r.score(), **s.score());
        assert!(r.is_dirty() && r.path() == Some(dir.join("song.gt").as_path()));
        found.discard();
        assert!(Autosave::at(dir.clone()).found.is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_app_offers_recovery_and_each_answer_does_what_it_says() {
        let dir = temp("app");
        std::fs::create_dir_all(&dir).unwrap();
        // Three scores left by stopped runs (no heartbeat at all).
        let mut s = Session::new();
        s.run("note.setFret", &json!({"fret": 12})).unwrap();
        for i in 1..=3 {
            guitario_format::save(s.score(), &dir.join(format!("unsaved-{i}.gt"))).unwrap();
        }
        std::fs::write(dir.join("unsaved-1.path"), dir.join("solo.gt").display().to_string()).unwrap();
        let mut app = crate::GuitarioApp::new(Session::new(), None);
        app.autosave = Autosave::at(dir.clone());
        assert_eq!(app.autosave.found.len(), 3);
        assert!(app.modal_open(), "the prompt holds the keys");
        // Later keeps the file for the next start; Discard removes it.
        app.answer_recovery(Recovery::Later);
        app.answer_recovery(Recovery::Discard);
        assert_eq!(Autosave::at(dir.clone()).found.len(), 2);
        // Recover opens it unsaved; its file is then this run's to autosave.
        let file = app.autosave.found[0].file.clone();
        app.answer_recovery(Recovery::Recover);
        assert_eq!(**app.session.score(), **s.score());
        assert!(app.session.is_dirty() && !file.exists());
        assert!(app.autosave.found.is_empty() && !app.modal_open());
        app.answer_recovery(Recovery::Recover);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn junk_in_the_folder_and_an_unwritable_folder_are_not_a_crash() {
        let dir = temp("junk");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("unsaved-1.gt"), b"not a score").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        let a = Autosave::at(dir.clone());
        assert_eq!(a.found.len(), 1, "offered; opening it says what is wrong");
        let mut r = Session::new();
        assert!(r.run("file.recover", &json!({"path": a.found[0].file.display().to_string()})).is_err());
        // A folder that is a file: writes fail and say so, nothing panics.
        let blocked = dir.join("notes.txt").join("recovery");
        let mut b = Autosave::at(blocked);
        let mut s = Session::new();
        s.run("note.setFret", &json!({"fret": 1})).unwrap();
        b.tick(Instant::now(), &s);
        b.wait();
        assert!(b.error.is_some());
        // Off does nothing at all.
        assert!(Autosave::off().tick(Instant::now(), &s).is_none());
        std::fs::remove_dir_all(dir).ok();
    }
}
