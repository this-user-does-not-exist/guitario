//! Imports every Guitar Pro file in the folder `GUITARIO_GP_CORPUS` names (skipped when unset).
//! Real files can't be committed (they belong to their authors), so this runs on a local corpus:
//! `GUITARIO_GP_CORPUS=~/gp-corpus cargo test -p guitario-gp --test corpus -- --nocapture`.
//! Set `GUITARIO_GP_DUMP=1` to print what each file imported as.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

mod common;

#[test]
fn every_corpus_file_imports() {
    let Ok(dir) = std::env::var("GUITARIO_GP_CORPUS") else { return };
    let dump = std::env::var_os("GUITARIO_GP_DUMP").is_some();
    let mut paths: Vec<_> = std::fs::read_dir(&dir).expect("corpus folder").filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();
    let (mut ok, mut failed) = (0, Vec::new());
    for path in paths.iter().filter(|p| guitario_gp::is_guitar_pro_path(&p.to_string_lossy())) {
        match guitario_gp::import_file(path) {
            Ok(imp) => {
                ok += 1;
                println!("ok   {} ({} bars, {} tracks) {:?}", path.display(), imp.score.headers.len(), imp.score.tracks.len(), imp.warnings);
                if dump {
                    print!("{}", describe(&imp.score));
                }
                // Anything that imports survives a trip through a Guitar Pro 7/8 file unchanged.
                match guitario_gp::export_gp(&imp.score).and_then(|out| guitario_gp::import(&out.bytes)) {
                    Ok(back) if back.score == common::as_gp(&imp.score) => {}
                    Ok(back) => {
                        println!("DIFF {}: the Guitar Pro 8 round trip changed the score", path.display());
                        if dump {
                            print!("{}", describe(&back.score));
                        }
                        failed.push(path.clone());
                    }
                    Err(e) => {
                        println!("FAIL {}: its Guitar Pro 8 export does not import: {e}", path.display());
                        failed.push(path.clone());
                    }
                }
                // What was read from Guitar Pro 3-5 survives a trip through the Guitar Pro 5 writer unchanged.
                let legacy = path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["gp3", "gp4", "gp5"].contains(&e.to_ascii_lowercase().as_str()));
                if !legacy {
                    continue;
                }
                let back = guitario_gp::import(&guitario_gp::export_gp5(&imp.score).bytes).map(|b| b.score);
                match back {
                    Ok(back) if back == imp.score => {}
                    Ok(back) => {
                        println!("DIFF {}: the Guitar Pro 5 round trip changed the score", path.display());
                        if dump {
                            print!("{}", describe(&back));
                        }
                        failed.push(path.clone());
                    }
                    Err(e) => {
                        println!("FAIL {}: its Guitar Pro 5 export does not import: {e}", path.display());
                        failed.push(path.clone());
                    }
                }
            }
            Err(e) => {
                println!("FAIL {}: {e}", path.display());
                failed.push(path.clone());
            }
        }
    }
    println!("{ok} imported, {} failed", failed.len());
    assert!(failed.is_empty(), "{failed:?}");
}

fn describe(s: &guitario_score::Score) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "  meta {:?} tempo {}", s.meta, s.tempo);
    for (i, h) in s.headers.iter().enumerate() {
        let _ = writeln!(
            out,
            "  bar {} {}/{} key {} tempo {:?} open {} close {} endings {:#b} marker {:?} targets {:?} jump {:?} feel {:?} double {}",
            i + 1,
            h.time_signature.numerator,
            h.time_signature.denominator,
            h.key,
            h.tempo,
            h.repeat_open,
            h.repeat_close,
            h.endings,
            h.marker,
            h.targets,
            h.jump,
            h.triplet_feel,
            h.double_bar
        );
    }
    for t in &s.tracks {
        let _ = writeln!(out, "  track {:?} {:?} {:?} capo {} program {} vol {} pan {}", t.name, t.kind, t.tuning, t.capo, t.program, t.volume, t.pan);
        for (mi, m) in t.measures.iter().enumerate() {
            for (vi, v) in m.voices.iter().enumerate() {
                let _ = write!(out, "    m{} v{}:", mi + 1, vi + 1);
                for b in &v.beats {
                    let d = b.duration;
                    let _ = write!(
                        out,
                        " [{:?}{}{}",
                        d.value,
                        ".".repeat(d.dots.into()),
                        if d.tuplet != (1, 1) { format!(" {}:{}", d.tuplet.0, d.tuplet.1) } else { String::new() }
                    );
                    if b.rest {
                        let _ = write!(out, " rest");
                    }
                    for n in &b.notes {
                        let _ = write!(out, " {}/{}{}", n.string, n.fret, fx(n));
                    }
                    if b.dynamic != guitario_score::Dynamic::F {
                        let _ = write!(out, " dyn={:?}", b.dynamic);
                    }
                    for (label, on) in [("text", b.text.is_some()), ("chord", b.chord.is_some())] {
                        if on {
                            let _ = write!(out, " {label}");
                        }
                    }
                    if let Some(t) = &b.text {
                        let _ = write!(out, "={t:?}");
                    }
                    if let Some(c) = &b.chord {
                        let _ = write!(out, "={:?}@{}{:?}", c.name, c.first_fret, c.frets);
                    }
                    for x in [
                        b.stroke.map(|x| format!("{x:?}")),
                        b.technique.map(|x| format!("{x:?}")),
                        b.fade.map(|x| format!("{x:?}")),
                        b.brush.map(|x| format!("{x:?}")),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        let _ = write!(out, " {x}");
                    }
                    let _ = write!(out, "]");
                }
                let _ = writeln!(out);
            }
        }
    }
    out
}

fn fx(n: &guitario_score::Note) -> String {
    let e = &n.effects;
    let mut s = String::new();
    if n.kind != guitario_score::NoteKind::Normal {
        s += &format!(" {:?}", n.kind);
    }
    for (label, on) in [
        ("h", e.hammer),
        ("vib", e.vibrato),
        ("wvib", e.wide_vibrato),
        ("pm", e.palm_mute),
        ("lr", e.let_ring),
        ("ghost", e.ghost),
        ("acc", e.accent),
        ("hacc", e.heavy_accent),
        ("stac", e.staccato),
        ("tap", e.tapped),
    ] {
        if on {
            s += &format!(" {label}");
        }
    }
    if let Some(x) = e.slide {
        s += &format!(" slide={x:?}");
    }
    if let Some(x) = e.harmonic {
        s += &format!(" harm={x:?}");
    }
    if let Some(x) = e.bend {
        s += &format!(" bend={:?}", x.points());
    }
    s
}
