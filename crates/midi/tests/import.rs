//! MIDI import: our own exports come back note for note, played timing is quantized, triplets
//! are found, channels split into tracks, and hostile bytes are refused without a panic.

use guitario_midi::{MidiError, export, import};
use guitario_score::{Beat, MeasureHeader, Note, NoteKind, Score, Track, timeline};
use guitario_theory::{Duration, NoteValue, TICKS_PER_QUARTER, TimeSignature, Tuning};
use std::path::PathBuf;

fn demos() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../demos")
}

/// (start, pitch, length) of every sounding note of a track, sorted.
fn heard(score: &Score, track: usize) -> Vec<(u64, u8, u32)> {
    let mut v: Vec<(u64, u8, u32)> =
        timeline(score).into_iter().filter(|n| n.track == track && !n.dead).filter_map(|n| Some((n.start_tick, n.pitch?.0, n.ticks))).collect();
    v.sort_unstable();
    v
}

/// A small Standard MIDI File writer for the tests: `(tick, bytes)` per track.
fn smf(format: u16, ppq: u16, tracks: &[Vec<(u32, Vec<u8>)>]) -> Vec<u8> {
    fn vlq(out: &mut Vec<u8>, v: u32) {
        let mut stack = vec![(v & 0x7f) as u8];
        let mut v = v >> 7;
        while v > 0 {
            stack.push((v & 0x7f) as u8 | 0x80);
            v >>= 7;
        }
        out.extend(stack.iter().rev());
    }
    let mut out = b"MThd".to_vec();
    out.extend(6u32.to_be_bytes());
    out.extend(format.to_be_bytes());
    out.extend((tracks.len() as u16).to_be_bytes());
    out.extend(ppq.to_be_bytes());
    for t in tracks {
        let mut ev = t.clone();
        ev.sort_by_key(|e| e.0);
        let mut body = Vec::new();
        let mut last = 0;
        for (tick, data) in ev {
            vlq(&mut body, tick - last);
            last = tick;
            body.extend(data);
        }
        body.extend([0x00, 0xff, 0x2f, 0x00]);
        out.extend(b"MTrk");
        out.extend((body.len() as u32).to_be_bytes());
        out.extend(body);
    }
    out
}

fn note(ch: u8, key: u8, on: u32, off: u32) -> [(u32, Vec<u8>); 2] {
    [(on, vec![0x90 | ch, key, 100]), (off, vec![0x80 | ch, key, 0])]
}

fn beats(score: &Score, track: usize, measure: usize) -> &[Beat] {
    score.tracks.get(track).and_then(|t| t.measures.get(measure)).and_then(|m| m.voices.first()).map_or(&[], |v| v.beats.as_slice())
}

#[test]
fn an_exported_score_comes_back_note_for_note() {
    let mut s = Score::new();
    s.meta.title = "Round Trip".into();
    s.tempo = 140;
    s.headers = vec![MeasureHeader::new(TimeSignature::COMMON); 3];
    s.headers[1].tempo = Some(90);
    s.headers[2].time_signature = TimeSignature::new(7, 8).unwrap();
    for h in &mut s.headers {
        h.key = 2;
    }
    let mut guitar = Track::new("Lead", Tuning::standard(), &s.headers);
    guitar.program = 30;
    let q = Duration::QUARTER;
    let e = Duration::EIGHTH;
    let t = Duration { tuplet: (3, 2), ..e };
    let chord = |frets: &[(u8, u8)], d| Beat { rest: false, notes: frets.iter().map(|&(s, f)| Note::new(s, f)).collect(), ..Beat::rest(d) };
    // Bar 1: a power chord, two eighths, a triplet, a rest.
    guitar.measures[0].voices[0].beats = vec![
        chord(&[(4, 2), (5, 0)], q),
        chord(&[(1, 5)], e),
        chord(&[(1, 7)], e),
        chord(&[(0, 3)], t),
        chord(&[(0, 5)], t),
        chord(&[(0, 7)], t),
        Beat::rest(q),
    ];
    // Bar 2: a half note tied over the bar line.
    let mut tied = chord(&[(2, 9)], Duration::new(NoteValue::Half));
    guitar.measures[1].voices[0].beats = vec![Beat::rest(Duration::new(NoteValue::Half)), tied.clone()];
    tied.notes[0].kind = NoteKind::Tie;
    tied.duration = q;
    guitar.measures[2].voices[0].beats = vec![tied, chord(&[(5, 0)], e), chord(&[(5, 3)], Duration::QUARTER.dotted(1)), chord(&[(5, 5)], e)];
    let mut drums = Track::drums("Drums", &s.headers);
    drums.measures[0].voices[0].beats = (0..4).map(|i| chord(&[(7, 36), (2, if i % 2 == 0 { 42 } else { 46 })], q)).collect();
    s.tracks = vec![guitar, drums];
    s.validate().unwrap();

    let back = import(&export(&s).unwrap()).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    let b = back.score;
    assert_eq!((b.tracks.len(), b.meta.title.as_str()), (2, "Round Trip"));
    assert_eq!(b.headers.len(), 3);
    assert_eq!((b.tempo, b.headers[1].tempo, b.tempo_at(2)), (140, Some(90), 90));
    assert_eq!(b.headers[2].time_signature, TimeSignature::new(7, 8).unwrap());
    assert!(b.headers.iter().all(|h| h.key == 2));
    assert_eq!((b.tracks[0].name.as_str(), b.tracks[0].program), ("Lead", 30));
    assert!(b.tracks[1].is_drums());
    assert_eq!(heard(&b, 0), heard(&s, 0));
    assert_eq!(heard(&b, 1), heard(&s, 1));
    // The triplet is written as one.
    assert_eq!(beats(&b, 0, 0).iter().filter(|x| x.duration.tuplet == (3, 2)).count(), 3);
    // Kick and hi-hat stay on their lanes.
    assert!(beats(&b, 1, 0)[0].notes.iter().any(|n| n.string == 7 && n.fret == 36));
}

