//! Guitar Pro 7/8 export read back by the importer: every demo survives the trip, the file is a
//! well-formed GPIF archive, and damaged archives are refused without a panic.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

mod common;

use common::{as_gp, assert_same, demos, riff};
use guitario_gp::{export_gp, import};
use guitario_score::*;
use guitario_theory::Duration;
use std::io::Read;

fn round_trip(s: &Score) -> (Score, Vec<String>, Vec<String>) {
    let out = export_gp(s).expect("export");
    let back = import(&out.bytes).unwrap_or_else(|e| panic!("exported file does not import: {e}"));
    (back.score, out.warnings, back.warnings)
}

fn score_gpif(bytes: &[u8]) -> String {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a ZIP archive");
    let mut xml = String::new();
    zip.by_name("Content/score.gpif").expect("Content/score.gpif").read_to_string(&mut xml).unwrap();
    xml
}

#[test]
fn every_demo_survives_guitar_pro_8() {
    for (name, score) in demos() {
        let (back, _, import_warnings) = round_trip(&score);
        assert!(import_warnings.is_empty(), "{name}: {import_warnings:?}");
        assert_same(&name, &as_gp(&score), &back);
        // A second trip changes nothing more.
        let (again, _, _) = round_trip(&back);
        assert_same(&name, &back, &again);
    }
}

#[test]
fn a_riff_with_every_effect_round_trips_and_nothing_is_left_out() {
    let s = riff();
    let (back, warnings, import_warnings) = round_trip(&s);
    assert!(import_warnings.is_empty(), "{import_warnings:?}");
    assert_same("riff", &as_gp(&s), &back);
    // Nothing the riff carries is left out; its chord diagram comes back.
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(back.tracks[0].measures[0].voices[0].beats[0].chord, s.tracks[0].measures[0].voices[0].beats[0].chord);
    // Everything else the riff carries came back: effects, ties, voices, repeats, endings, jumps.
    let h = &back.headers[1];
    assert_eq!((h.repeat_close, h.endings, h.jump, h.double_bar), (3, 0b101, Some(Jump::DalSegno(JumpEnd::Fine)), true));
    assert_eq!(back.headers[0].targets, vec![DirectionTarget::Segno]);
    assert_eq!(back.tracks[0].measures[1].voices.len(), 2);
}

#[test]
fn a_section_keeps_its_letter_apart_from_its_name() {
    let mut s = riff();
    s.headers[0].section_letter = Some("A".into());
    s.headers[1].marker = Some(String::new());
    s.headers[1].section_letter = Some("B".into());
    let (back, _, _) = round_trip(&s);
    let got: Vec<_> = back.headers.iter().take(2).map(|h| (h.section_letter.as_deref(), h.marker.as_deref())).collect();
    assert_eq!(got, vec![(Some("A"), Some("Intro")), (Some("B"), Some(""))]);
}

#[test]
fn the_interpretation_comes_back_except_automatic_let_ring() {
    let mut s = riff();
    let i = Interpretation { style: PlayingStyle::Fingers, palm_mute: 30, accentuation: 34, auto_let_ring: true, auto_brush: true, stringed: false };
    s.tracks[0].interpretation = i;
    let (back, _, _) = round_trip(&s);
    assert_eq!(back.tracks[0].interpretation, Interpretation { auto_let_ring: false, ..i });
}

#[test]
fn the_audio_engine_comes_back() {
    let mut s = riff();
    s.tracks[1].engine = SoundEngine::Midi;
    let (back, _, _) = round_trip(&s);
    assert_eq!((back.tracks[0].engine, back.tracks[1].engine), (SoundEngine::Rse, SoundEngine::Midi));
}

#[test]
fn each_tracks_notations_come_back() {
    let mut s = riff();
    s.tracks[0].notation = Notations { standard: false, tab: true, slash: true, numbered: false };
    let (back, _, _) = round_trip(&s);
    assert_eq!(back.tracks[0].notation, s.tracks[0].notation);
    assert_eq!(back.tracks[1].notation, Notations { tab: false, ..Notations::default() }, "a drum kit shows its staff only");
}

