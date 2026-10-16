//! `cargo xtask sounds`: downloads the recommended open SFZ libraries into the user's sound
//! folder and writes the `bank.json` that Guitario reads (see `guitario_playback::SoundBank`).
//!
//! The libraries are large (hundreds of MB each) and are never committed. Every one is public
//! domain (CC0) or CC-BY and is listed, with its licence and credit, in `docs/sounds.md` and
//! `ATTRIBUTION.md`. External tools: `curl`, plus `unzip`, `tar` (bsdtar reads 7z) or `git`.

use std::path::{Path, PathBuf};
use std::process::Command;

enum Source {
    Zip(&'static str),
    SevenZip(&'static str),
    Git(&'static str),
}

struct Lib {
    id: &'static str,
    role: &'static str,
    title: &'static str,
    license: &'static str,
    credit: &'static str,
    source: Source,
    /// The program to load: the shortest `.sfz` path (inside the library) containing this text.
    sfz_hint: &'static str,
    mb: u32,
}

const LIBS: &[Lib] = &[
    Lib {
        id: "freepats-guitar-distorted",
        role: "guitar_distorted",
        title: "FreePats FSBS Electric Guitar Distorted #1 (Fender, amp and effects rack)",
        license: "CC0 1.0",
        credit: "FreePats project, https://freepats.zenvoid.org/ElectricGuitar/distorted-electric-guitar.html",
        source: Source::SevenZip("https://github.com/freepats/electric-guitar-FSBS-dist1/releases/download/2022-09-11/EGuitarFSBS-dist1-SFZ+FLAC-20220911.7z"),
        sfz_hint: ".sfz",
        mb: 300,
    },
    Lib {
        id: "freepats-guitar-clean",
        role: "guitar_clean",
        title: "FreePats FSBS Electric Guitar Clean #1 (Fender bridge pickup)",
        license: "CC0 1.0",
        credit: "FreePats project, https://freepats.zenvoid.org/ElectricGuitar/clean-electric-guitar.html",
        source: Source::SevenZip("https://github.com/freepats/electric-guitar-FSBS-clean/releases/download/2026-08-07/EGuitarFSBS-clean-SFZ+FLAC-20260807.7z"),
        sfz_hint: ".sfz",
        mb: 120,
    },
    Lib {
        id: "karoryfer-growlybass",
        role: "bass",
        title: "Karoryfer Growlybass (Squier Jazz Bass, direct, roundwound)",
        license: "CC0 1.0",
        credit: "Karoryfer Samples, https://github.com/sfzinstruments/karoryfer.growlybass",
        source: Source::Zip("https://github.com/sfzinstruments/karoryfer.growlybass/releases/download/v1.002/Karoryfer.Growlybass.v1.002.zip"),
        sfz_hint: "growlybass_clean.sfz",
        mb: 160,
    },
    Lib {
        id: "muldjordkit",
        role: "drums",
        title: "MuldjordKit (Tama Superstar, sampled by Lars Muldjord; SFZ mapping by kinwie)",
        license: "CC BY 4.0",
        credit: "Lars Muldjord / DrumGizmo team; SFZ port by kinwie, https://github.com/sfzinstruments/DrumGizmo.MuldjordKit",
        source: Source::Git("https://github.com/sfzinstruments/DrumGizmo.MuldjordKit.git"),
        sfz_hint: "Stereo",
        mb: 340,
    },
];

const USAGE: &str = "usage: cargo xtask sounds [--list] [--dir <folder>] [--only <id>[,<id>...]]

Downloads open SFZ instruments for guitars, bass and drums and writes <folder>/bank.json.
The folder defaults to $GUITARIO_SOUNDS, else the per-user Guitario data folder.";

fn default_dir() -> Option<PathBuf> {
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

fn sh(mut c: Command, what: &str) -> Result<(), String> {
    eprintln!("==> {what}");
    let status = c.status().map_err(|e| format!("{what}: {e} (is the tool installed?)"))?;
    if status.success() { Ok(()) } else { Err(format!("{what} failed")) }
}

fn find_sfz(dir: &Path, hint: &str, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() && depth < 4 {
            find_sfz(&p, hint, depth + 1, out);
        } else if p.extension().is_some_and(|x| x == "sfz") && p.to_string_lossy().to_lowercase().contains(&hint.to_lowercase()) {
            out.push(p);
        }
    }
}

fn install(lib: &Lib, root: &Path) -> Result<(), String> {
    let dest = root.join(lib.id);
    if dest.join(".complete").exists() {
        eprintln!("==> {} already installed", lib.id);
        return Ok(());
    }
    let part = root.join(format!("{}.partial", lib.id));
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir_all(&part).map_err(|e| e.to_string())?;
    let downloads = root.join(".downloads");
    std::fs::create_dir_all(&downloads).map_err(|e| e.to_string())?;
    let fetch = |url: &str, name: &str| -> Result<PathBuf, String> {
        let file = downloads.join(name);
        let mut c = Command::new("curl");
        c.args(["-fL", "--retry", "3", "-C", "-", "-o"]).arg(&file).arg(url);
        sh(c, &format!("download {url}"))?;
        Ok(file)
    };
    match &lib.source {
        Source::Zip(url) => {
            let f = fetch(url, &format!("{}.zip", lib.id))?;
            let mut c = Command::new("unzip");
            c.args(["-q", "-o"]).arg(&f).arg("-d").arg(&part);
            sh(c, "unzip")?;
            let _ = std::fs::remove_file(f);
        }
        Source::SevenZip(url) => {
            let f = fetch(url, &format!("{}.7z", lib.id))?;
            let mut c = Command::new("tar");
            c.arg("-xf").arg(&f).arg("-C").arg(&part);
            sh(c, "extract 7z (needs bsdtar/libarchive or run `brew install sevenzip` and extract by hand)")?;
            let _ = std::fs::remove_file(f);
        }
        Source::Git(url) => {
            let _ = std::fs::remove_dir_all(&part);
            let mut c = Command::new("git");
            c.args(["clone", "--depth", "1", url]).arg(&part);
            sh(c, "git clone")?;
            let _ = std::fs::remove_dir_all(part.join(".git"));
        }
    }
    let mut found = Vec::new();
    find_sfz(&part, lib.sfz_hint, 0, &mut found);
    if found.is_empty() {
        return Err(format!("{}: no .sfz matching `{}` after extracting", lib.id, lib.sfz_hint));
    }
    let note = format!("{}\nrole: {}\nlicense: {}\ncredit: {}\n", lib.title, lib.role, lib.license, lib.credit);
    std::fs::write(part.join("SOURCE.txt"), note).map_err(|e| e.to_string())?;
    std::fs::write(part.join(".complete"), "").map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&part, &dest).map_err(|e| format!("{}: {e}", lib.id))
}

/// The program path of an installed library, relative to `root`.
fn program(lib: &Lib, root: &Path) -> Option<String> {
    let dest = root.join(lib.id);
    if !dest.join(".complete").exists() {
        return None;
    }
    let mut found = Vec::new();
    find_sfz(&dest, lib.sfz_hint, 0, &mut found);
    found.sort_by_key(|p| p.as_os_str().len());
    let first = found.first()?;
    first.strip_prefix(root).ok().map(|p| p.to_string_lossy().replace('\\', "/"))
}

fn write_bank(root: &Path) -> Result<Vec<&'static str>, String> {
    let mut map = serde_json::Map::new();
    let mut roles = Vec::new();
    for lib in LIBS {
        if let Some(p) = program(lib, root) {
            map.insert(lib.role.to_string(), serde_json::Value::String(p));
            roles.push(lib.role);
        }
    }
    let text = serde_json::to_string_pretty(&serde_json::Value::Object(map)).map_err(|e| e.to_string())?;
    std::fs::write(root.join("bank.json"), text + "\n").map_err(|e| e.to_string())?;
    Ok(roles)
}

pub fn run(args: &[&str]) -> Result<(), String> {
    let mut dir = None;
    let mut only: Option<Vec<String>> = None;
    let mut list = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match *a {
            "--list" => list = true,
            "--dir" => dir = it.next().map(PathBuf::from),
            "--only" => only = it.next().map(|s| s.split(',').map(str::to_string).collect()),
            _ => return Err(USAGE.into()),
        }
    }
    if list {
        for l in LIBS {
            println!("{:<28} {:<17} {:>4} MB  {:<9} {}", l.id, l.role, l.mb, l.license, l.title);
        }
        return Ok(());
    }
    let root = dir.or_else(default_dir).ok_or("cannot find a data folder; pass --dir <folder>")?;
    std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    if let Some(o) = &only
        && let Some(bad) = o.iter().find(|id| !LIBS.iter().any(|l| l.id == id.as_str()))
    {
        return Err(format!("unknown library `{bad}`; see --list"));
    }
    let mut failed = Vec::new();
    for lib in LIBS.iter().filter(|l| only.as_ref().is_none_or(|o| o.iter().any(|id| id == l.id))) {
        if let Err(e) = install(lib, &root) {
            eprintln!("error: {e}");
            failed.push(lib.id);
        }
    }
    let roles = write_bank(&root)?;
    eprintln!("==> {} ready for: {}", root.join("bank.json").display(), roles.join(", "));
    if failed.is_empty() { Ok(()) } else { Err(format!("failed: {}", failed.join(", "))) }
}
