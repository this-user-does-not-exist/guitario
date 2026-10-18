//! Effects › Bend… (B): the Bend window. Pick one of Guitar Pro's seven types and a height from a
//! quarter tone to three tones, or drag the points of the curve: the horizontal axis is the note's
//! length, the vertical axis the pitch. OK sends the curve to `note.setBend` as one undo step.

use crate::GuitarioApp;
use crate::bend_draw::bend_label;
use guitario_score::{BEND_END, BEND_MAX, BEND_MAX_POINTS, Bend, BendKind, BendPoint, NoteKind};
use serde_json::{Value, json};

/// How far apart the positions the editor offers are: twelve steps across the note.
const STEP: u8 = BEND_END / 12;

#[derive(Default)]
pub(crate) struct BendDialog {
    pub(crate) open: bool,
    /// The curve being edited, as typed: nothing is tidied until OK.
    points: Vec<BendPoint>,
    /// The note already has a bend, so it can be removed.
    has_bend: bool,
    /// The note continues the one before it, so a curve that starts bent is a Hold or a Release.
    tied: bool,
}

/// A fresh curve is a full step.
fn start_curve() -> Vec<BendPoint> {
    Bend::to(4).points().to_vec()
}

/// Moves point `i` to `pos` and `value`, keeping the curve a function of time: the first point stays
/// at the attack and the last at the end, the rest between their neighbours.
pub(crate) fn move_point(points: &mut [BendPoint], i: usize, pos: i32, value: i32) {
    let last = points.len().saturating_sub(1);
    let lo = if i == 0 { 0 } else { points.get(i - 1).map_or(0, |p| i32::from(p.pos) + 1) };
    let hi = if i >= last { i32::from(BEND_END) } else { points.get(i + 1).map_or(i32::from(BEND_END), |p| i32::from(p.pos) - 1) };
    let pos = if i == 0 {
        0
    } else if i >= last {
        i32::from(BEND_END)
    } else {
        pos.clamp(lo, hi.max(lo))
    };
    if let Some(p) = points.get_mut(i) {
        p.pos = u8::try_from(pos.clamp(0, i32::from(BEND_END))).unwrap_or(0);
        p.value = u8::try_from(value.clamp(0, i32::from(BEND_MAX))).unwrap_or(0);
    }
}

/// Adds a point at `pos` (not at an end, not where one is) on the curve as it runs there.
pub(crate) fn add_point(points: &mut Vec<BendPoint>, pos: u8) -> bool {
    if points.len() >= BEND_MAX_POINTS || pos == 0 || pos >= BEND_END || points.iter().any(|p| p.pos == pos) {
        return false;
    }
    let value = Bend::from_points(points).map_or(0, |b| b.value_at(f32::from(pos)).round() as u8);
    let at = points.iter().position(|p| p.pos > pos).unwrap_or(points.len());
    points.insert(at, BendPoint::new(pos, value));
    true
}

/// Removes an interior point; the ends and a two-point curve stay.
pub(crate) fn remove_point(points: &mut Vec<BendPoint>, i: usize) -> bool {
    if points.len() <= 2 || i == 0 || i + 1 >= points.len() {
        return false;
    }
    points.remove(i);
    true
}

impl BendDialog {
    fn curve(&self) -> Option<Bend> {
        Bend::from_points(&self.points)
    }

    fn peak(&self) -> u8 {
        self.points.iter().map(|p| p.value).max().unwrap_or(0)
    }

    fn kind(&self) -> Option<BendKind> {
        self.curve().and_then(|b| b.kind(self.tied))
    }

    /// What OK sends.
    pub(crate) fn params(&self) -> Value {
        json!({"points": self.points.iter().map(|p| json!({"pos": p.pos, "value": p.value})).collect::<Vec<_>>()})
    }
}

impl GuitarioApp {
    /// Effects › Bend…: needs the cursor on a note of a string track.
    pub fn open_bend(&mut self) {
        let c = self.session.cursor();
        let track = self.session.score().tracks.get(c.track);
        let note = track
            .filter(|t| !t.is_drums())
            .and_then(|t| t.measures.get(c.measure))
            .and_then(|m| m.voices.get(c.voice))
            .and_then(|v| v.beats.get(c.beat))
            .and_then(|b| b.notes.iter().find(|n| usize::from(n.string) == c.string))
            .copied();
        let Some(note) = note else {
            self.status = "Put the cursor on a note of a string track to bend it.".into();
            self.status_is_error = true;
            return;
        };
        self.bend = BendDialog {
            open: true,
            points: note.effects.bend.map_or_else(start_curve, |b| b.points().to_vec()),
            has_bend: note.effects.bend.is_some(),
            tied: note.kind == NoteKind::Tie,
        };
    }