#[test]
fn short_names_and_colours_come_back() {
    let mut s = riff();
    s.tracks[0].short_name = "ld.gt.".into();
    let (back, _, _) = round_trip(&s);
    assert_eq!((back.tracks[0].short_name.as_str(), back.tracks[0].color), ("ld.gt.", Some([0x12, 0x34, 0x56])));
    // A track with no colour of its own goes out with its default one.
    s.tracks[0].color = None;
    let (back, _, _) = round_trip(&s);
    assert_eq!(back.tracks[0].color, Some(guitario_score::TRACK_COLORS[0]));
}

#[test]
fn four_voices_and_the_whole_model_of_a_bar_come_back() {
    let mut s = riff();
    let note = |string: u8, fret: u8| Beat { rest: false, notes: vec![Note::new(string, fret)], ..Beat::rest(Duration::WHOLE) };
    s.tracks[0].measures[0].voices.truncate(1);
    s.tracks[0].measures[0].voices.push(Voice { beats: vec![note(2, 4)] });
    s.tracks[0].measures[0].voices.push(Voice { beats: vec![note(3, 5)] });
    s.tracks[0].measures[0].voices.push(Voice { beats: vec![note(4, 6)] });
    assert!(s.validate().is_ok());
    let (back, _, _) = round_trip(&s);
    assert_eq!(back.tracks[0].measures[0].voices.len(), 4);
    assert_same("four voices", &as_gp(&s), &back);
}

#[test]
fn the_archive_is_well_formed_and_hostile_text_is_made_safe() {
    let mut s = riff();
    s.meta.title = "A ]]> B \u{1}\u{7} <&> \"quoted\" ünï".into();
    s.tracks[0].name = "]]>]]><x/>".into();
    s.tracks[0].measures[0].voices[0].beats[0].text = Some("x]]>y\u{0}".into());
    let out = export_gp(&s).expect("export");
    let xml = score_gpif(&out.bytes);
    roxmltree::Document::parse(&xml).expect("score.gpif is well-formed XML");
    assert!(xml.starts_with("<?xml"));
    let back = import(&out.bytes).expect("imports").score;
    assert_eq!(back.meta.title, "A ]]> B  <&> \"quoted\" ünï", "the two control characters are removed, everything else is kept");
    assert_eq!(back.tracks[0].name, "]]>]]><x/>");
    assert_eq!(back.tracks[0].measures[0].voices[0].beats[0].text.as_deref(), Some("x]]>y"));
    // The container carries the version marker the fixture has, and the music under Content/.
    let zip = zip::ZipArchive::new(std::io::Cursor::new(&out.bytes)).unwrap();
    assert!(zip.file_names().any(|n| n == "VERSION") && zip.file_names().any(|n| n == "Content/score.gpif"));
}

#[test]
fn every_note_and_beat_has_its_own_id_and_every_reference_resolves() {
    let (_, score) = demos().into_iter().find(|(n, _)| n.contains("two-voices")).unwrap();
    let xml = score_gpif(&export_gp(&score).unwrap().bytes);
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let ids = |tag: &str| -> Vec<String> {
        doc.descendants().filter(|n| n.has_tag_name(tag) && n.attribute("id").is_some()).map(|n| n.attribute("id").unwrap().to_string()).collect()
    };
    for tag in ["Bar", "Voice", "Beat", "Note", "Rhythm"] {
        let all = ids(tag);
        let unique: std::collections::BTreeSet<_> = all.iter().collect();
        assert_eq!(all.len(), unique.len(), "duplicate {tag} ids");
    }
    let (notes, beats, rhythms, voices) = (ids("Note"), ids("Beat"), ids("Rhythm"), ids("Voice"));
    for beat in doc.descendants().filter(|n| n.has_tag_name("Beat") && n.attribute("id").is_some()) {
        let r = beat.children().find(|c| c.has_tag_name("Rhythm")).and_then(|c| c.attribute("ref")).unwrap();
        assert!(rhythms.iter().any(|x| x == r), "beat points at a missing rhythm {r}");
        if let Some(ns) = beat.children().find(|c| c.has_tag_name("Notes")).and_then(|c| c.text()) {
            assert!(ns.split_whitespace().all(|n| notes.iter().any(|x| x == n)), "beat points at a missing note");
        }
    }
    for voice in doc.descendants().filter(|n| n.has_tag_name("Voice") && n.attribute("id").is_some()) {
        let bs = voice.children().find(|c| c.has_tag_name("Beats")).and_then(|c| c.text()).unwrap();
        assert!(bs.split_whitespace().all(|b| beats.iter().any(|x| x == b)));
    }
    for bar in doc.descendants().filter(|n| n.has_tag_name("Bar") && n.attribute("id").is_some()) {
        let vs = bar.children().find(|c| c.has_tag_name("Voices")).and_then(|c| c.text()).unwrap();
        assert!(vs.split_whitespace().all(|v| v == "-1" || voices.iter().any(|x| x == v)));
        assert_eq!(vs.split_whitespace().count(), 4, "a bar lists four voice slots");
    }
    // Rhythms are shared; the two-voice study needs only a handful.
    assert!(rhythms.len() < 10, "{} rhythms", rhythms.len());
}

