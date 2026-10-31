//! MusicXML: every demo goes out and comes back, files written by other programs (two staves,
//! no tablature data, compressed) import, and hostile bytes are refused without a panic.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use guitario_musicxml::{MusicXmlError, export, import};
use guitario_score::{Beat, NoteKind, Score};
use std::path::PathBuf;

fn demos() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../demos")
}

fn scores() -> Vec<(String, Score)> {
    let mut out: Vec<(String, Score)> = std::fs::read_dir(demos())
        .unwrap()
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "gt").then(|| (p.file_stem().unwrap().to_string_lossy().to_string(), guitario_format::open(&p).unwrap()))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// What MusicXML carries of a beat: its length, rest, notes with strings, frets and kinds.
fn shape(b: &Beat) -> (u32, bool, Vec<(u8, u8, NoteKind)>) {
    let mut n: Vec<(u8, u8, NoteKind)> = b.notes.iter().map(|n| (n.string, n.fret, n.kind)).collect();
    n.sort_unstable_by_key(|x| (x.0, x.1));
    (b.duration.ticks(), b.rest, n)
}

#[test]
fn every_demo_comes_back_bar_for_bar() {
    for (name, s) in scores() {
        let out = export(&s);
        let back = import(out.xml.as_bytes()).unwrap_or_else(|e| panic!("{name}: {e}")).score;
        assert_eq!(back.headers.len(), s.headers.len(), "{name}");
        assert_eq!(back.tracks.len(), s.tracks.len(), "{name}");
        assert_eq!((back.tempo, &back.meta.title), (s.tempo, &s.meta.title), "{name}");
        for (mi, (a, b)) in s.headers.iter().zip(&back.headers).enumerate() {
            let what = |h: &guitario_score::MeasureHeader| {
                (
                    h.time_signature,
                    h.key,
                    h.tempo,
                    h.repeat_open,
                    h.repeat_close,
                    h.endings,
                    h.marker.clone(),
                    h.section_letter.clone(),
                    h.jump,
                    h.targets.clone(),
                    h.double_bar,
                    h.triplet_feel,
                )
            };
            assert_eq!(what(a), what(b), "{name} bar {}", mi + 1);
        }
        for (ti, (ta, tb)) in s.tracks.iter().zip(&back.tracks).enumerate() {
            assert_eq!(
                (&ta.name, ta.kind, &ta.tuning.strings, ta.capo, ta.program),
                (&tb.name, tb.kind, &tb.tuning.strings, tb.capo, tb.program),
                "{name} track {ti}"
            );
            for (mi, (ma, mb)) in ta.measures.iter().zip(&tb.measures).enumerate() {
                for (vi, va) in ma.voices.iter().enumerate() {
                    if vi > 0 && va.beats.iter().all(|b| b.rest) {
                        continue;
                    }
                    let vb = mb.voices.get(vi).unwrap_or_else(|| panic!("{name} track {ti} bar {} voice {vi} missing", mi + 1));
                    let sa: Vec<_> = va.beats.iter().map(shape).collect();
                    let sb: Vec<_> = vb.beats.iter().map(shape).collect();
                    assert_eq!(sa, sb, "{name} track {ti} bar {} voice {}", mi + 1, vi + 1);
                    for (ba, bb) in va.beats.iter().zip(&vb.beats) {
                        assert_eq!(ba.duration, bb.duration, "{name} track {ti} bar {}", mi + 1);
                        if !ba.rest && vi == 0 {
                            assert_eq!(ba.dynamic, bb.dynamic, "{name} track {ti} bar {}", mi + 1);
                        }
                        assert_eq!(
                            (ba.text.as_deref(), ba.stroke, ba.tremolo, ba.technique),
                            (bb.text.as_deref(), bb.stroke, bb.tremolo, bb.technique),
                            "{name} track {ti} bar {}",
                            mi + 1
                        );
                        assert_eq!(ba.chord.as_ref().map(|c| &c.name), bb.chord.as_ref().map(|c| &c.name), "{name} track {ti} bar {}", mi + 1);
                        for (na, nb) in ba.notes.iter().zip(&bb.notes) {
                            let (ea, eb) = (&na.effects, &nb.effects);
                            let flags = |e: &guitario_score::NoteEffects| {
                                (
                                    (e.hammer, e.slide, e.harmonic, e.vibrato, e.wide_vibrato, e.palm_mute, e.let_ring),
                                    (e.ghost, e.accent, e.heavy_accent, e.staccato, e.tapped, e.left_finger, e.right_finger),
                                )
                            };
                            assert_eq!(flags(ea), flags(eb), "{name} track {ti} bar {} string {}", mi + 1, na.string);
                            assert_eq!(ea.bend.map(|b| b.peak()), eb.bend.map(|b| b.peak()), "{name} track {ti} bar {} bend", mi + 1);
                            assert_eq!(ea.grace.map(|g| g.fret), eb.grace.map(|g| g.fret), "{name} track {ti} bar {} grace", mi + 1);
                        }
                    }
                }
            }
            assert_eq!(
                ta.lyrics.iter().map(|l| guitario_score::syllables(&l.text)).collect::<Vec<_>>(),
                tb.lyrics.iter().map(|l| guitario_score::syllables(&l.text)).collect::<Vec<_>>(),
                "{name} track {ti} lyrics"
            );
        }
    }
}

