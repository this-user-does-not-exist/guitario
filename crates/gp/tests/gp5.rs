//! Guitar Pro 5 export read back by the Guitar Pro 3-5 reader: every demo survives the trip, and
//! damaged files are refused without a panic.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

mod common;

use common::{assert_same, demos, riff};
use guitario_gp::{GpError, export_gp5, import};
use guitario_score::*;
use guitario_theory::{Duration, TICKS_PER_QUARTER};

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
fn a_wide_vibrato_replaces_the_plain_one() {
    // Guitar Pro 4 files flag a note both as vibrato and as wide vibrato (Vibrato.gp4 in the real
    // corpus); the model keeps one of them so a Guitar Pro 7/8 file, which has one element, agrees.
    let mut s = riff();
    let n = &mut s.tracks[0].measures[0].voices[0].beats[2].notes[0];
    n.effects.vibrato = true;
    n.effects.wide_vibrato = true;
    let (back, _, _) = round_trip(&s);
    let n = &back.tracks[0].measures[0].voices[0].beats[2].notes[0];
    assert!(n.effects.wide_vibrato && !n.effects.vibrato);
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
