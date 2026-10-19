//! File › Stylesheet… (F7): the paper, orientation and margins (Page & Score Format) and the texts
//! printed above the music and at the foot of each page (Header & Footer), laid out as Guitar Pro
//! 8's Stylesheet window. The window edits a copy and applies it with `score.setPage` and
//! `score.setHeaderFooter` on Apply or OK, so the page view, the PDF and the printer all follow it
//! and each is one undo step. File › Page Setup… opens it on the first tab.

use crate::GuitarioApp;
use egui::{Color32, Pos2, Rect, Stroke, vec2};
use guitario_score::{DEFAULT_PAGE_FONT, HeaderFooter, PAGE_TEXT_KEYS, PAGE_TEXT_SIZES, PageSetup, PageText, Paper, TOKENS, TextAlign};
use serde_json::{Map, Value, json};

/// The Stylesheet's tabs that exist so far, in Guitar Pro's order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tab {
    #[default]
    Page,
    HeaderFooter,
}

#[derive(Default)]
pub(crate) struct PageSetupDialog {
    pub open: bool,
    pub tab: Tab,
    draft: PageSetup,
    texts: HeaderFooter,
    /// Header & Footer shows the pages after the first.
    other_pages: bool,
}

impl PageSetupDialog {
    /// The `score.setPage` request that makes the score's page what the dialog shows.
    pub(crate) fn params(&self) -> Value {
        let d = &self.draft;
        json!({
            "paper": d.paper.name().to_lowercase(), "landscape": d.landscape,
            "customWidth": d.custom_width, "customHeight": d.custom_height,
            "marginLeft": d.margin_left, "marginRight": d.margin_right, "marginTop": d.margin_top, "marginBottom": d.margin_bottom,
            "bendPitch": d.bend_pitch,
        })
    }

    /// The `score.setHeaderFooter` request for every text as the dialog shows it.
    pub(crate) fn texts_params(&self) -> Value {
        let items: Map<String, Value> = PAGE_TEXT_KEYS
            .iter()
            .filter_map(|k| {
                let t = self.texts.get(k)?;
                Some((
                    k.to_string(),
                    json!({"on": t.on, "text": t.text, "align": t.align.name(), "font": t.font, "size": t.size, "bold": t.bold, "italic": t.italic}),
                ))
            })
            .collect();
        json!({ "items": items })
    }

    fn problem(&self) -> Option<String> {
        self.draft.check().err().or_else(|| self.texts.check().err())
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

fn page_tab(ui: &mut egui::Ui, d: &mut PageSetup) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            egui::Grid::new("page-setup-grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Format");
                egui::ComboBox::from_id_salt("page-paper").selected_text(d.paper.name()).width(190.0).show_ui(ui, |ui| {
                    for paper in Paper::ALL {
                        let (w, h) = paper.size_mm();
                        ui.selectable_value(&mut d.paper, paper, format!("{}  ({w:.0} x {h:.0} mm)", paper.name()));
                    }
                    ui.selectable_value(&mut d.paper, Paper::Custom, "Custom");
                });
                ui.end_row();
                if d.paper == Paper::Custom {
                    ui.label("Size");
                    ui.horizontal(|ui| {
                        let sides = PageSetup::CUSTOM_SIDES;
                        ui.add(egui::DragValue::new(&mut d.custom_width).range(sides.clone()).suffix(" mm"));
                        ui.label("x");
                        ui.add(egui::DragValue::new(&mut d.custom_height).range(sides).suffix(" mm"));
                    });
                    ui.end_row();
                }
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
    ui.checkbox(&mut d.bend_pitch, "Show the pitch bends reach in standard notation");
    if d.check().is_ok() {
        let (w, h) = d.size_mm();
        let printable = (w - f32::from(d.margin_left) - f32::from(d.margin_right), h - f32::from(d.margin_top) - f32::from(d.margin_bottom));
        ui.label(egui::RichText::new(format!("Printable area: {:.0} x {:.0} mm", printable.0, printable.1)).small());
    }
}

/// Three buttons with the left, centre and right alignment lines drawn on them.
fn align_buttons(ui: &mut egui::Ui, align: &mut TextAlign) {
    let t = crate::theme::tokens();
    ui.spacing_mut().item_spacing.x = 1.0;
    for a in TextAlign::ALL {
        let on = *align == a;
        let (rect, response) = ui.allocate_exact_size(vec2(26.0, 20.0), egui::Sense::click());
        let fill = if on {
            t.accent
        } else if response.hovered() {
            t.control_hover
        } else {
            t.control
        };
        ui.painter().rect_filled(rect, 3.0, fill);
        let ink = Stroke::new(1.2, t.text);
        for (i, w) in [12.0f32, 8.0, 12.0, 8.0].into_iter().enumerate() {
            let y = rect.top() + 5.0 + i as f32 * 3.3;
            let (x0, x1) = match a {
                TextAlign::Left => (rect.left() + 7.0, rect.left() + 7.0 + w),
                TextAlign::Center => (rect.center().x - w / 2.0, rect.center().x + w / 2.0),
                TextAlign::Right => (rect.right() - 7.0 - w, rect.right() - 7.0),
            };
            ui.painter().line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], ink);
        }
        if response.on_hover_text(format!("Align {}", a.name())).clicked() {
            *align = a;
        }
    }
}

