use guitario_engine::{Session, registry};
use serde_json::{Value, json};

#[allow(clippy::unwrap_used)]
fn run(s: &mut Session, id: &str, p: Value) {
    s.run(id, &p).map_err(|e| format!("{id}: {e}")).unwrap();
}

#[test]
fn enter_a_riff() {
    let mut s = Session::new();
    run(&mut s, "beat.setDuration", json!({"value": 4}));
    for (string, fret) in [(5, 0), (5, 3), (4, 2), (4, 3)] {
        run(&mut s, "cursor.set", json!({"string": string}));
        run(&mut s, "note.setFret", json!({"fret": fret}));
        s.run("cursor.right", &Value::Null).unwrap();
    }
    let sc = s.score();
    let beats = &sc.tracks[0].measures[0].voices[0].beats;
    assert_eq!(beats.len(), 4);
    assert_eq!(beats[1].notes[0].fret, 3);
    assert!(sc.validate().is_ok());
    // Cursor crossed into a new measure.
    assert_eq!(s.cursor().measure, 1);
}

#[test]
fn undo_redo_round_trip() {
    let mut s = Session::new();
    let before = (**s.score()).clone();
    run(&mut s, "note.setFret", json!({"fret": 7}));
    assert_ne!(**s.score(), before);
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(**s.score(), before);
    run(&mut s, "edit.redo", Value::Null);
    assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].notes[0].fret, 7);
}

#[test]
fn failed_command_changes_nothing() {
    let mut s = Session::new();
    let before = (**s.score()).clone();
    assert!(s.run("note.setFret", &json!({"fret": 99})).is_err());
    assert!(s.run("beat.setDuration", &json!({"value": 1, "dots": 2})).is_err()); // 1.75 whole > measure
    assert!(s.run("measure.delete", &Value::Null).is_err()); // last measure
    assert_eq!(**s.score(), before);
    assert!(!s.can_undo());
}

#[test]
fn measure_and_track_editing() {
    let mut s = Session::new();
    run(&mut s, "measure.append", Value::Null);
    run(&mut s, "track.add", json!({"name": "Bass", "tuning": "bass"}));
    assert_eq!(s.score().tracks[1].string_count(), 4);
    assert_eq!(s.score().tracks[1].measures.len(), 2);
    run(&mut s, "cursor.set", json!({"track": 0, "measure": 1}));
    run(&mut s, "measure.setTimeSignature", json!({"numerator": 3, "denominator": 4}));
    assert_eq!(s.score().headers[1].capacity(), 3 * guitario_theory::TICKS_PER_QUARTER);
    run(&mut s, "measure.delete", Value::Null);
    assert_eq!(s.score().headers.len(), 1);
    assert!(s.score().validate().is_ok());
}

#[test]
fn effects_toggle() {
    let mut s = Session::new();
    assert!(s.run("note.toggleEffect", &json!({"effect": "vibrato"})).is_err()); // no note yet
    run(&mut s, "note.setFret", json!({"fret": 5}));
    run(&mut s, "note.toggleEffect", json!({"effect": "palmMute"}));
    run(&mut s, "note.setSlide", json!({"slide": "intoFromBelow"}));
    let n = s.score().tracks[0].measures[0].voices[0].beats[0].notes[0];
    assert!(n.effects.palm_mute);
    assert!(n.effects.slide.is_some());
}

