//! The fretboard instrument view, drawn as Guitar Pro 8's: the notes of the beat under the cursor
//! as yellow marks with their names, and the display mode's other notes dimmed. Clicking a fret
//! enters that note; hovering names its pitch.

use crate::theme::tokens;
use crate::{GuitarioApp, instrument};
use egui::{Color32, Rect, Stroke, pos2, vec2};
use serde_json::json;

const FRETS: u8 = 24;
/// The open-string strip before the nut, and the nut.
const OPEN: Color32 = Color32::from_rgb(0x60, 0x6b, 0x7e);
const NUT: Color32 = Color32::from_rgb(0xd9, 0xe1, 0xea);

/// x of fret `n` (0 = nut) on a neck from `x0` to `x1`, spaced like a real scale length.
fn fret_x(n: u8, x0: f32, x1: f32) -> f32 {
    let pos = |k: f32| 1.0 - 2f32.powf(-k / 12.0);
    x0 + (x1 - x0) * pos(f32::from(n)) / pos(f32::from(FRETS))
}

/// The panel's height for a neck of `strings` strings (a drum pad takes the six-string height).
pub fn height(strings: f32) -> f32 {
    (strings.max(6.0) * 15.0 + 6.0).min(170.0)
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
    let (beat, others) = instrument::beats(track, c, app.view.instrument);
    let strings = track.string_count();
    let avail = ui.available_size();
    let (rect, resp) = ui.allocate_exact_size(vec2(avail.x, avail.y.clamp(90.0, 170.0)), egui::Sense::click());
    let board = instrument::side(app, ui, rect, true);
    let p = ui.painter();
    p.rect_filled(board, 0.0, t.neck);

    // Guitar Pro's neck: a pale strip for the open strings, the nut, then the frets. Left-handed,
    // the same mirrored.
    let nut_w = 30.0;
    let neck = Rect::from_min_max(pos2(board.left() + nut_w, board.top()), pos2(board.right() - 6.0, board.bottom()));
    let lefty = app.view.left_handed;
    let mx = |x: f32| if lefty { board.left() + board.right() - x } else { x };
    let span = |x0: f32, x1: f32| (mx(x0).min(mx(x1)), mx(x0).max(mx(x1)));
    let (o0, o1) = span(board.left(), neck.left());
    p.rect_filled(Rect::from_x_y_ranges(o0..=o1, board.y_range()), 0.0, OPEN);
    let sy = |s: usize| board.top() + board.height() * (s as f32 + 0.5) / strings.max(1) as f32;
    let fx = |n: u8| fret_x(n, neck.left(), neck.right());
    let space = board.height() / strings.max(1) as f32;

    // Inlays.
    let inlay = |x: f32, y: f32| p.circle_filled(pos2(mx(x), y), (space * 0.22).clamp(3.0, 6.0), t.inlay);
    for f in [3u8, 5, 7, 9, 15, 17, 19, 21] {
        inlay((fx(f - 1) + fx(f)) / 2.0, board.center().y);
    }
    for f in [12u8, 24] {
        let x = (fx(f - 1) + fx(f)) / 2.0;
        inlay(x, sy(1).min(board.center().y));
        inlay(x, sy(strings.saturating_sub(2)).max(board.center().y));
    }
    // Nut and frets.
    p.line_segment([pos2(mx(neck.left()), board.top()), pos2(mx(neck.left()), board.bottom())], Stroke::new(3.0, NUT));
    for f in 1..=FRETS {
        let x = mx(fx(f));
        p.line_segment([pos2(x, board.top()), pos2(x, board.bottom())], Stroke::new(2.0, t.fret_wire));
    }
    // Strings, a little thicker toward the bass.
    for s in 0..strings {
        let y = sy(s);
        let w = 1.0 + 0.8 * s as f32 / strings.max(2).saturating_sub(1) as f32;
        p.line_segment([pos2(board.left(), y), pos2(board.right(), y)], Stroke::new(w, t.string));
    }

    // Where a click would put a note.
    let hit = |pos: egui::Pos2| -> Option<(usize, u8)> {
        if !board.contains(pos) {
            return None;
        }
        let s = ((pos.y - board.top()) / board.height() * strings as f32).floor();
        if s < 0.0 || s >= strings as f32 {
            return None;
        }
        let x = mx(pos.x);
        let fret = if x < neck.left() { 0 } else { (1..=FRETS).find(|f| x <= fx(*f)).unwrap_or(FRETS) };
        Some((s as usize, fret))
    };
    let mark_x = |f: u8| mx(if f == 0 { board.left() + nut_w / 2.0 } else { (fx(f - 1) + fx(f)) / 2.0 });
    let radius = (space * 0.42).clamp(5.0, 10.0);
    let pitch = |s: u8, f: u8| track.tuning.strings.get(usize::from(s)).and_then(|o| o.checked_add(track.capo)?.checked_add(f));
    let mark = |s: u8, f: u8, fill: Color32| {
        let center = pos2(mark_x(f.min(FRETS)), sy(usize::from(s)));
        p.circle_filled(center, radius, fill);
        if let Some(m) = pitch(s, f) {
            p.text(center, egui::Align2::CENTER_CENTER, guitario_theory::note_name(m), crate::fonts::sans_bold(radius * 1.1), instrument::NOTE_INK);
        }
    };
    if let Some(pos) = resp.hover_pos()
        && let Some((s, f)) = hit(pos)
    {
        p.circle_stroke(pos2(mark_x(f), sy(s)), radius, Stroke::new(1.5, instrument::NOTE));
        if let Some(m) = pitch(s as u8, f) {
            resp.clone().on_hover_text(format!("{} (string {}, fret {f})", instrument::pitch_label(m), s + 1));
        }
    }
    for b in &others {
        for n in &b.notes {
            mark(n.string, n.fret, instrument::NOTE_SOFT);
        }
    }
    if let Some(b) = beat.filter(|b| !b.rest) {
        for n in &b.notes {
            mark(n.string, n.fret, instrument::NOTE);
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
    let (whole, _) = ui.allocate_exact_size(vec2(avail.x, avail.y.clamp(90.0, 170.0)), egui::Sense::hover());
    let rect = instrument::side(app, ui, whole, false);
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