/// The font button ("Aa Times New Roman - 23 pt - Regular") and its popup: family, size, bold, italic.
fn font_button(ui: &mut egui::Ui, text: &mut PageText, id: &str) {
    let style = match (text.bold, text.italic) {
        (false, false) => "Regular",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (true, true) => "Bold Italic",
    };
    let label = format!("Aa  {} - {} pt - {style}", text.font, trim_size(text.size));
    let button = ui.add_sized([290.0, 20.0], egui::Button::new(label).truncate());
    egui::Popup::from_toggle_button_response(&button).id(egui::Id::new(("hf-font", id))).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(
        |ui| {
            ui.set_min_width(220.0);
            let preview = crate::fonts::page_text(&text.font, 26.0, text.bold, text.italic);
            let (rect, _) = ui.allocate_exact_size(vec2(220.0, 46.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, Color32::WHITE);
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Title", preview, Color32::BLACK);
            egui::ComboBox::from_id_salt(("hf-family", id)).selected_text(text.font.clone()).width(210.0).show_ui(ui, |ui| {
                for family in [DEFAULT_PAGE_FONT, "Arial"] {
                    ui.selectable_value(&mut text.font, family.to_string(), family);
                }
            });
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut text.size).range(PAGE_TEXT_SIZES).speed(0.5).fixed_decimals(1).suffix(" pt"));
                ui.toggle_value(&mut text.bold, egui::RichText::new("B").strong());
                ui.toggle_value(&mut text.italic, egui::RichText::new("I").italics());
            });
        },
    );
}

fn trim_size(size: f32) -> String {
    if (size - size.round()).abs() < 0.05 { format!("{size:.0}") } else { format!("{size:.1}") }
}

/// The tokens a template may use, for the text fields' hover.
fn tokens_help() -> String {
    let mut out = String::from("Tokens:\n");
    for (token, what) in TOKENS {
        out.push_str(&format!("{token}  {what}\n"));
    }
    out
}

/// One text's row: the check box and name at the left; the template, alignment and font at the
/// right. A second copyright line has only its template.
fn text_row(ui: &mut egui::Ui, key: &str, label: &str, text: &mut PageText, formatted: bool) {
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(170.0, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(170.0);
            ui.checkbox(&mut text.on, label);
        });
        ui.add_enabled_ui(text.on, |ui| {
            ui.vertical(|ui| {
                ui.add(egui::TextEdit::singleline(&mut text.text).desired_width(320.0).char_limit(guitario_score::MAX_PAGE_TEXT)).on_hover_text(tokens_help());
                if formatted {
                    ui.horizontal(|ui| {
                        align_buttons(ui, &mut text.align);
                        ui.add_space(6.0);
                        font_button(ui, text, key);
                    });
                }
            });
        });
    });
    ui.add_space(8.0);
}

