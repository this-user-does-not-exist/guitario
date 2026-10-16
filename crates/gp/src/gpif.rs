use crate::{GpError, Import, MAX_MEASURES, MAX_TRACKS};
use guitario_score::*;
use guitario_theory::{Duration, NoteValue, TimeSignature, Tuning};
use roxmltree::{Document, Node};
use std::collections::HashMap;

type Table<'a> = HashMap<u32, Node<'a, 'a>>;

fn child<'a>(n: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    n.children().find(|c| c.has_tag_name(name))
}

fn text<'a>(n: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    child(n, name).and_then(|c| c.text()).map(str::trim)
}

fn ids(s: Option<&str>) -> Vec<i64> {
    s.map(|s| s.split_whitespace().filter_map(|t| t.parse().ok()).collect()).unwrap_or_default()
}

fn table<'a>(root: Node<'a, 'a>, name: &str, item: &str) -> Table<'a> {
    let mut map = HashMap::new();
    if let Some(t) = child(root, name) {
        for n in t.children().filter(|c| c.has_tag_name(item)) {
            if let Some(id) = n.attribute("id").and_then(|v| v.parse().ok()) {
                map.insert(id, n);
            }
        }
    }
    map
}

fn lookup<'a>(t: &Table<'a>, id: i64) -> Option<Node<'a, 'a>> {
    u32::try_from(id).ok().and_then(|i| t.get(&i).copied())
}

/// The `<Property name=…>` of an element, as the property node.
fn property<'a>(n: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    child(n, "Properties")?.children().find(|c| c.has_tag_name("Property") && c.attribute("name") == Some(name))
}

fn prop_value<'a>(n: Node<'a, 'a>, name: &str) -> Option<&'a str> {
    property(n, name)?.children().find(|c| c.is_element()).and_then(|c| c.text()).map(str::trim)
}

fn prop_on(n: Node, name: &str) -> bool {
    property(n, name).is_some_and(|p| child(p, "Enable").is_some())
}

fn prop_num<T: std::str::FromStr>(n: Node, name: &str) -> Option<T> {
    prop_value(n, name)?.parse().ok()
}

pub fn parse_gpif(xml: &str) -> Result<Import, GpError> {
    let doc = Document::parse(xml).map_err(|e| GpError::Xml(e.to_string()))?;
    let root = doc.root_element();
    if !root.has_tag_name("GPIF") {
        return Err(GpError::Damaged("root element is not GPIF".into()));
    }
    let mut warnings = Vec::new();
    let (bars, voices, beats, notes, rhythms) = (
        table(root, "Bars", "Bar"),
        table(root, "Voices", "Voice"),
        table(root, "Beats", "Beat"),
        table(root, "Notes", "Note"),
        table(root, "Rhythms", "Rhythm"),
    );

    let mut score = Score::new();
    if let Some(s) = child(root, "Score") {
        let get = |k: &str| text(s, k).unwrap_or_default().to_string();
        score.meta = Meta { title: get("Title"), artist: get("Artist"), album: get("Album"), author: get("Tabber"), notes: get("Notices") };
        if let Some(ps) = child(s, "PageSetup") {
            let mm = |k: &str| text(ps, k).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).map(|v| v.round().clamp(-1e6, 1e6) as i32);
            if let (Some(width), Some(height)) = (mm("Width"), mm("Height")) {
                let landscape = text(ps, "Orientation").map(|o| o.trim().eq_ignore_ascii_case("landscape"));
                let margins =
                    [mm("LeftMargin").unwrap_or(10), mm("RightMargin").unwrap_or(10), mm("TopMargin").unwrap_or(15), mm("BottomMargin").unwrap_or(10)];
                score.page = crate::page::from_gp(width, height, landscape, margins, &mut warnings);
            }
        }
    }

    // ---- master bars -------------------------------------------------------------------
    let masters: Vec<Node> = child(root, "MasterBars").map(|m| m.children().filter(|c| c.has_tag_name("MasterBar")).collect()).unwrap_or_default();
    if masters.is_empty() {
        return Err(GpError::Damaged("no master bars".into()));
    }
    let masters = if masters.len() > MAX_MEASURES {
        warnings.push(format!("only the first {MAX_MEASURES} measures were imported"));
        masters.into_iter().take(MAX_MEASURES).collect()
    } else {
        masters
    };
    score.headers = masters.iter().map(|m| header_of(*m)).collect();

    // Tempo automations live on the master track.
    if let Some(autos) = child(root, "MasterTrack").and_then(|m| child(m, "Automations")) {
        for a in autos.children().filter(|c| c.has_tag_name("Automation") && text(*c, "Type") == Some("Tempo")) {
            let bpm =
                text(a, "Value").and_then(|v| v.split_whitespace().next()).and_then(|v| v.parse::<f64>().ok()).map(|v| v.round().clamp(20.0, 400.0) as u16);
            let bar = text(a, "Bar").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let Some(bpm) = bpm else { continue };
            if bar == 0 {
                score.tempo = bpm;
            } else if let Some(h) = score.headers.get_mut(bar) {
                h.tempo = Some(bpm);
            }
        }
    }

    // ---- tracks ------------------------------------------------------------------------
    score.tracks.clear();
    let track_nodes: Vec<Node> = child(root, "Tracks").map(|t| t.children().filter(|c| c.has_tag_name("Track")).collect()).unwrap_or_default();
    let mut staff_offset = 0usize;
    for (ti, tn) in track_nodes.iter().enumerate() {
        let staves: Vec<Node> = child(*tn, "Staves").map(|s| s.children().filter(|c| c.has_tag_name("Staff")).collect()).unwrap_or_default();
        let my_staff = staff_offset;
        staff_offset += staves.len().max(1);
        let name = text(*tn, "Name").unwrap_or("Track").to_string();
        // Guitar Pro 7 names a drum kit's instrument set; Guitar Pro 6 plays it on the percussion table.
        let is_drums = child(*tn, "InstrumentSet").and_then(|i| text(i, "Type")) == Some("drumKit")
            || child(*tn, "GeneralMidi").and_then(|g| g.attribute("table")) == Some("Percussion");
        if score.tracks.len() >= MAX_TRACKS {
            warnings.push(format!("track `{name}` skipped: at most {MAX_TRACKS} tracks are supported"));
            continue;
        }
        // Guitar Pro 6 has no staves: tuning and capo sit on the track itself.
        let staff = staves.first().copied().unwrap_or(*tn);
        if !is_drums && property(staff, "Tuning").is_none() {
            warnings.push(format!("track `{name}` is not a fretted instrument and was skipped"));
            continue;
        }
        let kit = is_drums.then(|| articulations(*tn));
        match build_track(*tn, staff, my_staff, &name, kit.as_deref(), &masters, &score.headers, (&bars, &voices, &beats, &notes, &rhythms), &mut warnings) {
            Some(t) => score.tracks.push(t),
            None => warnings.push(format!("track {} (`{name}`) could not be read and was skipped", ti + 1)),
        }
    }
    if score.tracks.is_empty() {
        return Err(GpError::Damaged("no readable tracks".into()));
    }
    let gp6_drums = notes.values().filter(|n| property(**n, "Element").is_some() && property(**n, "Midi").is_none()).count();
    if gp6_drums > 0 {
        warnings.push(format!("Guitar Pro 6 drum hits (kit element and variation) are not supported yet and were dropped ({gp6_drums})"));
    }
    score.validate()?;
    Ok(Import { score, warnings })
}

