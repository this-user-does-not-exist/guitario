//! The demo suite in `demos/` (regenerate with `cargo run -p guitario-engine --example demos`):
//! every score must load, validate, export and play, and the suite must keep its metal focus.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use guitario_engine::Session;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn demos() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos")
}

fn scores() -> Vec<(String, PathBuf)> {
    let mut v: Vec<_> = std::fs::read_dir(demos())
        .expect("demos/ exists; run the demos example")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "gt"))
        .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), p))
        .collect();
    v.sort();
    v
}

fn open(path: &Path) -> Session {
    let mut s = Session::new();
    s.run("file.open", &json!({"path": path.display().to_string()})).expect("demo opens");
    s
}

#[test]
fn the_suite_is_complete() {
    let names: Vec<String> = scores().into_iter().map(|(n, _)| n).collect();
    assert!(names.len() >= 14, "{names:?}");
    for want in [
        "thrash",
        "funeral",
        "burial",
        "steel-horizon",
        "seven-strings",
        "odd-meter",
        "iron-gallop",
        "frostbitten",
        "breakdown",
        "fretboard-fury",
        "technique",
        "bass",
    ] {
        assert!(names.iter().any(|n| n.contains(want)), "missing a `{want}` demo in {names:?}");
    }
    // Each score ships with MIDI and ASCII tab copies.
    for n in &names {
        for (sub, ext) in [("midi", "mid"), ("tab", "txt")] {
            let p = demos().join(sub).join(format!("{n}.{ext}"));
            assert!(std::fs::metadata(&p).is_ok_and(|m| m.len() > 0), "{} missing or empty", p.display());
        }
    }
}

#[test]
fn every_demo_loads_validates_and_plays() {
    for (name, path) in scores() {
        let mut s = open(&path);
        assert!(s.run("score.validate", &Value::Null).is_ok(), "{name}");
        let score = s.score().clone();
        assert!(!score.meta.title.is_empty() || name.contains("blank"), "{name} has no title");
        let notes = guitario_score::timeline(&score);
        if !name.contains("blank") {
            assert!(!notes.is_empty(), "{name} has no notes");
            // Rendered audio is neither silent nor clipped into garbage.
            let pcm = guitario_playback::render_wav(&score, 11_025);
            let peak = pcm.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(pcm.iter().all(|x| x.is_finite()), "{name}");
            assert!(peak > 0.02 && peak <= 1.0, "{name} peak {peak}");
        }
        // Every track is filled to the bar line: no half-entered measures.
        for (ti, t) in score.tracks.iter().enumerate() {
            for (mi, m) in t.measures.iter().enumerate() {
                let used = m.voices[0].ticks();
                assert!(used == score.headers[mi].capacity(), "{name} track {ti} bar {} is {used}/{} ticks", mi + 1, score.headers[mi].capacity());
            }
        }
        let midi = guitario_midi::export(&score).expect("midi export");
        assert!(midi.starts_with(b"MThd"), "{name}");
    }
}

#[test]
fn the_suite_is_metal() {
    let mut low = 0;
    let mut distorted = 0;
    let mut seven = false;
    let mut scores_with_gallop_or_chug = 0;
    for (name, path) in scores() {
        if name.contains("blank") {
            continue;
        }
        let s = open(&path);
        let score = s.score();
        let guitars = || score.tracks.iter().filter(|t| !t.is_drums() && t.tuning.string_count() >= 6);
        // Low tunings: a lowest string below E2 (40) on a guitar track.
        if guitars().any(|t| t.tuning.strings.iter().min().is_some_and(|p| *p < 40)) {
            low += 1;
        }
        if score.tracks.iter().any(|t| matches!(t.program, 29 | 30)) {
            distorted += 1;
        }
        seven |= score.tracks.iter().any(|t| t.tuning.string_count() == 7);
        let palm = score
            .tracks
            .iter()
            .flat_map(|t| &t.measures)
            .flat_map(|m| &m.voices)
            .flat_map(|v| &v.beats)
            .flat_map(|b| &b.notes)
            .filter(|n| n.effects.palm_mute)
            .count();
        if palm >= 4 {
            scores_with_gallop_or_chug += 1;
        }
    }
    assert!(low >= 5, "only {low} demos use drop or low tunings");
    assert!(distorted >= 12, "only {distorted} demos use overdriven or distorted tones");
    assert!(seven, "no 7-string demo");
    assert!(scores_with_gallop_or_chug >= 8, "only {scores_with_gallop_or_chug} demos use palm-muted chugging");
}