fn header_footer_tab(ui: &mut egui::Ui, texts: &mut HeaderFooter, other_pages: &mut bool) {
    ui.horizontal(|ui| {
        ui.add_space(150.0);
        ui.selectable_value(other_pages, false, "First Page");
        ui.selectable_value(other_pages, true, "Other Pages");
    });
    ui.add_space(6.0);
    let heading = |ui: &mut egui::Ui, s: &str| {
        ui.label(egui::RichText::new(s).strong().size(16.0));
        ui.add_space(4.0);
    };
    egui::ScrollArea::vertical().max_height(430.0).auto_shrink([false, true]).show(ui, |ui| {
        if *other_pages {
            let o = &mut texts.other;
            heading(ui, "HEADER");
            text_row(ui, "other.header", "Header", &mut o.header, true);
            heading(ui, "FOOTER");
            text_row(ui, "other.copyright", "Copyright", &mut o.copyright, true);
            text_row(ui, "other.copyright2", "Copyright (second line)", &mut o.copyright2, false);
            text_row(ui, "other.pageNumber", "Page number", &mut o.page_number, true);
            ui.label(egui::RichText::new("Left and right swap on even pages, so two-sided prints keep these at the outer edge.").small());
        } else {
            let f = &mut texts.first;
            heading(ui, "HEADER");
            text_row(ui, "first.title", "Title", &mut f.title, true);
            text_row(ui, "first.subtitle", "Subtitle", &mut f.subtitle, true);
            text_row(ui, "first.artist", "Artist", &mut f.artist, true);
            text_row(ui, "first.album", "Album", &mut f.album, true);
            text_row(ui, "first.words", "Words", &mut f.words, true);
            text_row(ui, "first.music", "Music", &mut f.music, true);
            text_row(ui, "first.wordsAndMusic", "Words & Music", &mut f.words_and_music, true);
            text_row(ui, "first.tabber", "Transcriber", &mut f.tabber, true);
            heading(ui, "FOOTER");
            text_row(ui, "first.copyright", "Copyright", &mut f.copyright, true);
            text_row(ui, "first.copyright2", "Copyright (second line)", &mut f.copyright2, false);
            text_row(ui, "first.pageNumber", "Page number", &mut f.page_number, true);
        }
    });
    // The second copyright line follows the first's look.
    texts.first.copyright2 = PageText { on: texts.first.copyright2.on, text: texts.first.copyright2.text.clone(), ..texts.first.copyright.clone() };
    texts.other.copyright2 = PageText { on: texts.other.copyright2.on, text: texts.other.copyright2.text.clone(), ..texts.other.copyright.clone() };
}

/// Big tab buttons across the top, as Guitar Pro's Stylesheet has them.
fn tab_bar(ui: &mut egui::Ui, tab: &mut Tab) {
    let t = crate::theme::tokens();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for (which, label) in [(Tab::Page, "Page & Score Format"), (Tab::HeaderFooter, "Header & Footer")] {
            let on = *tab == which;
            let (rect, response) = ui.allocate_exact_size(vec2(200.0, 40.0), egui::Sense::click());
            ui.painter().rect_filled(
                rect,
                0.0,
                if on {
                    t.accent
                } else if response.hovered() {
                    t.control_hover
                } else {
                    t.control
                },
            );
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(14.0), t.text);
            if response.clicked() {
                *tab = which;
            }
        }
    });
}

impl GuitarioApp {
    /// File › Page Setup…: the Stylesheet on its page tab.
    pub fn open_page_setup(&mut self) {
        self.open_stylesheet();
        self.page_setup.tab = Tab::Page;
    }

    /// File › Stylesheet… (F7): starts from the score's page and texts, on the tab last shown.
    pub fn open_stylesheet(&mut self) {
        let score = self.session.score();
        let tab = self.page_setup.tab;
        self.page_setup = PageSetupDialog { open: true, tab, draft: score.page, texts: score.header_footer.clone(), other_pages: false };
    }

    /// The Stylesheet on its Header & Footer tab.
    pub fn open_header_footer(&mut self) {
        self.open_stylesheet();
        self.page_setup.tab = Tab::HeaderFooter;
    }

    /// Applies what the window shows: only what changed, each as one step. True when all of it took.
    fn apply_stylesheet(&mut self) -> bool {
        let score = self.session.score();
        let (page_changed, texts_changed) = (score.page != self.page_setup.draft, score.header_footer != self.page_setup.texts);
        if page_changed && self.run("score.setPage", self.page_setup.params()).is_none() {
            return false;
        }
        if texts_changed && self.run("score.setHeaderFooter", self.page_setup.texts_params()).is_none() {
            return false;
        }
        if page_changed || texts_changed {
            // A preview that is open lays its pages out again.
            self.preview.invalidate();
        }
        true
    }