#[test]
fn ties_mark_both_ends_and_hammer_ons_mark_their_target() {
    let (_, score) = demos().into_iter().find(|(n, _)| n.contains("two-voices")).unwrap();
    let xml = score_gpif(&export_gp(&score).unwrap().bytes);
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let tie = |origin: &str, destination: &str| {
        doc.descendants().filter(|n| n.has_tag_name("Tie") && n.attribute("origin") == Some(origin) && n.attribute("destination") == Some(destination)).count()
    };
    assert_eq!((tie("true", "false"), tie("false", "true")), (1, 1), "the G tied over the bar line");

    let mut s = Score::new();
    let mut a = Note::new(0, 3);
    a.effects.hammer = true;
    let beat = |n: Note| Beat { rest: false, notes: vec![n], ..Beat::rest(Duration::QUARTER) };
    s.tracks[0].measures[0].voices[0].beats = vec![beat(a), beat(Note::new(0, 5)), Beat::rest(Duration::new(guitario_theory::NoteValue::Half))];
    let xml = score_gpif(&export_gp(&s).unwrap().bytes);
    assert_eq!(xml.matches("name=\"HopoOrigin\"").count(), 1);
    assert_eq!(xml.matches("name=\"HopoDestination\"").count(), 1);
}

#[test]
fn grace_notes_are_written_as_grace_beats_before_their_beat() {
    let s = riff();
    let xml = score_gpif(&export_gp(&s).unwrap().bytes);
    assert!(xml.contains("<GraceNotes>BeforeBeat</GraceNotes>") && xml.contains("<GraceNotes>OnBeat</GraceNotes>"), "{xml}");
    // A chord whose notes have grace notes of two kinds gets one grace beat per kind.
    let mut s = riff();
    let mut a = Note::new(0, 5);
    a.effects.grace = Some(Grace::new(3));
    let mut b = Note::new(1, 5);
    b.effects.grace = Some(Grace { on_beat: true, ..Grace::new(7) });
    let mut c = Note::new(2, 5);
    c.effects.grace = Some(Grace::new(4));
    s.tracks[0].measures[1].voices =
        vec![Voice { beats: vec![Beat { rest: false, notes: vec![a, b, c], ..Beat::rest(Duration { dots: 1, ..Duration::WHOLE }) }] }];
    s.headers[1].time_signature = guitario_theory::TimeSignature { numerator: 6, denominator: 4 };
    assert!(s.validate().is_ok());
    let out = export_gp(&s).unwrap();
    assert_eq!(score_gpif(&out.bytes).matches("<GraceNotes>").count(), 2 + 5, "two for the chord, five in bar 1");
    let back = import(&out.bytes).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    let notes = &back.score.tracks[0].measures[1].voices[0].beats[0].notes;
    assert_eq!(notes.iter().map(|n| n.effects.grace).collect::<Vec<_>>(), vec![a.effects.grace, b.effects.grace, c.effects.grace]);
}

