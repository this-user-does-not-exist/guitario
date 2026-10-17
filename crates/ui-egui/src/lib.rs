//! L6 egui shell, laid out like Guitar Pro: top bar with transport and LCD, edition palette on the
//! left, white score pages in the middle, inspector on the right, fretboard and global track view
//! along the bottom. Thin by design: every edit goes through `Session::run`, and the menus and
//! shortcuts are generated from the engine's command registry.

mod audio_settings;
mod automation_dialog;
mod autosave;
mod chord_dialog;
mod fonts;
mod fretboard;
mod global_view;
mod glyphs;
mod icons;
mod inspector;
mod keys;
mod layout;
mod lyrics_dialog;
mod menu_catalog;
mod menus;
mod mixer;
mod notation;
mod ornaments;
mod page;
mod page_setup;
mod palette;
pub mod parity;
mod pdf;
mod perf;
mod prefs;
mod preview;
mod print_dialog;
mod sounds;
mod spool;
mod theme;
mod topbar;
mod transpose_dialog;

pub use layout::LayoutMode;
pub use pdf::PdfRenderer;
pub use spool::SystemSpooler;

use guitario_engine::Session;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::time::Instant;

pub struct Panels {
    pub palette: bool,
    pub inspector: bool,
    pub global: bool,
    pub fretboard: bool,
}

pub struct ViewState {
    pub zoom: f32,
    pub notation: bool,
    /// View › Layout: pages, one screen-wide sheet, or one line of bars.
    pub layout: layout::LayoutMode,
    /// View › Multitrack: every track in each system.
    pub multitrack: bool,
    /// Edit › Multivoice Editing: voices in colour, and a click picks a note of any voice.
    pub multivoice: bool,
}

/// Transport state. The player itself lives in the audio slot; this mirrors what the UI shows.
pub struct Transport {
    pub playing: bool,
    pub metronome: bool,
    pub count_in: bool,
    pub looping: bool,
    pub speed: f32,
    /// Song time in seconds while playing.
    pub position: f64,
    /// The beat (measure, beat) sounding on the selected track.
    pub beat: Option<(usize, usize)>,
}

pub struct GuitarioApp {
    session: Session,
    status: String,
    status_is_error: bool,
    /// The last typed digit and when, so `1` then `2` within a moment makes fret 12.
    last_digit: Option<(Instant, u8)>,
    follow_cursor: bool,
    fonts: fonts::FontSet,
    fonts_ready: bool,
    pub panels: Panels,
    pub view: ViewState,
    inspector_tab: inspector::Tab,
    edit_buffers: HashMap<&'static str, String>,
    pub transport: Transport,
    audio: Option<guitario_audio::AudioOut>,
    /// Sampled SFZ instruments for drums, bass and guitars; None plays the built-in synth.
    sounds: Option<std::sync::Arc<guitario_playback::SoundBank>>,
    /// Instruments decoding on a worker thread for the score as it is, so Play never waits on them.
    preload: Option<std::thread::JoinHandle<Vec<String>>>,
    /// The score version last checked against the bank.
    preloaded: Option<std::sync::Arc<guitario_score::Score>>,
    /// Play was pressed while sounds were loading: it starts when they are ready.
    play_when_ready: bool,
    settings: prefs::Store,
    /// A New/Open/Close waiting on the unsaved-changes prompt.
    pending: Option<Pending>,
    /// Don't Save was chosen, so the next close may go ahead.
    discard_confirmed: bool,
    about_open: bool,
    print: print_dialog::PrintDialog,
    page_setup: page_setup::PageSetupDialog,
    preview: preview::Preview,
    ornaments: ornaments::OrnamentDialog,
    chord: chord_dialog::ChordDialog,
    lyrics: lyrics_dialog::LyricsDialog,
    transpose: transpose_dialog::TransposeDialog,
    mixer: mixer::Mixer,
    tempo_automation: automation_dialog::TempoAutomation,
    mix_automation: automation_dialog::MixAutomation,
    /// The sound card to play through (Sound › Audio Settings…).
    audio_settings: guitario_audio::AudioSettings,
    audio_dialog: audio_settings::AudioSettingsDialog,
    /// Autosave and crash recovery; off until the persistent preferences are in use.
    autosave: autosave::Autosave,
}

/// What the unsaved-changes prompt will do once the user answers.
#[derive(Clone, Debug, PartialEq)]
enum Pending {
    Close,
    New,
    OpenDialog,
    OpenPath(String),
}

