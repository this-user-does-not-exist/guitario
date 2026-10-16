//! A sound bank: which SFZ instrument plays which kind of track.
//!
//! A bank is a folder with a `bank.json` that maps roles to `.sfz` paths relative to the folder:
//!
//! ```json
//! { "drums": "virtuosity/Programs/01-basic-kit.sfz",
//!   "bass": "black-and-blue-basses/Programs/05-darkblack_pluck.sfz",
//!   "guitar_clean": "...", "guitar_distorted": "..." }
//! ```
//!
//! `cargo xtask sounds` downloads the recommended libraries and writes this file. A track whose
//! role has no entry (or whose instrument fails to load) keeps the built-in synth.

use crate::player::used_keys;
use crate::sfz::Instrument;
use crate::{Options, Player};
use guitario_score::{Score, Track};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const BANK_FILE: &str = "bank.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Drums,
    Bass,
    GuitarClean,
    GuitarDistorted,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::Drums, Role::Bass, Role::GuitarClean, Role::GuitarDistorted];

    pub fn key(self) -> &'static str {
        match self {
            Role::Drums => "drums",
            Role::Bass => "bass",
            Role::GuitarClean => "guitar_clean",
            Role::GuitarDistorted => "guitar_distorted",
        }
    }

    /// The role of a track, from its kind and General MIDI program. Programs this bank has no
    /// sampled equivalent for (strings, pads, keys) return `None`.
    pub fn of(track: &Track) -> Option<Role> {
        if track.is_drums() {
            return Some(Role::Drums);
        }
        match track.program {
            29 | 30 => Some(Role::GuitarDistorted),
            24..=28 | 31 => Some(Role::GuitarClean),
            32..=39 => Some(Role::Bass),
            _ => None,
        }
    }
}

type Loaded = (PathBuf, Vec<u8>, Arc<Instrument>);

#[derive(Default)]
pub struct SoundBank {
    dir: PathBuf,
    entries: Vec<(Role, PathBuf)>,
    /// Instruments already decoded, by file and keys, so pressing Play twice does not decode twice.
    cache: std::sync::Mutex<Vec<Loaded>>,
}

impl std::fmt::Debug for SoundBank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoundBank").field("dir", &self.dir).field("entries", &self.entries).finish()
    }
}

impl SoundBank {
    /// Reads `<dir>/bank.json`. Unknown roles and missing files are skipped (reported by [`apply`]).
    pub fn open(dir: &Path) -> Result<SoundBank, String> {
        let text = std::fs::read_to_string(dir.join(BANK_FILE)).map_err(|e| format!("{}: {e}", dir.join(BANK_FILE).display()))?;
        let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{BANK_FILE}: {e}"))?;
        let obj = json.as_object().ok_or_else(|| format!("{BANK_FILE}: expected an object"))?;
        let entries = Role::ALL.iter().filter_map(|r| obj.get(r.key()).and_then(|v| v.as_str()).map(|p| (*r, dir.join(p)))).collect();
        Ok(SoundBank { dir: dir.to_path_buf(), entries, cache: std::sync::Mutex::default() })
    }

