//! The global view along the bottom, in Guitar Pro 8's columns: a header (add track, track menu,
//! move up/down, multitrack, mute, solo, Vol., Pan., Eq.), one row per track (colour, instrument
//! icon, number and name, multitrack eye, mute, solo, volume fader, volume automation, pan knob,
//! pan automation, equalizer) and a Master row; beside them a bar grid in the track colours, with
//! bar numbers and a marker above and the sections along the Master row.

use crate::GuitarioApp;
use crate::automation_dialog::MixKind;
use crate::icons::{Icon, draw};
use crate::mixer::Control;
use crate::page::is_hidden;
use crate::theme::{tokens, track_color};
use egui::{Color32, CornerRadius, Rect, Sense, Shape, Stroke, pos2, vec2};
use serde_json::json;

const ROW: f32 = 28.0;
const HEAD: f32 = 28.0;
const LEFT: f32 = 500.0;
const CELL: f32 = 26.0;
const FADER: f32 = 120.0;

/// Where each column sits, measured from the right edge of the track list.
mod col {
    pub const EQ: f32 = 18.0;
    pub const PAN_A: f32 = 46.0;
    pub const PAN: f32 = 74.0;
    pub const VOL_A: f32 = 104.0;
    /// The fader's right end.
    pub const FADER_END: f32 = 120.0;
    pub const SOLO: f32 = super::FADER + FADER_END + 22.0;
    pub const MUTE: f32 = SOLO + 26.0;
    pub const EYE: f32 = MUTE + 30.0;
}

/// Colours Guitar Pro gives mute (yellow-orange) and solo (red), and an enabled automation (green).
const MUTE_ON: Color32 = Color32::from_rgb(0xf2, 0xb2, 0x5c);
const SOLO_ON: Color32 = Color32::from_rgb(0xe8, 0x55, 0x4e);
const AUTO_ON: Color32 = Color32::from_rgb(0x9c, 0xc8, 0x78);

/// What a click in the global view asks for, done after drawing.
enum Act {
    Run(&'static str, serde_json::Value),
    ToggleHidden(usize),
    ToggleMultitrack,
    Automation(usize, MixKind),
}

/// Whether a track's volume or pan is automated anywhere.
fn automated(tr: &guitario_score::Track, kind: MixKind) -> bool {
    tr.measures.iter().any(|m| match kind {
        MixKind::Volume => m.volume.is_some(),
        MixKind::Pan => m.pan.is_some(),
    })
}

/// Bar numbers in the grid's header: the first bar and every fourth (1, 4, 8, 12 …), as Guitar
/// Pro labels them.
fn numbered(bar: usize) -> bool {
    bar == 1 || bar.is_multiple_of(4)
}

/// Guitar Pro's fader: a dark groove with a light handle, the last stretch set apart.
fn fader(p: &egui::Painter, slot: Rect, value: f32, enabled: bool, automated: bool) {
    let t = tokens();
    let groove = Rect::from_center_size(slot.center(), vec2(slot.width(), 12.0));
    let split = groove.right() - 16.0;
    let dark = Color32::from_rgb(0x10, 0x11, 0x14);
    p.rect_filled(Rect::from_min_max(groove.min, pos2(split - 2.0, groove.bottom())), 1.0, dark);
    p.rect_filled(Rect::from_min_max(pos2(split, groove.top()), groove.max), 1.0, dark);
    if automated {
        p.rect_stroke(groove.expand(2.0), 3.0, Stroke::new(1.5, AUTO_ON), egui::StrokeKind::Outside);
    }
    let x = groove.left() + (groove.width() - 6.0) * (value / 127.0).clamp(0.0, 1.0) + 3.0;
    let handle = Rect::from_center_size(pos2(x, groove.center().y), vec2(6.0, 18.0));
    p.rect_filled(handle, 1.0, if enabled { Color32::from_gray(0xe8) } else { t.text_dim.gamma_multiply(0.5) });
    p.rect_stroke(handle, 1.0, Stroke::new(1.0, Color32::from_black_alpha(160)), egui::StrokeKind::Outside);
}

/// A pan knob: a ring, open at the bottom, and a pointer from centre to the value (-63..=63).
fn knob(p: &egui::Painter, c: egui::Pos2, pan: f32, enabled: bool) {
    let t = tokens();
    let ink = if enabled { t.text } else { t.text_dim.gamma_multiply(0.5) };
    let r = 8.5;
    let (a0, a1) = (0.75 * std::f32::consts::PI, 2.25 * std::f32::consts::PI);
    let ring: Vec<_> = (0..=24)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / 24.0;
            c + vec2(r * a.cos(), r * a.sin())
        })
        .collect();
    p.add(Shape::line(ring, Stroke::new(1.6, ink)));
    p.circle_filled(c, r - 2.0, Color32::from_rgb(0x10, 0x11, 0x14));
    let a = 1.5 * std::f32::consts::PI + (pan / 63.0).clamp(-1.0, 1.0) * 0.75 * std::f32::consts::PI;
    p.line_segment([c + vec2(2.0 * a.cos(), 2.0 * a.sin()), c + vec2((r - 1.0) * a.cos(), (r - 1.0) * a.sin())], Stroke::new(2.0, ink));
}