impl GuitarioApp {
    pub fn new(mut session: Session, startup_error: Option<String>) -> Self {
        // The page view and the PDF come from the same engraving code, so the shell owns the renderer.
        session.set_renderer(std::sync::Arc::new(PdfRenderer));
        session.set_spooler(std::sync::Arc::new(SystemSpooler));
        let status_is_error = startup_error.is_some();
        Self {
            session,
            status: startup_error.unwrap_or_else(|| "Ready. Type a fret number to enter a note.".into()),
            status_is_error,
            last_digit: None,
            follow_cursor: true,
            fonts: fonts::FontSet::default(),
            fonts_ready: false,
            panels: Panels { palette: true, inspector: true, global: true, fretboard: true },
            view: ViewState { zoom: 1.25, notation: true, layout: layout::LayoutMode::Page, multitrack: false, multivoice: false },
            inspector_tab: inspector::Tab::Song,
            edit_buffers: HashMap::new(),
            transport: Transport { playing: false, metronome: false, count_in: false, looping: false, speed: 1.0, position: 0.0, beat: None },
            audio: None,
            sounds: None,
            preload: None,
            preloaded: None,
            play_when_ready: false,
            // In memory until the app hands over the real store, so tests and offscreen renders
            // never touch the user's profile.
            settings: prefs::Store::open(craft_settings::Location::Memory).0,
            pending: None,
            discard_confirmed: false,
            about_open: false,
            print: print_dialog::PrintDialog::default(),
            page_setup: page_setup::PageSetupDialog::default(),
            preview: preview::Preview::default(),
            ornaments: ornaments::OrnamentDialog::default(),
            chord: chord_dialog::ChordDialog::default(),
            lyrics: lyrics_dialog::LyricsDialog::default(),
            autosave: autosave::Autosave::off(),
            transpose: transpose_dialog::TransposeDialog::default(),
            mixer: mixer::Mixer::default(),
            tempo_automation: automation_dialog::TempoAutomation::default(),
            mix_automation: automation_dialog::MixAutomation::default(),
            audio_settings: guitario_audio::AudioSettings::default(),
            audio_dialog: audio_settings::AudioSettingsDialog::default(),
        }
    }

    /// Plays drums, bass and guitars from this sound bank (see `guitario_playback::SoundBank`).
    pub fn with_sounds(mut self, bank: Option<guitario_playback::SoundBank>) -> Self {
        self.sounds = bank.map(std::sync::Arc::new);
        self
    }

    /// Uses the sound bank installed for this user, if there is one (`cargo xtask sounds`).
    pub fn with_default_sounds(self) -> Self {
        self.with_sounds(guitario_playback::SoundBank::open_default())
    }

    /// Uses the persistent preferences (the app opens them with `Location::Auto`) and applies
    /// what they hold: view, panels, transport toggles and recent files.
    pub fn with_settings(mut self, store: prefs::Store, report: &craft_settings::LoadReport) -> Self {
        self.settings = store;
        let p = self.settings.get().clone().sanitized();
        self.view =
            ViewState { zoom: p.view.zoom, notation: p.view.notation, layout: p.view.layout, multitrack: p.view.multitrack, multivoice: p.view.multivoice };
        self.panels = Panels { palette: p.view.palette, inspector: p.view.inspector, global: p.view.global_view, fretboard: p.view.fretboard };
        let t = &mut self.transport;
        (t.metronome, t.count_in, t.looping, t.speed) = (p.playback.metronome, p.playback.count_in, p.playback.looping, p.playback.speed);
        self.audio_settings = guitario_audio::AudioSettings { device: p.playback.output.clone(), buffer: p.playback.buffer };
        // Unsaved work is kept beside the preferences, and what a stopped run left is offered back.
        if let Some(dir) = self.settings.dir() {
            self.autosave = autosave::Autosave::at(dir.join("recovery"));
        }
        if report.has_problems() && !self.status_is_error {
            self.status = "Preferences could not all be read; defaults were used where needed.".into();
        }
        self
    }

