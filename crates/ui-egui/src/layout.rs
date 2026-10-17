//! Page layout without drawing: measure spacing, system breaks with justification, and pages.
//! Dimensions follow Guitar Pro's default stylesheet: A4 portrait, 10 mm side margins, 15 mm top,
//! a 1.5875 mm staff space.

use guitario_score::{Beat, Chord, Measure, PageSetup, Score, Track};
use std::collections::HashMap;

const MM: f32 = 96.0 / 25.4;

/// How the score is laid out on screen, as Guitar Pro's View › Layout offers: pages, one long
/// sheet as wide as the window, or one line of bars.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutMode {
    #[default]
    Page,
    Screen,
    Horizontal,
}

/// Height of a page that never ends (screen and horizontal layouts).
const ENDLESS: f32 = 1.0e9;

#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub zoom: f32,
    pub space: f32,
    pub tab_gap: f32,
    pub page_w: f32,
    pub page_h: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub notation: bool,
    pub strings: usize,
    /// A drum track: one percussion staff, no tablature.
    pub drums: bool,
    /// The track has chord names or beat text: a taller lane above each system holds them.
    pub annotations: bool,
    /// The track has chord diagrams: the lane is taller still, with the diagrams under the names.
    pub diagrams: bool,
    /// Lines of lyrics under every system.
    pub lyric_lines: usize,
    /// Chord diagrams listed under the title on the first page, and the strings each has.
    pub chord_list: usize,
    pub chord_strings: usize,
    /// The voice being edited, 0-based. A voice with only rests is drawn when it is this one.
    pub voice: usize,
    pub mode: LayoutMode,
    /// The top track of a system: it carries the bar's markers, tempo and directions. A track
    /// below it in the multitrack view has less room above.
    pub top: bool,
}

impl Metrics {
    pub fn new(zoom: f32, notation: bool, strings: usize) -> Metrics {
        let z = zoom.clamp(0.25, 8.0);
        let space = 1.5875 * MM * z;
        Metrics {
            zoom: z,
            space,
            tab_gap: space * 1.55,
            page_w: 210.0 * MM * z,
            page_h: 297.0 * MM * z,
            margin_left: 10.0 * MM * z,
            margin_right: 10.0 * MM * z,
            margin_top: 15.0 * MM * z,
            margin_bottom: 12.0 * MM * z,
            notation,
            strings: strings.max(1),
            drums: false,
            annotations: false,
            diagrams: false,
            lyric_lines: 0,
            chord_list: 0,
            chord_strings: 6,
            voice: 0,
            mode: LayoutMode::Page,
            top: true,
        }
    }

    /// The screen layout: one sheet `width` pixels wide that never breaks into pages.
    pub fn with_screen(self, width: f32) -> Metrics {
        let side = 6.0 * MM * self.zoom;
        Metrics {
            mode: LayoutMode::Screen,
            page_w: width.max(110.0 * MM * self.zoom),
            page_h: ENDLESS,
            margin_left: side,
            margin_right: side,
            margin_top: 8.0 * MM * self.zoom,
            margin_bottom: 8.0 * MM * self.zoom,
            ..self
        }
    }

    /// The horizontal layout: every bar on one line, as wide as it needs.
    pub fn with_horizontal(self) -> Metrics {
        let side = 6.0 * MM * self.zoom;
        Metrics {
            mode: LayoutMode::Horizontal,
            page_w: ENDLESS,
            page_h: ENDLESS,
            margin_left: side,
            margin_right: side,
            margin_top: 6.0 * MM * self.zoom,
            margin_bottom: 6.0 * MM * self.zoom,
            ..self
        }
    }

    /// A track below the top one of a multitrack system.
    pub fn below(self) -> Metrics {
        Metrics { top: false, ..self }
    }

    /// The layout mode's page shape for `mode`, `width` being the window's for the screen layout.
    pub fn with_mode(self, mode: LayoutMode, width: f32) -> Metrics {
        match mode {
            LayoutMode::Page => self,
            LayoutMode::Screen => self.with_screen(width),
            LayoutMode::Horizontal => self.with_horizontal(),
        }
    }

    /// The paper, orientation and margins of `page`, at this zoom.
    pub fn with_page(self, page: &PageSetup) -> Metrics {
        let (w, h) = page.size_mm();
        let px = |mm: f32| mm * MM * self.zoom;
        Metrics {
            page_w: px(w),
            page_h: px(h),
            margin_left: px(f32::from(page.margin_left)),
            margin_right: px(f32::from(page.margin_right)),
            margin_top: px(f32::from(page.margin_top)),
            margin_bottom: px(f32::from(page.margin_bottom)),
            ..self
        }
    }

    /// Where the centre of the printable width lies, from the page's left edge.
    pub fn center_x(&self) -> f32 {
        self.margin_left + (self.page_w - self.margin_left - self.margin_right) / 2.0
    }

    /// Draws the voice being edited even while it holds only rests.
    pub fn with_voice(self, voice: usize) -> Metrics {
        Metrics { voice, ..self }
    }

    /// Room for chord names, diagrams and beat text above every system when `track` has any.
    pub fn with_annotations(self, track: &Track) -> Metrics {
        let beats = || track.measures.iter().flat_map(|m| &m.voices).flat_map(|v| &v.beats);
        let annotations = beats().any(|b| b.chord.is_some() || b.text.is_some());
        let diagrams = !track.is_drums() && beats().any(|b| b.chord.as_ref().is_some_and(|c| !c.frets.is_empty()));
        let lyric_lines = track.lyrics.iter().rposition(|l| !l.text.trim().is_empty()).map_or(0, |i| i + 1);
        let listed = chord_list(track);
        let chord_strings = listed.iter().map(|c| c.frets.len()).max().unwrap_or(6);
        Metrics { annotations, diagrams, lyric_lines, chord_list: listed.len(), chord_strings, ..self }
    }

    /// Takes the chord list (and its strings) of another part: the header shows the cursor's track.
    pub fn with_chord_list_of(self, other: &Metrics) -> Metrics {
        Metrics { chord_list: other.chord_list, chord_strings: other.chord_strings, ..self }
    }

    /// Width of one diagram's cell in the chord list, and the diagrams one row holds.
    pub fn chord_cell(&self) -> (f32, usize) {
        let w = diagram_width(self.chord_strings, self.space) + 3.4 * self.space;
        let width = self.page_w - self.margin_left - self.margin_right;
        (w, ((width / w).floor() as usize).max(1))
    }

    /// Height of one row of the chord list: the name, the open and muted marks and a grid of up
    /// to seven frets, with a gap under it.
    pub fn chord_row_h(&self) -> f32 {
        12.0 * self.space
    }

