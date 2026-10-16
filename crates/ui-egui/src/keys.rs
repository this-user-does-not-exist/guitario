//! Keyboard shortcuts, taken from the registry's `shortcut` strings such as `Cmd+Shift+Z`.

use crate::menu_catalog::{Action, CATALOG, MenuItem};
use guitario_engine::registry;

/// Parameterless commands that have a shortcut, most modifiers first so `Cmd+Shift+Z` is tried
/// before `Cmd+Z`.
pub fn bindings() -> Vec<(&'static str, egui::KeyboardShortcut)> {
    let mut out: Vec<_> = registry().iter().filter(|c| c.params.is_empty()).filter_map(|c| Some((c.id, parse(c.shortcut?)?))).collect();
    out.sort_by_key(|(_, ks)| std::cmp::Reverse(count(ks.modifiers)));
    out
}

fn count(m: egui::Modifiers) -> u8 {
    u8::from(m.command) + u8::from(m.shift) + u8::from(m.alt) + u8::from(m.ctrl)
}

pub fn parse(s: &str) -> Option<egui::KeyboardShortcut> {
    let mut mods = egui::Modifiers::NONE;
    let mut key = None;
    for part in s.split('+') {
        match part {
            "Cmd" => mods.command = true,
            "Shift" => mods.shift = true,
            "Alt" => mods.alt = true,
            "Ctrl" => mods.ctrl = true,
            other => key = egui::Key::from_name(other),
        }
    }
    Some(egui::KeyboardShortcut::new(mods, key?))
}

/// Shortcuts of menu-catalog items that are not plain registry commands: shell actions (Open,
/// Play, zoom …) and commands with parameters (Guitar Pro's single-key techniques such as `P` for
/// palm mute). Most modifiers first.
pub fn catalog_bindings() -> Vec<(egui::KeyboardShortcut, &'static MenuItem)> {
    let mut out: Vec<_> = CATALOG
        .iter()
        .filter(|i| !i.shortcut.is_empty())
        .filter(|i| matches!(i.action, Action::Ui(_)) || matches!(i.action, Action::Cmd(_, p) if !p.is_empty()))
        .filter_map(|i| Some((parse(i.shortcut)?, i)))
        .collect();
    out.sort_by_key(|(ks, _)| std::cmp::Reverse(count(ks.modifiers)));
    out
}

/// A bare 0–9 key press (not with Cmd/Alt/Ctrl).
pub fn digit_pressed(ctx: &egui::Context) -> Option<u8> {
    use egui::Key::*;
    const KEYS: [egui::Key; 10] = [Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9];
    ctx.input(|i| {
        if i.modifiers.command || i.modifiers.alt || i.modifiers.ctrl {
            return None;
        }
        KEYS.iter().position(|k| i.key_pressed(*k)).map(|p| p as u8)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts() {
        let ks = parse("Cmd+Shift+Z").unwrap();
        assert!(ks.modifiers.command && ks.modifiers.shift);
        assert_eq!(ks.logical_key, egui::Key::Z);
        assert_eq!(parse("Left").unwrap().logical_key, egui::Key::ArrowLeft);
        assert!(parse("Cmd+").is_none());
        assert!(catalog_bindings().iter().any(|(ks, i)| ks.logical_key == egui::Key::P && i.label == "Palm Mute"));
        assert!(catalog_bindings().iter().any(|(ks, _)| ks.logical_key == egui::Key::Space));
    }

    #[test]
    fn every_registry_shortcut_parses() {
        for c in registry() {
            if let Some(s) = c.shortcut {
                assert!(parse(s).is_some(), "{} has unparseable shortcut {s}", c.id);
            }
        }
    }
}
