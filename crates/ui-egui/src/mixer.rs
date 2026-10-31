//! Sound › Mixer…: a channel strip per track (instrument, pan, volume fader, mute, solo and the
//! equalizer) and the master's, heard live while the song plays. A fader being dragged is sent straight to the player and becomes one
//! undo step when it is let go.

use crate::GuitarioApp;
use crate::theme::{tokens, track_color};
use guitario_playback::Mix;
use serde_json::json;

/// Which control of a strip is being dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Control {
    Volume,
    Pan,
    /// The master's fader and pan (the track index is unused).
    MasterVolume,
    MasterPan,
}

#[derive(Default)]
pub(crate) struct Mixer {
    pub(crate) open: bool,
    /// A fader or pan being dragged: track, control, value so far.
    pub(crate) drag: Option<(usize, Control, i64)>,
}

impl GuitarioApp {
    /// Sound › Mixer… (the snapshot example opens it too).
    pub fn open_mixer(&mut self) {
        self.mixer.open = true;
    }

    /// What the player should hear now: the score's mixer with any fader being dragged.
    pub(crate) fn live_mix(&self) -> Vec<Mix> {
        let mut mix: Vec<Mix> = self.session.score().tracks.iter().map(Mix::of).collect();
        if let Some((ti, control, v)) = self.mixer.drag
            && let Some(m) = mix.get_mut(ti)
        {
            match control {
                Control::Volume => m.volume = v.clamp(0, 127) as u8,
                Control::Pan => m.pan = v.clamp(-63, 63) as i8,
                Control::MasterVolume | Control::MasterPan => {}
            }
        }
        mix
    }

    pub(crate) fn mixer_window(&mut self, ctx: &egui::Context) {
        if !self.mixer.open {
            return;
        }
        let mut open = true;
        let mut actions: Vec<serde_json::Value> = Vec::new();
        let mut master_actions: Vec<serde_json::Value> = Vec::new();
        let mut open_eq = None;
        let mut drag = self.mixer.drag;
        let score = self.session.score().clone();
        let tracks: Vec<(String, bool, Mix, String)> =
            score.tracks.iter().map(|t| (t.name.clone(), t.is_drums(), Mix::of(t), crate::inspector::sound_name(t.program, t.players))).collect();
        egui::Window::new("Mixer").open(&mut open).resizable(false).collapsible(false).default_pos([140.0, 130.0]).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                for (ti, (name, drums, mix, sound)) in tracks.iter().enumerate() {
                    ui.vertical(|ui| {
                        ui.set_width(92.0);
                        let (bar, _) = ui.allocate_exact_size(egui::vec2(92.0, 4.0), egui::Sense::hover());
                        ui.painter().rect_filled(bar, 1.0, track_color(&score, ti));
                        ui.label(egui::RichText::new(name).strong().small()).on_hover_text(name);
                        let sound = if *drums { "Drum kit" } else { sound.as_str() };
                        ui.label(egui::RichText::new(sound).small().color(tokens().text_dim));
                        // Pan.
                        let mut pan = match drag {
                            Some((t, Control::Pan, v)) if t == ti => v,
                            _ => i64::from(mix.pan),
                        };
                        ui.spacing_mut().slider_width = 84.0;
                        let r = ui.add(egui::Slider::new(&mut pan, -63..=63).show_value(false)).on_hover_text(pan_label(pan));
                        if r.dragged() || r.changed() {
                            drag = Some((ti, Control::Pan, pan));
                        }
                        if r.drag_stopped() || (r.changed() && !r.dragged()) {
                            actions.push(json!({"track": ti, "pan": pan}));
                            drag = None;
                        }
                        if r.double_clicked() {
                            actions.push(json!({"track": ti, "pan": 0}));
                            drag = None;
                        }
                        ui.label(egui::RichText::new(pan_label(pan)).small());
                        // Volume fader.
                        let mut vol = match drag {
                            Some((t, Control::Volume, v)) if t == ti => v,
                            _ => i64::from(mix.volume),
                        };
                        ui.spacing_mut().slider_width = 160.0;
                        let r = ui.add(egui::Slider::new(&mut vol, 0..=127).vertical().show_value(false));
                        if r.dragged() || r.changed() {
                            drag = Some((ti, Control::Volume, vol));
                        }
                        if r.drag_stopped() || (r.changed() && !r.dragged()) {
                            actions.push(json!({"track": ti, "volume": vol}));
                            drag = None;
                        }
                        ui.label(egui::RichText::new(vol.to_string()).small().monospace());
                        ui.horizontal(|ui| {
                            if ui.selectable_label(mix.muted, "M").on_hover_text("Mute").clicked() {
                                actions.push(json!({"track": ti, "muted": !mix.muted}));
                            }
                            if ui.selectable_label(mix.solo, "S").on_hover_text("Solo").clicked() {
                                actions.push(json!({"track": ti, "solo": !mix.solo}));
                            }
                            let shaping = score.tracks.get(ti).is_some_and(|t| t.eq.on && !t.eq.is_neutral());
                            if ui.selectable_label(shaping, "EQ").on_hover_text("Equalizer").clicked() {
                                open_eq = Some(ti);
                            }
                        });
                    });
                    ui.add_space(10.0);
                }
                // The master: volume after every track.
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(72.0);
                    ui.label(egui::RichText::new("Master").strong().small());
                    ui.label(egui::RichText::new("Song").small().color(tokens().text_dim));
                    ui.add_space(38.0);
                    let mut vol = match drag {
                        Some((_, Control::MasterVolume, v)) => v,
                        _ => i64::from(score.master.volume),
                    };
                    ui.spacing_mut().slider_width = 160.0;
                    let r = ui.add(egui::Slider::new(&mut vol, 0..=127).vertical().show_value(false)).on_hover_text("Master volume (100 is unity)");
                    if r.dragged() || r.changed() {
                        drag = Some((usize::MAX, Control::MasterVolume, vol));
                    }
                    if r.drag_stopped() || (r.changed() && !r.dragged()) {
                        master_actions.push(json!({"volume": vol}));
                        drag = None;
                    }
                    ui.label(egui::RichText::new(vol.to_string()).small().monospace());
                });
            });
        });
        self.mixer.drag = drag;
        for params in actions {
            self.run("track.setMix", params);
        }
        for params in master_actions {
            self.run("score.setMaster", params);
        }
        if let Some(ti) = open_eq {
            self.open_equalizer(ti);
        }
        self.mixer.open = open;
        if self.mixer.drag.is_some() {
            self.push_mix();
        }
    }

    /// Sends what the mixer shows to the player, if one is playing.
    pub(crate) fn push_mix(&self) {
        let (mix, master) = (self.live_mix(), self.live_master());
        if let Some(audio) = &self.audio
            && let Some(p) = audio.slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_mut()
        {
            p.set_mix(&mix);
            p.set_master(&master);
        }
    }
}

