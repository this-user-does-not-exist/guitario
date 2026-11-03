//! File › Preferences…: the theme (dark or light), the score's colours and the scale of the
//! interface, saved in the preferences. Changes apply on OK.

use crate::GuitarioApp;
use crate::prefs::{MAX_UI_SCALE, MIN_UI_SCALE, UiPrefs};
use crate::theme::{self, ScoreColours, Theme};

/// The choices of View › Score Colors, in menu order.
pub const SCORE_COLOURS: [(ScoreColours, &str); 3] =
    [(ScoreColours::Paper, "Paper"), (ScoreColours::Dark, "Dark"), (ScoreColours::FollowTheme, "Follow Theme")];

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

    pub fn ui_prefs(&self) -> UiPrefs {
        self.ui
    }

    /// View › Score Colors, which Preferences also shows. The pages read it each frame.
    pub(crate) fn set_score_colours(&mut self, colours: ScoreColours) {
        self.ui.score_colours = colours;
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
                ui.label("Score colors");
                ui.horizontal(|ui| {
                    for (c, label) in SCORE_COLOURS {
                        ui.selectable_value(&mut d.choice.score_colours, c, label);
                    }
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
        app.prefs_dialog.choice = UiPrefs { theme: Theme::Light, scale: 1.5, score_colours: ScoreColours::Dark };
        let choice = app.prefs_dialog.choice;
        app.apply_ui_prefs(&ctx, choice);
        assert_eq!(theme::theme(), Theme::Light);
        assert_eq!(theme::tokens().text, theme::LIGHT.text);
        // egui takes the scale at the start of the next frame.
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
        assert_eq!(ctx.zoom_factor(), 1.5);
        app.sync_settings(&ctx);
        assert_eq!(app.settings.get().ui, UiPrefs { theme: Theme::Light, scale: 1.5, score_colours: ScoreColours::Dark });
        // Leave the process-wide theme as other tests expect it.
        theme::set_theme(Theme::Dark);
    }

    /// Settings saved before View › Score Colors open on paper, and the choice round-trips.
    #[test]
    fn score_colours_default_to_paper_and_round_trip() {
        let old: UiPrefs = serde_json::from_str(r#"{"theme": "light", "scale": 1.25}"#).unwrap();
        assert_eq!(old, UiPrefs { theme: Theme::Light, scale: 1.25, score_colours: ScoreColours::Paper });
        for (c, _) in SCORE_COLOURS {
            let ui = UiPrefs { score_colours: c, ..UiPrefs::default() };
            assert_eq!(serde_json::from_str::<UiPrefs>(&serde_json::to_string(&ui).unwrap()).unwrap(), ui);
        }
        assert!(serde_json::to_string(&UiPrefs { score_colours: ScoreColours::FollowTheme, ..UiPrefs::default() }).unwrap().contains(r#""follow_theme""#));
    }

    /// The menu and Preferences share one value, which reaches the pages and the saved settings.
    #[test]
    fn the_score_colors_menu_and_preferences_agree() {
        use crate::menu_catalog::{Action, CATALOG, UiAction};
        let mut app = GuitarioApp::new(Session::new(), None);
        let ctx = egui::Context::default();
        let items: Vec<_> = CATALOG.iter().filter(|i| i.path == "View/Score Colors").collect();
        assert_eq!(items.iter().map(|i| i.label).collect::<Vec<_>>(), SCORE_COLOURS.map(|(_, l)| l));
        for (i, (c, _)) in items.iter().zip(SCORE_COLOURS) {
            assert_eq!(i.action, Action::Ui(UiAction::ScoreColours(c)));
            assert!(i.extension && i.shortcut.is_empty());
            app.ui_action(UiAction::ScoreColours(c));
            assert_eq!(app.ui_prefs().score_colours, c);
            app.open_preferences();
            assert_eq!(app.prefs_dialog.choice.score_colours, c, "Preferences opens on the menu's choice");
        }
        // Not through OK: that switches the process-wide theme other tests read.
        app.ui_action(UiAction::ScoreColours(ScoreColours::Dark));
        app.sync_settings(&ctx);
        assert_eq!(app.settings.get().ui.score_colours, ScoreColours::Dark);
        assert_eq!(theme::page_tokens(app.ui_prefs().theme, app.ui_prefs().score_colours).page, theme::DARK_PAGE.page);
    }
}
