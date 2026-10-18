//! The Homepage, as Guitar Pro 8 lays it out: the name, New File and Open File…, a search box,
//! then tabs of recent files (pinned first), local files from a chosen folder, templates and the
//! built-in examples, with a preview of the selected score's first page beside the list. The
//! toolbar's Home button shows it; opening anything goes back to the score.

use crate::GuitarioApp;
use crate::icons::{Icon, draw};
use crate::page::PrintSet;
use crate::theme::tokens;
use egui::{Color32, Pos2, Rect, Sense, Stroke, emath::TSTransform, pos2, vec2};
use guitario_engine::library::{EXAMPLES, TEMPLATES};
use guitario_score::Score;
use serde_json::json;
use std::sync::Arc;

/// Files a local folder lists at most, and how deep it is searched.
const MAX_LOCAL: usize = 500;
const LOCAL_DEPTH: usize = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Recent,
    Local,
    Templates,
    Examples,
}

impl Tab {
    const ALL: [(Tab, &'static str); 4] =
        [(Tab::Recent, "RECENT FILES"), (Tab::Local, "LOCAL FILES"), (Tab::Templates, "TEMPLATES"), (Tab::Examples, "EXAMPLES")];
}

/// Something the Homepage can open.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    File(String),
    Template(&'static str),
    Example(&'static str),
}

/// The selected entry's score, laid out for the preview, or why it could not be read.
struct Preview {
    entry: Entry,
    track: usize,
    loaded: Result<(Arc<Score>, PrintSet), String>,
}

#[derive(Default)]
pub struct Home {
    pub open: bool,
    pub tab: Tab,
    search: String,
    selected: Option<Entry>,
    /// The local folder's files, scanned when the folder or the tab changes.
    local: Option<(String, Vec<String>)>,
    preview: Option<Preview>,
}

/// A row of a list: what it opens, its name, whether it is pinned.
struct Row {
    entry: Entry,
    name: String,
    pinned: bool,
}

/// A file's name without its folder or extension.
fn stem(path: &str) -> String {
    std::path::Path::new(path).file_stem().map_or_else(|| path.to_string(), |s| s.to_string_lossy().into_owned())
}

/// Whether `path` is a Guitar Pro file, by its extension.
fn is_guitar_pro(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| guitario_engine::GUITAR_PRO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Whether `name` matches the search (any case); an empty search matches everything.
fn matches(name: &str, search: &str) -> bool {
    let s = search.trim().to_lowercase();
    s.is_empty() || name.to_lowercase().contains(&s)
}

/// Scores in `dir` and the folders under it, by path.
fn scan(dir: &str) -> Vec<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        fn walk(dir: &std::path::Path, depth: usize, out: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
            entries.sort();
            for path in entries {
                if out.len() >= MAX_LOCAL {
                    return;
                }
                if path.is_dir() {
                    if depth > 0 {
                        walk(&path, depth - 1, out);
                    }
                } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    let e = e.to_ascii_lowercase();
                    guitario_engine::NATIVE_EXTENSIONS.contains(&e.as_str()) || guitario_engine::GUITAR_PRO_EXTENSIONS.contains(&e.as_str())
                }) {
                    out.push(path.to_string_lossy().into_owned());
                }
            }
        }
        let mut out = Vec::new();
        walk(std::path::Path::new(dir), LOCAL_DEPTH, &mut out);
        out
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = dir;
        Vec::new()
    }
}

/// The score an entry opens, read without touching the document.
fn load(entry: &Entry) -> Result<Score, String> {
    match entry {
        Entry::File(path) => {
            let mut s = guitario_engine::Session::new();
            s.run("file.open", &json!({"path": path})).map_err(|e| e.to_string())?;
            Ok(s.score().as_ref().clone())
        }
        Entry::Template(id) => guitario_engine::library::template(id).ok_or_else(|| format!("no template `{id}`")),
        Entry::Example(id) => guitario_engine::library::example(id).map_err(|e| e.to_string()),
    }
}

fn layout(score: &Arc<Score>, track: usize) -> Result<(Arc<Score>, PrintSet), String> {
    PrintSet::new(score, &[track], true, false).map(|set| (Arc::clone(score), set))
}

impl GuitarioApp {
    /// Shows the Homepage (at start, and from the toolbar's Home button).
    pub fn show_home(&mut self) {
        self.home.open = true;
    }

