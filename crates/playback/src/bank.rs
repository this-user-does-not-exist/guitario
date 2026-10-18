//! A sound bank: which SFZ instrument plays which kind of track, and which program plays each
//! articulation.
//!
//! A bank is a folder with a `bank.json` that maps roles to `.sfz` paths relative to the folder:
//!
//! ```json
//! { "drums": "virtuosity/Programs/01-basic-kit.sfz",
//!   "bass": "black-and-blue-basses/Programs/05-darkblack_pluck.sfz",
//!   "guitar_clean": "...", "guitar_distorted": "..." }
//! ```
//!
//! A role can instead name a program per articulation (`sustain` plays every note without one of
//! its own), each a path or, for a library that switches articulations by key, a path and the
//! keyswitch:
//!
//! ```json
//! { "guitar_distorted": { "sustain": "dist/sustain.sfz", "palm_mute": "dist/palm.sfz",
//!                         "harmonic": { "program": "dist/all.sfz", "keyswitch": "c1" } } }
//! ```
//!
//! `cargo xtask sounds` downloads the recommended libraries and writes this file. A track whose
//! role has no entry (or whose instrument fails to load) keeps the built-in synth.

use crate::articulation::{Articulation, used_keys_by_articulation};
pub use crate::role::Role;
use crate::sfz::{Instrument, parse_key};
use crate::{Options, Player};
use guitario_score::Score;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const BANK_FILE: &str = "bank.json";

/// One program of a bank: an `.sfz` file and, in a library that switches articulations by key,
/// the keyswitch to hold (none: the file's own default).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub path: PathBuf,
    pub keyswitch: Option<u8>,
}

impl Program {
    /// A program from `bank.json`: a path, or `{"program": path, "keyswitch": "c1" or 24}`.
    fn read(dir: &Path, v: &serde_json::Value) -> Option<Program> {
        if let Some(p) = v.as_str() {
            return Some(Program { path: dir.join(p), keyswitch: None });
        }
        let path = dir.join(v.get("program")?.as_str()?);
        let keyswitch = match v.get("keyswitch") {
            Some(serde_json::Value::String(k)) => Some(parse_key(k)?),
            Some(serde_json::Value::Number(n)) => Some(u8::try_from(n.as_u64()?).ok().filter(|k| *k <= 127)?),
            _ => None,
        };
        Some(Program { path, keyswitch })
    }

    fn name(&self) -> String {
        self.path.file_stem().unwrap_or_default().to_string_lossy().into_owned()
    }
}

/// A program decoded for these keys (sorted), or why it could not be.
type Loaded = (Program, Vec<u8>, Result<Arc<Instrument>, String>);

/// What a track needs: an articulation, the program that plays it and the keys it plays there.
type Need = (usize, Articulation, Program, Vec<u8>);

