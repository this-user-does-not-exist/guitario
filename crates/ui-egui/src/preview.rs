//! File › Print Preview…: the pages exactly as the PDF and the printer get them. They come from the
//! same [`PrintSet`] and the same painting as the PDF, drawn small through a visual transform, so
//! what the preview shows is what the paper shows.

use crate::GuitarioApp;
use crate::page::PrintSet;
use egui::{Color32, Pos2, Rect, emath::TSTransform, vec2};

pub(crate) struct Preview {
    pub open: bool,
    all_tracks: bool,
    /// All tracks stacked in every system, as a full score.
    together: bool,
    notation: bool,
    /// The page on show, 0-based.
    page: usize,
    /// The laid-out pages, or why there are none; `None` until built or after something changed.
    set: Option<Result<PrintSet, String>>,
}

impl Default for Preview {
    fn default() -> Self {
        Preview { open: false, all_tracks: false, together: false, notation: true, page: 0, set: None }
    }
}

impl Preview {
    /// The pages are laid out again on the next frame.
    pub(crate) fn invalidate(&mut self) {
        self.set = None;
    }

    fn pages(&self) -> usize {
        match &self.set {
            Some(Ok(set)) => set.pages(),
            _ => 0,
        }
    }

    /// Moves to page `to` (0-based), staying inside the document.
    fn go(&mut self, to: usize) {
        self.page = to.min(self.pages().saturating_sub(1));
    }
}

/// What the preview does after a frame.
#[derive(Default)]
struct Wants {
    close: bool,
    print: bool,
    setup: bool,
    rebuild: bool,
    page: Option<usize>,
}

impl GuitarioApp {
    /// File › Print Preview…: what the page view shows: the track on screen, or every track as a
    /// full score in the multitrack view; with the page view's notation setting.
    pub fn open_preview(&mut self) {
        let score = self.view.multitrack && self.session.score().tracks.len() > 1;
        self.preview = Preview { open: true, all_tracks: score, together: score, notation: true, ..Preview::default() };
    }

