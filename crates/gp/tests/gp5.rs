//! Guitar Pro 5 export read back by the Guitar Pro 3-5 reader: every demo survives the trip, and
//! damaged files are refused without a panic.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

use guitario_gp::{GpError, export_gp5, import};
use guitario_score::*;
use guitario_theory::{Duration, NoteValue, TICKS_PER_QUARTER, TimeSignature};

fn demos() -> Vec<(String, Score)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos");
    let mut out: Vec<(String, Score)> = std::fs::read_dir(&dir)
        .expect("demos folder")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "gt"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), guitario_format::open(&p).expect("demo opens")))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(out.len() >= 15);
    out
}

/// What a score looks like after a trip through Guitar Pro 5: the format's own limits applied.
fn as_gp5(s: &Score) -> Score {
    let mut s = s.clone();
    let mut tempo = s.tempo;
    for h in &mut s.headers {
        h.free_time = false;
        if !matches!(h.triplet_feel, TripletFeel::None | TripletFeel::Triplet8th | TripletFeel::Triplet16th) {
            h.triplet_feel = TripletFeel::None;
        }
        match h.tempo {
            Some(t) if t == tempo => h.tempo = None,
            Some(t) => tempo = t,
            None => {}
        }
    }
    for t in &mut s.tracks {
        t.tuning.name.clear();
        t.volume = ((t.volume.saturating_add(4) / 8).min(16) * 8).min(127);
        let balance = ((i16::from(t.pan) + 64 + 4) / 8).clamp(0, 16);
        t.pan = (balance * 8 - 64).clamp(-63, 63) as i8;
        for m in &mut t.measures {
            m.voices.truncate(2);
            if m.voices.len() == 2 && m.voices[1].beats.iter().all(|b| b.rest) {
                m.voices.pop();
            }
            for b in m.voices.iter_mut().flat_map(|v| &mut v.beats) {
                b.duration.dots = b.duration.dots.min(1);
                if b.rest {
                    b.dynamic = Dynamic::default();
                }
                b.text = b.text.as_ref().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
                if !matches!(b.fade, Some(Fade::In)) {
                    b.fade = None;
                }
                if let Some(br) = &mut b.brush
                    && br.kind != BrushKind::Rasgueado
                {
                    br.kind = BrushKind::Brush;
                    let speed = (1..=6u32).min_by_key(|s| (TICKS_PER_QUARTER >> (6 - s)).abs_diff(br.ticks)).unwrap();
                    br.ticks = TICKS_PER_QUARTER >> (6 - speed);
                }
                if let Some(br) = &mut b.brush
                    && br.kind == BrushKind::Rasgueado
                {
                    *br = Brush { kind: BrushKind::Rasgueado, down: true, ticks: TICKS_PER_QUARTER / 8 };
                }
                // Tapping and wide vibrato are beat effects in Guitar Pro 5.
                let tapped = b.technique.is_none() && b.notes.iter().any(|n| n.effects.tapped);
                let wide = b.notes.iter().any(|n| n.effects.wide_vibrato);
                for n in &mut b.notes {
                    n.effects.left_hand_tapped = false;
                    n.effects.tapped = tapped;
                    n.effects.wide_vibrato = wide;
                }
            }
        }
    }
    s
}

fn round_trip(s: &Score) -> (Score, Vec<String>, Vec<String>) {
    let out = export_gp5(s);
    let back = import(&out.bytes).unwrap_or_else(|e| panic!("exported file does not import: {e}"));
    (back.score, out.warnings, back.warnings)
}

fn assert_same(name: &str, expected: &Score, got: &Score) {
    assert_eq!(expected.meta, got.meta, "{name}: song information");
    assert_eq!(expected.tempo, got.tempo, "{name}: tempo");
    assert_eq!(expected.headers.len(), got.headers.len(), "{name}: bar count");
    for (i, (a, b)) in expected.headers.iter().zip(&got.headers).enumerate() {
        assert_eq!(a, b, "{name}: bar {}", i + 1);
    }
    assert_eq!(expected.tracks.len(), got.tracks.len(), "{name}: track count");
    for (ti, (a, b)) in expected.tracks.iter().zip(&got.tracks).enumerate() {
        assert_eq!(
            (&a.name, a.kind, &a.tuning.strings, a.capo, a.program, a.volume, a.pan, a.muted, a.solo),
            (&b.name, b.kind, &b.tuning.strings, b.capo, b.program, b.volume, b.pan, b.muted, b.solo),
            "{name}: track {ti}"
        );
        for (mi, (ma, mb)) in a.measures.iter().zip(&b.measures).enumerate() {
            assert_eq!(ma, mb, "{name}: track {ti} bar {}", mi + 1);
        }
    }
}

