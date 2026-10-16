//! Parses real SFZ programs when `GUITARIO_SFZ_DIR` points at a folder of libraries (text only;
//! no audio is read). Skipped otherwise. Prints a summary with `--nocapture`.

use guitario_playback::sfz::parse;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "sfz") {
            out.push(p);
        }
    }
}

#[test]
fn parses_real_programs() {
    let Ok(dir) = std::env::var("GUITARIO_SFZ_DIR") else { return };
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    files.sort();
    let (mut with_regions, mut total) = (0, 0);
    for f in &files {
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        let base = f.parent().unwrap_or(Path::new(".")).to_path_buf();
        let parsed = parse(&text, &mut |rel| std::fs::read_to_string(base.join(rel.replace('\\', "/"))).ok());
        total += 1;
        if !parsed.regions.is_empty() {
            with_regions += 1;
        }
        let (lo, hi) = parsed.regions.iter().fold((127u8, 0u8), |a, r| (a.0.min(r.lokey), a.1.max(r.hikey)));
        println!("{:4} regions keys {lo:3}-{hi:3} warn {:2}  {}", parsed.regions.len(), parsed.warnings.len(), f.strip_prefix(&dir).unwrap_or(f).display());
        for w in parsed.warnings.iter().take(2) {
            println!("        ! {w}");
        }
    }
    println!("{with_regions}/{total} programs produced regions");
}

/// `GUITARIO_SFZ_FILE=<one program>`: prints its key map (which keys have regions, how many).
#[test]
fn prints_key_map() {
    let Ok(file) = std::env::var("GUITARIO_SFZ_FILE") else { return };
    let f = Path::new(&file);
    let Ok(text) = std::fs::read_to_string(f) else { return };
    let base = f.parent().unwrap_or(Path::new(".")).to_path_buf();
    let parsed = parse(&text, &mut |rel| std::fs::read_to_string(base.join(rel.replace('\\', "/"))).ok());
    let mut keys = std::collections::BTreeMap::new();
    for r in &parsed.regions {
        *keys.entry((r.lokey, r.hikey)).or_insert(0usize) += 1;
    }
    println!("{} regions, {} warnings", parsed.regions.len(), parsed.warnings.len());
    for ((lo, hi), n) in keys {
        println!("keys {lo:3}-{hi:3}: {n} regions");
    }
    if let Some(r) = parsed.regions.first() {
        println!("first: {r:?}");
    }
}

/// `GUITARIO_SFZ_RENDER=<program.sfz>` (+ optional `GUITARIO_SFZ_OUT=<out.wav>`): plays a
/// short riff on a pitched track through a real instrument, decoding its real samples.
#[test]
fn renders_with_a_real_instrument() {
    use guitario_playback::sfz::Instrument;
    use guitario_playback::{Options, Player, used_keys, wav_bytes};
    use guitario_score::{Beat, Note, Score};
    use guitario_theory::Duration;
    use std::sync::Arc;

    let Ok(file) = std::env::var("GUITARIO_SFZ_RENDER") else { return };
    let mut s = Score::new();
    let beats = [(0u8, 0u8), (0, 3), (1, 0), (2, 2), (3, 5), (5, 0)];
    s.tracks[0].measures[0].voices[0].beats =
        beats.iter().map(|(string, fret)| Beat { rest: false, notes: vec![Note::new(*string, *fret)], ..Beat::rest(Duration::EIGHTH) }).collect();
    let keys = used_keys(&s, 0);
    let t0 = std::time::Instant::now();
    let inst = Instrument::load(Path::new(&file), Some(&keys)).expect("instrument loads");
    println!("loaded {} regions for keys {keys:?} in {:?}; warnings {:?}", inst.region_count(), t0.elapsed(), inst.warnings.iter().take(3).collect::<Vec<_>>());
    let mut p = Player::new(&s, 44_100, Options::default());
    p.set_instrument(0, Some(Arc::new(inst)));
    p.play_from(0.0);
    let mut out = vec![0.0f32; 44_100 * 2 * 4];
    p.render(&mut out, 2);
    let rms = (out.iter().map(|x| x * x).sum::<f32>() / out.len() as f32).sqrt();
    let peak = out.iter().fold(0.0f32, |a, x| a.max(x.abs()));
    println!("rms {rms:.4} peak {peak:.3}");
    assert!(rms > 0.005 && peak <= 1.0 && out.iter().all(|x| x.is_finite()));
    if let Ok(path) = std::env::var("GUITARIO_SFZ_OUT") {
        std::fs::write(path, wav_bytes(&out, 44_100)).expect("write wav");
    }
}