#[test]
fn every_demo_exported_to_midi_imports_and_sounds_the_same() {
    let dir = demos();
    let mut checked = 0;
    for entry in std::fs::read_dir(dir.join("midi")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_string_lossy().to_string();
        let imported = import(&std::fs::read(&path).unwrap()).unwrap_or_else(|e| panic!("{name}: {e}"));
        imported.score.validate().unwrap();
        let original = guitario_format::open(&dir.join(format!("{name}.gt"))).unwrap();
        // Every pitch the demo plays is played by the import at the same moment, unless a grace
        // note, trill or tremolo put it off the grid or a chord ran out of strings. Harmonics are
        // left out: the file carries the pitch they ring at, not the fretted note.
        let want: Vec<(u64, u8)> =
            timeline(&original).into_iter().filter(|n| !n.dead && n.harmonic.is_none()).filter_map(|n| Some((n.start_tick, n.pitch?.0))).collect();
        let got: std::collections::BTreeSet<(u64, u8)> =
            (0..imported.score.tracks.len()).flat_map(|t| heard(&imported.score, t).into_iter().map(|n| (n.0, n.1))).collect();
        if want.is_empty() {
            continue;
        }
        let found = want.iter().filter(|n| got.contains(n)).count();
        assert!(found * 100 >= want.len() * if name.contains("technique") { 90 } else { 99 }, "{name}: only {found} of {} notes came back", want.len());
        checked += 1;
    }
    assert!(checked >= 10);
}

#[test]
fn played_timing_is_quantized_to_sixteenths() {
    // 480 ticks a quarter; sixteenths up to 6 ticks early or late, held a little short.
    let jitter = [3i32, -5, 6, -2, 0, 4, -6, 1];
    let mut ev = Vec::new();
    for (i, j) in jitter.iter().enumerate() {
        let on = (i as i32 * 120 + 480 + j).max(0) as u32;
        ev.extend(note(0, 52 + i as u8, on, on + 100));
    }
    let s = import(&smf(0, 480, &[ev])).unwrap().score;
    let b = beats(&s, 0, 0);
    assert!(b[0].rest);
    assert_eq!(b[0].duration, Duration::QUARTER);
    let sixteenth = Duration::new(NoteValue::Sixteenth);
    assert!(b[1..9].iter().all(|x| !x.rest && x.duration == sixteenth), "{:?}", b.iter().map(|x| x.duration).collect::<Vec<_>>());
    let starts: Vec<u64> = heard(&s, 0).iter().map(|n| n.0).collect();
    assert_eq!(starts, (0..8).map(|i| u64::from(TICKS_PER_QUARTER) + i * u64::from(TICKS_PER_QUARTER / 4)).collect::<Vec<_>>());
}

#[test]
fn format_zero_channels_become_tracks_and_channel_ten_is_drums() {
    let mut ev = vec![(0, vec![0xff, 0x03, 4, b'S', b'o', b'n', b'g']), (0, vec![0xc1, 33]), (0, vec![0xb1, 7, 90])];
    ev.extend(note(0, 64, 0, 480));
    ev.extend(note(1, 28, 0, 960));
    ev.extend(note(9, 36, 0, 10));
    ev.extend(note(9, 38, 480, 490));
    let s = import(&smf(0, 480, &[ev])).unwrap().score;
    assert_eq!(s.tracks.len(), 3);
    assert_eq!(s.tracks[0].name, "Song (1)");
    let bass = &s.tracks[1];
    assert_eq!((bass.program, bass.volume, bass.string_count()), (33, 90, 4));
    assert_eq!(beats(&s, 1, 0)[0].notes[0], Note::new(3, 0), "low E on the bass's open fourth string");
    assert!(s.tracks[2].is_drums());
    // Drum hits last until the next one: no rests between them.
    let d = beats(&s, 2, 0);
    assert_eq!((d[0].rest, d[0].duration, d[1].notes[0].fret), (false, Duration::QUARTER, 38));
}

