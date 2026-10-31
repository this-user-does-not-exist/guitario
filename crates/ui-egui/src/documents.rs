//! Several documents open at once, one tab each, as in Guitar Pro 8. The tab shown is the app's
//! own `session` (so everything else reads the active document as it always has); the others
//! wait in [`Doc`]s with their own undo history, cursor, autosave copy and hidden tracks. The
//! clipboard follows the active tab, so a copy in one document pastes into another.

use crate::GuitarioApp;
use crate::autosave::Autosave;
use guitario_engine::Session;

/// A document in a tab that is not the one shown.
pub(crate) struct Doc {
    pub session: Session,
    pub autosave: Autosave,
    /// The tracks it hides from the multitrack view.
    pub hidden: u64,
}

impl GuitarioApp {
    /// How many documents are open.
    pub(crate) fn tab_count(&self) -> usize {
        self.docs.len() + 1
    }

    /// The tab shown.
    pub(crate) fn active_tab(&self) -> usize {
        self.active
    }

    /// The session behind tab `i`.
    pub(crate) fn tab_session(&self, i: usize) -> Option<&Session> {
        match i.cmp(&self.active) {
            std::cmp::Ordering::Less => self.docs.get(i).map(|d| &d.session),
            std::cmp::Ordering::Equal => Some(&self.session),
            std::cmp::Ordering::Greater => self.docs.get(i - 1).map(|d| &d.session),
        }
    }

    /// The name tab `i` shows: its file's name, or "Untitled".
    pub(crate) fn tab_name(&self, i: usize) -> String {
        self.tab_session(i).map_or_else(String::new, session_name)
    }

    /// An untitled document nobody has touched: opening a file replaces it instead of adding a tab.
    pub(crate) fn pristine(&self) -> bool {
        self.session.path().is_none() && !self.session.is_dirty() && !self.session.can_undo()
    }

    /// Shows tab `i`.
    pub(crate) fn switch_to(&mut self, i: usize) {
        if i == self.active || i >= self.tab_count() {
            return;
        }
        let current = self.park();
        self.docs.insert(self.active, current);
        if i < self.docs.len() {
            let next = self.docs.remove(i);
            self.install(next);
            self.active = i;
        }
    }

    /// Window › Next Document (⌃⇥) and Previous Document (⌃⇧⇥), round the tabs.
    pub(crate) fn cycle_tab(&mut self, forward: bool) {
        let n = self.tab_count();
        let i = if forward { (self.active + 1) % n } else { (self.active + n - 1) % n };
        self.switch_to(i);
    }

    /// Adds an empty document in a new tab at the end and shows it.
    pub(crate) fn new_tab(&mut self) {
        let fresh = Doc { session: self.session.sibling(), autosave: self.autosave.sibling(), hidden: 0 };
        let current = self.park();
        self.docs.insert(self.active, current);
        self.install(fresh);
        self.active = self.docs.len();
    }

    /// Runs `open` (which replaces the active document) in a new tab, or in place of an untouched
    /// untitled one. If it fails, the new tab goes again and the one before is shown.
    pub(crate) fn open_in_tab(&mut self, open: impl FnOnce(&mut Self) -> bool) -> bool {
        if self.pristine() {
            return open(self);
        }
        let back = self.active;
        self.new_tab();
        if open(self) {
            return true;
        }
        if back < self.docs.len() {
            let previous = self.docs.remove(back);
            let mut failed = self.install(previous);
            failed.autosave.finish();
            self.active = back;
        }
        false
    }

    /// The tab holding `path`, if it is open.
    pub(crate) fn tab_of(&self, path: &std::path::Path) -> Option<usize> {
        (0..self.tab_count()).find(|&i| self.tab_session(i).and_then(Session::path).is_some_and(|p| same_file(p, path)))
    }

    /// Closes the tab shown without asking, and shows its right-hand neighbour (or the one to its
    /// left). The last tab is replaced by a new untitled document.
    pub(crate) fn close_tab_now(&mut self) {
        let next = if self.docs.is_empty() {
            Doc { session: self.session.sibling(), autosave: self.autosave.sibling(), hidden: 0 }
        } else {
            let i = self.active.min(self.docs.len() - 1);
            self.active = i;
            self.docs.remove(i)
        };
        let mut closed = self.install(next);
        closed.autosave.finish();
    }

    /// The first tab with unsaved changes.
    pub(crate) fn first_dirty_tab(&self) -> Option<usize> {
        (0..self.tab_count()).find(|&i| self.tab_session(i).is_some_and(Session::is_dirty))
    }

    /// Keeps every document's autosave going: the copies of the tabs behind are written too, and
    /// their heartbeats kept fresh so another Guitario never takes them for a stopped run's.
    pub(crate) fn autosave_others(&mut self, now: std::time::Instant) -> Option<std::time::Duration> {
        let mut soonest = None;
        for d in &mut self.docs {
            if let Some(after) = d.autosave.tick(now, &d.session) {
                soonest = Some(soonest.map_or(after, |s: std::time::Duration| s.min(after)));
            }
            if let Some(e) = d.autosave.error.take() {
                self.status = e;
                self.status_is_error = true;
            }
        }
        soonest
    }