fn header_of(m: Node) -> MeasureHeader {
    let ts = text(m, "Time")
        .and_then(|t| t.split_once('/'))
        .and_then(|(n, d)| TimeSignature::new(n.trim().parse().ok()?, d.trim().parse().ok()?))
        .unwrap_or(TimeSignature::COMMON);
    let mut h = MeasureHeader::new(ts);
    h.key = child(m, "Key").and_then(|k| text(k, "AccidentalCount")).and_then(|v| v.parse::<i8>().ok()).map_or(0, |k| k.clamp(-7, 7));
    if let Some(r) = child(m, "Repeat") {
        h.repeat_open = r.attribute("start") == Some("true");
        if r.attribute("end") == Some("true") {
            // `count` is the total number of plays; we store extra passes.
            let plays: u8 = r.attribute("count").and_then(|c| c.parse().ok()).unwrap_or(2);
            h.repeat_close = plays.saturating_sub(1).max(1);
        }
    }
    if let Some(s) = child(m, "Section") {
        let joined = [text(s, "Letter"), text(s, "Text")].into_iter().flatten().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
        h.marker = (!joined.is_empty()).then_some(joined);
    }
    if let Some(e) = text(m, "AlternateEndings") {
        h.endings = e.split_whitespace().filter_map(|n| n.parse::<u8>().ok()).filter(|n| (1..=8).contains(n)).fold(0, |b, n| b | 1 << (n - 1));
    }
    if let Some(d) = child(m, "Directions") {
        for c in d.children().filter(Node::is_element) {
            let v = c.text().unwrap_or("").trim();
            if c.has_tag_name("Target") {
                let t = match v {
                    "Segno" => Some(DirectionTarget::Segno),
                    "SegnoSegno" => Some(DirectionTarget::SegnoSegno),
                    "Coda" => Some(DirectionTarget::Coda),
                    "DoubleCoda" => Some(DirectionTarget::DoubleCoda),
                    "Fine" => Some(DirectionTarget::Fine),
                    _ => None,
                };
                if let Some(t) = t.filter(|t| !h.targets.contains(t)) {
                    h.targets.push(t);
                }
            } else if c.has_tag_name("Jump") {
                h.jump = jump_of(v).or(h.jump);
            }
        }
    }
    h.triplet_feel = match text(m, "TripletFeel") {
        Some("Triplet8th") => TripletFeel::Triplet8th,
        Some("Triplet16th") => TripletFeel::Triplet16th,
        Some("Dotted8th") => TripletFeel::Dotted8th,
        Some("Dotted16th") => TripletFeel::Dotted16th,
        Some("Scottish8th") => TripletFeel::Scottish8th,
        Some("Scottish16th") => TripletFeel::Scottish16th,
        _ => TripletFeel::None,
    };
    h.free_time = child(m, "FreeTime").is_some();
    h.double_bar = child(m, "DoubleBar").is_some();
    // Fermatas: a type, where in the bar as a fraction of a quarter note, and a length.
    if let Some(list) = child(m, "Fermatas") {
        let cap = h.capacity();
        for f in list.children().filter(|c| c.has_tag_name("Fermata")).take(guitario_score::MAX_FERMATAS) {
            let kind = match text(f, "Type") {
                Some("Short") => FermataKind::Short,
                Some("Long") => FermataKind::Long,
                _ => FermataKind::Medium,
            };
            let tick = text(f, "Offset")
                .and_then(|o| o.split_once('/'))
                .and_then(|(n, d)| Some((n.trim().parse::<u64>().ok()?, d.trim().parse::<u64>().ok().filter(|d| *d > 0)?)))
                .and_then(|(n, d)| u32::try_from(n.saturating_mul(u64::from(guitario_theory::TICKS_PER_QUARTER)) / d).ok())
                .unwrap_or(0);
            let length =
                text(f, "Length").and_then(|l| l.parse::<f64>().ok()).filter(|l| l.is_finite()).map_or(0, |l| (l * 100.0).round().clamp(0.0, 1000.0) as u16);
            if tick < cap && !h.fermatas.iter().any(|g| g.tick == tick) {
                h.fermatas.push(Fermata { tick, kind, length });
            }
        }
        h.fermatas.sort_by_key(|f| f.tick);
    }
    h
}

