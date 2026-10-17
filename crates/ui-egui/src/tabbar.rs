//! The tab bar under the toolbar, as in Guitar Pro 8: the open documents share the width with
//! their names centred, a dot marks unsaved changes, hovering shows save and close buttons, a
//! right-click opens the tab's menu and `+` at the right starts a new score. Guitario holds one
//! document at a time, so there is one tab.

use crate::GuitarioApp;
use crate::icons::{Icon, draw};
use crate::menu_catalog::UiAction as U;
use crate::theme::tokens;
use egui::{Color32, Rect, Sense, Stroke, pos2, vec2};

pub const HEIGHT: f32 = 28.0;
const PLUS: f32 = 30.0;

/// A tab's label: the file name without its extension.
pub(crate) fn tab_title(file_name: &str) -> String {
    match file_name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() && !ext.contains(' ') => stem.to_string(),
        _ => file_name.to_string(),
    }
}

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let t = tokens();
    let full = ui.max_rect();
    let p = ui.painter().clone();
    let tab = Rect::from_min_max(full.min, pos2(full.right() - PLUS, full.bottom()));
    let resp = ui.interact(tab, ui.id().with("doc-tab"), Sense::click());
    p.rect_filled(tab, 0.0, t.control);
    p.line_segment([pos2(tab.right(), tab.top()), pos2(tab.right(), tab.bottom())], Stroke::new(1.0, t.border));
    let title = tab_title(&app.doc_name());
    p.text(tab.center(), egui::Align2::CENTER_CENTER, &title, crate::fonts::sans_bold(14.0), t.text);
    let dirty = app.session.is_dirty();
    // Hovering shows save and close at the right; otherwise a modified file shows a dot there.
    let hover = resp.hovered() || ui.rect_contains_pointer(tab);
    let close = Rect::from_center_size(pos2(tab.right() - 18.0, tab.center().y), vec2(20.0, 20.0));
    let save = Rect::from_center_size(pos2(tab.right() - 42.0, tab.center().y), vec2(20.0, 20.0));
    if hover {
        let close_r = ui.interact(close, ui.id().with("doc-tab-close"), Sense::click()).on_hover_text("Close (⌘W)");
        let save_r = ui.interact(save, ui.id().with("doc-tab-save"), Sense::click()).on_hover_text("Save (⌘S)");
        for (r, resp) in [(save, &save_r), (close, &close_r)] {
            if resp.hovered() {
                p.rect_filled(r, 3.0, t.control_hover);
            }
        }
        draw(&p, save.shrink(3.0), Icon::Save, t.text);
        let x = close.shrink(6.0);
        let st = Stroke::new(1.8, t.text);
        p.line_segment([x.left_top(), x.right_bottom()], st);
        p.line_segment([x.right_top(), x.left_bottom()], st);
        if save_r.clicked() {
            app.save_dialog(false);
        }
        if close_r.clicked() {
            app.ui_action(U::CloseDocument);
        }
    } else if dirty {
        p.circle_filled(pos2(tab.right() - 18.0, tab.center().y), 4.0, t.text);
    }
    resp.context_menu(|ui| {
        if ui.button("Close").clicked() {
            ui.close();
            app.ui_action(U::CloseDocument);
        }
        if ui.button("Close All").clicked() {
            ui.close();
            app.ui_action(U::CloseDocument);
        }
    });

    let plus = Rect::from_min_max(pos2(full.right() - PLUS, full.top()), full.max);
    let plus_r = ui.interact(plus, ui.id().with("doc-tab-new"), Sense::click()).on_hover_text("New score (⌘N)");
    if plus_r.hovered() {
        p.rect_filled(plus, 0.0, t.control_hover);
    }
    let c = plus.center();
    let st = Stroke::new(2.0, if plus_r.hovered() { Color32::WHITE } else { t.text });
    p.line_segment([c - vec2(6.0, 0.0), c + vec2(6.0, 0.0)], st);
    p.line_segment([c - vec2(0.0, 6.0), c + vec2(0.0, 6.0)], st);
    if plus_r.clicked() {
        app.new_document();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_drop_the_extension_only() {
        assert_eq!(tab_title("Keystone.gt"), "Keystone");
        assert_eq!(tab_title("riff.v2.gp"), "riff.v2");
        assert_eq!(tab_title("Untitled"), "Untitled");
        assert_eq!(tab_title(".hidden"), ".hidden");
        assert_eq!(tab_title("Mr. Crowley Tab"), "Mr. Crowley Tab");
        assert_eq!(tab_title(""), "");
    }
}
