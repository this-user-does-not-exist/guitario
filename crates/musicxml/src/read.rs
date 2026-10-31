//! MusicXML import: score-partwise documents, plain or compressed (`.mxl`).
//!
//! Each part becomes a track. A part with a tablature staff is read from that staff alone (the
//! notation staff of a guitar part usually repeats the same notes); otherwise every staff is read,
//! each MusicXML voice becoming one of the bar's four voices. A note's `<string>` and `<fret>` are
//! kept when they agree with its pitch and the tuning, and other notes are fingered as a MIDI import
//! fingers them.

use crate::write::{KINDS, drum_display};
use crate::{Import, MAX_FILE, MusicXmlError};
use guitario_score::{
    Beat, BeatTechnique, Bend, BendKind, Brush, BrushKind, Chord, DirectionTarget, Dynamic, Fermata, FermataKind, Finger, Grace, GraceDuration,
    GraceTransition, Harmonic, Jump, JumpEnd, LyricLine, MAX_VOICES, Measure, MeasureHeader, Notations, Note, NoteKind, Score, Slide, StrokeDirection, Track,
    TremoloSpeed, Trill, TrillSpeed, TripletFeel, Voice,
};
use guitario_theory::{Duration, NoteValue, TICKS_PER_QUARTER, TimeSignature, Tuning};
use roxmltree::{Document, Node, ParsingOptions};
use std::collections::BTreeMap;
use std::io::Read;

const MAX_PARTS: usize = 64;
const MAX_MEASURES: usize = 20_000;
const MAX_NODES: u32 = 50_000_000;
/// Most syllables kept per line of lyrics.
const MAX_SYLLABLES: usize = 20_000;

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(name))
}

fn children<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(move |c| c.has_tag_name(name))
}

fn text<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(n, name).and_then(|c| c.text()).map(str::trim)
}

fn num<T: std::str::FromStr>(n: Node, name: &str) -> Option<T> {
    text(n, name).and_then(|t| t.parse().ok())
}

/// A decimal such as `1.5` or `-2`, as an `f64` that is finite.
fn dec(n: Node, name: &str) -> Option<f64> {
    text(n, name).and_then(|t| t.parse::<f64>().ok()).filter(|v| v.is_finite())
}

/// The XML text of a file: the root document of an `.mxl` archive, or the bytes themselves.
fn document_text(bytes: &[u8]) -> Result<String, MusicXmlError> {
    if bytes.len() as u64 > MAX_FILE {
        return Err(MusicXmlError::TooLarge);
    }
    if bytes.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| MusicXmlError::NotMusicXml)?;
        let read = |zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str| -> Result<Vec<u8>, MusicXmlError> {
            let file = zip.by_name(name).map_err(|_| MusicXmlError::Damaged(format!("the archive has no {name}")))?;
            let mut out = Vec::new();
            file.take(MAX_FILE + 1).read_to_end(&mut out).map_err(|e| MusicXmlError::Io(e.to_string()))?;
            if out.len() as u64 > MAX_FILE {
                return Err(MusicXmlError::TooLarge);
            }
            Ok(out)
        };
        // META-INF/container.xml names the score; without it, the first XML file outside META-INF.
        let root = read(&mut zip, "META-INF/container.xml").ok().and_then(|c| {
            let c = decode(&c).ok()?;
            let doc = Document::parse(&c).ok()?;
            doc.descendants().find(|n| n.has_tag_name("rootfile")).and_then(|n| n.attribute("full-path")).map(str::to_string)
        });
        let name = match root {
            Some(r) => r,
            None => zip
                .file_names()
                .find(|n| !n.starts_with("META-INF") && (n.ends_with(".xml") || n.ends_with(".musicxml")))
                .map(str::to_string)
                .ok_or(MusicXmlError::NotMusicXml)?,
        };
        let inner = read(&mut zip, &name)?;
        return decode(&inner);
    }
    decode(bytes)
}

/// UTF-8 (with or without a byte-order mark) or UTF-16 with one.
fn decode(bytes: &[u8]) -> Result<String, MusicXmlError> {
    let utf16 = |be: bool| -> String {
        let units: Vec<u16> =
            bytes.get(2..).unwrap_or(&[]).as_chunks::<2>().0.iter().map(|c| if be { u16::from_be_bytes(*c) } else { u16::from_le_bytes(*c) }).collect();
        String::from_utf16_lossy(&units)
    };
    match bytes {
        [0xff, 0xfe, ..] => Ok(utf16(false)),
        [0xfe, 0xff, ..] => Ok(utf16(true)),
        [0xef, 0xbb, 0xbf, rest @ ..] => String::from_utf8(rest.to_vec()).map_err(|_| MusicXmlError::Xml("the text is not UTF-8".into())),
        _ => String::from_utf8(bytes.to_vec()).map_err(|_| MusicXmlError::NotMusicXml),
    }
}

/// Reads a MusicXML document (or an `.mxl` archive) from memory.
pub fn import(bytes: &[u8]) -> Result<Import, MusicXmlError> {
    let xml = document_text(bytes)?;
    let options = ParsingOptions { allow_dtd: true, nodes_limit: MAX_NODES, ..ParsingOptions::default() };
    let doc = Document::parse_with_options(&xml, options).map_err(|e| MusicXmlError::Xml(e.to_string()))?;
    let root = doc.root_element();
    match root.tag_name().name() {
        "score-partwise" => {}
        "score-timewise" => return Err(MusicXmlError::Unsupported("timewise scores (save it as partwise)".into())),
        _ => return Err(MusicXmlError::NotMusicXml),
    }
    let mut r = Reader::default();
    r.read(root)?;
    r.finish()
}

/// What a `<score-part>` declares.
#[derive(Default, Clone)]
struct PartInfo {
    name: String,
    short: String,
    program: Option<u8>,
    channel: Option<u8>,
    volume: Option<u8>,
    pan: Option<i8>,
    /// Drum keys by instrument id.
    unpitched: BTreeMap<String, u8>,
}

/// One note as written, before it is fingered.
#[derive(Clone, Default)]
struct XNote {
    key: Option<u8>,
    string: Option<u8>,
    fret: Option<u8>,
    drum: Option<u8>,
    tie_stop: bool,
    dead: bool,
    effects: guitario_score::NoteEffects,
    /// A tremolo, a stroke, a technique, a brush: written on notes, held by the beat.
    tremolo: Option<TremoloSpeed>,
    stroke: Option<StrokeDirection>,
    technique: Option<BeatTechnique>,
    brush: Option<bool>,
    trill: bool,
    fermata: bool,
    lyrics: Vec<(usize, String, String)>,
}

