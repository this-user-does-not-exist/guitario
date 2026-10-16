//! File › Page Setup…: paper, orientation and margins. The dialog edits a copy and applies it with
//! `score.setPage` on OK, so the change is one undo step and the page view, the PDF and the
//! printer all follow it.

use crate::GuitarioApp;
use egui::{Color32, Pos2, Rect, Stroke, vec2};
use guitario_score::{PageSetup, Paper};
use serde_json::json;

#[derive(Default)]
pub(crate) struct PageSetupDialog {
    pub open: bool,
    draft: PageSetup,
}

impl PageSetupDialog {
    /// The `score.setPage` request that makes the score's page what the dialog shows.
    pub(crate) fn params(&self) -> serde_json::Value {
        let d = &self.draft;
        json!({
            "paper": d.paper.name().to_lowercase(), "landscape": d.landscape,
            "marginLeft": d.margin_left, "marginRight": d.margin_right, "marginTop": d.margin_top, "marginBottom": d.margin_bottom,
        })
    }
}

/// A small sheet of paper with its margins drawn in, so a margin can be judged by eye.
fn sheet(ui: &mut egui::Ui, page: &PageSetup) {
    let (w, h) = page.size_mm();
    let height = 120.0;
    let width = (height * w / h).clamp(60.0, 170.0);
    let (rect, _) = ui.allocate_exact_size(vec2(170.0, height), egui::Sense::hover());
    let paper = Rect::from_center_size(rect.center(), vec2(width, height - 4.0));
    let p = ui.painter();
    p.rect_filled(paper.translate(vec2(2.0, 3.0)), 0.0, Color32::from_black_alpha(80));
    p.rect_filled(paper, 0.0, Color32::WHITE);
    let sx = paper.width() / w;
    let sy = paper.height() / h;
    let inner = Rect::from_min_max(
        Pos2::new(paper.left() + f32::from(page.margin_left) * sx, paper.top() + f32::from(page.margin_top) * sy),
        Pos2::new(paper.right() - f32::from(page.margin_right) * sx, paper.bottom() - f32::from(page.margin_bottom) * sy),
    );
    if inner.width() > 0.0 && inner.height() > 0.0 {
        p.rect_stroke(inner, 0.0, Stroke::new(1.0, Color32::from_rgb(0x2f, 0x7b, 0xf5)), egui::StrokeKind::Inside);
    }
}

impl GuitarioApp {
    /// File › Page Setup…: starts from the score's page.
    pub fn open_page_setup(&mut self) {
        self.page_setup = PageSetupDialog { open: true, draft: self.session.score().page };
    }