#[test]
fn export_and_save_round_trip() {
    let dir = std::env::temp_dir().join(format!("gc-eng-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut s = Session::new();
    run(&mut s, "note.setFret", json!({"fret": 3}));
    let gt = dir.join("a.gt");
    run(&mut s, "file.save", json!({"path": gt.to_str().unwrap()}));
    assert!(!s.is_dirty());
    run(&mut s, "file.exportMidi", json!({"path": dir.join("a.mid").to_str().unwrap()}));
    let r = s.run("file.exportWav", &json!({"path": dir.join("a.wav").to_str().unwrap(), "sampleRate": 22050})).unwrap();
    assert!(r["seconds"].as_f64().unwrap() >= 2.0);
    assert!(std::fs::read(dir.join("a.wav")).unwrap().starts_with(b"RIFF"));
    // A sound bank folder without a bank.json is a parameter error, not a panic or a silent fallback.
    let bad = s.run("file.exportWav", &json!({"path": dir.join("b.wav").to_str().unwrap(), "sounds": dir.to_str().unwrap()}));
    assert!(bad.is_err());
    let mut t = Session::new();
    run(&mut t, "file.open", json!({"path": gt.to_str().unwrap()}));
    assert_eq!(**t.score(), **s.score());
    let r = t.run("file.exportAscii", &Value::Null).unwrap();
    assert!(r["text"].as_str().unwrap().contains("3"));
    std::fs::remove_dir_all(dir).unwrap();
}

/// Rule 9: no command panics for any params, and the score stays valid afterwards.
#[test]
fn panic_hunt() {
    let evil = [
        Value::Null,
        json!({}),
        json!([]),
        json!("x"),
        json!({"fret": -1, "value": -5, "index": u64::MAX, "bpm": 1e300, "numerator": 0, "denominator": 0}),
        json!({"fret": 1e19, "value": "nope", "dots": 255, "tuplet": [0, 0], "string": 999999, "measure": u64::MAX, "track": u64::MAX}),
        json!({"text": "é".repeat(50), "name": "\u{0}", "path": "", "preset": "x", "strings": [300, 2], "effect": "zzz", "key": 99, "kind": 5, "pan": 1000}),
        json!({"path": "/nonexistent/dir/file", "volume": 128, "program": 200, "close": 300}),
    ];
    for spec in registry() {
        if spec.id == "file.new" {
            continue;
        }
        for p in &evil {
            let mut s = Session::new();
            let _ = s.run("note.setFret", &json!({"fret": 1}));
            let _ = s.run(spec.id, p);
            assert!(s.score().validate().is_ok(), "{} left an invalid score for {p}", spec.id);
        }
    }
}

#[test]
fn registry_ids_are_unique() {
    let mut ids: Vec<_> = registry().iter().map(|c| c.id).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n);
}

#[test]
fn guitar_pro_style_toggles() {
    let mut s = Session::new();
    run(&mut s, "beat.setDuration", json!({"value": 4}));
    run(&mut s, "beat.toggleDot", Value::Null);
    assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].duration.ticks(), guitario_theory::TICKS_PER_QUARTER * 3 / 2);
    run(&mut s, "beat.shorten", Value::Null);
    run(&mut s, "beat.toggleTriplet", Value::Null);
    assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].duration.ticks(), guitario_theory::TICKS_PER_QUARTER / 2);
    run(&mut s, "note.setFret", json!({"fret": 5}));
    run(&mut s, "note.toggleDead", Value::Null);
    run(&mut s, "note.toggleTie", Value::Null);
    assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].notes[0].kind, guitario_score::NoteKind::Tie);
    run(&mut s, "beat.toggleRest", Value::Null);
    assert!(s.score().tracks[0].measures[0].voices[0].beats[0].rest);
    // Lengthening past the bar fails and changes nothing.
    run(&mut s, "beat.setDuration", json!({"value": 1}));
    assert!(s.run("beat.lengthen", &Value::Null).is_ok());
    assert!(s.run("beat.toggleDot", &Value::Null).is_err());
}

#[allow(clippy::indexing_slicing)]
fn frets(s: &Session, m: usize) -> Vec<Option<u8>> {
    s.score().tracks[s.cursor().track].measures[m].voices[0].beats.iter().map(|b| b.notes.first().map(|n| n.fret)).collect()
}

/// Enters quarter notes on the high E string.
#[allow(clippy::unwrap_used)]
fn quarters(s: &mut Session, fr: &[u8]) {
    run(s, "cursor.set", json!({"measure": 0, "beat": 0, "string": 0}));
    for f in fr {
        run(s, "beat.setDuration", json!({"value": 4}));
        run(s, "note.setFret", json!({"fret": f}));
        run(s, "cursor.right", Value::Null);
    }
}

#[test]
fn shift_arrows_select_and_copy_paste_overwrites() {
    let mut s = Session::new();
    quarters(&mut s, &[1, 2, 3, 4]);
    run(&mut s, "cursor.set", json!({"measure": 0, "beat": 0}));
    run(&mut s, "select.extendRight", Value::Null);
    let sel = s.selection().unwrap();
    assert_eq!(sel.ordered(), ((0, 0), (0, 1)));
    run(&mut s, "edit.copy", Value::Null);
    run(&mut s, "cursor.set", json!({"measure": 0, "beat": 2}));
    assert!(s.selection().is_none(), "moving the cursor deselects");
    run(&mut s, "edit.paste", Value::Null);
    assert_eq!(frets(&s, 0), vec![Some(1), Some(2), Some(1), Some(2)]);
    assert!(s.score().validate().is_ok());
    // One undo step.
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(frets(&s, 0), vec![Some(1), Some(2), Some(3), Some(4)]);
}