#[derive(Default)]
pub struct SoundBank {
    dir: PathBuf,
    entries: Vec<(Role, Articulation, Program)>,
    /// Programs already decoded, one per program with every key loaded so far, so pressing Play
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
    /// Reads `<dir>/bank.json`. Unknown roles and articulations, and malformed entries, are
    /// skipped; missing files are reported by [`apply`](Self::apply).
    pub fn open(dir: &Path) -> Result<SoundBank, String> {
        let text = std::fs::read_to_string(dir.join(BANK_FILE)).map_err(|e| format!("{}: {e}", dir.join(BANK_FILE).display()))?;
        let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{BANK_FILE}: {e}"))?;
        let obj = json.as_object().ok_or_else(|| format!("{BANK_FILE}: expected an object"))?;
        let mut entries = Vec::new();
        for role in Role::ALL {
            match obj.get(role.key()) {
                Some(serde_json::Value::Object(arts)) if !arts.contains_key("program") => {
                    for art in Articulation::ALL {
                        if let Some(p) = arts.get(art.key()).and_then(|v| Program::read(dir, v)) {
                            entries.push((role, art, p));
                        }
                    }
                }
                Some(v) => entries.extend(Program::read(dir, v).map(|p| (role, Articulation::Sustain, p))),
                None => {}
            }
        }
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

    /// The `.sfz` file of a role's sustain program.
    pub fn path_for(&self, role: Role) -> Option<&Path> {
        self.program(role, Articulation::Sustain).map(|p| p.path.as_path())
    }

    /// The program a role names for `articulation`, if it names one.
    pub fn program(&self, role: Role, articulation: Articulation) -> Option<&Program> {
        self.entries.iter().find(|(r, a, _)| *r == role && *a == articulation).map(|(_, _, p)| p)
    }

    /// The memory the decoded instruments take, in bytes.
    pub fn memory(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        self.cache().iter().filter_map(|(_, _, inst)| inst.as_ref().ok()).map(|i| i.memory(&mut seen)).sum()
    }

    /// Forgets decoded instruments (after the library files change).
    pub fn clear_cache(&self) {
        self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clear();
    }

    /// A track's sustain program and the role whose articulation programs go with it: the
    /// track's own choice (a role, or an `.sfz` file on its own), else the role its instrument
    /// picks. None when the bank has nothing for it.
    fn sustain_for(&self, track: &guitario_score::Track) -> Option<(Option<Role>, Program)> {
        let role = match track.bank.as_deref() {
            Some(name) => match Role::from_key(name) {
                Some(role) => role,
                None => return Some((None, Program { path: self.resolve(name), keyswitch: None })),
            },
            None => Role::of(track)?,
        };
        Some((Some(role), self.program(role, Articulation::Sustain)?.clone()))
    }

    /// An `.sfz` file a track names: absolute, or relative to the bank's folder.
    fn resolve(&self, file: &str) -> PathBuf {
        let p = Path::new(file);
        if p.is_absolute() { p.to_path_buf() } else { self.dir.join(p) }
    }

    /// The roles tracks of `score` ask for by name that this bank does not fill, as messages.
    pub fn missing(&self, score: &Score) -> Vec<String> {
        score
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.engine == guitario_score::SoundEngine::Rse)
            .filter_map(|(ti, t)| {
                let role = Role::from_key(t.bank.as_deref()?)?;
                self.program(role, Articulation::Sustain).is_none().then(|| format!("track {} asks for the bank's {}, which has none", ti + 1, role.key()))
            })
            .collect()
    }

