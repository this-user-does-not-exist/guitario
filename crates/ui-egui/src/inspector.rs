//! The inspector on the right: Song and Track tabs, as in Guitar Pro. Text fields commit one undo
//! step when they lose focus.

use crate::GuitarioApp;
use crate::theme::{tokens, track_color};
use egui::Color32;
use guitario_theory::Tuning;
use serde_json::{Value, json};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Song,
    Track,
}

/// General MIDI programs that make sense for a fretted instrument.
const PROGRAMS: [(u8, &str); 12] = [
    (24, "Nylon guitar"),
    (25, "Steel guitar"),
    (26, "Jazz guitar"),
    (27, "Clean guitar"),
    (28, "Muted guitar"),
    (29, "Overdrive guitar"),
    (30, "Distortion guitar"),
    (31, "Guitar harmonics"),
    (32, "Acoustic bass"),
    (33, "Fingered bass"),
    (34, "Picked bass"),
    (105, "Banjo"),
];

/// The name of a General MIDI program as the instrument lists show it.
pub(crate) fn program_name(program: u8) -> String {
    PROGRAMS.iter().find(|p| p.0 == program).map_or_else(|| format!("GM program {program}"), |p| p.1.to_string())
}

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let t = tokens();
    // Two equal tabs across the panel, the open one in the accent colour.
    let (strip, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::hover());
    for (i, (tab, name)) in [(Tab::Song, "SONG"), (Tab::Track, "TRACK")].into_iter().enumerate() {
        let half = strip.width() / 2.0;
        let r = egui::Rect::from_min_size(strip.min + egui::vec2(half * i as f32, 0.0), egui::vec2(half, strip.height()));
        let resp = ui.interact(r, egui::Id::new(("inspector-tab", i)), egui::Sense::click());
        let open = app.inspector_tab == tab;
        let fill = if open {
            t.accent
        } else if resp.hovered() {
            t.control_hover
        } else {
            t.control
        };
        ui.painter().rect_filled(r, 0.0, fill);
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(12.5), if open { Color32::WHITE } else { t.text_dim });
        if resp.clicked() {
            app.inspector_tab = tab;
        }
    }
    ui.add_space(6.0);
    let mut actions: Vec<(&'static str, Value)> = Vec::new();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| match app.inspector_tab {
        Tab::Song => song(app, ui, &mut actions),
        Tab::Track => track(app, ui, &mut actions),
    });
    for (id, p) in actions {
        app.run(id, p);
    }
}

/// A section: its name in large capitals, then the body (Guitar Pro's inspector has no folding).
fn section(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(8.0);
    ui.label(egui::RichText::new(title.to_uppercase()).size(15.0).strong());
    ui.add_space(4.0);
    body(ui);
}

