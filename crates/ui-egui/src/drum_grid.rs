//! The drum step grid in the instrument view: the bar under the cursor as kit pieces × steps.
//! Clicking a cell runs `drums.setStep`, one undo step a cell; the cursor's and the playing beat's
//! column are lit, and every beat of the bar starts a darker column group.

use crate::theme::tokens;
use crate::{GuitarioApp, instrument};
use egui::{Align2, Color32, FontId, Rect, Stroke, pos2, vec2};
use guitario_engine::Cursor;
use guitario_score::Track;
use guitario_theory::{TICKS_PER_QUARTER, drums};
use serde_json::json;

/// The panel's height with the grid showing: ten rows and the step numbers.
pub const HEIGHT: f32 = 190.0;
/// The pieces a grid always shows, top to bottom as on a drum staff; others join when the bar uses them.
const ROWS: [u8; 10] = [49, 51, 46, 42, 38, 50, 48, 43, 44, 36];
const LABEL_W: f32 = 96.0;
const HEADER_H: f32 = 16.0;

/// The rows for a bar holding `used`: the usual pieces, then any others in kit order.
fn rows(used: &[u8]) -> Vec<u8> {
    let mut rows = ROWS.to_vec();
    for midi in used {
        if !rows.contains(midi) {
            rows.push(*midi);
        }
    }
    rows.sort_by_key(|m| ROWS.iter().position(|r| r == m).unwrap_or(usize::from(drums::lane_for(*m)) + ROWS.len()));
    rows
}

/// The step `beat` of `track`'s bar starts on, if it starts on one.
fn beat_step(track: &Track, c: Cursor, step_ticks: u32) -> Option<usize> {
    let voice = track.measures.get(c.measure)?.voices.get(c.voice)?;
    let at: u32 = voice.beats.iter().take(c.beat).map(|b| b.duration.ticks()).sum();
    (step_ticks > 0 && at.is_multiple_of(step_ticks)).then(|| (at / step_ticks) as usize)
}

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui, track: &Track, c: Cursor) {
    let t = tokens();
    let avail = ui.available_size();
    let (whole, _) = ui.allocate_exact_size(vec2(avail.x, avail.y.max(90.0)), egui::Sense::hover());
    let rect = instrument::side(app, ui, whole, false);
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, t.chrome_dark);
    let view = app.view.drums.sanitized();
    let Some(header) = app.session.score().headers.get(c.measure).cloned() else { return };
    let Some(step) = guitario_engine::drum_step(u32::from(view.resolution), view.triplet) else { return };
    let voice = track.measures.get(c.measure).and_then(|m| m.voices.get(c.voice));
    let small = FontId::proportional(10.0);
    let Some(grid) = guitario_engine::drum_grid(voice, header.capacity(), step) else {
        p.text(rect.center(), Align2::CENTER_CENTER, format!("Bar {} does not divide into these steps", c.measure + 1), small, t.text_dim);
        return;
    };
    let mut used: Vec<u8> = grid.hits.iter().map(|(m, _)| *m).collect();
    used.dedup();
    let rows = rows(&used);
    let inner = rect.shrink2(vec2(8.0, 6.0));
    let cells = Rect::from_min_max(pos2(inner.left() + LABEL_W, inner.top() + HEADER_H), inner.max);
    let cw = cells.width() / grid.steps.max(1) as f32;
    let rh = (cells.height() / rows.len().max(1) as f32).min(18.0);
    let beat_ticks = TICKS_PER_QUARTER * 4 / u32::from(header.time_signature.denominator.max(1));
    let per_beat = (beat_ticks / step.ticks().max(1)).max(1) as usize;

    let mut label = format!("Bar {}", c.measure + 1);
    if grid.off_grid > 0 {
        label += &format!(" · {} between steps", grid.off_grid);
    }
    p.text(pos2(inner.left(), inner.top() + HEADER_H / 2.0), Align2::LEFT_CENTER, label, small.clone(), t.text_dim);
    let lit = [Some(c), app.transport.beat.filter(|_| app.transport.playing).map(|(m, b)| Cursor { measure: m, beat: b, ..c })]
        .into_iter()
        .flatten()
        .filter(|k| k.measure == c.measure)
        .filter_map(|k| beat_step(track, k, step.ticks()))
        .collect::<Vec<_>>();
    for s in 0..grid.steps {
        let x = cells.left() + s as f32 * cw;
        if s.is_multiple_of(per_beat) {
            p.text(pos2(x + cw / 2.0, inner.top() + HEADER_H / 2.0), Align2::CENTER_CENTER, (s / per_beat + 1).to_string(), small.clone(), t.text);
        }
        if lit.contains(&s) {
            let col = Rect::from_min_size(pos2(x, cells.top()), vec2(cw, rh * rows.len() as f32));
            p.rect_filled(col, 0.0, t.cursor_fill);
        }
    }

    let mut clicked = None;
    for (row, midi) in rows.iter().enumerate() {
        let y = cells.top() + row as f32 * rh;
        let name = drums::piece(*midi).map_or("Percussion", |p| p.name);
        let short = drums::piece(*midi).map_or("PC", |p| p.short);
        p.text(pos2(inner.left(), y + rh / 2.0), Align2::LEFT_CENTER, short, FontId::proportional(11.0), t.text);
        p.text(pos2(inner.left() + 28.0, y + rh / 2.0), Align2::LEFT_CENTER, name, small.clone(), t.text_dim);
        for s in 0..grid.steps {
            let r = Rect::from_min_size(pos2(cells.left() + s as f32 * cw, y), vec2(cw, rh)).shrink(1.0);
            let resp = ui.interact(r, ui.id().with(("step", *midi, s)), egui::Sense::click()).on_hover_text(format!("{name}, step {}", s + 1));
            let on = grid.has(*midi, s);
            let group = (s / per_beat).is_multiple_of(2);
            let fill = if on {
                t.accent
            } else if resp.hovered() {
                t.control_hover
            } else if group {
                t.control
            } else {
                t.panel
            };
            p.rect_filled(r, 2.0, fill);
            if s.is_multiple_of(per_beat) {
                p.line_segment([pos2(r.left() - 1.0, y), pos2(r.left() - 1.0, y + rh)], Stroke::new(1.0, t.border));
            }
            if on {
                p.circle_filled(r.center(), (rh * 0.2).min(cw * 0.25), Color32::WHITE);
            }
            if resp.clicked() {
                clicked = Some((*midi, s));
            }
        }
    }
    if let Some((midi, s)) = clicked {
        app.run("drums.setStep", json!({"drum": midi, "step": s, "resolution": view.resolution, "triplet": view.triplet}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_keep_the_kit_order_and_add_pieces_the_bar_uses() {
        assert_eq!(rows(&[]), ROWS.to_vec());
        // A china and a cowbell join below the usual pieces; a kick already has a row.
        let r = rows(&[52, 36, 56]);
        assert_eq!(r.len(), ROWS.len() + 2);
        assert_eq!(&r[..ROWS.len()], &ROWS[..]);
        assert!(r.contains(&52) && r.contains(&56));
    }
}
