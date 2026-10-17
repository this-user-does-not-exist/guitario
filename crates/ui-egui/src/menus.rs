//! The menu bar, built from Guitar Pro's menu catalog (`menu_catalog.rs`). Live items run their
//! engine command or shell action; items not implemented yet are shown greyed out, as a reminder
//! of what parity still needs.

use crate::GuitarioApp;
use crate::menu_catalog::{Action, CATALOG, MenuItem, TOP, UiAction};
use serde_json::{Value, json};

fn shortcut_text(s: &str) -> String {
    let cmd = if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" };
    s.replace("Cmd+", cmd)
        .replace("Shift+", "⇧")
        .replace("Alt+", if cfg!(target_os = "macos") { "⌥" } else { "Alt+" })
        .replace("Plus", "+")
        .replace("Minus", "−")
        .replace("Period", ".")
        .replace("Slash", "/")
}

pub fn menu_bar(app: &mut GuitarioApp, ui: &mut egui::Ui) {
    for top in TOP {
        ui.menu_button(top, |ui| menu(app, ui, top));
    }
}

/// Items directly under `path`, then each submenu in catalog order.
fn menu(app: &mut GuitarioApp, ui: &mut egui::Ui, path: &str) {
    let mut subs: Vec<&str> = Vec::new();
    for item in CATALOG {
        if item.path == path {
            entry(app, ui, item);
        } else if let Some(rest) = item.path.strip_prefix(path).and_then(|r| r.strip_prefix('/')) {
            let first = rest.split('/').next().unwrap_or(rest);
            if !subs.contains(&first) {
                subs.push(first);
                let sub_path = format!("{path}/{first}");
                ui.menu_button(first, |ui| menu(app, ui, &sub_path));
            }
        }
    }
}

