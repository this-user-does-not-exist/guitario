//! Effects › Whammy Bar…: the Whammy Bar window. Pick one of the shapes and a depth from an
//! octave down to an octave up in quarter tones, or drag the points of the curve: the horizontal
//! axis is the beat's length, the vertical axis the pitch, the written pitch in the middle. OK
//! sends the curve to `beat.setWhammy` as one undo step.

use crate::GuitarioApp;
use crate::whammy_draw::whammy_label;
use guitario_score::{WHAMMY_END, WHAMMY_MAX, WHAMMY_MAX_POINTS, WHAMMY_MIN, Whammy, WhammyKind, WhammyPoint};
use serde_json::{Value, json};

/// How far apart the positions the editor offers are: twelve steps across the beat.
const STEP: u8 = WHAMMY_END / 12;

#[derive(Default)]
pub(crate) struct WhammyDialog {
    pub(crate) open: bool,
    /// The curve being edited, as typed: nothing is tidied until OK.
    points: Vec<WhammyPoint>,
    /// The beat already has a whammy, so it can be removed.
    has_whammy: bool,
    /// The beat before has a whammy, so a curve that starts away from rest is a Hold or a Return.
    continues: bool,
}

/// A fresh curve dives a whole tone.
fn start_curve() -> Vec<WhammyPoint> {
    Whammy::preset(WhammyKind::Dive, -4).points().to_vec()
}

/// Moves point `i` to `pos` and `value`, keeping the curve a function of time: the first point stays
/// at the attack and the last at the end, the rest between their neighbours.
pub(crate) fn move_point(points: &mut [WhammyPoint], i: usize, pos: i32, value: i32) {
    let last = points.len().saturating_sub(1);
    let lo = if i == 0 { 0 } else { points.get(i - 1).map_or(0, |p| i32::from(p.pos) + 1) };
    let hi = if i >= last { i32::from(WHAMMY_END) } else { points.get(i + 1).map_or(i32::from(WHAMMY_END), |p| i32::from(p.pos) - 1) };
    let pos = if i == 0 {
        0
    } else if i >= last {
        i32::from(WHAMMY_END)
    } else {
        pos.clamp(lo, hi.max(lo))
    };
    if let Some(p) = points.get_mut(i) {
        p.pos = u8::try_from(pos.clamp(0, i32::from(WHAMMY_END))).unwrap_or(0);
        p.value = i8::try_from(value.clamp(i32::from(WHAMMY_MIN), i32::from(WHAMMY_MAX))).unwrap_or(0);
    }
}

/// Adds a point at `pos` (not at an end, not where one is) on the curve as it runs there.
pub(crate) fn add_point(points: &mut Vec<WhammyPoint>, pos: u8) -> bool {
    if points.len() >= WHAMMY_MAX_POINTS || pos == 0 || pos >= WHAMMY_END || points.iter().any(|p| p.pos == pos) {
        return false;
    }
    let value = Whammy::from_points(points).map_or(0, |w| w.value_at(f32::from(pos)).round() as i8);
    let at = points.iter().position(|p| p.pos > pos).unwrap_or(points.len());
    points.insert(at, WhammyPoint::new(pos, value));
    true
}

/// Removes an interior point; the ends and a two-point curve stay.
pub(crate) fn remove_point(points: &mut Vec<WhammyPoint>, i: usize) -> bool {
    if points.len() <= 2 || i == 0 || i + 1 >= points.len() {
        return false;
    }
    points.remove(i);
    true
}

impl WhammyDialog {
    fn curve(&self) -> Option<Whammy> {
        Whammy::from_points(&self.points)
    }

    /// The value farthest from rest, as the depth list shows it.
    fn depth(&self) -> i8 {
        self.curve().map_or(-4, |w| w.depth())
    }

    fn kind(&self) -> Option<WhammyKind> {
        self.curve().and_then(|w| w.kind(self.continues))
    }

    /// What OK sends.
    pub(crate) fn params(&self) -> Value {
        json!({"points": self.points.iter().map(|p| json!({"pos": p.pos, "value": p.value})).collect::<Vec<_>>()})
    }
}

impl GuitarioApp {
    /// Effects › Whammy Bar…: needs the cursor on a beat with notes on a string track.
    pub fn open_whammy(&mut self) {
        let c = self.session.cursor();
        let voice =
            self.session.score().tracks.get(c.track).filter(|t| !t.is_drums()).and_then(|t| t.measures.get(c.measure)).and_then(|m| m.voices.get(c.voice));
        let Some(beat) = voice.and_then(|v| v.beats.get(c.beat)).filter(|b| !b.rest) else {
            self.status = "Put the cursor on a beat with notes on a string track to use the whammy bar.".into();
            self.status_is_error = true;
            return;
        };
        let before = c.beat.checked_sub(1).and_then(|i| voice.and_then(|v| v.beats.get(i))).and_then(|b| b.whammy);
        self.whammy = WhammyDialog {
            open: true,
            points: beat.whammy.map_or_else(start_curve, |w| w.points().to_vec()),
            has_whammy: beat.whammy.is_some(),
            continues: before.is_some(),
        };
    }

