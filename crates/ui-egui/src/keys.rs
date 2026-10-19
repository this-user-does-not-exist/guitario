//! Keyboard shortcuts, taken from the registry's and the menu catalog's `shortcut` strings such as
//! `Cmd+Shift+Z`. They follow Guitar Pro 8's list (its manual's appendix), macOS first: `Cmd` is
//! ⌘ on a Mac and Ctrl elsewhere, `Ctrl` is ⌃ and Ctrl. A string may name a different shortcut
//! off macOS after a `;` (`Cmd+Plus;Insert`), where ⌘ and ⌃ would otherwise both be Ctrl.
//! Symbols with no key of their own are their US-layout keys with Shift (`$` is `Shift+4`).

use crate::menu_catalog::{Action, CATALOG, MenuItem};
use guitario_engine::registry;

/// What a shortcut does: a parameterless registry command, or a menu item.
#[derive(Clone, Copy, Debug)]
pub enum Binding {
    Command(&'static str),
    Item(&'static MenuItem),
}

/// Second shortcuts for registry commands, as Guitar Pro has them (⇧↑ and ⇧↓ both select the beat).
const ALIASES: [(&str, &str); 1] = [("select.beat", "Shift+Down")];

/// Every shortcut that does something, most modifiers first: egui lets a plain `R` match ⇧R too,
/// so ⇧R must be tried first. Parameterless registry commands bind their own shortcut; menu items
/// bind theirs when they are shell actions or commands with parameters (Guitar Pro's single-key
/// techniques such as `P` for palm mute). A shortcut on `+` also answers to `=`, the same key
/// without Shift.
pub fn bindings() -> Vec<(egui::KeyboardShortcut, Binding)> {
    let mut out: Vec<(egui::KeyboardShortcut, Binding)> = Vec::new();
    for c in registry().iter().filter(|c| c.params.is_empty()) {
        if let Some(ks) = c.shortcut.and_then(parse) {
            out.push((ks, Binding::Command(c.id)));
        }
    }
    for (id, s) in ALIASES {
        if let Some(ks) = parse(s) {
            out.push((ks, Binding::Command(id)));
        }
    }
    for item in CATALOG.iter().filter(|i| !i.shortcut.is_empty()) {
        let bound_here = matches!(item.action, Action::Ui(_)) || matches!(item.action, Action::Cmd(_, p) if !p.is_empty());
        if let (true, Some(ks)) = (bound_here, parse(item.shortcut)) {
            out.push((ks, Binding::Item(item)));
        }
    }
    let equals: Vec<_> = out
        .iter()
        .filter(|(ks, _)| ks.logical_key == egui::Key::Plus)
        .map(|(ks, b)| (egui::KeyboardShortcut::new(ks.modifiers, egui::Key::Equals), *b))
        .collect();
    out.extend(equals);
    out.sort_by_key(|(ks, _)| std::cmp::Reverse(count(ks.modifiers)));
    out
}

fn count(m: egui::Modifiers) -> u8 {
    u8::from(m.command) + u8::from(m.shift) + u8::from(m.alt) + u8::from(m.ctrl)
}

/// The part of a shortcut string for this platform.
pub fn for_platform(s: &str) -> &str {
    match s.split_once(';') {
        Some((mac, other)) => {
            if cfg!(target_os = "macos") {
                mac
            } else {
                other
            }
        }
        None => s,
    }
}

pub fn parse(s: &str) -> Option<egui::KeyboardShortcut> {
    let mut mods = egui::Modifiers::NONE;
    let mut key = None;
    for part in for_platform(s).split('+') {
        match part {
            "Cmd" => mods.command = true,
            "Shift" => mods.shift = true,
            "Alt" => mods.alt = true,
            "Ctrl" => {
                mods.ctrl = true;
                // On a Mac ⌃ is its own key; elsewhere Ctrl is the command key.
                if !cfg!(target_os = "macos") {
                    mods.command = true;
                }
            }
            other => key = egui::Key::from_name(other),
        }
    }
    Some(egui::KeyboardShortcut::new(mods, key?))
}

/// A shortcut as menus show it: `⇧⌘→` on a Mac, `Ctrl+Shift+Right` elsewhere, with the
/// symbols Shift types on a US keyboard (`$`, `<`, `>`).
pub fn display(s: &str) -> String {
    let mac = cfg!(target_os = "macos");
    let parts: Vec<&str> = for_platform(s).split('+').collect();
    let (mods, key) = parts.split_at(parts.len().saturating_sub(1));
    let key = key.first().copied().unwrap_or("");
    let shifted = mods.contains(&"Shift");
    let symbol = match (shifted, key) {
        (true, "1") => Some("!"),
        (true, "2") => Some("@"),
        (true, "4") => Some("$"),
        (true, "5") => Some("%"),
        (true, "7") => Some("&"),
        (true, "9") => Some("("),
        (true, "0") => Some(")"),
        (true, "Comma") => Some("<"),
        (true, "Period") => Some(">"),
        (true, "Quote") => Some("\""),
        _ => None,
    };
    let key_text = symbol.unwrap_or(match key {
        "Plus" => "+",
        "Minus" => "−",
        "Equals" => "=",
        "Period" => ".",
        "Comma" => ",",
        "Slash" => "/",
        "Colon" => ":",
        "Semicolon" => ";",
        "Quote" => "'",
        "Pipe" => "|",
        "Exclamationmark" => "!",
        "OpenBracket" => "[",
        "CloseBracket" => "]",
        "Left" if mac => "←",
        "Right" if mac => "→",
        "Up" if mac => "↑",
        "Down" if mac => "↓",
        "Home" if mac => "↖",
        "End" if mac => "↘",
        "Tab" if mac => "⇥",
        "Backspace" if mac => "⌫",
        "Delete" if mac => "⌦",
        "Enter" if mac => "↵",
        "Escape" => "Esc",
        other => other,
    });
    let mut out = String::new();
    // macOS's order: ⌃⌥⇧⌘.
    for (name, mac_sign) in [("Ctrl", "⌃"), ("Alt", "⌥"), ("Shift", "⇧"), ("Cmd", "⌘")] {
        if !mods.contains(&name) || (name == "Shift" && symbol.is_some()) {
            continue;
        }
        if mac {
            out.push_str(mac_sign);
        } else if !(name == "Cmd" && mods.contains(&"Ctrl")) {
            out.push_str(if name == "Cmd" { "Ctrl" } else { name });
            out.push('+');
        }
    }
    out.push_str(key_text);
    out
}

/// A bare 0–9 key press (not with Cmd, Alt, Ctrl or Shift: Shift+4 is `$`).
pub fn digit_pressed(ctx: &egui::Context) -> Option<u8> {
    use egui::Key::*;
    const KEYS: [egui::Key; 10] = [Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9];
    ctx.input(|i| {
        if i.modifiers.command || i.modifiers.alt || i.modifiers.ctrl || i.modifiers.shift {
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
        let item = |b: &Binding, label: &str| matches!(b, Binding::Item(i) if i.label == label);
        assert!(bindings().iter().any(|(ks, b)| ks.logical_key == egui::Key::P && item(b, "Palm Mute")));
        assert!(bindings().iter().any(|(ks, _)| ks.logical_key == egui::Key::Space));
        let mac = cfg!(target_os = "macos");
        assert_eq!(for_platform("Cmd+Plus;Insert"), if mac { "Cmd+Plus" } else { "Insert" });
    }

    #[test]
    fn every_registry_shortcut_parses() {
        for c in registry() {
            if let Some(s) = c.shortcut {
                assert!(parse(s).is_some(), "{} has unparseable shortcut {s}", c.id);
            }
        }
    }

    #[test]
    fn no_two_actions_share_a_shortcut() {
        let all = bindings();
        for (i, (a, ba)) in all.iter().enumerate() {
            for (b, bb) in all.iter().skip(i + 1) {
                assert!(a != b, "{a:?} is bound to both {ba:?} and {bb:?}");
            }
        }
    }

    #[test]
    fn a_menu_shows_the_shortcut_its_command_answers_to() {
        for item in CATALOG {
            if let Action::Cmd(id, "") = item.action {
                let registered = registry().iter().find(|c| c.id == id).and_then(|c| c.shortcut).unwrap_or("");
                assert_eq!(item.shortcut, registered, "{}: the menu and the command disagree", item.label);
            }
        }
    }

    #[test]
    fn shortcuts_read_as_guitar_pro_prints_them() {
        if cfg!(target_os = "macos") {
            assert_eq!(display("Cmd+Shift+Right"), "⇧⌘→");
            assert_eq!(display("Alt+Shift+Up"), "⌥⇧↑");
            assert_eq!(display("Shift+4"), "$");
            assert_eq!(display("Cmd+Shift+Period"), "⌘>");
            assert_eq!(display("Ctrl+Minus;Ctrl+Delete"), "⌃−");
        } else {
            assert_eq!(display("Cmd+Shift+Right"), "Shift+Ctrl+Right");
            assert_eq!(display("Ctrl+Minus;Ctrl+Delete"), "Ctrl+Delete");
        }
        assert_eq!(display("F7"), "F7");
    }
}