    /// Shows the Homepage on its examples with the first one selected (the snapshot example uses it).
    pub fn show_home_examples(&mut self) {
        self.home.open = true;
        self.home.tab = Tab::Examples;
        if let Some((id, _, _)) = EXAMPLES.first() {
            self.home_select(Entry::Example(id));
        }
    }

    /// The rows of the Homepage's current tab, filtered by the search.
    fn home_rows(&mut self, tab: Tab) -> Vec<Row> {
        let search = self.home.search.clone();
        let rows: Vec<Row> = match tab {
            Tab::Recent => {
                let files = self.settings.get().files.clone();
                let pinned = files.pinned.iter().map(|p| Row { entry: Entry::File(p.clone()), name: stem(p), pinned: true });
                let recent =
                    files.recent.iter().filter(|p| !files.pinned.contains(p)).map(|p| Row { entry: Entry::File(p.clone()), name: stem(p), pinned: false });
                pinned.chain(recent).collect()
            }
            Tab::Local => {
                let folder = self.settings.get().files.folder.clone();
                match folder {
                    None => Vec::new(),
                    Some(folder) => {
                        if self.home.local.as_ref().is_none_or(|(f, _)| *f != folder) {
                            self.home.local = Some((folder.clone(), scan(&folder)));
                        }
                        self.home
                            .local
                            .as_ref()
                            .map(|(_, files)| files.iter().map(|p| Row { entry: Entry::File(p.clone()), name: stem(p), pinned: false }).collect())
                            .unwrap_or_default()
                    }
                }
            }
            Tab::Templates => TEMPLATES.iter().map(|(id, name)| Row { entry: Entry::Template(id), name: (*name).to_string(), pinned: false }).collect(),
            Tab::Examples => EXAMPLES.iter().map(|(id, title, _)| Row { entry: Entry::Example(id), name: (*title).to_string(), pinned: false }).collect(),
        };
        rows.into_iter().filter(|r| matches(&r.name, &search)).collect()
    }

    /// Opens what the Homepage offers and goes back to the score.
    fn home_open(&mut self, entry: &Entry) {
        match entry {
            Entry::File(path) => self.open_path(path),
            Entry::Template(id) => self.start_document("file.newFromTemplate", json!({"template": id})),
            Entry::Example(id) => self.start_document("file.openExample", json!({"example": id})),
        }
        self.home.open = false;
    }

    fn home_select(&mut self, entry: Entry) {
        if self.home.preview.as_ref().is_some_and(|p| p.entry == entry) {
            return;
        }
        let loaded = load(&entry).map(Arc::new).and_then(|s| layout(&s, 0));
        self.home.preview = Some(Preview { entry: entry.clone(), track: 0, loaded });
        self.home.selected = Some(entry);
    }
}

