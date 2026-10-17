//! The global view along the bottom: one row per track (name, mute/solo, volume) beside a bar grid
//! showing which bars hold notes, as in Guitar Pro.

use crate::GuitarioApp;
use crate::theme::{tokens, track_color};
use egui::{Color32, Rect, Stroke, pos2, vec2};
use serde_json::json;

const ROW: f32 = 26.0;
const HEAD: f32 = 28.0;
const LEFT: f32 = 250.0;
const CELL: f32 = 16.0;

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let t = tokens();
    let score = app.session.score().clone();
    let c = app.session.cursor();
    let mut actions: Vec<(&'static str, serde_json::Value)> = Vec::new();
    let bars = score.headers.len();
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        // Track list.
        ui.vertical(|ui| {
            ui.set_width(LEFT);
            let (r, _) = ui.allocate_exact_size(vec2(LEFT, HEAD), egui::Sense::hover());
            ui.painter().rect_filled(r, 0.0, t.panel_header);
            ui.painter().text(r.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, "Tracks", egui::FontId::proportional(11.0), t.text_dim);
            for (i, tr) in score.tracks.iter().enumerate() {
                let (row, resp) = ui.allocate_exact_size(vec2(LEFT, ROW), egui::Sense::click());
                let p = ui.painter();
                let sel = i == c.track;
                p.rect_filled(
                    row,
                    0.0,
                    if sel {
                        t.accent.gamma_multiply(0.35)
                    } else if i % 2 == 0 {
                        t.panel
                    } else {
                        t.chrome
                    },
                );
                p.rect_filled(Rect::from_min_size(row.min, vec2(5.0, ROW)), 0.0, track_color(i));
                p.text(
                    row.left_center() + vec2(12.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    format!("{}. {}", i + 1, tr.name),
                    egui::FontId::proportional(12.0),
                    t.text,
                );
                // Mute / solo / volume on the right of the row.
                let ms = |dx: f32, label: &str, on: bool, color: Color32| {
                    let r = Rect::from_center_size(pos2(row.right() - dx, row.center().y), vec2(18.0, 16.0));
                    p.rect_filled(r, 3.0, if on { color } else { t.control });
                    p.text(r.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(10.0), if on { Color32::BLACK } else { t.text_dim });
                    r
                };
                let mr = ms(78.0, "M", tr.muted, Color32::from_rgb(0xe8, 0x9a, 0x3a));
                let sr = ms(56.0, "S", tr.solo, Color32::from_rgb(0xe8, 0xd4, 0x3a));
                let vol = Rect::from_min_size(pos2(row.right() - 42.0, row.center().y - 2.0), vec2(34.0, 4.0));
                p.rect_filled(vol, 2.0, t.chrome_dark);
                p.rect_filled(Rect::from_min_size(vol.min, vec2(vol.width() * f32::from(tr.volume) / 127.0, 4.0)), 2.0, track_color(i));
                if resp.clicked()
                    && let Some(pos) = resp.interact_pointer_pos()
                {
                    let part = if mr.contains(pos) {
                        RowPart::Mute
                    } else if sr.contains(pos) {
                        RowPart::Solo
                    } else {
                        RowPart::Name
                    };
                    actions.push(row_click(i, tr, part));
                }
            }
        });
        // Bar grid.
        egui::ScrollArea::horizontal().id_salt("global-bars").show(ui, |ui| {
            let w = (bars as f32 * CELL + 8.0).max(ui.available_width());
            let h = HEAD + ROW * score.tracks.len() as f32;
            let (rect, resp) = ui.allocate_exact_size(vec2(w, h), egui::Sense::click());
            let p = ui.painter();
            p.rect_filled(rect, 0.0, t.chrome_dark);
            p.rect_filled(Rect::from_min_size(rect.min, vec2(w, HEAD)), 0.0, t.panel_header);
            for (mi, hd) in score.headers.iter().enumerate() {
                let x = rect.left() + mi as f32 * CELL;
                if mi % 4 == 0 {
                    p.text(
                        pos2(x + 2.0, rect.top() + HEAD - 2.0),
                        egui::Align2::LEFT_BOTTOM,
                        (mi + 1).to_string(),
                        egui::FontId::proportional(9.5),
                        t.text_dim,
                    );
                }
                if let Some(m) = &hd.marker {
                    // Clip to the room before the next marker so neighbouring labels never overlap.
                    let next = score.headers.iter().enumerate().skip(mi + 1).find(|(_, h)| h.marker.is_some()).map_or(bars, |(i, _)| i);
                    let clip = Rect::from_min_max(pos2(x, rect.top()), pos2(rect.left() + next as f32 * CELL - 1.0, rect.top() + HEAD));
                    p.with_clip_rect(clip).text(pos2(x + 2.0, rect.top() + 1.0), egui::Align2::LEFT_TOP, m, egui::FontId::proportional(8.5), t.accent);
                }
                for (ti, tr) in score.tracks.iter().enumerate() {
                    let cell = Rect::from_min_size(pos2(x + 1.0, rect.top() + HEAD + ti as f32 * ROW + 4.0), vec2(CELL - 2.0, ROW - 8.0));
                    let has = tr.measures.get(mi).is_some_and(|m| m.voices.iter().any(|v| v.beats.iter().any(|b| !b.rest)));
                    if has {
                        p.rect_filled(cell, 2.0, track_color(ti).gamma_multiply(if tr.muted { 0.35 } else { 0.85 }));
                    } else {
                        p.rect_stroke(cell, 2.0, Stroke::new(1.0, Color32::from_gray(70)), egui::StrokeKind::Inside);
                    }
                }
            }
            if let Some((pm, _)) = app.transport.beat.filter(|_| app.transport.playing) {
                let px = rect.left() + pm as f32 * CELL;
                p.rect_filled(Rect::from_min_size(pos2(px, rect.top()), vec2(CELL, h)), 0.0, t.play_beat.gamma_multiply(0.35));
            }
            if let Some(sel) = app.session.selection() {
                let ((m0, _), (m1, _)) = sel.ordered();
                let r = Rect::from_min_max(
                    pos2(rect.left() + m0 as f32 * CELL, rect.top() + HEAD + sel.track as f32 * ROW),
                    pos2(rect.left() + (m1 + 1) as f32 * CELL, rect.top() + HEAD + (sel.track + 1) as f32 * ROW),
                );
                p.rect_stroke(r, 2.0, Stroke::new(2.0, t.accent), egui::StrokeKind::Inside);
            }
            let cx = rect.left() + c.measure as f32 * CELL;
            p.rect_stroke(Rect::from_min_size(pos2(cx, rect.top()), vec2(CELL, h)), 0.0, Stroke::new(1.5, t.cursor_stroke), egui::StrokeKind::Inside);
            if resp.clicked()
                && let Some(pos) = resp.interact_pointer_pos()
            {
                let mi = ((pos.x - rect.left()) / CELL).floor().max(0.0) as usize;
                let ti = ((pos.y - rect.top() - HEAD) / ROW).floor().max(0.0) as usize;
                if mi < bars {
                    actions.push(("cursor.set", json!({"track": ti.min(score.tracks.len().saturating_sub(1)), "measure": mi, "beat": 0})));
                }
            }
        });
    });
    for (id, p) in actions {
        app.run(id, p);
    }
}

