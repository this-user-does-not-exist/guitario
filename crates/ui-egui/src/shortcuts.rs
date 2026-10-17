//! Help › Keyboard Shortcuts: every shortcut of the menus, by menu, read from the catalog so the
//! list can never disagree with what the keys do.

use crate::GuitarioApp;
use crate::menu_catalog::CATALOG;
use crate::menus::shortcut_text;

/// `(menu, [(label, shortcut as shown)])`, in menu order, for items that have a shortcut.
pub(crate) fn listing() -> Vec<(&'static str, Vec<(&'static str, String)>)> {
    let mut out: Vec<(&'static str, Vec<(&'static str, String)>)> = Vec::new();
    for item in CATALOG.iter().filter(|i| !i.shortcut.is_empty()) {
        let top = item.path.split('/').next().unwrap_or(item.path);
        let entry = (item.label, shortcut_text(item.shortcut));
        match out.iter_mut().find(|(t, _)| *t == top) {
            Some((_, items)) => items.push(entry),
            None => out.push((top, vec![entry])),
        }
    }
    out
}

impl GuitarioApp {
    pub(crate) fn shortcuts_window(&mut self, ctx: &egui::Context) {
        let mut open = self.shortcuts_open;
        egui::Window::new("Keyboard Shortcuts").open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                ui.label(egui::RichText::new("0-9 types a fret on the cursor's string; arrows move the cursor.").small().weak());
                for (menu, items) in listing() {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(menu).strong());
                    egui::Grid::new(("shortcuts", menu)).num_columns(2).spacing([24.0, 2.0]).show(ui, |ui| {
                        for (label, keys) in items {
                            ui.label(label);
                            ui.label(egui::RichText::new(keys).monospace());
                            ui.end_row();
                        }
                    });
                }
            });
        });
        self.shortcuts_open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    #[test]
    fn the_list_comes_from_the_catalog_and_the_menu_item_opens_it() {
        let list = listing();
        let file = list.iter().find(|(m, _)| *m == "File").map(|(_, i)| i);
        assert!(file.is_some_and(|i| i.iter().any(|(l, _)| *l == "Open…")));
        assert!(list.iter().all(|(_, items)| !items.is_empty()));
        let mut app = GuitarioApp::new(Session::new(), None);
        assert!(!app.shortcuts_open);
        app.ui_action(crate::menu_catalog::UiAction::KeyboardShortcuts);
        assert!(app.shortcuts_open);
    }

    #[test]
    fn full_screen_toggles_and_minimize_queue_a_window_command() {
        let mut app = GuitarioApp::new(Session::new(), None);
        app.ui_action(crate::menu_catalog::UiAction::FullScreen);
        assert!(app.fullscreen);
        assert_eq!(app.viewport_cmds, vec![egui::ViewportCommand::Fullscreen(true)]);
        app.ui_action(crate::menu_catalog::UiAction::FullScreen);
        assert!(!app.fullscreen);
        app.viewport_cmds.clear();
        app.ui_action(crate::menu_catalog::UiAction::Minimize);
        assert_eq!(app.viewport_cmds, vec![egui::ViewportCommand::Minimized(true)]);
    }
}