pub fn show(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    let t = tokens();
    let full = ui.max_rect();
    // ⌘N and ⌘O work here as in the score.
    if ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::N))) {
        app.new_document();
        app.home.open = false;
        return;
    }
    if ui.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O))) {
        app.open_dialog();
        app.home.open = false;
        return;
    }
    let width = (full.width() - 160.0).clamp(560.0, 1400.0);
    let left = full.center().x - width / 2.0;
    let mut y = full.top() + 24.0;

    // The mark and the name, as Guitar Pro heads its Homepage with its logo.
    let p = ui.painter().clone();
    let name = p.layout_no_wrap("Guitario".into(), crate::fonts::sans_bold(44.0), t.text);
    let side = 56.0;
    let gap = 14.0;
    let x = full.center().x - (side + gap + name.size().x) / 2.0;
    if let Some(tex) = crate::app_icon::mark(ui.ctx()) {
        let r = Rect::from_min_size(pos2(x, y - 4.0), vec2(side, side));
        p.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), t.text);
    }
    let name_h = name.size().y;
    p.galley(pos2(x + side + gap, y + (side - 8.0 - name_h) / 2.0), name, t.text);
    y += 74.0;

    // New File and Open File…
    for (label, keys, icon_new) in [("New File", "(⌘N)", true), ("Open File…", "(⌘O)", false)] {
        let r = Rect::from_center_size(pos2(full.center().x, y + 14.0), vec2(240.0, 28.0));
        let resp = ui.interact(r, ui.id().with(label), Sense::click());
        let ink = if resp.hovered() { Color32::WHITE } else { t.text };
        file_icon(&p, Rect::from_center_size(pos2(r.left() + 30.0, r.center().y), vec2(16.0, 20.0)), icon_new, ink);
        p.text(pos2(r.left() + 52.0, r.center().y), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(17.0), ink);
        p.text(pos2(r.right() - 20.0, r.center().y), egui::Align2::RIGHT_CENTER, keys, egui::FontId::proportional(15.0), t.text_dim);
        if resp.clicked() {
            app.home.open = false;
            if icon_new {
                app.new_document();
            } else {
                app.open_dialog();
            }
            return;
        }
        y += 36.0;
    }
    y += 10.0;

    // Search.
    let search = Rect::from_center_size(pos2(full.center().x, y + 15.0), vec2(width.min(520.0), 30.0));
    p.rect_filled(search, 5.0, t.lcd_bg);
    p.rect_stroke(search, 5.0, Stroke::new(1.0, t.control), egui::StrokeKind::Inside);
    draw(&p, Rect::from_center_size(pos2(search.left() + 16.0, search.center().y), vec2(16.0, 16.0)), Icon::Zoom, t.text);
    let edit_rect = Rect::from_min_max(pos2(search.left() + 32.0, search.top() + 4.0), pos2(search.right() - 8.0, search.bottom() - 4.0));
    ui.put(edit_rect, egui::TextEdit::singleline(&mut app.home.search).hint_text("Search…").frame(egui::Frame::NONE).font(egui::FontId::proportional(16.0)));
    y += 52.0;

    // Tabs, with the number of matches while searching.
    let searching = !app.home.search.trim().is_empty();
    let tab_w = width / Tab::ALL.len() as f32;
    let current = app.home.tab;
    for (k, (tab, name)) in Tab::ALL.into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(left + k as f32 * tab_w, y), vec2(tab_w, 32.0));
        let resp = ui.interact(r, ui.id().with(("home-tab", k)), Sense::click());
        let on = tab == current;
        let ink = if on {
            Color32::WHITE
        } else if resp.hovered() {
            t.text
        } else {
            t.text_dim
        };
        let galley = p.layout_no_wrap(name.to_string(), crate::fonts::sans_bold(19.0), ink);
        let count = searching.then(|| app.home_rows(tab).len());
        let badge_w = if count.is_some() { 30.0 } else { 0.0 };
        let x = r.center().x - (galley.size().x + badge_w) / 2.0;
        p.galley(pos2(x, r.center().y - galley.size().y / 2.0), galley.clone(), ink);
        if let Some(n) = count {
            let b = Rect::from_center_size(pos2(x + galley.size().x + 17.0, r.center().y), vec2(24.0, 18.0));
            p.rect_filled(b, 9.0, t.accent);
            p.text(b.center(), egui::Align2::CENTER_CENTER, n.to_string(), egui::FontId::proportional(12.0), Color32::WHITE);
        }
        if resp.clicked() {
            app.home.tab = tab;
        }
    }
    y += 44.0;

    // The list at the left, the preview at the right.
    let footer = 44.0;
    let bottom = full.bottom() - footer - 54.0;
    let list = Rect::from_min_max(pos2(left, y), pos2(left + width * 0.49, bottom.max(y + 120.0)));
    let preview = Rect::from_min_max(pos2(left + width * 0.51, y), pos2(left + width, list.bottom()));
    p.rect_filled(list, 6.0, t.chrome_dark);
    p.rect_filled(preview, 6.0, t.chrome_dark);
    let rows = app.home_rows(app.home.tab);
    let mut open: Option<Entry> = None;
    let mut select: Option<Entry> = None;
    let mut pin: Option<String> = None;
    let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list.shrink(6.0)));
    egui::ScrollArea::vertical().id_salt("home-list").auto_shrink(false).show(&mut list_ui, |ui| {
        let tab = app.home.tab;
        if tab == Tab::Local && app.settings.get().files.folder.is_none() {
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Choose a folder of scores to see them here.").color(t.text_dim));
            return;
        }
        if rows.is_empty() {
            ui.add_space(12.0);
            ui.label(egui::RichText::new(if searching { "Nothing matches." } else { "Nothing here yet." }).color(t.text_dim));
            return;
        }
        let heading = |ui: &mut egui::Ui, text: &str| {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
            let g = ui.painter().layout_no_wrap(text.to_string(), crate::fonts::sans_bold(15.0), t.text);
            let w = g.size().x;
            ui.painter().galley(pos2(r.left() + 6.0, r.center().y - g.size().y / 2.0), g, t.text);
            ui.painter().line_segment([pos2(r.left() + w + 18.0, r.center().y), pos2(r.right() - 6.0, r.center().y)], Stroke::new(1.0, t.text));
        };
        let pinned_any = rows.iter().any(|r| r.pinned);
        if tab == Tab::Recent && pinned_any {
            heading(ui, "Pinned");
        }
        let mut recents_shown = false;
        for row in &rows {
            if tab == Tab::Recent && pinned_any && !row.pinned && !recents_shown {
                heading(ui, "Recents");
                recents_shown = true;
            }
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
            let selected = app.home.selected.as_ref() == Some(&row.entry);
            let p = ui.painter();
            if selected {
                p.rect_filled(r, 5.0, t.accent);
            } else if resp.hovered() {
                p.rect_filled(r, 5.0, t.control);
            }
            // The pin, on pinned files and on any recent file under the pointer.
            if let Entry::File(path) = &row.entry
                && tab == Tab::Recent
                && (row.pinned || resp.hovered())
            {
                let pr = Rect::from_center_size(pos2(r.left() + 14.0, r.center().y), vec2(16.0, 16.0));
                pin_icon(p, pr, if row.pinned { t.text } else { t.text_dim });
                if ui.interact(pr, ui.id().with(("pin", path)), Sense::click()).on_hover_text(if row.pinned { "Unpin" } else { "Pin to the top" }).clicked() {
                    pin = Some(path.clone());
                }
            }
            let kind = match &row.entry {
                Entry::File(path) if is_guitar_pro(path) => "GP",
                Entry::File(_) => "GT",
                Entry::Template(_) => "+",
                Entry::Example(_) => "♫",
            };
            let badge = Rect::from_center_size(pos2(r.left() + 40.0, r.center().y), vec2(18.0, 20.0));
            p.rect_filled(badge, 3.0, if selected { Color32::WHITE } else { t.text });
            p.text(badge.center(), egui::Align2::CENTER_CENTER, kind, crate::fonts::sans_bold(8.5), if selected { t.accent } else { t.chrome_dark });
            p.text(
                pos2(r.left() + 58.0, r.center().y),
                egui::Align2::LEFT_CENTER,
                &row.name,
                egui::FontId::proportional(16.0),
                if selected { Color32::WHITE } else { t.text },
            );
            let resp = match &row.entry {
                Entry::File(path) => resp.on_hover_text(path),
                _ => resp,
            };
            if resp.double_clicked() {
                open = Some(row.entry.clone());
            } else if resp.clicked() {
                select = Some(row.entry.clone());
            }
        }
    });

    // Under the list: the tab's own button.
    let under = Rect::from_min_size(pos2(list.left(), list.bottom() + 12.0), vec2(220.0, 30.0));
    let mut under_ui = ui.new_child(egui::UiBuilder::new().max_rect(under));
    match app.home.tab {
        Tab::Recent => {
            let none = app.settings.get().files.recent.is_empty();
            if under_ui.add_enabled(!none, egui::Button::new("⊗ Clear Recent Files")).clicked() {
                app.settings.update(|p| p.files.recent.clear());
            }
        }
        Tab::Local => {
            let label = if app.settings.get().files.folder.is_some() { "Change Folder…" } else { "Choose a Folder…" };
            if under_ui.button(label).clicked() {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    let dir = dir.to_string_lossy().into_owned();
                    app.settings.update(|p| p.files.folder = Some(dir));
                    app.home.local = None;
                }
            }
        }
        Tab::Templates | Tab::Examples => {}
    }

    if let Some(path) = pin {
        app.settings.update(|p| p.toggle_pin(&path));
    }
    if let Some(entry) = select {
        app.home_select(entry);
    }
    if let Some(entry) = open {
        app.home_open(&entry);
        return;
    }

    preview_panel(app, ui, preview);
    footer_bar(app, ui, Rect::from_min_max(pos2(full.left(), full.bottom() - footer), full.max));
}