#[test]
fn every_demo_survives_guitar_pro_5() {
    for (name, score) in demos() {
        let (back, export_warnings, import_warnings) = round_trip(&score);
        assert!(import_warnings.is_empty(), "{name}: {import_warnings:?}");
        let mut expected = as_gp5(&score);
        let mut got = back;
        for t in &mut got.tracks {
            t.tuning.name.clear();
        }
        // Guitar Pro has no "unset" dynamic; both sides agree once read back.
        assert_same(&name, &expected, &got);
        // A second trip changes nothing more.
        let (again, _, _) = round_trip(&got);
        for t in &mut expected.tracks {
            t.tuning.name.clear();
        }
        let mut again = again;
        for t in &mut again.tracks {
            t.tuning.name.clear();
        }
        assert_same(&name, &got, &again);
        let _ = export_warnings;
    }
}

fn riff() -> Score {
    let mut s = Score::new();
    s.meta = Meta {
        title: "Ünïcode – riff".into(),
        artist: "Guitario".into(),
        album: "Tests".into(),
        author: "Tabber".into(),
        notes: "line one\nline two".into(),
    };
    s.tempo = 152;
    let e = Duration::EIGHTH;
    let mut h = MeasureHeader::new(TimeSignature::COMMON);
    h.repeat_open = true;
    h.marker = Some("Intro".into());
    h.key = -3;
    h.triplet_feel = TripletFeel::Triplet16th;
    h.targets = vec![DirectionTarget::Segno];
    let mut h2 = MeasureHeader::new(TimeSignature { numerator: 6, denominator: 8 });
    h2.repeat_close = 3;
    h2.endings = 0b101;
    h2.tempo = Some(170);
    h2.double_bar = true;
    h2.jump = Some(Jump::DalSegno(JumpEnd::Fine));
    s.headers = vec![h, h2];
    let mut t = guitario_score::Track::new("Lead", guitario_theory::Tuning::drop_d(), &s.headers);
    t.capo = 2;
    t.program = 30;
    let mut n = Note::new(5, 0);
    n.effects.palm_mute = true;
    let mut bend = Note::new(1, 7);
    bend.effects.bend = Some(4);
    bend.effects.vibrato = true;
    let mut tie = Note::new(1, 7);
    tie.kind = NoteKind::Tie;
    let mut slide = Note::new(2, 5);
    slide.effects.slide = Some(Slide::IntoFromBelow);
    slide.effects.harmonic = Some(Harmonic::Pinch);
    slide.effects.accent = true;
    let mut dead = Note::new(3, 0);
    dead.kind = NoteKind::Dead;
    dead.effects.ghost = true;
    let note = |notes: Vec<Note>, d: Duration| Beat { rest: false, notes, ..Beat::rest(d) };
    let mut chord = note(vec![Note::new(3, 2), Note::new(4, 0), Note::new(5, 0)], Duration::QUARTER);
    chord.chord = Some(Chord { name: "D5".into(), frets: vec![None, None, None, Some(2), Some(0), Some(0)], first_fret: 1 });
    chord.dynamic = Dynamic::Ff;
    chord.brush = Some(Brush { kind: BrushKind::Brush, down: true, ticks: TICKS_PER_QUARTER / 4 });
    chord.text = Some("let it ring".into());
    let mut popped = note(vec![n], e);
    popped.technique = Some(BeatTechnique::Pop);
    popped.stroke = Some(StrokeDirection::Up);
    popped.fade = Some(Fade::In);
    let mut triplet = note(vec![slide], Duration { tuplet: (3, 2), ..e });
    triplet.dynamic = Dynamic::Pp;
    t.measures[0].voices[0].beats = vec![chord, popped, note(vec![bend], e), note(vec![tie], Duration::QUARTER), triplet.clone(), triplet.clone(), triplet];
    t.measures[1].voices = vec![
        Voice { beats: vec![note(vec![dead], Duration { dots: 1, ..Duration::QUARTER }), Beat::rest(Duration { dots: 1, ..Duration::QUARTER })] },
        Voice { beats: vec![Beat::rest(Duration::QUARTER), note(vec![Note::new(0, 12)], Duration::new(NoteValue::Half))] },
    ];
    let mut drums = guitario_score::Track::drums("Drums", &s.headers);
    drums.muted = true;
    let hit = |midi: u8| Note::new(guitario_theory::drums::lane_for(midi), midi);
    drums.measures[0].voices[0].beats =
        vec![note(vec![hit(49), hit(38), hit(36)], Duration::new(NoteValue::Half)), note(vec![hit(42)], Duration::new(NoteValue::Half))];
    s.tracks = vec![t, drums];
    s.validate().unwrap();
    s
}

