//! Shared by the Guitar Pro round-trip tests.
#![allow(dead_code, clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

use guitario_score::*;
use guitario_theory::{Duration, NoteValue, TICKS_PER_QUARTER, TimeSignature};

/// Every score in `demos/`, by file name.
pub fn demos() -> Vec<(String, Score)> {
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

/// Compares two scores part by part, so a failure says which bar differs.
pub fn assert_same(name: &str, expected: &Score, got: &Score) {
    assert_eq!(expected.meta, got.meta, "{name}: song information");
    assert_eq!(expected.tempo, got.tempo, "{name}: tempo");
    assert_eq!(expected.headers.len(), got.headers.len(), "{name}: bar count");
    for (i, (a, b)) in expected.headers.iter().zip(&got.headers).enumerate() {
        assert_eq!(a, b, "{name}: bar {}", i + 1);
    }
    assert_eq!(expected.tracks.len(), got.tracks.len(), "{name}: track count");
    for (ti, (a, b)) in expected.tracks.iter().zip(&got.tracks).enumerate() {
        assert_eq!(
            (&a.name, a.kind, &a.tuning.strings, a.capo, a.program, a.volume, a.pan, a.muted, a.solo, &a.lyrics, a.color_at(ti), a.notation),
            (&b.name, b.kind, &b.tuning.strings, b.capo, b.program, b.volume, b.pan, b.muted, b.solo, &b.lyrics, b.color_at(ti), b.notation),
            "{name}: track {ti}"
        );
        for (mi, (ma, mb)) in a.measures.iter().zip(&b.measures).enumerate() {
            assert_eq!(ma, mb, "{name}: track {ti} bar {}", mi + 1);
        }
    }
}

/// A short riff that uses every effect the model carries.
pub fn riff() -> Score {
    let mut s = Score::new();
    s.meta = Meta {
        title: "Ünïcode – riff".into(),
        artist: "Guitario".into(),
        subtitle: "Second take".into(),
        album: "Tests".into(),
        words: "A Lyricist".into(),
        music: "A Composer".into(),
        copyright: "2026 Guitario contributors".into(),
        author: "Tabber".into(),
        notes: "line one\nline two".into(),
        instructions: "Play it loud".into(),
    };
    s.tempo = 152;
    let e = Duration::EIGHTH;
    let mut h = MeasureHeader::new(TimeSignature::COMMON);
    h.repeat_open = true;
    h.marker = Some("Intro".into());
    h.key = -3;
    h.triplet_feel = TripletFeel::Triplet16th;
    h.targets = vec![DirectionTarget::Segno];
    h.fermatas =
        vec![Fermata { tick: 0, kind: FermataKind::Short, length: 26 }, Fermata { tick: TICKS_PER_QUARTER * 3 / 2, kind: FermataKind::Long, length: 100 }];
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
    n.effects.grace = Some(Grace { transition: GraceTransition::Hammer, ..Grace::new(2) });
    let mut bend = Note::new(1, 7);
    bend.effects.bend = Some(4);
    bend.effects.vibrato = true;
    bend.effects.left_finger = Some(Finger::Ring);
    bend.effects.right_finger = Some(Finger::Index);
    bend.effects.grace = Some(Grace { fret: 5, duration: GraceDuration::Sixteenth, transition: GraceTransition::Bend, on_beat: true, dead: false });
    let mut tie = Note::new(1, 7);
    tie.kind = NoteKind::Tie;
    let mut slide = Note::new(2, 5);
    slide.effects.slide = Some(Slide::IntoFromBelow);
    slide.effects.harmonic = Some(Harmonic::Pinch);
    slide.effects.accent = true;
    slide.effects.grace = Some(Grace { transition: GraceTransition::Slide, ..Grace::new(3) });
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
    popped.tremolo = Some(TremoloSpeed::ThirtySecond);
    let mut trilled = Note::new(0, 12);
    trilled.effects.trill = Some(Trill { fret: 14, speed: TrillSpeed::SixtyFourth });
    trilled.effects.left_finger = Some(Finger::Thumb);
    trilled.effects.right_finger = Some(Finger::Little);
    trilled.effects.grace = Some(Grace { duration: GraceDuration::SixtyFourth, dead: true, ..Grace::new(0) });
    let mut triplet = note(vec![slide], Duration { tuplet: (3, 2), ..e });
    triplet.dynamic = Dynamic::Pp;
    t.measures[0].voices[0].beats = vec![chord, popped, note(vec![bend], e), note(vec![tie], Duration::QUARTER), triplet.clone(), triplet.clone(), triplet];
    t.measures[1].voices = vec![
        Voice { beats: vec![note(vec![dead], Duration { dots: 1, ..Duration::QUARTER }), Beat::rest(Duration { dots: 1, ..Duration::QUARTER })] },
        Voice { beats: vec![Beat::rest(Duration::QUARTER), note(vec![trilled], Duration::new(NoteValue::Half))] },
    ];
    t.lyrics = vec![LyricLine { measure: 0, text: "[Intro] Ün-ï-code riff+again - yeah__".into() }, LyricLine { measure: 1, text: "se-cond line".into() }];
    let mut drums = guitario_score::Track::drums("Drums", &s.headers);
    drums.muted = true;
    let hit = |midi: u8| Note::new(guitario_theory::drums::lane_for(midi), midi);
    drums.measures[0].voices[0].beats =
        vec![note(vec![hit(49), hit(38), hit(36)], Duration::new(NoteValue::Half)), note(vec![hit(42)], Duration::new(NoteValue::Half))];
    t.color = Some([0x12, 0x34, 0x56]);
    drums.color = Some(guitario_score::TRACK_COLORS[1]);
    s.tracks = vec![t, drums];
    s.validate().unwrap();
    s
}

/// What a score looks like after a trip through a Guitar Pro 7/8 file: what that file does not
/// carry here (chord names on drum tracks, track volume and pan, beam groupings) is gone, the first bar's tempo is the song's
/// tempo, brush lengths are whole thousandths of a quarter, and voices that only rest are dropped.
pub fn as_gp(s: &Score) -> Score {
    let mut s = s.clone();
    if let Some(t) = s.headers.first().and_then(|h| h.tempo) {
        s.tempo = t;
        s.headers[0].tempo = None;
    }
    for h in &mut s.headers {
        h.beams.clear();
    }
    for t in &mut s.tracks {
        t.volume = 100;
        t.pan = 0;
        let drums = t.is_drums();
        for m in &mut t.measures {
            while m.voices.len() > 1 && m.voices.last().is_none_or(|v| v.beats.iter().all(|b| b.rest)) {
                m.voices.pop();
            }
            for b in m.voices.iter_mut().flat_map(|v| &mut v.beats) {
                if drums {
                    b.chord = None;
                }
                // One vibrato element: wide wins over plain.
                for n in &mut b.notes {
                    if n.effects.wide_vibrato {
                        n.effects.vibrato = false;
                    }
                }
                if let Some(br) = &mut b.brush {
                    *br = if br.kind == BrushKind::Rasgueado {
                        Brush { kind: BrushKind::Rasgueado, down: true, ticks: TICKS_PER_QUARTER / 8 }
                    } else {
                        Brush { ticks: (br.ticks / (TICKS_PER_QUARTER / 960)).clamp(1, 7680) * (TICKS_PER_QUARTER / 960), ..*br }
                    };
                }
            }
        }
    }
    s
}