/// The selected score's first page, its track picker, and the button that opens it.
fn preview_panel(app: &mut GuitarioApp, ui: &mut egui::Ui, area: Rect) {
    let t = tokens();
    let fonts = app.fonts;
    let Some(pv) = app.home.preview.as_ref() else {
        ui.painter().text(area.center(), egui::Align2::CENTER_CENTER, "Select a score to preview it.", egui::FontId::proportional(15.0), t.text_dim);
        return;
    };
    let page_area = Rect::from_min_max(area.min + vec2(14.0, 12.0), area.max - vec2(14.0, 50.0));
    let mut change_track: Option<usize> = None;
    let mut open = false;
    match &pv.loaded {
        Ok((score, set)) => {
            if let Some((layout, _)) = set.page(0) {
                let m = layout.metrics;
                let scale = (page_area.width() / m.page_w).min(page_area.height() / m.page_h).clamp(0.05, 2.0);
                let origin = page_area.center() - vec2(m.page_w, m.page_h) * scale / 2.0;
                ui.with_visual_transform(TSTransform::new(origin.to_vec2(), scale), |ui| {
                    let sheet = Rect::from_min_size(Pos2::ZERO, vec2(m.page_w, m.page_h));
                    let mut painter = ui.painter().clone();
                    painter.set_clip_rect(sheet);
                    painter.rect_filled(sheet, 0.0, Color32::WHITE);
                    set.paint(&painter, score, 0, fonts);
                });
            }
            // The track picker: previous, the track, next.
            let n = score.tracks.len();
            let bar = Rect::from_center_size(pos2(area.center().x, area.bottom() - 24.0), vec2((area.width() - 80.0).clamp(200.0, 420.0), 28.0));
            let p = ui.painter();
            p.rect_filled(bar, 5.0, t.control);
            let prev = Rect::from_min_size(bar.min, vec2(30.0, bar.height()));
            let next = Rect::from_min_size(pos2(bar.right() - 30.0, bar.top()), vec2(30.0, bar.height()));
            for (r, icon, d) in [(prev, Icon::ChevronLeft, -1i64), (next, Icon::ChevronRight, 1)] {
                let resp = ui.interact(r, ui.id().with(("home-track", d)), Sense::click());
                draw(ui.painter(), r.shrink(8.0), icon, if n > 1 { t.text } else { t.text_dim });
                if resp.clicked() && n > 1 {
                    change_track = Some(((pv.track as i64 + d).rem_euclid(n as i64)) as usize);
                }
            }
            if let Some(track) = score.tracks.get(pv.track) {
                draw(
                    ui.painter(),
                    Rect::from_center_size(pos2(prev.right() + 18.0, bar.center().y), vec2(16.0, 16.0)),
                    crate::icons::instrument(track),
                    t.text,
                );
                ui.painter().text(pos2(prev.right() + 34.0, bar.center().y), egui::Align2::LEFT_CENTER, &track.name, egui::FontId::proportional(15.0), t.text);
            }
        }
        Err(why) => {
            ui.painter().text(page_area.center(), egui::Align2::CENTER_CENTER, why, egui::FontId::proportional(14.0), Color32::from_rgb(0xff, 0x7a, 0x6a));
        }
    }
    // Open, at the top right.
    let r = Rect::from_min_size(pos2(area.right() - 40.0, area.top() + 10.0), vec2(30.0, 30.0));
    let resp = ui.interact(r, ui.id().with("home-open"), Sense::click()).on_hover_text("Open");
    ui.painter().rect_filled(r, 5.0, if resp.hovered() { t.control_hover } else { t.control });
    open_icon(ui.painter(), r.shrink(8.0), t.text);
    if resp.clicked() {
        open = true;
    }
    if let Some(track) = change_track
        && let Some(pv) = app.home.preview.as_mut()
        && let Ok((score, _)) = &pv.loaded
    {
        let score = Arc::clone(score);
        pv.loaded = layout(&score, track);
        pv.track = track;
    }
    if open && let Some(entry) = app.home.preview.as_ref().map(|p| p.entry.clone()) {
        app.home_open(&entry);
    }
}