/// A GPIF `Directions/Jump` name.
fn jump_of(name: &str) -> Option<Jump> {
    let (base, end) = match name.split_once("Al") {
        Some((b, e)) => (b, e),
        None => (name, ""),
    };
    let end = match end {
        "" => JumpEnd::End,
        "Fine" => JumpEnd::Fine,
        "Coda" => JumpEnd::Coda,
        "DoubleCoda" => JumpEnd::DoubleCoda,
        _ => return None,
    };
    Some(match (base, end) {
        ("DaCapo", e) => Jump::DaCapo(e),
        ("DaSegno", e) => Jump::DalSegno(e),
        ("DaSegnoSegno", e) => Jump::DalSegnoSegno(e),
        ("DaCoda", JumpEnd::End) => Jump::ToCoda,
        ("DaDoubleCoda", JumpEnd::End) => Jump::ToDoubleCoda,
        _ => return None,
    })
}

type Tables<'a, 'b> = (&'b Table<'a>, &'b Table<'a>, &'b Table<'a>, &'b Table<'a>, &'b Table<'a>);

/// The output MIDI number of every articulation of a drum kit, flattened across elements in
/// document order: a note's `InstrumentArticulation` indexes this list.
fn articulations(tn: Node) -> Vec<u8> {
    let Some(elements) = child(tn, "InstrumentSet").and_then(|i| child(i, "Elements")) else { return Vec::new() };
    elements
        .children()
        .filter(|e| e.has_tag_name("Element"))
        .filter_map(|e| child(e, "Articulations"))
        .flat_map(|a| a.children().filter(|c| c.has_tag_name("Articulation")))
        .map(|a| text(a, "OutputMidiNumber").and_then(|v| v.parse::<u8>().ok()).unwrap_or(0))
        .take(1024)
        .collect()
}

/// Where a note goes: strings are flipped (GP counts from the lowest), drum hits go to the lane
/// of their General MIDI number.
#[derive(Clone, Copy)]
enum Placement<'a> {
    /// String pitches highest first, as the track holds them, and the capo fret.
    Strings {
        open: &'a [u8],
        capo: u8,
    },
    Drums(&'a [u8]),
}

