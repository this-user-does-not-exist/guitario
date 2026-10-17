//! Imports the hand-built Guitar Pro 7 file in `demos/guitar-pro/` (made by
//! `cargo run -p guitario-gp --example fixture`).
#![allow(clippy::expect_used, clippy::indexing_slicing)]

use guitario_score::{Dynamic, Harmonic, NoteKind, Slide};
use guitario_theory::{Duration, TimeSignature};

fn load() -> guitario_gp::Import {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/guitar-pro/import-test.gp");
    guitario_gp::import_file(&path).expect("import-test.gp should import; regenerate it with the fixture example")
}

#[test]
fn song_information_tempo_and_tuning() {
    let imp = load();
    let s = &imp.score;
    assert_eq!((s.meta.title.as_str(), s.meta.artist.as_str(), s.meta.author.as_str()), ("GP7 Import Test", "Guitario", "Guitario demo"));
    assert_eq!(s.tempo, 160);
    let t = &s.tracks[0];
    assert_eq!((t.name.as_str(), t.program), ("Rhythm Guitar", 30));
    // GP lists drop D lowest string first; we store highest first.
    assert_eq!(t.tuning.strings, vec![64, 59, 55, 50, 45, 38]);
    assert!(imp.warnings.is_empty(), "{:?}", imp.warnings);
}

#[test]
fn bars_repeats_and_meters() {
    let s = load().score;
    assert_eq!(s.headers.len(), 3);
    assert!(s.headers[0].repeat_open);
    assert_eq!((s.headers[0].section_letter.as_deref(), s.headers[0].marker.as_deref()), (Some("A"), Some("Riff")));
    // A repeat end with count 3 means three plays, i.e. two extra passes.
    assert_eq!(s.headers[1].repeat_close, 2);
    assert_eq!(s.headers[2].time_signature, TimeSignature { numerator: 3, denominator: 4 });
    // Bars 1-2 play three times, then bar 3.
    assert_eq!(guitario_score::playback_order(&s), vec![0, 1, 0, 1, 0, 1, 2]);
}

#[test]
fn shared_beats_notes_and_effects() {
    let s = load().score;
    let t = &s.tracks[0];
    let bar1 = &t.measures[0].voices[0].beats;
    assert_eq!(bar1.len(), 8, "shared gallop beats are each used where referenced");
    assert_eq!(bar1.iter().map(|b| b.duration.ticks()).sum::<u32>(), 4 * guitario_theory::TICKS_PER_QUARTER);
    assert_eq!(bar1[1].duration, Duration::new(guitario_theory::NoteValue::Sixteenth));
    // GP string 0 (lowest) becomes our string 5.
    let low = &bar1[0].notes[0];
    assert_eq!((low.string, low.fret, low.effects.palm_mute), (5, 0, true));
    assert_eq!(bar1[0].dynamic, Dynamic::F);
    // The chord has both notes, sorted high string first, and accents.
    let chord = &bar1[6];
    assert_eq!(chord.notes.iter().map(|n| (n.string, n.fret)).collect::<Vec<_>>(), vec![(4, 3), (5, 3)]);
    assert!(chord.notes.iter().all(|n| n.effects.accent));
    assert_eq!(bar1[6].dynamic, Dynamic::Ff);
    assert!(bar1[7].rest);

    let bar2 = &t.measures[1].voices[0].beats;
    let a = &bar2[0].notes[0];
    assert!(a.effects.let_ring && a.effects.vibrato);
    assert_eq!(a.effects.slide, Some(Slide::Shift));
    let b = &bar2[1].notes[0];
    assert_eq!((b.fret, b.kind, b.effects.bend), (7, NoteKind::Tie, Some(4)));

    // The grace beat takes no time, so bar 3 holds exactly three quarters.
    let bar3 = &t.measures[2].voices[0].beats;
    assert_eq!(bar3.len(), 3);
    let h = &bar3[0].notes[0];
    assert_eq!((h.string, h.fret, h.effects.harmonic), (2, 12, Some(Harmonic::Natural)));
}

#[test]
fn imported_score_plays() {
    let s = load().score;
    let notes = guitario_score::timeline(&s);
    // The tie merges two half notes into one whole-note attack per pass.
    assert!(notes.iter().any(|n| n.string == 5 && n.ticks == 4 * guitario_theory::TICKS_PER_QUARTER));
    assert!(s.validate().is_ok());
}

#[test]
fn drum_tracks_import_onto_kit_lanes() {
    let imp = load();
    let s = &imp.score;
    assert_eq!(s.tracks.len(), 2);
    let d = &s.tracks[1];
    assert!(d.is_drums(), "the drumKit track imports as a drum track");
    assert_eq!(d.name, "Drums");
    let hits = |m: usize, b: usize| {
        let mut v: Vec<(u8, u8)> = d.measures[m].voices[0].beats[b].notes.iter().map(|n| (n.string, n.fret)).collect();
        v.sort_unstable();
        v
    };
    // Articulation 1 is the kick (36), articulation 2 the closed hi-hat (42).
    assert_eq!(hits(0, 0), vec![(2, 42), (7, 36)]);
    // An explicit Midi number wins over the articulation index.
    assert_eq!(hits(0, 1), vec![(2, 42), (4, 38)]);
    assert_eq!(d.measures[0].voices[0].beats.len(), 8);
    assert_eq!(hits(1, 0), vec![(0, 49), (7, 36)]);
    assert!(d.measures[1].voices[0].beats[1].rest);
    // A number outside the articulation list still plays: ride (51) on the ride lane.
    assert_eq!(hits(2, 0), vec![(1, 51)]);
    assert!(s.validate().is_ok());
    let midi = guitario_midi_free_check(s);
    assert!(midi > 0);
}

/// Drum hits reach the playback timeline as their own MIDI numbers.
fn guitario_midi_free_check(s: &guitario_score::Score) -> usize {
    guitario_score::timeline(s).iter().filter(|n| n.track == 1 && n.pitch.is_some_and(|p| p.0 == 36)).count()
}