    pub(crate) fn whammy_dialog(&mut self, ctx: &egui::Context) {
        if !self.whammy.open {
            return;
        }
        let (mut apply, mut remove, mut cancel) = (false, false, false);
        let d = &mut self.whammy;
        egui::Modal::new(egui::Id::new("whammy-dialog")).show(ctx, |ui| {
            ui.set_width(430.0);
            ui.heading("Whammy Bar");
            ui.add_space(6.0);
            // The shapes: each fills the curve at the chosen depth.
            let current = d.kind();
            let depth = d.depth();
            ui.horizontal_wrapped(|ui| {
                for kind in WhammyKind::ALL {
                    let usable = !matches!(kind, WhammyKind::Hold | WhammyKind::Return) || d.continues;
                    let on = current == Some(kind);
                    if ui
                        .add_enabled(usable, egui::Button::new(kind.label()).selected(on))
                        .on_disabled_hover_text("Use the whammy bar on the beat before first")
                        .clicked()
                    {
                        d.points = Whammy::preset(kind, depth).points().to_vec();
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Pitch");
                let mut q = depth;
                egui::ComboBox::from_id_salt("whammy-depth").selected_text(whammy_label(q)).show_ui(ui, |ui| {
                    for v in (WHAMMY_MIN..=WHAMMY_MAX).rev().filter(|v| *v != 0) {
                        ui.selectable_value(&mut q, v, whammy_label(v));
                    }
                });
                if q != depth
                    && let Some(w) = d.curve()
                {
                    d.points = w.with_depth(q).points().to_vec();
                }
                ui.label(egui::RichText::new(current.map_or("custom curve", WhammyKind::label)).weak());
            });
            ui.add_space(6.0);
            graph(ui, d);
            ui.label(egui::RichText::new("Drag a point. Double-click the graph to add one, a point to remove it.").weak().small());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let valid = d.curve().is_some();
                if ui.add_enabled(valid, egui::Button::new("OK")).clicked() || (valid && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    apply = true;
                }
                if ui.add_enabled(d.has_whammy, egui::Button::new("Remove")).clicked() {
                    remove = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        });
        let request = if apply {
            Some(self.whammy.params())
        } else if remove {
            Some(json!({"points": null}))
        } else {
            None
        };
        if let Some(params) = request {
            if self.run("beat.setWhammy", params).is_some() {
                self.whammy.open = false;
            }
        } else if cancel {
            self.whammy.open = false;
        }
    }
}

/// The curve as a graph of blue points: the beat's length across, an octave down to an octave up,
/// the written pitch on the middle line.
fn graph(ui: &mut egui::Ui, d: &mut WhammyDialog) {
    use egui::{Color32, Rect, Sense, Stroke, pos2, vec2};
    let (rect, bg) = ui.allocate_exact_size(vec2(ui.available_width(), 220.0), Sense::click());
    let inner = rect.shrink2(vec2(14.0, 12.0));
    let span = f32::from(WHAMMY_MAX) - f32::from(WHAMMY_MIN);
    let y_of = |v: f32| inner.bottom() - inner.height() * (v - f32::from(WHAMMY_MIN)) / span;
    let to_screen = |p: WhammyPoint| pos2(inner.left() + inner.width() * f32::from(p.pos) / f32::from(WHAMMY_END), y_of(f32::from(p.value)));
    let from_screen = |pt: egui::Pos2| {
        let pos = ((pt.x - inner.left()) / inner.width() * f32::from(WHAMMY_END) / f32::from(STEP)).round() as i32 * i32::from(STEP);
        let value = ((inner.bottom() - pt.y) / inner.height() * span + f32::from(WHAMMY_MIN)).round() as i32;
        (pos, value)
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, Color32::from_rgb(0x0c, 0x0e, 0x12));
    let grid = Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2d, 0x38));
    let strong = Stroke::new(1.0, Color32::from_rgb(0x4a, 0x4f, 0x63));
    for i in 0..=12u8 {
        let x = inner.left() + inner.width() * f32::from(i) / 12.0;
        painter.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], grid);
    }
    for v in (WHAMMY_MIN..=WHAMMY_MAX).step_by(4) {
        let y = y_of(f32::from(v));
        painter.line_segment(
            [pos2(inner.left(), y), pos2(inner.right(), y)],
            if v == 0 {
                Stroke::new(1.5, Color32::from_gray(0x80))
            } else if v % 8 == 0 {
                strong
            } else {
                grid
            },
        );
        if v % 8 == 0 && v != 0 {
            painter.text(pos2(rect.left() + 2.0, y), egui::Align2::LEFT_BOTTOM, whammy_label(v), egui::FontId::proportional(9.0), Color32::from_gray(0x90));
        }
    }
    let line: Vec<egui::Pos2> = d.points.iter().map(|p| to_screen(*p)).collect();
    painter.add(egui::Shape::line(line.clone(), Stroke::new(2.0, Color32::from_rgb(0x56, 0x8f, 0xff))));
    // Points last, over the background's click, so a double-click on one removes it.
    let mut remove = None;
    let mut moved = None;
    for (i, c) in line.iter().enumerate() {
        let hit = Rect::from_center_size(*c, vec2(16.0, 16.0));
        let r = ui.interact(hit, ui.id().with(("whammy-point", i)), Sense::click_and_drag());
        let hot = r.hovered() || r.dragged();
        painter.circle(*c, if hot { 6.0 } else { 5.0 }, Color32::from_rgb(0x56, 0x8f, 0xff), Stroke::new(1.5, Color32::WHITE));
        if r.dragged()
            && let Some(pt) = r.interact_pointer_pos()
        {
            moved = Some((i, from_screen(pt)));
        }
        if r.double_clicked() {
            remove = Some(i);
        }
    }
    if let Some((i, (pos, value))) = moved {
        move_point(&mut d.points, i, pos, value);
    }
    if let Some(i) = remove {
        remove_point(&mut d.points, i);
    } else if bg.double_clicked()
        && let Some(pt) = bg.interact_pointer_pos()
    {
        let (pos, _) = from_screen(pt);
        if let Ok(pos) = u8::try_from(pos.clamp(0, i32::from(WHAMMY_END))) {
            add_point(&mut d.points, pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    fn p(pos: u8, value: i8) -> WhammyPoint {
        WhammyPoint::new(pos, value)
    }

    fn app_with_note(fret: u64) -> GuitarioApp {
        let mut s = Session::new();
        s.run("note.setFret", &json!({"fret": fret})).unwrap();
        GuitarioApp::new(s, None)
    }

    fn whammy_on_first_beat(app: &GuitarioApp) -> Option<Whammy> {
        app.session.score().tracks[0].measures[0].voices[0].beats[0].whammy
    }

    #[test]
    fn points_stay_in_order_and_in_range() {
        let mut v = vec![p(0, 0), p(45, -4), p(60, -4)];
        move_point(&mut v, 1, 80, -99);
        assert_eq!(v[1], p(59, WHAMMY_MIN), "an inner point stops before the end and at the bottom");
        move_point(&mut v, 1, -5, 99);
        assert_eq!(v[1], p(1, WHAMMY_MAX));
        move_point(&mut v, 0, 40, -6);
        assert_eq!(v[0], p(0, -6), "the first point only moves up and down");
        let before = v.clone();
        move_point(&mut v, 9, 10, 10);
        assert_eq!(v, before);
    }

    #[test]
    fn a_point_is_added_on_the_curve_and_removed_unless_it_is_an_end() {
        let mut v = vec![p(0, 0), p(60, -8)];
        assert!(add_point(&mut v, 30));
        assert_eq!(v, [p(0, 0), p(30, -4), p(60, -8)]);
        assert!(!add_point(&mut v, 30) && !add_point(&mut v, 0) && !add_point(&mut v, 60));
        assert!(!remove_point(&mut v, 0) && !remove_point(&mut v, 2));
        assert!(remove_point(&mut v, 1));
        assert!(!remove_point(&mut v, 1), "two points stay");
    }

    #[test]
    fn the_window_starts_on_a_dive_and_applies_what_it_shows() {
        let mut app = app_with_note(7);
        app.open_whammy();
        assert!(app.whammy.open && !app.whammy.has_whammy && !app.whammy.continues);
        assert_eq!(app.whammy.kind(), Some(WhammyKind::Dive));
        app.whammy.points = Whammy::preset(WhammyKind::Dip, -12).points().to_vec();
        assert!(app.run("beat.setWhammy", app.whammy.params()).is_some());
        assert_eq!(whammy_on_first_beat(&app), Some(Whammy::preset(WhammyKind::Dip, -12)));
        app.open_whammy();
        assert!(app.whammy.has_whammy);
        assert_eq!((app.whammy.kind(), app.whammy.depth()), (Some(WhammyKind::Dip), -12));
        assert!(app.run("beat.setWhammy", json!({"points": null})).is_some());
        assert_eq!(whammy_on_first_beat(&app), None);
    }

    #[test]
    fn a_curve_that_never_moves_cannot_be_applied() {
        let mut app = app_with_note(7);
        app.open_whammy();
        app.whammy.points = vec![p(0, 0), p(60, 0)];
        assert!(app.whammy.curve().is_none());
        assert!(app.run("beat.setWhammy", app.whammy.params()).is_none());
        assert_eq!(whammy_on_first_beat(&app), None);
    }

    #[test]
    fn on_a_rest_nothing_opens_and_the_status_says_why() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.open_whammy();
        assert!(!app.whammy.open && app.status_is_error && app.status.contains("whammy"), "{}", app.status);
    }

    #[test]
    fn the_window_survives_frames_without_a_click() {
        let mut app = app_with_note(5);
        app.open_whammy();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.whammy_dialog(ui.ctx())).textures_delta.clear();
        }
        assert!(app.whammy.open);
        assert_eq!(whammy_on_first_beat(&app), None);
    }
}
