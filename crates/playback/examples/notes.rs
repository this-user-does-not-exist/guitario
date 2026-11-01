//! Renders single held notes of a modelled instrument, dry (no reverb), one WAV per note, to set
//! against recordings of the real instrument:
//! `cargo run -p guitario-playback --release --example notes -- <dir> <program> <midi,…> [dynamic,…] [seconds]`.
//!
//! Each note is bowed (or struck) once and held for `seconds` (default 4, in whole bars at 60 bpm),
//! then let go, with a bar's rest after. Files are named `<program>-<midi>-<dynamic>.wav`.
//! `WET=1` keeps the track's default effects (strings: a hall); `RATE=96000` renders at another
//! sample rate.

// A development tool over fixed tables.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use guitario_score::{Beat, Dynamic, MeasureHeader, Note, Score, Track, Voice};
use guitario_theory::{Duration, NoteValue, TimeSignature, Tuning};
use std::path::PathBuf;

fn rate() -> u32 {
    std::env::var("RATE").ok().and_then(|r| r.parse().ok()).unwrap_or(48_000)
}

fn dynamic(s: &str) -> Dynamic {
    match s {
        "ppp" => Dynamic::Ppp,
        "pp" => Dynamic::Pp,
        "p" => Dynamic::P,
        "mp" => Dynamic::Mp,
        "mf" => Dynamic::Mf,
        "f" => Dynamic::F,
        "ff" => Dynamic::Ff,
        "fff" => Dynamic::Fff,
        d => panic!("bad dynamic {d}"),
    }
}

/// One note on a one-string "instrument" tuned to it, held for `seconds` at 60 bpm (whole notes
/// are four seconds, so the note is tied over as many bars as it needs), then a bar's rest.
fn score(program: u8, midi: u8, dynamic: Dynamic, seconds: f32, vibrato: bool) -> Score {
    let bars = (seconds / 4.0).ceil().max(1.0) as usize;
    let mut s = Score::new();
    let mut h = MeasureHeader::new(TimeSignature { numerator: 4, denominator: 4 });
    h.tempo = Some(60);
    s.headers = vec![h];
    for _ in 0..bars {
        s.headers.push(MeasureHeader::new(TimeSignature { numerator: 4, denominator: 4 }));
    }
    s.tracks.clear();
    // Stopped a fifth above the string, so the note is fingered (an open string has no vibrato).
    let mut t = Track::new("Note", Tuning { name: "Note".into(), strings: vec![midi.saturating_sub(7)] }, &s.headers);
    t.program = program;
    t.interpretation.stringed = true;
    // Dry unless asked for the track's default effects (strings: a hall).
    if std::env::var("WET").is_err() {
        t.effects = Some(Vec::new());
    }
    for (i, m) in t.measures.iter_mut().enumerate() {
        let mut b = Beat::rest(Duration::new(NoteValue::Whole));
        b.dynamic = dynamic;
        if i < bars {
            b.rest = false;
            let mut n = Note::new(0, 7);
            n.effects.vibrato = vibrato;
            if i > 0 {
                n.kind = guitario_score::NoteKind::Tie;
            }
            b.notes.push(n);
        }
        if m.voices.is_empty() {
            m.voices.push(Voice::default());
        }
        m.voices[0].beats = vec![b];
    }
    s.tracks.push(t);
    s
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().unwrap_or_else(|| "notes".into()));
    let program: u8 = args.next().unwrap_or_else(|| "40".into()).parse().expect("program");
    let notes: Vec<u8> = args.next().unwrap_or_else(|| "55,67,79,91".into()).split(',').map(|n| n.parse().expect("midi note")).collect();
    let dynamics: Vec<String> = args.next().unwrap_or_else(|| "p,f".into()).split(',').map(str::to_owned).collect();
    let seconds: f32 = args.next().unwrap_or_else(|| "4".into()).parse().expect("seconds");
    std::fs::create_dir_all(&dir).expect("output directory");
    for &midi in &notes {
        for d in &dynamics {
            let s = score(program, midi, dynamic(d), seconds, false);
            if let Err(e) = s.validate() {
                eprintln!("{midi}: invalid score: {e}");
                continue;
            }
            let samples = guitario_playback::render_wav(&s, rate());
            let path = dir.join(format!("{program}-{midi}-{d}.wav"));
            std::fs::write(&path, guitario_playback::wav_bytes(&samples, rate())).expect("write");
        }
    }
    println!("{} notes -> {}", notes.len() * dynamics.len(), dir.display());
}
