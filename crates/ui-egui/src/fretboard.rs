//! The instrument view: a fretboard showing the notes of the beat under the cursor. Clicking a
//! fret enters that note, as in Guitar Pro.

use crate::GuitarioApp;
use crate::theme::tokens;
use egui::{Color32, Rect, Stroke, pos2, vec2};
use serde_json::json;

const FRETS: u8 = 24;

/// x of fret `n` (0 = nut) on a neck from `x0` to `x1`, spaced like a real scale length.
fn fret_x(n: u8, x0: f32, x1: f32) -> f32 {
    let pos = |k: f32| 1.0 - 2f32.powf(-k / 12.0);
    x0 + (x1 - x0) * pos(f32::from(n)) / pos(f32::from(FRETS))
}

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let t = tokens();
    let score = app.session.score().clone();
    let mut c = app.session.cursor();
    if let Some((m, b)) = app.transport.beat.filter(|_| app.transport.playing) {
        c.measure = m;
        c.beat = b;
    }
    let Some(track) = score.tracks.get(c.track) else { return };
    if track.is_drums() {
        drum_pad(app, ui, track, c);
        return;
    }
    let beat = track.measures.get(c.measure).and_then(|m| m.voices.get(c.voice)).and_then(|v| v.beats.get(c.beat));
    let strings = track.string_count();
    let avail = ui.available_size();
    let (rect, resp) = ui.allocate_exact_size(vec2(avail.x, avail.y.clamp(90.0, 170.0)), egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, t.chrome_dark);

    let label_w = 34.0;
    let open_w = 34.0;
    let neck = Rect::from_min_max(pos2(rect.left() + label_w + open_w, rect.top() + 12.0), pos2(rect.right() - 16.0, rect.bottom() - 22.0));
    p.rect_filled(neck, 2.0, t.neck);
    let sy = |s: usize| neck.top() + neck.height() * (s as f32 + 0.5) / strings.max(1) as f32;
    let fx = |n: u8| fret_x(n, neck.left(), neck.right());

    // Inlays.
    for f in [3u8, 5, 7, 9, 15, 17, 19, 21] {
        p.circle_filled(pos2((fx(f - 1) + fx(f)) / 2.0, neck.center().y), 5.0, t.inlay.gamma_multiply(0.85));
    }
    for f in [12u8, 24] {
        let x = (fx(f - 1) + fx(f)) / 2.0;
        p.circle_filled(pos2(x, neck.top() + neck.height() * 0.3), 5.0, t.inlay.gamma_multiply(0.85));
        p.circle_filled(pos2(x, neck.top() + neck.height() * 0.7), 5.0, t.inlay.gamma_multiply(0.85));
    }
    // Nut and frets.
    p.rect_filled(Rect::from_min_size(pos2(neck.left() - 5.0, neck.top()), vec2(5.0, neck.height())), 1.0, Color32::from_rgb(0xee, 0xe8, 0xd8));
    for f in 1..=FRETS {
        let x = fx(f);
        p.line_segment([pos2(x, neck.top()), pos2(x, neck.bottom())], Stroke::new(2.0, t.fret_wire));
        if [3, 5, 7, 9, 12, 15, 17, 19, 21, 24].contains(&f) {
            p.text(pos2((fx(f - 1) + x) / 2.0, neck.bottom() + 10.0), egui::Align2::CENTER_CENTER, f.to_string(), egui::FontId::proportional(10.0), t.text_dim);
        }
    }
    // Strings, thicker toward the bass, with their open-note names.
    for s in 0..strings {
        let y = sy(s);
        let w = 0.8 + 1.8 * s as f32 / strings.max(2).saturating_sub(1) as f32;
        p.line_segment([pos2(rect.left() + label_w, y), pos2(neck.right(), y)], Stroke::new(w, t.string));
        let name = track.tuning.strings.get(s).map_or("", |p| guitario_theory::note_name(*p));
        let col = if s == c.string { t.accent } else { t.text_dim };
        p.text(pos2(rect.left() + label_w / 2.0, y), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(11.0), col);
    }

    // Hover: where a click would put a note.
    let hit = |pos: egui::Pos2| -> Option<(usize, u8)> {
        if pos.x < rect.left() + label_w || !rect.contains(pos) {
            return None;
        }
        let s = ((pos.y - neck.top()) / neck.height() * strings as f32).floor();
        if s < 0.0 || s >= strings as f32 {
            return None;
        }
        let fret = if pos.x < neck.left() { 0 } else { (1..=FRETS).find(|f| pos.x <= fx(*f)).unwrap_or(FRETS) };
        Some((s as usize, fret))
    };
    let mark_x = |f: u8| if f == 0 { neck.left() - open_w / 2.0 } else { (fx(f - 1) + fx(f)) / 2.0 };
    if let Some(pos) = resp.hover_pos()
        && let Some((s, f)) = hit(pos)
    {
        p.circle_stroke(pos2(mark_x(f), sy(s)), 8.0, Stroke::new(1.5, t.accent.gamma_multiply(0.8)));
    }
    // Notes of the current beat.
    if let Some(b) = beat.filter(|b| !b.rest) {
        for n in &b.notes {
            let s = usize::from(n.string);
            let center = pos2(mark_x(n.fret.min(FRETS)), sy(s));
            let fill = if s == c.string { t.accent } else { Color32::from_rgb(0xe0, 0x4f, 0x4f) };
            p.circle_filled(center, 9.0, fill);
            p.text(center, egui::Align2::CENTER_CENTER, n.fret.to_string(), egui::FontId::proportional(10.0), Color32::WHITE);
        }
    }
    if resp.clicked()
        && let Some(pos) = resp.interact_pointer_pos()
        && let Some((s, f)) = hit(pos)
    {
        app.run("cursor.set", json!({"string": s}));
        app.run("note.setFret", json!({"fret": f}));
    }
}

