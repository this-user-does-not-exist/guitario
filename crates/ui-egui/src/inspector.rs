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

/// The sounds the instrument lists offer: a General MIDI program, how many play it (0 is one
/// player) and its name. The fretted instruments play on the plucked-string synth; the grand
/// pianos and the bowed strings are modelled, the violin family alone or as an orchestra's
/// sections.
pub(crate) const SOUNDS: [(u8, u8, &str); 33] = [
    (0, 0, "Grand piano"),
    (1, 0, "Bright piano"),
    (2, 0, "Electric grand"),
    (3, 0, "Honky-tonk piano"),
    (24, 0, "Nylon guitar"),
    (25, 0, "Steel guitar"),
    (26, 0, "Jazz guitar"),
    (27, 0, "Clean guitar"),
    (28, 0, "Muted guitar"),
    (29, 0, "Overdrive guitar"),
    (30, 0, "Distortion guitar"),
    (31, 0, "Guitar harmonics"),
    (32, 0, "Acoustic bass"),
    (33, 0, "Fingered bass"),
    (34, 0, "Picked bass"),
    (35, 0, "Fretless bass"),
    (36, 0, "Slap bass"),
    (40, 0, "Violin"),
    (40, 16, "Violin section"),
    (41, 0, "Viola"),
    (41, 12, "Viola section"),
    (42, 0, "Cello"),
    (42, 10, "Cello section"),
    (43, 0, "Contrabass"),
    (43, 8, "Contrabass section"),
    (44, 0, "Tremolo strings"),
    (45, 0, "Pizzicato strings"),
    (48, 0, "String ensemble"),
    (49, 0, "Slow strings"),
    (104, 0, "Sitar"),
    (105, 0, "Banjo"),
    (106, 0, "Shamisen"),
    (110, 0, "Fiddle"),
];

/// Whether a section can play the program: the violin family's, violin to double bass.
pub(crate) fn sectioned(program: u8) -> bool {
    guitario_score::orchestral_section(program).is_some()
}

/// How many play a track's program as the sound lists count them: 0 for one player.
pub(crate) fn players_of(program: u8, players: u8) -> u8 {
    if sectioned(program) && players > 1 { players } else { 0 }
}

/// The name of a sound as the instrument lists show it; a section of a size they do not offer
/// says how many play.
pub(crate) fn sound_name(program: u8, players: u8) -> String {
    let players = players_of(program, players);
    if let Some(s) = SOUNDS.iter().find(|s| s.0 == program && s.1 == players) {
        return s.2.to_string();
    }
    let base = SOUNDS.iter().find(|s| s.0 == program && s.1 == 0).map_or_else(|| format!("GM program {program}"), |s| s.2.to_string());
    if players > 1 { format!("{base} section of {players}") } else { base }
}

/// The section sizes the Players list offers: one player, a few desks, a symphony orchestra's.
const PLAYER_CHOICES: [u8; 11] = [1, 2, 3, 4, 5, 6, 8, 10, 12, 14, 16];

fn players_label(n: u8) -> String {
    if n <= 1 { "1 (soloist)".to_string() } else { format!("{n} (section)") }
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
    let r = ui.add(edit.desired_width(width).frame(crate::theme::field_frame()));
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
            ui.horizontal(|ui| dim_switch(ui, "No track transposes for reading (not in Guitario yet)"));
            ui.end_row();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label("Stylesheet:"));
            if ui.button("Open…").on_hover_text("Page format (the rest of the stylesheet is not in Guitario yet)").clicked() {
                app.open_stylesheet();
            }
            ui.end_row();
        });
    });
    section(ui, "Sound mastering", |ui| {
        ui.label("Setup the mastering for all tracks");
        ui.add_space(4.0);
        // Guitar Pro's three master effects in their fixed order: a compressor, a reverb and an
        // equalizer, each with its preset list, power and options.
        let master = app.session.score().master.clone();
        crate::effects_ui::mastering_section(app, ui, &master, actions);
        ui.add_space(4.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
            ui.add_enabled(false, egui::Button::new("Show Pedalboard…")).on_disabled_hover_text("The pedalboard (not in Guitario yet)");
        });
    });
}