#[test]
fn a_conductor_track_names_the_song_and_carries_the_tempo() {
    let conductor =
        vec![(0, vec![0xff, 0x03, 5, b'T', b'i', b't', b'l', b'e']), (0, vec![0xff, 0x51, 3, 0x07, 0xa1, 0x20]), (1920, vec![0xff, 0x51, 3, 0x03, 0xd0, 0x90])];
    let mut guitar = vec![(0, vec![0xff, 0x03, 3, b'G', b't', b'r'])];
    guitar.extend(note(0, 40, 0, 3840));
    let s = import(&smf(1, 480, &[conductor, guitar])).unwrap().score;
    assert_eq!(s.meta.title, "Title");
    assert_eq!((s.tempo, s.headers[1].tempo), (120, Some(240)));
    assert_eq!(s.tracks.len(), 1);
    assert_eq!(s.tracks[0].name, "Gtr");
    // A whole note held over two bars: tied into the second.
    assert_eq!(beats(&s, 0, 1)[0].notes[0].kind, NoteKind::Tie);
}

#[test]
fn the_tuning_follows_the_range() {
    let low = |key: u8| {
        let mut ev = Vec::new();
        ev.extend(note(0, key, 0, 480));
        ev.extend(note(0, 64, 480, 960));
        import(&smf(0, 480, &[ev])).unwrap().score.tracks.remove(0).tuning
    };
    assert_eq!(low(40).strings, Tuning::standard().strings);
    assert_eq!(low(38).strings, Tuning::drop_d().strings);
    assert_eq!(low(35).strings.len(), 7);
    assert_eq!(low(30).strings.len(), 8);
}

#[test]
fn chords_are_fingered_without_crossing_strings() {
    // An open E major chord, then a barre A major at the fifth fret.
    let mut ev = Vec::new();
    for k in [40, 47, 52, 56, 59, 64] {
        ev.extend(note(0, k, 0, 480));
    }
    for k in [45, 52, 57, 61, 64, 69] {
        ev.extend(note(0, k, 480, 960));
    }
    let s = import(&smf(0, 480, &[ev])).unwrap().score;
    let b = beats(&s, 0, 0);
    let frets = |i: usize| {
        let mut v: Vec<(u8, u8)> = b[i].notes.iter().map(|n| (n.string, n.fret)).collect();
        v.sort_unstable();
        v
    };
    assert_eq!(frets(0), vec![(0, 0), (1, 0), (2, 1), (3, 2), (4, 2), (5, 0)]);
    assert_eq!(frets(1), vec![(0, 5), (1, 5), (2, 6), (3, 7), (4, 7), (5, 5)]);
}

#[test]
fn eighth_triplets_and_triplet_quarters_are_found() {
    let mut ev = Vec::new();
    for i in 0..3 {
        ev.extend(note(0, 60 + i, i as u32 * 160, i as u32 * 160 + 150));
    }
    ev.extend(note(0, 64, 480, 1920));
    let s = import(&smf(0, 480, &[ev])).unwrap().score;
    let b = beats(&s, 0, 0);
    let triplet_eighth = Duration { tuplet: (3, 2), ..Duration::EIGHTH };
    assert!(b[..3].iter().all(|x| x.duration == triplet_eighth));
    assert_eq!(b[3].duration, Duration::new(NoteValue::Half).dotted(1));
}

#[test]
fn hostile_files_are_refused_without_a_panic() {
    assert!(matches!(import(b""), Err(MidiError::NotMidi)));
    assert!(matches!(import(b"RIFF...."), Err(MidiError::NotMidi)));
    assert!(import(b"MThd\x00\x00\x00\x06\x00\x01\x00\x01\x00\x00").is_err(), "zero division");
    assert!(import(b"MThd\x00\x00\x00\x06\x00\x01\x00\x01\x01\xe0MTrk\xff\xff\xff\xff\x00\x90").is_err());
    assert!(import(b"MThd\xff\xff\xff\xff").is_err());
    // A note that never ends, a running status with nothing before it, a huge delta.
    let open = smf(0, 96, &[vec![(0, vec![0x90, 60, 100])]]);
    assert!(import(&open).is_ok());
    let running = smf(0, 96, &[vec![(0, vec![60, 100])]]);
    assert!(import(&running).is_err());
    let far = smf(0, 1, &[note(0, 60, 0x0fff_fff0, 0x0fff_ffff).to_vec()]);
    let r = import(&far).unwrap();
    assert!(r.score.headers.len() <= 20_000 && r.warnings.iter().any(|w| w.contains("cut")));
    // Every demo, cut short and with bytes flipped.
    let mut seed = 0x2545_f491_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for entry in std::fs::read_dir(demos().join("midi")).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        for _ in 0..40 {
            let mut b = bytes.clone();
            let cut = (next() as usize) % (b.len() + 1);
            if next() % 2 == 0 {
                b.truncate(cut);
            }
            for _ in 0..(next() % 8) {
                if b.is_empty() {
                    break;
                }
                let at = (next() as usize) % b.len();
                b[at] = next() as u8;
            }
            if let Ok(i) = import(&b) {
                i.score.validate().unwrap();
            }
        }
    }
}
