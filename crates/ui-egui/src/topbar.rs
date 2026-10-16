//! The top bar: file and history buttons, the transport with its LCD display, and view toggles.

use crate::GuitarioApp;
use crate::icons::{Icon, button};
use crate::theme::tokens;
use egui::{Color32, Rect, pos2, vec2};
use guitario_score::{Score, playback_order};
use serde_json::{Value, json};

/// Seconds from the start to `measure` (first pass) and the whole song, following repeats.
fn song_times(score: &Score, measure: usize, beat_ticks: u32) -> (f64, f64) {
    let secs = |mi: usize, ticks: u32| f64::from(ticks) / f64::from(guitario_theory::TICKS_PER_QUARTER) * 60.0 / f64::from(score.tempo_at(mi).max(1));
    let mut total = 0.0;
    let mut at = None;
    for mi in playback_order(score) {
        if mi == measure && at.is_none() {
            at = Some(total + secs(mi, beat_ticks));
        }
        total += secs(mi, score.headers.get(mi).map_or(0, |h| h.capacity()));
    }
    (at.unwrap_or(0.0), total)
}

fn clock(s: f64) -> String {
    let s = s.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let t = tokens();
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let b = 30.0;
        if button(ui, Icon::New, b, false, true, "New (⌘N)").clicked() {
            app.new_document();
        }
        if button(ui, Icon::Open, b, false, true, "Open… (⌘O)").clicked() {
            app.open_dialog();
        }
        if button(ui, Icon::Save, b, false, true, "Save (⌘S)").clicked() {
            app.save_dialog(false);
        }
        ui.add_space(10.0);
        if button(ui, Icon::Undo, b, false, app.session.can_undo(), "Undo (⌘Z)").clicked() {
            app.run("edit.undo", Value::Null);
        }
        if button(ui, Icon::Redo, b, false, app.session.can_redo(), "Redo (⇧⌘Z)").clicked() {
            app.run("edit.redo", Value::Null);
        }

        // Centre the transport + LCD block.
        let lcd_w = 520.0;
        let block = 5.0 * (b + 2.0) + 12.0 + lcd_w + 12.0 + 3.0 * (b + 2.0) + 70.0;
        let gap = ((ui.available_width() - block - 7.0 * (b + 2.0)) / 2.0).max(10.0);
        ui.add_space(gap);
        let score = app.session.score().clone();
        let c = app.session.cursor();
        let last = score.headers.len().saturating_sub(1);
        if button(ui, Icon::First, b, false, true, "First bar").clicked() {
            app.run("cursor.set", json!({"measure": 0, "beat": 0}));
        }
        if button(ui, Icon::Prev, b, false, c.measure > 0, "Previous bar").clicked() {
            app.run("cursor.set", json!({"measure": c.measure.saturating_sub(1), "beat": 0}));
        }
        let playing = app.transport.playing;
        if button(
            ui,
            if playing { Icon::Stop } else { Icon::Play },
            b + 6.0,
            playing,
            true,
            if playing { "Stop (Space)" } else { "Play from the cursor (Space)" },
        )
        .clicked()
        {
            app.toggle_play();
        }
        if button(ui, Icon::Next, b, false, c.measure < last, "Next bar").clicked() {
            app.run("cursor.set", json!({"measure": (c.measure + 1).min(last), "beat": 0}));
        }
        if button(ui, Icon::Last, b, false, true, "Last bar").clicked() {
            app.run("cursor.set", json!({"measure": last, "beat": 0}));
        }
        ui.add_space(10.0);
        let play = app.transport.beat.filter(|_| app.transport.playing).map(|(m, bt)| (app.transport.position, m, bt));
        lcd(ui, &score, c, lcd_w, play);
        ui.add_space(10.0);
        let tr = &mut app.transport;
        if button(ui, Icon::Loop, b, tr.looping, true, "Loop").clicked() {
            tr.looping = !tr.looping;
        }
        if button(ui, Icon::Metronome, b, tr.metronome, true, "Metronome").clicked() {
            tr.metronome = !tr.metronome;
        }
        if button(ui, Icon::CountIn, b, tr.count_in, true, "Count-in").clicked() {
            tr.count_in = !tr.count_in;
        }
        ui.add_space(4.0);
        egui::ComboBox::from_id_salt("speed")
            .width(58.0)
            .selected_text(format!("{:.0}%", tr.speed * 100.0))
            .show_ui(ui, |ui| {
                for v in [0.25f32, 0.5, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0] {
                    ui.selectable_value(&mut tr.speed, v, format!("{:.0}%", v * 100.0));
                }
            })
            .response
            .on_hover_text("Relative speed");

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(6.0);
            if button(ui, Icon::Inspector, b, app.panels.inspector, true, "Inspector").clicked() {
                app.panels.inspector = !app.panels.inspector;
            }
            if button(ui, Icon::GlobalView, b, app.panels.global, true, "Global view").clicked() {
                app.panels.global = !app.panels.global;
            }
            if button(ui, Icon::Fretboard, b, app.panels.fretboard, true, "Fretboard").clicked() {
                app.panels.fretboard = !app.panels.fretboard;
            }
            if button(ui, Icon::Palette, b, app.panels.palette, true, "Edition palette").clicked() {
                app.panels.palette = !app.panels.palette;
            }
            if button(ui, Icon::Notation, b, app.view.notation, true, "Standard notation").clicked() {
                app.view.notation = !app.view.notation;
            }
            ui.add_space(6.0);
            egui::ComboBox::from_id_salt("zoom").width(64.0).selected_text(format!("{:.0}%", app.view.zoom * 100.0)).show_ui(ui, |ui| {
                for z in [0.5, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 2.0, 3.0] {
                    ui.selectable_value(&mut app.view.zoom, z, format!("{:.0}%", z * 100.0));
                }
            });
        });
    });
    let _ = t;
}