/// A greyed on/off switch, off, for a setting Guitario does not have yet.
fn dim_switch(ui: &mut egui::Ui, tip: &str) {
    let t = tokens();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(40.0, 22.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 11.0, t.control.gamma_multiply(0.6));
    p.circle_filled(egui::pos2(r.left() + 11.0, r.center().y), 7.0, Color32::WHITE.gamma_multiply(0.6));
    ui.label(egui::RichText::new("Off").color(t.text_dim));
    resp.on_hover_text(tip);
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

/// A notation button's icon.
#[derive(Clone, Copy)]
pub(crate) enum Glyph {
    Tab,
    Standard,
    Slash,
    Numbered,
    SingleStaff,
    GrandStaff,
}

fn draw_glyph(p: &egui::Painter, r: egui::Rect, glyph: Glyph, c: Color32) {
    let at = |x: f32, y: f32| egui::pos2(r.left() + x * r.width(), r.top() + y * r.height());
    let line = egui::Stroke::new(1.2, c);
    match glyph {
        Glyph::Tab => {
            for y in [0.3, 0.7] {
                p.line_segment([at(0.0, y), at(0.3, y)], line);
                p.line_segment([at(0.7, y), at(1.0, y)], line);
            }
            p.text(at(0.5, 0.5), egui::Align2::CENTER_CENTER, "5", crate::fonts::sans_bold(r.height() * 0.8), c);
        }
        Glyph::Standard => {
            for k in 0..5 {
                let y = 0.1 + 0.2 * k as f32;
                p.line_segment([at(0.0, y), at(1.0, y)], egui::Stroke::new(0.8, c));
            }
            p.add(egui::Shape::ellipse_filled(at(0.45, 0.7), egui::vec2(r.width() * 0.14, r.height() * 0.11), c));
            p.line_segment([at(0.58, 0.68), at(0.58, 0.0)], line);
            p.line_segment([at(0.58, 0.0), at(0.8, 0.2)], line);
        }
        Glyph::Slash => {
            p.add(egui::Shape::convex_polygon(vec![at(0.1, 0.95), at(0.3, 0.95), at(0.55, 0.65), at(0.35, 0.65)], c, egui::Stroke::NONE));
            p.line_segment([at(0.53, 0.68), at(0.53, 0.0)], line);
            p.line_segment([at(0.0, 0.8), at(1.0, 0.8)], egui::Stroke::new(0.8, c));
        }
        Glyph::Numbered => {
            p.text(at(0.5, 0.6), egui::Align2::CENTER_CENTER, "2", egui::FontId::proportional(r.height() * 0.85), c);
            p.circle_filled(at(0.5, 0.05), 1.3, c);
        }
        Glyph::SingleStaff => {
            p.rect_stroke(egui::Rect::from_min_max(at(0.05, 0.3), at(0.95, 0.7)), 1.0, egui::Stroke::new(1.5, c), egui::StrokeKind::Middle);
        }
        Glyph::GrandStaff => {
            p.text(at(0.15, 0.5), egui::Align2::CENTER_CENTER, "{", egui::FontId::proportional(r.height() * 1.1), c);
            for (y0, y1) in [(0.05, 0.42), (0.58, 0.95)] {
                p.rect_stroke(egui::Rect::from_min_max(at(0.4, y0), at(0.95, y1)), 1.0, egui::Stroke::new(1.2, c), egui::StrokeKind::Middle);
            }
        }
    }
}

/// Buttons side by side in one rounded group, as Guitar Pro groups them: (icon, lit, enabled, tip).
/// Returns the index of the one clicked.
pub(crate) fn button_group(ui: &mut egui::Ui, id: &str, buttons: &[(Glyph, bool, bool, &str)]) -> Option<usize> {
    let t = tokens();
    let size = egui::vec2(30.0, 26.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size.x * buttons.len() as f32, size.y), egui::Sense::hover());
    let mut clicked = None;
    let last = buttons.len().saturating_sub(1);
    for (k, (glyph, on, enabled, tip)) in buttons.iter().enumerate() {
        let r = egui::Rect::from_min_size(rect.min + egui::vec2(size.x * k as f32, 0.0), size);
        let sense = if *enabled { egui::Sense::click() } else { egui::Sense::hover() };
        let resp = ui.interact(r, ui.id().with((id, k)), sense).on_hover_text(*tip);
        let corners = egui::CornerRadius {
            nw: if k == 0 { 5 } else { 0 },
            sw: if k == 0 { 5 } else { 0 },
            ne: if k == last { 5 } else { 0 },
            se: if k == last { 5 } else { 0 },
        };
        let fill = if *on {
            t.accent
        } else if resp.hovered() && *enabled {
            t.control_hover
        } else {
            t.control
        };
        let p = ui.painter();
        p.rect_filled(r, corners, fill);
        if k > 0 {
            p.line_segment([r.left_top(), r.left_bottom()], egui::Stroke::new(1.0, t.panel));
        }
        let ink = if !*enabled {
            t.text_dim.gamma_multiply(0.5)
        } else if *on {
            Color32::WHITE
        } else {
            t.text
        };
        draw_glyph(p, r.shrink2(egui::vec2(7.0, 6.0)), *glyph, ink);
        if resp.clicked() {
            clicked = Some(k);
        }
    }
    clicked
}