/// A chord or rest of one voice in one bar.
#[derive(Clone, Default)]
struct XBeat {
    start: u64,
    ticks: u64,
    written: Option<Duration>,
    notes: Vec<XNote>,
    graces: Vec<(XNote, bool, GraceDuration, GraceTransition)>,
    dynamic: Dynamic,
    text: Option<String>,
    chord: Option<Chord>,
}

/// One bar of one part: beats by voice key.
#[derive(Default)]
struct XMeasure {
    voices: BTreeMap<usize, Vec<XBeat>>,
    implicit: bool,
    length: u64,
}

#[derive(Default)]
struct XPart {
    info: PartInfo,
    measures: Vec<XMeasure>,
    tuning: Option<Vec<u8>>,
    capo: u8,
    tab_only: bool,
    drums: bool,
    /// Voice keys in order of first use; the index is the model voice.
    voice_order: Vec<(usize, String)>,
}

#[derive(Default)]
struct Reader {
    score: Score,
    headers: Vec<MeasureHeader>,
    parts: Vec<XPart>,
    warnings: BTreeMap<String, usize>,
    tempo_set: bool,
}

/// Header facts read from the first part, with the bar they belong to.
struct BarFacts {
    tempo: Option<u16>,
}

impl Reader {
    fn warn(&mut self, what: impl Into<String>) {
        *self.warnings.entry(what.into()).or_insert(0) += 1;
    }

    fn read(&mut self, root: Node) -> Result<(), MusicXmlError> {
        self.score = Score::new();
        let meta = &mut self.score.meta;
        if let Some(t) = child(root, "work").and_then(|w| text(w, "work-title")) {
            meta.title = t.to_string();
        }
        if let Some(t) = text(root, "movement-title") {
            if meta.title.is_empty() {
                meta.title = t.to_string();
            } else {
                meta.subtitle = t.to_string();
            }
        }
        if let Some(id) = child(root, "identification") {
            for c in children(id, "creator") {
                let value = c.text().unwrap_or("").trim().to_string();
                match c.attribute("type") {
                    Some("composer") => meta.music = value,
                    Some("lyricist" | "poet") => meta.words = value,
                    Some("artist") => meta.artist = value,
                    Some("transcriber" | "arranger") if meta.author.is_empty() => meta.author = value,
                    _ => {}
                }
            }
            if let Some(r) = text(id, "rights") {
                meta.copyright = r.to_string();
            }
        }
        let mut infos: BTreeMap<String, PartInfo> = BTreeMap::new();
        if let Some(list) = child(root, "part-list") {
            for sp in children(list, "score-part") {
                let Some(id) = sp.attribute("id") else { continue };
                let mut info = PartInfo {
                    name: text(sp, "part-name").unwrap_or("").to_string(),
                    short: text(sp, "part-abbreviation").unwrap_or("").to_string(),
                    ..PartInfo::default()
                };
                for mi in children(sp, "midi-instrument") {
                    if info.channel.is_none() {
                        info.channel = num::<u8>(mi, "midi-channel");
                    }
                    if info.program.is_none() {
                        info.program = num::<u8>(mi, "midi-program").map(|p| p.saturating_sub(1).min(127));
                    }
                    if info.volume.is_none() {
                        info.volume = dec(mi, "volume").map(|v| (v.clamp(0.0, 100.0) * 127.0 / 100.0).round() as u8);
                    }
                    if info.pan.is_none() {
                        info.pan = dec(mi, "pan").map(|p| (p.clamp(-90.0, 90.0) * 63.0 / 90.0).round() as i8);
                    }
                    if let (Some(iid), Some(u)) = (mi.attribute("id"), num::<u8>(mi, "midi-unpitched")) {
                        // An instrument named as one of the kit's articulations (a choked crash
                        // sounds as the crash) is that articulation.
                        let named = children(sp, "score-instrument")
                            .find(|si| si.attribute("id") == Some(iid))
                            .and_then(|si| text(si, "instrument-name"))
                            .and_then(|name| guitario_theory::drums::KIT.iter().find(|p| p.name.eq_ignore_ascii_case(name.trim())))
                            .filter(|p| p.plays == u.saturating_sub(1))
                            .map(|p| p.midi);
                        info.unpitched.insert(iid.to_string(), named.unwrap_or(u.saturating_sub(1).min(127)));
                    }
                }
                infos.insert(id.to_string(), info);
            }
        }
        let parts: Vec<Node> = children(root, "part").collect();
        if parts.len() > MAX_PARTS {
            self.warn(format!("only the first {MAX_PARTS} parts were read"));
        }
        for (pi, part) in parts.into_iter().take(MAX_PARTS).enumerate() {
            let info = part.attribute("id").and_then(|id| infos.get(id)).cloned().unwrap_or_default();
            self.read_part(pi, part, info)?;
        }
        if self.parts.is_empty() {
            return Err(MusicXmlError::Damaged("the score has no parts".into()));
        }
        Ok(())
    }

