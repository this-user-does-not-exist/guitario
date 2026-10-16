//! Dependency layering rules.
//!
//! The rule engine works on a small model so it can be unit-tested; `from_metadata` builds that
//! model from `cargo metadata`. A crate may depend only on lower layers (or earlier crates of an
//! explicit chain within its layer); standalone crates depend on no workspace crate; nothing below
//! the shell may use a UI toolkit.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Layer(u8),
    /// Layer 0 that depends on no workspace crate at all (publishable on its own).
    Standalone,
    /// Binaries and tooling: exempt.
    Exempt,
}

impl Class {
    fn layer(self) -> Option<u8> {
        match self {
            Class::Layer(l) => Some(l),
            Class::Standalone => Some(0),
            Class::Exempt => None,
        }
    }
}

/// Package names without the `guitario-` prefix. A new crate must be added here.
pub const TABLE: &[(&str, Class)] = &[
    ("theory", Class::Standalone),
    ("score", Class::Layer(1)),
    ("midi", Class::Layer(2)),
    ("playback", Class::Layer(2)),
    ("format", Class::Layer(3)),
    ("gp", Class::Layer(3)),
    // The sound card: the only crate below the shell that touches platform audio.
    ("audio", Class::Layer(4)),
    ("engine", Class::Layer(5)),
    ("ui-egui", Class::Layer(6)),
    ("guitario", Class::Exempt),
    ("cli", Class::Exempt),
    ("xtask", Class::Exempt),
];

/// External crates that make up a UI toolkit. Entries ending in `*` are prefixes.
pub const UI_CRATES: &[&str] = &["egui", "eframe", "winit", "egui_kittest", "craft-ui", "craft-quit", "rfd", "egui_*"];
pub const UI_MIN_LAYER: u8 = 6;

/// Sound-card crates, allowed only from the audio layer up.
pub const AUDIO_CRATES: &[&str] = &["cpal"];
pub const AUDIO_MIN_LAYER: u8 = 4;

pub fn short_name(pkg: &str) -> &str {
    pkg.strip_prefix("guitario-").unwrap_or(pkg)
}

pub fn classify(pkg: &str) -> Option<Class> {
    let s = short_name(pkg);
    TABLE.iter().find(|(n, _)| *n == s).map(|(_, c)| *c)
}

fn matches(list: &[&str], name: &str) -> bool {
    list.iter().any(|p| match p.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == *p,
    })
}

#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub dev: bool,
    pub workspace: bool,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub deps: Vec<Dep>,
}

pub fn from_metadata(meta: &Value) -> Result<Vec<Package>, String> {
    let pkgs = meta.get("packages").and_then(serde_json::Value::as_array).ok_or("cargo metadata: no packages")?;
    let names: Vec<&str> = pkgs.iter().filter_map(|p| p.get("name").and_then(serde_json::Value::as_str)).collect();
    Ok(pkgs
        .iter()
        .filter_map(|p| {
            let name = p.get("name").and_then(serde_json::Value::as_str)?.to_string();
            let deps = p
                .get("dependencies")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|d| {
                    let n = d.get("name").and_then(serde_json::Value::as_str)?.to_string();
                    let workspace = names.contains(&n.as_str());
                    Some(Dep { dev: d.get("kind").and_then(serde_json::Value::as_str) == Some("dev"), workspace, name: n })
                })
                .collect();
            Some(Package { name, deps })
        })
        .collect())
}

pub fn check(pkgs: &[Package]) -> Vec<String> {
    let mut errors = Vec::new();
    for p in pkgs {
        let Some(class) = classify(&p.name) else {
            errors.push(format!("{}: not in the layering table (xtask/src/layers.rs)", p.name));
            continue;
        };
        let Some(layer) = class.layer() else { continue };
        for d in &p.deps {
            if d.workspace {
                if d.dev {
                    continue;
                }
                if class == Class::Standalone {
                    errors.push(format!("{}: standalone crate depends on workspace crate {}", p.name, d.name));
                    continue;
                }
                match classify(&d.name).and_then(Class::layer) {
                    Some(dl) if dl < layer => {}
                    Some(dl) => errors.push(format!("{} (L{layer}) depends on {} (L{dl}): only lower layers are allowed", p.name, d.name)),
                    None => errors.push(format!("{} depends on unlayered workspace crate {}", p.name, d.name)),
                }
            } else if !d.dev && matches(UI_CRATES, &d.name) && layer < UI_MIN_LAYER {
                errors.push(format!("{} (L{layer}) uses UI crate {}: only L{UI_MIN_LAYER} may", p.name, d.name));
            } else if !d.dev && matches(AUDIO_CRATES, &d.name) && layer < AUDIO_MIN_LAYER {
                errors.push(format!("{} (L{layer}) uses audio crate {}: only L{AUDIO_MIN_LAYER}+ may", p.name, d.name));
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, deps: &[(&str, bool, bool)]) -> Package {
        Package { name: name.into(), deps: deps.iter().map(|(n, ws, dev)| Dep { name: (*n).into(), workspace: *ws, dev: *dev }).collect() }
    }

    #[test]
    fn lower_layers_are_fine() {
        assert!(check(&[pkg("guitario-engine", &[("guitario-score", true, false), ("serde", false, false)])]).is_empty());
    }

    #[test]
    fn upward_and_sideways_deps_fail() {
        assert_eq!(check(&[pkg("guitario-score", &[("guitario-engine", true, false)])]).len(), 1);
        assert_eq!(check(&[pkg("guitario-midi", &[("guitario-playback", true, false)])]).len(), 1);
    }

    #[test]
    fn standalone_and_ui_rules() {
        assert_eq!(check(&[pkg("guitario-theory", &[("guitario-score", true, false)])]).len(), 1);
        assert_eq!(check(&[pkg("guitario-engine", &[("egui", false, false)])]).len(), 1);
        assert!(check(&[pkg("guitario-ui-egui", &[("egui", false, false), ("eframe", false, false)])]).is_empty());
        assert_eq!(check(&[pkg("guitario-playback", &[("cpal", false, false)])]).len(), 1);
        assert!(check(&[pkg("guitario-audio", &[("cpal", false, false)])]).is_empty());
        // Dev-dependencies are free: tests may use anything.
        assert!(check(&[pkg("guitario-engine", &[("egui_kittest", false, true)])]).is_empty());
    }

    #[test]
    fn unknown_crates_must_be_registered() {
        assert_eq!(check(&[pkg("guitario-newthing", &[])]).len(), 1);
        assert!(check(&[pkg("xtask", &[("guitario-engine", true, false)])]).is_empty());
    }
}
