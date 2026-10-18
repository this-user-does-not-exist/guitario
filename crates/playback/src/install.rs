//! Installs the recommended open SFZ libraries into a sound bank folder and writes the
//! `bank.json` that [`SoundBank`](crate::SoundBank) reads. The app's Sound Bank window, the
//! engine's `sound.installLibraries` command and `cargo xtask sounds` all run this.
//!
//! Downloads go through `curl`, which ships with macOS, Windows 10 and later and Linux desktops,
//! and resumes a download that broke off; archives are unpacked here (zip and 7z), so no other tool
//! is needed. A library that is a git repository is cloned when `git` is installed and downloaded
//! as GitHub's zip of it otherwise. Every library is CC0 or CC BY: its title, licence and credit are
//! written beside its files (`SOURCE.txt`), and the libraries are never part of Guitario itself.

use crate::bank::{BANK_FILE, Role};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

/// Where a library comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Zip(&'static str),
    SevenZip(&'static str),
    Git(&'static str),
}

/// One recommended library and the role it plays.
#[derive(Debug)]
pub struct Library {
    pub id: &'static str,
    pub role: Role,
    pub title: &'static str,
    pub license: &'static str,
    pub credit: &'static str,
    pub source: Source,
    /// The program to load: the shortest `.sfz` path (inside the library) containing this text.
    pub sfz_hint: &'static str,
    /// Download size, roughly.
    pub mb: u32,
}

pub const LIBRARIES: &[Library] = &[
    Library {
        id: "freepats-guitar-distorted",
        role: Role::GuitarDistorted,
        title: "FreePats FSBS Electric Guitar Distorted #1 (Fender, amp and effects rack)",
        license: "CC0 1.0",
        credit: "FreePats project, https://freepats.zenvoid.org/ElectricGuitar/distorted-electric-guitar.html",
        source: Source::SevenZip("https://github.com/freepats/electric-guitar-FSBS-dist1/releases/download/2022-09-11/EGuitarFSBS-dist1-SFZ+FLAC-20220911.7z"),
        sfz_hint: ".sfz",
        mb: 300,
    },
    Library {
        id: "freepats-guitar-clean",
        role: Role::GuitarClean,
        title: "FreePats FSBS Electric Guitar Clean #1 (Fender bridge pickup)",
        license: "CC0 1.0",
        credit: "FreePats project, https://freepats.zenvoid.org/ElectricGuitar/clean-electric-guitar.html",
        source: Source::SevenZip("https://github.com/freepats/electric-guitar-FSBS-clean/releases/download/2026-08-07/EGuitarFSBS-clean-SFZ+FLAC-20260807.7z"),
        sfz_hint: ".sfz",
        mb: 120,
    },
    Library {
        id: "karoryfer-growlybass",
        role: Role::Bass,
        title: "Karoryfer Growlybass (Squier Jazz Bass, direct, roundwound)",
        license: "CC0 1.0",
        credit: "Karoryfer Samples, https://github.com/sfzinstruments/karoryfer.growlybass",
        source: Source::Zip("https://github.com/sfzinstruments/karoryfer.growlybass/releases/download/v1.002/Karoryfer.Growlybass.v1.002.zip"),
        sfz_hint: "growlybass_dirty.sfz",
        mb: 160,
    },
    Library {
        id: "muldjordkit",
        role: Role::Drums,
        title: "MuldjordKit (Tama Superstar, sampled by Lars Muldjord; SFZ mapping by kinwie)",
        license: "CC BY 4.0",
        credit: "Lars Muldjord / DrumGizmo team; SFZ port by kinwie, https://github.com/sfzinstruments/DrumGizmo.MuldjordKit",
        source: Source::Git("https://github.com/sfzinstruments/DrumGizmo.MuldjordKit.git"),
        sfz_hint: "Stereo",
        mb: 340,
    },
];

/// Marks a library whose files are all in place.
const COMPLETE: &str = ".complete";
/// The licence and credit written beside a library's files.
pub const SOURCE_FILE: &str = "SOURCE.txt";

/// How an installation is going.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Bytes downloaded so far, of `total` when the server says.
    Downloading {
        done: u64,
        total: Option<u64>,
    },
    Unpacking,
    Installed,
}