/// Guitar Pro's Musical Notation section: the notations and staff, the tuning, and under More the
/// capo and transposition.
fn notation(ui: &mut egui::Ui, tr: &guitario_score::Track, ti: usize, actions: &mut Vec<(&'static str, Value)>) {
    let n = tr.notation;
    let drums = tr.is_drums();
    let later = " (not in Guitario yet)";
    let label = |ui: &mut egui::Ui, text: &str| {
        ui.allocate_ui_with_layout(egui::vec2(72.0, 26.0), egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(text));
    };
    ui.horizontal(|ui| {
        label(ui, "Notation:");
        let tab_tip = if drums { format!("Drum tablature{later}") } else { "Tablature".to_string() };
        let slash_tip = format!("Slash notation{later}");
        let numbered_tip = format!("Numbered notation (jianpu){later}");
        let buttons = [
            (Glyph::Tab, n.tab && !drums, !drums, tab_tip.as_str()),
            (Glyph::Standard, n.standard || drums, !drums, "Standard notation"),
            (Glyph::Slash, n.slash, false, slash_tip.as_str()),
            (Glyph::Numbered, n.numbered, false, numbered_tip.as_str()),
        ];
        match button_group(ui, "notations", &buttons) {
            Some(0) => actions.push(("track.setNotation", json!({"track": ti, "tab": !n.tab}))),
            Some(1) => actions.push(("track.setNotation", json!({"track": ti, "standard": !n.standard}))),
            _ => {}
        }
        ui.add_space(10.0);
        let grand_tip = format!("Two staves (grand staff){later}");
        button_group(ui, "staves", &[(Glyph::SingleStaff, true, true, "One staff"), (Glyph::GrandStaff, false, false, grand_tip.as_str())]);
    });
    if drums {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new("Type a General MIDI number (36 kick, 38 snare, 42 hi-hat, 49 crash) or click the drum pad.").small().color(tokens().text_dim),
        );
        return;
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        label(ui, "Tuning:");
        let names: Vec<&str> = tr.tuning.strings.iter().rev().map(|p| guitario_theory::note_name(*p)).collect();
        let t = tokens();
        // The two cells butt together; the gap between them must come out of the width too, or the row
        // overflows every frame and the (resizable) panel widens until it fills the window.
        ui.spacing_mut().item_spacing.x = 1.0;
        let w = (ui.available_width() - 28.0 - ui.spacing().item_spacing.x).max(80.0);
        let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 26.0), egui::Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(r, egui::CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 }, if resp.hovered() { t.control_hover } else { t.control });
        crate::icons::draw(
            &p,
            egui::Rect::from_center_size(egui::pos2(r.left() + 14.0, r.center().y), egui::vec2(14.0, 14.0)),
            crate::icons::Icon::Gear,
            t.text,
        );
        p.text(egui::pos2(r.left() + 28.0, r.center().y), egui::Align2::LEFT_CENTER, names.join(" "), egui::FontId::proportional(14.0), t.text);
        let resp = resp.on_hover_text(format!("{} — click to change", tr.tuning.name));
        egui::Popup::menu(&resp).show(|ui| {
            for preset in Tuning::presets() {
                if ui.selectable_label(preset.strings == tr.tuning.strings, &preset.name).clicked() {
                    actions.push(("track.setTuning", json!({"preset": preset.name})));
                    ui.close();
                }
            }
        });
        let (pr, presp) = ui.allocate_exact_size(egui::vec2(28.0, 26.0), egui::Sense::hover());
        p.rect_filled(pr, egui::CornerRadius { nw: 0, sw: 0, ne: 5, se: 5 }, t.control);
        let c = pr.center();
        p.add(egui::Shape::convex_polygon(
            vec![c + egui::vec2(-4.0, -6.0), c + egui::vec2(6.0, 0.0), c + egui::vec2(-4.0, 6.0)],
            t.text_dim.gamma_multiply(0.5),
            egui::Stroke::NONE,
        ));
        presp.on_hover_text(format!("Play the open strings{later}"));
    });
    let more_id = egui::Id::new("inspector-notation-more");
    let more = ui.data(|d| d.get_temp::<bool>(more_id)).unwrap_or(false);
    if more {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            label(ui, "Capo:");
            let mut capo = i64::from(tr.capo);
            if ui.add(egui::DragValue::new(&mut capo).range(0..=24)).changed() {
                actions.push(("track.setCapo", json!({"fret": capo})));
            }
        });
        ui.label("Transposition tonality");
        ui.horizontal(|ui| {
            label(ui, "C played as:");
            ui.add_enabled(false, egui::Button::new("C")).on_disabled_hover_text(format!("Transposing instruments{later}"));
        });
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
        if more_button(ui, more).clicked() {
            ui.data_mut(|d| d.insert_temp(more_id, !more));
        }
    });
}