/// "L 20", "C", "R 63".
pub(crate) fn pan_label(pan: i64) -> String {
    match pan {
        0 => "C".into(),
        p if p < 0 => format!("L {}", -p),
        p => format!("R {p}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    #[test]
    fn a_dragged_fader_is_heard_before_it_is_let_go_and_then_saved_once() {
        let mut s = Session::new();
        s.run("track.add", &json!({"name": "Bass", "tuning": "bass"})).unwrap();
        let mut app = GuitarioApp::new(s, None);
        let saved = app.session.score().tracks[1].volume;
        app.mixer.drag = Some((1, Control::Volume, 20));
        let mix = app.live_mix();
        assert_eq!((mix[1].volume, mix[0].volume), (20, app.session.score().tracks[0].volume));
        assert_eq!(app.session.score().tracks[1].volume, saved, "nothing saved while dragging");
        // Out-of-range values never reach the player.
        app.mixer.drag = Some((0, Control::Pan, -500));
        assert_eq!(app.live_mix()[0].pan, -63);
        app.mixer.drag = Some((7, Control::Pan, 3));
        assert_eq!(app.live_mix().len(), 2);
        assert_eq!((pan_label(0), pan_label(-20), pan_label(63)), ("C".to_string(), "L 20".to_string(), "R 63".to_string()));
    }

    #[test]
    fn the_window_draws_every_track_and_keeps_its_state() {
        let mut s = Session::new();
        s.run("track.addDrums", &json!({})).unwrap();
        let mut app = GuitarioApp::new(s, None);
        app.mixer.open = true;
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::definitions().0);
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.mixer_window(ui.ctx())).textures_delta.clear();
        }
        assert!(app.mixer.open && app.mixer.drag.is_none());
    }
}