impl Library {
    pub fn by_id(id: &str) -> Option<&'static Library> {
        LIBRARIES.iter().find(|l| l.id == id)
    }

    /// Where the library goes under the bank folder `root`.
    pub fn dir(&self, root: &Path) -> PathBuf {
        root.join(self.id)
    }

    pub fn installed(&self, root: &Path) -> bool {
        self.dir(root).join(COMPLETE).exists()
    }

    /// Whether its licence asks to credit its authors (CC BY).
    pub fn needs_credit(&self) -> bool {
        self.license.starts_with("CC BY")
    }

    /// The program it plays, relative to `root`, once installed.
    pub fn program(&self, root: &Path) -> Option<String> {
        if !self.installed(root) {
            return None;
        }
        let mut found = Vec::new();
        find_sfz(&self.dir(root), self.sfz_hint, 0, &mut found);
        found.sort_by_key(|p| p.as_os_str().len());
        let first = found.first()?;
        first.strip_prefix(root).ok().map(|p| p.to_string_lossy().replace('\\', "/"))
    }
}

fn find_sfz(dir: &Path, hint: &str, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() && depth < 6 {
            find_sfz(&p, hint, depth + 1, out);
        } else if p.extension().is_some_and(|x| x == "sfz") && p.to_string_lossy().to_lowercase().contains(&hint.to_lowercase()) {
            out.push(p);
        }
    }
}

/// Whether `tool` runs here.
fn have(tool: &str) -> bool {
    Command::new(tool).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// Waits for `child`, calling `tick` every fifth of a second and killing it when `cancel` is set.
fn wait(mut child: std::process::Child, what: &str, cancel: &AtomicBool, tick: &mut dyn FnMut()) -> Result<(), String> {
    loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cancelled".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                tick();
                if status.success() {
                    return Ok(());
                }
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut e, &mut err);
                }
                let err = err.trim();
                return Err(if err.is_empty() { format!("{what} failed ({status})") } else { format!("{what}: {err}") });
            }
            Ok(None) => tick(),
            Err(e) => return Err(format!("{what}: {e}")),
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// The size `url` reports, following redirects (the last `Content-Length`).
fn content_length(url: &str) -> Option<u64> {
    let out = Command::new("curl").args(["-sIL", url]).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    // The last one: the size after redirects.
    text.lines().rev().find_map(|l| {
        let (name, value) = l.split_once(':')?;
        name.trim().eq_ignore_ascii_case("content-length").then(|| value.trim().parse::<u64>().ok()).flatten()
    })
}

/// Downloads `url` to `file`, carrying on from what an earlier attempt at the same address left
/// there (`<file>.url` says which).
fn download(url: &str, file: &Path, on: &mut dyn FnMut(Step), cancel: &AtomicBool) -> Result<(), String> {
    let from = file.with_extension("url");
    if std::fs::read_to_string(&from).ok().as_deref() != Some(url) {
        let _ = std::fs::remove_file(file);
        std::fs::write(&from, url).map_err(|e| format!("{}: {e}", from.display()))?;
    }
    let total = content_length(url);
    let size = || std::fs::metadata(file).map_or(0, |m| m.len());
    if total.is_some_and(|t| t > 0 && size() == t) {
        on(Step::Downloading { done: size(), total });
        return Ok(());
    }
    let child = Command::new("curl")
        .args(["-fsSL", "--retry", "3", "-C", "-", "-o"])
        .arg(file)
        .arg(url)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {e} (Guitario downloads the sound libraries with curl)"))?;
    wait(child, &format!("downloading {url}"), cancel, &mut || on(Step::Downloading { done: size(), total }))
}

fn unzip(archive: &Path, dest: &Path) -> Result<(), String> {
    let f = std::fs::File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(f).map_err(|e| format!("{}: {e}", archive.display()))?;
    zip.extract(dest).map_err(|e| format!("{}: {e}", archive.display()))
}