/// Guitar Pro's drum pad: one pad per kit piece. Clicking a pad toggles that hit on the beat under
/// the cursor; pads light up for the hits of the current (or playing) beat.
fn drum_pad(app: &mut GuitarioApp, ui: &mut egui::Ui, track: &guitario_score::Track, c: guitario_engine::Cursor) {
    use guitario_theory::drums::KIT;
    let t = tokens();
    let beat = track.measures.get(c.measure).and_then(|m| m.voices.get(c.voice)).and_then(|v| v.beats.get(c.beat));
    let lit = |midi: u8| beat.is_some_and(|b| !b.rest && b.notes.iter().any(|n| n.fret == midi));
    let avail = ui.available_size();
    let (rect, _) = ui.allocate_exact_size(vec2(avail.x, avail.y.clamp(90.0, 170.0)), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, t.chrome_dark);
    let cols = KIT.len().div_ceil(2);
    let gap = 6.0;
    let pad_w = ((rect.width() - 24.0 - gap * (cols as f32 - 1.0)) / cols as f32).max(30.0);
    let pad_h = ((rect.height() - 24.0 - gap) / 2.0).max(24.0);
    let mut clicked = None;
    for (i, piece) in KIT.iter().enumerate() {
        let (row, col) = (i / cols, i % cols);
        let r = Rect::from_min_size(pos2(rect.left() + 12.0 + col as f32 * (pad_w + gap), rect.top() + 12.0 + row as f32 * (pad_h + gap)), vec2(pad_w, pad_h));
        let resp = ui
            .interact(r, ui.id().with(("pad", piece.midi)), egui::Sense::click())
            .on_hover_text(format!("{} (GM {}, key {})", piece.name, piece.midi, piece.midi));
        let p = ui.painter();
        let on = lit(piece.midi);
        let fill = if on {
            t.accent
        } else if resp.hovered() {
            t.control_hover
        } else {
            t.control
        };
        p.rect_filled(r, 6.0, fill);
        p.rect_stroke(r, 6.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        p.text(
            r.center() - vec2(0.0, 7.0),
            egui::Align2::CENTER_CENTER,
            piece.short,
            egui::FontId::proportional(15.0),
            if on { Color32::WHITE } else { t.text },
        );
        p.text(
            r.center() + vec2(0.0, 10.0),
            egui::Align2::CENTER_CENTER,
            piece.name,
            egui::FontId::proportional(9.5),
            if on { Color32::WHITE } else { t.text_dim },
        );
        if resp.clicked() {
            clicked = Some(piece.midi);
        }
    }
    if let Some(midi) = clicked {
        app.run("note.setDrum", json!({"drum": midi}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frets_get_closer_together() {
        let w1 = fret_x(1, 0.0, 1000.0) - fret_x(0, 0.0, 1000.0);
        let w12 = fret_x(13, 0.0, 1000.0) - fret_x(12, 0.0, 1000.0);
        assert!(w1 > w12 * 1.8);
        assert!((fret_x(FRETS, 0.0, 1000.0) - 1000.0).abs() < 1e-3);
    }
}