/// Guitar Pro's slider for 0 to 100: a thin dark track, the travelled part in the accent colour,
/// a white knob. Click or drag to set it.
fn slider(ui: &mut egui::Ui, value: &mut f32) -> egui::Response {
    let t = tokens();
    let w = ui.available_width().max(60.0);
    let (r, mut resp) = ui.allocate_exact_size(egui::vec2(w, 22.0), egui::Sense::click_and_drag());
    let (x0, x1) = (r.left() + 8.0, r.right() - 8.0);
    if (resp.dragged() || resp.clicked())
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let v = ((pos.x - x0) / (x1 - x0).max(1.0)).clamp(0.0, 1.0) * 100.0;
        if (v - *value).abs() > f32::EPSILON {
            *value = v;
            resp.mark_changed();
        }
    }
    let p = ui.painter();
    let y = r.center().y;
    let knob = x0 + (x1 - x0) * (*value / 100.0).clamp(0.0, 1.0);
    p.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, y - 2.0), egui::pos2(x1, y + 2.0)), 2.0, t.lcd_bg);
    p.rect_filled(egui::Rect::from_min_max(egui::pos2(x0, y - 2.0), egui::pos2(knob, y + 2.0)), 2.0, t.accent);
    p.circle_filled(egui::pos2(knob, y), 8.0, Color32::WHITE);
    resp
}