fn un7z(archive: &Path, dest: &Path) -> Result<(), String> {
    sevenz_rust2::decompress_file(archive, dest).map_err(|e| format!("{}: {e}", archive.display()))
}

/// GitHub's zip of a repository's default branch.
fn archive_of(repo: &str) -> String {
    format!("{}/archive/HEAD.zip", repo.trim_end_matches(".git"))
}

/// Installs `lib` into `root` unless it is there already: downloads it (resuming), unpacks it,
/// writes its licence beside it and marks it complete. `on` hears how it goes; setting `cancel`
/// stops it, keeping the partial download for next time.
pub fn install(lib: &Library, root: &Path, on: &mut dyn FnMut(Step), cancel: &AtomicBool) -> Result<(), String> {
    install_from(lib, lib.source, root, on, cancel)
}

/// [`install`] from another source (tests point it at local files).
pub fn install_from(lib: &Library, source: Source, root: &Path, on: &mut dyn FnMut(Step), cancel: &AtomicBool) -> Result<(), String> {
    let dest = lib.dir(root);
    if lib.installed(root) {
        on(Step::Installed);
        return Ok(());
    }
    let part = root.join(format!("{}.partial", lib.id));
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir_all(&part).map_err(|e| format!("{}: {e}", part.display()))?;
    let filled = fill(lib, source, root, &part, on, cancel);
    if filled.is_err() {
        // What was unpacked goes; a download that broke off stays to carry on from next time.
        let _ = std::fs::remove_dir_all(&part);
    }
    filled?;
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&part, &dest).map_err(|e| format!("{}: {e}", lib.id))?;
    on(Step::Installed);
    Ok(())
}

/// Downloads and unpacks `lib` into `part`, with its licence and the mark that it is complete.
fn fill(lib: &Library, source: Source, root: &Path, part: &Path, on: &mut dyn FnMut(Step), cancel: &AtomicBool) -> Result<(), String> {
    let downloads = root.join(".downloads");
    std::fs::create_dir_all(&downloads).map_err(|e| format!("{}: {e}", downloads.display()))?;
    let fetch = |url: &str, ext: &str, on: &mut dyn FnMut(Step)| -> Result<PathBuf, String> {
        let file = downloads.join(format!("{}.{ext}", lib.id));
        download(url, &file, on, cancel)?;
        Ok(file)
    };
    let archive = match source {
        Source::Zip(url) => Some((fetch(url, "zip", on)?, false)),
        Source::SevenZip(url) => Some((fetch(url, "7z", on)?, true)),
        Source::Git(url) if have("git") => {
            let _ = std::fs::remove_dir_all(part);
            let child = Command::new("git")
                .args(["clone", "--quiet", "--depth", "1", url])
                .arg(part)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| format!("git: {e}"))?;
            wait(child, &format!("cloning {url}"), cancel, &mut || on(Step::Downloading { done: 0, total: None }))?;
            let _ = std::fs::remove_dir_all(part.join(".git"));
            None
        }
        Source::Git(url) => Some((fetch(&archive_of(url), "zip", on)?, false)),
    };
    if let Some((file, seven)) = archive {
        on(Step::Unpacking);
        let unpacked = if seven { un7z(&file, part) } else { unzip(&file, part) };
        // Done with, or not worth resuming: either way the next attempt starts afresh.
        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_file(file.with_extension("url"));
        unpacked?;
    }
    let mut found = Vec::new();
    find_sfz(part, lib.sfz_hint, 0, &mut found);
    if found.is_empty() {
        return Err(format!("{}: no .sfz matching `{}` in what was downloaded", lib.id, lib.sfz_hint));
    }
    let note = format!("{}\nrole: {}\nlicense: {}\ncredit: {}\n", lib.title, lib.role.key(), lib.license, lib.credit);
    std::fs::write(part.join(SOURCE_FILE), note).map_err(|e| e.to_string())?;
    std::fs::write(part.join(COMPLETE), "").map_err(|e| e.to_string())
}

