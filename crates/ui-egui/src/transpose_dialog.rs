//! Edit › Transpose… and Track › Transpose Track…: one dialog that moves notes by any interval, for
//! the selection (or the note at the cursor), the track on screen, or every track but the drums.

use crate::GuitarioApp;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    Selection,
    Track,
    AllTracks,
}

#[derive(Default)]
pub(crate) struct TransposeDialog {
    pub(crate) open: bool,
    semitones: i64,
    scope: Option<Scope>,
    /// Whether there is a selection to offer.
    has_selection: bool,
    error: Option<String>,
}

/// The interval's name, as a transpose dialog shows it.
pub(crate) fn interval_name(semitones: i64) -> String {
    const NAMES: [&str; 12] = [
        "unison",
        "minor second",
        "major second",
        "minor third",
        "major third",
        "perfect fourth",
        "tritone",
        "perfect fifth",
        "minor sixth",
        "major sixth",
        "minor seventh",
        "major seventh",
    ];
    let n = semitones.unsigned_abs();
    let name = match (n % 12, n / 12) {
        (0, 0) => "unison".to_string(),
        (0, 1) => "octave".to_string(),
        (0, o) => format!("{o} octaves"),
        (r, 0) => NAMES.get(r as usize).copied().unwrap_or("").to_string(),
        (r, o) => format!("{} + {o} octave{}", NAMES.get(r as usize).copied().unwrap_or(""), if o > 1 { "s" } else { "" }),
    };
    // "a minor third", "an octave", "2 octaves".
    let article = if name.starts_with(|c: char| c.is_ascii_digit()) {
        ""
    } else if name.starts_with('o') {
        "an "
    } else {
        "a "
    };
    match semitones.signum() {
        1 => format!("up {article}{name}"),
        -1 => format!("down {article}{name}"),
        _ => name,
    }
}

impl TransposeDialog {
    /// The command OK runs.
    pub(crate) fn command(&self) -> (&'static str, Value) {
        match self.scope.unwrap_or(Scope::Track) {
            Scope::Selection => ("edit.transpose", json!({"semitones": self.semitones})),
            Scope::Track => ("track.transpose", json!({"semitones": self.semitones})),
            Scope::AllTracks => ("track.transpose", json!({"semitones": self.semitones, "tracks": "all"})),
        }
    }
}

impl GuitarioApp {
    /// Opens the dialog for `scope`; Edit › Transpose… starts on the selection when there is one.
    pub(crate) fn open_transpose(&mut self, scope: Scope) {
        let has_selection = self.session.selection().is_some();
        let scope = if scope == Scope::Selection && !has_selection { Scope::Track } else { scope };
        self.transpose = TransposeDialog { open: true, semitones: 0, scope: Some(scope), has_selection, error: None };
    }

    pub(crate) fn transpose_dialog(&mut self, ctx: &egui::Context) {
        if !self.transpose.open {
            return;
        }
        let (mut apply, mut cancel) = (false, false);
        let d = &mut self.transpose;
        egui::Modal::new(egui::Id::new("transpose-dialog")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading("Transpose");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Semitones");
                ui.add(egui::DragValue::new(&mut d.semitones).range(-24..=24));
                ui.label(egui::RichText::new(interval_name(d.semitones)).weak());
            });
            ui.add_space(4.0);
            let mut scope = d.scope.unwrap_or(Scope::Track);
            ui.add_enabled_ui(d.has_selection, |ui| ui.radio_value(&mut scope, Scope::Selection, "Selection"));
            ui.radio_value(&mut scope, Scope::Track, "This track");
            ui.radio_value(&mut scope, Scope::AllTracks, "All tracks (not drums)");
            d.scope = Some(scope);
            if let Some(e) = &d.error {
                ui.colored_label(egui::Color32::from_rgb(0xff, 0x7a, 0x6a), e);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(d.semitones != 0, egui::Button::new("OK")).clicked() || (d.semitones != 0 && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    apply = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        });
        if apply {
            self.apply_transpose();
        } else if cancel {
            self.transpose.open = false;
        }
    }

    /// OK: runs the transposition; on failure the dialog stays open and says why.
    pub(crate) fn apply_transpose(&mut self) {
        let (id, params) = self.transpose.command();
        match self.session.run(id, &params) {
            Ok(_) => {
                self.transpose.open = false;
                self.status = format!("Transposed {}.", interval_name(self.transpose.semitones));
                self.status_is_error = false;
            }
            Err(e) => self.transpose.error = Some(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    #[test]
    fn intervals_are_named() {
        assert_eq!(interval_name(0), "unison");
        assert_eq!(interval_name(3), "up a minor third");
        assert_eq!(interval_name(-7), "down a perfect fifth");
        assert_eq!(interval_name(12), "up an octave");
        assert_eq!(interval_name(-14), "down a major second + 1 octave");
        assert_eq!(interval_name(24), "up 2 octaves");
    }

    #[test]
    fn each_scope_runs_its_command_and_a_failure_keeps_the_dialog_open() {
        let mut s = Session::new();
        s.run("cursor.set", &json!({"string": 5})).unwrap();
        s.run("note.setFret", &json!({"fret": 3})).unwrap();
        let mut app = GuitarioApp::new(s, None);
        // No selection: Edit › Transpose… falls back to the track.
        app.open_transpose(Scope::Selection);
        assert_eq!(app.transpose.scope, Some(Scope::Track));
        app.transpose.semitones = 2;
        assert_eq!(app.transpose.command(), ("track.transpose", json!({"semitones": 2})));
        app.transpose.scope = Some(Scope::AllTracks);
        assert_eq!(app.transpose.command().1["tracks"], "all");
        app.transpose.scope = Some(Scope::Selection);
        assert_eq!(app.transpose.command().0, "edit.transpose");
        // OK on the track: done and closed.
        app.transpose.scope = Some(Scope::Track);
        app.apply_transpose();
        assert!(!app.transpose.open);
        assert_eq!(app.session.score().tracks[0].measures[0].voices[0].beats[0].notes[0].fret, 5);
        // Down past the open low E: refused, the dialog stays open and says why, nothing moves.
        app.open_transpose(Scope::Track);
        app.transpose.semitones = -6;
        app.apply_transpose();
        assert!(app.transpose.open && app.transpose.error.as_deref().is_some_and(|e| e.contains("fretboard")), "{:?}", app.transpose.error);
        assert_eq!(app.session.score().tracks[0].measures[0].voices[0].beats[0].notes[0].fret, 5);
    }
}