    /// Where banks live by default: `$GUITARIO_SOUNDS`, else the per-user data folder.
    pub fn default_dir() -> Option<PathBuf> {
        if let Some(d) = std::env::var_os("GUITARIO_SOUNDS").filter(|d| !d.is_empty()) {
            return Some(PathBuf::from(d));
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if cfg!(target_os = "macos") {
            home.map(|h| h.join("Library/Application Support/Guitario/sounds"))
        } else if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Guitario").join("sounds"))
        } else {
            std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| home.map(|h| h.join(".local/share"))).map(|d| d.join("guitario/sounds"))
        }
    }

    /// The bank at [`default_dir`](Self::default_dir), if one is installed.
    pub fn open_default() -> Option<SoundBank> {
        SoundBank::open(&Self::default_dir()?).ok()
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path_for(&self, role: Role) -> Option<&Path> {
        self.entries.iter().find(|(r, _)| *r == role).map(|(_, p)| p.as_path())
    }

    /// Forgets decoded instruments (after the library files change).
    pub fn clear_cache(&self) {
        self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
    }

    /// Gives each track with a mapped role its sampled instrument, loading only the samples the
    /// score plays. Returns one message per problem; those tracks keep the built-in synth.
    pub fn apply(&self, player: &mut Player, score: &Score) -> Vec<String> {
        let mut notes = Vec::new();
        let mut cache = self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        for (ti, track) in score.tracks.iter().enumerate() {
            let Some(path) = Role::of(track).and_then(|r| self.path_for(r)) else { continue };
            let keys = used_keys(score, ti);
            if keys.is_empty() {
                continue;
            }
            if let Some((_, _, inst)) = cache.iter().find(|(p, k, _)| p == path && *k == keys) {
                player.set_instrument(ti, Some(Arc::clone(inst)));
                continue;
            }
            match Instrument::load(path, Some(&keys)) {
                Ok(inst) => {
                    let inst = Arc::new(inst);
                    notes.extend(inst.warnings.iter().take(3).map(|w| format!("{}: {w}", inst.name)));
                    player.set_instrument(ti, Some(Arc::clone(&inst)));
                    cache.push((path.to_path_buf(), keys, inst));
                }
                Err(e) => notes.push(format!("{}: {e}", track.name)),
            }
        }
        notes
    }

    /// Renders the whole score offline with this bank (see [`crate::render_wav`]).
    pub fn render(&self, score: &Score, sample_rate: u32) -> (Vec<f32>, Vec<String>) {
        let mut p = Player::new(score, sample_rate, Options::default());
        let notes = self.apply(&mut p, score);
        (crate::wav::render_player(&mut p), notes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::{Beat, Note};
    use guitario_theory::Duration;

    fn bank_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gc-bank-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("g")).unwrap();
        let spec = hound::WavSpec { channels: 1, sample_rate: 8000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(dir.join("g/a.wav"), spec).unwrap();
        for n in 0..16_000 {
            w.write_sample(((n as f32 * 0.1).sin() * 20_000.0) as i16).unwrap();
        }
        w.finalize().unwrap();
        std::fs::write(dir.join("g/k.sfz"), "<region> sample=a.wav lokey=0 hikey=127 pitch_keycenter=64").unwrap();
        dir
    }

    #[test]
    fn roles_follow_the_program() {
        let s = Score::new();
        let mut t = s.tracks[0].clone();
        t.program = 30;
        assert_eq!(Role::of(&t), Some(Role::GuitarDistorted));
        t.program = 33;
        assert_eq!(Role::of(&t), Some(Role::Bass));
        t.program = 25;
        assert_eq!(Role::of(&t), Some(Role::GuitarClean));
        t.program = 90;
        assert_eq!(Role::of(&t), None);
        assert_eq!(Role::of(&Track::drums("d", &s.headers)), Some(Role::Drums));
    }

    #[test]
    fn bank_renders_sampled_audio_and_bad_banks_fall_back() {
        let dir = bank_dir("ok");
        std::fs::write(dir.join(BANK_FILE), r#"{"guitar_clean":"g/k.sfz","bass":"g/missing.sfz","bogus":1}"#).unwrap();
        let bank = SoundBank::open(&dir).unwrap();
        assert!(bank.path_for(Role::GuitarClean).is_some() && bank.path_for(Role::Drums).is_none());
        let mut s = Score::new();
        s.tracks[0].program = 25;
        s.tracks[0].measures[0].voices[0].beats = vec![Beat { rest: false, notes: vec![Note::new(0, 0)], ..Beat::rest(Duration::WHOLE) }];
        let (pcm, notes) = bank.render(&s, 8000);
        assert!(notes.is_empty(), "{notes:?}");
        let rms = (pcm.iter().map(|x| x * x).sum::<f32>() / pcm.len() as f32).sqrt();
        assert!(rms > 0.05, "rms {rms}");
        // A bass track pointing at a missing file reports it and still plays (built-in synth).
        s.tracks[0].program = 33;
        let (pcm, notes) = bank.render(&s, 8000);
        assert_eq!(notes.len(), 1);
        assert!(pcm.iter().any(|x| x.abs() > 0.01));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_bank_files_are_errors_not_panics() {
        let dir = bank_dir("bad");
        for text in ["", "[]", "{", "null", r#"{"drums": 5}"#] {
            std::fs::write(dir.join(BANK_FILE), text).unwrap();
            let _ = SoundBank::open(&dir);
        }
        assert!(SoundBank::open(&dir.join("nowhere")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
