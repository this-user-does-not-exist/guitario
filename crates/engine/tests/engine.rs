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

#[test]
fn song_information_holds_every_guitar_pro_field() {
    let mut s = Session::new();
    let fields = ["title", "artist", "subtitle", "album", "words", "music", "copyright", "author", "notes", "instructions"];
    let mut p = serde_json::Map::new();
    for f in fields {
        p.insert(f.into(), json!(format!("{f} value")));
    }
    run(&mut s, "score.setMeta", Value::Object(p));
    let m = &s.score().meta;
    let got = [&m.title, &m.artist, &m.subtitle, &m.album, &m.words, &m.music, &m.copyright, &m.author, &m.notes, &m.instructions];
    for (f, v) in fields.iter().zip(got) {
        assert_eq!(v, &format!("{f} value"));
    }
    // Only the fields given change.
    run(&mut s, "score.setMeta", json!({"subtitle": ""}));
    assert_eq!((s.score().meta.subtitle.as_str(), s.score().meta.title.as_str()), ("", "title value"));
    assert!(s.run("score.setMeta", &json!({"words": 3})).is_err());
}

#[test]
fn track_information_sets_name_short_name_and_colour() {
    let mut s = Session::new();
    run(&mut s, "track.setInfo", json!({"name": " Rhythm Guitar ", "shortName": "r.gt.", "color": "#1a2B3c"}));
    let t = &s.score().tracks[0];
    assert_eq!((t.name.as_str(), t.short_name.as_str(), t.color), ("Rhythm Guitar", "r.gt.", Some([0x1a, 0x2b, 0x3c])));
    run(&mut s, "track.setInfo", json!({"color": null}));
    assert_eq!(s.score().tracks[0].color, None);
    assert_eq!(s.score().tracks[0].color_at(1), guitario_score::TRACK_COLORS[1]);
    for bad in [
        json!({"color": "red"}),
        json!({"color": "#12345"}),
        json!({"color": "#ééé"}),
        json!({"color": 7}),
        json!({"name": "  "}),
        json!({"track": 9, "name": "x"}),
    ] {
        assert!(s.run("track.setInfo", &bad).is_err(), "{bad}");
    }
    assert_eq!(s.score().tracks[0].name, "Rhythm Guitar", "a refused change leaves the track as it was");
}

#[test]
fn notations_are_chosen_per_track_and_one_always_stays() {
    let mut s = Session::new();
    run(&mut s, "track.add", json!({"name": "Bass", "tuning": "bass"}));
    run(&mut s, "track.setNotation", json!({"track": 0, "tab": false}));
    let n = s.score().tracks[0].notation;
    assert_eq!((n.standard, n.tab), (true, false));
    assert_eq!(s.score().tracks[1].notation, guitario_score::Notations::default(), "the other track keeps its own");
    assert!(s.run("track.setNotation", &json!({"track": 0, "standard": false})).is_err(), "nothing left to show");
    run(&mut s, "track.setNotation", json!({"track": 0, "standard": false, "slash": true}));
    assert_eq!(s.score().tracks[0].notation.bits(), 4);
    assert!(s.run("track.setNotation", &json!({"standard": "yes"})).is_err());
}

#[test]
fn interpretation_is_set_per_track_within_range() {
    let mut s = Session::new();
    run(
        &mut s,
        "track.setInterpretation",
        json!({"style": "fingers", "palmMute": 30, "accentuation": 20, "autoLetRing": true, "autoBrush": true, "stringed": false}),
    );
    let i = s.score().tracks[0].interpretation;
    assert_eq!(i.style, guitario_score::PlayingStyle::Fingers);
    assert_eq!((i.palm_mute, i.accentuation, i.auto_let_ring, i.auto_brush, i.stringed), (30, 20, true, true, false));
    for bad in [json!({"palmMute": 101}), json!({"accentuation": -1}), json!({"style": "kazoo"}), json!({"style": 3}), json!({"autoBrush": "on"})] {
        assert!(s.run("track.setInterpretation", &bad).is_err(), "{bad}");
    }
    run(&mut s, "track.addDrums", json!({}));
    assert!(s.run("track.setInterpretation", &json!({"autoBrush": true})).is_err(), "drums have none");
}

