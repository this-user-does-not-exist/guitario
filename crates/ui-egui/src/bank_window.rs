//! The Sound Bank window: the folder the sampled sounds live in, what plays each kind of track,
//! the recommended open libraries with their licences, installing them on a worker thread with
//! progress (a download that breaks off carries on next time), and the credits their licences ask
//! for. At first start, when no bank is installed, the app offers to download them, as Guitar Pro's
//! installer offers its sound banks.

use guitario_playback::install::{LIBRARIES, Library, Step, install, write_bank};
use guitario_playback::{Role, SoundBank};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// How a role is named to people.
pub(crate) fn role_label(role: Role) -> &'static str {
    match role {
        Role::Drums => "Drum kit",
        Role::Bass => "Bass",
        Role::GuitarClean => "Clean guitar",
        Role::GuitarDistorted => "Distorted guitar",
    }
}

/// What the install worker has done so far.
#[derive(Default)]
struct JobState {
    current: Option<&'static str>,
    step: Option<Step>,
    done: Vec<&'static str>,
    failed: Vec<(&'static str, String)>,
    finished: bool,
}

struct Job {
    dir: PathBuf,
    state: Arc<Mutex<JobState>>,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct BankWindow {
    pub open: bool,
    /// The first-run offer is armed (the desktop app arms it; tests and snapshots do not).
    pub offer_armed: bool,
    /// The offer is on screen.
    pub offering: bool,
    job: Option<Job>,
    /// What the last installation could not do, per library.
    errors: Vec<(&'static str, String)>,
}

fn lock(state: &Mutex<JobState>) -> std::sync::MutexGuard<'_, JobState> {
    state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl crate::GuitarioApp {
    /// The bank's folder: the open bank's, else where a new one goes.
    pub(crate) fn bank_dir(&self) -> Option<PathBuf> {
        self.sounds.as_ref().map(|b| b.dir().to_path_buf()).or_else(SoundBank::default_dir)
    }

    /// Opens the Sound Bank window.
    pub fn open_sound_bank(&mut self) {
        self.bank_window.open = true;
    }

    /// Offers to download the libraries at start when none is installed and the offer was not
    /// declined for good.
    pub fn with_sound_offer(mut self) -> Self {
        self.bank_window.offer_armed = true;
        self
    }

    /// Whether the installer is running.
    pub fn installing_sounds(&self) -> bool {
        self.bank_window.job.is_some()
    }

    /// Downloads and unpacks `libs` on a worker thread, then points the bank at them.
    pub(crate) fn install_sounds(&mut self, libs: Vec<&'static Library>) {
        if self.bank_window.job.is_some() {
            return;
        }
        let Some(dir) = self.bank_dir() else {
            self.status = "No folder for the sound bank: set GUITARIO_SOUNDS".into();
            self.status_is_error = true;
            return;
        };
        let state = Arc::new(Mutex::new(JobState::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let (st, stop, root) = (Arc::clone(&state), Arc::clone(&cancel), dir.clone());
        let work = move || {
            if let Err(e) = std::fs::create_dir_all(&root) {
                lock(&st).failed.push(("bank", format!("{}: {e}", root.display())));
            } else {
                for lib in libs {
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    lock(&st).current = Some(lib.id);
                    let result = install(lib, &root, &mut |s| lock(&st).step = Some(s), &stop);
                    let mut s = lock(&st);
                    match result {
                        Ok(()) => s.done.push(lib.id),
                        Err(e) => s.failed.push((lib.id, e)),
                    }
                }
                if let Err(e) = write_bank(&root) {
                    lock(&st).failed.push(("bank", e));
                }
            }
            lock(&st).finished = true;
        };
        if std::thread::Builder::new().name("install-sounds".into()).spawn(work).is_err() {
            self.status = "Could not start the download".into();
            self.status_is_error = true;
            return;
        }
        self.bank_window.errors.clear();
        self.bank_window.job = Some(Job { dir, state, cancel });
    }

    /// Takes a finished installation: the bank is opened again with the new libraries.
    fn finish_install(&mut self) {
        let Some(job) = self.bank_window.job.take_if(|j| lock(&j.state).finished) else { return };
        let s = std::mem::take(&mut *lock(&job.state));
        if let Ok(bank) = SoundBank::open(&job.dir) {
            self.sounds = Some(Arc::new(bank));
            // Decode what the score needs from the new instruments.
            self.preloaded = None;
        }
        self.bank_window.errors = s.failed;
        if self.bank_window.errors.is_empty() {
            let names: Vec<&str> = s.done.iter().filter_map(|id| Library::by_id(id)).map(|l| role_label(l.role)).collect();
            self.status = if names.is_empty() { "Nothing was installed".into() } else { format!("Sampled sounds installed: {}", names.join(", ")) };
            self.status_is_error = false;
        } else {
            self.status = format!("Sound libraries: {}", self.bank_window.errors.iter().map(|(id, e)| format!("{id}: {e}")).collect::<Vec<_>>().join("; "));
            self.status_is_error = true;
        }
    }

    pub(crate) fn sound_bank_window(&mut self, ctx: &egui::Context) {
        self.finish_install();
        if self.bank_window.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        self.sound_offer(ctx);
        if !self.bank_window.open {
            return;
        }
        let dir = self.bank_dir();
        let bank = self.sounds.clone();
        let job = self.bank_window.job.as_ref().map(|j| {
            let s = lock(&j.state);
            (s.current, s.step, s.done.clone())
        });
        let mut wanted: Option<Vec<&'static Library>> = None;
        let mut cancel = false;
        let mut open = true;
        egui::Window::new("Sound Bank").open(&mut open).resizable(false).collapsible(false).default_width(560.0).default_pos([200.0, 120.0]).show(ctx, |ui| {
            ui.label(
                egui::RichText::new("Recorded guitars, bass and drums for the RSE engine. A track the bank does not cover plays the built-in synthesiser.")
                    .weak(),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Folder");
                ui.add(
                    egui::Label::new(egui::RichText::new(dir.as_ref().map_or_else(|| "none".to_string(), |d| d.display().to_string())).monospace()).truncate(),
                );
            });
            ui.add_space(6.0);
            egui::Grid::new("bank-roles").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                for role in Role::ALL {
                    ui.label(role_label(role));
                    let plays = bank.as_ref().and_then(|b| b.path_for(role)).map(|p| {
                        let lib = dir.as_ref().and_then(|d| LIBRARIES.iter().find(|l| l.role == role && p.starts_with(l.dir(d))));
                        lib.map_or_else(|| p.file_stem().unwrap_or_default().to_string_lossy().into_owned(), |l| l.title.to_string())
                    });
                    match plays {
                        Some(name) => ui.label(name),
                        None => ui.label(egui::RichText::new("Built-in synthesiser").weak()),
                    };
                    ui.end_row();
                }
            });
            ui.separator();
            ui.strong("Open libraries");
            let root = dir.clone().unwrap_or_default();
            for lib in LIBRARIES {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(400.0);
                        ui.label(egui::RichText::new(lib.title).strong());
                        ui.label(egui::RichText::new(format!("{} · {} · about {} MB", role_label(lib.role), lib.license, lib.mb)).small().weak());
                        match &job {
                            Some((Some(id), Some(Step::Downloading { done, total }), _)) if *id == lib.id => {
                                let mb = |b: u64| b as f32 / 1_048_576.0;
                                let text = total.map_or_else(
                                    || format!("Downloading: {:.0} MB", mb(*done)),
                                    |t| format!("Downloading: {:.0} of {:.0} MB", mb(*done), mb(t)),
                                );
                                let fraction = total.filter(|t| *t > 0).map_or(0.0, |t| (*done as f32 / t as f32).clamp(0.0, 1.0));
                                ui.add(egui::ProgressBar::new(fraction).text(text).desired_width(390.0));
                            }
                            Some((Some(id), Some(Step::Unpacking), _)) if *id == lib.id => {
                                ui.add(egui::ProgressBar::new(1.0).text("Unpacking…").desired_width(390.0));
                            }
                            _ => {
                                if let Some((_, e)) = self.bank_window.errors.iter().find(|(id, _)| *id == lib.id) {
                                    ui.label(egui::RichText::new(e).small().color(crate::theme::tokens().bar_error));
                                }
                            }
                        }
                    });
                    let installed = lib.installed(&root);
                    if installed {
                        ui.label(egui::RichText::new("Installed").weak());
                    } else if ui.add_enabled(job.is_none() && dir.is_some(), egui::Button::new("Install")).clicked() {
                        wanted = Some(vec![lib]);
                    }
                });
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let missing: Vec<&'static Library> = LIBRARIES.iter().filter(|l| !l.installed(&root)).collect();
                let mb: u32 = missing.iter().map(|l| l.mb).sum();
                if job.is_some() {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    ui.label(egui::RichText::new("A download that stops carries on next time.").small().weak());
                } else if !missing.is_empty() && ui.add_enabled(dir.is_some(), egui::Button::new(format!("Install All (about {mb} MB)"))).clicked() {
                    wanted = Some(missing);
                }
            });
            let credits: Vec<&Library> = LIBRARIES.iter().filter(|l| l.needs_credit() && l.installed(&root)).collect();
            if !credits.is_empty() {
                ui.separator();
                ui.strong("Credits");
                for l in credits {
                    ui.label(egui::RichText::new(format!("{}: {} ({})", l.title, l.credit, l.license)).small());
                }
            }
            ui.label(egui::RichText::new("Each library's licence and credit are kept beside its files (SOURCE.txt).").small().weak());
        });
        if !open {
            self.bank_window.open = false;
        }
        if cancel && let Some(j) = &self.bank_window.job {
            j.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(libs) = wanted {
            self.install_sounds(libs);
        }
    }

    /// At first start without a sound bank: offers to download the libraries.
    fn sound_offer(&mut self, ctx: &egui::Context) {
        if self.bank_window.offer_armed {
            self.bank_window.offer_armed = false;
            let filled = self.sounds.as_ref().is_some_and(|b| Role::ALL.iter().any(|r| b.path_for(*r).is_some()));
            self.bank_window.offering = !filled && !self.settings.get().sounds.offer_declined;
        }
        if !self.bank_window.offering {
            return;
        }
        let mb: u32 = LIBRARIES.iter().map(|l| l.mb).sum();
        let (mut download, mut later, mut never) = (false, false, false);
        egui::Modal::new(egui::Id::new("sound-offer")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.heading("Sampled sounds");
            ui.add_space(6.0);
            ui.label("Guitario plays every track with its built-in synthesiser. For recorded guitars, bass and drums, download the open sound libraries.");
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!(
                    "About {mb} MB, public domain (CC0) and CC BY, kept in your user folder. You can install them later from the Sound Bank window."
                ))
                .small()
                .weak(),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                download = ui.button("Download").clicked();
                later = ui.button("Not Now").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape));
                never = ui.button("Don't Ask Again").clicked();
            });
        });
        if download || later || never {
            self.bank_window.offering = false;
        }
        if never {
            self.settings.update(|p| p.sounds.offer_declined = true);
        }
        if download {
            self.bank_window.open = true;
            let root = self.bank_dir().unwrap_or_default();
            self.install_sounds(LIBRARIES.iter().filter(|l| !l.installed(&root)).collect());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_engine::Session;

    fn frame(app: &mut crate::GuitarioApp) {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| app.sound_bank_window(ui.ctx())).textures_delta.clear();
    }

    #[test]
    fn the_offer_shows_once_without_a_bank_and_not_after_it_is_declined() {
        let mut app = crate::GuitarioApp::new(Session::new(), None).with_sound_offer();
        frame(&mut app);
        assert!(app.bank_window.offering);
        // Declined for good: the next start does not ask.
        app.settings.update(|p| p.sounds.offer_declined = true);
        let store = std::mem::replace(&mut app.settings, crate::prefs::Store::open(craft_settings::Location::Memory).0);
        let mut again = crate::GuitarioApp::new(Session::new(), None).with_sound_offer();
        again.settings = store;
        frame(&mut again);
        assert!(!again.bank_window.offering);
        // Without the offer armed (tests, snapshots), nothing is asked.
        let mut quiet = crate::GuitarioApp::new(Session::new(), None);
        frame(&mut quiet);
        assert!(!quiet.bank_window.offering);
    }

    #[test]
    fn the_window_shows_the_bank_and_an_installed_library_needs_no_download() {
        let dir = std::env::temp_dir().join(format!("gt-bank-window-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let kit = dir.join("muldjordkit/Kit/Stereo");
        std::fs::create_dir_all(&kit).unwrap();
        std::fs::write(kit.join("kit.sfz"), "<region> sample=a.wav key=36").unwrap();
        std::fs::write(dir.join("muldjordkit/.complete"), "").unwrap();
        std::fs::write(dir.join("bank.json"), "{}").unwrap();
        let mut app = crate::GuitarioApp::new(Session::new(), None).with_sounds(SoundBank::open(&dir).ok());
        app.open_sound_bank();
        frame(&mut app);
        // Installing what is in place finishes without the network and fills the bank.
        app.install_sounds(vec![Library::by_id("muldjordkit").unwrap()]);
        assert!(app.installing_sounds());
        for _ in 0..200 {
            frame(&mut app);
            if !app.installing_sounds() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!app.installing_sounds() && app.bank_window.errors.is_empty(), "{:?}", app.bank_window.errors);
        assert!(app.sounds.as_ref().and_then(|b| b.path_for(Role::Drums)).is_some_and(|p| p.ends_with("kit.sfz")));
        assert!(app.status.contains("Drum kit"), "{}", app.status);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