#[test]
fn paste_flows_into_new_measures() {
    let mut s = Session::new();
    quarters(&mut s, &[1, 2, 3, 4, 5, 6, 7, 8]);
    run(&mut s, "select.all", Value::Null);
    run(&mut s, "edit.copy", Value::Null);
    let measures = s.score().headers.len();
    run(&mut s, "cursor.set", json!({"measure": measures - 1, "beat": 0}));
    run(&mut s, "edit.paste", Value::Null);
    assert!(s.score().headers.len() > measures);
    assert!(s.score().validate().is_ok());
    assert_eq!(frets(&s, measures)[..4], [Some(5), Some(6), Some(7), Some(8)]);
}

#[test]
fn selection_wide_effects_cut_and_transpose() {
    let mut s = Session::new();
    quarters(&mut s, &[5, 7, 9, 10]);
    run(&mut s, "select.set", json!({"fromMeasure": 0, "fromBeat": 1, "toMeasure": 0, "toBeat": 3}));
    run(&mut s, "note.toggleEffect", json!({"effect": "palmMute"}));
    let beats = &s.score().tracks[0].measures[0].voices[0].beats;
    assert_eq!(beats.iter().map(|b| b.notes[0].effects.palm_mute).collect::<Vec<_>>(), vec![false, true, true, true]);
    run(&mut s, "edit.transposeUp", Value::Null);
    assert_eq!(frets(&s, 0), vec![Some(5), Some(8), Some(10), Some(11)]);
    run(&mut s, "edit.cut", Value::Null);
    assert_eq!(frets(&s, 0), vec![Some(5), None, None, None]);
    run(&mut s, "cursor.set", json!({"measure": 0, "beat": 0}));
    run(&mut s, "edit.paste", Value::Null);
    assert_eq!(frets(&s, 0)[..3], [Some(8), Some(10), Some(11)]);
    // Out of range transposition fails without changing anything.
    run(&mut s, "select.all", Value::Null);
    let before = (**s.score()).clone();
    for _ in 0..40 {
        let _ = s.run("edit.transposeUp", &Value::Null);
    }
    assert!(s.score().tracks[0].measures[0].voices[0].beats.iter().flat_map(|b| &b.notes).all(|n| n.fret <= 36));
    assert_ne!(**s.score(), before);
}

#[test]
fn drum_tracks_take_kit_pieces_on_their_lanes() {
    let mut s = Session::new();
    run(&mut s, "track.addDrums", json!({}));
    let t = s.cursor().track;
    assert!(s.score().tracks[t].is_drums());
    // A rock beat: kick + hi-hat, then snare + hi-hat, all in one beat each.
    run(&mut s, "beat.setDuration", json!({"value": 8}));
    for d in [json!("kick"), json!("hh"), json!(49)] {
        run(&mut s, "note.setDrum", json!({"drum": d}));
    }
    let beat = &s.score().tracks[t].measures[0].voices[0].beats[0];
    let mut hits: Vec<(u8, u8)> = beat.notes.iter().map(|n| (n.string, n.fret)).collect();
    hits.sort_unstable();
    assert_eq!(hits, vec![(0, 49), (2, 42), (7, 36)]);
    // Toggling the same hit removes it; a different piece on the same lane replaces it.
    run(&mut s, "note.setDrum", json!({"drum": "crash"}));
    run(&mut s, "note.setDrum", json!({"drum": "oh"}));
    let beat = &s.score().tracks[t].measures[0].voices[0].beats[0];
    assert_eq!(beat.notes.iter().map(|n| n.fret).collect::<Vec<_>>(), vec![46, 36]);
    // Drum numbers go past fret 36; guitar-only commands are refused.
    run(&mut s, "cursor.set", json!({"string": 4}));
    run(&mut s, "note.setFret", json!({"fret": 81}));
    assert!(s.run("track.setTuning", &json!({"preset": "drop d"})).is_err());
    assert!(s.run("track.setCapo", &json!({"fret": 2})).is_err());
    assert!(s.run("edit.transposeUp", &Value::Null).is_err());
    assert!(s.run("note.setDrum", &json!({"drum": "cowbell-ish"})).is_err());
    assert!(s.score().validate().is_ok());
    // Not on a guitar track.
    run(&mut s, "track.select", json!({"index": 0}));
    assert!(s.run("note.setDrum", &json!({"drum": "kick"})).is_err());
}

