//! File › Preferences…: the theme (dark or light) and the scale of the interface, saved in the
//! preferences. Changes apply on OK.

use crate::GuitarioApp;
use crate::prefs::{MAX_UI_SCALE, MIN_UI_SCALE, UiPrefs};
use crate::theme::{self, Theme};

#[derive(Default)]
pub(crate) struct PreferencesDialog {
    pub(crate) open: bool,
    /// What the dialog shows, applied on OK.
    pub(crate) choice: UiPrefs,
}

impl GuitarioApp {
    /// File › Preferences… (the snapshot example opens it too).
    pub fn open_preferences(&mut self) {
        self.prefs_dialog = PreferencesDialog { open: true, choice: self.ui };
    }

    /// Sets the look before the first frame, which installs it (the snapshot example).
    pub fn set_ui_prefs(&mut self, ui: UiPrefs) {
        self.ui = ui;
    }

    /// Keeps `ui`, switches the tokens and rescales egui.
    pub(crate) fn apply_ui_prefs(&mut self, ctx: &egui::Context, ui: UiPrefs) {
        self.ui = ui;
        theme::set_theme(ui.theme);
        theme::apply(ctx);
        ctx.set_zoom_factor(ui.scale);
    }

    pub(crate) fn preferences_dialog(&mut self, ctx: &egui::Context) {
        if !self.prefs_dialog.open {
            return;
        }
        let (mut ok, mut cancel) = (false, false);
        let d = &mut self.prefs_dialog;
        egui::Modal::new(egui::Id::new("preferences")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Preferences");
            ui.add_space(6.0);
            egui::Grid::new("prefs-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Theme");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut d.choice.theme, Theme::Dark, "Dark");
                    ui.selectable_value(&mut d.choice.theme, Theme::Light, "Light");
                });
                ui.end_row();
                ui.label("Interface scale");
                let mut percent = (d.choice.scale * 100.0).round();
                if ui.add(egui::Slider::new(&mut percent, MIN_UI_SCALE * 100.0..=MAX_UI_SCALE * 100.0).suffix(" %").step_by(5.0)).changed() {
                    d.choice.scale = percent / 100.0;
                }
                ui.end_row();
            });
            ui.label(egui::RichText::new("The score has its own zoom in the top bar.").small().weak());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    ok = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                if ui.button("Defaults").clicked() {
                    d.choice = UiPrefs::default();
                }
            });
        });
        if ok {
            let choice = self.prefs_dialog.choice;
            self.apply_ui_prefs(ctx, choice);
            self.prefs_dialog.open = false;
        } else if cancel {
            self.prefs_dialog.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    #[test]
    fn ok_applies_the_theme_and_scale_and_they_are_saved() {
        let mut app = GuitarioApp::new(Session::new(), None);
        let ctx = egui::Context::default();
        app.open_preferences();
        assert!(app.prefs_dialog.open);
        app.prefs_dialog.choice = UiPrefs { theme: Theme::Light, scale: 1.5 };
        let choice = app.prefs_dialog.choice;
        app.apply_ui_prefs(&ctx, choice);
        assert_eq!(theme::theme(), Theme::Light);
        assert_eq!(theme::tokens().text, theme::LIGHT.text);
        // egui takes the scale at the start of the next frame.
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
        assert_eq!(ctx.zoom_factor(), 1.5);
        app.sync_settings(&ctx);
        assert_eq!(app.settings.get().ui, UiPrefs { theme: Theme::Light, scale: 1.5 });
        // Leave the process-wide theme as other tests expect it.
        theme::set_theme(Theme::Dark);
    }
}