    /// A clean exit: no copy is left of a document that is saved, or whose changes were thrown away.
    pub(crate) fn finish_autosaves(&mut self, discarded: bool) {
        if !self.session.is_dirty() || discarded {
            self.autosave.finish();
        }
        for d in &mut self.docs {
            if !d.session.is_dirty() || discarded {
                d.autosave.finish();
            }
        }
    }

    /// The active document, taken out to wait in a tab (an empty one stands in until `install`).
    fn park(&mut self) -> Doc {
        let standin = Doc { session: Session::new(), autosave: Autosave::off(), hidden: 0 };
        self.install(standin)
    }

    /// Shows `doc` and returns the document it replaces. Playback stops; the clipboard, the
    /// recovery offer and the view carry over.
    fn install(&mut self, mut doc: Doc) -> Doc {
        self.stop();
        self.session.hand_clipboard_to(&mut doc.session);
        doc.autosave.found = std::mem::take(&mut self.autosave.found);
        let old = Doc {
            session: std::mem::replace(&mut self.session, doc.session),
            autosave: std::mem::replace(&mut self.autosave, doc.autosave),
            hidden: std::mem::replace(&mut self.view.hidden, doc.hidden),
        };
        self.follow_cursor = true;
        self.edit_buffers.clear();
        self.preloaded = None;
        self.play_when_ready = false;
        old
    }
}

/// A document's name: its file's name, or "Untitled".
pub(crate) fn session_name(s: &Session) -> String {
    s.path().and_then(|p| p.file_name()).map_or_else(|| "Untitled".to_string(), |n| n.to_string_lossy().into_owned())
}