/// The version at the left; help at the right.
fn footer_bar(app: &mut GuitarioApp, ui: &mut egui::Ui, r: Rect) {
    let t = tokens();
    ui.painter().text(
        pos2(r.left() + 20.0, r.center().y),
        egui::Align2::LEFT_CENTER,
        format!("Version {}", env!("CARGO_PKG_VERSION")),
        egui::FontId::proportional(14.0),
        t.text_dim,
    );
    let mut right = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(20.0, 8.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
    if right.button("About Guitario…").clicked() {
        app.ui_action(crate::menu_catalog::UiAction::About);
    }
    if right.button("Keyboard Shortcuts…").clicked() {
        app.ui_action(crate::menu_catalog::UiAction::KeyboardShortcuts);
    }
}

/// A sheet of paper, or an open folder.
fn file_icon(p: &egui::Painter, r: Rect, sheet: bool, c: Color32) {
    let st = Stroke::new(1.4, c);
    if sheet {
        let fold = 5.0;
        p.add(egui::Shape::closed_line(
            vec![r.left_top(), pos2(r.right() - fold, r.top()), pos2(r.right(), r.top() + fold), r.right_bottom(), r.left_bottom()],
            st,
        ));
        p.line_segment([pos2(r.right() - fold, r.top()), pos2(r.right() - fold, r.top() + fold)], st);
        p.line_segment([pos2(r.right() - fold, r.top() + fold), pos2(r.right(), r.top() + fold)], st);
    } else {
        let back = Rect::from_min_max(pos2(r.left() - 2.0, r.top() + 3.0), pos2(r.right() + 2.0, r.bottom()));
        p.rect_filled(back, 2.0, c);
        p.add(egui::Shape::convex_polygon(
            vec![
                pos2(back.left() + 3.0, back.top() + 6.0),
                pos2(back.right() + 3.0, back.top() + 6.0),
                pos2(back.right(), back.bottom()),
                pos2(back.left(), back.bottom()),
            ],
            Color32::from_black_alpha(90),
            Stroke::NONE,
        ));
    }
}

/// A push pin.
fn pin_icon(p: &egui::Painter, r: Rect, c: Color32) {
    let at = |x: f32, y: f32| pos2(r.left() + x * r.width(), r.top() + y * r.height());
    p.rect_filled(Rect::from_min_max(at(0.25, 0.0), at(0.75, 0.12)), 1.0, c);
    p.add(egui::Shape::convex_polygon(vec![at(0.35, 0.12), at(0.65, 0.12), at(0.75, 0.55), at(0.25, 0.55)], c, Stroke::NONE));
    p.rect_filled(Rect::from_min_max(at(0.1, 0.55), at(0.9, 0.65)), 1.0, c);
    p.line_segment([at(0.5, 0.65), at(0.5, 1.0)], Stroke::new(1.5, c));
}

/// A box with an arrow leaving it: open.
fn open_icon(p: &egui::Painter, r: Rect, c: Color32) {
    let st = Stroke::new(1.6, c);
    let at = |x: f32, y: f32| pos2(r.left() + x * r.width(), r.top() + y * r.height());
    p.add(egui::Shape::line(vec![at(0.45, 0.15), at(0.1, 0.15), at(0.1, 0.9), at(0.85, 0.9), at(0.85, 0.55)], st));
    p.line_segment([at(0.4, 0.6), at(0.95, 0.05)], st);
    p.add(egui::Shape::line(vec![at(0.6, 0.05), at(0.95, 0.05), at(0.95, 0.4)], st));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_search() {
        assert_eq!(stem("/a/b/Iron Gallop.gp"), "Iron Gallop");
        assert!(matches("Iron Gallop", "iron"));
        assert!(matches("Iron Gallop", "  "));
        assert!(!matches("Iron Gallop", "doom"));
    }

    #[test]
    fn every_template_and_example_previews() {
        for (id, _) in TEMPLATES {
            let score = load(&Entry::Template(id)).map(Arc::new).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(layout(&score, 0).is_ok(), "{id}");
        }
        for (id, _, _) in EXAMPLES.iter().take(3) {
            assert!(load(&Entry::Example(id)).is_ok(), "{id}");
        }
        assert!(load(&Entry::File("/no/such/file.gt".into())).is_err());
    }

    #[test]
    fn a_folder_lists_scores_only_and_a_missing_one_lists_nothing() {
        let dir = std::env::temp_dir().join(format!("guitario-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for f in ["a.gt", "b.gp5", "notes.txt", "sub/c.gp"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let found: Vec<String> = scan(&dir.to_string_lossy()).iter().map(|p| stem(p)).collect();
        assert_eq!(found, ["a", "b", "c"]);
        assert!(scan("/no/such/folder").is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