    /// Height of the chord list under the title, zero without one.
    pub fn chord_list_h(&self) -> f32 {
        if self.chord_list == 0 {
            return 0.0;
        }
        let per_row = self.chord_cell().1;
        self.chord_list.div_ceil(per_row) as f32 * self.chord_row_h() + 1.0 * self.space
    }

    /// Height of one line of lyrics.
    pub fn lyric_line_h(&self) -> f32 {
        2.4 * self.space
    }

    /// Where the lyrics lane starts, from the system top: under the tab (or the drum staff), clear
    /// of what is drawn below it.
    pub fn lyrics_top(&self) -> f32 {
        let below = if self.drums {
            4.0
        } else if self.notation {
            2.6
        } else {
            6.6
        } * self.space;
        self.tab_top() + self.tab_h() + below
    }

    /// Where chord names sit (their baseline) above the top staff line.
    pub fn chord_name_above(&self) -> f32 {
        if self.diagrams { 14.0 * self.space } else { 6.4 * self.space }
    }

    /// Metrics for a drum track: a five-line percussion staff only.
    pub fn for_drums(zoom: f32) -> Metrics {
        Metrics { drums: true, ..Metrics::new(zoom, true, 1) }
    }

    /// Room left of every system for the short track name.
    pub fn indent(&self) -> f32 {
        6.5 * self.space
    }

    /// Width available to a system (clef prefix and measures).
    pub fn content_w(&self) -> f32 {
        self.page_w - self.margin_left - self.margin_right - self.indent()
    }

    /// Space above the first staff: section markers, chord names and text, directions, tempo,
    /// bar numbers.
    pub fn lane_above(&self) -> f32 {
        let spaces = match (self.top, self.diagrams, self.annotations) {
            (true, true, _) => 18.5,
            (true, false, true) => 11.0,
            (true, false, false) => 9.0,
            (false, true, _) => 15.0,
            (false, false, true) => 7.5,
            (false, false, false) => 4.0,
        };
        spaces * self.space
    }

    pub fn staff_h(&self) -> f32 {
        4.0 * self.space
    }

    /// Between the notation staff and the tab: effect lanes (P.M., let ring, vibrato, bends).
    pub fn staff_gap(&self) -> f32 {
        11.0 * self.space
    }

    pub fn tab_h(&self) -> f32 {
        if self.drums { 0.0 } else { self.tab_gap * self.strings.saturating_sub(1) as f32 }
    }

    /// Offset of the notation staff's top line from the system top.
    pub fn notation_top(&self) -> f32 {
        self.lane_above()
    }

    /// Offset of the tab staff's top line from the system top.
    pub fn tab_top(&self) -> f32 {
        if self.drums {
            self.lane_above() + self.staff_h()
        } else if self.notation {
            self.lane_above() + self.staff_h() + self.staff_gap()
        } else {
            self.lane_above() + 2.0 * self.space
        }
    }

    pub fn system_h(&self) -> f32 {
        let below = if self.drums {
            5.0
        } else if self.notation {
            3.5
        } else {
            7.0
        } * self.space;
        let lyrics = if self.lyric_lines > 0 { self.lyric_lines as f32 * self.lyric_line_h() + 0.6 * self.space } else { 0.0 };
        self.tab_top() + self.tab_h() + below + lyrics + 4.0 * self.space
    }

    pub fn header_h(&self, score: &Score) -> f32 {
        if self.mode == LayoutMode::Horizontal {
            return 0.0;
        }
        let lines: f32 = title_lines(&score.meta).iter().map(TitleLine::height).sum();
        (14.0 + lines) * self.space + self.chord_list_h()
    }

    /// Where the chord list starts below the top margin.
    pub fn chord_list_top(&self, score: &Score) -> f32 {
        self.header_h(score) - self.chord_list_h()
    }
}

/// A line of the title block under the title, in Guitar Pro's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TitleLine {
    Subtitle(String),
    Artist(String),
    Album(String),
    /// Words and music by the same people.
    WordsAndMusic(String),
    /// Words at the left, music at the right; either may be empty.
    Credits {
        words: String,
        music: String,
    },
}

impl TitleLine {
    /// The line's height in staff spaces.
    pub fn height(&self) -> f32 {
        match self {
            TitleLine::Subtitle(_) => 5.0,
            TitleLine::Artist(_) | TitleLine::Album(_) => 4.0,
            TitleLine::WordsAndMusic(_) | TitleLine::Credits { .. } => 3.5,
        }
    }
}

/// The title block's lines under the title, skipping empty fields.
pub fn title_lines(meta: &guitario_score::Meta) -> Vec<TitleLine> {
    let field = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
    let mut out = Vec::new();
    out.extend(field(&meta.subtitle).map(TitleLine::Subtitle));
    out.extend(field(&meta.artist).map(TitleLine::Artist));
    out.extend(field(&meta.album).map(TitleLine::Album));
    match (field(&meta.words), field(&meta.music)) {
        (Some(w), Some(m)) if w == m => out.push(TitleLine::WordsAndMusic(w)),
        (None, None) => {}
        (w, m) => out.push(TitleLine::Credits { words: w.unwrap_or_default(), music: m.unwrap_or_default() }),
    }
    out
}