#[test]
fn the_audio_engine_switches_for_one_track_or_all() {
    use guitario_score::SoundEngine;
    let mut s = Session::new();
    run(&mut s, "track.addDrums", json!({}));
    run(&mut s, "track.setEngine", json!({"engine": "midi", "tracks": "all"}));
    assert!(s.score().tracks.iter().all(|t| t.engine == SoundEngine::Midi), "drums too");
    run(&mut s, "track.setEngine", json!({"engine": "rse", "tracks": [0]}));
    assert_eq!((s.score().tracks[0].engine, s.score().tracks[1].engine), (SoundEngine::Rse, SoundEngine::Midi));
    for bad in [json!({"engine": "fm"}), json!({}), json!({"engine": "rse", "tracks": [5]}), json!({"engine": "rse", "tracks": "some"})] {
        assert!(s.run("track.setEngine", &bad).is_err(), "{bad}");
    }
}

#[test]
fn a_track_is_created_set_up_in_one_step_or_replaces_one() {
    let mut s = Session::new();
    run(
        &mut s,
        "track.create",
        json!({"name": "Fiddle", "tuning": "violin", "program": 40, "standard": true, "tab": false, "shortName": "vln.", "color": "#336699"}),
    );
    let t = &s.score().tracks[1];
    assert_eq!((t.name.as_str(), t.short_name.as_str(), t.program, t.string_count()), ("Fiddle", "vln.", 40, 4));
    assert_eq!((t.notation.standard, t.notation.tab, t.color), (true, false, Some([0x33, 0x66, 0x99])));
    assert_eq!(s.cursor().track, 1);
    // One undo removes it all.
    assert!(s.run("edit.undo", &json!(null)).is_ok());
    assert_eq!(s.score().tracks.len(), 1);
    // Replacing the first track, as the new-file assistant does.
    run(&mut s, "track.create", json!({"kind": "drums", "name": "Kit", "replace": 0}));
    assert!(s.score().tracks.len() == 1 && s.score().tracks[0].is_drums());
    for bad in [
        json!({"name": ""}),
        json!({"name": "x", "kind": "horn"}),
        json!({"name": "x", "tuning": "nope"}),
        json!({"name": "x", "strings": []}),
        json!({"name": "x", "replace": 4}),
        json!({"name": "x", "standard": false, "tab": false}),
        json!({"name": "x", "color": "blue"}),
        json!({"name": "x", "players": 17}),
        json!({"name": "x", "pan": 64}),
    ] {
        assert!(s.run("track.create", &bad).is_err(), "{bad}");
    }
    assert_eq!(s.score().tracks.len(), 1, "refusals leave the score as it was");
}

#[test]
fn a_string_section_is_created_resized_and_kept() {
    let mut s = Session::new();
    run(&mut s, "track.create", json!({"name": "Violins I", "tuning": "violin", "program": 40, "players": 16, "pan": -40}));
    let t = &s.score().tracks[1];
    assert_eq!((t.program, t.players, t.pan), (40, 16, -40));
    let info = s.run("score.info", &json!(null)).unwrap();
    assert_eq!(info["tracks"][1]["players"], 16);
    assert_eq!(info["tracks"][0]["players"], 1, "a guitar has one player");
    // Smaller, then a soloist: each one undo step.
    assert_eq!(s.run("track.setPlayers", &json!({"players": 8})).unwrap()["players"], 8);
    run(&mut s, "track.setProgram", json!({"program": 42, "players": 0}));
    assert_eq!((s.score().tracks[1].program, s.score().tracks[1].players), (42, 0));
    run(&mut s, "track.setProgram", json!({"program": 41}));
    assert_eq!(s.score().tracks[1].players, 0, "a program alone keeps the players");
    run(&mut s, "edit.undo", json!(null));
    run(&mut s, "edit.undo", json!(null));
    assert_eq!((s.score().tracks[1].program, s.score().tracks[1].players), (40, 8));
    for bad in [json!({"players": 17}), json!({"players": -1}), json!({"players": "many"}), json!({})] {
        assert!(s.run("track.setPlayers", &bad).is_err(), "{bad}");
    }
    assert!(s.run("track.setProgram", &json!({"program": 40, "players": 40})).is_err());
    run(&mut s, "track.addDrums", json!({}));
    assert!(s.run("track.setPlayers", &json!({"players": 4})).is_err(), "one drummer");
    // Saved and opened again.
    let bytes = guitario_format::to_bytes(s.score()).unwrap();
    let back = guitario_format::from_bytes(&bytes).unwrap();
    assert_eq!(back.tracks[1].players, 8);
    assert!(back.validate().is_ok());
}