/// Guitar Pro's on/off switch with its word beside it. Returns true when clicked.
pub(crate) fn switch(ui: &mut egui::Ui, on: bool) -> bool {
    let t = tokens();
    let (r, resp) = ui.allocate_exact_size(egui::vec2(40.0, 22.0), egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(r, 11.0, if on { t.accent } else { t.control });
    let knob = if on { r.right() - 11.0 } else { r.left() + 11.0 };
    p.circle_filled(egui::pos2(knob, r.center().y), 7.0, Color32::WHITE);
    ui.label(if on { "On" } else { "Off" });
    resp.clicked()
}

/// Guitar Pro's Interpretation section: playing style, palm mute and accentuation, then automatic
/// let ring, automatic brush and stringed instrument.
fn interpretation(ui: &mut egui::Ui, tr: &guitario_score::Track, ti: usize, actions: &mut Vec<(&'static str, Value)>) {
    use guitario_score::PlayingStyle as P;
    let i = tr.interpretation;
    let label = |ui: &mut egui::Ui, text: &str| {
        ui.allocate_ui_with_layout(egui::vec2(104.0, 22.0), egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(text));
    };
    let styles = [(P::Pick, "pick", "Pick"), (P::Fingers, "fingers", "Fingers"), (P::Picking, "picking", "Picking"), (P::BassSlap, "bassSlap", "Bass Slap")];
    ui.horizontal(|ui| {
        label(ui, "Playing style:");
        // The instrument's own style is a pick.
        let shown = styles.iter().find(|s| s.0 == i.style).map_or("Pick", |s| s.2);
        egui::ComboBox::from_id_salt("playing-style").width(110.0).selected_text(shown).show_ui(ui, |ui| {
            for (style, id, name) in styles {
                if ui.selectable_label(i.style == style || (i.style == P::Default && style == P::Pick), name).clicked() {
                    actions.push(("track.setInterpretation", json!({"track": ti, "style": id})));
                }
            }
        });
    });
    for (name, key, value) in [("Palm mute:", "palmMute", i.palm_mute), ("Accentuation:", "accentuation", i.accentuation)] {
        ui.horizontal(|ui| {
            label(ui, name);
            // Dragging shows the value live; letting go makes one undo step.
            let id = ui.id().with(("interpretation", key, ti));
            let mut v: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(f32::from(value));
            let resp = slider(ui, &mut v).on_hover_text(format!("{}%", v.round()));
            if resp.dragged() || resp.changed() {
                ui.data_mut(|d| d.insert_temp(id, v));
            }
            if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
                ui.data_mut(|d| d.remove::<f32>(id));
                let n = v.round().clamp(0.0, 100.0) as u8;
                if n != value {
                    actions.push(("track.setInterpretation", json!({"track": ti, key: n})));
                }
            }
        });
    }
    for (name, key, on) in
        [("Auto let ring:", "autoLetRing", i.auto_let_ring), ("Auto brush:", "autoBrush", i.auto_brush), ("Stringed:", "stringed", i.stringed)]
    {
        ui.horizontal(|ui| {
            label(ui, name);
            if switch(ui, on) {
                actions.push(("track.setInterpretation", json!({"track": ti, key: !on})));
            }
        });
    }
}

/// Guitar Pro's Sounds section: the audio engine, then the track's sound with its options.
fn sounds(app: &mut GuitarioApp, ui: &mut egui::Ui, tr: &guitario_score::Track, ti: usize, actions: &mut Vec<(&'static str, Value)>) {
    use guitario_score::SoundEngine;
    let t = tokens();
    let later = " (not in Guitario yet)";
    ui.horizontal(|ui| {
        // RSE or MIDI, side by side.
        for (k, (engine, name, id, tip)) in [
            (SoundEngine::Rse, "RSE", "rse", "Realistic sound: the sampled sound bank (the synth where it has no such instrument)"),
            (SoundEngine::Midi, "MIDI", "midi", "The lighter synthesised sound"),
        ]
        .into_iter()
        .enumerate()
        {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(50.0, 26.0), egui::Sense::click());
            let on = tr.engine == engine;
            let corners = if k == 0 { egui::CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 } } else { egui::CornerRadius { nw: 0, sw: 0, ne: 5, se: 5 } };
            let p = ui.painter();
            p.rect_filled(
                r,
                corners,
                if on {
                    t.accent
                } else if resp.hovered() {
                    t.control_hover
                } else {
                    t.control
                },
            );
            // Guitar Pro prints the engine's name in a little white plate.
            let plate = egui::Rect::from_center_size(r.center(), egui::vec2(36.0, 16.0));
            p.rect_filled(plate, 3.0, Color32::WHITE);
            p.text(plate.center(), egui::Align2::CENTER_CENTER, name, crate::fonts::sans_bold(12.0), if on { t.accent } else { t.control });
            if resp.on_hover_text(tip).clicked() && !on {
                actions.push(("track.setEngine", json!({"engine": id, "tracks": [ti]})));
            }
            ui.add_space(-ui.spacing().item_spacing.x);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_enabled(false, egui::Button::new(egui::RichText::new("A").strong()).min_size(egui::vec2(26.0, 26.0)))
                .on_disabled_hover_text(format!("Sound automations{later}"));
            ui.add_enabled(false, egui::Button::new(egui::RichText::new("+").strong()).min_size(egui::vec2(26.0, 26.0)))
                .on_disabled_hover_text(format!("Several sounds on one track{later}"));
        });
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        let w = (ui.available_width() - 2.0 * 27.0 - 2.0).max(80.0);
        let name = if tr.is_drums() { "Drum kit".to_string() } else { sound_name(tr.program, tr.players) };
        let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 30.0), egui::Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(r, egui::CornerRadius { nw: 5, sw: 5, ne: 0, se: 0 }, if resp.hovered() && !tr.is_drums() { t.control_hover } else { t.control });
        p.text(egui::pos2(r.left() + 10.0, r.center().y), egui::Align2::LEFT_CENTER, format!("1. {name}"), egui::FontId::proportional(14.0), t.text);
        // Up and down chevrons: the sound list.
        let cx = egui::pos2(r.right() - 14.0, r.center().y);
        for (dy, tip) in [(-4.0, -1.0), (4.0, 1.0)] {
            p.add(egui::Shape::line(
                vec![cx + egui::vec2(-4.0, dy - 2.0 * tip), cx + egui::vec2(0.0, dy + 2.0 * tip), cx + egui::vec2(4.0, dy - 2.0 * tip)],
                egui::Stroke::new(1.4, t.text),
            ));
        }
        if !tr.is_drums() {
            egui::Popup::menu(&resp).show(|ui| {
                let current = (tr.program, players_of(tr.program, tr.players));
                for (program, players, label) in SOUNDS {
                    if ui.selectable_label(current == (program, players), label).clicked() {
                        actions.push(("track.setProgram", json!({"track": ti, "program": program, "players": players})));
                        ui.close();
                    }
                }
            });
        }
        let (mr, mresp) = ui.allocate_exact_size(egui::vec2(27.0, 30.0), egui::Sense::click());
        p.rect_filled(mr, 0.0, if mresp.hovered() { t.control_hover } else { t.control });
        crate::icons::draw(&p, mr.shrink(7.0), crate::icons::Icon::More, t.text);
        egui::Popup::menu(&mresp.on_hover_text("Options")).show(|ui| {
            if ui.button("Mixer…").clicked() {
                ui.close();
                app.ui_action(crate::menu_catalog::UiAction::Mixer);
            }
            if ui.button("Audio Settings…").clicked() {
                ui.close();
                app.ui_action(crate::menu_catalog::UiAction::AudioSettings);
            }
        });
        let (sr, sresp) = ui.allocate_exact_size(egui::vec2(27.0, 30.0), egui::Sense::hover());
        p.rect_filled(sr, egui::CornerRadius { nw: 0, sw: 0, ne: 5, se: 5 }, t.control);
        speaker(&p, sr.shrink(7.0), t.text_dim.gamma_multiply(0.6));
        sresp.on_hover_text(format!("Lock this sound over sound automations{later}"));
    });
    ui.add_space(4.0);
    if sectioned(tr.program) {
        players_row(ui, tr, ti, actions);
    }
    bank_row(app, ui, tr, ti, actions);
}