/// Where one voice's beats sit in a bar, relative to the bar's left edge.
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceCols {
    /// The track the voice belongs to.
    pub track: usize,
    /// The voice's index in the bar (0 is Voice 1).
    pub voice: usize,
    /// Where each beat's notehead or fret goes.
    pub beats: Vec<f32>,
    /// The stretch of the bar each beat covers, `(left, right)`: from its own column to the
    /// column of the voice's next beat.
    pub spans: Vec<(f32, f32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeasureBox {
    pub index: usize,
    /// Left edge, relative to the system's staff start (after the clef/key prefix).
    pub x: f32,
    pub width: f32,
    /// The voices drawn in this bar, Voice 1 first. Beats that start together share a column.
    pub voices: Vec<VoiceCols>,
    pub show_time: bool,
    /// Room at the bar's start for a key change inside the system (naturals and the new key),
    /// before any time signature; zero when the key does not change here or the bar opens a system.
    pub key_w: f32,
}

impl MeasureBox {
    /// The columns of voice `voice`, or of Voice 1 when that voice is not drawn here.
    pub fn cols(&self, voice: usize) -> Option<&VoiceCols> {
        self.voices.iter().find(|v| v.voice == voice).or_else(|| self.voices.first())
    }

    /// This bar with only the voices of `track`, for drawing that track's staves.
    pub fn of_track(&self, track: usize) -> MeasureBox {
        MeasureBox { voices: self.voices.iter().filter(|v| v.track == track).cloned().collect(), ..self.clone() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SystemBox {
    pub measures: Vec<MeasureBox>,
    /// Top of the system relative to the page's top edge.
    pub y: f32,
    /// Width of the clef + key signature prefix.
    pub prefix: f32,
    /// Room after the last bar line for courtesy signatures: the key or time signature the next
    /// system opens with, when it changes there. Zero without.
    pub courtesy: f32,
}

impl SystemBox {
    /// This system with only the voices of `track`.
    pub fn of_track(&self, track: usize) -> SystemBox {
        SystemBox { measures: self.measures.iter().map(|m| m.of_track(track)).collect(), ..self.clone() }
    }

    /// How wide the system is, prefix and courtesy signatures included.
    pub fn width(&self) -> f32 {
        self.prefix + self.measures.iter().map(|m| m.width).sum::<f32>() + self.courtesy
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct PageBox {
    pub systems: Vec<SystemBox>,
}

/// Indexes of the voices drawn in a bar: Voice 1 always, the others when they hold a note or
/// are the voice being edited.
pub fn drawn_voices(measure: &Measure, active: usize) -> Vec<usize> {
    measure
        .voices
        .iter()
        .enumerate()
        .filter(|(i, v)| !v.beats.is_empty() && (*i == 0 || *i == active || v.beats.iter().any(|b| !b.rest)))
        .map(|(i, _)| i)
        .collect()
}

/// Most diagrams the chord list at the top of the score shows.
pub const MAX_CHORD_LIST: usize = 48;

/// The chord list Guitar Pro draws under the title: each distinct chord diagram of `track` once, in
/// the order the chords first appear. A drum track has none.
pub fn chord_list(track: &Track) -> Vec<&Chord> {
    let mut out: Vec<&Chord> = Vec::new();
    if track.is_drums() {
        return out;
    }
    let chords = track.measures.iter().flat_map(|m| &m.voices).flat_map(|v| &v.beats).filter_map(|b| b.chord.as_ref());
    for c in chords.filter(|c| !c.frets.is_empty()) {
        if !out.contains(&c) {
            out.push(c);
            if out.len() == MAX_CHORD_LIST {
                break;
            }
        }
    }
    out
}

/// Width of a chord diagram's grid for `strings` strings.
pub fn diagram_width(strings: usize, space: f32) -> f32 {
    strings.saturating_sub(1) as f32 * 0.8 * space
}

/// Width of a column whose next onset is `gap` ticks away, plus room for wide frets and chords.
fn column_natural(gap: u32, beats: &[&Beat], space: f32) -> f32 {
    // Width grows with the log of the length, from 2.7 spaces for a sixteenth.
    let sixteenth = (guitario_theory::TICKS_PER_QUARTER / 4) as f32;
    let mut w = space * (2.7 + 1.15 * (gap.max(1) as f32 / sixteenth).log2()).max(1.9);
    if beats.iter().any(|b| b.notes.iter().any(|n| n.fret >= 10)) {
        w += 0.5 * space;
    }
    if beats.iter().any(|b| b.notes.len() > 1) {
        w += 0.3 * space;
    }
    w
}

struct VoiceNat {
    track: usize,
    voice: usize,
    /// The column each beat starts in.
    col_of: Vec<usize>,
}

/// Room left of a beat for its grace notes, in staff spaces.
pub const GRACE_ROOM: f32 = 3.0;

struct Natural {
    fixed: f32,
    /// Column widths: one per moment a beat starts in any voice.
    cols: Vec<f32>,
    /// Room kept at the left of each column for grace notes, inside its width.
    pads: Vec<f32>,
    voices: Vec<VoiceNat>,
    show_time: bool,
    lead: f32,
    /// Room for a key change, inside `lead` and `fixed`; given back when the bar opens a system.
    key_w: f32,
}

/// The longest syllable sung on each beat of the first voice, in characters, by (measure, beat).
pub fn sung_lengths(track: &Track) -> HashMap<(usize, usize), usize> {
    let mut out = HashMap::new();
    for line in &track.lyrics {
        for (mi, bi, syl) in guitario_score::dispatch_lyrics(track, line) {
            let n = syl.text.chars().count() + usize::from(syl.hyphen);
            let slot = out.entry((mi, bi)).or_insert(0);
            *slot = (*slot).max(n);
        }
    }
    out
}

/// One track of a layout: its index, the track and its metrics.
pub struct Part<'a> {
    pub index: usize,
    pub track: &'a Track,
    pub metrics: Metrics,
}

fn natural(score: &Score, parts: &[Part], mi: usize, s: f32, sung: &[HashMap<(usize, usize), usize>]) -> Natural {
    let header = score.headers.get(mi);
    let prev = mi.checked_sub(1).and_then(|i| score.headers.get(i));
    let show_time = match (header, prev) {
        (Some(h), Some(p)) => h.time_signature != p.time_signature,
        (Some(_), None) => true,
        _ => false,
    };
    // Every beat with the tick it starts at, per drawn voice of every track: beats that start
    // together in any track share a column, so the tracks of a multitrack system line up in time.
    struct Timed<'b> {
        part: usize,
        voice: usize,
        beats: Vec<(u64, &'b Beat)>,
    }
    let mut timed: Vec<Timed> = Vec::new();
    for (pi, part) in parts.iter().enumerate() {
        let Some(measure) = part.track.measures.get(mi) else { continue };
        for vi in drawn_voices(measure, part.metrics.voice) {
            let Some(voice) = measure.voices.get(vi) else { continue };
            let mut at = 0u64;
            let beats = voice
                .beats
                .iter()
                .map(|b| {
                    let start = at;
                    at += u64::from(b.duration.ticks());
                    (start, b)
                })
                .collect();
            timed.push(Timed { part: pi, voice: vi, beats });
        }
    }
    let mut onsets: Vec<u64> = timed.iter().flat_map(|t| t.beats.iter().map(|(t, _)| *t)).collect();
    onsets.sort_unstable();
    onsets.dedup();
    let mut pads = Vec::with_capacity(onsets.len());
    let cols: Vec<f32> = onsets
        .iter()
        .enumerate()
        .map(|(ci, &t)| {
            // The beats starting here, with the part each belongs to.
            let here: Vec<(usize, &Beat)> =
                timed.iter().flat_map(|tv| tv.beats.iter().filter(move |(st, _)| *st == t).map(move |(_, b)| (tv.part, *b))).collect();
            let beats: Vec<&Beat> = here.iter().map(|(_, b)| *b).collect();
            let pad = if beats.iter().any(|b| !b.rest && b.notes.iter().any(|n| n.effects.grace.is_some())) { GRACE_ROOM * s } else { 0.0 };
            pads.push(pad);
            let gap = match onsets.get(ci + 1) {
                Some(&next) => u32::try_from(next - t).unwrap_or(u32::MAX),
                None => beats.iter().map(|b| b.duration.ticks()).min().unwrap_or(1),
            };
            let mut w = column_natural(gap, &beats, s);
            // A chord diagram is wider than a short note's column.
            let diagram = here
                .iter()
                .filter(|(p, _)| parts.get(*p).is_some_and(|p| !p.metrics.drums))
                .filter_map(|(_, b)| b.chord.as_ref())
                .map(|c| c.frets.len())
                .filter(|n| *n > 0)
                .max();
            if let Some(strings) = diagram {
                w = w.max(diagram_width(strings, s) + 2.6 * s);
            }
            // A long syllable needs a wider column than its note.
            for (pi, lengths) in sung.iter().enumerate() {
                let voice1 = timed.iter().find(|tv| tv.part == pi && tv.voice == 0).and_then(|tv| tv.beats.iter().position(|(st, _)| *st == t));
                if let Some(chars) = voice1.and_then(|bi| lengths.get(&(mi, bi))) {
                    w = w.max(*chars as f32 * 0.9 * s + 1.0 * s);
                }
            }
            if onsets.len() == 1 && beats.iter().all(|b| b.rest) {
                w = w.max(8.0 * s);
            }
            w + pad
        })
        .collect();
    let voices = timed
        .iter()
        .map(|tv| VoiceNat {
            track: parts.get(tv.part).map_or(0, |p| p.index),
            voice: tv.voice,
            col_of: tv.beats.iter().filter_map(|(t, _)| onsets.binary_search(t).ok()).collect(),
        })
        .collect();
    // A key change, where any part shows standard notation.
    let keyed = parts.iter().any(|p| p.metrics.notation && !p.metrics.drums);
    let key_w = match (header, prev) {
        (Some(h), Some(p)) if keyed && h.key != p.key => {
            let glyphs = crate::notation::key_cancel(p.key, h.key).len() + usize::from(h.key.unsigned_abs().min(7));
            (glyphs as f32 + 0.8) * s
        }
        _ => 0.0,
    };
    let mut lead = 1.8 * s + key_w;
    if show_time {
        lead += 3.0 * s;
    }
    if header.is_some_and(|h| h.repeat_open) {
        lead += 1.4 * s;
    }
    let mut fixed = lead + 1.2 * s;
    if header.is_some_and(|h| h.repeat_close > 0) {
        fixed += 1.4 * s;
    }
    Natural { fixed, cols, pads, voices, show_time, lead, key_w }
}

/// What a system ending at bar `last` warns of: the key change (`(old, new)`) and the time
/// signature the next bar brings, when it changes there.
pub fn courtesy_of(score: &Score, last: usize) -> (Option<(i8, i8)>, Option<guitario_theory::TimeSignature>) {
    let (Some(h), Some(next)) = (score.headers.get(last), last.checked_add(1).and_then(|n| score.headers.get(n))) else { return (None, None) };
    let key = (next.key != h.key).then_some((h.key, next.key));
    let time = (next.time_signature != h.time_signature && !next.free_time).then_some(next.time_signature);
    (key, time)
}

/// Room for the courtesy signatures after a system ending at bar `last`. Key signatures show only
/// where some part has standard notation.
fn courtesy_w(score: &Score, parts: &[Part], last: usize, s: f32) -> f32 {
    let (key, time) = courtesy_of(score, last);
    let keyed = parts.iter().any(|p| p.metrics.notation && !p.metrics.drums);
    let key_w = match key {
        Some((old, new)) if keyed => (crate::notation::key_cancel(old, new).len() + usize::from(new.unsigned_abs().min(7))) as f32 * s,
        _ => 0.0,
    };
    // A digit is about 1.3 spaces wide; 13/16 needs room for two.
    let digits = |n: u8| if n >= 10 { 2.0 } else { 1.0 };
    let time_w = time.map_or(0.0, |ts| (digits(ts.numerator.max(ts.denominator)) * 1.3 + 0.8) * s);
    if key_w + time_w > 0.0 { key_w + time_w + 1.2 * s } else { 0.0 }
}

/// The clef + key signature prefix at the start of every system.
pub fn system_prefix(score: &Score, first_measure: usize, m: &Metrics) -> f32 {
    let key = score.headers.get(first_measure).map_or(0, |h| h.key.unsigned_abs().min(7));
    if m.drums {
        4.4 * m.space
    } else if m.notation {
        (4.2 + f32::from(key) * 1.0 + 0.6) * m.space
    } else {
        4.6 * m.space
    }
}

/// One track on its own (the tests' shorthand for [`layout_parts`]).
#[cfg(test)]
pub fn layout(score: &Score, track_index: usize, m: &Metrics) -> Vec<PageBox> {
    let Some(track) = score.tracks.get(track_index) else { return vec![] };
    layout_parts(score, &[Part { index: track_index, track, metrics: *m }], m)
}

/// Lays out `parts` (one track, or every track of the multitrack view) as systems of stacked
/// staves on pages shaped by `page`: its paper, margins, zoom and layout mode.
pub fn layout_parts(score: &Score, parts: &[Part], page: &Metrics) -> Vec<PageBox> {
    if parts.is_empty() {
        return vec![];
    }
    let s = page.space;
    let sung: Vec<HashMap<(usize, usize), usize>> = parts.iter().map(|p| sung_lengths(p.track)).collect();
    let naturals: Vec<Natural> = (0..score.headers.len()).map(|mi| natural(score, parts, mi, s, &sung)).collect();
    let total = |n: &Natural| n.fixed + n.cols.iter().sum::<f32>();
    let prefix_at = |mi: usize| parts.iter().map(|p| system_prefix(score, mi, &p.metrics)).fold(0.0f32, f32::max);
    let system_h: f32 = parts.iter().map(|p| p.metrics.system_h()).sum();
    let horizontal = page.mode == LayoutMode::Horizontal;

    // Break into systems; the horizontal layout keeps every bar on one line.
    let mut systems: Vec<(Vec<usize>, f32)> = Vec::new();
    let mut i = 0;
    while i < naturals.len() {
        let prefix = prefix_at(i);
        let avail = if horizontal { f32::INFINITY } else { page.content_w() - prefix };
        let mut used = 0.0;
        let mut row = Vec::new();
        while let Some(n) = naturals.get(i) {
            let w = total(n);
            if !row.is_empty() && used + w > avail {
                break;
            }
            used += w;
            row.push(i);
            i += 1;
        }
        // Courtesy signatures for the next system must fit too: give up bars until they do.
        if !horizontal {
            while row.len() > 1 && row.last().is_some_and(|&l| used + courtesy_w(score, parts, l, s) > avail) {
                if let Some(l) = row.pop() {
                    used -= naturals.get(l).map_or(0.0, total);
                    i = l;
                }
            }
        }
        systems.push((row, prefix));
    }

    // Justify and paginate.
    let mut pages = vec![PageBox::default()];
    let mut y = page.margin_top + page.header_h(score);
    let count = systems.len();
    for (si, (row, prefix)) in systems.into_iter().enumerate() {
        let courtesy = if horizontal || si + 1 == count { 0.0 } else { row.last().map_or(0.0, |&l| courtesy_w(score, parts, l, s)) };
        let avail = page.content_w() - prefix - courtesy;
        let used: f32 = row.iter().filter_map(|i| naturals.get(*i)).map(total).sum();
        let beats_sum: f32 = row.iter().filter_map(|i| naturals.get(*i)).map(|n| n.cols.iter().sum::<f32>()).sum();
        let last_and_short = si + 1 == count && used < avail * 0.7;
        let extra = if horizontal || last_and_short || beats_sum <= 0.0 { 0.0 } else { (avail - used).max(0.0) };
        let stretch = if beats_sum > 0.0 { 1.0 + extra / beats_sum } else { 1.0 };
        let mut x = 0.0;
        let mut measures = Vec::new();
        for (k, mi) in row.into_iter().enumerate() {
            let Some(n) = naturals.get(mi) else { continue };
            // The system's own key signature stands for a change in its first bar.
            let key_w = if k == 0 { 0.0 } else { n.key_w };
            let given_back = n.key_w - key_w;
            let col_w: Vec<f32> = n.cols.iter().map(|w| w * stretch).collect();
            // Left edge of each column, with one more entry for the right edge of the last.
            let mut edges = Vec::with_capacity(col_w.len() + 1);
            let mut bx = n.lead - given_back;
            edges.push(bx);
            for w in &col_w {
                bx += w;
                edges.push(bx);
            }
            let edge = |c: usize| edges.get(c).copied().unwrap_or(bx);
            let pad = |c: usize| n.pads.get(c).copied().unwrap_or(0.0);
            let voices = n
                .voices
                .iter()
                .map(|v| {
                    // A beat sits in the middle of its column, right of any room kept for grace notes.
                    let beats = v.col_of.iter().map(|&c| (edge(c) + pad(c) + edge(c + 1)) / 2.0).collect();
                    let spans = v.col_of.iter().enumerate().map(|(i, &c)| (edge(c), edge(v.col_of.get(i + 1).copied().unwrap_or(col_w.len())))).collect();
                    VoiceCols { track: v.track, voice: v.voice, beats, spans }
                })
                .collect();
            let width = n.fixed - given_back + col_w.iter().sum::<f32>();
            measures.push(MeasureBox { index: mi, x, width, voices, show_time: n.show_time, key_w });
            x += width;
        }
        if y + system_h > page.page_h - page.margin_bottom && pages.last().is_some_and(|p| !p.systems.is_empty()) {
            pages.push(PageBox::default());
            y = page.margin_top;
        }
        if let Some(pg) = pages.last_mut() {
            pg.systems.push(SystemBox { measures, y, prefix, courtesy });
        }
        y += system_h;
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::MeasureHeader;

    fn score(n: usize) -> Score {
        let mut s = Score::new();
        for _ in 1..n {
            s.headers.push(MeasureHeader::new(Default::default()));
            let h = s.headers[0].clone();
            s.tracks[0].measures.push(guitario_score::Measure::empty_for(&h));
        }
        s
    }

    #[test]
    fn every_measure_is_placed_once_in_order() {
        let s = score(60);
        let m = Metrics::new(1.0, true, 6);
        let pages = layout(&s, 0, &m);
        let order: Vec<usize> = pages.iter().flat_map(|p| &p.systems).flat_map(|s| &s.measures).map(|m| m.index).collect();
        assert_eq!(order, (0..60).collect::<Vec<_>>());
        assert!(pages.len() > 1, "60 empty bars should need more than one page");
    }

    #[test]
    fn a_beat_with_a_grace_note_keeps_room_on_its_left() {
        use guitario_score::{Beat, Grace, Note};
        use guitario_theory::Duration;
        let mut s = score(1);
        let plain = |fret: u8| Beat { rest: false, notes: vec![Note::new(0, fret)], ..Beat::rest(Duration::QUARTER) };
        s.tracks[0].measures[0].voices[0].beats = vec![plain(1), plain(2), plain(3), plain(4)];
        let m = Metrics::new(1.0, true, 6);
        let room = |s: &Score| {
            let pages = layout(s, 0, &m);
            let v = &pages[0].systems[0].measures[0].voices[0];
            // From where each beat's stretch starts to its column.
            v.beats.iter().zip(&v.spans).map(|(x, (l, _))| x - l).collect::<Vec<f32>>()
        };
        let before = room(&s);
        s.tracks[0].measures[0].voices[0].beats[2].notes[0].effects.grace = Some(Grace::new(5));
        let after = room(&s);
        assert!(after[2] >= before[2] + GRACE_ROOM * m.space * 0.99, "{before:?} {after:?}");
        assert!((after[1] - before[1]).abs() < 0.01, "other beats are not moved off-centre");
    }

    #[test]
    fn a_key_change_takes_room_inside_a_system_but_not_at_its_start() {
        let mut s = score(40);
        let m = Metrics::new(1.0, true, 6);
        let boxes = |s: &Score, m: &Metrics| -> Vec<MeasureBox> { layout(s, 0, m).into_iter().flat_map(|p| p.systems).flat_map(|sys| sys.measures).collect() };
        let plain = boxes(&s, &m);
        // From bar 3 (index 2) in A major; the second system opens on another change, to C.
        for h in s.headers.iter_mut().skip(2) {
            h.key = 3;
        }
        let opener = layout(&s, 0, &m).into_iter().flat_map(|p| p.systems).nth(1).and_then(|sys| sys.measures.first().map(|b| b.index)).unwrap();
        for h in s.headers.iter_mut().skip(opener) {
            h.key = 0;
        }
        let keyed = boxes(&s, &m);
        let at = |i: usize| keyed.iter().find(|b| b.index == i).unwrap();
        assert!((at(2).key_w - (3.0 + 0.8) * m.space).abs() < 0.01, "three sharps: {}", at(2).key_w);
        assert!(at(2).voices[0].beats[0] > plain[2].voices[0].beats[0], "the notes move right of the key");
        // A bar opening a system never keeps room: the system's own key signature shows it.
        let openers: Vec<MeasureBox> = layout(&s, 0, &m).into_iter().flat_map(|p| p.systems).filter_map(|sys| sys.measures.into_iter().next()).collect();
        assert!(openers.iter().all(|b| b.key_w == 0.0));
        // The change back to C either opens a system or, moved by a courtesy, keeps room inside one.
        let opens = openers.iter().any(|b| b.index == opener);
        assert_eq!(at(opener).key_w > 0.0, !opens);
        assert_eq!(at(1).key_w, 0.0);
        // Tab alone shows no key signatures, so it keeps no room.
        let tab = Metrics::new(1.0, false, 6);
        assert!(boxes(&s, &tab).iter().all(|b| b.key_w == 0.0));
    }

    #[test]
    fn courtesy_signatures_take_room_after_the_system_before_a_change() {
        let mut s = score(40);
        let m = Metrics::new(1.0, true, 6);
        let systems = |s: &Score| -> Vec<SystemBox> { layout(s, 0, &m).into_iter().flat_map(|p| p.systems).collect() };
        let plain = systems(&s);
        assert!(plain.iter().all(|sys| sys.courtesy == 0.0));
        let opener = plain.get(1).and_then(|sys| sys.measures.first()).unwrap().index;
        for h in s.headers.iter_mut().skip(opener) {
            h.time_signature = guitario_theory::TimeSignature::new(3, 4).unwrap();
        }
        let changed = systems(&s);
        let first = changed.first().unwrap();
        assert!(first.courtesy > 0.0, "the first system warns of 3/4");
        // The bars and the courtesy still fit the line, and no other system warns.
        assert!(first.width() <= m.content_w() + 0.01, "{} > {}", first.width(), m.content_w());
        let next = first.measures.last().unwrap().index + 1;
        assert!(changed.iter().skip(1).all(|sys| sys.courtesy == 0.0 || sys.measures.last().is_some_and(|b| b.index + 1 == next)));
        // The horizontal layout is one line: nothing to warn of.
        let h = Metrics::new(1.0, true, 6).with_horizontal();
        assert!(layout(&s, 0, &h).iter().flat_map(|p| &p.systems).all(|sys| sys.courtesy == 0.0));
    }

    #[test]
    fn the_chord_list_takes_room_under_the_title_and_pushes_the_systems_down() {
        use guitario_score::{Beat, Chord, Note};
        use guitario_theory::Duration;
        let mut s = score(4);
        let base = Metrics::new(1.0, true, 6).with_annotations(&s.tracks[0]);
        let first_y = |s: &Score, m: &Metrics| layout(s, 0, m).first().and_then(|p| p.systems.first()).map(|sys| sys.y);
        let before = first_y(&s, &base);
        assert_eq!(base.chord_list_h(), 0.0);
        // Sixty distinct diagrams: the list stops at MAX_CHORD_LIST and wraps into rows.
        let chord = |i: u8| Chord { name: format!("C{i}"), frets: vec![Some(i % 20), None, None, None, None, None], first_fret: 1 };
        s.tracks[0].measures[0].voices[0].beats =
            (0..60).map(|i| Beat { rest: false, notes: vec![Note::new(0, 0)], chord: Some(chord(i)), ..Beat::rest(Duration::QUARTER) }).collect();
        let listed = chord_list(&s.tracks[0]);
        assert_eq!(listed.len(), MAX_CHORD_LIST);
        let m = Metrics::new(1.0, true, 6).with_annotations(&s.tracks[0]);
        assert_eq!(m.chord_list, MAX_CHORD_LIST);
        let rows = MAX_CHORD_LIST.div_ceil(m.chord_cell().1);
        assert!(rows >= 2, "{rows} rows");
        assert!((m.chord_list_h() - (rows as f32 * m.chord_row_h() + m.space)).abs() < 0.01);
        assert!(m.chord_list_top(&s) >= 14.0 * m.space - 0.01, "under the title");
        // Diagrams also widen the lane above each system, so the first system starts lower still.
        let after = first_y(&s, &m);
        assert!(after.zip(before).is_some_and(|(a, b)| a >= b + m.chord_list_h()), "{before:?} -> {after:?}");
        // The horizontal layout has no header at all.
        assert_eq!(m.with_horizontal().header_h(&s), 0.0);
        // A drum track never lists chords.
        assert!(chord_list(&guitario_score::Track::drums("d", &s.headers)).is_empty());
    }

    #[test]
    fn chord_diagrams_make_the_lane_taller_and_their_column_wide_enough() {
        use guitario_score::{Beat, Chord, Note};
        use guitario_theory::Duration;
        let mut s = score(1);
        let sixteenth = Duration::new(guitario_theory::NoteValue::Sixteenth);
        s.tracks[0].measures[0].voices[0].beats = (0..16).map(|i| Beat { rest: false, notes: vec![Note::new(0, i)], ..Beat::rest(sixteenth) }).collect();
        let named = |s: &Score| Metrics::new(1.0, true, 6).with_annotations(&s.tracks[0]);
        let plain = named(&s);
        s.tracks[0].measures[0].voices[0].beats[0].chord = Some(Chord::named("E5"));
        let names = named(&s);
        assert!(names.annotations && !names.diagrams && names.lane_above() > plain.lane_above());
        s.tracks[0].measures[0].voices[0].beats[0].chord =
            Some(Chord { name: "E5".into(), frets: vec![None, None, None, Some(2), Some(2), Some(0)], first_fret: 1 });
        let m = named(&s);
        assert!(m.diagrams && m.lane_above() > names.lane_above() && m.chord_name_above() > names.chord_name_above());
        let pages = layout(&s, 0, &m);
        let v = &pages[0].systems[0].measures[0].voices[0];
        let (l, r) = v.spans[0];
        assert!(r - l >= diagram_width(6, m.space) + 2.0 * m.space, "a sixteenth's column holds the diagram: {}", r - l);
        // A drum track never draws diagrams.
        let drums = guitario_score::Track::drums("d", &s.headers);
        assert!(!Metrics::for_drums(1.0).with_annotations(&drums).diagrams);
    }

    #[test]
    fn lyrics_add_a_line_under_every_system_and_widen_long_syllables() {
        use guitario_score::{Beat, LyricLine, Note};
        use guitario_theory::Duration;
        let mut s = score(1);
        let sixteenth = Duration::new(guitario_theory::NoteValue::Sixteenth);
        s.tracks[0].measures[0].voices[0].beats = (0..16).map(|i| Beat { rest: false, notes: vec![Note::new(0, i)], ..Beat::rest(sixteenth) }).collect();
        let base = Metrics::new(1.0, true, 6);
        let plain = base.with_annotations(&s.tracks[0]);
        s.tracks[0].lyrics = vec![LyricLine { measure: 0, text: "a Extraordinarily".into() }, LyricLine::default(), LyricLine { measure: 0, text: "b".into() }];
        let m = base.with_annotations(&s.tracks[0]);
        assert_eq!(m.lyric_lines, 3, "an empty line between others still takes its row");
        assert!(m.system_h() >= plain.system_h() + 3.0 * m.lyric_line_h());
        assert!(m.lyrics_top() > m.tab_top() + m.tab_h());
        let pages = layout(&s, 0, &m);
        let spans = &pages[0].systems[0].measures[0].voices[0].spans;
        let w = |i: usize| spans[i].1 - spans[i].0;
        assert!(w(1) > 2.0 * w(2), "the long word's column is wider: {} vs {}", w(1), w(2));
    }

    #[test]
    fn the_screen_layout_wraps_to_the_window_and_never_breaks_into_pages() {
        let s = score(40);
        let base = Metrics::new(1.0, true, 6);
        let narrow = base.with_screen(500.0);
        let wide = base.with_screen(1600.0);
        let (pn, pw) = (layout(&s, 0, &narrow), layout(&s, 0, &wide));
        assert_eq!((pn.len(), pw.len()), (1, 1), "one sheet");
        let systems = |p: &[PageBox]| p.iter().map(|p| p.systems.len()).sum::<usize>();
        assert!(systems(&pn) > systems(&pw), "a narrow window needs more systems");
        for sys in &pw[0].systems {
            assert!(sys.width() <= wide.content_w() + 0.5);
        }
        // The page view of the same score has several pages.
        assert!(layout(&s, 0, &base).len() > 1);
    }

    #[test]
    fn the_horizontal_layout_puts_every_bar_on_one_unstretched_line() {
        let s = score(40);
        let m = Metrics::new(1.0, true, 6).with_horizontal();
        let pages = layout(&s, 0, &m);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].systems.len(), 1);
        let sys = &pages[0].systems[0];
        assert_eq!(sys.measures.iter().map(|m| m.index).collect::<Vec<_>>(), (0..40).collect::<Vec<_>>());
        // Natural widths: every empty bar is as wide as the others after the first.
        let w: Vec<f32> = sys.measures.iter().skip(1).map(|m| m.width).collect();
        assert!(w.windows(2).all(|p| (p[0] - p[1]).abs() < 0.01), "{w:?}");
        assert_eq!(m.header_h(&s), 0.0, "no title in the horizontal layout");
    }

    #[test]
    fn a_multitrack_layout_lines_up_beats_that_sound_together() {
        use guitario_score::{Beat, Note};
        use guitario_theory::{Duration, NoteValue};
        let mut s = score(1);
        let note = |d: Duration| Beat { rest: false, notes: vec![Note::new(0, 1)], ..Beat::rest(d) };
        s.tracks[0].measures[0].voices[0].beats = vec![note(Duration::new(NoteValue::Half)), note(Duration::new(NoteValue::Half))];
        let mut bass = guitario_score::Track::new("Bass", guitario_theory::Tuning::bass(), &s.headers);
        bass.measures[0].voices[0].beats = (0..4).map(|_| note(Duration::QUARTER)).collect();
        s.tracks.push(bass);
        let m = Metrics::new(1.0, true, 6);
        let parts = [Part { index: 0, track: &s.tracks[0], metrics: m }, Part { index: 1, track: &s.tracks[1], metrics: Metrics::new(1.0, true, 4).below() }];
        let pages = layout_parts(&s, &parts, &m);
        let mb = &pages[0].systems[0].measures[0];
        let guitar = mb.voices.iter().find(|v| v.track == 0).unwrap();
        let bass = mb.voices.iter().find(|v| v.track == 1).unwrap();
        assert_eq!(guitar.beats.len(), 2);
        assert_eq!(bass.beats.len(), 4);
        // The guitar's second half note starts with the bass's third quarter.
        assert!((guitar.beats[1] - bass.beats[2]).abs() < 0.01);
        assert_eq!(mb.of_track(1).voices.len(), 1);
        // A system is as tall as both tracks.
        assert!(m.lane_above() > parts[1].metrics.lane_above(), "a lower track needs less room above");
    }

    #[test]
    fn systems_fit_the_page_width() {
        let s = score(30);
        let m = Metrics::new(1.5, true, 6);
        for sys in layout(&s, 0, &m).iter().flat_map(|p| &p.systems) {
            let w: f32 = sys.measures.iter().map(|x| x.width).sum();
            assert!(w + sys.prefix <= m.content_w() + 0.5, "{w} + {} > {}", sys.prefix, m.content_w());
            assert!(sys.y + m.system_h() <= m.page_h);
        }
    }

    #[test]
    fn every_paper_lays_out_inside_its_margins() {
        use guitario_score::Paper;
        let s = score(40);
        for paper in Paper::ALL {
            for landscape in [false, true] {
                let page = PageSetup { paper, landscape, margin_left: 5, margin_right: 25, margin_top: 20, margin_bottom: 10 };
                let m = Metrics::new(1.0, true, 6).with_page(&page);
                let (w, h) = page.size_mm();
                assert!((m.page_w - w * MM).abs() < 0.01 && (m.page_h - h * MM).abs() < 0.01);
                let pages = layout(&s, 0, &m);
                let order: Vec<usize> = pages.iter().flat_map(|p| &p.systems).flat_map(|s| &s.measures).map(|x| x.index).collect();
                assert_eq!(order, (0..40).collect::<Vec<_>>(), "{paper:?} landscape={landscape}");
                for page in &pages {
                    for sys in &page.systems {
                        let used: f32 = sys.measures.iter().map(|x| x.width).sum::<f32>() + sys.prefix;
                        assert!(used <= m.content_w() + 0.5, "{paper:?}: {used} > {}", m.content_w());
                        // A system stays above the bottom margin unless it is alone on a page too short for it.
                        assert!(sys.y + m.system_h() <= m.page_h - m.margin_bottom + 0.5 || page.systems.len() == 1, "{paper:?} landscape={landscape}");
                    }
                }
            }
        }
    }

    #[test]
    fn margins_move_the_printable_width_and_its_centre() {
        let even = Metrics::new(1.0, true, 6);
        let page = PageSetup { margin_left: 30, margin_right: 10, ..PageSetup::default() };
        let m = even.with_page(&page);
        assert!((even.content_w() - m.content_w() - 20.0 * MM).abs() < 0.01);
        // The centre of the printable width sits right of the page centre when the left margin is wider.
        assert!(m.center_x() > m.page_w / 2.0);
        assert!((even.center_x() - even.page_w / 2.0).abs() < 0.01);
        // Zoom scales the paper and the margins together.
        let zoomed = Metrics::new(2.0, true, 6).with_page(&page);
        assert!((zoomed.page_w - 2.0 * m.page_w).abs() < 0.01 && (zoomed.margin_left - 2.0 * m.margin_left).abs() < 0.01);
    }

    #[test]
    fn first_measure_shows_time_signature() {
        let pages = layout(&score(2), 0, &Metrics::new(1.0, false, 6));
        let ms = &pages[0].systems[0].measures;
        assert!(ms[0].show_time);
        assert!(!ms[1].show_time);
    }

    #[test]
    fn drum_systems_are_one_staff_high() {
        let g = Metrics::new(1.0, true, 6);
        let d = Metrics::for_drums(1.0);
        assert_eq!(d.tab_h(), 0.0);
        assert!(d.system_h() < g.system_h() * 0.7, "{} vs {}", d.system_h(), g.system_h());
        let pages = layout(&score(40), 0, &d);
        assert!(pages.iter().flat_map(|p| &p.systems).all(|s| s.y + d.system_h() <= d.page_h));
    }

    fn voiced_score() -> Score {
        use guitario_score::{Note, Voice};
        use guitario_theory::{Duration, NoteValue};
        let mut s = score(1);
        let note = |d: Duration| Beat { rest: false, notes: vec![Note::new(0, 5)], ..Beat::rest(d) };
        let (q, h) = (Duration::QUARTER, Duration::new(NoteValue::Half));
        // Voice 1: two halves. Voice 2: a quarter, a quarter, then a half: onsets 0, q, 2q.
        s.tracks[0].measures[0].voices = vec![Voice { beats: vec![note(h), note(h)] }, Voice { beats: vec![note(q), note(q), note(h)] }];
        s
    }

    #[test]
    fn voices_share_a_column_when_their_beats_start_together() {
        let s = voiced_score();
        let pages = layout(&s, 0, &Metrics::new(1.0, true, 6));
        let mb = &pages[0].systems[0].measures[0];
        let (v1, v2) = (&mb.voices[0], &mb.voices[1]);
        assert_eq!((v1.voice, v2.voice), (0, 1));
        // Voice 1 beat 0 and voice 2 beat 0 start together; so do voice 1 beat 1 and voice 2 beat 2.
        assert_eq!(v1.beats[0], v2.beats[0]);
        assert_eq!(v1.beats[1], v2.beats[2]);
        // Voice 2's middle beat falls between them.
        assert!(v2.beats[0] < v2.beats[1] && v2.beats[1] < v2.beats[2]);
        // A beat's span runs to its voice's next beat: voice 1's first half covers voice 2's first two.
        assert_eq!(v1.spans[0].0, v2.spans[0].0);
        assert_eq!(v1.spans[0].1, v2.spans[1].1);
        assert_eq!(v1.spans[1].1, v2.spans[2].1, "both end at the last column");
    }

    #[test]
    fn a_voice_with_only_rests_is_drawn_only_while_it_is_being_edited() {
        use guitario_score::Voice;
        let mut s = voiced_score();
        let rest = guitario_score::Beat::rest(guitario_theory::Duration::WHOLE);
        s.tracks[0].measures[0].voices[1] = Voice { beats: vec![rest] };
        let measure = &s.tracks[0].measures[0];
        assert_eq!(drawn_voices(measure, 0), vec![0]);
        assert_eq!(drawn_voices(measure, 1), vec![0, 1]);
        // A bar that lacks the voice has nothing to draw for it.
        s.tracks[0].measures[0].voices.truncate(1);
        assert_eq!(drawn_voices(&s.tracks[0].measures[0], 1), vec![0]);
        let pages = layout(&s, 0, &Metrics::new(1.0, true, 6).with_voice(1));
        assert_eq!(pages[0].systems[0].measures[0].voices.len(), 1);
    }

    #[test]
    fn spans_tile_each_voice_and_one_voice_lays_out_as_before() {
        let s = voiced_score();
        let pages = layout(&s, 0, &Metrics::new(1.0, true, 6));
        let mb = &pages[0].systems[0].measures[0];
        for v in &mb.voices {
            for w in v.spans.windows(2) {
                assert_eq!(w[0].1, w[1].0, "no gap or overlap between a voice's beats");
            }
            for (c, (l, r)) in v.beats.iter().zip(&v.spans) {
                assert!(l <= c && c <= r);
            }
        }
        // One voice: every beat's span is its own column and the centre is in the middle of it.
        let one = score(2);
        let pages = layout(&one, 0, &Metrics::new(1.0, true, 6));
        let v = &pages[0].systems[0].measures[0].voices[0];
        assert_eq!(v.beats.len(), 1);
        assert!(((v.spans[0].0 + v.spans[0].1) / 2.0 - v.beats[0]).abs() < 0.001);
    }

    #[test]
    fn a_bar_of_eighths_leaves_room_for_more_bars() {
        // Spacing is relative to the tick resolution: a sixteenth is 2.7 spaces whatever
        // TICKS_PER_QUARTER is, and four bars of straight eighths share one A4 system.
        let m = Metrics::new(1.0, true, 6);
        let sixteenth = Beat {
            rest: false,
            notes: vec![guitario_score::Note::new(0, 0)],
            ..Beat::rest(guitario_theory::Duration::new(guitario_theory::NoteValue::Sixteenth))
        };
        assert!((column_natural(sixteenth.duration.ticks(), &[&sixteenth], m.space) - 2.7 * m.space).abs() < 0.01);
        let mut s = score(8);
        let eighth = Beat { rest: false, notes: vec![guitario_score::Note::new(0, 5)], ..Beat::rest(guitario_theory::Duration::EIGHTH) };
        for m in &mut s.tracks[0].measures {
            m.voices[0].beats = vec![eighth.clone(); 8];
        }
        let pages = layout(&s, 0, &m);
        assert!(pages[0].systems[0].measures.len() >= 3, "{} bars on the first system", pages[0].systems[0].measures.len());
    }

    #[test]
    fn missing_track_is_empty() {
        assert!(layout(&score(2), 9, &Metrics::new(1.0, true, 6)).is_empty());
    }
}