#[test]
fn transpose_track_moves_frets_and_wraps_to_another_string() {
    let mut s = Session::new();
    run(&mut s, "cursor.set", json!({"string": 5}));
    run(&mut s, "note.setFret", json!({"fret": 3}));
    run(&mut s, "track.transpose", json!({"semitones": 2}));
    let note = s.score().tracks[0].measures[0].voices[0].beats[0].notes[0];
    assert_eq!((note.string, note.fret), (5, 5));
    // Down to the open low E, then one more semitone: nothing fits, so nothing changes.
    run(&mut s, "track.transpose", json!({"semitones": -5}));
    let before = (**s.score()).clone();
    assert!(s.run("track.transpose", &json!({"semitones": -1})).is_err());
    assert_eq!(**s.score(), before);
    assert!(s.score().validate().is_ok());
    assert!(s.run("track.transpose", &json!({"semitones": 99})).is_err());
}

#[test]
fn transpose_track_relocates_a_note_that_runs_off_a_string() {
    let mut s = Session::new();
    run(&mut s, "cursor.set", json!({"string": 1}));
    run(&mut s, "note.setFret", json!({"fret": 0}));
    // B3 down a semitone is fret -1 on its string; the next string up can play it at fret 3.
    run(&mut s, "track.transpose", json!({"semitones": -1}));
    let note = s.score().tracks[0].measures[0].voices[0].beats[0].notes[0];
    assert_eq!((note.string, note.fret), (2, 3));
    assert!(s.score().validate().is_ok());
}

#[test]
fn transpose_several_tracks_at_once_all_or_nothing() {
    let mut s = Session::new();
    run(&mut s, "cursor.set", json!({"string": 5}));
    run(&mut s, "note.setFret", json!({"fret": 3}));
    run(&mut s, "track.add", json!({"name": "Bass", "tuning": "bass"}));
    run(&mut s, "cursor.set", json!({"track": 1, "string": 3}));
    run(&mut s, "note.setFret", json!({"fret": 0}));
    run(&mut s, "track.addDrums", json!({}));
    let fret = |s: &Session, t: usize| s.score().tracks[t].measures[0].voices[0].beats[0].notes[0].fret;
    // "all" moves every pitched track and leaves the drums alone.
    let drums = s.score().tracks[2].clone();
    run(&mut s, "track.transpose", json!({"semitones": 2, "tracks": "all"}));
    assert_eq!((fret(&s, 0), fret(&s, 1)), (5, 2));
    assert_eq!(s.score().tracks[2], drums);
    // The open low E of the bass cannot go down: neither track moves.
    let before = (**s.score()).clone();
    run(&mut s, "track.transpose", json!({"semitones": -2, "tracks": [0, 1]}));
    assert!(s.run("track.transpose", &json!({"semitones": -1, "tracks": [0, 1]})).is_err());
    run(&mut s, "track.transpose", json!({"semitones": 2, "tracks": [0, 1]}));
    assert_eq!(**s.score(), before);
    for bad in [json!({"semitones": 1, "tracks": [9]}), json!({"semitones": 1, "tracks": "some"}), json!({"semitones": 1, "tracks": [2]})] {
        assert!(s.run("track.transpose", &bad).is_err(), "{bad}");
    }
    // Edit › Transpose… moves the note at the cursor (or the selection) by any interval.
    run(&mut s, "cursor.set", json!({"track": 0, "measure": 0, "beat": 0, "string": 5}));
    run(&mut s, "edit.transpose", json!({"semitones": 7}));
    assert_eq!(fret(&s, 0), 12);
    assert!(s.run("edit.transpose", &json!({"semitones": 30})).is_err());
}