#[test]
fn a_grace_beat_with_nothing_to_ornament_is_reported() {
    let s = riff();
    let xml = score_gpif(&export_gp(&s).unwrap().bytes);
    // Turn the first grace beat's target into a rest: the grace note has no note to belong to.
    let first = xml.find("<GraceNotes>BeforeBeat</GraceNotes>").unwrap();
    let beat_start = xml[first..].find("</Beat>").unwrap() + first;
    let next_notes = xml[beat_start..].find("<Notes>").unwrap() + beat_start;
    let end = xml[next_notes..].find("</Notes>").unwrap() + next_notes + "</Notes>".len();
    let broken = format!("{}{}", &xml[..next_notes], &xml[end..]);
    let imp = guitario_gp::parse_gpif(&broken).unwrap();
    assert!(imp.warnings.iter().any(|w| w.contains("grace notes")), "{:?}", imp.warnings);
    assert!(imp.score.validate().is_ok());
}

#[test]
fn chords_are_listed_once_per_track_with_frets_from_their_base_fret() {
    let mut s = riff();
    let barre = Chord { name: "D <5> & \"x\"".into(), frets: vec![Some(5), Some(7), Some(7), Some(7), Some(5), None], first_fret: 5 };
    let named = Chord::named("N.C.");
    let beats = &mut s.tracks[0].measures[0].voices[0].beats;
    beats[1].chord = Some(barre.clone());
    beats[2].chord = Some(barre.clone());
    beats[3].chord = Some(named.clone());
    let xml = score_gpif(&export_gp(&s).unwrap().bytes);
    // Three distinct chords on the guitar: D5, the barre and the name; the barre is listed once.
    assert_eq!(xml.matches("<Item id=").count(), 3, "{xml}");
    assert!(xml.contains("baseFret=\"4\"") && xml.contains("<Fret string=\"1\" fret=\"1\"/>"), "relative to the base fret");
    assert!(xml.contains("name=\"D &lt;5&gt; &amp; &quot;x&quot;\""), "names are escaped");
    let (back, _, warnings) = round_trip(&s);
    assert!(warnings.is_empty(), "{warnings:?}");
    let got: Vec<_> = back.tracks[0].measures[0].voices[0].beats.iter().map(|b| b.chord.clone()).collect();
    assert_eq!(got[..4], [s.tracks[0].measures[0].voices[0].beats[0].chord.clone(), Some(barre.clone()), Some(barre), Some(named)]);
    // A chord on a drum track is said to be left out.
    let mut d = riff();
    d.tracks[1].measures[0].voices[0].beats[0].chord = Some(Chord::named("X"));
    let out = export_gp(&d).unwrap();
    assert!(out.warnings.iter().any(|w| w.contains("drum tracks")), "{:?}", out.warnings);
}

#[test]
fn lyrics_are_written_as_typed_and_on_their_beats_and_read_from_either() {
    let s = riff();
    let xml = score_gpif(&export_gp(&s).unwrap().bytes);
    assert!(xml.contains("<Lyrics dispatched=\"true\">") && xml.contains("<Offset>1</Offset>"), "the lines as typed");
    assert!(xml.contains("<Line><![CDATA[Ün-]]></Line>") && xml.contains("<Line><![CDATA[riff again]]></Line>"), "syllables on beats");
    // Without the typed lines, the beats' syllables give the same lyrics back.
    let start = xml.find("<Lyrics dispatched=\"true\">").unwrap();
    let end = xml[start..].find("</Lyrics>").unwrap() + start + "</Lyrics>".len();
    let beats_only = format!("{}{}", &xml[..start], &xml[end..]);
    let back = guitario_gp::parse_gpif(&beats_only).unwrap().score;
    let sung = |s: &Score, k: usize| {
        let t = &s.tracks[0];
        t.lyrics.get(k).map(|l| dispatch_lyrics(t, l).into_iter().map(|(m, b, x)| (m, b, x.text, x.hyphen)).collect::<Vec<_>>()).unwrap_or_default()
    };
    for k in 0..2 {
        assert!(!sung(&s, k).is_empty());
        assert_eq!(sung(&back, k), sung(&s, k), "line {}", k + 1);
    }
}

