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

use crate::player::used_keys_by_track;
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

/// An instrument file decoded for these keys (sorted), or why it could not be.
type Loaded = (PathBuf, Vec<u8>, Result<Arc<Instrument>, String>);

#[derive(Default)]
pub struct SoundBank {
    dir: PathBuf,
    entries: Vec<(Role, PathBuf)>,
    /// Instruments already decoded, one per file with every key loaded so far, so pressing Play
    /// again (or adding a note) does not decode again. Held only to look up and insert, never
    /// while decoding.
    cache: std::sync::Mutex<Vec<Loaded>>,
}

/// Whether `have` (sorted) holds every key of `want` (sorted).
fn covers(have: &[u8], want: &[u8]) -> bool {
    want.iter().all(|k| have.binary_search(k).is_ok())
}

impl std::fmt::Debug for SoundBank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoundBank").field("dir", &self.dir).field("entries", &self.entries).finish()
    }
}

/// The per-user `sounds` folder of an app called `name` (`folder` on Linux).
fn data_dir(name: &str, folder: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Application Support").join(name).join("sounds"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(name).join("sounds"))
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| home.map(|h| h.join(".local/share"))).map(|d| d.join(folder).join("sounds"))
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

    /// Where banks live by default: `$GUITARIO_SOUNDS`, else the per-user data folder. A bank
    /// installed before the app was renamed from Guitario is used where it is until Guitario has
    /// its own.
    pub fn default_dir() -> Option<PathBuf> {
        if let Some(d) = std::env::var_os("GUITARIO_SOUNDS").filter(|d| !d.is_empty()) {
            return Some(PathBuf::from(d));
        }
        let dir = data_dir("Guitario", "guitario")?;
        if !dir.exists()
            && let Some(old) = data_dir("Guitario", "guitario").filter(|d| d.exists())
        {
            return Some(old);
        }
        Some(dir)
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

    /// What `score` needs from the bank: each track with a mapped role, its instrument file and
    /// the keys it plays.
    fn needs(&self, score: &Score) -> Vec<(usize, PathBuf, Vec<u8>)> {
        let keys = used_keys_by_track(score);
        score
            .tracks
            .iter()
            .zip(keys)
            .enumerate()
            .filter(|(_, (_, k))| !k.is_empty())
            .filter_map(|(ti, (track, k))| Some((ti, Role::of(track).and_then(|r| self.path_for(r))?.to_path_buf(), k)))
            .collect()
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, Vec<Loaded>> {
        self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether every instrument `score` needs is decoded (or known not to load), so [`apply`]
    /// returns at once. Cheap enough to ask after each edit.
    pub fn ready(&self, score: &Score) -> bool {
        let needs = self.needs(score);
        let cache = self.cache();
        needs.iter().all(|(_, path, keys)| cache.iter().any(|(p, k, inst)| p == path && (inst.is_err() || covers(k, keys))))
    }

    /// Decodes what `score` needs and is not decoded yet: each file once, for every key any
    /// track asks of it plus those it had. Safe to run on a worker thread; returns one message per
    /// problem.
    pub fn preload(&self, score: &Score) -> Vec<String> {
        // Keys wanted per file, across the tracks that share it.
        let mut wanted: Vec<(PathBuf, Vec<u8>)> = Vec::new();
        for (_, path, keys) in self.needs(score) {
            match wanted.iter_mut().find(|(p, _)| *p == path) {
                Some((_, k)) => k.extend(keys),
                None => wanted.push((path, keys)),
            }
        }
        let mut notes = Vec::new();
        for (path, mut keys) in wanted {
            keys.sort_unstable();
            keys.dedup();
            let had = {
                let cache = self.cache();
                match cache.iter().find(|(p, _, _)| *p == path) {
                    Some((_, k, inst)) if inst.is_err() || covers(k, &keys) => continue,
                    Some((_, k, _)) => k.clone(),
                    None => Vec::new(),
                }
            };
            keys.extend(had);
            keys.sort_unstable();
            keys.dedup();
            let loaded =
                Instrument::load(&path, Some(&keys)).map(Arc::new).map_err(|e| format!("{}: {e}", path.file_stem().unwrap_or_default().to_string_lossy()));
            match &loaded {
                Ok(inst) => notes.extend(inst.warnings.iter().take(3).map(|w| format!("{}: {w}", inst.name))),
                Err(e) => notes.push(e.clone()),
            }
            let mut cache = self.cache();
            cache.retain(|(p, _, _)| *p != path);
            cache.push((path, keys, loaded));
        }
        notes
    }

    /// Gives each track with a mapped role its sampled instrument, loading only the samples the
    /// score plays (nothing when [`preload`] already did). Returns one message per problem; those
    /// tracks keep the built-in synth.
    pub fn apply(&self, player: &mut Player, score: &Score) -> Vec<String> {
        let mut notes = self.preload(score);
        let cache = self.cache();
        for (ti, path, _) in self.needs(score) {
            match cache.iter().find(|(p, _, _)| *p == path).map(|(_, _, i)| i) {
                Some(Ok(inst)) => player.set_instrument(ti, Some(Arc::clone(inst))),
                Some(Err(e)) if !notes.contains(e) => notes.push(e.clone()),
                _ => {}
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
    fn preloading_decodes_once_and_grows_with_new_notes() {
        let dir = bank_dir("pre");
        std::fs::write(dir.join(BANK_FILE), r#"{"guitar_clean":"g/k.sfz","bass":"g/missing.sfz"}"#).unwrap();
        let bank = SoundBank::open(&dir).unwrap();
        let mut s = Score::new();
        s.tracks[0].program = 25;
        let notes = |frets: &[u8]| frets.iter().map(|f| Beat { rest: false, notes: vec![Note::new(0, *f)], ..Beat::rest(Duration::QUARTER) }).collect();
        s.tracks[0].measures[0].voices[0].beats = notes(&[0, 2, 4, 5]);
        assert!(!bank.ready(&s));
        assert!(bank.preload(&s).is_empty());
        assert!(bank.ready(&s));
        let first = |b: &SoundBank| match &b.cache()[0] {
            (_, keys, Ok(inst)) => (keys.clone(), Arc::as_ptr(inst)),
            _ => panic!("not loaded"),
        };
        let (keys, inst) = first(&bank);
        assert_eq!(keys, vec![64, 66, 68, 69]);
        // Fewer notes: the same instrument, nothing decoded.
        s.tracks[0].measures[0].voices[0].beats = notes(&[2, 4]);
        assert!(bank.ready(&s));
        bank.preload(&s);
        assert_eq!(first(&bank).1, inst);
        // A new note: decoded again with the old keys and the new one, once.
        s.tracks[0].measures[0].voices[0].beats = notes(&[7]);
        assert!(!bank.ready(&s));
        let mut p = Player::new(&s, 8000, Options::default());
        assert!(bank.apply(&mut p, &s).is_empty());
        assert_eq!(first(&bank).0, vec![64, 66, 68, 69, 71]);
        assert_eq!(bank.cache().len(), 1);
        // A file that cannot load is reported once per Play, and not decoded again.
        let mut bass = s.tracks[0].clone();
        bass.program = 33;
        s.tracks.push(bass);
        assert!(!bank.ready(&s));
        assert_eq!(bank.preload(&s).len(), 1);
        assert!(bank.ready(&s) && bank.preload(&s).is_empty());
        assert_eq!(bank.apply(&mut Player::new(&s, 8000, Options::default()), &s).len(), 1);
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