/// `play` is (song seconds, measure, beat) while playing; otherwise the edit cursor is shown.
fn lcd(ui: &mut egui::Ui, score: &Score, c: guitario_engine::Cursor, w: f32, play: Option<(f64, usize, usize)>) {
    let mut c = c;
    if let Some((_, m, b)) = play {
        c.measure = m;
        c.beat = b;
    }
    let t = tokens();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 40.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 5.0, t.lcd_bg);
    p.rect_stroke(rect, 5.0, egui::Stroke::new(1.0, Color32::from_gray(10)), egui::StrokeKind::Inside);
    let track = score.tracks.get(c.track);
    let header = score.headers.get(c.measure);
    let beat_ticks: u32 = track
        .and_then(|tr| tr.measures.get(c.measure))
        .and_then(|m| m.voices.first())
        .map_or(0, |v| v.beats.iter().take(c.beat).map(|b| b.duration.ticks()).sum());
    let (now, total) = song_times(score, c.measure, beat_ticks);
    let now = play.map_or(now, |p| p.0.max(0.0));
    let beat_no = beat_ticks / (4 * guitario_theory::TICKS_PER_QUARTER / u32::from(header.map_or(4, |h| h.time_signature.denominator).max(1))).max(1) + 1;
    let cells: [(&str, String, f32); 5] = [
        ("TRACK", track.map_or_else(String::new, |tr| format!("{}. {}", c.track + 1, tr.name)), 0.36),
        ("BAR", format!("{} / {}", c.measure + 1, score.headers.len()), 0.15),
        ("BEAT", format!("{}", beat_no), 0.1),
        ("SIG.", header.map_or_else(String::new, |h| format!("{}/{}", h.time_signature.numerator, h.time_signature.denominator)), 0.1),
        ("TIME", format!("{} / {}", clock(now), clock(total)), 0.29),
    ];
    let mut x = rect.left() + 10.0;
    let inner = w - 20.0;
    for (i, (label, value, frac)) in cells.iter().enumerate() {
        let cw = inner * frac;
        p.text(pos2(x, rect.top() + 6.0), egui::Align2::LEFT_TOP, *label, egui::FontId::proportional(8.5), t.lcd_dim);
        let max_w = cw - 8.0;
        let galley = p.layout(value.clone(), egui::FontId::monospace(14.0), t.lcd_text, f32::INFINITY);
        let clip = Rect::from_min_size(pos2(x, rect.top()), vec2(max_w, rect.height()));
        p.with_clip_rect(clip).galley(pos2(x, rect.bottom() - 6.0 - galley.size().y), galley, t.lcd_text);
        x += cw;
        if i + 1 < cells.len() {
            p.line_segment([pos2(x - 6.0, rect.top() + 7.0), pos2(x - 6.0, rect.bottom() - 7.0)], egui::Stroke::new(1.0, Color32::from_gray(45)));
        }
    }
    // Tempo badge at the right edge.
    let bpm = score.tempo_at(c.measure);
    p.text(pos2(rect.right() - 10.0, rect.top() + 6.0), egui::Align2::RIGHT_TOP, format!("♩ = {bpm}"), egui::FontId::proportional(9.5), t.lcd_dim);
}