    pub(crate) fn page_setup_dialog(&mut self, ctx: &egui::Context) {
        if !self.page_setup.open {
            return;
        }
        let (mut ok, mut apply, mut cancel) = (false, false, false);
        let problem = self.page_setup.problem();
        let dialog = &mut self.page_setup;
        egui::Modal::new(egui::Id::new("page-setup")).show(ctx, |ui| {
            ui.set_width(520.0);
            tab_bar(ui, &mut dialog.tab);
            ui.add_space(10.0);
            match dialog.tab {
                Tab::Page => page_tab(ui, &mut dialog.draft),
                Tab::HeaderFooter => header_footer_tab(ui, &mut dialog.texts, &mut dialog.other_pages),
            }
            if let Some(why) = &problem {
                ui.colored_label(Color32::from_rgb(0xff, 0x7a, 0x6a), why);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Defaults").on_hover_text("Guitar Pro's stylesheet for this tab").clicked() {
                    match dialog.tab {
                        Tab::Page => dialog.draft = PageSetup { bend_pitch: dialog.draft.bend_pitch, ..PageSetup::default() },
                        Tab::HeaderFooter => dialog.texts = HeaderFooter::default(),
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(problem.is_none(), egui::Button::new("OK")).clicked() {
                        ok = true;
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        cancel = true;
                    }
                    if ui.add_enabled(problem.is_none(), egui::Button::new("Apply")).clicked() {
                        apply = true;
                    }
                });
            });
        });
        if ok || apply {
            if self.apply_stylesheet() && ok {
                self.page_setup.open = false;
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
        assert!(app.apply_stylesheet());
        assert!(before && app.session.score().page == PageSetup { paper: Paper::Letter, landscape: true, margin_left: 25, ..PageSetup::default() });
        app.session.run("edit.undo", &serde_json::Value::Null).unwrap();
        assert_eq!(app.session.score().page.paper, Paper::Legal);
    }

    #[test]
    fn a_custom_paper_goes_through_the_engine() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.open_stylesheet();
        app.page_setup.draft.paper = Paper::Custom;
        (app.page_setup.draft.custom_width, app.page_setup.draft.custom_height) = (240, 320);
        assert!(app.apply_stylesheet());
        assert_eq!(app.session.score().page.size_mm(), (240.0, 320.0));
    }

    #[test]
    fn header_and_footer_edits_apply_as_one_step_and_only_when_changed() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.open_header_footer();
        assert_eq!(app.page_setup.tab, Tab::HeaderFooter);
        assert!(app.apply_stylesheet());
        assert!(!app.session.is_dirty(), "nothing changed, nothing applied");
        app.page_setup.texts.first.music.text = "Composed by %MUSIC%".into();
        app.page_setup.texts.first.title.bold = true;
        app.page_setup.texts.other.header.on = true;
        assert!(app.apply_stylesheet());
        let hf = &app.session.score().header_footer;
        assert!(hf.first.title.bold && hf.other.header.on && hf.first.music.text == "Composed by %MUSIC%");
        app.session.run("edit.undo", &Value::Null).unwrap();
        assert!(app.session.score().header_footer.is_default(), "one undo step");
        // Frames draw both tabs without touching the score.
        for tab in [Tab::Page, Tab::HeaderFooter] {
            app.page_setup.tab = tab;
            for other in [false, true] {
                app.page_setup.other_pages = other;
                pump(&mut app);
            }
        }
        assert!(app.page_setup.open && app.session.score().header_footer.is_default());
    }

    #[test]
    fn a_page_the_layout_cannot_use_is_not_applied_and_the_dialog_stays() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.open_page_setup();
        app.page_setup.draft = PageSetup { paper: Paper::A5, margin_left: 50, margin_right: 50, ..PageSetup::default() };
        assert!(app.page_setup.problem().is_some());
        pump(&mut app);
        assert!(app.page_setup.open, "frames alone never close or apply the dialog");
        assert_eq!(app.session.score().page, PageSetup::default());
        assert!(!app.apply_stylesheet(), "the engine refuses it too");
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
            ("stylesheet", GuitarioApp::open_header_footer),
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
