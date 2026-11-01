//! MusicXML 4.0 export (score-partwise): one part per track, the tuning in `<staff-details>`, each
//! note's string and fret in `<technical>`, voices separated by `<backup>`.

use crate::Export;
use guitario_score::{
    Beat, Bend, BendKind, Chord, DirectionTarget, Dynamic, Harmonic, Jump, MeasureHeader, Note, NoteKind, Score, Slide, StrokeDirection, Syllable, Track,
    TripletFeel, dispatch_lyrics,
};
use guitario_theory::{Duration, NoteValue, TICKS_PER_QUARTER};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Escapes text for an element or a double-quoted attribute.
pub(crate) fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

pub(crate) fn type_name(v: NoteValue) -> &'static str {
    match v {
        NoteValue::Whole => "whole",
        NoteValue::Half => "half",
        NoteValue::Quarter => "quarter",
        NoteValue::Eighth => "eighth",
        NoteValue::Sixteenth => "16th",
        NoteValue::ThirtySecond => "32nd",
        NoteValue::SixtyFourth => "64th",
    }
}

/// Step, alter and octave of a MIDI key: sharps in sharp keys and C, flats in flat keys.
pub(crate) fn spell(key: u8, fifths: i8) -> (char, i8, i32) {
    let pc = key % 12;
    let octave = i32::from(key / 12) - 1;
    let sharps = [('C', 0), ('C', 1), ('D', 0), ('D', 1), ('E', 0), ('F', 0), ('F', 1), ('G', 0), ('G', 1), ('A', 0), ('A', 1), ('B', 0)];
    let flats = [('C', 0), ('D', -1), ('D', 0), ('E', -1), ('E', 0), ('F', 0), ('G', -1), ('G', 0), ('A', -1), ('A', 0), ('B', -1), ('B', 0)];
    let table = if fifths < 0 { flats } else { sharps };
    let (step, alter) = table.get(usize::from(pc)).copied().unwrap_or(('C', 0));
    (step, alter, octave)
}

/// The staff position of a drum piece as a display step and octave: half-space steps above the
/// bottom line of a treble staff (E4).
pub(crate) fn drum_display(staff_pos: i8) -> (char, i32) {
    let n = 30 + i32::from(staff_pos); // E4 counted in diatonic steps from C0
    let letters = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];
    (letters.get(n.rem_euclid(7) as usize).copied().unwrap_or('E'), n.div_euclid(7))
}

/// The MusicXML notehead of drum `key` on `track` (`normal` off a drum track).
fn drum_head(track: &Track, key: u8) -> &'static str {
    use guitario_theory::drums::Head;
    match guitario_theory::drums::piece(key).filter(|_| track.is_drums()).map(|p| p.head) {
        Some(Head::X | Head::HeavyX | Head::HeavyXHat) => "x",
        Some(Head::CircleX) => "circle-x",
        Some(Head::CircleSlash) => "circled",
        Some(Head::Diamond) => "diamond",
        Some(Head::Triangle) => "triangle",
        Some(Head::Normal) | None => "normal",
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

pub(crate) fn dynamic_name(d: Dynamic) -> &'static str {
    match d {
        Dynamic::Ppp => "ppp",
        Dynamic::Pp => "pp",
        Dynamic::P => "p",
        Dynamic::Mp => "mp",
        Dynamic::Mf => "mf",
        Dynamic::F => "f",
        Dynamic::Ff => "ff",
        Dynamic::Fff => "fff",
    }
}

/// MusicXML chord kinds for the usual suffixes; anything else is `other` with its text.
pub(crate) const KINDS: [(&str, &str); 14] = [
    ("", "major"),
    ("m", "minor"),
    ("7", "dominant"),
    ("maj7", "major-seventh"),
    ("m7", "minor-seventh"),
    ("dim", "diminished"),
    ("dim7", "diminished-seventh"),
    ("aug", "augmented"),
    ("m7b5", "half-diminished"),
    ("6", "major-sixth"),
    ("m6", "minor-sixth"),
    ("sus2", "suspended-second"),
    ("sus4", "suspended-fourth"),
    ("5", "power"),
];

struct Writer<'a> {
    score: &'a Score,
    out: String,
    /// Ticks per MusicXML division.
    unit: u32,
    warnings: BTreeMap<&'static str, usize>,
}

/// Writes the score as a MusicXML 4.0 partwise document.
pub fn export(score: &Score) -> Export {
    // The coarsest division that measures every beat exactly.
    let mut unit = TICKS_PER_QUARTER;
    for h in &score.headers {
        unit = gcd(unit, h.capacity());
    }
    for b in score.tracks.iter().flat_map(|t| &t.measures).flat_map(|m| &m.voices).flat_map(|v| &v.beats) {
        unit = gcd(unit, b.duration.ticks());
    }
    let mut w = Writer { score, out: String::new(), unit: unit.max(1), warnings: BTreeMap::new() };
    w.document();
    let warnings = w.warnings.iter().map(|(what, n)| format!("{n} × {what} not written to MusicXML")).collect();
    Export { xml: w.out, warnings }
}

