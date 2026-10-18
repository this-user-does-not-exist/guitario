//! Edit › Paste Special…: paste the clipboard a number of times, inserting new bars or replacing
//! what is there, into the tracks chosen. An all-track clip always goes to every track as new bars.

use crate::GuitarioApp;
use serde_json::{Value, json};

#[derive(Default)]
pub(crate) struct PasteDialog {
    pub(crate) open: bool,
    count: u32,
    insert: bool,
    /// One flag per track of the score: paste a one-track clip there.
    tracks: Vec<bool>,
    /// The clipboard holds whole bars of every track.
    all_tracks: bool,
    error: Option<String>,
}

impl PasteDialog {
    /// What OK sends to `edit.pasteSpecial`.
    pub(crate) fn params(&self) -> Value {
        if self.all_tracks {
            return json!({"count": self.count});
        }
        let tracks: Vec<usize> = self.tracks.iter().enumerate().filter(|(_, on)| **on).map(|(i, _)| i).collect();
        json!({"count": self.count, "mode": if self.insert { "insert" } else { "replace" }, "tracks": tracks})
    }

    fn ready(&self) -> bool {
        self.all_tracks || self.tracks.iter().any(|on| *on)
    }
}

impl GuitarioApp {
    /// Edit › Paste Special…: needs something on the clipboard.
    pub fn open_paste_special(&mut self) {
        let Some(clip) = self.session.clipboard() else {
            self.status = "Copy something first: the clipboard is empty.".into();
            self.status_is_error = true;
            return;
        };
        let all_tracks = clip.is_all_tracks();
        let cursor = self.session.cursor().track;
        let tracks = (0..self.session.score().tracks.len()).map(|i| i == cursor).collect();
        self.paste = PasteDialog { open: true, count: 1, insert: all_tracks, tracks, all_tracks, error: None };
    }

    pub(crate) fn paste_dialog(&mut self, ctx: &egui::Context) {
        if !self.paste.open {
            return;
        }
        let names: Vec<String> = self.session.score().tracks.iter().enumerate().map(|(i, t)| format!("{}. {}", i + 1, t.name)).collect();
        let (mut apply, mut cancel) = (false, false);
        let d = &mut self.paste;
        egui::Modal::new(egui::Id::new("paste-special")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.heading("Paste Special");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Number of times");
                ui.add(egui::DragValue::new(&mut d.count).range(1..=100));
            });
            ui.add_space(4.0);
            ui.add_enabled_ui(!d.all_tracks, |ui| {
                ui.radio_value(&mut d.insert, false, "Replace from the cursor");
                ui.radio_value(&mut d.insert, true, "Insert new bars before the cursor's bar");
            });
            ui.add_space(4.0);
            if d.all_tracks {
                ui.label(egui::RichText::new("The clipboard holds whole bars of every track: they are inserted on every track.").weak());
            } else {
                ui.label("Paste into");
                egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                    for (on, name) in d.tracks.iter_mut().zip(&names) {
                        ui.checkbox(on, name);
                    }
                });
            }
            if let Some(e) = &d.error {
                ui.colored_label(egui::Color32::from_rgb(0xff, 0x7a, 0x6a), e);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ready = d.ready();
                if ui.add_enabled(ready, egui::Button::new("OK")).clicked() || (ready && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    apply = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        });
        if apply {
            self.apply_paste_special();
        } else if cancel {
            self.paste.open = false;
        }
    }

    /// OK: pastes; on failure the window stays open and says why.
    pub(crate) fn apply_paste_special(&mut self) {
        match self.session.run("edit.pasteSpecial", &self.paste.params()) {
            Ok(_) => {
                self.paste.open = false;
                self.status_is_error = false;
            }
            Err(e) => self.paste.error = Some(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    fn app_with_copied_note() -> GuitarioApp {
        let mut s = Session::new();
        s.run("beat.setDuration", &json!({"value": 4})).unwrap();
        s.run("note.setFret", &json!({"fret": 5})).unwrap();
        s.run("edit.copy", &json!(null)).unwrap();
        GuitarioApp::new(s, None)
    }

    #[test]
    fn with_nothing_copied_the_window_does_not_open() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.open_paste_special();
        assert!(!app.paste.open && app.status_is_error);
    }

    #[test]
    fn it_starts_on_the_cursor_track_once_replacing_and_pastes_what_it_shows() {
        let mut app = app_with_copied_note();
        app.open_paste_special();
        assert!(app.paste.open && !app.paste.all_tracks && !app.paste.insert);
        assert_eq!(app.paste.params(), json!({"count": 1, "mode": "replace", "tracks": [0]}));
        let bars = app.session.score().headers.len();
        app.paste.count = 3;
        app.paste.insert = true;
        app.apply_paste_special();
        assert!(!app.paste.open);
        assert_eq!(app.session.score().headers.len(), bars + 3);
    }

    #[test]
    fn no_track_chosen_means_no_ok_and_an_error_keeps_the_window_open() {
        let mut app = app_with_copied_note();
        app.open_paste_special();
        app.paste.tracks.iter_mut().for_each(|t| *t = false);
        assert!(!app.paste.ready());
        app.apply_paste_special();
        assert!(app.paste.open && app.paste.error.is_some());
    }

    #[test]
    fn an_all_track_clip_always_inserts_on_every_track() {
        let mut app = app_with_copied_note();
        app.session.run("edit.copyAllTracks", &json!(null)).unwrap();
        app.open_paste_special();
        assert!(app.paste.all_tracks && app.paste.insert);
        assert_eq!(app.paste.params(), json!({"count": 1}));
        let ctx = egui::Context::default();
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.paste_dialog(ui.ctx())).textures_delta.clear();
        }
        assert!(app.paste.open);
    }
}