#[test]
fn fermatas_are_written_where_they_fall_as_a_fraction_of_a_quarter() {
    let xml = score_gpif(&export_gp(&riff()).unwrap().bytes);
    assert!(xml.contains("<Fermata><Type>Short</Type><Offset>0/1</Offset><Length>0.26</Length></Fermata>"), "{xml}");
    assert!(xml.contains("<Fermata><Type>Long</Type><Offset>3/2</Offset><Length>1</Length></Fermata>"));
    // Hostile offsets are dropped, not trusted.
    let hostile = xml.replace("<Offset>3/2</Offset>", "<Offset>99/0</Offset>").replace("<Offset>0/1</Offset>", "<Offset>400/1</Offset>");
    let back = guitario_gp::parse_gpif(&hostile).unwrap().score;
    assert!(back.validate().is_ok());
    assert!(back.headers[0].fermatas.len() <= 1);
}

#[test]
fn drum_hits_outside_the_kit_keep_their_midi_number() {
    let mut s = Score::new();
    s.tracks = vec![Track::drums("Drums", &s.headers)];
    let hit = |midi: u8| Note::new(guitario_theory::drums::lane_for(midi), midi);
    // 56 is a cowbell: not in the kit map, so it travels as a General MIDI number.
    s.tracks[0].measures[0].voices[0].beats = vec![
        Beat { rest: false, notes: vec![hit(42), hit(38), hit(36)], ..Beat::rest(Duration::QUARTER) },
        Beat { rest: false, notes: vec![hit(56)], ..Beat::rest(Duration::QUARTER) },
        Beat::rest(Duration::new(guitario_theory::NoteValue::Half)),
    ];
    let (back, _, _) = round_trip(&s);
    let b = &back.tracks[0].measures[0].voices[0].beats;
    assert_eq!(b[0].notes.iter().map(|n| n.fret).collect::<Vec<_>>(), vec![42, 38, 36]);
    assert_eq!(b[1].notes[0].fret, 56);
    assert_same("drums", &as_gp(&s), &back);
}

#[test]
fn a_long_score_exports_and_comes_back_whole() {
    let mut s = Score::new();
    for _ in 0..2_999 {
        let h = s.headers[0].clone();
        s.headers.push(h.clone());
        for t in &mut s.tracks {
            t.measures.push(Measure::empty_for(&h));
        }
    }
    let beat = |f: u8| Beat { rest: false, notes: vec![Note::new(1, f)], ..Beat::rest(Duration::QUARTER) };
    for (i, m) in s.tracks[0].measures.iter_mut().enumerate() {
        m.voices[0].beats = vec![beat((i % 20) as u8), beat(3), beat(5), beat(7)];
    }
    let (back, warnings, _) = round_trip(&s);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(back.headers.len(), 3_000);
    assert_eq!(back.tracks[0].measures[2_999].voices[0].beats[0].notes[0].fret, (2_999 % 20) as u8);
}

#[test]
fn every_truncation_is_refused_and_corruption_never_panics() {
    let bytes = export_gp(&riff()).unwrap().bytes;
    for len in 0..bytes.len() {
        assert!(import(&bytes[..len]).is_err(), "a file cut at {len} of {} bytes imported", bytes.len());
    }
    for i in 0..bytes.len() {
        for v in [0x00, 0x7f, 0xff] {
            let mut b = bytes.clone();
            b[i] = v;
            if let Ok(imp) = import(&b) {
                assert!(imp.score.validate().is_ok());
            }
        }
    }
}

#[test]
fn the_page_setup_goes_out_and_comes_back() {
    let mut s = riff();
    for page in [
        PageSetup::default(),
        PageSetup { paper: Paper::Letter, ..PageSetup::default() },
        PageSetup { paper: Paper::Legal, landscape: true, margin_left: 5, margin_right: 25, margin_top: 20, margin_bottom: 8 },
        PageSetup { paper: Paper::A5, margin_left: 0, margin_right: 0, margin_top: 0, margin_bottom: 0, ..PageSetup::default() },
    ] {
        s.page = page;
        let (back, export_warnings, import_warnings) = round_trip(&s);
        assert_eq!(back.page, page);
        assert!(import_warnings.is_empty(), "{import_warnings:?}");
        assert!(export_warnings.iter().all(|w| !w.contains("page")), "{export_warnings:?}");
    }
    // A file nobody gave a page leaves it out; one that has a page says so.
    s.page = PageSetup::default();
    assert!(!score_gpif(&export_gp(&s).unwrap().bytes).contains("<PageSetup>"));
    s.page.paper = Paper::Letter;
    let xml = score_gpif(&export_gp(&s).unwrap().bytes);
    assert!(xml.contains("<Width>216</Width><Height>279</Height><Orientation>Portrait</Orientation>"), "{xml}");
}

