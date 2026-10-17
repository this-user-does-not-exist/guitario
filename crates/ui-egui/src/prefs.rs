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
    pub ui: UiPrefs,
}

/// File › Preferences…: how the app looks.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(default)]
pub struct UiPrefs {
    pub theme: crate::theme::Theme,
    /// Scale of every menu, panel and dialog (the score has its own zoom).
    pub scale: f32,
}

impl Default for UiPrefs {
    fn default() -> Self {
        UiPrefs { theme: crate::theme::Theme::Dark, scale: 1.0 }
    }
}

pub const MIN_UI_SCALE: f32 = 0.75;
pub const MAX_UI_SCALE: f32 = 2.0;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct ViewPrefs {
    pub zoom: f32,
    pub layout: crate::layout::LayoutMode,
    pub multitrack: bool,
    /// Edit › Multivoice Editing.
    pub multivoice: bool,
    pub palette: bool,
    pub inspector: bool,
    pub global_view: bool,
    pub fretboard: bool,
    /// View › Instrument View › Keyboard.
    pub keyboard: bool,
    /// The instrument views' display mode and left-handed fretboard.
    pub instrument: crate::instrument::Display,
    pub left_handed: bool,
}

impl Default for ViewPrefs {
    fn default() -> Self {
        ViewPrefs {
            zoom: 1.25,
            layout: crate::layout::LayoutMode::Page,
            multitrack: false,
            multivoice: false,
            palette: true,
            inspector: true,
            global_view: true,
            fretboard: true,
            keyboard: false,
            instrument: crate::instrument::Display::Beat,
            left_handed: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct PlaybackPrefs {
    pub metronome: bool,
    pub count_in: bool,
    pub looping: bool,
    pub speed: f32,
    /// The toolbar's relative tonality, in semitones.
    pub transpose: i8,
    /// Sound › Audio Settings…: the output device by name (none is the system default) and the
    /// buffer size in frames (none lets the device choose).
    pub output: Option<String>,
    pub buffer: Option<u32>,
}

impl Default for PlaybackPrefs {
    fn default() -> Self {
        PlaybackPrefs { metronome: false, count_in: false, looping: false, speed: 1.0, transpose: 0, output: None, buffer: None }
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
        self.ui.scale = if self.ui.scale.is_finite() { self.ui.scale.clamp(MIN_UI_SCALE, MAX_UI_SCALE) } else { 1.0 };
        let p = &mut self.playback;
        p.speed = if p.speed.is_finite() { p.speed.clamp(0.25, 2.0) } else { 1.0 };
        p.buffer = p.buffer.map(|b| b.clamp(32, 8192));
        p.transpose = p.transpose.clamp(-12, 12);
        p.output = p.output.take().filter(|o| !o.trim().is_empty() && o.len() <= 256);
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

/// Preferences saved before the app was renamed from Guitario: read once, when Guitario has no
/// `config.toml` of its own yet. The old file is left in place.
pub fn legacy_files() -> Vec<craft_settings::LegacyFile<Prefs>> {
    craft_settings::Probe::default()
        .platform_config_dir
        .map(|d| vec![craft_settings::LegacyFile { path: d.join("guitario").join("config.toml"), parse: parse_config }])
        .unwrap_or_default()
}

/// A `config.toml` written by this app under another name; unreadable sections fall back to defaults.
fn parse_config(bytes: &[u8]) -> Result<Prefs, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let doc = craft_settings::document::parse(text).map_err(|e| e.to_string())?;
    Ok(craft_settings::document::deserialize_sections::<Prefs>(&doc).0.sanitized())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_hostile_values() {
        let mut p = Prefs::default();
        p.view.zoom = f32::NAN;
        p.playback.speed = 99.0;
        p.playback.transpose = -100;
        p.ui.scale = f32::INFINITY;
        p.files.recent = (0..50).map(|i| i.to_string()).collect();
        let p = p.sanitized();
        assert_eq!(p.view.zoom, 1.25);
        assert_eq!(p.playback.speed, 2.0);
        assert_eq!(p.playback.transpose, -12);
        assert_eq!(p.ui.scale, 1.0);
        let mut big = Prefs::default();
        big.ui.scale = 9.0;
        assert_eq!(big.sanitized().ui.scale, MAX_UI_SCALE);
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
    fn guitario_preferences_carry_over() {
        let mut old = Prefs::default();
        old.view.zoom = 2.0;
        old.push_recent("/songs/riff.gt");
        let mut doc = craft_settings::document::new_document(1);
        craft_settings::document::merge_into(&mut doc, &old).unwrap();
        let read = parse_config(craft_settings::document::render(&doc).as_bytes()).unwrap();
        assert_eq!(read, old);
        assert!(parse_config(b"\xff\xfe").is_err());
        assert!(legacy_files().iter().all(|f| f.path.ends_with("guitario/config.toml")));
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
