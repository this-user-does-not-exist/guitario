//! `cargo xtask sounds`: downloads the recommended open SFZ libraries into the user's sound
//! folder and writes the `bank.json` that Guitario reads. It runs `guitario_playback::install`, as
//! the app's Sound Bank window does.
//!
//! The libraries are large (hundreds of MB each) and are never committed. Every one is public
//! domain (CC0) or CC-BY and is listed, with its licence and credit, in `docs/sounds.md` and
//! `ATTRIBUTION.md`. Needs `curl`.

use guitario_playback::SoundBank;
use guitario_playback::install::{LIBRARIES, Step, install, write_bank};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

const USAGE: &str = "usage: cargo xtask sounds [--list] [--dir <folder>] [--only <id>[,<id>...]]

Downloads open SFZ instruments for guitars, bass and drums and writes <folder>/bank.json.
The folder defaults to $GUITARIO_SOUNDS, else the per-user Guitario data folder.";

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
        for l in LIBRARIES {
            println!("{:<28} {:<17} {:>4} MB  {:<9} {}", l.id, l.role.key(), l.mb, l.license, l.title);
        }
        return Ok(());
    }
    let root = dir.or_else(SoundBank::default_dir).ok_or("cannot find a data folder; pass --dir <folder>")?;
    std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    if let Some(o) = &only
        && let Some(bad) = o.iter().find(|id| !LIBRARIES.iter().any(|l| l.id == id.as_str()))
    {
        return Err(format!("unknown library `{bad}`; see --list"));
    }
    let mut failed = Vec::new();
    let never = AtomicBool::new(false);
    for lib in LIBRARIES.iter().filter(|l| only.as_ref().is_none_or(|o| o.iter().any(|id| id == l.id))) {
        eprintln!("==> {}", lib.id);
        let mut shown = 0u64;
        let mut progress = |s: Step| match s {
            Step::Downloading { done, total } if done >= shown + (8 << 20) || total == Some(done) => {
                shown = done;
                eprintln!("    {} of {} MB", done >> 20, total.map_or_else(|| "?".to_string(), |t| (t >> 20).to_string()));
            }
            Step::Unpacking => eprintln!("    unpacking"),
            Step::Installed => eprintln!("    installed"),
            Step::Downloading { .. } => {}
        };
        if let Err(e) = install(lib, &root, &mut progress, &never) {
            eprintln!("error: {e}");
            failed.push(lib.id);
        }
    }
    let roles = write_bank(&root)?;
    let names: Vec<&str> = roles.iter().map(|r| r.key()).collect();
    eprintln!("==> {} ready for: {}", root.join("bank.json").display(), names.join(", "));
    if failed.is_empty() { Ok(()) } else { Err(format!("failed: {}", failed.join(", "))) }
}