    pub(crate) fn preview_dialog(&mut self, ctx: &egui::Context) {
        if !self.preview.open {
            return;
        }
        if self.preview.set.is_none() {
            let count = self.session.score().tracks.len();
            let tracks: Vec<usize> = if self.preview.all_tracks { (0..count).collect() } else { vec![self.session.cursor().track] };
            let built = PrintSet::new(self.session.score(), &tracks, self.preview.notation, self.preview.together);
            self.preview.set = Some(built);
            self.preview.go(self.preview.page);
        }
        let screen = ctx.content_rect();
        let (width, height) = ((screen.width() - 80.0).clamp(480.0, 1200.0), (screen.height() - 80.0).clamp(380.0, 1000.0));
        let (score, fonts) = (self.session.score(), self.fonts);
        let tracks = score.tracks.len();
        let this_track = score.tracks.get(self.session.cursor().track).map(|t| t.name.clone()).unwrap_or_default();
        let mut all = self.preview.all_tracks;
        let mut together = self.preview.together;
        let mut notation = self.preview.notation;
        let (page, pages) = (self.preview.page, self.preview.pages());
        let mut wants = Wants::default();
        let set = &self.preview.set;
        let sheet = score.page;

        egui::Modal::new(egui::Id::new("print-preview")).show(ctx, |ui| {
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.heading("Print Preview");
                ui.label(egui::RichText::new(format!("{} {}", sheet.paper.name(), if sheet.landscape { "landscape" } else { "portrait" })).weak());
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut all, false, format!("This track ({this_track})"));
                ui.add_enabled_ui(tracks > 1, |ui| ui.radio_value(&mut all, true, format!("All tracks ({tracks})")));
                ui.add_enabled(all, egui::Checkbox::new(&mut together, "As a full score"));
                ui.separator();
                ui.checkbox(&mut notation, "Standard notation");
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(page > 0, egui::Button::new("\u{25c0}")).clicked() {
                    wants.page = Some(page - 1);
                }
                ui.label(if pages == 0 { "No pages".to_string() } else { format!("Page {} of {pages}", page + 1) });
                if ui.add_enabled(page + 1 < pages, egui::Button::new("\u{25b6}")).clicked() {
                    wants.page = Some(page + 1);
                }
                ui.separator();
                if ui.button("Page Setup…").clicked() {
                    wants.setup = true;
                }
                if ui.add_enabled(pages > 0, egui::Button::new("Print…")).clicked() {
                    wants.print = true;
                }
                if ui.button("Close").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    wants.close = true;
                }
            });
            ui.input(|i| {
                if i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::PageDown) {
                    wants.page = Some(page.saturating_add(1));
                }
                if i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::PageUp) {
                    wants.page = Some(page.saturating_sub(1));
                }
                if i.key_pressed(egui::Key::Home) {
                    wants.page = Some(0);
                }
                if i.key_pressed(egui::Key::End) {
                    wants.page = Some(usize::MAX);
                }
            });
            ui.add_space(4.0);
            let (area, _) = ui.allocate_exact_size(vec2(width, (height - 110.0).max(240.0)), egui::Sense::hover());
            ui.painter().rect_filled(area, 4.0, crate::theme::tokens().workspace);
            match set {
                Some(Ok(set)) => {
                    if let Some((layout, _)) = set.page(page) {
                        let m = layout.metrics;
                        let avail = area.shrink(14.0);
                        let scale = (avail.width() / m.page_w).min(avail.height() / m.page_h).clamp(0.05, 2.0);
                        let origin = avail.center() - vec2(m.page_w, m.page_h) * scale / 2.0;
                        ui.with_visual_transform(TSTransform::new(origin.to_vec2(), scale), |ui| {
                            let sheet = Rect::from_min_size(Pos2::ZERO, vec2(m.page_w, m.page_h));
                            // Painted in page coordinates, so the clip is too (the parent's is on screen).
                            let mut painter = ui.painter().clone();
                            painter.set_clip_rect(sheet.expand(8.0));
                            painter.rect_filled(sheet.translate(vec2(3.0, 4.0)), 0.0, Color32::from_black_alpha(70));
                            painter.rect_filled(sheet, 0.0, Color32::WHITE);
                            painter.set_clip_rect(sheet);
                            set.paint(&painter, score, page, fonts);
                        });
                    }
                }
                Some(Err(why)) => {
                    ui.painter().text(area.center(), egui::Align2::CENTER_CENTER, why, egui::FontId::proportional(14.0), Color32::from_rgb(0xff, 0x7a, 0x6a));
                }
                None => {}
            }
        });

        if all != self.preview.all_tracks || together != self.preview.together || notation != self.preview.notation {
            self.preview.all_tracks = all;
            self.preview.together = together;
            self.preview.notation = notation;
            wants.rebuild = true;
        }
        if wants.rebuild {
            self.preview.page = 0;
            self.preview.invalidate();
        } else if let Some(to) = wants.page {
            self.preview.go(to);
        }
        if wants.setup {
            self.open_page_setup();
        }
        if wants.print {
            let (all, together, notation) = (self.preview.all_tracks, self.preview.together, self.preview.notation);
            self.preview.open = false;
            self.open_print_with(all, together, notation);
        } else if wants.close {
            self.preview.open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;
    use serde_json::json;

    fn frames(app: &mut GuitarioApp, n: usize) {
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::definitions().0);
        for _ in 0..n {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                app.preview_dialog(ui.ctx());
                app.page_setup_dialog(ui.ctx());
            })
            .textures_delta
            .clear();
        }
    }

    fn app() -> GuitarioApp {
        let mut s = Session::new();
        s.run("track.add", &json!({"name": "Bass", "tuning": "bass"})).unwrap();
        // Enough bars to need more than one page.
        for _ in 0..80 {
            s.run("measure.append", &serde_json::Value::Null).unwrap();
        }
        GuitarioApp::new(s, None)
    }

    #[test]
    fn opening_lays_out_the_track_on_screen_and_pages_stay_in_range() {
        let mut app = app();
        app.open_preview();
        frames(&mut app, 2);
        let pages = app.preview.pages();
        assert!(app.preview.open && pages > 1, "{pages}");
        app.preview.go(usize::MAX);
        assert_eq!(app.preview.page, pages - 1);
        app.preview.go(0);
        assert_eq!(app.preview.page, 0);
    }

    #[test]
    fn all_tracks_and_a_page_setup_change_lay_the_pages_out_again() {
        let mut app = app();
        app.open_preview();
        frames(&mut app, 2);
        let one = app.preview.pages();
        app.preview.all_tracks = true;
        app.preview.invalidate();
        frames(&mut app, 2);
        assert!(app.preview.pages() > one, "two tracks take more pages than one");
        // Stacked as a full score they share their systems: fewer pages than one after the other.
        let parts = app.preview.pages();
        app.preview.together = true;
        app.preview.invalidate();
        frames(&mut app, 2);
        assert!(app.preview.pages() < parts && app.preview.pages() >= one, "{} vs {parts}", app.preview.pages());
        app.preview.together = false;
        app.preview.invalidate();
        frames(&mut app, 2);
        // The same bars on small landscape paper take more pages than on A4.
        let a4 = app.preview.pages();
        app.session.run("score.setPage", &json!({"paper": "a5", "landscape": true})).unwrap();
        app.preview.invalidate();
        frames(&mut app, 2);
        assert!(app.preview.pages() > a4, "{} vs {a4}", app.preview.pages());
    }

    #[test]
    fn a_score_with_no_such_track_shows_the_reason_not_a_crash() {
        let mut app = app();
        app.open_preview();
        app.preview.set = Some(PrintSet::new(app.session.score(), &[9], true, false));
        frames(&mut app, 2);
        assert!(matches!(app.preview.set, Some(Err(_))) && app.preview.pages() == 0);
        app.preview.go(5);
        assert_eq!(app.preview.page, 0);
    }
}