/// An "A" automation button: green when the track has that automation.
fn auto_badge(p: &egui::Painter, c: egui::Pos2, on: bool, hovered: bool, enabled: bool) {
    let t = tokens();
    let r = Rect::from_center_size(c, vec2(16.0, 18.0));
    if on {
        p.rect_filled(r, 3.0, AUTO_ON);
    } else if hovered {
        p.rect_filled(r, 3.0, t.control_hover);
    }
    let ink = if on {
        Color32::from_rgb(0x18, 0x22, 0x10)
    } else if enabled {
        Color32::from_gray(0x16)
    } else {
        t.text_dim.gamma_multiply(0.35)
    };
    p.text(c, egui::Align2::CENTER_CENTER, "A", crate::fonts::sans_bold(15.0), if enabled && !on && hovered { t.text } else { ink });
}

/// A small icon cell: `on` fills it in `fill` with dark ink, as Guitar Pro's mute and solo do.
fn icon_cell(p: &egui::Painter, c: egui::Pos2, icon: Icon, on: bool, fill: Color32, ink: Color32, hovered: bool) {
    let t = tokens();
    let r = Rect::from_center_size(c, vec2(22.0, 20.0));
    if on {
        p.rect_filled(r, 3.0, fill);
    } else if hovered {
        p.rect_filled(r, 3.0, t.control_hover);
    }
    draw(p, Rect::from_center_size(c, vec2(15.0, 15.0)), icon, if on { Color32::from_gray(0x22) } else { ink });
}

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let t = tokens();
    let score = app.session.score().clone();
    let c = app.session.cursor();
    let mut acts: Vec<Act> = Vec::new();
    let mut vol_slots: Vec<(usize, Rect)> = Vec::new();
    let mut pan_knobs: Vec<(usize, Rect)> = Vec::new();
    let bars = score.headers.len();
    let n = score.tracks.len();
    let dim = t.text_dim.gamma_multiply(0.55);
    let live = |i: usize, control: Control, stored: i64| match app.mixer.drag {
        Some((ti, ctl, v)) if ti == i && ctl == control => v,
        _ => stored,
    };
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        // Track list.
        ui.vertical(|ui| {
            ui.set_width(LEFT);
            let (head, _) = ui.allocate_exact_size(vec2(LEFT, HEAD), Sense::hover());
            let p = ui.painter().clone();
            p.rect_filled(head, 0.0, t.chrome_dark);
            let right = head.right();
            let cy = head.center().y;
            let hit = |ui: &mut egui::Ui, x: f32, w: f32, id: &str, tip: &str| {
                ui.interact(Rect::from_center_size(pos2(x, cy), vec2(w, 22.0)), egui::Id::new(("global-head", id)), Sense::click()).on_hover_text(tip)
            };
            let add = hit(ui, head.left() + 18.0, 24.0, "add", "Add a track");
            if add.hovered() {
                p.rect_filled(add.rect, 3.0, t.control_hover);
            }
            p.text(add.rect.center(), egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(20.0), t.text);
            egui::Popup::menu(&add).show(|ui| {
                for (label, id, params) in [
                    ("Guitar", "track.add", json!({"name": "Guitar"})),
                    ("Bass", "track.add", json!({"name": "Bass", "tuning": "bass"})),
                    ("Drums", "track.addDrums", json!({})),
                ] {
                    if ui.button(label).clicked() {
                        acts.push(Act::Run(id, params));
                        ui.close();
                    }
                }
            });
            let label = |x: f32, text: &str, align: egui::Align2| {
                p.text(pos2(x, cy), align, text, egui::FontId::proportional(14.0), t.text);
            };
            label(head.left() + 44.0, "Tracks", egui::Align2::LEFT_CENTER);
            let menu = hit(ui, head.left() + 104.0, 16.0, "menu", "Track options");
            draw(&p, Rect::from_center_size(menu.rect.center(), vec2(14.0, 14.0)), Icon::More, if menu.hovered() { Color32::WHITE } else { t.text });
            egui::Popup::menu(&menu).show(|ui| {
                for (label, id) in [
                    ("Duplicate Track", "track.duplicate"),
                    ("Move Track Up", "track.moveUp"),
                    ("Move Track Down", "track.moveDown"),
                    ("Delete Track", "track.remove"),
                ] {
                    if ui.button(label).clicked() {
                        acts.push(Act::Run(id, json!({})));
                        ui.close();
                    }
                }
            });
            // Move the cursor's track down / up.
            let chevrons = [(right - col::EYE - 52.0, "down", 1.0f32, "Move the track down"), (right - col::EYE - 30.0, "up", -1.0, "Move the track up")];
            for (x, id, dir, tip) in chevrons {
                let r = hit(ui, x, 20.0, id, tip);
                let enabled = if dir > 0.0 { c.track + 1 < n } else { c.track > 0 };
                let col = if !enabled {
                    dim
                } else if r.hovered() {
                    Color32::WHITE
                } else {
                    t.text
                };
                let (cx, cy) = (x, cy);
                p.add(Shape::line(vec![pos2(cx - 5.0, cy - dir * 2.5), pos2(cx, cy + dir * 2.5), pos2(cx + 5.0, cy - dir * 2.5)], Stroke::new(2.0, col)));
                if r.clicked() && enabled {
                    acts.push(Act::Run(if dir > 0.0 { "track.moveDown" } else { "track.moveUp" }, json!({})));
                }
            }
            let mt = hit(ui, right - col::EYE, 22.0, "multitrack", "Multitrack view");
            icon_cell(&p, pos2(right - col::EYE, cy), Icon::Multitrack, app.view.multitrack, t.accent, t.text, mt.hovered());
            if mt.clicked() {
                acts.push(Act::ToggleMultitrack);
            }
            draw(&p, Rect::from_center_size(pos2(right - col::MUTE, cy), vec2(15.0, 15.0)), Icon::Mute, dim);
            draw(&p, Rect::from_center_size(pos2(right - col::SOLO, cy), vec2(15.0, 15.0)), Icon::Solo, dim);
            label(right - col::FADER_END - FADER / 2.0, "Vol.", egui::Align2::CENTER_CENTER);
            label(right - col::PAN, "Pan.", egui::Align2::CENTER_CENTER);
            label(right - col::EQ, "Eq.", egui::Align2::CENTER_CENTER);

            for (i, tr) in score.tracks.iter().enumerate() {
                let (row, resp) = ui.allocate_exact_size(vec2(LEFT, ROW), Sense::click());
                let p = ui.painter().clone();
                let sel = i == c.track;
                p.rect_filled(row, 0.0, if sel { t.control } else { t.panel });
                p.line_segment([row.left_bottom(), row.right_bottom()], Stroke::new(1.0, t.chrome_dark));
                p.rect_filled(Rect::from_min_size(row.min, vec2(5.0, ROW)), 0.0, track_color(i));
                let cy = row.center().y;
                draw(&p, Rect::from_center_size(pos2(row.left() + 22.0, cy), vec2(16.0, 16.0)), crate::icons::instrument(tr), t.text);
                let name_clip = Rect::from_min_max(pos2(row.left() + 38.0, row.top()), pos2(row.right() - col::EYE - 16.0, row.bottom()));
                let num = p.with_clip_rect(name_clip).text(
                    pos2(name_clip.left(), cy),
                    egui::Align2::LEFT_CENTER,
                    format!("{}.", i + 1),
                    egui::FontId::proportional(14.0),
                    t.text_dim,
                );
                p.with_clip_rect(name_clip).text(pos2(num.right() + 5.0, cy), egui::Align2::LEFT_CENTER, &tr.name, egui::FontId::proportional(14.0), t.text);
                let right = row.right();
                let at = |x: f32| pos2(right - x, cy);
                let cell = |x: f32, w: f32| Rect::from_center_size(at(x), vec2(w, ROW - 4.0));
                let hovered = |r: Rect| resp.hover_pos().is_some_and(|h| r.contains(h));
                let shown = !is_hidden(app.view.hidden, i);
                let eye = cell(col::EYE, 24.0);
                icon_cell(&p, at(col::EYE), Icon::Eye, false, t.accent, if shown { t.text } else { dim }, hovered(eye));
                let mute = cell(col::MUTE, 24.0);
                icon_cell(&p, at(col::MUTE), Icon::Mute, tr.muted, MUTE_ON, dim, hovered(mute));
                let solo = cell(col::SOLO, 24.0);
                icon_cell(&p, at(col::SOLO), Icon::Solo, tr.solo, SOLO_ON, dim, hovered(solo));
                // The volume fader: dragged live, saved as one step on release.
                let slot = Rect::from_min_size(pos2(right - col::FADER_END - FADER, row.top() + 3.0), vec2(FADER, ROW - 6.0));
                fader(&p, slot, live(i, Control::Volume, i64::from(tr.volume)) as f32, true, automated(tr, MixKind::Volume));
                vol_slots.push((i, slot));
                let vol_a = cell(col::VOL_A, 18.0);
                auto_badge(&p, at(col::VOL_A), automated(tr, MixKind::Volume), hovered(vol_a), true);
                let pan = cell(col::PAN, 24.0);
                knob(&p, at(col::PAN), live(i, Control::Pan, i64::from(tr.pan)) as f32, true);
                pan_knobs.push((i, pan));
                let pan_a = cell(col::PAN_A, 18.0);
                auto_badge(&p, at(col::PAN_A), automated(tr, MixKind::Pan), hovered(pan_a), true);
                let eq = cell(col::EQ, 22.0);
                draw(&p, Rect::from_center_size(at(col::EQ), vec2(16.0, 16.0)), Icon::Equalizer, dim);
                if resp.clicked()
                    && let Some(pos) = resp.interact_pointer_pos()
                    && !slot.contains(pos)
                    && !pan.contains(pos)
                {
                    let part = if eye.contains(pos) {
                        RowPart::Eye
                    } else if mute.contains(pos) {
                        RowPart::Mute
                    } else if solo.contains(pos) {
                        RowPart::Solo
                    } else if vol_a.contains(pos) {
                        RowPart::VolumeAutomation
                    } else if pan_a.contains(pos) {
                        RowPart::PanAutomation
                    } else if eq.contains(pos) {
                        RowPart::Equalizer
                    } else {
                        RowPart::Name
                    };
                    acts.extend(row_click(i, tr, part));
                }
                if hovered(eq) {
                    resp.clone().on_hover_text("Equalizer (not in Guitario yet)");
                } else if hovered(eye) {
                    resp.clone().on_hover_text(if shown { "Hide from the multitrack view" } else { "Show in the multitrack view" });
                } else if hovered(vol_a) || hovered(pan_a) {
                    resp.clone().on_hover_text("Edit the automation");
                }
            }
            // The Master row: master volume, pan and focus are not built yet.
            let (row, _) = ui.allocate_exact_size(vec2(LEFT, ROW), Sense::hover());
            let p = ui.painter().clone();
            p.rect_filled(row, 0.0, t.chrome_dark);
            let cy = row.center().y;
            p.text(pos2(row.left() + 44.0, cy), egui::Align2::LEFT_CENTER, "Master", egui::FontId::proportional(14.0), t.text);
            let at = |x: f32| pos2(row.right() - x, cy);
            knob(&p, at(col::MUTE - 13.0), 0.0, false);
            fader(&p, Rect::from_min_size(pos2(row.right() - col::FADER_END - FADER, row.top() + 3.0), vec2(FADER, ROW - 6.0)), 100.0, false, false);
            auto_badge(&p, at(col::VOL_A), false, false, false);
            knob(&p, at(col::PAN), 0.0, false);
            auto_badge(&p, at(col::PAN_A), false, false, false);
            let _ = ui
                .interact(Rect::from_min_max(pos2(row.right() - col::EYE, row.top()), row.max), egui::Id::new("global-master"), Sense::hover())
                .on_hover_text("Master volume, pan and focus are not in Guitario yet");
        });
        // Bar grid.
        egui::ScrollArea::horizontal().id_salt("global-bars").show(ui, |ui| {
            let w = (bars as f32 * CELL + 8.0).max(ui.available_width());
            let h = HEAD + ROW * (n as f32 + 1.0);
            let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
            let p = ui.painter();
            p.rect_filled(rect, 0.0, t.chrome_dark);
            let grid_top = rect.top() + HEAD;
            let master_y = grid_top + ROW * n as f32;
            for (mi, hd) in score.headers.iter().enumerate() {
                let x = rect.left() + mi as f32 * CELL;
                if numbered(mi + 1) {
                    p.text(
                        pos2(x + 4.0, rect.top() + HEAD / 2.0 + 1.0),
                        egui::Align2::LEFT_CENTER,
                        (mi + 1).to_string(),
                        egui::FontId::proportional(13.0),
                        t.text,
                    );
                }
                if let Some(m) = &hd.marker {
                    // A bookmark and the section's name along the Master row, up to the next one.
                    let next = score.headers.iter().enumerate().skip(mi + 1).find(|(_, h)| h.marker.is_some()).map_or(bars, |(i, _)| i);
                    let clip = Rect::from_min_max(pos2(x, master_y), pos2(rect.left() + next as f32 * CELL - 2.0, master_y + ROW));
                    let label = p.with_clip_rect(clip);
                    draw(&label, Rect::from_center_size(pos2(x + 10.0, master_y + ROW / 2.0), vec2(13.0, 13.0)), Icon::Bookmark, t.text);
                    label.text(pos2(x + 21.0, master_y + ROW / 2.0), egui::Align2::LEFT_CENTER, m, egui::FontId::proportional(13.0), t.text);
                }
                for (ti, tr) in score.tracks.iter().enumerate() {
                    let cell = Rect::from_min_size(pos2(x + 1.0, grid_top + ti as f32 * ROW + 1.0), vec2(CELL - 2.0, ROW - 2.0));
                    let has = tr.measures.get(mi).is_some_and(|m| m.voices.iter().any(|v| v.beats.iter().any(|b| !b.rest)));
                    let fill = if has { track_color(ti).gamma_multiply(if tr.muted { 0.45 } else { 1.0 }) } else { Color32::from_rgb(0x4a, 0x4c, 0x55) };
                    p.rect_filled(cell, 1.0, fill);
                }
            }
            if let Some((pm, _)) = app.transport.beat.filter(|_| app.transport.playing) {
                let px = rect.left() + pm as f32 * CELL;
                p.rect_filled(Rect::from_min_size(pos2(px, grid_top), vec2(CELL, ROW * n as f32)), 0.0, Color32::from_white_alpha(46));
            }
            if let Some(sel) = app.session.selection() {
                let ((m0, _), (m1, _)) = sel.ordered();
                let r = Rect::from_min_max(
                    pos2(rect.left() + m0 as f32 * CELL, grid_top + sel.track as f32 * ROW),
                    pos2(rect.left() + (m1 + 1) as f32 * CELL, grid_top + (sel.track + 1) as f32 * ROW),
                );
                p.rect_stroke(r, 2.0, Stroke::new(2.0, t.accent), egui::StrokeKind::Inside);
            }
            // The cursor: a dark square in its bar, and a marker over the bar numbers.
            let shown_bar = app.transport.beat.filter(|_| app.transport.playing).map_or(c.measure, |(m, _)| m);
            let cx = rect.left() + c.measure as f32 * CELL;
            let cursor_cell = Rect::from_center_size(pos2(cx + CELL / 2.0, grid_top + c.track as f32 * ROW + ROW / 2.0), vec2(CELL - 10.0, ROW - 10.0));
            p.rect_filled(cursor_cell, CornerRadius::same(3), Color32::from_rgb(0x16, 0x17, 0x1a));
            let mx = rect.left() + shown_bar as f32 * CELL + CELL / 2.0;
            p.add(Shape::convex_polygon(
                vec![pos2(mx - 5.0, rect.top() + 1.0), pos2(mx + 5.0, rect.top() + 1.0), pos2(mx, rect.top() + 6.0)],
                t.accent,
                Stroke::NONE,
            ));
            if resp.clicked()
                && let Some(pos) = resp.interact_pointer_pos()
                && let Some((ti, mi)) = grid_hit(pos - rect.min, n, bars)
            {
                acts.push(Act::Run("cursor.set", json!({"track": ti, "measure": mi, "beat": 0})));
            }
        });
    });
    // Faders and knobs sit above the rows' click areas.
    let mut drag = app.mixer.drag;
    for (i, slot) in vol_slots {
        let r = ui.interact(slot.expand(2.0), egui::Id::new(("global-volume", i)), Sense::click_and_drag());
        if let Some(pos) = r.interact_pointer_pos().filter(|_| r.dragged() || r.clicked() || r.drag_stopped()) {
            let v = (((pos.x - slot.left() - 3.0) / (slot.width() - 6.0)).clamp(0.0, 1.0) * 127.0).round() as i64;
            drag = Some((i, Control::Volume, v));
            if r.drag_stopped() || (r.clicked() && !r.dragged()) {
                acts.push(Act::Run("track.setMix", json!({"track": i, "volume": v})));
                drag = None;
            }
        }
    }
    for (i, area) in pan_knobs {
        let r = ui.interact(area, egui::Id::new(("global-pan", i)), Sense::click_and_drag()).on_hover_text("Pan (drag; double-click to centre)");
        let stored = score.tracks.get(i).map_or(0, |t| i64::from(t.pan));
        if r.double_clicked() {
            acts.push(Act::Run("track.setMix", json!({"track": i, "pan": 0})));
            drag = None;
        } else if r.dragged() {
            let from = match drag {
                Some((ti, Control::Pan, v)) if ti == i => v,
                _ => stored,
            };
            let d = r.drag_delta();
            drag = Some((i, Control::Pan, pan_step(from, d.x - d.y)));
        } else if r.drag_stopped()
            && let Some((ti, Control::Pan, v)) = drag
            && ti == i
        {
            acts.push(Act::Run("track.setMix", json!({"track": i, "pan": v})));
            drag = None;
        }
    }
    let dragging = drag.is_some();
    app.mixer.drag = drag;
    for act in acts {
        match act {
            Act::Run(id, p) => {
                app.run(id, p);
            }
            Act::ToggleHidden(i) => {
                if let Some(bit) = u32::try_from(i).ok().and_then(|i| 1u64.checked_shl(i)) {
                    app.view.hidden ^= bit;
                }
            }
            Act::ToggleMultitrack => app.ui_action(crate::menu_catalog::UiAction::ToggleMultitrack),
            Act::Automation(i, kind) => {
                app.run("track.select", json!({"index": i}));
                app.open_mix_automation(kind);
            }
        }
    }
    if dragging {
        app.push_mix();
    }
}