/// Whether two paths name the same file (compared as given when one cannot be resolved).
fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn app() -> GuitarioApp {
        GuitarioApp::new(Session::new(), None)
    }

    fn fret(s: &Session) -> Option<u8> {
        s.score().tracks.first()?.measures.first()?.voices.first()?.beats.first()?.notes.first().map(|n| n.fret)
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("gt-docs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).ok();
        d
    }

    #[test]
    fn new_adds_a_tab_and_each_tab_keeps_its_own_score_cursor_and_undo() {
        let mut a = app();
        a.run("note.setFret", json!({"fret": 3}));
        a.new_document();
        assert_eq!((a.tab_count(), a.active_tab()), (2, 1));
        assert!(a.add_track.open, "a new score asks for its first track");
        a.add_track.open = false;
        assert_eq!(fret(&a.session), None);
        a.run("note.setFret", json!({"fret": 9}));
        a.switch_to(0);
        assert_eq!(fret(&a.session), Some(3));
        a.run("edit.undo", Value::Null);
        assert_eq!(fret(&a.session), None);
        a.cycle_tab(true);
        assert_eq!((a.active_tab(), fret(&a.session)), (1, Some(9)));
        a.cycle_tab(true);
        assert_eq!(a.active_tab(), 0, "next wraps round");
        a.cycle_tab(false);
        assert_eq!(a.active_tab(), 1);
        assert_eq!(a.tab_name(0), "Untitled");
        // Out of range does nothing.
        a.switch_to(7);
        assert_eq!((a.tab_count(), a.active_tab()), (2, 1));
    }

    #[test]
    fn copy_in_one_tab_and_paste_in_another() {
        let mut a = app();
        a.run("note.setFret", json!({"fret": 5}));
        a.run("edit.copy", Value::Null);
        a.new_document();
        a.add_track.open = false;
        a.run("edit.paste", Value::Null);
        assert_eq!(fret(&a.session), Some(5));
        a.switch_to(0);
        assert!(a.session.clipboard().is_some(), "the clipboard follows the tab shown");
    }

    #[test]
    fn opening_replaces_an_untouched_untitled_tab_switches_to_an_open_file_and_a_failure_goes_back() {
        let dir = temp("open");
        let (one, two) = (dir.join("one.gt"), dir.join("two.gt"));
        let mut s = Session::new();
        s.run("note.setFret", &json!({"fret": 1})).unwrap();
        s.run("file.save", &json!({"path": one.display().to_string()})).unwrap();
        s.run("note.setFret", &json!({"fret": 2})).unwrap();
        s.run("file.save", &json!({"path": two.display().to_string()})).unwrap();
        let mut a = app();
        a.open_path(&one.display().to_string());
        assert_eq!(a.tab_count(), 1, "the untouched untitled document made way");
        a.open_path(&two.display().to_string());
        assert_eq!((a.tab_count(), a.active_tab(), a.tab_name(1)), (2, 1, "two.gt".to_string()));
        // Opening a file that is open shows its tab.
        a.open_path(&one.display().to_string());
        assert_eq!((a.tab_count(), a.active_tab()), (2, 0));
        assert_eq!(a.tab_of(&two), Some(1));
        // A file that does not open adds no tab and says why.
        a.open_path(&dir.join("missing.gt").display().to_string());
        assert_eq!((a.tab_count(), a.active_tab(), a.tab_name(0)), (2, 0, "one.gt".to_string()));
        assert!(a.status_is_error && !a.status.is_empty());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn close_asks_about_unsaved_changes_and_shows_a_neighbour() {
        let mut a = app();
        a.run("note.setFret", json!({"fret": 1}));
        a.new_document();
        a.add_track.open = false;
        a.new_document();
        a.add_track.open = false;
        a.run("note.setFret", json!({"fret": 3}));
        a.switch_to(1);
        // Untouched: closes at once, and the tab to the right is shown.
        a.close_document();
        assert_eq!((a.tab_count(), a.active_tab(), fret(&a.session)), (2, 1, Some(3)));
        // Unsaved: asks first, and Cancel keeps it.
        a.close_document();
        assert!(a.pending.is_some() && a.modal_open());
        a.pending = None;
        assert_eq!(a.tab_count(), 2);
        // Closing the last tab leaves an untitled document.
        a.close_tab_now();
        a.close_tab_now();
        assert_eq!((a.tab_count(), a.active_tab(), a.session.is_dirty()), (1, 0, false));
    }

    #[test]
    fn close_all_asks_once_per_unsaved_document() {
        let mut a = app();
        a.run("note.setFret", json!({"fret": 1}));
        a.new_document();
        a.add_track.open = false;
        a.new_document();
        a.add_track.open = false;
        a.run("note.setFret", json!({"fret": 3}));
        a.close_all();
        // The shown tab is unsaved: it asks; Don't Save closes it and moves on to the next.
        assert!(a.pending.is_some());
        a.answer_prompt(2);
        assert!(a.pending.is_some(), "the first tab is unsaved too");
        assert_eq!(fret(&a.session), Some(1));
        a.answer_prompt(2);
        assert!(a.pending.is_none());
        assert_eq!((a.tab_count(), a.session.is_dirty(), fret(&a.session)), (1, false, None));
    }

    #[test]
    fn close_others_keeps_one_tab_and_asks_about_the_unsaved_rest() {
        let mut a = app();
        a.run("note.setFret", json!({"fret": 1}));
        for f in [2, 3] {
            a.new_document();
            a.add_track.open = false;
            a.run("note.setFret", json!({"fret": f}));
        }
        a.new_document();
        a.add_track.open = false;
        // Keep the third (fret 3); the first and second are unsaved, the fourth is not.
        a.close_others(2);
        assert_eq!((a.pending, fret(&a.session)), (Some(crate::Pending::CloseOthers(2)), Some(1)));
        a.answer_prompt(2);
        assert_eq!((a.pending, fret(&a.session)), (Some(crate::Pending::CloseOthers(1)), Some(2)));
        a.answer_prompt(2);
        assert!(a.pending.is_none());
        assert_eq!((a.tab_count(), fret(&a.session)), (1, Some(3)));
    }

    #[test]
    fn quitting_asks_about_each_unsaved_tab_and_cancel_stops_it() {
        let mut a = app();
        a.run("note.setFret", json!({"fret": 1}));
        a.new_document();
        a.add_track.open = false;
        assert!(a.ask_before_quit(), "a tab behind is unsaved");
        assert_eq!(a.active_tab(), 0, "its tab is shown");
        a.answer_prompt(3);
        assert!(a.pending.is_none() && a.tab_count() == 2 && !a.discard_confirmed);
        assert!(a.ask_before_quit());
        a.answer_prompt(2);
        assert!(a.discard_confirmed, "nothing else is unsaved: the window may close");
        assert!(!a.ask_before_quit());
    }

    #[test]
    fn every_tab_is_autosaved_and_a_clean_exit_removes_the_saved_ones() {
        let dir = temp("autosave");
        let mut a = app().with_autosave_dir(dir.clone());
        a.run("note.setFret", json!({"fret": 4}));
        a.new_document();
        a.add_track.open = false;
        a.run("note.setFret", json!({"fret": 6}));
        let now = std::time::Instant::now();
        a.autosave.tick(now, &a.session);
        a.autosave_others(now);
        a.autosave.wait();
        for d in &mut a.docs {
            d.autosave.wait();
        }
        let copies = || std::fs::read_dir(&dir).map(|r| r.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "gt")).count()).unwrap_or(0);
        assert_eq!(copies(), 2, "one copy per unsaved document");
        // Closing a tab with Don't Save removes its copy.
        a.close_tab_now();
        assert_eq!(copies(), 1);
        a.finish_autosaves(true);
        assert_eq!(copies(), 0);
        std::fs::remove_dir_all(dir).ok();
    }
}