/// Where a click on a track row landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowPart {
    Name,
    Mute,
    Solo,
}

/// The command a click on row `i` runs. Mute and solo name their track, so any track can be
/// muted or soloed (while playing too) without moving the cursor to it.
fn row_click(i: usize, tr: &guitario_score::Track, part: RowPart) -> (&'static str, serde_json::Value) {
    match part {
        RowPart::Mute => ("track.setMix", json!({"track": i, "muted": !tr.muted})),
        RowPart::Solo => ("track.setMix", json!({"track": i, "solo": !tr.solo})),
        RowPart::Name => ("track.select", json!({"index": i})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    #[test]
    fn mute_and_solo_act_on_their_own_row_not_the_cursor_track() {
        let mut s = Session::new();
        s.run("track.add", &json!({"name": "Bass", "tuning": "bass"})).unwrap();
        s.run("track.addDrums", &json!({})).unwrap();
        s.run("track.select", &json!({"index": 0})).unwrap();
        for (row, part) in [(1, RowPart::Mute), (2, RowPart::Solo)] {
            let (id, p) = row_click(row, &s.score().tracks[row].clone(), part);
            s.run(id, &p).unwrap();
        }
        let t = &s.score().tracks;
        assert!(!t[0].muted && !t[0].solo, "the cursor's track is untouched");
        assert!(t[1].muted && t[2].solo);
        assert_eq!(s.cursor().track, 0, "the cursor stays");
        // A second click turns it back off.
        let (id, p) = row_click(1, &s.score().tracks[1].clone(), RowPart::Mute);
        s.run(id, &p).unwrap();
        assert!(!s.score().tracks[1].muted);
        assert_eq!(row_click(2, &s.score().tracks[2], RowPart::Name).0, "track.select");
    }
}