    fn read_part(&mut self, pi: usize, part: Node, info: PartInfo) -> Result<(), MusicXmlError> {
        let mut xp = XPart { drums: info.channel == Some(10) || !info.unpitched.is_empty(), info, ..XPart::default() };
        let first = pi == 0;
        let mut divisions: u64 = 1;
        let mut transpose: i16 = 0;
        // The staff that is read: the tablature staff when the part has one, else every staff.
        let mut tab_staff: Option<u8> = None;
        let mut staves = 1u8;
        let mut dynamic = Dynamic::Mf;
        let mut ts = TimeSignature::COMMON;
        let mut key = 0i8;
        let mut feel = TripletFeel::None;
        for (mi, m) in children(part, "measure").enumerate() {
            if mi >= MAX_MEASURES {
                self.warn(format!("the song was cut at {MAX_MEASURES} bars"));
                break;
            }
            let mut xm = XMeasure { implicit: m.attribute("implicit") == Some("yes"), ..XMeasure::default() };
            let mut header = if first { MeasureHeader::new(ts) } else { MeasureHeader::new(TimeSignature::COMMON) };
            if first {
                header.key = key;
                header.triplet_feel = feel;
            }
            let mut facts = BarFacts { tempo: None };
            let mut cursor: u64 = 0;
            let mut last_start: u64 = 0;
            let mut pending_text: Option<String> = None;
            let mut pending_chord: Option<Chord> = None;
            let mut graces: Vec<(XNote, bool, GraceDuration, GraceTransition)> = Vec::new();
            let mut last_voice_key: Option<usize> = None;
            let ticks_of = |d: u64, divisions: u64| -> u64 {
                (u128::from(d) * u128::from(TICKS_PER_QUARTER) / u128::from(divisions.max(1))).min(u128::from(u32::MAX)) as u64
            };
            for c in m.children().filter(Node::is_element) {
                match c.tag_name().name() {
                    "attributes" => {
                        if let Some(d) = num::<u64>(c, "divisions").filter(|d| *d > 0 && *d <= 1_000_000) {
                            divisions = d;
                        }
                        if let Some(k) = child(c, "key").and_then(|k| num::<i8>(k, "fifths")) {
                            key = k.clamp(-7, 7);
                            if first {
                                header.key = key;
                            }
                        }
                        if let Some(t) = child(c, "time") {
                            let parsed = num::<u8>(t, "beats").zip(num::<u8>(t, "beat-type")).and_then(|(n, d)| TimeSignature::new(n, d));
                            match parsed {
                                Some(next) => {
                                    ts = next;
                                    if first {
                                        header.time_signature = ts;
                                    }
                                }
                                None => self.warn("a time signature the score cannot hold (compound or over 32 beats) was left as it was"),
                            }
                        }
                        if let Some(s) = num::<u8>(c, "staves") {
                            staves = s.max(1);
                        }
                        for clef in children(c, "clef") {
                            let number = clef.attribute("number").and_then(|n| n.parse().ok()).unwrap_or(1u8);
                            match text(clef, "sign") {
                                Some("TAB") => tab_staff = Some(number),
                                Some("percussion") => xp.drums = true,
                                _ => {}
                            }
                        }
                        for sd in children(c, "staff-details") {
                            let tunings: Vec<(u8, u8)> = children(sd, "staff-tuning")
                                .filter_map(|st| {
                                    let line: u8 = st.attribute("line")?.parse().ok()?;
                                    let step = text(st, "tuning-step")?;
                                    let alter = dec(st, "tuning-alter").unwrap_or(0.0).round() as i16;
                                    let octave: i16 = num(st, "tuning-octave")?;
                                    let key = pitch_key(step, alter, octave)?;
                                    Some((line, key))
                                })
                                .collect();
                            if !tunings.is_empty() {
                                let mut by_line = tunings;
                                by_line.sort_unstable();
                                by_line.dedup_by_key(|t| t.0);
                                // Line 1 is the lowest string; the model keeps the highest first.
                                xp.tuning = Some(by_line.iter().rev().map(|t| t.1).collect());
                            }
                            if let Some(capo) = num::<u8>(sd, "capo") {
                                xp.capo = capo.min(24);
                            }
                        }
                        if let Some(t) = child(c, "transpose") {
                            let chromatic = num::<i16>(t, "chromatic").unwrap_or(0).clamp(-24, 24);
                            let octave = num::<i16>(t, "octave-change").unwrap_or(0).clamp(-3, 3);
                            transpose = chromatic + 12 * octave;
                        }
                    }
                    "direction" => {
                        for dt in children(c, "direction-type") {
                            for e in dt.children().filter(Node::is_element) {
                                match e.tag_name().name() {
                                    "rehearsal" => {
                                        let t = e.text().unwrap_or("").trim().to_string();
                                        if first {
                                            let words = children(c, "direction-type").filter_map(|d| text(d, "words")).next().map(str::to_string);
                                            let is_letter = !t.is_empty() && t.len() <= 2 && t.chars().all(|ch| ch.is_ascii_uppercase());
                                            if is_letter {
                                                header.section_letter = Some(t);
                                                header.marker = Some(words.unwrap_or_default());
                                            } else {
                                                header.marker = Some(t);
                                            }
                                        }
                                    }
                                    "segno" if first => push_target(&mut header, DirectionTarget::Segno),
                                    "coda" if first => push_target(&mut header, DirectionTarget::Coda),
                                    "dynamics" => {
                                        if let Some(d) = e.children().filter(Node::is_element).find_map(|d| dynamic_of(d.tag_name().name())) {
                                            dynamic = d;
                                        }
                                    }
                                    "metronome" => {
                                        if facts.tempo.is_none()
                                            && let Some(bpm) = dec(e, "per-minute")
                                        {
                                            let unit = match text(e, "beat-unit") {
                                                Some("half") => 2.0,
                                                Some("eighth") => 0.5,
                                                Some("whole") => 4.0,
                                                _ => 1.0,
                                            } * if child(e, "beat-unit-dot").is_some() { 1.5 } else { 1.0 };
                                            facts.tempo = Some((bpm * unit).round().clamp(20.0, 400.0) as u16);
                                        }
                                    }
                                    "words" => {
                                        let w = e.text().unwrap_or("").trim();
                                        if w.is_empty() || children(c, "direction-type").any(|d| child(d, "rehearsal").is_some()) {
                                            continue;
                                        }
                                        if let Some(j) = jump_named(w) {
                                            if first {
                                                header.jump = Some(j);
                                            }
                                        } else if let Some(t) = target_named(w) {
                                            if first {
                                                push_target(&mut header, t);
                                            }
                                        } else if w.eq_ignore_ascii_case("swing") || w.to_ascii_lowercase().contains("feel") {
                                        } else {
                                            pending_text = Some(w.chars().take(200).collect());
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        if let Some(s) = child(c, "sound") {
                            if let Some(t) = s.attribute("tempo").and_then(|t| t.parse::<f64>().ok()).filter(|t| t.is_finite()) {
                                facts.tempo = Some(t.round().clamp(20.0, 400.0) as u16);
                            }
                            if let Some(sw) = child(s, "swing")
                                && first
                            {
                                feel = swing_of(sw);
                                header.triplet_feel = feel;
                            }
                        }
                    }
                    "sound" => {
                        if let Some(t) = c.attribute("tempo").and_then(|t| t.parse::<f64>().ok()).filter(|t| t.is_finite()) {
                            facts.tempo = Some(t.round().clamp(20.0, 400.0) as u16);
                        }
                    }
                    "harmony" => pending_chord = harmony_of(c),
                    "backup" => cursor = cursor.saturating_sub(ticks_of(num(c, "duration").unwrap_or(0), divisions)),
                    "forward" => cursor = cursor.saturating_add(ticks_of(num(c, "duration").unwrap_or(0), divisions)),
                    "barline" => {
                        if first {
                            barline(c, &mut header);
                        }
                    }
                    "note" => {
                        let staff: u8 = num(c, "staff").unwrap_or(1);
                        if let Some(tab) = tab_staff.filter(|_| staves > 1)
                            && staff != tab
                        {
                            // The notation staff repeats the tab's notes: only time moves on.
                            if child(c, "chord").is_none() && child(c, "grace").is_none() {
                                last_start = cursor;
                                cursor = cursor.saturating_add(ticks_of(num(c, "duration").unwrap_or(0), divisions));
                            }
                            continue;
                        }
                        let voice_name = text(c, "voice").unwrap_or("1").to_string();
                        let vkey = match xp.voice_order.iter().position(|(s, v)| *s == usize::from(staff) && *v == voice_name) {
                            Some(i) => i,
                            None => {
                                xp.voice_order.push((usize::from(staff), voice_name));
                                xp.voice_order.len() - 1
                            }
                        };
                        let mut xn = self.note_of(c, &xp, transpose);
                        if let Some(g) = child(c, "grace") {
                            let on_beat = g.attribute("slash") == Some("no");
                            let duration = match text(c, "type") {
                                Some("16th") => GraceDuration::Sixteenth,
                                Some("64th") => GraceDuration::SixtyFourth,
                                _ => GraceDuration::ThirtySecond,
                            };
                            let notations = child(c, "notations");
                            let transition = if notations.and_then(|n| child(n, "slide")).is_some() {
                                GraceTransition::Slide
                            } else if notations
                                .and_then(|n| child(n, "technical"))
                                .is_some_and(|t| child(t, "hammer-on").is_some() || child(t, "pull-off").is_some())
                            {
                                GraceTransition::Hammer
                            } else {
                                GraceTransition::None
                            };
                            xn.lyrics.clear();
                            graces.push((xn, on_beat, duration, transition));
                            continue;
                        }
                        let is_chord = child(c, "chord").is_some();
                        let start = if is_chord { last_start } else { cursor };
                        let dur = ticks_of(num(c, "duration").unwrap_or(0), divisions);
                        let rest = child(c, "rest");
                        let written = written_duration(c);
                        let beats = xm.voices.entry(vkey).or_default();
                        let joined = is_chord && beats.last().is_some_and(|b| b.start == start && last_voice_key == Some(vkey));
                        if joined {
                            if let Some(b) = beats.last_mut()
                                && rest.is_none()
                            {
                                b.notes.push(xn);
                            }
                        } else {
                            let mut b = XBeat { start, ticks: dur, written, dynamic, ..XBeat::default() };
                            if rest.is_some() {
                                if rest.and_then(|r| r.attribute("measure")) == Some("yes") || dur == 0 {
                                    b.ticks = dur;
                                    b.written = None;
                                }
                            } else {
                                b.notes.push(xn);
                                b.graces = std::mem::take(&mut graces);
                                b.text = pending_text.take();
                                b.chord = pending_chord.take();
                            }
                            beats.push(b);
                            last_start = start;
                            cursor = cursor.saturating_add(dur);
                        }
                        last_voice_key = Some(vkey);
                    }
                    _ => {}
                }
                xm.length = xm.length.max(cursor);
            }
            if !graces.is_empty() {
                self.warn("grace notes with no note after them were left out");
            }
            if first {
                if let Some(t) = facts.tempo {
                    if !self.tempo_set && mi == 0 {
                        self.score.tempo = t;
                        self.tempo_set = true;
                    } else {
                        // Kept even when it repeats the tempo: the mark is drawn.
                        header.tempo = Some(t);
                    }
                }
                // Fermatas over the bar from the notes that carry one.
                let mut ticks: Vec<u32> =
                    xm.voices.values().flatten().filter(|b| b.notes.iter().any(|n| n.fermata)).map(|b| b.start.min(u64::from(u32::MAX)) as u32).collect();
                ticks.sort_unstable();
                ticks.dedup();
                header.fermatas = ticks
                    .into_iter()
                    .filter(|t| *t < header.capacity())
                    .take(guitario_score::MAX_FERMATAS)
                    .map(|tick| Fermata { tick, kind: FermataKind::Medium, length: 0 })
                    .collect();
                self.headers.push(header);
            }
            xp.measures.push(xm);
        }
        xp.tab_only = tab_staff.is_some() && staves == 1;
        self.parts.push(xp);
        Ok(())
    }

    fn note_of(&mut self, n: Node, xp: &XPart, transpose: i16) -> XNote {
        let mut x = XNote::default();
        if let Some(p) = child(n, "pitch") {
            let alter = dec(p, "alter").unwrap_or(0.0).round() as i16;
            x.key = text(p, "step")
                .zip(num::<i16>(p, "octave"))
                .and_then(|(s, o)| pitch_key(s, alter, o))
                .and_then(|k| u8::try_from(i16::from(k) + transpose).ok())
                .filter(|k| *k <= 127);
        }
        if let Some(u) = child(n, "unpitched") {
            let from_instrument = child(n, "instrument").and_then(|i| i.attribute("id")).and_then(|id| xp.info.unpitched.get(id)).copied();
            let from_staff = || {
                let step = text(u, "display-step")?;
                let octave: i32 = num(u, "display-octave")?;
                let cross = matches!(text(n, "notehead"), Some("x" | "circle-x" | "circled"));
                let pieces = guitario_theory::drums::KIT;
                let at = |p: &guitario_theory::drums::DrumPiece| drum_display(p.staff_pos) == (step.chars().next().unwrap_or('E'), octave);
                pieces.iter().find(|p| at(p) && p.cross() == cross).or_else(|| pieces.iter().find(|p| at(p))).map(|p| p.midi)
            };
            x.drum = from_instrument.or_else(from_staff).or(Some(38));
        } else if xp.drums
            && let Some(id) = child(n, "instrument").and_then(|i| i.attribute("id"))
        {
            x.drum = xp.info.unpitched.get(id).copied().or(x.key);
        }
        x.tie_stop = children(n, "tie").any(|t| t.attribute("type") == Some("stop"));
        match text(n, "notehead") {
            Some("x") if x.drum.is_none() => x.dead = true,
            _ => {}
        }
        if child(n, "notehead").and_then(|h| h.attribute("parentheses")) == Some("yes") {
            x.effects.ghost = true;
        }
        for no in children(n, "notations") {
            for e in no.children().filter(Node::is_element) {
                match e.tag_name().name() {
                    "tied" if e.attribute("type") == Some("stop") => x.tie_stop = true,
                    "slide" if e.attribute("type") == Some("start") => x.effects.slide = Some(Slide::Shift),
                    "glissando" if e.attribute("type") == Some("start") => x.effects.slide = Some(Slide::Legato),
                    "fermata" => x.fermata = true,
                    "arpeggiate" => x.brush = Some(e.attribute("direction") != Some("down")),
                    "technical" => self.technical(e, &mut x),
                    "articulations" => {
                        for a in e.children().filter(Node::is_element) {
                            match a.tag_name().name() {
                                "accent" => x.effects.accent = true,
                                "strong-accent" => x.effects.heavy_accent = true,
                                "staccato" | "staccatissimo" => x.effects.staccato = true,
                                "scoop" => x.effects.slide = Some(Slide::IntoFromBelow),
                                "plop" => x.effects.slide = Some(Slide::IntoFromAbove),
                                "doit" => x.effects.slide = Some(Slide::OutUp),
                                "falloff" => x.effects.slide = Some(Slide::OutDown),
                                "other-articulation" if a.attribute("smufl").is_some_and(|s| s.contains("WideVibrato")) => x.effects.wide_vibrato = true,
                                "other-articulation" if a.attribute("smufl").is_some_and(|s| s.contains("Vibrato")) => x.effects.vibrato = true,
                                _ => {}
                            }
                        }
                    }
                    "ornaments" => {
                        for o in e.children().filter(Node::is_element) {
                            match o.tag_name().name() {
                                "wavy-line" if o.attribute("type") == Some("start") => {
                                    if o.attribute("smufl").is_some_and(|s| s.contains("Wide")) {
                                        x.effects.wide_vibrato = true;
                                    } else {
                                        x.effects.vibrato = true;
                                    }
                                }
                                "vibrato" => x.effects.vibrato = true,
                                // SMuFL's guitar vibrato symbols, as notation programs write them.
                                "other-ornament" | "other-articulation" => match o.attribute("smufl").unwrap_or("") {
                                    s if s.contains("WideVibrato") => x.effects.wide_vibrato = true,
                                    s if s.contains("Vibrato") => x.effects.vibrato = true,
                                    _ if o.text().is_some_and(|t| t.contains("wide")) => x.effects.wide_vibrato = true,
                                    _ => {}
                                },
                                "trill-mark" => x.trill = true,
                                "tremolo" if o.attribute("type").is_none_or(|t| t == "single") => {
                                    x.tremolo = o.text().and_then(|t| t.trim().parse::<u8>().ok()).and_then(TremoloSpeed::from_slashes);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        for l in children(n, "lyric") {
            let line: usize = l.attribute("number").and_then(|v| v.parse().ok()).unwrap_or(1);
            if let (Some(t), true) = (text(l, "text"), (1..=guitario_score::MAX_LYRIC_LINES).contains(&line)) {
                x.lyrics.push((line - 1, text(l, "syllabic").unwrap_or("single").to_string(), t.to_string()));
            }
        }
        x
    }

    fn technical(&mut self, t: Node, x: &mut XNote) {
        let mut bends: Vec<(f64, bool, bool)> = Vec::new();
        for e in t.children().filter(Node::is_element) {
            let value = e.text().unwrap_or("").trim();
            match e.tag_name().name() {
                "string" => x.string = value.parse::<u8>().ok().and_then(|s| s.checked_sub(1)),
                "fret" => x.fret = value.parse().ok(),
                "hammer-on" | "pull-off" if e.attribute("type") == Some("start") => x.effects.hammer = true,
                "bend" => {
                    let alter = dec(e, "bend-alter").unwrap_or(0.0);
                    bends.push((alter, child(e, "pre-bend").is_some(), child(e, "release").is_some() || alter < 0.0));
                }
                "harmonic" => {
                    x.effects.harmonic = Some(if child(e, "artificial").is_some() { Harmonic::Artificial } else { Harmonic::Natural });
                }
                "tap" => {
                    if e.attribute("hand") == Some("left") {
                        x.effects.left_hand_tapped = true;
                    } else {
                        x.effects.tapped = true;
                    }
                }
                "fingering" => x.effects.left_finger = Finger::ALL.into_iter().find(|f| f.left_mark().eq_ignore_ascii_case(value)),
                "pluck" => x.effects.right_finger = Finger::ALL.into_iter().find(|f| f.right_mark().eq_ignore_ascii_case(value)),
                "up-bow" => x.stroke = Some(StrokeDirection::Up),
                "down-bow" => x.stroke = Some(StrokeDirection::Down),
                "other-technical" => {
                    let v = value.to_ascii_lowercase();
                    if v.contains("palm") || v == "p.m." || v == "pm" {
                        x.effects.palm_mute = true;
                    } else if v.contains("let ring") {
                        x.effects.let_ring = true;
                    } else if v.contains("pinch") {
                        x.effects.harmonic = Some(Harmonic::Pinch);
                    } else if v.contains("tapped harmonic") {
                        x.effects.harmonic = Some(Harmonic::Tapped);
                    } else if v.contains("semi") {
                        x.effects.harmonic = Some(Harmonic::Semi);
                    } else if v == "slap" {
                        x.technique = Some(BeatTechnique::Slap);
                    } else if v == "pop" {
                        x.technique = Some(BeatTechnique::Pop);
                    }
                }
                _ => {}
            }
        }
        if !bends.is_empty() {
            x.effects.bend = bend_of(&bends, x.tie_stop);
        }
    }

    fn finish(mut self) -> Result<Import, MusicXmlError> {
        let bars = self.parts.iter().map(|p| p.measures.len()).max().unwrap_or(0).clamp(1, MAX_MEASURES);
        while self.headers.len() < bars {
            let last = self.headers.last().cloned().unwrap_or_else(|| MeasureHeader::new(TimeSignature::COMMON));
            let mut h = MeasureHeader::new(last.time_signature);
            h.key = last.key;
            h.triplet_feel = last.triplet_feel;
            self.headers.push(h);
        }
        self.headers.truncate(bars);
        let headers = std::mem::take(&mut self.headers);
        let mut tracks = Vec::new();
        let parts = std::mem::take(&mut self.parts);
        for (pi, part) in parts.iter().enumerate() {
            tracks.push(self.track_of(pi, part, &headers));
        }
        // A pickup bar is shorter than its time signature: its notes move to the end of the bar.
        let mut score = std::mem::take(&mut self.score);
        score.headers = headers;
        score.tracks = tracks;
        score.validate()?;
        let warnings = self.warnings.iter().map(|(w, n)| if *n > 1 { format!("{w} ({n}×)") } else { w.clone() }).collect();
        Ok(Import { score, warnings })
    }

    fn track_of(&mut self, pi: usize, part: &XPart, headers: &[MeasureHeader]) -> Track {
        let name = if part.info.name.is_empty() { format!("Track {}", pi + 1) } else { part.info.name.clone() };
        let program = part.info.program.unwrap_or(25);
        let mut track = if part.drums {
            Track::drums(name, headers)
        } else {
            let keys: Vec<u8> = part.measures.iter().flat_map(|m| m.voices.values()).flatten().flat_map(|b| &b.notes).filter_map(|n| n.key).collect();
            let tuning = part
                .tuning
                .clone()
                .and_then(|strings| {
                    let name = Tuning::presets().into_iter().find(|t| t.strings == strings).map_or_else(|| "Custom".to_string(), |t| t.name);
                    Tuning::new(name, strings).ok()
                })
                .unwrap_or_else(|| guitario_midi::tuning_for(program, &keys).0);
            let mut t = Track::new(name, tuning, headers);
            t.program = program;
            t.capo = part.capo;
            if part.tab_only {
                t.notation = Notations { standard: false, tab: true, ..t.notation };
            }
            t
        };
        track.short_name = part.info.short.clone();
        if let Some(v) = part.info.volume {
            track.volume = v.min(127);
        }
        if let Some(p) = part.info.pan {
            track.pan = p.clamp(-63, 63);
        }
        let mut hand = 0.0f32;
        let mut lyric_marks: Vec<Vec<(usize, usize, String, String)>> = vec![Vec::new(); guitario_score::MAX_LYRIC_LINES];
        let mut overflow = 0usize;
        let mut dropped_voices = 0usize;
        for (mi, (xm, header)) in part.measures.iter().zip(headers).enumerate() {
            let cap = u64::from(header.capacity());
            let mut voices: Vec<Voice> = Vec::new();
            for (vkey, xbeats) in &xm.voices {
                if *vkey >= MAX_VOICES {
                    dropped_voices += 1;
                    continue;
                }
                while voices.len() <= *vkey {
                    voices.push(Voice::default());
                }
                // A pickup bar's notes sit at its end.
                let shift = if xm.implicit && mi == 0 && xm.length < cap { cap - xm.length } else { 0 };
                let mut at = 0u64;
                let mut prev_strings: BTreeMap<u8, u8> = BTreeMap::new();
                let Some(voice) = voices.get_mut(*vkey) else { continue };
                for xb in xbeats {
                    let start = xb.start + shift;
                    if start < at {
                        overflow += 1;
                        continue;
                    }
                    if start > at {
                        for d in fill(start - at) {
                            voice.beats.push(Beat::rest(d));
                        }
                        at = start;
                    }
                    let mut ticks = if xb.ticks == 0 && xb.notes.is_empty() { cap.saturating_sub(at) } else { xb.ticks };
                    if at + ticks > cap {
                        overflow += usize::from(at + ticks > cap + 1);
                        ticks = cap.saturating_sub(at);
                    }
                    if ticks == 0 {
                        continue;
                    }
                    let pieces = match xb.written.filter(|d| u64::from(d.ticks()) == ticks) {
                        Some(d) => vec![d],
                        None => fill(ticks),
                    };
                    let placed = if xb.notes.is_empty() { Vec::new() } else { self.place(&track, xb, &mut hand, &mut prev_strings) };
                    for (i, d) in pieces.into_iter().enumerate() {
                        let mut beat = Beat::rest(d);
                        if !placed.is_empty() {
                            beat.rest = false;
                            beat.notes =
                                placed.iter().map(|(n, _)| if i == 0 { *n } else { Note { kind: NoteKind::Tie, effects: Default::default(), ..*n } }).collect();
                            beat.dynamic = xb.dynamic;
                            if i == 0 {
                                beat.text = xb.text.clone();
                                beat.chord = xb.chord.clone();
                                if let Some((first, _)) = placed.first().map(|p| (p.1.clone(), ())) {
                                    beat.tremolo = first.tremolo;
                                    beat.stroke = xb.notes.iter().find_map(|n| n.stroke);
                                    beat.technique = xb.notes.iter().find_map(|n| n.technique);
                                    beat.brush =
                                        xb.notes.iter().find_map(|n| n.brush).map(|down| Brush { kind: BrushKind::Brush, down, ticks: TICKS_PER_QUARTER / 4 });
                                }
                                if *vkey == 0 {
                                    let bi = voice.beats.len();
                                    for (_, xn) in &placed {
                                        for (line, syllabic, text) in &xn.lyrics {
                                            if let Some(marks) = lyric_marks.get_mut(*line)
                                                && marks.len() < MAX_SYLLABLES
                                            {
                                                marks.push((mi, bi, syllabic.clone(), text.clone()));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        voice.beats.push(beat);
                    }
                    at += ticks;
                }
                if *vkey == 0 && at < cap {
                    for d in fill(cap - at) {
                        voice.beats.push(Beat::rest(d));
                    }
                }
            }
            if voices.is_empty() || voices.first().is_some_and(|v| v.beats.is_empty()) {
                continue;
            }
            for v in &mut voices {
                if v.beats.is_empty() {
                    v.beats = fill(cap).into_iter().map(Beat::rest).collect();
                }
            }
            if let Some(m) = track.measures.get_mut(mi) {
                *m = Measure { voices, ..Measure::default() };
            }
        }
        if overflow > 0 {
            self.warn(format!("{}: notes that overlap or run past the bar line were cut", track.name));
        }
        if dropped_voices > 0 {
            self.warn(format!("{}: voices beyond the fourth were left out", track.name));
        }
        track.lyrics = lyric_lines(&track, &lyric_marks);
        track
    }

    /// Strings and frets for one chord: the file's when they agree with the pitch and the tuning,
    /// else fingered. Each placed note keeps the note it came from.
    fn place(&mut self, track: &Track, xb: &XBeat, hand: &mut f32, prev_strings: &mut BTreeMap<u8, u8>) -> Vec<(Note, XNote)> {
        let mut out: Vec<(Note, XNote)> = Vec::new();
        if track.is_drums() {
            for xn in &xb.notes {
                let Some(key) = xn.drum.or(xn.key) else { continue };
                if out.iter().any(|(n, _)| n.fret == key) {
                    continue;
                }
                let lane = guitario_theory::drums::lane_for(key);
                let free = (0..8u8).filter(|l| !out.iter().any(|(n, _)| n.string == *l)).min_by_key(|l| (i16::from(*l) - i16::from(lane)).abs());
                if let Some(l) = free {
                    out.push((self.note(Note::new(l, key), xn, None), xn.clone()));
                }
            }
            // Hits are kept in lane order, as entering them leaves them.
            out.sort_by_key(|(n, _)| n.string);
            // A flam: the grace note joins the hit on its lane, or the first hit.
            for (g, on_beat, duration, transition) in &xb.graces {
                let Some(drum) = g.drum.or(g.key) else { continue };
                let lane = guitario_theory::drums::lane_for(drum);
                let at = out.iter().position(|(n, _)| n.string == lane).or(if out.is_empty() { None } else { Some(0) });
                match at.and_then(|i| out.get_mut(i)) {
                    Some((n, _)) => n.effects.grace = Some(Grace { fret: drum, duration: *duration, transition: *transition, on_beat: *on_beat, dead: g.dead }),
                    None => self.warn("grace notes with no drum hit after them were left out"),
                }
            }
            return out;
        }
        let strings = &track.tuning.strings;
        let agrees = |xn: &XNote| -> Option<(u8, u8)> {
            let (s, f) = (xn.string?, xn.fret?);
            let pitch = track.tuning.pitch(usize::from(s), f, track.capo)?;
            (f <= track.max_fret() && xn.key.is_none_or(|k| k == pitch.0)).then_some((s, f))
        };
        let given: Vec<Option<(u8, u8)>> = xb.notes.iter().map(agrees).collect();
        let distinct = {
            let mut used: Vec<u8> = given.iter().flatten().map(|g| g.0).collect();
            let n = used.len();
            used.sort_unstable();
            used.dedup();
            used.len() == n
        };
        if distinct && given.iter().all(Option::is_some) {
            for (xn, g) in xb.notes.iter().zip(given) {
                if let Some((s, f)) = g {
                    out.push((self.note(Note::new(s, f), xn, Some(prev_strings)), xn.clone()));
                }
            }
        } else {
            let (lo, hi) = guitario_midi::range(&track.tuning);
            let capo = track.capo;
            // Fingered without the capo: the frets count from it.
            let mut keyed: Vec<(u8, &XNote)> =
                xb.notes.iter().filter_map(|xn| xn.key.map(|k| (guitario_midi::into_range(k.saturating_sub(capo), lo, hi), xn))).collect();
            keyed.sort_by_key(|k| std::cmp::Reverse(k.0));
            keyed.dedup_by_key(|k| k.0);
            let keys: Vec<u8> = keyed.iter().map(|k| k.0).collect();
            let (notes, missing) = guitario_midi::finger(&Tuning { name: String::new(), strings: strings.clone() }, &keys, hand);
            if missing > 0 {
                self.warn(format!("{}: notes that found no free string were left out", track.name));
            }
            let mut seen: Vec<u8> = Vec::new();
            for mut n in notes {
                let Some(k) = strings.get(usize::from(n.string)).map(|open| open.saturating_add(n.fret)) else { continue };
                let Some(&(_, xn)) = keyed.iter().find(|(kk, _)| *kk == k) else { continue };
                // A tied note stays on the string it was tied from when that string is free.
                if xn.tie_stop
                    && let Some(&s) = prev_strings.get(&k.saturating_add(capo))
                    && s != n.string
                    && !seen.contains(&s)
                    && let Some(open) = strings.get(usize::from(s)).copied()
                    && k >= open
                    && k - open <= 36
                {
                    n.string = s;
                    n.fret = k - open;
                }
                if seen.contains(&n.string) {
                    continue;
                }
                seen.push(n.string);
                out.push((self.note(n, xn, None), xn.clone()));
            }
        }
        // Grace notes join the main note on their string, or the first note.
        for (g, on_beat, duration, transition) in &xb.graces {
            let fret = g.fret.or_else(|| {
                let k = g.key?;
                let s = out.first()?.0.string;
                let open = *strings.get(usize::from(g.string.unwrap_or(s)))?;
                k.checked_sub(open + track.capo)
            });
            let target = out.iter_mut().find(|(n, _)| Some(n.string) == g.string).map(|o| &mut o.0);
            if let (Some(n), Some(fret)) = (target, fret.filter(|f| *f <= track.max_fret())) {
                n.effects.grace = Some(Grace { fret, duration: *duration, transition: *transition, on_beat: *on_beat, dead: g.dead });
            } else {
                self.warn("grace notes on a string with no main note were left out");
            }
        }
        for (n, _) in &out {
            if let Some(k) = track.tuning.pitch(usize::from(n.string), n.fret, track.capo) {
                prev_strings.insert(k.0, n.string);
            }
        }
        out
    }

    /// A placed note with the written note's kind and effects.
    fn note(&mut self, mut n: Note, xn: &XNote, _prev: Option<&mut BTreeMap<u8, u8>>) -> Note {
        n.effects = xn.effects;
        if xn.tie_stop {
            n.kind = NoteKind::Tie;
        } else if xn.dead {
            n.kind = NoteKind::Dead;
        }
        if xn.trill {
            n.effects.trill = Some(Trill { fret: n.fret.saturating_add(2).min(36), speed: TrillSpeed::Sixteenth });
        }
        n
    }
}

fn push_target(h: &mut MeasureHeader, t: DirectionTarget) {
    if !h.targets.contains(&t) {
        h.targets.push(t);
    }
}

fn dynamic_of(name: &str) -> Option<Dynamic> {
    Some(match name {
        "ppp" | "pppp" | "ppppp" | "pppppp" => Dynamic::Ppp,
        "pp" => Dynamic::Pp,
        "p" => Dynamic::P,
        "mp" => Dynamic::Mp,
        "mf" => Dynamic::Mf,
        "f" | "sf" | "sfz" | "fz" => Dynamic::F,
        "ff" => Dynamic::Ff,
        "fff" | "ffff" | "fffff" | "ffffff" => Dynamic::Fff,
        _ => return None,
    })
}

fn jump_named(w: &str) -> Option<Jump> {
    let ends = [JumpEnd::End, JumpEnd::Fine, JumpEnd::Coda, JumpEnd::DoubleCoda];
    let all = ends.iter().flat_map(|e| [Jump::DaCapo(*e), Jump::DalSegno(*e), Jump::DalSegnoSegno(*e)]).chain([Jump::ToCoda, Jump::ToDoubleCoda]);
    let wanted = w.trim().trim_end_matches('.').to_ascii_lowercase();
    all.into_iter().find(|j| j.label().trim_end_matches('.').to_ascii_lowercase() == wanted)
}

fn target_named(w: &str) -> Option<DirectionTarget> {
    match w.trim().to_ascii_lowercase().as_str() {
        "fine" => Some(DirectionTarget::Fine),
        "segno segno" => Some(DirectionTarget::SegnoSegno),
        "double coda" => Some(DirectionTarget::DoubleCoda),
        _ => None,
    }
}

fn swing_of(sw: Node) -> TripletFeel {
    if child(sw, "straight").is_some() {
        return TripletFeel::None;
    }
    let first: u32 = num(sw, "first").unwrap_or(2);
    let second: u32 = num(sw, "second").unwrap_or(1);
    let sixteenth = text(sw, "swing-type") == Some("16th");
    match (first.cmp(&second), first * 2 >= second * 5, sixteenth) {
        (std::cmp::Ordering::Less, _, false) => TripletFeel::Scottish8th,
        (std::cmp::Ordering::Less, _, true) => TripletFeel::Scottish16th,
        (std::cmp::Ordering::Equal, _, _) => TripletFeel::None,
        (std::cmp::Ordering::Greater, true, false) => TripletFeel::Dotted8th,
        (std::cmp::Ordering::Greater, true, true) => TripletFeel::Dotted16th,
        (std::cmp::Ordering::Greater, false, false) => TripletFeel::Triplet8th,
        (std::cmp::Ordering::Greater, false, true) => TripletFeel::Triplet16th,
    }
}

fn barline(b: Node, h: &mut MeasureHeader) {
    let left = b.attribute("location") == Some("left");
    if let Some(r) = child(b, "repeat") {
        match r.attribute("direction") {
            Some("forward") => h.repeat_open = true,
            Some("backward") => {
                let times: u16 = r.attribute("times").and_then(|t| t.parse().ok()).unwrap_or(2);
                h.repeat_close = times.saturating_sub(1).clamp(1, 99) as u8;
            }
            _ => {}
        }
    }
    if let Some(e) = child(b, "ending")
        && (e.attribute("type") == Some("start") || !left)
    {
        for n in e.attribute("number").unwrap_or("").split([',', ' ']).filter_map(|n| n.trim().parse::<u8>().ok()) {
            if (1..=8).contains(&n) {
                h.endings |= 1 << (n - 1);
            }
        }
    }
    if text(b, "bar-style") == Some("light-light") && !left {
        h.double_bar = true;
    }
}

fn harmony_of(h: Node) -> Option<Chord> {
    let root = child(h, "root")?;
    let step = text(root, "root-step")?;
    let alter = dec(root, "root-alter").unwrap_or(0.0).round() as i8;
    let accidental = match alter {
        1 => "#",
        -1 => "b",
        _ => "",
    };
    let kind = child(h, "kind");
    let suffix = kind
        .and_then(|k| k.attribute("text").map(str::to_string))
        .or_else(|| kind.and_then(|k| k.text()).and_then(|v| KINDS.iter().find(|(_, name)| *name == v.trim()).map(|(s, _)| s.to_string())))
        .unwrap_or_default();
    let mut name = format!("{step}{accidental}{suffix}");
    if let Some(b) = child(h, "bass") {
        let bstep = text(b, "bass-step").unwrap_or("");
        let balter = match dec(b, "bass-alter").unwrap_or(0.0).round() as i8 {
            1 => "#",
            -1 => "b",
            _ => "",
        };
        name = format!("{name}/{bstep}{balter}");
    }
    let mut chord = Chord::named(name.chars().take(40).collect::<String>());
    if let Some(frame) = child(h, "frame") {
        let strings: usize = num::<usize>(frame, "frame-strings").unwrap_or(6).clamp(1, Tuning::MAX_STRINGS);
        let mut frets = vec![None; strings];
        for fnote in children(frame, "frame-note") {
            if let (Some(s), Some(f)) = (num::<usize>(fnote, "string"), num::<u8>(fnote, "fret"))
                && let Some(slot) = s.checked_sub(1).and_then(|i| frets.get_mut(i))
            {
                *slot = Some(f.min(36));
            }
        }
        chord.frets = frets;
        chord.first_fret = num::<u8>(frame, "first-fret").unwrap_or(1).clamp(1, 36);
    }
    Some(chord)
}

fn pitch_key(step: &str, alter: i16, octave: i16) -> Option<u8> {
    let base: i16 = match step.trim() {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        "B" => 11,
        _ => return None,
    };
    let k = (octave.clamp(-2, 10) + 1) * 12 + base + alter.clamp(-2, 2);
    u8::try_from(k).ok().filter(|k| *k <= 127)
}

/// The written duration: type, dots and tuplet.
fn written_duration(n: Node) -> Option<Duration> {
    let value = match text(n, "type")? {
        "whole" => NoteValue::Whole,
        "half" => NoteValue::Half,
        "quarter" => NoteValue::Quarter,
        "eighth" => NoteValue::Eighth,
        "16th" => NoteValue::Sixteenth,
        "32nd" => NoteValue::ThirtySecond,
        "64th" => NoteValue::SixtyFourth,
        _ => return None,
    };
    let dots = children(n, "dot").count().min(2) as u8;
    let tuplet = child(n, "time-modification")
        .and_then(|t| Some((num::<u8>(t, "actual-notes")?, num::<u8>(t, "normal-notes")?)))
        .filter(|(a, b)| *a > 0 && *b > 0)
        .unwrap_or((1, 1));
    Some(Duration { value, dots, tuplet })
}

/// Durations filling `ticks`, longest first: plain and dotted values, then tuplets of the common
/// ratios for what is left.
fn fill(ticks: u64) -> Vec<Duration> {
    let mut out = Vec::new();
    let mut left = ticks;
    let mut choices: Vec<Duration> = NoteValue::ALL.into_iter().flat_map(|v| [Duration::new(v).dotted(1), Duration::new(v)]).collect();
    for ratio in [(3u8, 2u8), (5, 4), (6, 4), (7, 4), (9, 8)] {
        choices.extend(NoteValue::ALL.into_iter().map(|v| Duration { tuplet: ratio, ..Duration::new(v) }));
    }
    while left > 0 && out.len() < 64 {
        let Some(d) = choices.iter().copied().filter(|d| u64::from(d.ticks()) <= left).max_by_key(|d| d.ticks()) else { break };
        out.push(d);
        left -= u64::from(d.ticks());
    }
    out
}

fn bend_of(bends: &[(f64, bool, bool)], tied: bool) -> Option<Bend> {
    let q = |semis: f64| (semis.abs() * 2.0).round().clamp(1.0, f64::from(guitario_score::BEND_MAX)) as u8;
    let kind = match bends {
        [(a, true, _), (b, false, false), ..] => return Some(Bend::preset(BendKind::PrebendBend, q(*a + *b))),
        [(a, true, _), (_, _, true), ..] => (BendKind::PrebendRelease, q(*a)),
        [(a, true, _)] => (if tied { BendKind::Hold } else { BendKind::Prebend }, q(*a)),
        [(a, false, false), (_, _, true), ..] => (BendKind::BendRelease, q(*a)),
        [(a, _, true), ..] => (BendKind::Release, q(*a)),
        [(a, _, _), ..] => (BendKind::Bend, q(*a)),
        [] => return None,
    };
    Some(Bend::preset(kind.0, kind.1))
}

/// Lines of lyrics from the syllables placed on voice 1: a `-` for each sung beat without one.
fn lyric_lines(track: &Track, marks: &[Vec<(usize, usize, String, String)>]) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    for line in marks {
        let Some(&(first_bar, _, _, _)) = line.first() else {
            lines.push(LyricLine::default());
            continue;
        };
        let at: BTreeMap<(usize, usize), (&str, &str)> = line.iter().map(|(m, b, s, t)| ((*m, *b), (s.as_str(), t.as_str()))).collect();
        let last = line.last().map_or((0, 0), |l| (l.0, l.1));
        let mut text = String::new();
        'bars: for (mi, m) in track.measures.iter().enumerate().skip(first_bar) {
            let Some(v) = m.voices.first() else { continue };
            for (bi, b) in v.beats.iter().enumerate() {
                if (mi, bi) > last {
                    break 'bars;
                }
                if b.rest {
                    continue;
                }
                match at.get(&(mi, bi)) {
                    Some((syllabic, t)) => {
                        text.push_str(&t.replace(' ', "+").replace('-', ""));
                        text.push_str(if matches!(*syllabic, "begin" | "middle") { "-" } else { " " });
                    }
                    None if b.notes.iter().all(|n| n.kind == NoteKind::Tie) => text.push_str("- "),
                    None => text.push_str("- "),
                }
            }
        }
        lines.push(LyricLine { measure: first_bar, text: text.trim_end().to_string() });
    }
    while lines.last().is_some_and(|l| l.text.is_empty()) {
        lines.pop();
    }
    lines
}
