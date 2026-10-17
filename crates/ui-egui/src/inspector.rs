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
        // White on both tabs, as in Guitar Pro; the light theme's pale closed tab takes dark text.
        let ink = if open || crate::theme::theme() == crate::theme::Theme::Dark { Color32::WHITE } else { t.text };
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, name, crate::fonts::sans_bold(13.0), ink);
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
    ui.label(egui::RichText::new(title.to_uppercase()).font(crate::fonts::sans_bold(18.0)));
    ui.add_space(4.0);
    body(ui);
}

impl GuitarioApp {
    /// Opens the inspector's Track tab (the snapshot example uses it).
    pub fn show_track_inspector(&mut self) {
        self.inspector_tab = Tab::Track;
    }
}

/// A text box that commits on Return or when it loses focus. `multiline` boxes take Ctrl+Return
/// (⌘Return) for a new line, as Guitar Pro's do. Returns the text to commit, if it changed.
fn text_box(app: &mut GuitarioApp, ui: &mut egui::Ui, key: &'static str, current: &str, width: f32, multiline: bool) -> Option<String> {
    let mut text = app.edit_buffers.get(key).cloned().unwrap_or_else(|| current.to_string());
    let edit = if multiline {
        egui::TextEdit::multiline(&mut text).desired_rows(1).return_key(egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Enter))
    } else {
        egui::TextEdit::singleline(&mut text)
    };
    let r = ui.add(edit.desired_width(width));
    if r.changed() {
        app.edit_buffers.insert(key, text.clone());
    }
    // Return alone finishes a multiline box too.
    if multiline && r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.command) {
        r.surrender_focus();
    }
    let mut out = None;
    if r.lost_focus() {
        if text != current {
            out = Some(text);
        }
        app.edit_buffers.remove(key);
    }
    out
}

/// Guitar Pro's "More ▾" / "Less ▴" button (the UI font has no triangles, so they are drawn).
fn more_button(ui: &mut egui::Ui, more: bool) -> egui::Response {
    let t = tokens();
    let font = egui::FontId::proportional(14.0);
    let galley = ui.painter().layout_no_wrap(if more { "Less" } else { "More" }.into(), font, t.text);
    let size = egui::vec2(galley.size().x + 28.0, 24.0);
    let (r, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(r, 5.0, if resp.hovered() { t.control_hover } else { t.control });
    p.galley(egui::pos2(r.left() + 9.0, r.center().y - galley.size().y / 2.0), galley, t.text);
    let c = egui::pos2(r.right() - 12.0, r.center().y);
    let (tip, base) = if more { (-3.5, 2.5) } else { (3.5, -2.5) };
    p.add(egui::Shape::convex_polygon(vec![c + egui::vec2(-4.5, base), c + egui::vec2(4.5, base), c + egui::vec2(0.0, tip)], t.text, egui::Stroke::NONE));
    resp
}

/// A labelled song field: "Title:" over its box.
fn song_field(app: &mut GuitarioApp, ui: &mut egui::Ui, label: &str, key: &'static str, current: &str, actions: &mut Vec<(&'static str, Value)>) {
    ui.label(egui::RichText::new(format!("{label}:")).size(13.0));
    if let Some(text) = text_box(app, ui, key, current, f32::INFINITY, true) {
        actions.push(("score.setMeta", json!({ key: text })));
    }
    ui.add_space(4.0);
}

fn song(app: &mut GuitarioApp, ui: &mut egui::Ui, actions: &mut Vec<(&'static str, Value)>) {
    let score = app.session.score().clone();
    let m = &score.meta;
    let more_id = egui::Id::new("inspector-song-more");
    let more = ui.data(|d| d.get_temp::<bool>(more_id)).unwrap_or(false);
    section(ui, "Information", |ui| {
        song_field(app, ui, "Title", "title", &m.title, actions);
        song_field(app, ui, "Artist", "artist", &m.artist, actions);
        if more {
            for (label, key, value) in [
                ("Subtitle", "subtitle", &m.subtitle),
                ("Album", "album", &m.album),
                ("Words", "words", &m.words),
                ("Music", "music", &m.music),
                ("Copyright", "copyright", &m.copyright),
                ("Transcriber", "author", &m.author),
                ("Notice", "notes", &m.notes),
                ("Instructions", "instructions", &m.instructions),
            ] {
                song_field(app, ui, label, key, value, actions);
            }
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Ctrl+Return for new line.").size(12.0).color(tokens().text_dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if more_button(ui, more).clicked() {
                    ui.data_mut(|d| d.insert_temp(more_id, !more));
                }
            });
        });
    });
    section(ui, "Musical notation", |ui| {
        egui::Grid::new("song-notation").num_columns(2).spacing([8.0, 8.0]).show(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label("Concert tone:"));
            let mut concert = false;
            ui.add_enabled(false, egui::Checkbox::new(&mut concert, "Off")).on_disabled_hover_text("No track transposes for reading (not in Guitario yet)");
            ui.end_row();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label("Stylesheet:"));
            if ui.button("Open…").on_hover_text("Page format (the rest of the stylesheet is not in Guitario yet)").clicked() {
                app.open_page_setup();
            }
            ui.end_row();
        });
    });
}