/// Points `bank.json` in `root` at the installed libraries and returns the roles it fills. A role
/// the file maps elsewhere (a library of the user's, articulation programs) is left as it is; a
/// file that cannot be read is kept as `bank.json.bak` and a new one written.
pub fn write_bank(root: &Path) -> Result<Vec<Role>, String> {
    let path = root.join(BANK_FILE);
    let mut map = match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(serde_json::Value::Object(m)) => m,
            _ => {
                let _ = std::fs::rename(&path, root.join(format!("{BANK_FILE}.bak")));
                serde_json::Map::new()
            }
        },
        Err(_) => serde_json::Map::new(),
    };
    for lib in LIBRARIES {
        let Some(program) = lib.program(root) else { continue };
        let ours = match map.get(lib.role.key()) {
            None => true,
            Some(serde_json::Value::String(p)) => p.starts_with(&format!("{}/", lib.id)),
            Some(_) => false,
        };
        if ours {
            map.insert(lib.role.key().to_string(), serde_json::Value::String(program));
        }
    }
    let roles = Role::ALL.into_iter().filter(|r| map.contains_key(r.key())).collect();
    let text = serde_json::to_string_pretty(&serde_json::Value::Object(map)).map_err(|e| e.to_string())?;
    let tmp = root.join(format!("{BANK_FILE}.tmp"));
    std::fs::write(&tmp, text + "\n").map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(roles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gt-install-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A zip of a tiny library: a program and its sample in a folder.
    fn zip_library(at: &Path) {
        let mut z = zip::ZipWriter::new(std::fs::File::create(at).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        z.start_file("Growly/growlybass_dirty.sfz", opts).unwrap();
        z.write_all(b"<region> sample=a.wav key=40").unwrap();
        z.start_file("Growly/a.wav", opts).unwrap();
        z.write_all(&[0u8; 64]).unwrap();
        z.finish().unwrap();
    }

    fn url(p: &Path) -> &'static str {
        Box::leak(format!("file://{}", p.display()).into_boxed_str())
    }

    #[test]
    fn installs_from_a_zip_with_its_licence_and_points_the_bank_at_it() {
        if !have("curl") {
            eprintln!("curl is not installed; skipped");
            return;
        }
        let dir = root("zip");
        let src = dir.join("src.zip");
        zip_library(&src);
        let bank = dir.join("bank");
        std::fs::create_dir_all(&bank).unwrap();
        // The user's own entry for the drums stays; the bass is ours to fill.
        std::fs::write(bank.join(BANK_FILE), r#"{"drums": "mine/kit.sfz"}"#).unwrap();
        let lib = Library::by_id("karoryfer-growlybass").unwrap();
        let mut steps = Vec::new();
        install_from(lib, Source::Zip(url(&src)), &bank, &mut |s| steps.push(s), &AtomicBool::new(false)).unwrap();
        assert!(steps.iter().any(|s| matches!(s, Step::Downloading { done, total: Some(t) } if done == t && *t > 0)), "{steps:?}");
        assert!(steps.contains(&Step::Unpacking) && steps.last() == Some(&Step::Installed));
        assert!(lib.installed(&bank));
        let source = std::fs::read_to_string(lib.dir(&bank).join(SOURCE_FILE)).unwrap();
        assert!(source.contains("CC0 1.0") && source.contains("Karoryfer"));
        assert_eq!(lib.program(&bank).as_deref(), Some("karoryfer-growlybass/Growly/growlybass_dirty.sfz"));
        assert_eq!(write_bank(&bank).unwrap(), vec![Role::Drums, Role::Bass]);
        let sb = crate::SoundBank::open(&bank).unwrap();
        assert_eq!(sb.path_for(Role::Bass), Some(bank.join("karoryfer-growlybass/Growly/growlybass_dirty.sfz").as_path()));
        assert_eq!(sb.path_for(Role::Drums), Some(bank.join("mine/kit.sfz").as_path()));
        // A second run finds it installed and downloads nothing.
        let mut again = Vec::new();
        install_from(lib, Source::Zip("file:///nowhere.zip"), &bank, &mut |s| again.push(s), &AtomicBool::new(false)).unwrap();
        assert_eq!(again, vec![Step::Installed]);
        assert!(!bank.join(".downloads/karoryfer-growlybass.zip").exists(), "the archive is removed once unpacked");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn installs_from_a_7z_and_a_half_downloaded_file_carries_on() {
        if !have("curl") {
            eprintln!("curl is not installed; skipped");
            return;
        }
        let dir = root("7z");
        let content = dir.join("content/Programs");
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(content.join("dist.sfz"), "<region> sample=../Samples/a.flac key=40").unwrap();
        let src = dir.join("src.7z");
        sevenz_rust2::compress_to_path(dir.join("content"), &src).unwrap();
        let bank = dir.join("bank");
        std::fs::create_dir_all(bank.join(".downloads")).unwrap();
        // An earlier attempt at this address broke off after the first third.
        let bytes = std::fs::read(&src).unwrap();
        let partial = bank.join(".downloads/freepats-guitar-distorted.7z");
        std::fs::write(&partial, &bytes[..bytes.len() / 3]).unwrap();
        std::fs::write(partial.with_extension("url"), url(&src)).unwrap();
        let lib = Library::by_id("freepats-guitar-distorted").unwrap();
        let mut first = None;
        install_from(lib, Source::SevenZip(url(&src)), &bank, &mut |s| _ = first.get_or_insert(s), &AtomicBool::new(false)).unwrap();
        assert!(matches!(first, Some(Step::Downloading { done, .. }) if done >= (bytes.len() / 3) as u64), "carried on: {first:?}");
        assert_eq!(lib.program(&bank).as_deref(), Some("freepats-guitar-distorted/Programs/dist.sfz"));
        // A partial download from another address is not carried on.
        let other = bank.join(".downloads/freepats-guitar-clean.7z");
        std::fs::write(&other, b"stale bytes from elsewhere").unwrap();
        std::fs::write(other.with_extension("url"), "https://example.invalid/old.7z").unwrap();
        let clean = Library::by_id("freepats-guitar-clean").unwrap();
        install_from(clean, Source::SevenZip(url(&src)), &bank, &mut |_| {}, &AtomicBool::new(false)).unwrap();
        assert!(clean.installed(&bank));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failures_are_errors_and_cancelling_stops() {
        let dir = root("fail");
        let lib = Library::by_id("karoryfer-growlybass").unwrap();
        // Nothing at the address.
        assert!(install_from(lib, Source::Zip("file:///nowhere/at/all.zip"), &dir, &mut |_| {}, &AtomicBool::new(false)).is_err());
        // Not an archive.
        let junk = dir.join("junk.zip");
        std::fs::write(&junk, b"not a zip").unwrap();
        if have("curl") {
            let e = install_from(lib, Source::Zip(url(&junk)), &dir, &mut |_| {}, &AtomicBool::new(false)).unwrap_err();
            assert!(e.contains("junk") || e.contains("zip"), "{e}");
        }
        // An archive without the program it should hold.
        let empty = dir.join("empty.zip");
        zip::ZipWriter::new(std::fs::File::create(&empty).unwrap()).finish().unwrap();
        if have("curl") {
            let e = install_from(lib, Source::Zip(url(&empty)), &dir, &mut |_| {}, &AtomicBool::new(false)).unwrap_err();
            assert!(e.contains("no .sfz"), "{e}");
        }
        assert!(!lib.installed(&dir));
        assert!(!dir.join("karoryfer-growlybass.partial").exists(), "a failed install leaves nothing unpacked behind");
        let cancelled = install_from(lib, Source::Zip(url(&junk)), &dir, &mut |_| {}, &AtomicBool::new(true));
        assert!(cancelled.is_err());
        // A bank.json that cannot be read is kept aside and a new one written.
        std::fs::write(dir.join(BANK_FILE), "{ not json").unwrap();
        assert!(write_bank(&dir).unwrap().is_empty());
        assert!(dir.join(format!("{BANK_FILE}.bak")).exists());
        assert_eq!(archive_of("https://github.com/a/b.git"), "https://github.com/a/b/archive/HEAD.zip");
        assert!(LIBRARIES.iter().filter(|l| l.needs_credit()).all(|l| l.id == "muldjordkit"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