impl Placement<'_> {
    fn place(self, nn: Node) -> Option<(u8, u8)> {
        match self {
            Placement::Strings { open, .. } => {
                let strings = open.len();
                let gp_string = prop_num::<usize>(nn, "String")?;
                let fret = prop_num::<u8>(nn, "Fret")?;
                (gp_string < strings).then(|| ((strings - 1 - gp_string) as u8, fret.min(36)))
            }
            Placement::Drums(kit) => {
                let midi = prop_num::<u8>(nn, "Midi").or_else(|| {
                    let i = text(nn, "InstrumentArticulation")?.parse::<usize>().ok()?;
                    kit.get(i).copied().filter(|m| *m > 0)
                })?;
                let midi = midi.min(guitario_theory::drums::MAX_DRUM);
                Some((guitario_theory::drums::lane_for(midi), midi))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_track(
    tn: Node,
    staff: Node,
    staff_index: usize,
    name: &str,
    kit: Option<&[u8]>,
    masters: &[Node],
    headers: &[MeasureHeader],
    t: Tables,
    warnings: &mut Vec<String>,
) -> Option<Track> {
    if let Some(kit) = kit {
        let mut track = Track::drums(name, headers);
        let sung = fill_measures(&mut track, staff_index, name, Placement::Drums(kit), &HashMap::new(), masters, headers, t, warnings);
        track.lyrics = lyrics_of(tn, &track, &sung);
        track.muted = text(tn, "PlaybackState") == Some("Mute");
        track.solo = text(tn, "PlaybackState") == Some("Solo");
        return Some(track);
    }
    // GP lists tuning lowest string first; we store highest first.
    let mut pitches: Vec<u8> = property(staff, "Tuning")
        .and_then(|p| text(p, "Pitches"))
        .map(|s| s.split_whitespace().filter_map(|v| v.parse::<u8>().ok()).collect())
        .unwrap_or_default();
    pitches.reverse();
    let preset = || Tuning::presets().into_iter().find(|t| t.strings == pitches).map_or_else(|| "Custom".to_string(), |t| t.name);
    let tuning_name = child(staff, "Properties").and_then(|p| text(p, "Name")).filter(|n| !n.is_empty()).map_or_else(preset, str::to_string);
    let tuning = Tuning::new(tuning_name, pitches).ok()?;
    let mut track = Track::new(name, tuning, headers);
    track.capo = prop_num::<u8>(staff, "CapoFret").unwrap_or(0).min(24);
    let program = child(tn, "Sounds")
        .and_then(|s| child(s, "Sound"))
        .and_then(|s| child(s, "MIDI"))
        .and_then(|m| text(m, "Program"))
        .or_else(|| child(tn, "GeneralMidi").and_then(|g| text(g, "Program")));
    if let Some(p) = program.and_then(|v| v.parse::<u8>().ok()) {
        track.program = p.min(127);
    }
    match text(tn, "PlaybackState") {
        Some("Mute") => track.muted = true,
        Some("Solo") => track.solo = true,
        _ => {}
    }
    let open = track.tuning.strings.clone();
    let capo = track.capo;
    let chords = diagrams(tn, open.len());
    let sung = fill_measures(&mut track, staff_index, name, Placement::Strings { open: &open, capo }, &chords, masters, headers, t, warnings);
    track.lyrics = lyrics_of(tn, &track, &sung);
    Some(track)
}

/// The syllables a beat of the first voice carries, per lyrics line: `(measure, beat, lines)`.
type Sung = Vec<(usize, usize, Vec<String>)>;

/// A track's lyrics: the lines as typed (`Lyrics/Line/Text`, from bar `Offset`), or, when those are
/// empty, lines rebuilt from the syllables Guitar Pro dispatched onto the beats (`-` for a beat
/// without one, `+` joining words on one beat).
fn lyrics_of(tn: Node, track: &Track, sung: &Sung) -> Vec<LyricLine> {
    let bars = track.measures.len();
    let mut lines: Vec<LyricLine> = child(tn, "Lyrics")
        .map(|l| {
            l.children()
                .filter(|c| c.has_tag_name("Line"))
                .take(MAX_LYRIC_LINES)
                .map(|line| LyricLine {
                    measure: text(line, "Offset").and_then(|o| o.parse::<usize>().ok()).filter(|o| *o < bars).unwrap_or(0),
                    text: text(line, "Text").unwrap_or("").chars().take(MAX_LYRICS_TEXT / 4).collect(),
                })
                .collect()
        })
        .unwrap_or_default();
    if lines.iter().all(|l| l.text.trim().is_empty()) {
        lines = (0..MAX_LYRIC_LINES)
            .map(|k| {
                let on = |e: &(usize, usize, Vec<String>)| e.2.get(k).filter(|t| !t.trim().is_empty()).cloned();
                let Some(first) = sung.iter().position(|e| on(e).is_some()) else { return LyricLine::default() };
                let measure = sung.get(first).map_or(0, |e| e.0);
                // Every sung beat from the start of that bar: a syllable, or `-` for none.
                let mut words: Vec<String> =
                    sung.iter().skip_while(|e| e.0 < measure).map(|e| on(e).map_or_else(|| "-".to_string(), |t| t.trim().replace(' ', "+"))).collect();
                while words.last().is_some_and(|w| w == "-") {
                    words.pop();
                }
                LyricLine { measure, text: words.join(" ") }
            })
            .collect();
    }
    while lines.last().is_some_and(|l| l.text.trim().is_empty()) {
        lines.pop();
    }
    lines
}

/// The chord diagrams a track lists (`DiagramCollection` on the staff in Guitar Pro 7/8, on the
/// track in Guitar Pro 6), by id: the name, and frets made absolute and turned highest string
/// first. A diagram's frets count from its `baseFret`; 0 is an open string.
fn diagrams(tn: Node, strings: usize) -> HashMap<i64, Chord> {
    let mut out = HashMap::new();
    let collections = tn.descendants().filter(|c| c.has_tag_name("Property") && matches!(c.attribute("name"), Some("DiagramCollection" | "ChordCollection")));
    for items in collections.filter_map(|c| child(c, "Items")) {
        for item in items.children().filter(|c| c.has_tag_name("Item")).take(4096) {
            let Some(id) = item.attribute("id").and_then(|v| v.parse::<i64>().ok()) else { continue };
            let name: String = item.attribute("name").unwrap_or("").trim().chars().take(64).collect();
            let mut chord = Chord::named(name);
            if let Some(d) = child(item, "Diagram") {
                let base = d.attribute("baseFret").and_then(|v| v.parse::<u8>().ok()).unwrap_or(0).min(35);
                let mut frets: Vec<Option<u8>> = vec![None; strings];
                for f in d.children().filter(|c| c.has_tag_name("Fret")) {
                    let (Some(gs), Some(fret)) =
                        (f.attribute("string").and_then(|v| v.parse::<usize>().ok()), f.attribute("fret").and_then(|v| v.parse::<u8>().ok()))
                    else {
                        continue;
                    };
                    let absolute = if fret == 0 { 0 } else { base.saturating_add(fret) };
                    if gs < strings
                        && absolute <= 36
                        && let Some(slot) = frets.get_mut(strings - 1 - gs)
                    {
                        *slot = Some(absolute);
                    }
                }
                if frets.iter().any(Option::is_some) {
                    chord.frets = frets;
                    chord.first_fret = base + 1;
                }
            }
            if !chord.name.is_empty() || !chord.frets.is_empty() {
                out.entry(id).or_insert(chord);
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn fill_measures(
    track: &mut Track,
    staff_index: usize,
    name: &str,
    place: Placement,
    chords: &HashMap<i64, Chord>,
    masters: &[Node],
    headers: &[MeasureHeader],
    t: Tables,
    warnings: &mut Vec<String>,
) -> Sung {
    let mut sung: Sung = Vec::new();
    let mut lost_graces = 0usize;
    for (mi, master) in masters.iter().enumerate() {
        let bar_id = ids(text(*master, "Bars")).get(staff_index).copied().unwrap_or(-1);
        let Some(bar) = lookup(t.0, bar_id) else { continue };
        let cap = headers.get(mi).map_or(0, MeasureHeader::capacity);
        // A bar lists up to four voices; -1 is a voice it does not have.
        let mut voices: Vec<Voice> = Vec::new();
        for (vi, voice_id) in ids(text(bar, "Voices")).into_iter().take(MAX_VOICES).enumerate() {
            let mut out = Voice::default();
            if let Some(voice) = lookup(t.1, voice_id) {
                let mut used = 0u32;
                // Grace beats take no time: their notes become the grace notes of the next beat's
                // notes on the same strings.
                let mut graces: Vec<(u8, Grace)> = Vec::new();
                for beat_id in ids(text(voice, "Beats")) {
                    let Some(bn) = lookup(t.2, beat_id) else { continue };
                    if let Some(kind) = text(bn, "GraceNotes") {
                        if let Some(gb) = build_beat(bn, &t, place, chords).filter(|_| matches!(place, Placement::Strings { .. })) {
                            for (string, grace) in grace_notes(&gb, kind == "OnBeat") {
                                if let Some(old) = graces.iter_mut().find(|(st, _)| *st == string) {
                                    // Two grace notes on one string: the one nearer the note is kept.
                                    old.1 = grace;
                                    lost_graces += 1;
                                } else {
                                    graces.push((string, grace));
                                }
                            }
                        }
                        continue;
                    }
                    let Some(mut beat) = build_beat(bn, &t, place, chords) else { continue };
                    for (string, grace) in graces.drain(..) {
                        match beat.notes.iter_mut().find(|n| n.string == string && n.kind != NoteKind::Tie) {
                            Some(n) => n.effects.grace = Some(grace),
                            None => lost_graces += 1,
                        }
                    }
                    let ticks = beat.duration.ticks();
                    if used + ticks > cap {
                        warnings.push(format!("track `{name}` measure {}: beats overflow the bar and were trimmed", mi + 1));
                        break;
                    }
                    used += ticks;
                    // The first voice's beats carry the lyrics Guitar Pro dispatched, if any.
                    if vi == 0 && !beat.rest && sung.len() < 100_000 {
                        let lines: Vec<String> = child(bn, "Lyrics")
                            .map(|l| {
                                l.children().filter(|c| c.has_tag_name("Line")).take(MAX_LYRIC_LINES).map(|c| c.text().unwrap_or("").to_string()).collect()
                            })
                            .unwrap_or_default();
                        sung.push((mi, out.beats.len(), lines));
                    }
                    out.beats.push(beat);
                }
                lost_graces += graces.len();
            }
            voices.push(out);
        }
        // Voices after the first only matter when they sound; the rest would be bar-long rests.
        while voices.len() > 1 && voices.last().is_none_or(|v| v.beats.iter().all(|b| b.rest)) {
            voices.pop();
        }
        let Some(m) = track.measures.get_mut(mi) else { continue };
        if voices.iter().all(|v| v.beats.is_empty()) {
            continue;
        }
        // A voice between others that has no beats is a bar-long rest.
        for v in &mut voices {
            if v.beats.is_empty()
                && let Some(header) = headers.get(mi)
            {
                *v = Measure::empty_for(header).voices.into_iter().next().unwrap_or_default();
            }
        }
        m.voices = voices;
    }
    if lost_graces > 0 {
        warnings.push(format!("track `{name}`: {lost_graces} grace notes without a struck note on their string after them were left out"));
    }
    sung
}

/// The notes of a grace beat as grace notes, by string. A hammer-on, slide or bend on a grace note
/// is how it moves into the note.
fn grace_notes(gb: &Beat, on_beat: bool) -> Vec<(u8, Grace)> {
    let duration = match gb.duration.value {
        NoteValue::Sixteenth => GraceDuration::Sixteenth,
        NoteValue::SixtyFourth => GraceDuration::SixtyFourth,
        _ => GraceDuration::ThirtySecond,
    };
    gb.notes
        .iter()
        .filter(|n| n.kind != NoteKind::Tie)
        .map(|n| {
            let transition = if n.effects.hammer {
                GraceTransition::Hammer
            } else if matches!(n.effects.slide, Some(Slide::Legato | Slide::Shift)) {
                GraceTransition::Slide
            } else if n.effects.bend.is_some() {
                GraceTransition::Bend
            } else {
                GraceTransition::None
            };
            let dead = n.kind == NoteKind::Dead;
            (n.string, Grace { fret: if dead { 0 } else { n.fret }, duration, transition, on_beat, dead })
        })
        .collect()
}

fn rhythm_duration(r: Node) -> Option<Duration> {
    let value = match text(r, "NoteValue")? {
        "Whole" => NoteValue::Whole,
        "Half" => NoteValue::Half,
        "Quarter" => NoteValue::Quarter,
        "Eighth" => NoteValue::Eighth,
        "16th" => NoteValue::Sixteenth,
        "32nd" => NoteValue::ThirtySecond,
        "64th" => NoteValue::SixtyFourth,
        _ => return None,
    };
    let dots = child(r, "AugmentationDot").and_then(|d| d.attribute("count")).and_then(|c| c.parse::<u8>().ok()).unwrap_or(0).min(2);
    let tuplet = child(r, "PrimaryTuplet")
        .and_then(|t| Some((t.attribute("num")?.parse::<u8>().ok()?, t.attribute("den")?.parse::<u8>().ok()?)))
        .filter(|(n, d)| (1..=16).contains(n) && (1..=16).contains(d))
        .unwrap_or((1, 1));
    Some(Duration { value, dots, tuplet })
}

/// The note `XProperty` that holds a trill's period, in 960ths of a quarter (240 sixteenth, 120 thirty-second, 60 sixty-fourth).
pub(crate) const TRILL_SPEED_ID: &str = "688062467";

/// An `XProperty` by id, as an integer.
fn xproperty(bn: Node, id: &str) -> Option<i64> {
    child(bn, "XProperties")?
        .children()
        .find(|c| c.has_tag_name("XProperty") && c.attribute("id") == Some(id))
        .and_then(|x| x.children().find(|c| c.is_element()))
        .and_then(|v| v.text())
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .map(|v| v as i64)
}

/// Beat techniques: strums, arpeggios, rasgueado, pick strokes, slap/pop and fades.
fn beat_effects(bn: Node, beat: &mut Beat) {
    let dir = |name: &str| property(bn, name).and_then(|p| text(p, "Direction"));
    // Brush and arpeggio durations are stored at 960 ticks per quarter.
    let ticks = |id: &str, default: u32| {
        let scale = guitario_theory::TICKS_PER_QUARTER / 960;
        xproperty(bn, id).and_then(|v| u32::try_from(v.clamp(1, 7680)).ok()).map_or(default, |v| v * scale)
    };
    let default = guitario_theory::TICKS_PER_QUARTER / 8;
    if let Some(d) = dir("Brush") {
        beat.brush = Some(Brush { kind: BrushKind::Brush, down: d == "Down", ticks: ticks("687935489", default) });
    } else if let Some(d) = text(bn, "Arpeggio") {
        beat.brush = Some(Brush { kind: BrushKind::Arpeggio, down: d == "Down", ticks: ticks("687931393", default) });
    } else if property(bn, "Rasgueado").is_some() {
        beat.brush = Some(Brush { kind: BrushKind::Rasgueado, down: true, ticks: default });
    }
    beat.stroke = match dir("PickStroke") {
        Some("Up") => Some(StrokeDirection::Up),
        Some("Down") => Some(StrokeDirection::Down),
        _ => None,
    };
    beat.technique = if prop_on(bn, "Slapped") {
        Some(BeatTechnique::Slap)
    } else if prop_on(bn, "Popped") {
        Some(BeatTechnique::Pop)
    } else {
        None
    };
    beat.fade = match text(bn, "Fadding") {
        Some("FadeIn") => Some(Fade::In),
        Some("FadeOut") => Some(Fade::Out),
        Some("VolumeSwell") => Some(Fade::Swell),
        _ => None,
    };
}

fn build_beat(bn: Node, t: &Tables, place: Placement, chords: &HashMap<i64, Chord>) -> Option<Beat> {
    let rid = child(bn, "Rhythm")?.attribute("ref")?.parse::<i64>().ok()?;
    let duration = rhythm_duration(lookup(t.4, rid)?)?;
    let mut beat = Beat::rest(duration);
    beat.dynamic = match text(bn, "Dynamic") {
        Some("PPP") => Dynamic::Ppp,
        Some("PP") => Dynamic::Pp,
        Some("P") => Dynamic::P,
        Some("MP") => Dynamic::Mp,
        Some("F") => Dynamic::F,
        Some("FF") => Dynamic::Ff,
        Some("FFF") => Dynamic::Fff,
        _ => Dynamic::Mf,
    };
    beat.text = text(bn, "FreeText").filter(|s| !s.is_empty()).map(str::to_string);
    beat.chord = text(bn, "Chord").and_then(|id| id.parse::<i64>().ok()).and_then(|id| chords.get(&id)).cloned();
    beat_effects(bn, &mut beat);
    // The fraction is the slash count's note value as Guitar Pro writes it: 1/8 one slash, 1/4 two, 1/2 three.
    let tremolo = match text(bn, "Tremolo") {
        Some("1/8") => Some(TremoloSpeed::Eighth),
        Some("1/4") => Some(TremoloSpeed::Sixteenth),
        Some("1/2") => Some(TremoloSpeed::ThirtySecond),
        _ => None,
    };
    for nid in ids(text(bn, "Notes")) {
        let Some(nn) = lookup(t.3, nid) else { continue };
        let Some((string, fret)) = place.place(nn) else { continue };
        if beat.notes.iter().any(|n| n.string == string) {
            continue;
        }
        let mut note = Note::new(string, fret);
        note.kind = if prop_on(nn, "Muted") {
            NoteKind::Dead
        } else if child(nn, "Tie").and_then(|t| t.attribute("destination")) == Some("true") {
            NoteKind::Tie
        } else {
            NoteKind::Normal
        };
        let e = &mut note.effects;
        e.hammer = prop_on(nn, "HopoOrigin");
        e.wide_vibrato = text(nn, "Vibrato") == Some("Wide");
        e.vibrato = child(nn, "Vibrato").is_some() && !e.wide_vibrato;
        e.tapped = prop_on(nn, "Tapped");
        e.left_hand_tapped = prop_on(nn, "LeftHandTapped");
        e.let_ring = child(nn, "LetRing").is_some();
        e.ghost = child(nn, "AntiAccent").is_some();
        e.palm_mute = prop_on(nn, "PalmMuted");
        let finger = |name: &str| match text(nn, name) {
            Some("P") => Some(Finger::Thumb),
            Some("I") => Some(Finger::Index),
            Some("M") => Some(Finger::Middle),
            Some("A") => Some(Finger::Ring),
            Some("C") => Some(Finger::Little),
            _ => None,
        };
        if matches!(place, Placement::Strings { .. }) {
            e.left_finger = finger("LeftFingering");
            e.right_finger = finger("RightFingering");
        }
        let accent = text(nn, "Accent").and_then(|v| v.parse::<u8>().ok()).unwrap_or(0);
        e.accent = accent & 8 != 0;
        e.heavy_accent = accent & 4 != 0;
        e.staccato = accent & 1 != 0;
        e.slide = match prop_num::<u32>(nn, "Slide").unwrap_or(0) {
            f if f & 1 != 0 => Some(Slide::Shift),
            f if f & 2 != 0 => Some(Slide::Legato),
            f if f & 4 != 0 => Some(Slide::OutDown),
            f if f & 8 != 0 => Some(Slide::OutUp),
            f if f & 16 != 0 => Some(Slide::IntoFromBelow),
            f if f & 32 != 0 => Some(Slide::IntoFromAbove),
            _ => None,
        };
        // Guitar Pro 6 marks a harmonic by its type alone.
        if prop_on(nn, "Harmonic") || property(nn, "HarmonicType").is_some() {
            e.harmonic = match prop_value(nn, "HarmonicType") {
                Some("Natural") => Some(Harmonic::Natural),
                Some("Artificial") => Some(Harmonic::Artificial),
                Some("Pinch") => Some(Harmonic::Pinch),
                Some("Tap") => Some(Harmonic::Tapped),
                Some("Semi") => Some(Harmonic::Semi),
                _ => Some(Harmonic::Natural),
            };
        }
        // A trill names the pitch it alternates with; the period is an XProperty in 960ths of a quarter.
        if let (Some(target), Placement::Strings { open, capo }) = (text(nn, "Trill").and_then(|v| v.trim().parse::<i32>().ok()), place)
            && let Some(base) = open.get(usize::from(string)).map(|o| i32::from(*o) + i32::from(capo))
            && let alt = target - base
            && (0..=36).contains(&alt)
            && alt != i32::from(fret)
            && note.kind == NoteKind::Normal
        {
            let speed = match xproperty(nn, TRILL_SPEED_ID) {
                Some(t) if t <= 90 => TrillSpeed::SixtyFourth,
                Some(t) if t <= 180 => TrillSpeed::ThirtySecond,
                _ => TrillSpeed::Sixteenth,
            };
            e.trill = Some(Trill { fret: u8::try_from(alt).unwrap_or(0), speed });
        }
        if prop_on(nn, "Bended") {
            // Curve values are hundredths of a whole tone; a quarter tone is 25.
            let peak = ["BendOriginValue", "BendMiddleValue", "BendDestinationValue"].iter().filter_map(|k| prop_num::<f32>(nn, k)).fold(0.0f32, f32::max);
            let q = (peak / 25.0).round().clamp(0.0, 12.0) as u8;
            e.bend = (q > 0).then_some(q);
        }
        beat.notes.push(note);
    }
    beat.notes.sort_by_key(|n| n.string);
    beat.rest = beat.notes.is_empty();
    beat.tremolo = tremolo.filter(|_| !beat.rest);
    Some(beat)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GPIF: &str = r#"<?xml version="1.0"?>
<GPIF><GPVersion>7</GPVersion>
<Score><Title><![CDATA[Demo]]></Title><Artist><![CDATA[Me]]></Artist></Score>
<MasterTrack><Tracks>0</Tracks><Automations><Automation><Type>Tempo</Type><Bar>0</Bar><Position>0</Position><Value>96 2</Value></Automation>
<Automation><Type>Tempo</Type><Bar>1</Bar><Position>0</Position><Value>140 2</Value></Automation></Automations></MasterTrack>
<Tracks><Track id="0"><Name><![CDATA[Gtr]]></Name><PlaybackState>Default</PlaybackState>
<Sounds><Sound><MIDI><Program>29</Program></MIDI></Sound></Sounds>
<Staves><Staff><Properties>
<Property name="CapoFret"><Fret>2</Fret></Property>
<Property name="Tuning"><Pitches>40 45 50 55 59 64</Pitches></Property>
<Name><![CDATA[Standard]]></Name></Properties></Staff></Staves></Track></Tracks>
<MasterBars>
<MasterBar><Key><AccidentalCount>1</AccidentalCount></Key><Time>4/4</Time><Bars>0</Bars><Repeat start="true" end="false" count="0"/><Section><Letter>A</Letter><Text>Verse</Text></Section></MasterBar>
<MasterBar><Time>3/4</Time><Bars>1</Bars><Repeat start="false" end="true" count="3"/><AlternateEndings>1 2</AlternateEndings><Directions><Target>Fine</Target><Target>Segno</Target><Jump>DaSegnoSegnoAlDoubleCoda</Jump></Directions><TripletFeel>Dotted16th</TripletFeel><FreeTime/><DoubleBar/></MasterBar>
</MasterBars>
<Bars><Bar id="0"><Voices>0 1 -1 -1</Voices></Bar><Bar id="1"><Voices>-1</Voices></Bar></Bars>
<Voices><Voice id="0"><Beats>0 1</Beats></Voice><Voice id="1"><Beats>2</Beats></Voice></Voices>
<Beats>
<Beat id="0"><Dynamic>F</Dynamic><Rhythm ref="0"/><Notes>0 1</Notes><Fadding>VolumeSwell</Fadding><Properties><Property name="Brush"><Direction>Up</Direction></Property><Property name="PickStroke"><Direction>Down</Direction></Property><Property name="Popped"><Enable/></Property></Properties><XProperties><XProperty id="687935489"><Int>240</Int></XProperty></XProperties></Beat>
<Beat id="1"><Rhythm ref="1"/></Beat>
<Beat id="2"><Rhythm ref="2"/><Notes>2</Notes></Beat>
</Beats>
<Notes>
<Note id="0"><Properties><Property name="String"><String>0</String></Property><Property name="Fret"><Fret>3</Fret></Property><Property name="Slide"><Flags>2</Flags></Property></Properties><LetRing/></Note>
<Note id="1"><Vibrato>Wide</Vibrato><Accent>5</Accent><Properties><Property name="String"><String>5</String></Property><Property name="Fret"><Fret>0</Fret></Property><Property name="PalmMuted"><Enable/></Property><Property name="Tapped"><Enable/></Property></Properties></Note>
<Note id="2"><Properties><Property name="String"><String>2</String></Property><Property name="Fret"><Fret>7</Fret></Property></Properties></Note>
</Notes>
<Rhythms><Rhythm id="2"><NoteValue>Whole</NoteValue></Rhythm><Rhythm id="0"><NoteValue>Quarter</NoteValue></Rhythm><Rhythm id="1"><NoteValue>Half</NoteValue><AugmentationDot count="1"/></Rhythm></Rhythms>
</GPIF>"#;

    #[test]
    fn parses_a_minimal_file() {
        let imp = parse_gpif(GPIF).unwrap();
        let s = &imp.score;
        assert_eq!(s.meta.title, "Demo");
        assert_eq!(s.tempo, 96);
        assert_eq!(s.headers[1].tempo, Some(140));
        assert_eq!(s.headers[0].key, 1);
        assert!(s.headers[0].repeat_open);
        assert_eq!(s.headers[1].repeat_close, 2);
        assert_eq!(s.headers[0].marker.as_deref(), Some("A Verse"));
        assert_eq!(s.headers[1].time_signature, TimeSignature { numerator: 3, denominator: 4 });
        let h = &s.headers[1];
        assert_eq!(h.endings, 0b11);
        assert_eq!(h.targets, vec![DirectionTarget::Fine, DirectionTarget::Segno]);
        assert_eq!(h.jump, Some(Jump::DalSegnoSegno(JumpEnd::DoubleCoda)));
        assert_eq!(h.triplet_feel, TripletFeel::Dotted16th);
        assert!(h.free_time && h.double_bar);
        assert!(!s.headers[0].free_time && s.headers[0].jump.is_none());
        assert_eq!(jump_of("DaCoda"), Some(Jump::ToCoda));
        assert_eq!(jump_of("DaCapoAlFine"), Some(Jump::DaCapo(JumpEnd::Fine)));
        assert_eq!(jump_of("Nonsense"), None);
        let t = &s.tracks[0];
        assert_eq!((t.capo, t.program), (2, 29));
        // GP string 0 is the lowest; ours 0 is the highest.
        assert_eq!(t.tuning.strings, vec![64, 59, 55, 50, 45, 40]);
        let beats = &t.measures[0].voices[0].beats;
        assert_eq!(beats.len(), 2);
        assert_eq!(beats[0].notes.iter().map(|n| (n.string, n.fret)).collect::<Vec<_>>(), vec![(0, 0), (5, 3)]);
        assert!(beats[0].notes[0].effects.palm_mute);
        assert_eq!(beats[0].notes[1].effects.slide, Some(Slide::Legato));
        assert!(beats[0].notes[1].effects.let_ring);
        assert!(beats[1].rest);
        assert_eq!(beats[1].duration.ticks(), 3 * guitario_theory::TICKS_PER_QUARTER);
        assert_eq!(beats[0].dynamic, Dynamic::F);
        let b = &beats[0];
        assert_eq!(b.brush, Some(Brush { kind: BrushKind::Brush, down: false, ticks: 240 * (guitario_theory::TICKS_PER_QUARTER / 960) }));
        assert_eq!((b.stroke, b.technique, b.fade), (Some(StrokeDirection::Down), Some(BeatTechnique::Pop), Some(Fade::Swell)));
        // Note 1 is on GP string 5, the highest: our string 0.
        let n = b.notes.iter().find(|n| n.effects.tapped).unwrap();
        assert!(n.effects.wide_vibrato && !n.effects.vibrato && n.effects.heavy_accent && n.effects.staccato && !n.effects.accent);
        // Empty bar falls back to a rest.
        assert!(t.measures[1].voices[0].beats[0].rest);
    }

    #[test]
    fn reads_every_voice_a_bar_lists() {
        let s = parse_gpif(GPIF).unwrap().score;
        let m = &s.tracks[0].measures[0];
        assert_eq!(m.voices.len(), 2);
        assert_eq!(m.voices[0].beats.len(), 2, "voice 1 is untouched");
        // GP string 2 of 6 counts from the lowest; ours counts from the top.
        let b = &m.voices[1].beats[0];
        assert_eq!((b.notes[0].string, b.notes[0].fret, b.duration.ticks()), (3, 7, 4 * guitario_theory::TICKS_PER_QUARTER));
        assert!(s.validate().is_ok());
    }

    #[test]
    fn a_voice_that_only_rests_is_dropped_and_a_gap_becomes_a_rest() {
        // Voice 2 holds a rest: nothing to keep.
        let silent = GPIF.replace("<Beat id=\"2\"><Rhythm ref=\"2\"/><Notes>2</Notes></Beat>", "<Beat id=\"2\"><Rhythm ref=\"2\"/></Beat>");
        assert_eq!(parse_gpif(&silent).unwrap().score.tracks[0].measures[0].voices.len(), 1);
        // Voice 2 absent but voice 3 present: voice 2 is filled in as a bar-long rest.
        let gap = GPIF.replace("<Voices>0 1 -1 -1</Voices>", "<Voices>0 -1 1 -1</Voices>");
        let m = &parse_gpif(&gap).unwrap().score.tracks[0].measures[0];
        assert_eq!(m.voices.len(), 3);
        assert!(m.voices[1].beats.len() == 1 && m.voices[1].beats[0].rest);
        assert!(!m.voices[2].beats[0].rest);
        // A hostile bar naming a dozen voices keeps at most four.
        let many = GPIF.replace("<Voices>0 1 -1 -1</Voices>", "<Voices>0 1 1 1 1 1 1 1 1 1 1 1</Voices>");
        assert_eq!(parse_gpif(&many).unwrap().score.tracks[0].measures[0].voices.len(), guitario_score::MAX_VOICES);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_gpif("").is_err());
        assert!(parse_gpif("<x/>").is_err());
        assert!(parse_gpif("<GPIF/>").is_err());
    }

    #[test]
    fn overflowing_bar_is_trimmed_with_warning() {
        let xml = GPIF.replace("<Beats>0 1</Beats>", "<Beats>0 1 1 1</Beats>");
        let imp = parse_gpif(&xml).unwrap();
        assert!(imp.score.validate().is_ok());
        assert!(!imp.warnings.is_empty());
    }

    #[test]
    fn bad_container() {
        assert!(matches!(crate::import(b"nope"), Err(GpError::NotGuitarPro)));
        assert!(matches!(crate::import(b"PK nope"), Err(GpError::NotGuitarPro)));
    }
}