    /// Autosaves into `dir` instead of beside the preferences, and offers back what stopped runs
    /// left there (agents, tests and the snapshot example).
    pub fn with_autosave_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.autosave = autosave::Autosave::at(dir);
        self
    }

    /// Opens persistent preferences the suite's way: `config.toml` in the platform folder, or
    /// `GUITARIO_CONFIG_DIR`. The first run after the rename imports Guitario's preferences.
    pub fn open_settings() -> (prefs::Store, craft_settings::LoadReport) {
        prefs::Store::open_with(craft_settings::OpenOptions::new(craft_settings::Location::Auto).legacy(prefs::legacy_files()))
    }

    /// Writes the current UI state into the settings store; it saves shortly after a change.
    fn sync_settings(&mut self, ctx: &egui::Context) {
        let (v, pn, t) = (&self.view, &self.panels, &self.transport);
        let view = prefs::ViewPrefs {
            zoom: v.zoom,
            notation: v.notation,
            layout: v.layout,
            multitrack: v.multitrack,
            multivoice: v.multivoice,
            palette: pn.palette,
            inspector: pn.inspector,
            global_view: pn.global,
            fretboard: pn.fretboard,
        };
        let playback = prefs::PlaybackPrefs {
            metronome: t.metronome,
            count_in: t.count_in,
            looping: t.looping,
            speed: t.speed,
            output: self.audio_settings.device.clone(),
            buffer: self.audio_settings.buffer,
        };
        self.settings.update(|p| {
            p.view = view;
            p.playback = playback;
        });
        let tick = self.settings.tick(Instant::now());
        if let craft_settings::SaveStatus::Failed(err) = &tick.status {
            self.status = format!("Preferences not saved: {err}");
            self.status_is_error = true;
        }
        if let Some(deadline) = tick.next_deadline {
            ctx.request_repaint_after(deadline.saturating_duration_since(Instant::now()));
        }
    }

    pub(crate) fn recent_files(&self) -> Vec<String> {
        self.settings.get().files.recent.clone()
    }

    fn remember(&mut self, path: &str) {
        self.settings.update(|p| p.push_recent(path));
    }

    /// Runs `action` now, or asks about unsaved changes first.
    fn guard(&mut self, action: Pending) {
        if self.session.is_dirty() {
            self.pending = Some(action);
        } else {
            self.perform(action);
        }
    }

    fn perform(&mut self, action: Pending) {
        match action {
            Pending::New => {
                self.run("file.new", Value::Null);
            }
            Pending::OpenDialog => self.open_dialog_now(),
            Pending::OpenPath(p) => self.open_path_now(&p),
            Pending::Close => {}
        }
    }

    pub(crate) fn open_path(&mut self, path: &str) {
        self.guard(Pending::OpenPath(path.to_string()));
    }

    fn open_path_now(&mut self, path: &str) {
        if self.run("file.open", json!({"path": path})).is_some() {
            self.remember(path);
        }
    }

    /// Shell actions from the menu catalog.
    pub(crate) fn ui_action(&mut self, a: menu_catalog::UiAction) {
        use menu_catalog::UiAction as U;
        let c = self.session.cursor();
        let header = self.session.score().headers.get(c.measure).cloned();
        let track = self.session.score().tracks.get(c.track).cloned();
        match a {
            U::Open => self.open_dialog(),
            U::OpenRecent
            | U::TimeSignature
            | U::KeySignature
            | U::Section
            | U::TempoChange
            | U::TripletFeel
            | U::Tuplet
            | U::Brush(_)
            | U::AlternateEndings
            | U::Directions => {}
            U::CloseDocument => self.new_document(),
            U::CheckBarDurations => self.check_bar_durations(),
            U::Save => self.save_dialog(false),
            U::SaveAs => self.save_dialog(true),
            U::ExportMidi => self.export_dialog("MIDI", "mid", "file.exportMidi"),
            U::ExportWav => self.export_dialog("WAV audio", "wav", "file.exportWav"),
            U::ExportAudio => self.export_dialog("FLAC audio (lossless)", "flac", "file.exportAudio"),
            U::ExportAscii => self.export_dialog("Text", "txt", "file.exportAscii"),
            U::ExportGp5 => self.export_dialog("Guitar Pro 5", "gp5", "file.exportGp5"),
            U::ExportGp => self.export_dialog("Guitar Pro 8", "gp", "file.exportGp"),
            U::ExportPdf => self.export_pdf_dialog(),
            U::Print => self.open_print(),
            U::PrintPreview => self.open_preview(),
            U::PageSetup => self.open_page_setup(),
            U::Trill => self.open_trill(),
            U::TremoloPicking => self.open_tremolo(),
            U::GraceNote => self.open_grace(),
            U::Fingering => self.open_fingering(),
            U::ChordDiagrams => self.open_chord(),
            U::Lyrics => self.open_lyrics(),
            U::ImportGuitarPro => self.open_dialog(),
            U::Quit => ctx_quit(),
            U::SongInfo => {
                self.panels.inspector = true;
                self.inspector_tab = inspector::Tab::Song;
            }
            U::TrackProperties => {
                self.panels.inspector = true;
                self.inspector_tab = inspector::Tab::Track;
            }
            U::ToggleMute => {
                self.run("track.setMix", json!({"muted": !track.is_some_and(|t| t.muted)}));
            }
            U::ToggleSolo => {
                self.run("track.setMix", json!({"solo": !track.is_some_and(|t| t.solo)}));
            }
            U::ToggleRepeatOpen => {
                self.run("measure.setRepeat", json!({"open": !header.as_ref().is_some_and(|h| h.repeat_open)}));
            }
            U::ToggleRepeatClose => {
                self.run("measure.setRepeat", json!({"close": if header.as_ref().is_some_and(|h| h.repeat_close > 0) { 0 } else { 1 }}));
            }
            U::DoubleDotted => {
                let div = self.current_divisor();
                let dots = if self.current_dots() == 2 { 0 } else { 2 };
                self.run("beat.setDuration", json!({"value": div, "dots": dots}));
            }
            U::Duration(div) => {
                let dots = self.current_dots();
                self.run("beat.setDuration", json!({"value": div, "dots": dots}));
            }
            U::PlayStop => self.toggle_play(),
            U::ToggleMetronome => self.transport.metronome = !self.transport.metronome,
            U::ToggleCountIn => self.transport.count_in = !self.transport.count_in,
            U::ToggleLoop => self.transport.looping = !self.transport.looping,
            U::ZoomIn => self.view.zoom = (self.view.zoom * 1.25).min(4.0),
            U::ZoomOut => self.view.zoom = (self.view.zoom / 1.25).max(0.25),
            U::ZoomReset => self.view.zoom = 1.0,
            U::ToggleNotation => self.view.notation = !self.view.notation,
            U::Layout(mode) => {
                self.view.layout = mode;
                self.follow_cursor = true;
            }
            U::ToggleMultitrack => {
                self.view.multitrack = !self.view.multitrack;
                self.follow_cursor = true;
            }
            U::ToggleMultivoice => self.view.multivoice = !self.view.multivoice,
            U::Transpose => self.open_transpose(transpose_dialog::Scope::Selection),
            U::TransposeTrack => self.open_transpose(transpose_dialog::Scope::Track),
            U::Mixer => self.mixer.open = !self.mixer.open,
            U::TempoAutomation => self.open_tempo_automation(),
            U::VolumeAutomation => self.open_mix_automation(automation_dialog::MixKind::Volume),
            U::PanAutomation => self.open_mix_automation(automation_dialog::MixKind::Pan),
            U::AudioSettings => self.open_audio_settings(),
            U::TogglePalette => self.panels.palette = !self.panels.palette,
            U::ToggleInspector => self.panels.inspector = !self.panels.inspector,
            U::ToggleGlobalView => self.panels.global = !self.panels.global,
            U::ToggleFretboard => self.panels.fretboard = !self.panels.fretboard,
            U::About => self.about_open = true,
        }
    }

    /// Tools › Check Bar Durations: reports incomplete bars and moves to the first one.
    fn check_bar_durations(&mut self) {
        let Some(r) = self.run("tools.checkBarDurations", Value::Null) else { return };
        let bars = r.get("bars").and_then(Value::as_array).cloned().unwrap_or_default();
        match bars.first() {
            None => self.status = "Every bar is complete.".into(),
            Some(first) => {
                let track = first.get("track").and_then(Value::as_u64).unwrap_or(0);
                let measure = first.get("measure").and_then(Value::as_u64).unwrap_or(0);
                self.run("cursor.set", json!({"track": track, "measure": measure, "beat": 0}));
                self.status = format!("{} incomplete bar(s); the first is bar {} of track {}.", bars.len(), measure + 1, track + 1);
                self.status_is_error = true;
            }
        }
    }

    fn export_dialog(&mut self, kind: &str, ext: &str, id: &str) {
        let stem = self.session.path().and_then(|p| p.file_stem()).map_or_else(|| "score".to_string(), |s| s.to_string_lossy().into_owned());
        if let Some(p) = rfd::FileDialog::new().add_filter(kind, &[ext]).set_file_name(format!("{stem}.{ext}")).save_file() {
            self.run(id, json!({"path": p.display().to_string()}));
        }
    }

    /// Exports what the page view shows as PDF: the track on screen, or in the multitrack view
    /// every track as a full score; with the page view's notation setting.
    fn export_pdf_dialog(&mut self) {
        let stem = self.session.path().and_then(|p| p.file_stem()).map_or_else(|| "score".to_string(), |s| s.to_string_lossy().into_owned());
        if let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{stem}.pdf")).save_file() {
            let together = self.view.multitrack;
            let tracks = if together { Value::Null } else { json!([self.session.cursor().track]) };
            self.run("file.exportPdf", json!({"path": p.display().to_string(), "tracks": tracks, "notation": self.view.notation, "together": together}));
        }
    }

    fn current_divisor(&self) -> u32 {
        let c = self.session.cursor();
        self.session
            .score()
            .tracks
            .get(c.track)
            .and_then(|t| t.measures.get(c.measure))
            .and_then(|m| m.voices.get(c.voice))
            .and_then(|v| v.beats.get(c.beat))
            .map_or(4, |b| b.duration.value.divisor())
    }

    /// The unsaved-changes prompt, Guitar Pro style: Save, Don't Save, Cancel.
    fn prompt(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending.clone() else { return };
        let name = self.doc_name();
        let mut answer = None;
        egui::Modal::new(egui::Id::new("unsaved-changes")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading(format!("Save changes to \u{201c}{name}\u{201d}?"));
            ui.label("Your changes will be lost if you don't save them.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    answer = Some(1);
                }
                if ui.button("Don't Save").clicked() {
                    answer = Some(2);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    answer = Some(3);
                }
            });
        });
        match answer {
            Some(1) => {
                self.save_dialog(false);
                if !self.session.is_dirty() {
                    self.pending = None;
                    self.finish(pending, ctx);
                }
            }
            Some(2) => {
                self.pending = None;
                self.discard_confirmed = true;
                self.finish(pending, ctx);
            }
            Some(_) => {
                self.pending = None;
                self.discard_confirmed = false;
            }
            None => {}
        }
    }

    fn finish(&mut self, pending: Pending, ctx: &egui::Context) {
        if pending == Pending::Close {
            self.discard_confirmed = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            self.discard_confirmed = false;
            self.perform(pending);
        }
    }

    fn about(&mut self, ctx: &egui::Context) {
        let mut open = self.about_open;
        egui::Window::new("About Guitario").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.heading("Guitario");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.label("Tablature, notation and playback; a clean-room take on Guitar Pro in pure Rust.");
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("MIT OR Apache-2.0. Music symbols use a SMuFL font found on this computer (Bravura or Leland, SIL OFL) when present.")
                    .small(),
            );
            ui.label(egui::RichText::new("Guitar Pro is a trademark of Arobas Music; Guitario is not affiliated with it.").small());
        });
        self.about_open = open;
    }

    /// Installs the theme and fonts. Call once before the first frame (the app does it from the
    /// eframe creation callback); `show` also does it lazily and skips that one frame.
    pub fn setup(&mut self, ctx: &egui::Context) {
        theme::apply(ctx);
        self.fonts = fonts::install(ctx);
        self.fonts_ready = true;
    }

    /// Runs a command, reporting any error in the status bar.
    pub(crate) fn run(&mut self, id: &str, params: Value) -> Option<Value> {
        match self.session.run(id, &params) {
            Ok(v) => {
                self.status_is_error = false;
                self.follow_cursor = true;
                self.status.clear();
                if let Some(w) = v.get("warnings").and_then(Value::as_array).filter(|w| !w.is_empty()) {
                    let done = if id.starts_with("file.export") { "Exported" } else { "Imported" };
                    self.status = format!("{done} with {} warning(s): {}", w.len(), w.first().and_then(Value::as_str).unwrap_or_default());
                }
                Some(v)
            }
            Err(e) => {
                self.status = e.to_string();
                self.status_is_error = true;
                None
            }
        }
    }

    pub(crate) fn open_dialog(&mut self) {
        self.guard(Pending::OpenDialog);
    }

    fn open_dialog_now(&mut self) {
        let mut kinds = guitario_engine::NATIVE_EXTENSIONS.to_vec();
        kinds.extend(guitario_engine::GUITAR_PRO_EXTENSIONS);
        let dialog = rfd::FileDialog::new().add_filter("Guitar scores", &kinds).add_filter("Guitar Pro", &guitario_engine::GUITAR_PRO_EXTENSIONS);
        if let Some(p) = dialog.pick_file() {
            self.open_path_now(&p.display().to_string());
        }
    }

    pub(crate) fn new_document(&mut self) {
        self.guard(Pending::New);
    }

    pub(crate) fn save_dialog(&mut self, as_new: bool) {
        if !as_new && self.session.path().is_some() {
            self.run("file.save", Value::Null);
        } else if let Some(p) = {
            // Suggest the song title as the file name, as Guitar Pro does.
            let title = self.session.score().meta.title.trim().to_string();
            let stem = if title.is_empty() { "Untitled".to_string() } else { title };
            rfd::FileDialog::new().add_filter("Guitario", &["gt"]).set_file_name(format!("{stem}.gt")).save_file()
        } {
            let path = p.display().to_string();
            if self.run("file.save", json!({"path": path})).is_some() {
                self.remember(&path);
            }
        }
    }

    fn options(&self) -> guitario_playback::Options {
        let t = &self.transport;
        guitario_playback::Options { metronome: t.metronome, count_in: t.count_in, looping: t.looping, speed: t.speed }
    }

    /// Space: play from the cursor, or stop and leave the cursor where playback was.
    pub(crate) fn toggle_play(&mut self) {
        if self.transport.playing {
            self.stop();
            return;
        }
        if self.play_when_ready {
            self.play_when_ready = false;
            self.status = "Playback cancelled.".into();
            return;
        }
        // Sampled instruments still decoding: start once they are ready instead of freezing.
        if let Some(bank) = &self.sounds
            && !bank.ready(self.session.score())
        {
            self.play_when_ready = true;
            self.preloaded = None;
            self.status = "Loading sounds\u{2026}".into();
            self.status_is_error = false;
            return;
        }
        if self.audio.is_none() {
            match guitario_audio::AudioOut::open_with(&self.audio_settings) {
                Ok(a) => {
                    if a.fell_back {
                        self.status = format!("The chosen audio output is not connected; playing through {}.", a.device);
                    }
                    self.audio = Some(a);
                }
                Err(e) => {
                    self.status = format!("Cannot play: {e}");
                    self.status_is_error = true;
                    return;
                }
            }
        }
        let Some(audio) = &self.audio else { return };
        let c = self.session.cursor();
        let mut player = guitario_playback::Player::new(self.session.score(), audio.sample_rate, self.options());
        let sound_notes = self.sounds.as_ref().map(|b| b.apply(&mut player, self.session.score())).unwrap_or_default();
        // With loop on and a selection, play just the selection, round and round.
        let range = self.session.selection().filter(|s| self.transport.looping && s.track == c.track).map(|s| s.ordered());
        let from = match range {
            Some(((m0, b0), (m1, b1))) => {
                if let Some(end) = player.time_after(c.track, m1, b1) {
                    player.set_end(end);
                }
                player.time_of(c.track, m0, b0).unwrap_or(0.0)
            }
            None => player.time_of(c.track, c.measure, c.beat).unwrap_or(0.0),
        };
        player.play_from(from);
        self.transport.position = player.position();
        if let Some(first) = sound_notes.first() {
            self.status = format!("Sound bank: {first}");
            self.status_is_error = true;
        }
        *audio.slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(player);
        self.transport.playing = true;
        self.transport.beat = Some((c.measure, c.beat));
    }

    pub(crate) fn stop(&mut self) {
        if let Some(audio) = &self.audio
            && let Some(p) = audio.slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_mut()
        {
            p.stop();
        }
        self.transport.playing = false;
        if let Some((measure, beat)) = self.transport.beat.take() {
            self.run("cursor.set", json!({"measure": measure, "beat": beat}));
        }
    }

    /// Mirrors the player into the transport each frame and keeps repainting while it plays.
    fn poll_playback(&mut self, ctx: &egui::Context) {
        if !self.transport.playing {
            return;
        }
        let track = self.session.cursor().track;
        let opts = self.options();
        // The mixer is live: volume, pan, mute and solo changes (and a fader being dragged) are heard.
        let mix = self.live_mix();
        let state = self.audio.as_ref().map(|a| {
            let mut guard = a.slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.as_mut().map(|p| {
                p.opts = opts;
                p.set_mix(&mix);
                (p.is_playing(), p.position(), p.current_beat(track).map(|b| (b.measure, b.beat)))
            })
        });
        match state {
            Some(Some((true, pos, beat))) => {
                self.transport.position = pos;
                if beat.is_some() {
                    self.transport.beat = beat;
                }
                ctx.request_repaint();
            }
            Some(_) => self.stop(),
            // No device (offscreen rendering): leave the transport as set.
            None => {}
        }
    }

    fn doc_name(&self) -> String {
        self.session.path().and_then(|p| p.file_name()).map_or_else(|| "Untitled".to_string(), |n| n.to_string_lossy().into_owned())
    }

    fn title(&self) -> String {
        format!("{}{} — Guitario", self.doc_name(), if self.session.is_dirty() { " •" } else { "" })
    }

    /// A dialog has the window: keys must not reach the score behind it.
    fn modal_open(&self) -> bool {
        self.pending.is_some()
            || self.print.open
            || self.preview.open
            || self.page_setup.open
            || self.ornaments.is_open()
            || self.chord.open
            || self.lyrics.open
            || !self.autosave.found.is_empty()
            || self.transpose.open
            || self.audio_dialog.open
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        for (id, ks) in keys::bindings() {
            if ctx.input_mut(|i| i.consume_shortcut(&ks)) {
                if id == "file.new" {
                    self.new_document();
                } else {
                    self.run(id, Value::Null);
                }
            }
        }
        for (ks, item) in keys::catalog_bindings() {
            if ctx.input_mut(|i| i.consume_shortcut(&ks)) {
                match item.action {
                    menu_catalog::Action::Cmd(id, params) => {
                        self.run(id, serde_json::from_str(params).unwrap_or(Value::Null));
                    }
                    menu_catalog::Action::Ui(a) => self.ui_action(a),
                    menu_catalog::Action::Missing => {}
                }
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
            self.run("note.clear", Value::Null);
        }
        if let Some(d) = keys::digit_pressed(ctx) {
            let now = Instant::now();
            let c = self.session.cursor();
            let drums = self.session.score().tracks.get(c.track).is_some_and(guitario_score::Track::is_drums);
            let recent = self.last_digit.filter(|(t, _)| now.duration_since(*t).as_millis() < 900).map(|(_, p)| p);
            if drums {
                // Drum hits are General MIDI numbers, two digits as in Guitar Pro: 3 then 6 is a kick.
                match recent {
                    Some(prev) if prev > 0 => {
                        self.last_digit = None;
                        self.run("note.setDrum", json!({"drum": prev * 10 + d}));
                    }
                    _ => self.last_digit = Some((now, d)),
                }
            } else {
                let fret = match recent {
                    Some(prev) if prev * 10 + d <= 36 => prev * 10 + d,
                    _ => d,
                };
                self.last_digit = Some((now, d));
                self.run("note.setFret", json!({"fret": fret}));
            }
        }
    }

    fn current_dots(&self) -> u8 {
        let c = self.session.cursor();
        self.session
            .score()
            .tracks
            .get(c.track)
            .and_then(|t| t.measures.get(c.measure))
            .and_then(|m| m.voices.get(c.voice))
            .and_then(|v| v.beats.get(c.beat))
            .map_or(0, |b| b.duration.dots)
    }

    /// One frame of the whole window. Public so tests and the snapshot example can drive it
    /// without an `eframe::Frame`.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.fonts_ready {
            self.setup(&ctx);
            ctx.request_repaint();
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
        if !self.modal_open() {
            self.handle_keys(&ctx);
        }
        self.sounds_tick(&ctx);
        self.poll_playback(&ctx);
        self.sync_settings(&ctx);
        self.autosave_tick(&ctx);
        self.recovery_dialog(&ctx);
        self.prompt(&ctx);
        self.about(&ctx);
        self.preview_dialog(&ctx);
        self.page_setup_dialog(&ctx);
        self.ornament_dialog(&ctx);
        self.chord_dialog(&ctx);
        self.lyrics_dialog(&ctx);
        self.transpose_dialog(&ctx);
        self.mixer_window(&ctx);
        self.tempo_automation_dialog(&ctx);
        self.mix_automation_dialog(&ctx);
        self.audio_settings_dialog(&ctx);
        self.print_dialog(&ctx);
        let t = theme::tokens();
        let bar = |fill| egui::Frame::NONE.fill(fill).inner_margin(egui::Margin::symmetric(6, 0));

        egui::Panel::top("menu").frame(bar(t.chrome_dark)).show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| menus::menu_bar(self, ui));
        });
        egui::Panel::top("topbar").exact_size(54.0).frame(bar(t.chrome)).show(ui, |ui| topbar::show(self, ui));
        egui::Panel::top("doctabs").exact_size(26.0).frame(bar(t.chrome_dark)).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let name = format!("{}{}", self.doc_name(), if self.session.is_dirty() { " *" } else { "" });
                let (r, _) = ui.allocate_exact_size(egui::vec2(180.0, 24.0), egui::Sense::hover());
                ui.painter().rect_filled(r.with_min_y(r.min.y + 2.0), egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, t.panel);
                ui.painter().rect_filled(egui::Rect::from_min_size(r.min + egui::vec2(0.0, 2.0), egui::vec2(r.width(), 2.0)), 0.0, t.accent);
                ui.painter().text(r.left_center() + egui::vec2(10.0, 1.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(12.0), t.text);
            });
        });
        egui::Panel::bottom("status").exact_size(22.0).frame(bar(t.chrome_dark)).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let c = self.session.cursor();
                let text = egui::RichText::new(&self.status).small();
                ui.label(if self.status_is_error { text.color(egui::Color32::from_rgb(0xff, 0x7a, 0x6a)) } else { text.color(t.text_dim) });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let sel = self.session.selection().map(|s| {
                        let ((m0, b0), (m1, b1)) = s.ordered();
                        let beats_in = |m: usize| {
                            self.session
                                .score()
                                .tracks
                                .get(s.track)
                                .and_then(|t| t.measures.get(m))
                                .and_then(|x| x.voices.get(c.voice))
                                .map_or(1, |v| v.beats.len())
                        };
                        let n: usize = (m0..=m1)
                            .map(|m| {
                                let first = if m == m0 { b0 } else { 0 };
                                let last = if m == m1 { b1 + 1 } else { beats_in(m) };
                                last.saturating_sub(first)
                            })
                            .sum();
                        format!("{n} beats selected · ")
                    });
                    let voice = if c.voice > 0 { format!(" · Voice {}", c.voice + 1) } else { String::new() };
                    ui.label(
                        egui::RichText::new(format!("{}Bar {} · Beat {} · String {}{voice}", sel.unwrap_or_default(), c.measure + 1, c.beat + 1, c.string + 1))
                            .small()
                            .color(t.text_dim),
                    );
                });
            });
        });
        if self.panels.global {
            egui::Panel::bottom("global")
                .resizable(true)
                .default_size(150.0)
                .frame(egui::Frame::NONE.fill(t.chrome_dark))
                .show(ui, |ui| global_view::show(self, ui));
        }
        if self.panels.fretboard {
            egui::Panel::bottom("fretboard").exact_size(132.0).frame(egui::Frame::NONE.fill(t.chrome_dark)).show(ui, |ui| fretboard::show(self, ui));
        }
        if self.panels.palette {
            egui::Panel::left("palette")
                .exact_size(80.0)
                .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::same(5)))
                .show(ui, |ui| palette::show(self, ui));
        }
        if self.panels.inspector {
            egui::Panel::right("inspector")
                .default_size(270.0)
                .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::same(8)))
                .show(ui, |ui| inspector::show(self, ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.workspace)).show(ui, |ui| {
            let play = self.transport.beat.filter(|_| self.transport.playing);
            let selection = self.session.selection().filter(|s| s.track == self.session.cursor().track).map(|s| s.ordered());
            let view = page::View {
                zoom: self.view.zoom,
                notation: self.view.notation,
                fonts: self.fonts,
                play,
                selection,
                layout: self.view.layout,
                multitrack: self.view.multitrack,
                multivoice: self.view.multivoice,
            };
            let score = self.session.score().clone();
            let cursor = self.session.cursor();
            let follow = self.follow_cursor;
            let out = egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                let out = page::show(ui, &score, cursor, &view);
                if let Some(r) = out.play_rect {
                    ui.scroll_to_rect(r.expand(30.0), None);
                } else if follow && let Some(r) = out.cursor_rect {
                    ui.scroll_to_rect(r.expand(30.0), None);
                }
                out
            });
            self.follow_cursor = false;
            if let Some(page::Click::Beat { track, measure, beat, string, voice, shift }) = out.inner.click {
                if shift && track == self.session.cursor().track {
                    // Shift+click extends from the selection's anchor (or the cursor).
                    let c = self.session.cursor();
                    let anchor = self.session.selection().map_or((c.measure, c.beat), |s| s.anchor);
                    self.run("select.set", json!({"fromMeasure": anchor.0, "fromBeat": anchor.1, "toMeasure": measure, "toBeat": beat}));
                } else {
                    // A null voice keeps the cursor's.
                    self.run("cursor.set", json!({"track": track, "measure": measure, "beat": beat, "string": string, "voice": voice}));
                }
                self.follow_cursor = false;
            }
            if let Some((from, to)) = out.inner.drag
                && self.session.selection().map(|s| (s.anchor, s.head)) != Some((from, to))
            {
                self.run("select.set", json!({"fromMeasure": from.0, "fromBeat": from.1, "toMeasure": to.0, "toBeat": to.1}));
                self.follow_cursor = false;
            }
        });
    }
}

/// File › Quit: close the window the same way the close button does, so the unsaved-changes
/// check runs; `craft_quit::on_close_request` then quits the platform's way.
fn ctx_quit() {
    QUIT_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
}

static QUIT_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl eframe::App for GuitarioApp {
    /// Runs every frame, even minimized: the close decision lives here (craft-quit's contract),
    /// with `on_close_request` last so it sees our `CancelClose`.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if QUIT_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.session.is_dirty() && !self.discard_confirmed {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Close);
        }
        craft_quit::on_close_request(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }

    fn on_exit(&mut self) {
        self.stop();
        // Saved, or Don't Save was chosen: nothing to recover. Otherwise the copy stays.
        if !self.session.is_dirty() || self.discard_confirmed {
            self.autosave.finish();
        }
        let _ = self.settings.flush();
    }
}