#[test]
fn the_export_is_well_formed_and_names_its_tuning() {
    let (_, s) = scores().into_iter().find(|(n, _)| n.contains("toccata")).unwrap();
    let xml = export(&s).xml;
    roxmltree::Document::parse_with_options(&xml, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }).unwrap();
    assert!(xml.contains(r#"<score-partwise version="4.0">"#));
    assert!(xml.contains(r#"<staff-tuning line="1"><tuning-step>A</tuning-step><tuning-octave>1</tuning-octave></staff-tuning>"#));
    assert!(xml.contains("<technical><string>"));
}

/// A guitar part as notation programs write it: a standard staff and a tab staff with the same
/// notes, and a second part without any string or fret.
const TWO_STAVES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN" "http://www.musicxml.org/dtds/partwise.dtd">
<score-partwise version="4.0">
  <work><work-title>Two Staves</work-title></work>
  <part-list>
    <score-part id="P1"><part-name>Guitar</part-name><midi-instrument id="P1-I1"><midi-channel>1</midi-channel><midi-program>31</midi-program></midi-instrument></score-part>
    <score-part id="P2"><part-name>Melody</part-name></score-part>
  </part-list>
  <part id="P1">
    <measure number="1">
      <attributes><divisions>2</divisions><key><fifths>1</fifths></key><time><beats>3</beats><beat-type>4</beat-type></time><staves>2</staves>
        <clef number="1"><sign>G</sign><line>2</line><clef-octave-change>-1</clef-octave-change></clef>
        <clef number="2"><sign>TAB</sign><line>5</line></clef>
        <staff-details number="2"><staff-lines>6</staff-lines>
          <staff-tuning line="1"><tuning-step>D</tuning-step><tuning-octave>2</tuning-octave></staff-tuning>
          <staff-tuning line="2"><tuning-step>A</tuning-step><tuning-octave>2</tuning-octave></staff-tuning>
          <staff-tuning line="3"><tuning-step>D</tuning-step><tuning-octave>3</tuning-octave></staff-tuning>
          <staff-tuning line="4"><tuning-step>G</tuning-step><tuning-octave>3</tuning-octave></staff-tuning>
          <staff-tuning line="5"><tuning-step>B</tuning-step><tuning-octave>3</tuning-octave></staff-tuning>
          <staff-tuning line="6"><tuning-step>E</tuning-step><tuning-octave>4</tuning-octave></staff-tuning>
        </staff-details>
      </attributes>
      <direction><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>96</per-minute></metronome></direction-type><sound tempo="96"/></direction>
      <note><pitch><step>D</step><octave>2</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type><staff>1</staff></note>
      <note><pitch><step>F</step><alter>1</alter><octave>3</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type><staff>1</staff></note>
      <note><pitch><step>A</step><octave>3</octave></pitch><duration>2</duration><voice>1</voice><type>quarter</type><staff>1</staff></note>
      <backup><duration>6</duration></backup>
      <note><pitch><step>D</step><octave>2</octave></pitch><duration>2</duration><voice>5</voice><type>quarter</type><staff>2</staff><notations><technical><string>6</string><fret>0</fret></technical></notations></note>
      <note><pitch><step>F</step><alter>1</alter><octave>3</octave></pitch><duration>2</duration><voice>5</voice><type>quarter</type><staff>2</staff><notations><technical><string>4</string><fret>4</fret></technical></notations></note>
      <note><pitch><step>A</step><octave>3</octave></pitch><duration>2</duration><voice>5</voice><type>quarter</type><staff>2</staff><notations><technical><string>4</string><fret>7</fret></technical></notations></note>
      <barline location="right"><bar-style>light-heavy</bar-style></barline>
    </measure>
  </part>
  <part id="P2">
    <measure number="1">
      <attributes><divisions>3</divisions><time><beats>3</beats><beat-type>4</beat-type></time><clef><sign>G</sign><line>2</line></clef></attributes>
      <note><pitch><step>B</step><octave>4</octave></pitch><duration>1</duration><voice>1</voice><type>eighth</type><time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification></note>
      <note><pitch><step>C</step><octave>5</octave></pitch><duration>1</duration><voice>1</voice><type>eighth</type><time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification></note>
      <note><pitch><step>D</step><octave>5</octave></pitch><duration>1</duration><voice>1</voice><type>eighth</type><time-modification><actual-notes>3</actual-notes><normal-notes>2</normal-notes></time-modification></note>
      <note><rest/><duration>3</duration><voice>1</voice><type>quarter</type></note>
      <note><pitch><step>E</step><octave>4</octave></pitch><duration>3</duration><voice>1</voice><type>quarter</type><lyric number="1"><syllabic>single</syllabic><text>hey</text></lyric></note>
    </measure>
  </part>
</score-partwise>
"#;

#[test]
fn a_two_staff_guitar_part_is_read_from_its_tab_and_a_plain_part_is_fingered() {
    let r = import(TWO_STAVES.as_bytes()).unwrap();
    let s = r.score;
    assert_eq!(s.meta.title, "Two Staves");
    assert_eq!((s.tempo, s.headers[0].key, s.headers[0].time_signature.numerator), (96, 1, 3));
    let g = &s.tracks[0];
    assert_eq!(g.tuning.strings, vec![64, 59, 55, 50, 45, 38], "drop D from the tab staff");
    assert_eq!(g.program, 30);
    let beats = &g.measures[0].voices[0].beats;
    assert_eq!(g.measures[0].voices.len(), 1, "the notation staff is not read twice");
    assert_eq!(beats.iter().map(|b| (b.notes[0].string, b.notes[0].fret)).collect::<Vec<_>>(), vec![(5, 0), (3, 4), (3, 7)]);
    let m = &s.tracks[1];
    let b = &m.measures[0].voices[0].beats;
    assert_eq!(b.len(), 5);
    assert!(b[..3].iter().all(|x| x.duration.tuplet == (3, 2) && !x.rest));
    assert!(b[3].rest);
    // B4 is fret 0 on the second string or fret 4 on the third: either is right, both sound B4.
    let pitch = |b: &Beat| m.tuning.pitch(usize::from(b.notes[0].string), b.notes[0].fret, m.capo).unwrap().0;
    assert_eq!(b.iter().filter(|x| !x.rest).map(pitch).collect::<Vec<_>>(), vec![71, 72, 74, 64]);
    assert_eq!(m.lyrics[0].text, "- - - hey");
}

#[test]
fn a_compressed_file_imports() {
    use std::io::Write;
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("META-INF/container.xml", opts).unwrap();
        zip.write_all(br#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="score/song.musicxml"/></rootfiles></container>"#).unwrap();
        zip.start_file("score/song.musicxml", opts).unwrap();
        zip.write_all(TWO_STAVES.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    let s = import(&buf.into_inner()).unwrap().score;
    assert_eq!(s.tracks.len(), 2);
}

#[test]
fn hostile_files_are_refused_without_a_panic() {
    assert!(matches!(import(b""), Err(MusicXmlError::NotMusicXml | MusicXmlError::Xml(_))));
    assert!(matches!(import(b"<html></html>"), Err(MusicXmlError::NotMusicXml)));
    assert!(matches!(import(b"<score-timewise/>"), Err(MusicXmlError::Unsupported(_))));
    assert!(import(b"<score-partwise/>").is_err(), "no parts");
    assert!(import(b"PK\x03\x04garbage").is_err());
    let lol = r#"<?xml version="1.0"?><!DOCTYPE lolz [<!ENTITY lol "lol"><!ENTITY lol2 "&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;"><!ENTITY lol3 "&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;"><!ENTITY lol4 "&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;&lol3;">]><score-partwise><work><work-title>&lol4;</work-title></work></score-partwise>"#;
    assert!(import(lol.as_bytes()).is_err());
    // Silly numbers: divisions 0, giant durations, strings and frets out of range, 12 voices.
    let silly = TWO_STAVES
        .replace("<divisions>3</divisions>", "<divisions>0</divisions>")
        .replace(
            "<duration>3</duration><voice>1</voice><type>quarter</type><lyric",
            "<duration>999999999999</duration><voice>12</voice><type>quarter</type><lyric",
        )
        .replace("<string>4</string><fret>4</fret>", "<string>40</string><fret>250</fret>")
        .replace("<beats>3</beats>", "<beats>300</beats>");
    if let Ok(r) = import(silly.as_bytes()) {
        r.score.validate().unwrap();
    }
    // Every demo's export, cut short and with bytes changed.
    let mut seed = 0x9e37_79b9_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for (_, s) in scores().into_iter().take(6) {
        let bytes = export(&s).xml.into_bytes();
        for _ in 0..30 {
            let mut b = bytes.clone();
            if next() % 2 == 0 {
                b.truncate((next() as usize) % (b.len() + 1));
            }
            for _ in 0..(next() % 12) {
                if b.is_empty() {
                    break;
                }
                let at = (next() as usize) % b.len();
                b[at] = b"0123456789<>/\" -x"[(next() % 17) as usize];
            }
            if let Ok(r) = import(&b) {
                r.score.validate().unwrap();
            }
        }
    }
}

/// Steel Horizon (a former original demo) as exported here, and the same file opened in MuseScore 4
/// and written back out by it: a file from another program, without string and fret data.
#[test]
fn a_file_written_by_musescore_imports_with_the_same_music() {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let back = import(&std::fs::read(data.join("steel-horizon-musescore.musicxml")).unwrap()).unwrap().score;
    let s = import(&std::fs::read(data.join("steel-horizon.musicxml")).unwrap()).unwrap().score;
    assert_eq!((back.headers.len(), back.tracks.len(), back.tempo, &back.meta.title), (s.headers.len(), s.tracks.len(), s.tempo, &s.meta.title));
    for (ta, tb) in s.tracks.iter().zip(&back.tracks) {
        assert_eq!((&ta.name, &ta.tuning.strings, ta.kind), (&tb.name, &tb.tuning.strings, tb.kind));
        // The same rhythms and pitches; MuseScore leaves out strings and frets, so the fingering may differ.
        let heard = |t: &guitario_score::Track| -> Vec<(u32, Vec<u8>)> {
            t.measures
                .iter()
                .flat_map(|m| m.voices.first())
                .flat_map(|v| &v.beats)
                .map(|b| {
                    let mut p: Vec<u8> =
                        b.notes.iter().map(|n| if t.is_drums() { n.fret } else { t.tuning.pitch(usize::from(n.string), n.fret, t.capo).unwrap().0 }).collect();
                    p.sort_unstable();
                    (b.duration.ticks(), p)
                })
                .collect()
        };
        assert_eq!(heard(ta), heard(tb), "{}", ta.name);
    }
}

#[test]
fn drum_articulations_and_flams_come_back() {
    use guitario_score::{Grace, Note, Track};
    use guitario_theory::{Duration, NoteValue, drums::lane_for};
    let mut s = Score::new();
    s.tracks = vec![Track::drums("Drums", &s.headers)];
    let hit = |midi: u8| Note::new(lane_for(midi), midi);
    let mut flam = hit(38);
    flam.effects.grace = Some(Grace::new(38));
    let mut ghost = hit(91);
    ghost.effects.ghost = true;
    s.tracks[0].measures[0].voices[0].beats = vec![
        Beat { rest: false, notes: vec![hit(97), hit(36)], ..Beat::rest(Duration::QUARTER) },
        Beat { rest: false, notes: vec![hit(92), flam], ..Beat::rest(Duration::QUARTER) },
        Beat { rest: false, notes: vec![ghost], ..Beat::rest(Duration::new(NoteValue::Half)) },
    ];
    let xml = export(&s).xml;
    // Other programs hear the General MIDI drum and see the head.
    assert!(xml.contains("<midi-unpitched>50</midi-unpitched>") && xml.contains("Crash (choke)"), "the choke sounds as the crash");
    assert!(xml.contains("<notehead>circled</notehead>") && xml.contains(r#"<notehead parentheses="yes">diamond</notehead>"#));
    let back = import(xml.as_bytes()).unwrap().score;
    let b = &back.tracks[0].measures[0].voices[0].beats;
    let frets = |i: usize| {
        let mut f: Vec<u8> = b[i].notes.iter().map(|n| n.fret).collect();
        f.sort_unstable();
        f
    };
    assert_eq!((frets(0), frets(1), frets(2)), (vec![36, 97], vec![38, 92], vec![91]));
    assert!(b[1].notes.iter().any(|n| n.fret == 38 && n.effects.grace.is_some_and(|g| g.fret == 38)), "the flam");
    assert!(b[2].notes[0].effects.ghost);
}