#[test]
fn the_mixer_sets_any_track_by_index() {
    let mut s = Session::new();
    run(&mut s, "track.add", json!({"name": "Bass", "tuning": "bass"}));
    run(&mut s, "cursor.set", json!({"track": 0}));
    run(&mut s, "track.setMix", json!({"track": 1, "volume": 40, "pan": -20, "muted": true}));
    run(&mut s, "track.setProgram", json!({"track": 1, "program": 34}));
    let (a, b) = (&s.score().tracks[0], &s.score().tracks[1]);
    assert_eq!((b.volume, b.pan, b.muted, b.program), (40, -20, true, 34));
    assert!(!a.muted && a.volume != 40, "the cursor's track is untouched");
    // Without `track`, the cursor's.
    run(&mut s, "track.setMix", json!({"solo": true}));
    assert!(s.score().tracks[0].solo);
    for bad in [json!({"track": 2, "volume": 1}), json!({"track": -1, "volume": 1}), json!({"track": 0, "volume": 128})] {
        assert!(s.run("track.setMix", &bad).is_err(), "{bad}");
    }
    assert!(s.run("track.setProgram", &json!({"track": 5, "program": 1})).is_err());
}

#[test]
fn tempo_points_glide_or_jump_and_are_set_on_any_bar() {
    let mut s = Session::new();
    for _ in 0..3 {
        run(&mut s, "measure.append", Value::Null);
    }
    run(&mut s, "measure.setTempo", json!({"bpm": 160, "linear": true, "measure": 3}));
    let h = &s.score().headers[3];
    assert_eq!((h.tempo, h.tempo_linear), (Some(160), true));
    assert_eq!(s.cursor().measure, 0, "the cursor stays");
    // Bars 0-2 glide from the song tempo to 160.
    let c = s.score().tempo_curves();
    assert!(c[0].0 < c[2].1 && (c[2].1 - 160.0).abs() < 1e-9, "{c:?}");
    // A plain change jumps; clearing drops the glide too.
    run(&mut s, "measure.setTempo", json!({"bpm": 150, "measure": 3}));
    assert!(!s.score().headers[3].tempo_linear);
    run(&mut s, "measure.setTempo", json!({"bpm": 150, "linear": true, "measure": 3}));
    run(&mut s, "measure.setTempo", json!({"bpm": null, "measure": 3}));
    assert_eq!((s.score().headers[3].tempo, s.score().headers[3].tempo_linear), (None, false));
    for bad in [json!({"bpm": 120, "measure": 9}), json!({"bpm": 120, "linear": "yes"}), json!({"bpm": 10})] {
        assert!(s.run("measure.setTempo", &bad).is_err(), "{bad}");
    }
}

#[test]
fn audio_exports_as_wav_or_flac_by_name_or_format() {
    let mut s = Session::new();
    run(&mut s, "note.setFret", json!({"fret": 5}));
    let dir = std::env::temp_dir().join(format!("gt-audio-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = |n: &str| dir.join(n).display().to_string();
    let flac = s.run("file.exportAudio", &json!({"path": path("a.flac"), "sampleRate": 22050})).unwrap();
    let wav = s.run("file.exportAudio", &json!({"path": path("a.wav"), "sampleRate": 22050})).unwrap();
    assert!(std::fs::read(path("a.flac")).unwrap().starts_with(b"fLaC"));
    assert!(std::fs::read(path("a.wav")).unwrap().starts_with(b"RIFF"));
    assert!(flac["bytes"].as_u64() < wav["bytes"].as_u64());
    // `format` wins over the name; the old command still writes WAV.
    s.run("file.exportAudio", &json!({"path": path("b.audio"), "format": "flac", "sampleRate": 8000})).unwrap();
    assert!(std::fs::read(path("b.audio")).unwrap().starts_with(b"fLaC"));
    s.run("file.exportWav", &json!({"path": path("c.flac"), "sampleRate": 8000})).unwrap();
    assert!(std::fs::read(path("c.flac")).unwrap().starts_with(b"RIFF"));
    // MP3 and OGG say what to use instead.
    let e = s.run("file.exportAudio", &json!({"path": path("d.mp3")})).unwrap_err().to_string();
    assert!(e.contains("FLAC"), "{e}");
    assert!(s.run("file.exportAudio", &json!({"path": path("e.wav"), "format": 3})).is_ok(), "an odd format falls back to the name");
    std::fs::remove_dir_all(dir).ok();
}