impl Writer<'_> {
    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn warn(&mut self, what: &'static str) {
        *self.warnings.entry(what).or_insert(0) += 1;
    }

    fn document(&mut self) {
        let s = self.score;
        self.line(0, r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>"#);
        self.line(0, r#"<!DOCTYPE score-partwise PUBLIC "-//Recordare//DTD MusicXML 4.0 Partwise//EN" "http://www.musicxml.org/dtds/partwise.dtd">"#);
        self.line(0, r#"<score-partwise version="4.0">"#);
        if !s.meta.title.is_empty() {
            self.line(1, "<work>");
            self.line(2, &format!("<work-title>{}</work-title>", esc(&s.meta.title)));
            self.line(1, "</work>");
        }
        if !s.meta.subtitle.is_empty() {
            self.line(1, &format!("<movement-title>{}</movement-title>", esc(&s.meta.subtitle)));
        }
        self.line(1, "<identification>");
        for (kind, text) in [("composer", &s.meta.music), ("lyricist", &s.meta.words), ("artist", &s.meta.artist), ("transcriber", &s.meta.author)] {
            if !text.is_empty() {
                self.line(2, &format!(r#"<creator type="{kind}">{}</creator>"#, esc(text)));
            }
        }
        if !s.meta.copyright.is_empty() {
            self.line(2, &format!("<rights>{}</rights>", esc(&s.meta.copyright)));
        }
        self.line(2, "<encoding>");
        self.line(3, "<software>Guitario</software>");
        for feature in ["staff-details", "string", "fret"] {
            self.line(3, &format!(r#"<supports element="{feature}" type="yes"/>"#));
        }
        self.line(2, "</encoding>");
        self.line(1, "</identification>");
        self.part_list();
        for ti in 0..s.tracks.len() {
            self.part(ti);
        }
        self.line(0, "</score-partwise>");
    }

    fn drums_used(track: &Track) -> Vec<u8> {
        let notes = track.measures.iter().flat_map(|m| &m.voices).flat_map(|v| &v.beats).flat_map(|b| &b.notes);
        let mut keys: Vec<u8> = notes.flat_map(|n| std::iter::once(n.fret).chain(n.effects.grace.map(|g| g.fret))).collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    fn part_list(&mut self) {
        self.line(1, "<part-list>");
        let mut channel = 0u8;
        for (ti, t) in self.score.tracks.iter().enumerate() {
            let id = ti + 1;
            self.line(2, &format!(r#"<score-part id="P{id}">"#));
            self.line(3, &format!("<part-name>{}</part-name>", esc(&t.name)));
            if !t.short_name.is_empty() {
                self.line(3, &format!("<part-abbreviation>{}</part-abbreviation>", esc(&t.short_name)));
            }
            let volume = f64::from(t.volume) * 100.0 / 127.0;
            let pan = f64::from(t.pan) * 90.0 / 63.0;
            if t.is_drums() {
                let keys = Self::drums_used(t);
                for k in &keys {
                    let name = guitario_theory::drums::piece(*k).map_or_else(|| format!("Drum {k}"), |p| p.name.to_string());
                    self.line(3, &format!(r#"<score-instrument id="P{id}-I{k}"><instrument-name>{}</instrument-name></score-instrument>"#, esc(&name)));
                }
                for k in &keys {
                    self.line(
                        3,
                        &format!(
                            r#"<midi-instrument id="P{id}-I{k}"><midi-channel>10</midi-channel><midi-program>1</midi-program><midi-unpitched>{}</midi-unpitched><volume>{volume:.1}</volume><pan>{pan:.0}</pan></midi-instrument>"#,
                            // Guitar Pro's articulations sound as the General MIDI drum they vary.
                            u16::from(guitario_theory::drums::plays(*k)) + 1
                        ),
                    );
                }
            } else {
                // Channels 1-16 skipping 10, as the MIDI export assigns them.
                channel = if channel == 9 { 11 } else { channel + 1 };
                let ch = if channel > 16 { 16 } else { channel };
                // A section says how many play it.
                let ensemble = t.section().map_or_else(String::new, |n| format!("<ensemble>{n}</ensemble>"));
                self.line(3, &format!(r#"<score-instrument id="P{id}-I1"><instrument-name>{}</instrument-name>{ensemble}</score-instrument>"#, esc(&t.name)));
                self.line(
                    3,
                    &format!(
                        r#"<midi-instrument id="P{id}-I1"><midi-channel>{ch}</midi-channel><midi-program>{}</midi-program><volume>{volume:.1}</volume><pan>{pan:.0}</pan></midi-instrument>"#,
                        u16::from(t.program) + 1
                    ),
                );
            }
            self.line(2, "</score-part>");
        }
        self.line(1, "</part-list>");
    }

    fn part(&mut self, ti: usize) {
        let s = self.score;
        let Some(track) = s.tracks.get(ti) else { return };
        self.line(1, &format!(r#"<part id="P{}">"#, ti + 1));
        // Syllables by (bar, beat) of voice 1.
        let mut lyrics: BTreeMap<(usize, usize), Vec<(usize, Syllable)>> = BTreeMap::new();
        for (li, line) in track.lyrics.iter().enumerate() {
            let placed = dispatch_lyrics(track, line);
            // Hyphens join a syllable to the next: begin, middle, end, single.
            for (mi, bi, syl) in placed {
                if !syl.text.is_empty() {
                    lyrics.entry((mi, bi)).or_default().push((li, syl));
                }
            }
        }
        let mut open_hyphen = vec![false; guitario_score::MAX_LYRIC_LINES];
        let mut last_dynamic: Option<Dynamic> = None;
        for (mi, header) in s.headers.iter().enumerate() {
            let prev = mi.checked_sub(1).and_then(|p| s.headers.get(p));
            let next = s.headers.get(mi + 1);
            self.line(2, &format!(r#"<measure number="{}">"#, mi + 1));
            self.left_barline(header, prev);
            self.attributes(track, header, prev, mi == 0);
            if ti == 0 {
                self.bar_directions(header, mi);
            }
            if header.triplet_feel != TripletFeel::None && prev.is_none_or(|p| p.triplet_feel != header.triplet_feel) && ti == 0 {
                self.swing(header.triplet_feel);
            }
            let Some(measure) = track.measures.get(mi) else {
                self.line(2, "</measure>");
                continue;
            };
            // The ticks of the voice written last: the next one backs up by as much.
            let mut written: Option<u32> = None;
            for (vi, voice) in measure.voices.iter().enumerate() {
                if vi > 0 && voice.beats.iter().all(|b| b.rest) {
                    continue;
                }
                if let Some(done) = written.filter(|d| *d > 0) {
                    self.line(3, &format!("<backup><duration>{}</duration></backup>", done / self.unit));
                }
                if voice.beats.is_empty() {
                    self.line(
                        3,
                        &format!("<note><rest measure=\"yes\"/><duration>{}</duration><voice>{}</voice></note>", header.capacity() / self.unit, vi + 1),
                    );
                    written = Some(header.capacity());
                    continue;
                }
                let mut tuplet = TupletState::default();
                let mut at = 0u32;
                for (bi, beat) in voice.beats.iter().enumerate() {
                    if vi == 0 && !beat.rest && last_dynamic != Some(beat.dynamic) {
                        if last_dynamic.is_some() || beat.dynamic != Dynamic::Mf {
                            self.line(
                                3,
                                &format!(
                                    r#"<direction placement="below"><direction-type><dynamics><{}/></dynamics></direction-type><voice>1</voice></direction>"#,
                                    dynamic_name(beat.dynamic)
                                ),
                            );
                        }
                        last_dynamic = Some(beat.dynamic);
                    }
                    if let Some(text) = beat.text.as_deref().filter(|t| !t.is_empty()) {
                        self.line(
                            3,
                            &format!(
                                r#"<direction placement="above"><direction-type><words>{}</words></direction-type><voice>{}</voice></direction>"#,
                                esc(text),
                                vi + 1
                            ),
                        );
                    }
                    if let Some(chord) = &beat.chord {
                        self.harmony(chord);
                    }
                    if beat.whammy.is_some() || beat.bar_vibrato.is_some() {
                        self.warn("whammy bar effect");
                    }
                    if beat.fade.is_some() {
                        self.warn("fade");
                    }
                    let syllables = if vi == 0 { lyrics.get(&(mi, bi)).cloned().unwrap_or_default() } else { Vec::new() };
                    let fermata = vi == 0 && ti == 0 && header.fermatas.iter().any(|f| f.tick == at);
                    let ctx = BeatCtx { track, mi, vi, bi, key: header.key, fermata };
                    self.beat(&ctx, beat, &mut tuplet, &syllables, &mut open_hyphen);
                    at = at.saturating_add(beat.duration.ticks());
                }
                written = Some(at);
            }
            if ti == 0
                && let Some(jump) = header.jump
            {
                self.jump(jump);
            }
            self.right_barline(header, next);
            self.line(2, "</measure>");
        }
        self.line(1, "</part>");
    }
}

/// Where a beat sits, for looking at its neighbours on the same string.
struct BeatCtx<'a> {
    track: &'a Track,
    mi: usize,
    vi: usize,
    bi: usize,
    key: i8,
    fermata: bool,
}

impl BeatCtx<'_> {
    fn beat_at(&self, mi: usize, bi: usize) -> Option<&Beat> {
        self.track.measures.get(mi)?.voices.get(self.vi)?.beats.get(bi)
    }

    /// The next beat of the voice, in this bar or the next.
    fn next(&self) -> Option<&Beat> {
        self.beat_at(self.mi, self.bi + 1).or_else(|| self.track.measures.get(self.mi + 1)?.voices.get(self.vi)?.beats.first())
    }

    fn prev(&self) -> Option<&Beat> {
        match self.bi.checked_sub(1) {
            Some(b) => self.beat_at(self.mi, b),
            None => self.track.measures.get(self.mi.checked_sub(1)?)?.voices.get(self.vi)?.beats.last(),
        }
    }
}

/// An open tuplet bracket: its ratio, how long the group is and how much of it is written.
#[derive(Default)]
struct TupletState {
    open: Option<((u8, u8), u32, u32)>,
}

impl TupletState {
    /// Whether this beat starts and whether it ends a bracket.
    fn step(&mut self, d: Duration) -> (bool, bool) {
        let (n, m) = d.tuplet;
        if n == 0 || m == 0 || (n, m) == (1, 1) {
            self.open = None;
            return (false, false);
        }
        let ticks = d.ticks();
        let mut start = false;
        let (target, so_far) = match self.open {
            Some((ratio, target, so_far)) if ratio == (n, m) => (target, so_far + ticks),
            _ => {
                start = true;
                (Duration { tuplet: (1, 1), ..d }.ticks() * u32::from(m), ticks)
            }
        };
        if so_far >= target {
            self.open = None;
            (start, true)
        } else {
            self.open = Some(((n, m), target, so_far));
            (start, false)
        }
    }
}

fn semitones(quarter_tones: u8) -> String {
    if quarter_tones.is_multiple_of(2) { format!("{}", quarter_tones / 2) } else { format!("{}.5", quarter_tones / 2) }
}

/// Guitar Pro's bend types as MusicXML `<bend>` elements.
fn bend_xml(b: &Bend, tied: bool) -> String {
    let q = b.peak();
    let up = |q: u8, extra: &str| format!("<bend><bend-alter>{}</bend-alter>{extra}</bend>", semitones(q));
    let down = |q: u8| format!("<bend><bend-alter>-{}</bend-alter><release/></bend>", semitones(q));
    match b.kind(tied) {
        Some(BendKind::Bend) | None => up(q, ""),
        Some(BendKind::Release) => down(q),
        Some(BendKind::BendRelease) => format!("{}{}", up(q, ""), down(q)),
        Some(BendKind::Hold | BendKind::Prebend) => up(q, "<pre-bend/>"),
        Some(BendKind::PrebendBend) => {
            let start = b.start();
            format!("{}{}", up(start, "<pre-bend/>"), up(q.saturating_sub(start).max(1), ""))
        }
        Some(BendKind::PrebendRelease) => format!("{}{}", up(q, "<pre-bend/>"), down(q)),
    }
}

/// The ending numbers of a bit set: `"1, 2"`.
fn ending_list(bits: u8) -> String {
    (0..8).filter(|i| bits & (1 << i) != 0).map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(", ")
}

impl Writer<'_> {
    fn left_barline(&mut self, h: &MeasureHeader, prev: Option<&MeasureHeader>) {
        let ending_start = h.endings != 0 && prev.is_none_or(|p| p.endings != h.endings);
        if !h.repeat_open && !ending_start {
            return;
        }
        self.line(3, r#"<barline location="left">"#);
        if h.repeat_open {
            self.line(4, "<bar-style>heavy-light</bar-style>");
        }
        if ending_start {
            let list = ending_list(h.endings);
            self.line(4, &format!(r#"<ending number="{list}" type="start">{list}.</ending>"#));
        }
        if h.repeat_open {
            self.line(4, r#"<repeat direction="forward"/>"#);
        }
        self.line(3, "</barline>");
    }

    fn right_barline(&mut self, h: &MeasureHeader, next: Option<&MeasureHeader>) {
        let ending_stop = h.endings != 0 && next.is_none_or(|n| n.endings != h.endings);
        let last = next.is_none();
        if h.repeat_close == 0 && !ending_stop && !h.double_bar && !last {
            return;
        }
        self.line(3, r#"<barline location="right">"#);
        let style = if h.repeat_close > 0 || last {
            "light-heavy"
        } else if h.double_bar {
            "light-light"
        } else {
            "regular"
        };
        self.line(4, &format!("<bar-style>{style}</bar-style>"));
        if ending_stop {
            let kind = if h.repeat_close > 0 { "stop" } else { "discontinue" };
            self.line(4, &format!(r#"<ending number="{}" type="{kind}"/>"#, ending_list(h.endings)));
        }
        if h.repeat_close > 0 {
            self.line(4, &format!(r#"<repeat direction="backward" times="{}"/>"#, u16::from(h.repeat_close) + 1));
        }
        self.line(3, "</barline>");
    }

    fn attributes(&mut self, track: &Track, h: &MeasureHeader, prev: Option<&MeasureHeader>, first: bool) {
        let key = first || prev.is_some_and(|p| p.key != h.key);
        let time = first || prev.is_some_and(|p| p.time_signature != h.time_signature);
        if !key && !time {
            return;
        }
        self.line(3, "<attributes>");
        if first {
            self.line(4, &format!("<divisions>{}</divisions>", TICKS_PER_QUARTER / self.unit));
        }
        if key {
            self.line(4, &format!("<key><fifths>{}</fifths></key>", h.key.clamp(-7, 7)));
        }
        if time {
            if h.free_time {
                self.warn("free time");
            }
            self.line(4, &format!("<time><beats>{}</beats><beat-type>{}</beat-type></time>", h.time_signature.numerator, h.time_signature.denominator));
        }
        if first {
            if track.is_drums() {
                self.line(4, "<clef><sign>percussion</sign></clef>");
                self.line(4, "<staff-details><staff-lines>5</staff-lines></staff-details>");
            } else {
                let (standard, tab) = track.notation.drawn();
                let low = track.tuning.strings.iter().copied().min().unwrap_or(40);
                let clef = if !standard && tab {
                    "<clef><sign>TAB</sign><line>5</line></clef>".to_string()
                } else if low < 36 {
                    "<clef><sign>F</sign><line>4</line><clef-octave-change>-1</clef-octave-change></clef>".to_string()
                } else {
                    "<clef><sign>G</sign><line>2</line><clef-octave-change>-1</clef-octave-change></clef>".to_string()
                };
                self.line(4, &clef);
                let n = track.string_count();
                let mut details = String::from("<staff-details>");
                if !standard && tab {
                    let _ = write!(details, "<staff-lines>{n}</staff-lines>");
                }
                // Line 1 is the lowest string; strings are stored from the top.
                for (line, open) in track.tuning.strings.iter().rev().enumerate() {
                    let (step, alter, octave) = spell(*open, 0);
                    let alter = if alter != 0 { format!("<tuning-alter>{alter}</tuning-alter>") } else { String::new() };
                    let _ = write!(
                        details,
                        r#"<staff-tuning line="{}"><tuning-step>{step}</tuning-step>{alter}<tuning-octave>{octave}</tuning-octave></staff-tuning>"#,
                        line + 1
                    );
                }
                if track.capo > 0 {
                    let _ = write!(details, "<capo>{}</capo>", track.capo);
                }
                details.push_str("</staff-details>");
                self.line(4, &details);
            }
        }
        self.line(3, "</attributes>");
    }

    fn bar_directions(&mut self, h: &MeasureHeader, mi: usize) {
        let tempo = if mi == 0 { Some(h.tempo.unwrap_or(self.score.tempo)) } else { h.tempo };
        if let Some(t) = tempo {
            self.line(
                3,
                &format!(
                    r#"<direction placement="above"><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>{t}</per-minute></metronome></direction-type><sound tempo="{t}"/></direction>"#
                ),
            );
        }
        if let Some(name) = &h.marker {
            let mut d = String::from(r#"<direction placement="above">"#);
            if let Some(letter) = h.section_letter.as_deref().filter(|l| !l.is_empty()) {
                let _ = write!(d, "<direction-type><rehearsal>{}</rehearsal></direction-type>", esc(letter));
                if !name.is_empty() {
                    let _ = write!(d, "<direction-type><words>{}</words></direction-type>", esc(name));
                }
            } else {
                let _ = write!(d, "<direction-type><rehearsal>{}</rehearsal></direction-type>", esc(name));
            }
            d.push_str("</direction>");
            self.line(3, &d);
        }
        for t in &h.targets {
            let d = match t {
                DirectionTarget::Segno => r#"<direction placement="above"><direction-type><segno/></direction-type><sound segno="segno"/></direction>"#,
                DirectionTarget::Coda => r#"<direction placement="above"><direction-type><coda/></direction-type><sound coda="coda"/></direction>"#,
                DirectionTarget::SegnoSegno => {
                    r#"<direction placement="above"><direction-type><words>Segno Segno</words></direction-type><sound segno="segno2"/></direction>"#
                }
                DirectionTarget::DoubleCoda => {
                    r#"<direction placement="above"><direction-type><words>Double Coda</words></direction-type><sound coda="coda2"/></direction>"#
                }
                DirectionTarget::Fine => r#"<direction placement="above"><direction-type><words>Fine</words></direction-type><sound fine="yes"/></direction>"#,
            };
            self.line(3, d);
        }
    }

    fn jump(&mut self, jump: Jump) {
        let sound = match jump {
            Jump::DaCapo(_) => r#"dacapo="yes""#,
            Jump::DalSegno(_) => r#"dalsegno="segno""#,
            Jump::DalSegnoSegno(_) => r#"dalsegno="segno2""#,
            Jump::ToCoda => r#"tocoda="coda""#,
            Jump::ToDoubleCoda => r#"tocoda="coda2""#,
        };
        self.line(
            3,
            &format!(r#"<direction placement="above"><direction-type><words>{}</words></direction-type><sound {sound}/></direction>"#, esc(jump.label())),
        );
    }

    fn swing(&mut self, feel: TripletFeel) {
        let (first, second, unit) = match feel {
            TripletFeel::None => return,
            TripletFeel::Triplet8th => (2, 1, "eighth"),
            TripletFeel::Triplet16th => (2, 1, "16th"),
            TripletFeel::Dotted8th => (3, 1, "eighth"),
            TripletFeel::Dotted16th => (3, 1, "16th"),
            TripletFeel::Scottish8th => (1, 3, "eighth"),
            TripletFeel::Scottish16th => (1, 3, "16th"),
        };
        self.line(
            3,
            &format!(
                r#"<direction placement="above"><direction-type><words>Swing</words></direction-type><sound><swing><first>{first}</first><second>{second}</second><swing-type>{unit}</swing-type></swing></sound></direction>"#
            ),
        );
    }

    fn harmony(&mut self, chord: &Chord) {
        let name = chord.name.trim();
        let (main, bass) = name.split_once('/').map_or((name, None), |(a, b)| (a, Some(b)));
        let root = |text: &str| -> Option<(char, i8, usize)> {
            let mut chars = text.chars();
            let step = chars.next().filter(|c| ('A'..='G').contains(c))?;
            match chars.next() {
                Some('#') => Some((step, 1, 2)),
                Some('b') => Some((step, -1, 2)),
                _ => Some((step, 0, 1)),
            }
        };
        let Some((step, alter, used)) = root(main) else {
            if !name.is_empty() {
                self.warn("chord name without a root note");
            }
            return;
        };
        let suffix = main.get(used..).unwrap_or("");
        let kind = KINDS.iter().find(|(s, _)| *s == suffix).map_or("other", |(_, k)| k);
        let mut h = format!("<harmony><root><root-step>{step}</root-step>");
        if alter != 0 {
            let _ = write!(h, "<root-alter>{alter}</root-alter>");
        }
        let _ = write!(h, r#"</root><kind text="{}">{kind}</kind>"#, esc(suffix));
        if let Some((bstep, balter, _)) = bass.and_then(root) {
            let _ = write!(h, "<bass><bass-step>{bstep}</bass-step>");
            if balter != 0 {
                let _ = write!(h, "<bass-alter>{balter}</bass-alter>");
            }
            h.push_str("</bass>");
        }
        if chord.frets.iter().any(Option::is_some) {
            let _ = write!(h, "<frame><frame-strings>{}</frame-strings><frame-frets>5</frame-frets>", chord.frets.len());
            if chord.first_fret > 1 {
                let _ = write!(h, "<first-fret>{}</first-fret>", chord.first_fret);
            }
            for (i, f) in chord.frets.iter().enumerate() {
                if let Some(f) = f {
                    let _ = write!(h, "<frame-note><string>{}</string><fret>{f}</fret></frame-note>", i + 1);
                }
            }
            h.push_str("</frame>");
        }
        h.push_str("</harmony>");
        self.line(3, &h);
    }

    fn duration_xml(&self, d: Duration) -> String {
        let mut out = format!("<type>{}</type>", type_name(d.value));
        for _ in 0..d.dots.min(2) {
            out.push_str("<dot/>");
        }
        let (n, m) = d.tuplet;
        if n > 0 && m > 0 && (n, m) != (1, 1) {
            let _ = write!(out, "<time-modification><actual-notes>{n}</actual-notes><normal-notes>{m}</normal-notes></time-modification>");
        }
        out
    }

    fn beat(&mut self, ctx: &BeatCtx, beat: &Beat, tuplet: &mut TupletState, syllables: &[(usize, Syllable)], open_hyphen: &mut [bool]) {
        let dur = beat.duration.ticks() / self.unit;
        let (t_start, t_stop) = tuplet.step(beat.duration);
        let mut tuplet_marks = String::new();
        if t_start {
            tuplet_marks.push_str(r#"<tuplet type="start" bracket="yes"/>"#);
        }
        if t_stop {
            tuplet_marks.push_str(r#"<tuplet type="stop"/>"#);
        }
        let voice = ctx.vi + 1;
        let fermata = if ctx.fermata { r#"<fermata type="upright"/>"# } else { "" };
        if beat.rest || beat.notes.is_empty() {
            let notations =
                if tuplet_marks.is_empty() && fermata.is_empty() { String::new() } else { format!("<notations>{tuplet_marks}{fermata}</notations>") };
            self.line(3, &format!("<note><rest/><duration>{dur}</duration><voice>{voice}</voice>{}{notations}</note>", self.duration_xml(beat.duration)));
            return;
        }
        let track = ctx.track;
        let pitch_of = |n: &Note| -> Option<u8> {
            if track.is_drums() { Some(n.fret) } else { track.tuning.pitch(usize::from(n.string), n.fret, track.capo).map(|p| p.0) }
        };
        let mut notes: Vec<&Note> = beat.notes.iter().collect();
        notes.sort_by_key(|n| pitch_of(n).unwrap_or(0));
        // Grace notes come first, as a chord of their own.
        let graces: Vec<&Note> = notes.iter().copied().filter(|n| n.effects.grace.is_some()).collect();
        for (gi, n) in graces.iter().enumerate() {
            let Some(g) = n.effects.grace else { continue };
            let Some(key) = (if track.is_drums() { Some(g.fret) } else { track.tuning.pitch(usize::from(n.string), g.fret, track.capo).map(|p| p.0) }) else {
                continue;
            };
            let slash = if g.on_beat { "no" } else { "yes" };
            let chord = if gi > 0 { "<chord/>" } else { "" };
            let value = match g.duration {
                guitario_score::GraceDuration::Sixteenth => "16th",
                guitario_score::GraceDuration::ThirtySecond => "32nd",
                guitario_score::GraceDuration::SixtyFourth => "64th",
            };
            let head = match if g.dead { "x" } else { drum_head(track, key) } {
                "normal" => String::new(),
                h => format!("<notehead>{h}</notehead>"),
            };
            let instrument = if track.is_drums() { format!(r#"<instrument id="P{}-I{key}"/>"#, self.part_of(track)) } else { String::new() };
            let transition = if g.transition == guitario_score::GraceTransition::Slide { r#"<slide type="start" line-type="solid"/>"# } else { "" };
            let hammer = if g.transition == guitario_score::GraceTransition::Hammer { r#"<hammer-on type="start">H</hammer-on>"# } else { "" };
            let technical = if track.is_drums() {
                String::new()
            } else {
                format!("<technical><string>{}</string><fret>{}</fret>{hammer}</technical>", u16::from(n.string) + 1, g.fret)
            };
            let pitch = self.pitch_xml(track, key, ctx.key);
            self.line(3, &format!(r#"<note><grace slash="{slash}"/>{chord}{pitch}{instrument}<voice>{voice}</voice><type>{value}</type>{head}<notations>{transition}{technical}</notations></note>"#));
        }
        let (prev, next) = (ctx.prev(), ctx.next());
        let mut first = true;
        for n in notes {
            let Some(key) = pitch_of(n) else {
                self.warn("note outside the MIDI range");
                continue;
            };
            let e = &n.effects;
            let mut x = String::from("<note>");
            if !first {
                x.push_str("<chord/>");
            }
            x.push_str(&self.pitch_xml(track, key, ctx.key));
            let _ = write!(x, "<duration>{dur}</duration>");
            let tie_stop = n.kind == NoteKind::Tie;
            let after = next.and_then(|b| b.note_on(n.string));
            let before = prev.and_then(|b| b.note_on(n.string));
            let tie_start = after.is_some_and(|a| a.kind == NoteKind::Tie);
            if tie_stop {
                x.push_str(r#"<tie type="stop"/>"#);
            }
            if tie_start {
                x.push_str(r#"<tie type="start"/>"#);
            }
            if track.is_drums() {
                let _ = write!(x, r#"<instrument id="P{}-I{key}"/>"#, self.part_of(track));
            }
            let _ = write!(x, "<voice>{voice}</voice>{}", self.duration_xml(beat.duration));
            let head = if n.kind == NoteKind::Dead { "x" } else { drum_head(track, key) };
            match (head, e.ghost) {
                ("normal", false) => {}
                (head, true) => {
                    let _ = write!(x, r#"<notehead parentheses="yes">{head}</notehead>"#);
                }
                (head, false) => {
                    let _ = write!(x, "<notehead>{head}</notehead>");
                }
            }
            let mut notations = String::new();
            if tie_stop {
                notations.push_str(r#"<tied type="stop"/>"#);
            }
            if tie_start {
                notations.push_str(r#"<tied type="start"/>"#);
            }
            // A slur stops on the first beat after its run and starts on its first slurred beat.
            if first && prev.is_some_and(|p| p.legato) && !beat.legato {
                notations.push_str(r#"<slur type="stop" number="1"/>"#);
            }
            if first && beat.legato && !prev.is_some_and(|p| p.legato) {
                notations.push_str(r#"<slur type="start" number="1"/>"#);
            }
            if first {
                notations.push_str(&tuplet_marks);
            }
            // A slur joins hammer-ons and pull-offs, as notation programs draw them.
            if before.is_some_and(|b| b.effects.hammer) {
                notations.push_str(r#"<slur type="stop" number="2"/>"#);
            }
            if e.hammer && after.is_some() {
                notations.push_str(r#"<slur type="start" number="2"/>"#);
            }
            // Slides between two notes on the string: a shift slide, or a legato one (glissando).
            match before.and_then(|b| b.effects.slide) {
                Some(Slide::Shift) => notations.push_str(r#"<slide type="stop" line-type="solid"/>"#),
                Some(Slide::Legato) => notations.push_str(r#"<glissando type="stop" line-type="solid"/>"#),
                _ => {}
            }
            match e.slide {
                Some(Slide::Shift) if after.is_some() => notations.push_str(r#"<slide type="start" line-type="solid"/>"#),
                Some(Slide::Legato) if after.is_some() => notations.push_str(r#"<glissando type="start" line-type="solid"/>"#),
                Some(Slide::Shift | Slide::Legato) => self.warn("slide to no following note"),
                _ => {}
            }
            let mut ornaments = String::new();
            if e.vibrato {
                ornaments.push_str(r#"<other-ornament smufl="guitarVibratoStroke"/>"#);
            }
            if e.wide_vibrato {
                ornaments.push_str(r#"<other-ornament smufl="guitarWideVibratoStroke"/>"#);
            }
            if e.trill.is_some() {
                ornaments.push_str("<trill-mark/>");
            }
            if let Some(t) = beat.tremolo {
                let _ = write!(ornaments, r#"<tremolo type="single">{}</tremolo>"#, t.slashes());
            }
            if !ornaments.is_empty() {
                let _ = write!(notations, "<ornaments>{ornaments}</ornaments>");
            }
            let mut technical = String::new();
            if !track.is_drums() {
                let _ = write!(technical, "<string>{}</string><fret>{}</fret>", u16::from(n.string) + 1, n.fret);
            }
            if first {
                match beat.stroke {
                    Some(StrokeDirection::Up) => technical.push_str("<up-bow/>"),
                    Some(StrokeDirection::Down) => technical.push_str("<down-bow/>"),
                    None => {}
                }
            }
            if let Some(b) = before.filter(|b| b.effects.hammer) {
                if n.fret >= b.fret {
                    technical.push_str(r#"<hammer-on type="stop"/>"#);
                } else {
                    technical.push_str(r#"<pull-off type="stop"/>"#);
                }
            }
            if e.hammer {
                match after {
                    Some(a) if a.fret >= n.fret => technical.push_str(r#"<hammer-on type="start">H</hammer-on>"#),
                    Some(_) => technical.push_str(r#"<pull-off type="start">P</pull-off>"#),
                    None => self.warn("hammer-on to no following note"),
                }
            }
            if let Some(b) = &e.bend {
                technical.push_str(&bend_xml(b, tie_stop));
            }
            match e.harmonic {
                Some(Harmonic::Natural) => technical.push_str("<harmonic><natural/></harmonic>"),
                Some(Harmonic::Artificial) => technical.push_str("<harmonic><artificial/></harmonic>"),
                Some(Harmonic::Pinch) => technical.push_str("<harmonic><artificial/></harmonic><other-technical>pinch harmonic</other-technical>"),
                Some(Harmonic::Tapped) => technical.push_str("<harmonic><artificial/></harmonic><other-technical>tapped harmonic</other-technical>"),
                Some(Harmonic::Semi) => technical.push_str("<harmonic><artificial/></harmonic><other-technical>semi harmonic</other-technical>"),
                None => {}
            }
            if e.tapped {
                technical.push_str("<tap/>");
            }
            if e.left_hand_tapped {
                technical.push_str(r#"<tap hand="left"/>"#);
            }
            if let Some(f) = e.left_finger {
                let _ = write!(technical, "<fingering>{}</fingering>", f.left_mark());
            }
            if let Some(f) = e.right_finger {
                let _ = write!(technical, "<pluck>{}</pluck>", f.right_mark());
            }
            if e.palm_mute {
                technical.push_str("<other-technical>palm mute</other-technical>");
            }
            if e.let_ring {
                technical.push_str("<other-technical>let ring</other-technical>");
            }
            if first {
                match beat.technique {
                    Some(guitario_score::BeatTechnique::Slap) => technical.push_str("<other-technical>slap</other-technical>"),
                    Some(guitario_score::BeatTechnique::Pop) => technical.push_str("<other-technical>pop</other-technical>"),
                    None => {}
                }
            }
            if !technical.is_empty() {
                let _ = write!(notations, "<technical>{technical}</technical>");
            }
            let mut articulations = String::new();
            if e.accent {
                articulations.push_str("<accent/>");
            }
            if e.heavy_accent {
                articulations.push_str("<strong-accent/>");
            }
            if e.staccato {
                articulations.push_str("<staccato/>");
            }
            match e.slide {
                Some(Slide::IntoFromBelow) => articulations.push_str("<scoop/>"),
                Some(Slide::IntoFromAbove) => articulations.push_str("<plop/>"),
                Some(Slide::OutUp) => articulations.push_str("<doit/>"),
                Some(Slide::OutDown) => articulations.push_str("<falloff/>"),
                _ => {}
            }
            if !articulations.is_empty() {
                let _ = write!(notations, "<articulations>{articulations}</articulations>");
            }
            if first {
                notations.push_str(fermata);
            }
            if let Some(brush) = beat.brush {
                let dir = if brush.down { "up" } else { "down" };
                let _ = write!(notations, r#"<arpeggiate direction="{dir}"/>"#);
            }
            if !notations.is_empty() {
                let _ = write!(x, "<notations>{notations}</notations>");
            }
            if first {
                for (li, syl) in syllables {
                    let Some(open) = open_hyphen.get_mut(*li) else { continue };
                    let syllabic = match (*open, syl.hyphen) {
                        (false, false) => "single",
                        (false, true) => "begin",
                        (true, true) => "middle",
                        (true, false) => "end",
                    };
                    *open = syl.hyphen;
                    let _ = write!(x, r#"<lyric number="{}"><syllabic>{syllabic}</syllabic><text>{}</text></lyric>"#, li + 1, esc(&syl.text));
                }
            }
            x.push_str("</note>");
            self.line(3, &x);
            first = false;
        }
    }

    fn part_of(&self, track: &Track) -> usize {
        self.score.tracks.iter().position(|t| std::ptr::eq(t, track)).map_or(1, |i| i + 1)
    }

    fn pitch_xml(&self, track: &Track, key: u8, fifths: i8) -> String {
        if track.is_drums() {
            let pos = guitario_theory::drums::piece(key).map_or(5, |p| p.staff_pos);
            let (step, octave) = drum_display(pos);
            return format!("<unpitched><display-step>{step}</display-step><display-octave>{octave}</display-octave></unpitched>");
        }
        let (step, alter, octave) = spell(key, fifths);
        let alter = if alter != 0 { format!("<alter>{alter}</alter>") } else { String::new() };
        format!("<pitch><step>{step}</step>{alter}<octave>{octave}</octave></pitch>")
    }
}