/// A knob drag of `pixels` (right or up is positive) moves the pan from `from`, two pixels a step.
fn pan_step(from: i64, pixels: f32) -> i64 {
    let step = if pixels.is_finite() { (pixels / 2.0).round() as i64 } else { 0 };
    from.saturating_add(step).clamp(-63, 63)
}

/// The (track, bar) under a point `at` measured from the grid's top-left, if it is on a cell.
fn grid_hit(at: egui::Vec2, tracks: usize, bars: usize) -> Option<(usize, usize)> {
    if !(at.x.is_finite() && at.y.is_finite()) || at.x < 0.0 || at.y < HEAD {
        return None;
    }
    let mi = (at.x / CELL).floor() as usize;
    let ti = ((at.y - HEAD) / ROW).floor() as usize;
    (mi < bars && ti < tracks).then_some((ti, mi))
}

/// Where a click on a track row landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowPart {
    Name,
    Eye,
    Mute,
    Solo,
    VolumeAutomation,
    PanAutomation,
    Equalizer,
}

/// What a click on row `i` does. Mute, solo and the eye name their track, so any track can be
/// changed (while playing too) without moving the cursor to it.
fn row_click(i: usize, tr: &guitario_score::Track, part: RowPart) -> Option<Act> {
    Some(match part {
        RowPart::Mute => Act::Run("track.setMix", json!({"track": i, "muted": !tr.muted})),
        RowPart::Solo => Act::Run("track.setMix", json!({"track": i, "solo": !tr.solo})),
        RowPart::Name => Act::Run("track.select", json!({"index": i})),
        RowPart::Eye => Act::ToggleHidden(i),
        RowPart::VolumeAutomation => Act::Automation(i, MixKind::Volume),
        RowPart::PanAutomation => Act::Automation(i, MixKind::Pan),
        RowPart::Equalizer => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    fn run(s: &mut Session, act: Option<Act>) {
        match act {
            Some(Act::Run(id, p)) => {
                s.run(id, &p).unwrap();
            }
            _ => panic!("expected a command"),
        }
    }

    #[test]
    fn mute_and_solo_act_on_their_own_row_not_the_cursor_track() {
        let mut s = Session::new();
        s.run("track.add", &json!({"name": "Bass", "tuning": "bass"})).unwrap();
        s.run("track.addDrums", &json!({})).unwrap();
        s.run("track.select", &json!({"index": 0})).unwrap();
        for (row, part) in [(1, RowPart::Mute), (2, RowPart::Solo)] {
            let act = row_click(row, &s.score().tracks[row].clone(), part);
            run(&mut s, act);
        }
        let t = &s.score().tracks;
        assert!(!t[0].muted && !t[0].solo, "the cursor's track is untouched");
        assert!(t[1].muted && t[2].solo);
        assert_eq!(s.cursor().track, 0, "the cursor stays");
        // A second click turns it back off.
        let act = row_click(1, &s.score().tracks[1].clone(), RowPart::Mute);
        run(&mut s, act);
        assert!(!s.score().tracks[1].muted);
        assert!(matches!(row_click(2, &s.score().tracks[2], RowPart::Name), Some(Act::Run("track.select", _))));
        assert!(matches!(row_click(2, &s.score().tracks[2], RowPart::Eye), Some(Act::ToggleHidden(2))));
        assert!(matches!(row_click(1, &s.score().tracks[1], RowPart::PanAutomation), Some(Act::Automation(1, MixKind::Pan))));
        assert!(row_click(1, &s.score().tracks[1], RowPart::Equalizer).is_none());
    }

    #[test]
    fn automation_badges_follow_the_track() {
        let mut s = Session::new();
        let tr = s.score().tracks[0].clone();
        assert!(!automated(&tr, MixKind::Volume) && !automated(&tr, MixKind::Pan));
        s.run("track.setAutomation", &json!({"kind": "volume", "value": 90, "measure": 0})).unwrap();
        let tr = s.score().tracks[0].clone();
        assert!(automated(&tr, MixKind::Volume) && !automated(&tr, MixKind::Pan));
    }

    #[test]
    fn bar_numbers_and_hits() {
        let labelled: Vec<usize> = (1..=13).filter(|b| numbered(*b)).collect();
        assert_eq!(labelled, vec![1, 4, 8, 12]);
        assert_eq!(grid_hit(vec2(CELL * 2.5, HEAD + ROW * 1.5), 3, 10), Some((1, 2)));
        assert_eq!(grid_hit(vec2(5.0, 5.0), 3, 10), None, "the bar numbers row");
        assert_eq!(grid_hit(vec2(CELL * 20.0, HEAD + 1.0), 3, 10), None, "past the last bar");
        assert_eq!(grid_hit(vec2(5.0, HEAD + ROW * 3.5), 3, 10), None, "the Master row");
        assert_eq!(grid_hit(vec2(f32::NAN, 40.0), 3, 10), None);
    }

    #[test]
    fn pan_drags_step_and_stop_at_the_ends() {
        assert_eq!(pan_step(0, 10.0), 5);
        assert_eq!(pan_step(60, 40.0), 63);
        assert_eq!(pan_step(-60, -40.0), -63);
        assert_eq!(pan_step(3, f32::NAN), 3);
    }
}