    pub(crate) fn page_setup_dialog(&mut self, ctx: &egui::Context) {
        if !self.page_setup.open {
            return;
        }
        let mut ok = false;
        let mut cancel = false;
        let d = &mut self.page_setup.draft;
        let problem = d.check().err();
        egui::Modal::new(egui::Id::new("page-setup")).show(ctx, |ui| {
            ui.set_width(430.0);
            ui.heading("Page Setup");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    egui::Grid::new("page-setup-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("Paper");
                        egui::ComboBox::from_id_salt("page-paper").selected_text(d.paper.name()).show_ui(ui, |ui| {
                            for paper in Paper::ALL {
                                let (w, h) = paper.size_mm();
                                ui.selectable_value(&mut d.paper, paper, format!("{}  ({w:.0} x {h:.0} mm)", paper.name()));
                            }
                        });
                        ui.end_row();
                        ui.label("Orientation");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut d.landscape, false, "Portrait");
                            ui.radio_value(&mut d.landscape, true, "Landscape");
                        });
                        ui.end_row();
                        for (label, value) in [
                            ("Left margin", &mut d.margin_left),
                            ("Right margin", &mut d.margin_right),
                            ("Top margin", &mut d.margin_top),
                            ("Bottom margin", &mut d.margin_bottom),
                        ] {
                            ui.label(label);
                            ui.add(egui::DragValue::new(value).range(0..=PageSetup::MAX_MARGIN).suffix(" mm"));
                            ui.end_row();
                        }
                    });
                });
                ui.add_space(12.0);
                sheet(ui, d);
            });
            ui.add_space(6.0);
            match &problem {
                Some(why) => {
                    ui.colored_label(Color32::from_rgb(0xff, 0x7a, 0x6a), why);
                }
                None => {
                    let (w, h) = d.size_mm();
                    let printable = (w - f32::from(d.margin_left) - f32::from(d.margin_right), h - f32::from(d.margin_top) - f32::from(d.margin_bottom));
                    ui.label(egui::RichText::new(format!("Printable area: {:.0} x {:.0} mm", printable.0, printable.1)).small());
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(problem.is_none(), egui::Button::new("OK")).clicked() {
                    ok = true;
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                }
                if ui.button("Defaults").clicked() {
                    *d = PageSetup::default();
                }
            });
        });
        if ok {
            let params = self.page_setup.params();
            if self.run("score.setPage", params).is_some() {
                self.page_setup.open = false;
                // A preview that is open lays its pages out again.
                self.preview.invalidate();
            }
        } else if cancel {
            self.page_setup.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    fn pump(app: &mut GuitarioApp) {
        let ctx = egui::Context::default();
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| app.page_setup_dialog(ui.ctx())).textures_delta.clear();
        }
    }

    #[test]
    fn the_dialog_starts_from_the_score_and_applies_one_undo_step() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.session.run("score.setPage", &json!({"paper": "legal"})).unwrap();
        app.open_page_setup();
        assert!(app.page_setup.open && app.page_setup.draft.paper == Paper::Legal);
        app.page_setup.draft.paper = Paper::Letter;
        app.page_setup.draft.landscape = true;
        app.page_setup.draft.margin_left = 25;
        let before = app.session.can_undo();
        let params = app.page_setup.params();
        assert!(app.run("score.setPage", params).is_some());
        assert!(before && app.session.score().page == PageSetup { paper: Paper::Letter, landscape: true, margin_left: 25, ..PageSetup::default() });
        app.session.run("edit.undo", &serde_json::Value::Null).unwrap();
        assert_eq!(app.session.score().page.paper, Paper::Legal);
    }

    #[test]
    fn a_page_the_layout_cannot_use_is_not_applied_and_the_dialog_stays() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.open_page_setup();
        app.page_setup.draft = PageSetup { paper: Paper::A5, margin_left: 50, margin_right: 50, ..PageSetup::default() };
        assert!(app.page_setup.draft.check().is_err());
        pump(&mut app);
        assert!(app.page_setup.open, "frames alone never close or apply the dialog");
        assert_eq!(app.session.score().page, PageSetup::default());
        let params = app.page_setup.params();
        assert!(app.run("score.setPage", params).is_none(), "the engine refuses it too");
        assert!(app.status_is_error);
    }

    /// Types a `5` into the window, as a guitarist entering fret 5 would.
    fn type_five(app: &mut GuitarioApp) {
        let ctx = egui::Context::default();
        for frame in 0..5 {
            let key = egui::Event::Key { key: egui::Key::Num5, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE };
            let raw = egui::RawInput { events: if frame == 3 { vec![key] } else { vec![] }, ..egui::RawInput::default() };
            ctx.run_ui(raw, |ui| app.show(ui)).textures_delta.clear();
        }
    }

    #[test]
    fn no_dialog_lets_keys_through_to_the_score() {
        let first_fret = |app: &GuitarioApp| app.session.score().tracks[0].measures[0].voices[0].beats[0].notes.first().map(|n| n.fret);

        // Control: with nothing open the digit makes a note.
        let mut app = GuitarioApp::new(Session::new(), None);
        type_five(&mut app);
        assert_eq!(first_fret(&app), Some(5));

        for (name, open) in [
            ("page setup", GuitarioApp::open_page_setup as fn(&mut GuitarioApp)),
            ("print preview", GuitarioApp::open_preview),
            ("print", GuitarioApp::open_print),
        ] {
            let mut app = GuitarioApp::new(Session::new(), None);
            open(&mut app);
            type_five(&mut app);
            assert_eq!(first_fret(&app), None, "a digit reached the score behind the {name} dialog");
        }
    }
}
