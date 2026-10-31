//! The tab bar under the toolbar, as in Guitar Pro 8: one tab per open document sharing the
//! width, names centred, the shown one lit; a dot marks unsaved changes, hovering shows save and
//! close buttons, a middle click closes, a right-click opens the tab's menu and `+` at the right
//! starts a new score.

use crate::GuitarioApp;
use crate::icons::{Icon, draw};
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

/// What a click on a tab asks for; done after drawing, since most of it changes the tabs.
enum Ask {
    Show(usize),
    Save(usize),
    Close(usize),
    CloseOthers(usize),
    CloseAll,
}

/// The tabs' rectangles across `full`, less the `+` button.
fn tab_rects(full: Rect, n: usize) -> impl Iterator<Item = Rect> {
    let w = (full.width() - PLUS).max(0.0) / n.max(1) as f32;
    (0..n).map(move |i| Rect::from_min_size(pos2(full.left() + w * i as f32, full.top()), vec2(w, full.height())))
}

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let t = tokens();
    let full = ui.max_rect();
    let p = ui.painter().clone();
    let n = app.tab_count();
    let mut ask = None;
    for (i, tab) in tab_rects(full, n).enumerate() {
        let resp = ui.interact(tab, ui.id().with(("doc-tab", i)), Sense::click());
        let shown = i == app.active_tab();
        let hover = resp.hovered() || ui.rect_contains_pointer(tab);
        let fill = match (shown, hover) {
            (true, _) => t.tab,
            (false, true) => t.control_hover,
            (false, false) => t.control,
        };
        p.rect_filled(tab, 0.0, fill);
        p.line_segment([pos2(tab.right(), tab.top()), pos2(tab.right(), tab.bottom())], Stroke::new(1.0, t.border));
        // Room for the buttons at the right; a long name is cut short with an ellipsis.
        let room = (tab.width() - 64.0).max(16.0);
        let font = if shown { crate::fonts::sans_bold(14.0) } else { egui::FontId::proportional(14.0) };
        let ink = if shown { Color32::WHITE } else { t.text };
        let mut job = egui::text::LayoutJob::simple_singleline(tab_title(&app.tab_name(i)), font, ink);
        job.wrap = egui::text::TextWrapping::truncate_at_width(room);
        let galley = p.layout_job(job);
        p.galley(tab.center() - galley.size() / 2.0, galley, ink);
        let dirty = app.tab_session(i).is_some_and(guitario_engine::Session::is_dirty);
        let close = Rect::from_center_size(pos2(tab.right() - 18.0, tab.center().y), vec2(20.0, 20.0));
        let save = Rect::from_center_size(pos2(tab.right() - 42.0, tab.center().y), vec2(20.0, 20.0));
        if hover {
            let close_r = ui.interact(close, ui.id().with(("doc-tab-close", i)), Sense::click()).on_hover_text("Close (⌘W)");
            let save_r = ui.interact(save, ui.id().with(("doc-tab-save", i)), Sense::click()).on_hover_text("Save (⌘S)");
            for (r, resp) in [(save, &save_r), (close, &close_r)] {
                if resp.hovered() {
                    p.rect_filled(r, 3.0, if shown { t.control_hover } else { t.control });
                }
            }
            draw(&p, save.shrink(3.0), Icon::Save, t.text);
            let x = close.shrink(6.0);
            let st = Stroke::new(1.8, t.text);
            p.line_segment([x.left_top(), x.right_bottom()], st);
            p.line_segment([x.right_top(), x.left_bottom()], st);
            if save_r.clicked() {
                ask = Some(Ask::Save(i));
            }
            if close_r.clicked() {
                ask = Some(Ask::Close(i));
            }
        } else if dirty {
            p.circle_filled(pos2(tab.right() - 18.0, tab.center().y), 4.0, t.text);
        }
        if resp.clicked() && ask.is_none() {
            ask = Some(Ask::Show(i));
        }
        if resp.middle_clicked() {
            ask = Some(Ask::Close(i));
        }
        resp.context_menu(|ui| {
            for (label, a) in [("Close", Ask::Close(i)), ("Close Others", Ask::CloseOthers(i)), ("Close All", Ask::CloseAll)] {
                if ui.add_enabled(n > 1 || !matches!(a, Ask::CloseOthers(_)), egui::Button::new(label)).clicked() {
                    ui.close();
                    ask = Some(a);
                }
            }
        });
    }

    let plus = Rect::from_min_max(pos2(full.right() - PLUS, full.top()), full.max);
    let plus_r = ui.interact(plus, ui.id().with("doc-tab-new"), Sense::click()).on_hover_text("New score (⌘N)");
    p.rect_filled(plus, 0.0, if plus_r.hovered() { t.control_hover } else { t.control });
    let c = plus.center();
    let st = Stroke::new(2.0, if plus_r.hovered() { Color32::WHITE } else { t.text });
    p.line_segment([c - vec2(6.0, 0.0), c + vec2(6.0, 0.0)], st);
    p.line_segment([c - vec2(0.0, 6.0), c + vec2(0.0, 6.0)], st);
    if plus_r.clicked() {
        app.home.open = false;
        app.new_document();
    }
    if let Some(ask) = ask {
        // A tab also takes the window back from the Homepage.
        app.home.open = false;
        answer(app, ask);
    }
}

fn answer(app: &mut GuitarioApp, ask: Ask) {
    match ask {
        Ask::Show(i) => app.switch_to(i),
        Ask::Save(i) => {
            app.switch_to(i);
            app.save_dialog(false);
        }
        Ask::Close(i) => {
            app.switch_to(i);
            app.close_document();
        }
        Ask::CloseOthers(i) => app.close_others(i),
        Ask::CloseAll => app.close_all(),
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

    #[test]
    fn tabs_share_the_width_left_of_the_plus() {
        let full = Rect::from_min_size(pos2(0.0, 0.0), vec2(330.0, HEIGHT));
        let r: Vec<Rect> = tab_rects(full, 3).collect();
        assert_eq!(r.len(), 3);
        assert!(r.iter().all(|r| (r.width() - 100.0).abs() < 1e-3));
        assert!((r[2].right() - (330.0 - PLUS)).abs() < 1e-3);
        // A window too narrow for the plus never makes a negative tab.
        assert!(tab_rects(Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, HEIGHT)), 2).all(|r| r.width() >= 0.0));
    }
}