#[test]
fn a_real_letter_page_setup_is_read() {
    // Our own export with the block put in as a real Guitar Pro 6 file stores it.
    let base = score_gpif(&export_gp(&riff()).unwrap().bytes);
    assert!(!base.contains("<PageSetup>"));
    let with = |setup: &str| base.replacen("</Score>", &format!("{setup}</Score>"), 1);
    let page = |setup: &str| guitario_gp::parse_gpif(&with(setup)).expect("imports").score.page;
    let letter = "<PageSetup><Width>216</Width><Height>279</Height><Orientation>Portrait</Orientation><TopMargin>15</TopMargin><LeftMargin>10</LeftMargin><RightMargin>10</RightMargin><BottomMargin>10</BottomMargin><Scale>1</Scale></PageSetup>";
    assert_eq!(page(letter), PageSetup { paper: Paper::Letter, ..PageSetup::default() });
    assert_eq!(page(""), PageSetup::default());
    // Landscape from the flag, whichever way round the sizes are written; odd values never panic.
    let wide = letter.replace("Portrait", "Landscape");
    assert!(page(&wide).landscape);
    for odd in [
        "<PageSetup></PageSetup>",
        "<PageSetup><Width>x</Width><Height>NaN</Height></PageSetup>",
        "<PageSetup><Width>1e300</Width><Height>-1e300</Height><LeftMargin>9999999999999</LeftMargin></PageSetup>",
        "<PageSetup><Width>216</Width><Height>279</Height><TopMargin>-50</TopMargin><BottomMargin>inf</BottomMargin></PageSetup>",
    ] {
        let p = page(odd);
        assert!(p.check().is_ok(), "{odd}: {p:?}");
    }
}

#[test]
fn tempo_ramps_go_out_and_come_back_and_points_inside_bars_move_to_a_bar_line() {
    let mut s = riff();
    while s.headers.len() < 4 {
        let h = s.headers[0].clone();
        s.headers.push(MeasureHeader { fermatas: vec![], ..h });
        for t in &mut s.tracks {
            let m = guitario_score::Measure::empty_for(&s.headers[0]);
            t.measures.push(m);
        }
    }
    for h in &mut s.headers {
        h.tempo = None;
        h.tempo_linear = false;
    }
    s.headers[3].tempo = Some(160);
    s.headers[3].tempo_linear = true;
    let (back, _, warnings) = round_trip(&s);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!((back.headers[3].tempo, back.headers[3].tempo_linear), (Some(160), true));
    assert!(score_gpif(&export_gp(&s).unwrap().bytes).contains("<Linear>true</Linear><Bar>3</Bar>"));
    // Guitar Pro 5 cannot glide: the tempo arrives as a jump, and the export says so.
    let gp5 = guitario_gp::export_gp5(&s);
    assert!(gp5.warnings.iter().any(|w| w.contains("gradual tempo")), "{:?}", gp5.warnings);
    // A point three quarters into bar 2 lands on bar 3's line; one a quarter in stays on bar 2.
    let xml = score_gpif(&export_gp(&s).unwrap().bytes)
        .replace("<Linear>true</Linear><Bar>3</Bar><Position>0</Position>", "<Linear>true</Linear><Bar>1</Bar><Position>0.75</Position>");
    let imp = guitario_gp::parse_gpif(&xml).unwrap();
    assert_eq!((imp.score.headers[2].tempo, imp.score.headers[2].tempo_linear), (Some(160), true));
    assert!(imp.warnings.iter().any(|w| w.contains("moved to the nearest bar line")), "{:?}", imp.warnings);
    let xml = xml.replace("<Position>0.75</Position>", "<Position>0.25</Position>");
    assert_eq!(guitario_gp::parse_gpif(&xml).unwrap().score.headers[1].tempo, Some(160));
}