/// Colours offered for a track: Guitar Pro's eight, then lighter and darker ones.
const SWATCHES: [[u8; 3]; 16] = [
    [0xe0, 0x4f, 0x4f],
    [0x3f, 0x9b, 0xe8],
    [0x5c, 0xbd, 0x5c],
    [0xe8, 0xa2, 0x3a],
    [0xa8, 0x6b, 0xe0],
    [0x3c, 0xc4, 0xb8],
    [0xe0, 0x6b, 0xb5],
    [0xc4, 0xc4, 0x48],
    [0xf0, 0x90, 0x80],
    [0x8c, 0xc8, 0xf0],
    [0xa8, 0xe0, 0x90],
    [0xf0, 0xd0, 0x80],
    [0x80, 0x30, 0x30],
    [0x30, 0x50, 0x90],
    [0x30, 0x70, 0x40],
    [0x90, 0x90, 0x98],
];

/// Guitar Pro's track information row: colour, instrument icon, name and short name.
fn track_info(app: &mut GuitarioApp, ui: &mut egui::Ui, score: &guitario_score::Score, ti: usize, actions: &mut Vec<(&'static str, Value)>) {
    let Some(tr) = score.tracks.get(ti) else { return };
    let t = tokens();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let (r, resp) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
        ui.painter().rect_filled(r, egui::CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 }, if resp.hovered() { t.control_hover } else { t.control });
        ui.painter().rect_filled(r.shrink(6.0), 2.0, track_color(score, ti));
        let resp = resp.on_hover_text("Track colour");
        egui::Popup::menu(&resp).show(|ui| {
            egui::Grid::new("track-swatches").spacing([3.0, 3.0]).show(ui, |ui| {
                for (k, c) in SWATCHES.iter().enumerate() {
                    let (r, sr) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                    ui.painter().rect_filled(r, 3.0, Color32::from_rgb(c[0], c[1], c[2]));
                    if tr.color_at(ti) == *c {
                        ui.painter().rect_stroke(r, 3.0, egui::Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Inside);
                    }
                    if sr.clicked() {
                        actions.push(("track.setInfo", json!({"track": ti, "color": format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])})));
                        ui.close();
                    }
                    if k % 8 == 7 {
                        ui.end_row();
                    }
                }
            });
            if ui.add_enabled(tr.color.is_some(), egui::Button::new("Default colour")).clicked() {
                actions.push(("track.setInfo", json!({"track": ti, "color": null})));
                ui.close();
            }
        });
        let (r, _) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::hover());
        ui.painter().rect_filled(r, egui::CornerRadius { nw: 0, sw: 0, ne: 5, se: 5 }, t.control);
        crate::icons::draw(ui.painter(), r.shrink(5.0), crate::icons::instrument(tr), t.text);
        let short_w = 64.0;
        let name_w = (ui.available_width() - short_w - 8.0).max(60.0);
        if let Some(name) = text_box(app, ui, "track.name", &tr.name, name_w, false)
            && !name.trim().is_empty()
        {
            actions.push(("track.setInfo", json!({"track": ti, "name": name})));
        }
        let shown = crate::page::short_name(tr);
        if let Some(short) = text_box(app, ui, "track.short", &shown, short_w, false) {
            actions.push(("track.setInfo", json!({"track": ti, "shortName": short})));
        }
    });
}

fn track(app: &mut GuitarioApp, ui: &mut egui::Ui, actions: &mut Vec<(&'static str, Value)>) {
    let score = app.session.score().clone();
    let c = app.session.cursor();
    let Some(tr) = score.tracks.get(c.track) else { return };
    section(ui, "Information", |ui| track_info(app, ui, &score, c.track, actions));
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
        if tr.is_drums() {
            ui.label("Drum kit (General MIDI)");
        } else {
            egui::ComboBox::from_id_salt("program").width(ui.available_width()).selected_text(format!("1. {}", program_name(tr.program))).show_ui(ui, |ui| {
                for (p, name) in PROGRAMS {
                    if ui.selectable_label(tr.program == p, name).clicked() {
                        actions.push(("track.setProgram", json!({"program": p})));
                    }
                }
            });
        }
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
}