    pub(crate) fn bend_dialog(&mut self, ctx: &egui::Context) {
        if !self.bend.open {
            return;
        }
        let (mut apply, mut remove, mut cancel) = (false, false, false);
        let d = &mut self.bend;
        egui::Modal::new(egui::Id::new("bend-dialog")).show(ctx, |ui| {
            ui.set_width(430.0);
            ui.heading("Bend");
            ui.add_space(6.0);
            // The seven types: each fills the curve with its shape at the chosen height.
            let current = d.kind();
            let height = d.peak().max(1);
            ui.horizontal_wrapped(|ui| {
                for kind in BendKind::ALL {
                    let usable = !matches!(kind, BendKind::Hold | BendKind::Release) || d.tied;
                    let on = current == Some(kind);
                    if ui.add_enabled(usable, egui::Button::new(kind.label()).selected(on)).on_disabled_hover_text("Tie the note to a bent one first").clicked()
                    {
                        d.points = Bend::preset(kind, height).points().to_vec();
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Pitch");
                let mut q = d.peak().max(1);
                let shown = bend_label(q);
                egui::ComboBox::from_id_salt("bend-height").selected_text(shown).show_ui(ui, |ui| {
                    for v in 1..=BEND_MAX {
                        ui.selectable_value(&mut q, v, bend_label(v));
                    }
                });
                if q != d.peak().max(1)
                    && let Some(b) = d.curve()
                {
                    d.points = b.with_peak(q).points().to_vec();
                }
                ui.label(egui::RichText::new(current.map_or("custom curve", BendKind::label)).weak());
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
                if ui.add_enabled(d.has_bend, egui::Button::new("Remove")).clicked() {
                    remove = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
            });
        });
        let request = if apply {
            Some(("note.setBend", self.bend.params()))
        } else if remove {
            Some(("note.setBend", json!({"points": null})))
        } else {
            None
        };
        if let Some((id, params)) = request {
            if self.run(id, params).is_some() {
                self.bend.open = false;
            }
        } else if cancel {
            self.bend.open = false;
        }
    }
}

/// The curve as a graph of blue points: the note's length across, a quarter tone to three tones up.
fn graph(ui: &mut egui::Ui, d: &mut BendDialog) {
    use egui::{Color32, Rect, Sense, Stroke, pos2, vec2};
    let (rect, bg) = ui.allocate_exact_size(vec2(ui.available_width(), 170.0), Sense::click());
    let inner = rect.shrink2(vec2(14.0, 12.0));
    let to_screen = |p: BendPoint| {
        pos2(inner.left() + inner.width() * f32::from(p.pos) / f32::from(BEND_END), inner.bottom() - inner.height() * f32::from(p.value) / f32::from(BEND_MAX))
    };
    let from_screen = |pt: egui::Pos2| {
        let pos = ((pt.x - inner.left()) / inner.width() * f32::from(BEND_END) / f32::from(STEP)).round() as i32 * i32::from(STEP);
        let value = ((inner.bottom() - pt.y) / inner.height() * f32::from(BEND_MAX)).round() as i32;
        (pos, value)
    };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, Color32::from_rgb(0x0c, 0x0e, 0x12));
    let grid = Stroke::new(1.0, Color32::from_rgb(0x2a, 0x2d, 0x38));
    let strong = Stroke::new(1.0, Color32::from_rgb(0x4a, 0x4f, 0x63));
    for i in 0..=12u8 {
        let x = inner.left() + inner.width() * f32::from(i) / 12.0;
        painter.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], grid);
        let y = inner.bottom() - inner.height() * f32::from(i) / f32::from(BEND_MAX);
        painter.line_segment([pos2(inner.left(), y), pos2(inner.right(), y)], if i % 4 == 0 { strong } else { grid });
        if i % 4 == 0 && i > 0 {
            painter.text(pos2(rect.left() + 2.0, y), egui::Align2::LEFT_BOTTOM, bend_label(i), egui::FontId::proportional(9.0), Color32::from_gray(0x90));
        }
    }
    let line: Vec<egui::Pos2> = d.points.iter().map(|p| to_screen(*p)).collect();
    painter.add(egui::Shape::line(line.clone(), Stroke::new(2.0, Color32::from_rgb(0x56, 0x8f, 0xff))));
    // Points last, over the background's click, so a double-click on one removes it.
    let mut remove = None;
    let mut moved = None;
    for (i, c) in line.iter().enumerate() {
        let hit = Rect::from_center_size(*c, vec2(16.0, 16.0));
        let r = ui.interact(hit, ui.id().with(("bend-point", i)), Sense::click_and_drag());
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
        if let Ok(pos) = u8::try_from(pos.clamp(0, i32::from(BEND_END))) {
            add_point(&mut d.points, pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    fn p(pos: u8, value: u8) -> BendPoint {
        BendPoint::new(pos, value)
    }

    fn app_with_note(fret: u64) -> GuitarioApp {
        let mut s = Session::new();
        s.run("note.setFret", &json!({"fret": fret})).unwrap();
        GuitarioApp::new(s, None)
    }

    fn bend_on_first_note(app: &GuitarioApp) -> Option<Bend> {
        app.session.score().tracks[0].measures[0].voices[0].beats[0].notes[0].effects.bend
    }

    #[test]
    fn points_stay_in_order_and_the_ends_stay_at_the_ends() {
        let mut v = vec![p(0, 0), p(30, 4), p(60, 4)];
        move_point(&mut v, 1, 80, 99);
        assert_eq!(v[1], p(59, 12), "an inner point stops before the end and at the top");
        move_point(&mut v, 1, -5, -3);
        assert_eq!(v[1], p(1, 0));
        move_point(&mut v, 0, 40, 6);
        assert_eq!(v[0], p(0, 6), "the first point only moves up and down");
        move_point(&mut v, 2, 10, 2);
        assert_eq!(v[2], p(60, 2), "so does the last");
        let before = v.clone();
        move_point(&mut v, 9, 10, 10);
        assert_eq!(v, before, "a point that is not there is ignored");
    }

    #[test]
    fn a_point_is_added_on_the_curve_and_removed_unless_it_is_an_end() {
        let mut v = vec![p(0, 0), p(60, 8)];
        assert!(add_point(&mut v, 30));
        assert_eq!(v, [p(0, 0), p(30, 4), p(60, 8)], "it lands on the line");
        assert!(!add_point(&mut v, 30) && !add_point(&mut v, 0) && !add_point(&mut v, 60), "not twice, not on an end");
        while v.len() < BEND_MAX_POINTS {
            let pos = (1..60).find(|x| v.iter().all(|p| p.pos != *x)).unwrap();
            assert!(add_point(&mut v, pos));
        }
        assert!(!add_point(&mut v, 59) || v.len() <= BEND_MAX_POINTS);
        assert_eq!(v.len(), BEND_MAX_POINTS);
        assert!(!remove_point(&mut v, 0) && !remove_point(&mut v, BEND_MAX_POINTS - 1));
        assert!(remove_point(&mut v, 1));
        let mut two = vec![p(0, 0), p(60, 4)];
        assert!(!remove_point(&mut two, 1));
    }

    #[test]
    fn the_window_starts_on_a_full_step_and_applies_what_it_shows() {
        let mut app = app_with_note(7);
        app.open_bend();
        assert!(app.bend.open && !app.bend.has_bend);
        assert_eq!(app.bend.curve(), Some(Bend::to(4)));
        assert_eq!(app.bend.kind(), Some(BendKind::Bend));
        // Pick a type and a height, then OK.
        app.bend.points = Bend::preset(BendKind::BendRelease, 6).points().to_vec();
        let (id, params) = ("note.setBend", app.bend.params());
        assert!(app.run(id, params).is_some());
        assert_eq!(bend_on_first_note(&app), Some(Bend::preset(BendKind::BendRelease, 6)));
        // Opened again it shows the bend the note has, which can be removed.
        app.open_bend();
        assert!(app.bend.has_bend);
        assert_eq!(app.bend.kind(), Some(BendKind::BendRelease));
        assert_eq!(app.bend.peak(), 6);
        assert!(app.run("note.setBend", json!({"points": null})).is_some());
        assert_eq!(bend_on_first_note(&app), None);
    }

    #[test]
    fn a_curve_that_never_leaves_the_string_cannot_be_applied() {
        let mut app = app_with_note(7);
        app.open_bend();
        app.bend.points = vec![p(0, 0), p(60, 0)];
        assert!(app.bend.curve().is_none());
        assert!(app.run("note.setBend", app.bend.params()).is_none(), "the engine refuses it too");
        assert!(app.status_is_error);
        assert_eq!(bend_on_first_note(&app), None);
    }

    #[test]
    fn without_a_note_nothing_opens_and_the_status_says_why() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.open_bend();
        assert!(!app.bend.open && app.status_is_error && app.status.contains("bend"), "{}", app.status);
    }

    #[test]
    fn the_window_survives_frames_without_a_click() {
        let mut app = app_with_note(5);
        app.open_bend();
        let ctx = egui::Context::default();
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.bend_dialog(ui.ctx())).textures_delta.clear();
        }
        assert!(app.bend.open);
        assert_eq!(bend_on_first_note(&app), None);
    }
}