    /// What `score` needs from the bank: for each track it has an instrument for, each
    /// articulation it plays with its program and keys. An articulation without a program of its
    /// own is played by the sustain program, which a track needs to be sampled at all.
    fn needs(&self, score: &Score) -> Vec<Need> {
        let mut out = Vec::new();
        for (ti, (track, by_art)) in score.tracks.iter().zip(used_keys_by_articulation(score)).enumerate() {
            if track.engine != guitario_score::SoundEngine::Rse {
                continue;
            }
            let Some((role, sustain)) = self.sustain_for(track) else { continue };
            let mut plain: Vec<u8> = Vec::new();
            for (art, keys) in Articulation::ALL.into_iter().zip(by_art) {
                match role.and_then(|r| self.program(r, art)).filter(|_| art != Articulation::Sustain && !keys.is_empty()) {
                    Some(p) => out.push((ti, art, p.clone(), keys)),
                    None => plain.extend(keys),
                }
            }
            if !plain.is_empty() {
                plain.sort_unstable();
                plain.dedup();
                out.push((ti, Articulation::Sustain, sustain.clone(), plain));
            }
        }
        out
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, Vec<Loaded>> {
        self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether every instrument `score` needs is decoded (or known not to load), so [`apply`]
    /// returns at once. Cheap enough to ask after each edit.
    pub fn ready(&self, score: &Score) -> bool {
        let needs = self.needs(score);
        let cache = self.cache();
        needs.iter().all(|(_, _, prog, keys)| cache.iter().any(|(p, k, inst)| p == prog && (inst.is_err() || covers(k, keys))))
    }

    /// Decodes what `score` needs and is not decoded yet: each program once, for every key any
    /// track asks of it plus those it had. Safe to run on a worker thread; returns one message per
    /// problem.
    pub fn preload(&self, score: &Score) -> Vec<String> {
        self.preload_needs(&self.needs(score))
    }

    fn preload_needs(&self, needs: &[Need]) -> Vec<String> {
        // Keys wanted per program, across the tracks and articulations that share it.
        let mut wanted: Vec<(Program, Vec<u8>)> = Vec::new();
        for (_, _, prog, keys) in needs {
            match wanted.iter_mut().find(|(p, _)| p == prog) {
                Some((_, k)) => k.extend(keys),
                None => wanted.push((prog.clone(), keys.clone())),
            }
        }
        let mut notes = Vec::new();
        for (prog, mut keys) in wanted {
            keys.sort_unstable();
            keys.dedup();
            let had = {
                let cache = self.cache();
                match cache.iter().find(|(p, _, _)| *p == prog) {
                    Some((_, k, inst)) if inst.is_err() || covers(k, &keys) => continue,
                    Some((_, k, _)) => k.clone(),
                    None => Vec::new(),
                }
            };
            keys.extend(had);
            keys.sort_unstable();
            keys.dedup();
            let loaded = Instrument::load_switched(&prog.path, Some(&keys), prog.keyswitch).map(Arc::new).map_err(|e| format!("{}: {e}", prog.name()));
            match &loaded {
                Ok(inst) => notes.extend(inst.warnings.iter().take(3).map(|w| format!("{}: {w}", inst.name))),
                Err(e) => notes.push(e.clone()),
            }
            let mut cache = self.cache();
            cache.retain(|(p, _, _)| *p != prog);
            cache.push((prog, keys, loaded));
        }
        notes
    }

    /// Gives each track with a mapped role its sampled programs, loading only the samples the
    /// score plays (nothing when [`preload`] already did). Returns one message per problem; a
    /// track whose sustain program fails keeps the built-in synth.
    pub fn apply(&self, player: &mut Player, score: &Score) -> Vec<String> {
        let needs = self.needs(score);
        let mut notes = self.preload_needs(&needs);
        notes.extend(self.missing(score));
        let cache = self.cache();
        for (ti, art, prog, _) in needs {
            match cache.iter().find(|(p, _, _)| *p == prog).map(|(_, _, i)| i) {
                Some(Ok(inst)) => player.set_articulation(ti, art, Some(Arc::clone(inst))),
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
        assert_eq!(Role::from_key("guitar_clean"), Some(Role::GuitarClean));
        assert_eq!(Role::from_key("piano"), None);
        t.program = 30;
        assert_eq!(Role::of(&t), Some(Role::GuitarDistorted));
        t.program = 33;
        assert_eq!(Role::of(&t), Some(Role::Bass));
        t.program = 25;
        assert_eq!(Role::of(&t), Some(Role::GuitarClean));
        t.program = 90;
        assert_eq!(Role::of(&t), None);
        assert_eq!(Role::of(&guitario_score::Track::drums("d", &s.headers)), Some(Role::Drums));
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
        // On the MIDI engine the track takes the synth, not the samples.
        s.tracks[0].engine = guitario_score::SoundEngine::Midi;
        assert!(bank.needs(&s).is_empty());
        s.tracks[0].engine = guitario_score::SoundEngine::Rse;
        assert_eq!(bank.needs(&s).len(), 1);
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
    fn articulations_have_programs_of_their_own_or_borrow_the_sustain_one() {
        use guitario_score::Harmonic;
        let dir = bank_dir("art");
        // One program with two keyswitched articulations: c1 (its default) and d1.
        std::fs::write(
            dir.join("g/ks.sfz"),
            "<global> sw_default=c1\n<region> sample=a.wav lokey=0 hikey=127 sw_last=c1\n<region> sample=a.wav lokey=0 hikey=127 pitch_keycenter=40 sw_last=d1",
        )
        .unwrap();
        std::fs::write(
            dir.join(BANK_FILE),
            r#"{"guitar_clean": {"sustain": "g/k.sfz", "palm_mute": {"program": "g/ks.sfz", "keyswitch": "d1"},
                                 "harmonic": {"program": "g/ks.sfz", "keyswitch": 9999}, "bogus": "x"},
                "bass": {"program": "g/ks.sfz", "keyswitch": 26}}"#,
        )
        .unwrap();
        let bank = SoundBank::open(&dir).unwrap();
        assert_eq!(bank.program(Role::GuitarClean, Articulation::PalmMute).map(|p| p.keyswitch), Some(parse_key("d1")));
        assert!(bank.program(Role::GuitarClean, Articulation::Harmonic).is_none(), "a keyswitch out of range is skipped");
        assert_eq!(bank.program(Role::Bass, Articulation::Sustain).and_then(|p| p.keyswitch), Some(26));
        // A palm mute, a plain note and a harmonic: the palm mute has a program, the harmonic
        // borrows the sustain one at the pitch it sounds.
        let mut s = Score::new();
        s.tracks[0].program = 25;
        let mut pm = Note::new(5, 3);
        pm.effects.palm_mute = true;
        let mut h = Note::new(0, 12);
        h.effects.harmonic = Some(Harmonic::Natural);
        s.tracks[0].measures[0].voices[0].beats =
            [pm, Note::new(5, 5), h].into_iter().map(|n| Beat { rest: false, notes: vec![n], ..Beat::rest(Duration::QUARTER) }).collect();
        let summary: Vec<(Articulation, Option<u8>, Vec<u8>)> = bank.needs(&s).into_iter().map(|(_, a, p, k)| (a, p.keyswitch, k)).collect();
        assert_eq!(summary, vec![(Articulation::PalmMute, parse_key("d1"), vec![43]), (Articulation::Sustain, None, vec![45, 76])]);
        let (pcm, notes) = bank.render(&s, 8000);
        assert!(notes.is_empty(), "{notes:?}");
        assert!(pcm.iter().any(|x| x.abs() > 0.01));
        assert!(bank.ready(&s));
        // The keyswitched program is loaded with its switch held: the d1 region alone.
        match bank.cache().iter().find(|(p, _, _)| p.keyswitch.is_some()) {
            Some((_, _, Ok(inst))) => assert_eq!(inst.region_count(), 1),
            other => panic!("not loaded: {:?}", other.map(|o| &o.0)),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_track_can_choose_its_instrument() {
        let dir = bank_dir("choice");
        std::fs::write(dir.join(BANK_FILE), r#"{"guitar_clean":"g/k.sfz"}"#).unwrap();
        let bank = SoundBank::open(&dir).unwrap();
        let mut s = Score::new();
        s.tracks[0].program = 33;
        s.tracks[0].measures[0].voices[0].beats = vec![Beat { rest: false, notes: vec![Note::new(0, 0)], ..Beat::rest(Duration::WHOLE) }];
        assert!(bank.needs(&s).is_empty(), "a bass with no bass in the bank plays the synth");
        // The track asks for the clean guitar by its role, then for the file by its path.
        for choice in ["guitar_clean", "g/k.sfz", &dir.join("g/k.sfz").display().to_string()] {
            s.tracks[0].bank = Some(choice.to_string());
            let needs = bank.needs(&s);
            assert_eq!(needs.len(), 1, "{choice}");
            assert_eq!(needs[0].2.path, dir.join("g/k.sfz"), "{choice}");
            let (pcm, notes) = bank.render(&s, 8000);
            assert!(notes.is_empty() && pcm.iter().any(|x| x.abs() > 0.05), "{choice}: {notes:?}");
        }
        // A role the bank lacks, or a file that is not there: the synth, and a message.
        s.tracks[0].bank = Some("drums".into());
        let (pcm, notes) = bank.render(&s, 8000);
        assert!(notes.iter().any(|n| n.contains("drums")) && pcm.iter().any(|x| x.abs() > 0.01), "{notes:?}");
        s.tracks[0].bank = Some("g/nowhere.sfz".into());
        let (pcm, notes) = bank.render(&s, 8000);
        assert!(notes.len() == 1 && pcm.iter().any(|x| x.abs() > 0.01), "{notes:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_bank_files_are_errors_not_panics() {
        let dir = bank_dir("bad");
        for text in [
            "",
            "[]",
            "{",
            "null",
            r#"{"drums": 5}"#,
            r#"{"bass": {"sustain": 5, "dead": {"program": 3}}}"#,
            r#"{"bass": {"program": "x", "keyswitch": "zz"}}"#,
        ] {
            std::fs::write(dir.join(BANK_FILE), text).unwrap();
            let _ = SoundBank::open(&dir);
        }
        assert!(SoundBank::open(&dir.join("nowhere")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
