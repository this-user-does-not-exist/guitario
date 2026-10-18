//! Sound › Audio Settings…: the output device and its buffer size, saved in the preferences. A
//! change closes the sound card; the next Play opens the new one.

use crate::GuitarioApp;
use guitario_audio::{AudioSettings, BUFFER_SIZES, OutputInfo};

#[derive(Default)]
pub(crate) struct AudioSettingsDialog {
    pub(crate) open: bool,
    /// What the dialog shows, applied on OK.
    pub(crate) choice: AudioSettings,
    /// The devices found when the dialog opened.
    outputs: Vec<OutputInfo>,
}

impl GuitarioApp {
    /// Sound › Audio Settings… (the snapshot example opens it too).
    pub fn open_audio_settings_dialog(&mut self) {
        self.open_audio_settings();
    }

    pub(crate) fn open_audio_settings(&mut self) {
        self.audio_dialog = AudioSettingsDialog { open: true, choice: self.audio_settings.clone(), outputs: guitario_audio::outputs() };
    }

    /// OK: keeps the settings and closes the sound card so the next Play uses them.
    pub(crate) fn apply_audio_settings(&mut self, settings: AudioSettings) {
        if settings != self.audio_settings {
            if self.transport.playing {
                self.stop();
            }
            self.audio = None;
            self.audio_settings = settings;
        }
        self.audio_dialog.open = false;
    }

    pub(crate) fn audio_settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.audio_dialog.open {
            return;
        }
        let playing_on = self.audio.as_ref().map(|a| a.device.clone());
        let (mut ok, mut cancel, mut bank) = (false, false, false);
        let d = &mut self.audio_dialog;
        egui::Modal::new(egui::Id::new("audio-settings")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Audio Settings");
            ui.add_space(6.0);
            egui::Grid::new("audio-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Output");
                let shown = d.choice.device.clone().unwrap_or_else(|| {
                    d.outputs.iter().find(|o| o.default).map_or_else(|| "System default".into(), |o| format!("System default ({})", o.name))
                });
                egui::ComboBox::from_id_salt("audio-output").selected_text(shown).width(250.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut d.choice.device, None, "System default");
                    for o in &d.outputs {
                        ui.selectable_value(&mut d.choice.device, Some(o.name.clone()), &o.name);
                    }
                });
                ui.end_row();
                ui.label("Buffer");
                let label = |b: Option<u32>| b.map_or_else(|| "Device default".to_string(), |n| format!("{n} samples"));
                egui::ComboBox::from_id_salt("audio-buffer").selected_text(label(d.choice.buffer)).width(250.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut d.choice.buffer, None, label(None));
                    for n in BUFFER_SIZES {
                        ui.selectable_value(&mut d.choice.buffer, Some(n), label(Some(n)));
                    }
                });
                ui.end_row();
            });
            if d.outputs.is_empty() {
                ui.label(egui::RichText::new("No output devices were found.").weak());
            }
            if let Some(name) = &playing_on {
                ui.label(egui::RichText::new(format!("Playing through {name}.")).small().weak());
            }
            ui.label(egui::RichText::new("A smaller buffer answers sooner; a larger one is safer from crackles.").small().weak());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    ok = true;
                }
                if ui.button("Sound Bank…").on_hover_text("The recorded instruments and their libraries").clicked() {
                    bank = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        });
        if ok {
            let choice = self.audio_dialog.choice.clone();
            self.apply_audio_settings(choice);
        } else if cancel {
            self.audio_dialog.open = false;
        } else if bank {
            self.audio_dialog.open = false;
            self.open_sound_bank();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    #[test]
    fn applying_keeps_the_choice_closes_the_card_and_saves_it() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.audio_dialog = AudioSettingsDialog { open: true, choice: AudioSettings { device: Some("Interface".into()), buffer: Some(256) }, outputs: vec![] };
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::definitions().0);
        ctx.run_ui(egui::RawInput::default(), |ui| app.audio_settings_dialog(ui.ctx())).textures_delta.clear();
        assert!(app.audio_dialog.open);
        let choice = app.audio_dialog.choice.clone();
        app.apply_audio_settings(choice);
        assert_eq!(app.audio_settings, AudioSettings { device: Some("Interface".into()), buffer: Some(256) });
        assert!(!app.audio_dialog.open && app.audio.is_none());
        // They are kept in the preferences.
        let ctx2 = egui::Context::default();
        app.sync_settings(&ctx2);
        assert_eq!((app.settings.get().playback.output.as_deref(), app.settings.get().playback.buffer), (Some("Interface"), Some(256)));
    }
}