#[test]
fn specific_pieces_have_their_signature_features() {
    let load = |n: &str| open(&scores().into_iter().find(|(name, _)| name.contains(n)).unwrap().1);
    // Odd meters.
    let odd = load("odd-meter");
    let sigs: Vec<_> = odd.score().headers.iter().map(|h| (h.time_signature.numerator, h.time_signature.denominator)).collect();
    for want in [(7, 8), (5, 4), (13, 16), (6, 8), (12, 8)] {
        assert!(sigs.contains(&want), "{want:?} missing from {sigs:?}");
    }
    // Black metal tremolo uses thirty-second notes and a key signature.
    let bm = load("frostbitten");
    assert!(bm.score().tracks[0].measures[0].voices[0].beats.iter().all(|b| b.duration.value.divisor() == 32));
    assert_eq!(bm.score().headers[0].key, 3);
    // Djent is on a seven-string.
    assert_eq!(load("seven-strings").score().tracks[0].string_count(), 7);
    // The power-metal demo has twin leads, rhythm, bass and drums.
    assert_eq!(load("steel-horizon").score().tracks.len(), 5);
    // The technique showcase touches every effect.
    let t = load("technique");
    let notes: Vec<_> = t.score().tracks[0].measures.iter().flat_map(|m| &m.voices[0].beats).flat_map(|b| b.notes.iter()).copied().collect();
    assert!(notes.iter().any(|n| n.effects.hammer) && notes.iter().any(|n| n.effects.slide.is_some()) && notes.iter().any(|n| n.effects.bend.is_some()));
    assert!(notes.iter().any(|n| n.effects.harmonic.is_some()) && notes.iter().any(|n| n.effects.vibrato) && notes.iter().any(|n| n.effects.palm_mute));
    assert!(notes.iter().any(|n| n.effects.ghost) && notes.iter().any(|n| n.effects.accent));
    assert!(notes.iter().any(|n| n.kind == guitario_score::NoteKind::Dead) && notes.iter().any(|n| n.kind == guitario_score::NoteKind::Tie));
}

#[test]
fn the_guitar_pro_fixture_imports_through_the_engine() {
    let mut s = Session::new();
    let path = demos().join("guitar-pro/import-test.gp");
    let r = s.run("file.importGp", &json!({"path": path.display().to_string()})).expect("import");
    assert_eq!(r["warnings"].as_array().map(Vec::len), Some(0));
    assert_eq!(s.score().tracks[0].tuning.strings, vec![64, 59, 55, 50, 45, 38]);
    // And it survives a round trip through the native format.
    let tmp = std::env::temp_dir().join(format!("gc-demos-{}.gt", std::process::id()));
    s.run("file.save", &json!({"path": tmp.display().to_string()})).unwrap();
    assert_eq!(**open(&tmp).score(), **s.score());
    std::fs::remove_file(tmp).ok();
}

#[test]
fn band_pieces_have_drums_on_their_kit_lanes() {
    let mut with_drums = 0;
    for (name, path) in scores() {
        let s = open(&path);
        let score = s.score();
        for t in score.tracks.iter().filter(|t| t.is_drums()) {
            with_drums += 1;
            let hits: Vec<_> = t.measures.iter().flat_map(|m| &m.voices[0].beats).flat_map(|b| &b.notes).collect();
            assert!(!hits.is_empty(), "{name}: empty drum track");
            for n in hits {
                assert_eq!(n.string, guitario_theory::drums::lane_for(n.fret), "{name}: drum {} on the wrong lane", n.fret);
                assert!(guitario_theory::drums::piece(n.fret).is_some(), "{name}: unknown drum {}", n.fret);
            }
            // Drums go to General MIDI channel 10.
            let midi = guitario_midi::export(score).unwrap();
            assert!(midi.windows(2).any(|w| w[0] == 0x99), "{name}: no channel-10 notes");
        }
    }
    assert!(with_drums >= 12, "only {with_drums} demos have drums");
}

#[test]
fn the_drum_reference_covers_the_metal_grooves() {
    let (_, path) = scores().into_iter().find(|(n, _)| n.contains("drum-grooves")).unwrap();
    let s = open(&path);
    let t = &s.score().tracks[0];
    assert!(t.is_drums() && s.score().tracks.len() == 1);
    let bars: Vec<Vec<&guitario_score::Beat>> = t.measures.iter().map(|m| m.voices[0].beats.iter().collect()).collect();
    let has = |b: &guitario_score::Beat, m: u8| b.notes.iter().any(|n| n.fret == m);
    // A blast beat: sixteen sixteenths alternating kick and snare.
    assert!(
        bars.iter()
            .any(|bar| bar.len() == 16 && bar.iter().enumerate().all(|(i, b)| b.duration.value.divisor() == 16 && has(b, if i % 2 == 0 { 36 } else { 38 })))
    );
    // Double kick: a kick on every sixteenth.
    assert!(bars.iter().any(|bar| bar.len() == 16 && bar.iter().all(|b| has(b, 36))));
    // A triplet tom fill and china hits.
    assert!(bars.iter().any(|bar| bar.iter().filter(|b| b.duration.tuplet == (3, 2) && b.notes.iter().any(|n| matches!(n.fret, 43 | 48 | 50))).count() >= 6));
    assert!(bars.iter().flatten().any(|b| has(b, 52)));
    // It plays.
    let pcm = guitario_playback::render_wav(s.score(), 11_025);
    let peak = pcm.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!(peak > 0.05, "drum render peak {peak}");
}
