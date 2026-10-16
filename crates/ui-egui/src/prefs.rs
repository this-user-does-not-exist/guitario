//! Preferences that survive a restart, stored by the suite's `craft-settings` in
//! `config.toml` (platform config folder; `GUITARIO_CONFIG_DIR` overrides it, and an empty value
//! keeps everything in memory for tests and agents).

use craft_settings::{Settings, SettingsStore};
use serde::{Deserialize, Serialize};

pub const MAX_RECENT: usize = 10;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Prefs {
    pub view: ViewPrefs,
    pub playback: PlaybackPrefs,
    pub files: FilePrefs,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct ViewPrefs {
    pub zoom: f32,
    pub notation: bool,
    pub palette: bool,
    pub inspector: bool,
    pub global_view: bool,
    pub fretboard: bool,
}

impl Default for ViewPrefs {
    fn default() -> Self {
        ViewPrefs { zoom: 1.25, notation: true, palette: true, inspector: true, global_view: true, fretboard: true }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct PlaybackPrefs {
    pub metronome: bool,
    pub count_in: bool,
    pub looping: bool,
    pub speed: f32,
}

impl Default for PlaybackPrefs {
    fn default() -> Self {
        PlaybackPrefs { metronome: false, count_in: false, looping: false, speed: 1.0 }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct FilePrefs {
    /// Most recent first.
    pub recent: Vec<String>,
}

impl Settings for Prefs {
    const APP_ID: &'static str = "guitario";
    const SCHEMA_VERSION: u32 = 1;
}

impl Prefs {
    /// Clamps values a hand-edited file could set out of range.
    pub fn sanitized(mut self) -> Prefs {
        let v = &mut self.view;
        v.zoom = if v.zoom.is_finite() { v.zoom.clamp(0.25, 4.0) } else { 1.25 };
        let p = &mut self.playback;
        p.speed = if p.speed.is_finite() { p.speed.clamp(0.25, 2.0) } else { 1.0 };
        self.files.recent.truncate(MAX_RECENT);
        self
    }

    pub fn push_recent(&mut self, path: &str) {
        self.files.recent.retain(|p| p != path);
        self.files.recent.insert(0, path.to_string());
        self.files.recent.truncate(MAX_RECENT);
    }
}

pub type Store = SettingsStore<Prefs>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_hostile_values() {
        let mut p = Prefs::default();
        p.view.zoom = f32::NAN;
        p.playback.speed = 99.0;
        p.files.recent = (0..50).map(|i| i.to_string()).collect();
        let p = p.sanitized();
        assert_eq!(p.view.zoom, 1.25);
        assert_eq!(p.playback.speed, 2.0);
        assert_eq!(p.files.recent.len(), MAX_RECENT);
    }

    #[test]
    fn recent_files_move_to_the_front() {
        let mut p = Prefs::default();
        p.push_recent("a");
        p.push_recent("b");
        p.push_recent("a");
        assert_eq!(p.files.recent, vec!["a", "b"]);
    }

    #[test]
    fn memory_store_round_trip() {
        let (mut store, report) = Store::open(craft_settings::Location::Memory);
        assert!(!report.has_problems());
        store.update(|p| p.view.zoom = 2.0);
        assert_eq!(store.get().view.zoom, 2.0);
        assert!(store.flush().is_ok());
    }
}