fn entry(app: &mut GuitarioApp, ui: &mut egui::Ui, item: &MenuItem) {
    match item.action {
        Action::Ui(UiAction::OpenRecent) => recent_menu(app, ui, item.label),
        Action::Ui(UiAction::TimeSignature) => {
            ui.menu_button(item.label, |ui| {
                for (n, d) in [(2, 4), (3, 4), (4, 4), (5, 4), (6, 4), (7, 4), (3, 8), (6, 8), (7, 8), (9, 8), (12, 8), (13, 16)] {
                    if ui.button(format!("{n}/{d}")).clicked() {
                        app.run("measure.setTimeSignature", json!({"numerator": n, "denominator": d}));
                        ui.close();
                    }
                }
            });
        }
        Action::Ui(UiAction::KeySignature) => {
            ui.menu_button(item.label, |ui| {
                let names = ["C♭", "G♭", "D♭", "A♭", "E♭", "B♭", "F", "C", "G", "D", "A", "E", "B", "F♯", "C♯"];
                for (i, n) in names.iter().enumerate() {
                    let k = i as i64 - 7;
                    if ui.button(format!("{n} major / {} minor", names.get(i.wrapping_add(3) % names.len()).unwrap_or(n))).clicked() {
                        app.run("measure.setKey", json!({"key": k}));
                        ui.close();
                    }
                }
            });
        }
        Action::Ui(UiAction::Section) => {
            ui.menu_button(item.label, |ui| {
                for name in ["Intro", "Verse", "Pre-Chorus", "Chorus", "Bridge", "Breakdown", "Solo", "Outro"] {
                    if ui.button(name).clicked() {
                        app.run("measure.setMarker", json!({"text": name}));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Remove Section").clicked() {
                    app.run("measure.setMarker", json!({"text": ""}));
                    ui.close();
                }
            });
        }
        Action::Ui(UiAction::TempoChange) => {
            ui.menu_button(item.label, |ui| {
                for bpm in [60, 80, 100, 120, 140, 160, 180, 200, 220] {
                    if ui.button(format!("♩ = {bpm}")).clicked() {
                        app.run("measure.setTempo", json!({"bpm": bpm}));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Remove Tempo Change").clicked() {
                    app.run("measure.setTempo", json!({"bpm": Value::Null}));
                    ui.close();
                }
            });
        }
        Action::Ui(UiAction::TripletFeel) => {
            ui.menu_button(item.label, |ui| {
                for (label, feel) in [
                    ("None", "none"),
                    ("Triplet 8th", "triplet8th"),
                    ("Triplet 16th", "triplet16th"),
                    ("Dotted 8th", "dotted8th"),
                    ("Dotted 16th", "dotted16th"),
                    ("Scottish 8th", "scottish8th"),
                    ("Scottish 16th", "scottish16th"),
                ] {
                    if ui.button(label).clicked() {
                        app.run("measure.setTripletFeel", json!({"feel": feel}));
                        ui.close();
                    }
                }
            });
        }
        Action::Ui(UiAction::AlternateEndings) => {
            ui.menu_button(item.label, |ui| {
                let c = app.session.cursor();
                let bits = app.session.score().headers.get(c.measure).map_or(0, |h| h.endings);
                for n in 1..=8u8 {
                    let mut on = bits & (1 << (n - 1)) != 0;
                    if ui.checkbox(&mut on, format!("Ending {n}")).clicked() {
                        let next = if on { bits | (1 << (n - 1)) } else { bits & !(1 << (n - 1)) };
                        let list: Vec<u8> = (1..=8u8).filter(|k| next & (1 << (k - 1)) != 0).collect();
                        app.run("measure.setEndings", json!({"endings": list}));
                    }
                }
                ui.separator();
                if ui.button("No Ending").clicked() {
                    app.run("measure.setEndings", json!({"endings": []}));
                    ui.close();
                }
            });
        }
        Action::Ui(UiAction::Directions) => {
            ui.menu_button(item.label, |ui| {
                let c = app.session.cursor();
                let header = app.session.score().headers.get(c.measure).cloned();
                let targets = header.as_ref().map(|h| h.targets.clone()).unwrap_or_default();
                let names = |t: &guitario_score::DirectionTarget| guitario_engine::direction_targets().iter().find(|(_, x)| x == t).map(|(n, _)| *n);
                for (label, name) in [("Segno", "segno"), ("Segno Segno", "segnoSegno"), ("Coda", "coda"), ("Double Coda", "doubleCoda"), ("Fine", "fine")] {
                    let mut on = targets.iter().any(|t| names(t) == Some(name));
                    if ui.checkbox(&mut on, label).clicked() {
                        let mut list: Vec<&str> = targets.iter().filter_map(names).filter(|n| *n != name).collect();
                        if on {
                            list.push(name);
                        }
                        app.run("measure.setDirections", json!({"targets": list}));
                    }
                }
                ui.separator();
                for (label, name) in [
                    ("D.C.", "daCapo"),
                    ("D.C. al Fine", "daCapoAlFine"),
                    ("D.C. al Coda", "daCapoAlCoda"),
                    ("D.C. al Double Coda", "daCapoAlDoubleCoda"),
                    ("D.S.", "dalSegno"),
                    ("D.S. al Fine", "dalSegnoAlFine"),
                    ("D.S. al Coda", "dalSegnoAlCoda"),
                    ("D.S. al Double Coda", "dalSegnoAlDoubleCoda"),
                    ("D.S.S.", "dalSegnoSegno"),
                    ("D.S.S. al Fine", "dalSegnoSegnoAlFine"),
                    ("D.S.S. al Coda", "dalSegnoSegnoAlCoda"),
                    ("D.S.S. al Double Coda", "dalSegnoSegnoAlDoubleCoda"),
                    ("To Coda", "toCoda"),
                    ("To Double Coda", "toDoubleCoda"),
                ] {
                    if ui.button(label).clicked() {
                        app.run("measure.setDirections", json!({"jump": name}));
                        ui.close();
                    }
                }
                if ui.button("No Jump").clicked() {
                    app.run("measure.setDirections", json!({"jump": null}));
                    ui.close();
                }
            });
        }
        Action::Ui(UiAction::Tuplet) => {
            ui.menu_button(item.label, |ui| {
                for (n, m) in [(1, 1), (3, 2), (5, 4), (6, 4), (7, 4), (9, 8), (10, 8)] {
                    let label = if n == 1 { "None".to_string() } else { format!("{n}:{m}") };
                    if ui.button(label).clicked() {
                        app.run("beat.setTuplet", json!({"enters": n, "times": m}));
                        ui.close();
                    }
                }
            });
        }
        Action::Ui(UiAction::Brush(kind)) => {
            ui.menu_button(item.label, |ui| {
                for (label, dir) in [("Down (low to high strings)", "down"), ("Up (high to low strings)", "up")] {
                    if ui.button(label).clicked() {
                        app.run("beat.setBrush", json!({"kind": kind, "direction": dir}));
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("None").clicked() {
                    app.run("beat.setBrush", json!({"kind": "none"}));
                    ui.close();
                }
            });
        }
        Action::Missing => {
            ui.add_enabled(false, egui::Button::new(item.label)).on_disabled_hover_text("Not in Guitario yet (see docs/parity-checklist.md)");
        }
        action => {
            let mut b = egui::Button::new(item.label);
            if !item.shortcut.is_empty() {
                b = b.shortcut_text(shortcut_text(item.shortcut));
            }
            // View choices show which one is on.
            let on = match action {
                Action::Ui(UiAction::Layout(mode)) => Some(app.view.layout == mode),
                Action::Ui(UiAction::ToggleMultitrack) => Some(app.view.multitrack),
                Action::Ui(UiAction::ToggleMultivoice) => Some(app.view.multivoice),
                Action::Ui(UiAction::ToggleNotation) => Some(app.view.notation),
                _ => None,
            };
            if let Some(on) = on {
                b = b.selected(on);
            }
            if ui.add(b).clicked() {
                ui.close();
                match action {
                    Action::Cmd("file.new", _) => app.new_document(),
                    Action::Cmd(id, params) => {
                        let p = if params.is_empty() { Value::Null } else { serde_json::from_str(params).unwrap_or(Value::Null) };
                        app.run(id, p);
                    }
                    Action::Ui(a) => app.ui_action(a),
                    Action::Missing => {}
                }
            }
        }
    }
}

fn recent_menu(app: &mut GuitarioApp, ui: &mut egui::Ui, label: &str) {
    let recent = app.recent_files();
    ui.add_enabled_ui(!recent.is_empty(), |ui| {
        ui.menu_button(label, |ui| {
            for path in recent {
                let name = std::path::Path::new(&path).file_name().map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
                if ui.button(name).on_hover_text(&path).clicked() {
                    ui.close();
                    app.open_path(&path);
                }
            }
        });
    });
}