fn field(
    app: &mut GuitarioApp,
    ui: &mut egui::Ui,
    label: &str,
    key: &'static str,
    current: &str,
    commit: impl FnOnce(&str) -> (&'static str, Value),
    actions: &mut Vec<(&'static str, Value)>,
) {
    ui.label(egui::RichText::new(label).small().color(tokens().text_dim));
    let mut text = app.edit_buffers.get(key).cloned().unwrap_or_else(|| current.to_string());
    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(f32::INFINITY));
    if r.changed() {
        app.edit_buffers.insert(key, text.clone());
    }
    if r.lost_focus() {
        if text != current {
            actions.push(commit(&text));
        }
        app.edit_buffers.remove(key);
    }
    ui.add_space(4.0);
}

fn song(app: &mut GuitarioApp, ui: &mut egui::Ui, actions: &mut Vec<(&'static str, Value)>) {
    let score = app.session.score().clone();
    let meta = &score.meta;
    section(ui, "Information", |ui| {
        field(app, ui, "Title", "title", &meta.title, |v| ("score.setMeta", json!({"title": v})), actions);
        field(app, ui, "Artist", "artist", &meta.artist, |v| ("score.setMeta", json!({"artist": v})), actions);
        field(app, ui, "Album", "album", &meta.album, |v| ("score.setMeta", json!({"album": v})), actions);
        field(app, ui, "Tab author", "author", &meta.author, |v| ("score.setMeta", json!({"author": v})), actions);
    });
    section(ui, "Tempo", |ui| {
        let mut bpm = i64::from(score.tempo);
        ui.horizontal(|ui| {
            ui.label("♩ =");
            if ui.add(egui::DragValue::new(&mut bpm).range(20..=400)).changed() {
                actions.push(("score.setTempo", json!({"bpm": bpm})));
            }
        });
    });
    section(ui, "Notes", |ui| {
        field(app, ui, "Notes", "notes", &meta.notes, |v| ("score.setMeta", json!({"notes": v})), actions);
    });
}

fn track(app: &mut GuitarioApp, ui: &mut egui::Ui, actions: &mut Vec<(&'static str, Value)>) {
    let score = app.session.score().clone();
    let c = app.session.cursor();
    let Some(tr) = score.tracks.get(c.track) else { return };
    section(ui, "Information", |ui| {
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 2.0, track_color(c.track));
            ui.label(format!("Track {}", c.track + 1));
        });
        field(app, ui, "Name", "track.name", &tr.name, |v| ("track.rename", json!({"name": v})), actions);
        if tr.is_drums() {
            ui.label(egui::RichText::new("Instrument").small().color(tokens().text_dim));
            ui.label("Drum kit (General MIDI)");
            return;
        }
        ui.label(egui::RichText::new("Instrument").small().color(tokens().text_dim));
        let current = program_name(tr.program);
        egui::ComboBox::from_id_salt("program").width(ui.available_width()).selected_text(current).show_ui(ui, |ui| {
            for (p, name) in PROGRAMS {
                if ui.selectable_label(tr.program == p, name).clicked() {
                    actions.push(("track.setProgram", json!({"program": p})));
                }
            }
        });
    });
    if tr.is_drums() {
        section(ui, "Kit", |ui| {
            ui.label(egui::RichText::new("Type a General MIDI number (36 kick, 38 snare, 42 hi-hat, 49 crash) or click the drum pad.").small());
            for lane in guitario_theory::drums::LANES {
                ui.label(egui::RichText::new(lane).small().color(tokens().text_dim));
            }
        });
    }
    // Kit lanes have no tuning or capo.
    if !tr.is_drums() {
        section(ui, "Musical notation", |ui| {
            egui::ComboBox::from_id_salt("tuning").width(ui.available_width()).selected_text(&tr.tuning.name).show_ui(ui, |ui| {
                for preset in Tuning::presets() {
                    if ui.selectable_label(preset.strings == tr.tuning.strings, &preset.name).clicked() {
                        actions.push(("track.setTuning", json!({"preset": preset.name})));
                    }
                }
            });
            let names: Vec<String> = tr.tuning.strings.iter().map(|p| guitario_theory::Pitch(*p).name()).collect();
            ui.label(egui::RichText::new(names.join("  ")).monospace().small());
            let mut capo = i64::from(tr.capo);
            ui.horizontal(|ui| {
                ui.label("Capo");
                if ui.add(egui::DragValue::new(&mut capo).range(0..=24)).changed() {
                    actions.push(("track.setCapo", json!({"fret": capo})));
                }
            });
        });
    }
    section(ui, "Sounds", |ui| {
        let mut vol = i64::from(tr.volume);
        if ui.add(egui::Slider::new(&mut vol, 0..=127).text("Volume")).drag_stopped() {
            actions.push(("track.setMix", json!({"volume": vol})));
        }
        let mut pan = i64::from(tr.pan);
        if ui.add(egui::Slider::new(&mut pan, -63..=63).text("Pan")).drag_stopped() {
            actions.push(("track.setMix", json!({"pan": pan})));
        }
        ui.horizontal(|ui| {
            if ui.selectable_label(tr.muted, "Mute").clicked() {
                actions.push(("track.setMix", json!({"muted": !tr.muted})));
            }
            if ui.selectable_label(tr.solo, "Solo").clicked() {
                actions.push(("track.setMix", json!({"solo": !tr.solo})));
            }
        });
    });
    if let Some(h) = score.headers.get(c.measure) {
        section(ui, &format!("Bar {}", c.measure + 1), |ui| {
            ui.label(format!("Time signature  {}/{}", h.time_signature.numerator, h.time_signature.denominator));
            ui.label(format!(
                "Key  {}",
                match h.key {
                    0 => "C major".to_string(),
                    k if k > 0 => format!("{k} sharp(s)"),
                    k => format!("{} flat(s)", -k),
                }
            ));
            ui.label(format!("Tempo  {}", h.tempo.map_or_else(|| format!("{} (inherited)", score.tempo_at(c.measure)), |t| t.to_string())));
        });
    }
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.button("Add track").clicked() {
            actions.push(("track.add", json!({})));
        }
        if ui.button("Add drums").clicked() {
            actions.push(("track.addDrums", json!({})));
        }
        if ui.add_enabled(score.tracks.len() > 1, egui::Button::new("Remove track")).clicked() {
            actions.push(("track.remove", Value::Null));
        }
    });
}