/// How many play a bowed string track: a soloist or a section, up to an orchestra's sixteen.
fn players_row(ui: &mut egui::Ui, tr: &guitario_score::Track, ti: usize, actions: &mut Vec<(&'static str, Value)>) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Players").color(tokens().text_dim));
        let current = tr.players.max(1);
        let resp = ui.add(egui::Button::new(egui::RichText::new(players_label(current)).size(13.0)).min_size(egui::vec2(ui.available_width(), 24.0)));
        let resp = resp.on_hover_text("One player, or a section: each player bows with their own tuning, timing and vibrato on their own instrument");
        egui::Popup::menu(&resp).show(|ui| {
            for n in PLAYER_CHOICES {
                if ui.selectable_label(current == n, players_label(n)).clicked() {
                    actions.push(("track.setPlayers", json!({"track": ti, "players": n})));
                    ui.close();
                }
            }
        });
    });
    ui.add_space(4.0);
}

/// What a track plays from the sound bank, and the choice of it: its instrument's role, another
/// role, or an `.sfz` file.
fn bank_row(app: &mut GuitarioApp, ui: &mut egui::Ui, tr: &guitario_score::Track, ti: usize, actions: &mut Vec<(&'static str, Value)>) {
    use crate::bank_window::role_label;
    use guitario_playback::Role;
    let bank = app.sounds.clone();
    // What a role plays: the library's (short) name, or the synth where the bank has none.
    let source = |role: Role| {
        bank.as_ref().and_then(|b| b.path_for(role)).map_or_else(
            || "built-in synth".to_string(),
            |p| {
                let lib = guitario_playback::install::LIBRARIES.iter().find(|l| l.role == role && bank.as_ref().is_some_and(|b| p.starts_with(l.dir(b.dir()))));
                lib.map_or_else(
                    || p.file_stem().unwrap_or_default().to_string_lossy().into_owned(),
                    |l| l.title.split(" (").next().unwrap_or(l.title).to_string(),
                )
            },
        )
    };
    let shown = match (tr.engine, tr.bank.as_deref()) {
        (guitario_score::SoundEngine::Midi, _) => "Synthesiser (MIDI engine)".to_string(),
        (_, Some(name)) => match Role::from_key(name) {
            Some(role) => format!("{}: {}", role_label(role), source(role)),
            None => std::path::Path::new(name).file_name().map_or_else(|| name.to_string(), |f| f.to_string_lossy().into_owned()),
        },
        (_, None) => Role::of(tr).map_or_else(|| "Built-in synth".to_string(), |role| format!("{}: {}", role_label(role), source(role))),
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Bank").color(tokens().text_dim));
        let rse = tr.engine == guitario_score::SoundEngine::Rse;
        let resp = ui.add_enabled(rse, egui::Button::new(egui::RichText::new(shown).size(13.0)).truncate().min_size(egui::vec2(ui.available_width(), 24.0)));
        let resp = resp
            .on_hover_text("The recorded instrument this track plays; Automatic follows its instrument")
            .on_disabled_hover_text("The MIDI engine plays the synthesiser");
        egui::Popup::menu(&resp).show(|ui| {
            if ui.selectable_label(tr.bank.is_none(), "Automatic (by instrument)").clicked() {
                actions.push(("track.setBank", json!({"track": ti, "bank": null})));
                ui.close();
            }
            for role in Role::ALL {
                let text = format!("{}: {}", role_label(role), source(role));
                if ui.selectable_label(tr.bank.as_deref() == Some(role.key()), text).clicked() {
                    actions.push(("track.setBank", json!({"track": ti, "bank": role.key()})));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Choose an SFZ File…").clicked() {
                ui.close();
                if let Some(p) = rfd::FileDialog::new().add_filter("SFZ instrument", &["sfz"]).pick_file() {
                    actions.push(("track.setBank", json!({"track": ti, "bank": p.display().to_string()})));
                }
            }
            if ui.button("Sound Bank…").clicked() {
                ui.close();
                app.open_sound_bank();
            }
        });
    });
}

/// A loudspeaker with sound waves.
fn speaker(p: &egui::Painter, r: egui::Rect, c: Color32) {
    let at = |x: f32, y: f32| egui::pos2(r.left() + x * r.width(), r.top() + y * r.height());
    p.add(egui::Shape::convex_polygon(vec![at(0.0, 0.35), at(0.2, 0.35), at(0.45, 0.1), at(0.45, 0.9), at(0.2, 0.65), at(0.0, 0.65)], c, egui::Stroke::NONE));
    for rad in [0.18, 0.32] {
        let pts: Vec<_> = (0..=8).map(|i| -0.9 + 1.8 * i as f32 / 8.0).map(|a: f32| at(0.45 + rad * a.cos(), 0.5 + rad * 1.6 * a.sin())).collect();
        p.add(egui::Shape::line(pts, egui::Stroke::new(1.3, c)));
    }
}

fn track(app: &mut GuitarioApp, ui: &mut egui::Ui, actions: &mut Vec<(&'static str, Value)>) {
    let score = app.session.score().clone();
    let c = app.session.cursor();
    let Some(tr) = score.tracks.get(c.track) else { return };
    section(ui, "Information", |ui| track_info(app, ui, &score, c.track, actions));
    section(ui, "Musical notation", |ui| notation(ui, tr, c.track, actions));
    section(ui, "Sounds", |ui| {
        sounds(app, ui, tr, c.track, actions);
        ui.add_space(6.0);
        crate::effects_ui::chain_section(app, ui, tr, c.track, actions);
    });
    if !tr.is_drums() {
        section(ui, "Interpretation", |ui| interpretation(ui, tr, c.track, actions));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_name_sections_and_their_sizes() {
        assert_eq!(sound_name(40, 0), "Violin");
        assert_eq!(sound_name(40, 1), "Violin");
        assert_eq!(sound_name(40, 16), "Violin section");
        assert_eq!(sound_name(42, 6), "Cello section of 6");
        assert_eq!(sound_name(25, 16), "Steel guitar", "a guitar is one player whatever it says");
        assert_eq!(sound_name(99, 0), "GM program 99");
        // Every section offered is one of the violin family's, at a size a track may have.
        for (program, players, _) in SOUNDS {
            assert!(players == 0 || (sectioned(program) && players <= guitario_score::MAX_PLAYERS && PLAYER_CHOICES.contains(&players)), "{program} {players}");
        }
        assert!(PLAYER_CHOICES.iter().all(|&n| n <= guitario_score::MAX_PLAYERS));
    }
}