#[test]
fn a_riff_with_every_effect_round_trips() {
    let s = riff();
    let (back, export_warnings, import_warnings) = round_trip(&s);
    assert!(export_warnings.is_empty(), "{export_warnings:?}");
    assert!(import_warnings.is_empty(), "{import_warnings:?}");
    let mut got = back.clone();
    assert_eq!(got.tracks[0].tuning.name, "Drop D", "a preset tuning keeps its name");
    for t in &mut got.tracks {
        t.tuning.name.clear();
    }
    assert_same("riff", &as_gp5(&s), &got);
    // The sixteenth triplet feel, segno and D.S. al Fine are where they were.
    assert_eq!(back.headers[0].triplet_feel, TripletFeel::Triplet16th);
    assert_eq!(back.headers[1].jump, Some(Jump::DalSegno(JumpEnd::Fine)));
    assert_eq!(back.headers[1].tempo, Some(170));
}

#[test]
fn what_guitar_pro_5_cannot_hold_is_reported() {
    let mut s = riff();
    let b = &mut s.tracks[0].measures[0].voices[0].beats[1];
    b.fade = Some(Fade::Swell);
    b.duration.dots = 2;
    s.headers[1].free_time = true;
    s.headers[0].triplet_feel = TripletFeel::Scottish8th;
    s.tracks[0].measures[0].voices.push(Voice::default());
    s.tracks[0].measures[0].voices.push(Voice { beats: vec![Beat::rest(Duration::WHOLE)] });
    let out = export_gp5(&s);
    let all = out.warnings.join("\n");
    for what in ["fades out", "double dots", "free-time", "triplet feels", "third and fourth voices"] {
        assert!(all.contains(what), "missing `{what}` in {all}");
    }
    // Still a readable file.
    assert!(import(&out.bytes).is_ok());
}

#[test]
fn seven_string_tracks_fit_and_twelve_string_tracks_are_cut() {
    let mut s = Score::new();
    s.tracks[0] = guitario_score::Track::new("7", guitario_theory::Tuning::seven_string(), &s.headers);
    s.tracks[0].measures[0].voices[0].beats = vec![Beat { rest: false, notes: vec![Note::new(6, 3)], ..Beat::rest(Duration::WHOLE) }];
    let back = import(&export_gp5(&s).bytes).unwrap().score;
    assert_eq!(back.tracks[0].tuning.strings, vec![64, 59, 55, 50, 45, 40, 35]);
    assert_eq!(back.tracks[0].measures[0].voices[0].beats[0].notes[0].string, 6);

    let tuning = guitario_theory::Tuning::new("12", vec![64, 64, 59, 59, 55, 55, 50, 50, 45, 45, 40, 40]).unwrap();
    s.tracks[0] = guitario_score::Track::new("12", tuning, &s.headers);
    s.tracks[0].measures[0].voices[0].beats = vec![Beat { rest: false, notes: vec![Note::new(0, 1), Note::new(11, 3)], ..Beat::rest(Duration::WHOLE) }];
    let out = export_gp5(&s);
    assert!(out.warnings.iter().any(|w| w.contains("seventh")), "{:?}", out.warnings);
    let back = import(&out.bytes).unwrap().score;
    assert_eq!(back.tracks[0].string_count(), 7);
    assert_eq!(back.tracks[0].measures[0].voices[0].beats[0].notes.len(), 1);
}

#[test]
fn every_truncation_is_refused_without_a_panic() {
    let bytes = export_gp5(&riff()).bytes;
    for len in 0..bytes.len() {
        match import(bytes.get(..len).unwrap()) {
            Err(_) => {}
            // Cutting only the final measure's padding still reads.
            Ok(_) => assert!(len + 2 >= bytes.len(), "a file cut at {len} of {} bytes imported", bytes.len()),
        }
    }
}

#[test]
fn corrupted_bytes_never_panic() {
    let bytes = export_gp5(&riff()).bytes;
    // Every byte set to a few hostile values in turn: errors are fine, panics are not.
    for i in 0..bytes.len() {
        for v in [0x00, 0x7f, 0x80, 0xff] {
            let mut b = bytes.clone();
            b[i] = v;
            if let Ok(imp) = import(&b) {
                assert!(imp.score.validate().is_ok());
            }
        }
    }
}

#[test]
fn old_and_foreign_banners_are_refused_clearly() {
    let mut b = vec![24u8];
    b.extend(b"FICHIER GUITARE PRO v1.04");
    b.resize(31, 0);
    assert!(matches!(import(&b), Err(GpError::NotGuitarPro | GpError::Unsupported(_))));
    let mut b = vec![24u8];
    b.extend(b"FICHIER GUITAR PRO v6.00");
    b.resize(40, 0);
    assert!(matches!(import(&b), Err(GpError::Unsupported(_))));
    assert!(matches!(import(b""), Err(GpError::NotGuitarPro)));
}
