//! The page view: white A4 pages on a grey workspace, each system a standard-notation staff over a
//! tablature staff joined by a bracket, laid out by `layout` and painted here. Clicks move the
//! cursor; the painter never edits the score directly.

use crate::fonts::{FontSet, sans, serif, serif_bold, serif_italic};
use crate::glyphs::{self as g, Pen};
use crate::layout::{LayoutMode, Metrics, PageBox, Part, SystemBox, layout_parts};
use crate::notation::{Accidentals, Clef, beam_groups, spell, staff_pos};
use crate::theme::{Tokens, tokens};
use egui::{Color32, Painter, Rect, Stroke, pos2, vec2};
use guitario_engine::Cursor;
use guitario_score::{
    Beat, BeatTechnique, BrushKind, DirectionTarget, Fade, Jump, MeasureHeader, Note, NoteKind, Score, Slide, StrokeDirection, Track, TripletFeel,
};

const PAGE_GAP: f32 = 24.0;

type BeatTest = fn(&Beat) -> bool;

/// What a click on the page asks for.
pub enum Click {
    /// `voice` is set when Multivoice Editing let the click pick a note of another voice.
    Beat { track: usize, measure: usize, beat: usize, string: Option<usize>, voice: Option<usize>, shift: bool },
}

/// An inclusive range of beats, `(measure, beat)` in score order.
pub type Range = ((usize, usize), (usize, usize));

pub struct View {
    pub zoom: f32,
    pub fonts: FontSet,
    /// The beat being played, shown as Guitar Pro's moving playback cursor.
    pub play: Option<(usize, usize)>,
    pub selection: Option<Range>,
    pub layout: LayoutMode,
    /// Every track, stacked in each system, instead of the cursor's track alone.
    pub multitrack: bool,
    /// Edit › Multivoice Editing: each voice in its own colour, and a click picks any voice's note.
    pub multivoice: bool,
    /// Tracks hidden from the multitrack view (the global view's eye), one bit per track index.
    pub hidden: u64,
}

/// The tracks drawn: the cursor's alone, or in the multitrack view every track not hidden by the
/// global view's eye. The cursor's track is always drawn; tracks past the 64th cannot be hidden.
pub fn shown_tracks(count: usize, cursor_track: usize, multitrack: bool, hidden: u64) -> Vec<usize> {
    if !multitrack {
        return vec![cursor_track];
    }
    (0..count).filter(|&i| i == cursor_track || !is_hidden(hidden, i)).collect()
}

/// Whether track `i` is hidden in `hidden` (see [`View::hidden`]).
pub fn is_hidden(hidden: u64, i: usize) -> bool {
    u32::try_from(i).ok().and_then(|i| 1u64.checked_shl(i)).is_some_and(|bit| hidden & bit != 0)
}

pub struct Output {
    pub click: Option<Click>,
    /// A drag across beats: from where it started to where the pointer is.
    pub drag: Option<Range>,
    pub cursor_rect: Option<Rect>,
    pub play_rect: Option<Rect>,
}

/// The layout metrics for `track`: a drum staff or notation over tab, with room for chord names.
/// `notation` false leaves standard notation out of every track (a print option); otherwise each
/// track shows its own notations.
fn metrics_for(score: &Score, track: &Track, zoom: f32, notation: bool, voice: usize) -> Metrics {
    let (standard, tab) = track.notation.drawn();
    let m = if track.is_drums() { Metrics::for_drums(zoom) } else { Metrics::new(zoom, true, track.string_count()).with_notations(standard && notation, tab) };
    m.with_page(&score.page).with_annotations(track).with_sections(score).with_voice(voice)
}

/// Tracks laid out for paper at 100 %: one track, or several stacked in each system as a full
/// score.
pub struct PrintLayout {
    /// The page: paper, margins and the header's chord list (the first track's).
    pub metrics: Metrics,
    /// The tracks drawn in each system, top first, with their own metrics.
    parts: Vec<(usize, Metrics)>,
    pub pages: Vec<PageBox>,
}

pub fn print_layout(score: &Score, track_index: usize, notation: bool) -> Option<PrintLayout> {
    print_score_layout(score, &[track_index], notation)
}

/// `tracks` stacked in each system, in this order, as View › Multitrack shows them.
pub fn print_score_layout(score: &Score, tracks: &[usize], notation: bool) -> Option<PrintLayout> {
    let mut parts = Vec::with_capacity(tracks.len());
    for (k, &ti) in tracks.iter().enumerate() {
        let track = score.tracks.get(ti)?;
        let m = metrics_for(score, track, 1.0, notation, 0);
        parts.push(Part { index: ti, track, metrics: if k == 0 { m } else { m.below() } });
    }
    let metrics = parts.first()?.metrics;
    let pages = layout_parts(score, &parts, &metrics);
    Some(PrintLayout { metrics, parts: parts.iter().map(|p| (p.index, p.metrics)).collect(), pages })
}

/// A document's pages: every chosen track laid out in order (or all of them stacked as one score),
/// numbered through the whole. The PDF, the printer and the print preview all take their pages
/// from one of these.
pub struct PrintSet {
    layouts: Vec<PrintLayout>,
}

impl PrintSet {
    /// Lays out `tracks` (indexes, in print order): each track on its own pages, or with
    /// `together` all of them in every system. An error says what was asked that cannot be printed.
    pub fn new(score: &Score, tracks: &[usize], notation: bool, together: bool) -> Result<PrintSet, String> {
        if tracks.is_empty() {
            return Err("no tracks to print".into());
        }
        let missing = |ti: &usize| format!("track {} does not exist", ti.saturating_add(1));
        if let Some(ti) = tracks.iter().find(|ti| score.tracks.get(**ti).is_none()) {
            return Err(missing(ti));
        }
        let layouts = if together {
            vec![print_score_layout(score, tracks, notation).ok_or_else(|| "no tracks to print".to_string())?]
        } else {
            tracks.iter().map(|ti| print_layout(score, *ti, notation).ok_or_else(|| missing(ti))).collect::<Result<_, _>>()?
        };
        Ok(PrintSet { layouts })
    }

    /// Pages in the whole document.
    pub fn pages(&self) -> usize {
        self.layouts.iter().map(|l| l.pages.len()).sum()
    }

    /// Page `n` (0-based): its layout and the page's place in it.
    pub fn page(&self, n: usize) -> Option<(&PrintLayout, usize)> {
        let mut before = 0;
        for layout in &self.layouts {
            let here = layout.pages.len();
            if n < before + here {
                return Some((layout, n - before));
            }
            before += here;
        }
        None
    }

    /// Paints page `n` (0-based) with its top-left corner at the origin.
    pub fn paint(&self, painter: &Painter, score: &Score, n: usize, fonts: FontSet) {
        if let Some((layout, page)) = self.page(n) {
            paint_print_page(painter, score, layout, page, (n + 1, self.pages()), fonts);
        }
    }
}

/// Paints page `page` of a print layout with its top-left corner at the origin: the same
/// engraving the page view draws, without the cursor, selection or playback marks. `number` is
/// `(this page, pages in the document)` for the page number.
pub fn paint_print_page(painter: &Painter, score: &Score, pl: &PrintLayout, page: usize, number: (usize, usize), fonts: FontSet) {
    let Some(boxed) = pl.pages.get(page) else { return };
    let mut ctxs: Vec<Ctx> = pl
        .parts
        .iter()
        .filter_map(|&(ti, m)| {
            let track = score.tracks.get(ti)?;
            Some(Ctx {
                pen: Pen { painter, fonts, space: m.space },
                m,
                score,
                track,
                // A cursor on a bar that does not exist is never drawn.
                cursor: Cursor { track: ti, measure: usize::MAX, ..Cursor::default() },
                // Paper is white and the inks are the score's own, whatever the theme.
                t: &crate::theme::PAPER,
                hits: Vec::new(),
                cursor_rect: None,
                play: None,
                play_rect: None,
                selection: None,
                clef: Clef::for_tuning(track.tuning.strings.iter().copied().max().unwrap_or(64)),
                lyrics: sung(track),
                tempo_curves: score.tempo_curves(),
                multivoice: false,
            })
        })
        .collect();
    let m = pl.metrics;
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(m.page_w, m.page_h));
    if page == 0
        && let Some(cx) = ctxs.first()
    {
        cx.header(rect);
    }
    painter.text(
        rect.right_bottom() - vec2(m.margin_right, m.margin_bottom * 0.5),
        egui::Align2::RIGHT_CENTER,
        format!("{}/{}", number.0, number.1),
        serif(9.0 * m.zoom),
        Color32::from_gray(110),
    );
    paint_systems(&mut ctxs, boxed, rect, page == 0);
}

pub fn show(ui: &mut egui::Ui, score: &Score, cursor: Cursor, view: &View) -> Output {
    let none = Output { click: None, drag: None, cursor_rect: None, play_rect: None };
    if score.tracks.get(cursor.track).is_none() {
        return none;
    }
    // The tracks drawn: the cursor's, or every track in the multitrack view (the top one carries
    // the bar's markers and directions).
    let indices = shown_tracks(score.tracks.len(), cursor.track, view.multitrack, view.hidden);
    let window = (ui.available_width() - 2.0 * PAGE_GAP).max(1.0);
    let parts: Vec<Part> = indices
        .iter()
        .enumerate()
        .filter_map(|(k, &ti)| {
            let track = score.tracks.get(ti)?;
            let voice = if ti == cursor.track { cursor.voice } else { 0 };
            let m = metrics_for(score, track, view.zoom, true, voice).with_mode(view.layout, window);
            Some(Part { index: ti, track, metrics: if k == 0 { m } else { m.below() } })
        })
        .collect();
    // The page's header, chord list included, belongs to the cursor's track.
    let Some(page_m) = parts.first().map(|p| p.metrics) else { return none };
    let page_m = parts.iter().find(|p| p.index == cursor.track).map_or(page_m, |p| page_m.with_chord_list_of(&p.metrics));
    let pages = layout_parts(score, &parts, &page_m);
    let system_h: f32 = parts.iter().map(|p| p.metrics.system_h()).sum();
    // Pages, or one sheet as wide as the window (screen) or as the line of bars (horizontal).
    let bottom = pages.last().and_then(|p| p.systems.last()).map_or(page_m.margin_top, |sys| sys.y + system_h);
    let (sheet_w, sheet_h) = match view.layout {
        LayoutMode::Page => (page_m.page_w, page_m.page_h),
        LayoutMode::Screen => (page_m.page_w, bottom + page_m.margin_bottom),
        LayoutMode::Horizontal => {
            let widest = pages.iter().flat_map(|p| &p.systems).map(SystemBox::width).fold(0.0f32, f32::max);
            (page_m.margin_left + page_m.indent() + widest + page_m.margin_right, bottom + page_m.margin_bottom)
        }
    };
    let width = (sheet_w + 2.0 * PAGE_GAP).max(ui.available_width());
    let height = pages.len() as f32 * (sheet_h + PAGE_GAP) + PAGE_GAP;
    let (response, painter) = ui.allocate_painter(vec2(width, height), egui::Sense::click_and_drag());
    let t = tokens();
    painter.rect_filled(response.rect, 0.0, t.workspace);
    let left = response.rect.left() + (width - sheet_w) / 2.0;
    let mut ctxs: Vec<Ctx> = parts
        .iter()
        .map(|p| {
            let mine = p.index == cursor.track;
            Ctx {
                pen: Pen { painter: &painter, fonts: view.fonts, space: p.metrics.space },
                m: p.metrics,
                score,
                track: p.track,
                // Only the cursor's track shows the cursor, the selection and the playback mark.
                cursor: if mine { cursor } else { Cursor { track: p.index, measure: usize::MAX, ..Cursor::default() } },
                t,
                hits: Vec::new(),
                cursor_rect: None,
                play: view.play.filter(|_| mine),
                play_rect: None,
                selection: view.selection.filter(|_| mine),
                clef: Clef::for_tuning(p.track.tuning.strings.iter().copied().max().unwrap_or(64)),
                lyrics: sung(p.track),
                tempo_curves: if p.metrics.top { score.tempo_curves() } else { Vec::new() },
                multivoice: view.multivoice && mine,
            }
        })
        .collect();

    for (pi, page) in pages.iter().enumerate() {
        let top = response.rect.top() + PAGE_GAP + pi as f32 * (sheet_h + PAGE_GAP);
        let rect = Rect::from_min_size(pos2(left, top), vec2(sheet_w, sheet_h));
        if !painter.clip_rect().intersects(rect) {
            continue;
        }
        if view.layout == LayoutMode::Page {
            painter.rect_filled(rect.translate(vec2(3.0, 4.0)), 0.0, Color32::from_black_alpha(70));
        }
        painter.rect_filled(rect, 0.0, t.page);
        if pi == 0
            && view.layout != LayoutMode::Horizontal
            && let Some(cx) = ctxs.iter().find(|c| c.cursor.track == cursor.track && c.cursor.measure != usize::MAX).or(ctxs.first())
        {
            cx.header(rect);
        }
        if view.layout == LayoutMode::Page {
            painter.text(
                rect.right_bottom() - vec2(page_m.margin_right, page_m.margin_bottom * 0.5),
                egui::Align2::RIGHT_CENTER,
                format!("{}/{}", pi + 1, pages.len()),
                serif(9.0 * page_m.zoom),
                Color32::from_gray(110),
            );
        }
        paint_systems(&mut ctxs, page, rect, pi == 0);
    }

    // Clicks may land on any track's staves; a drag stays in the cursor's track.
    let mut click = None;
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
        && let Some((cx, h)) = ctxs.iter().find_map(|cx| cx.hits.iter().find(|h| h.rect.contains(pos)).map(|h| (cx, h)))
    {
        let m = cx.m;
        let string = (m.tab && pos.y >= h.tab_top - m.tab_gap / 2.0 && pos.y <= h.tab_top + m.tab_h() + m.tab_gap / 2.0)
            .then(|| (((pos.y - h.tab_top) / m.tab_gap).round().max(0.0) as usize).min(cx.track.string_count().saturating_sub(1)));
        let h = pick(&cx.hits, pos, string).unwrap_or(h);
        let voice = (cx.multivoice && !h.edit).then_some(h.voice);
        let shift = ui.input(|i| i.modifiers.shift) && voice.is_none();
        click = Some(Click::Beat { track: cx.cursor.track, measure: h.measure, beat: h.beat, string, voice, shift });
    }
    let mine = ctxs.iter().find(|c| c.cursor.track == cursor.track);
    let hit_at = |p: egui::Pos2| mine.and_then(|cx| cx.hits.iter().find(|h| h.edit && h.rect.contains(p)).map(|h| (h.measure, h.beat)));
    let drag = if response.dragged() {
        let origin = ui.input(|i| i.pointer.press_origin());
        match (origin.and_then(hit_at), response.interact_pointer_pos().and_then(hit_at)) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        }
    } else {
        None
    };
    Output { click, drag, cursor_rect: mine.and_then(|c| c.cursor_rect), play_rect: mine.and_then(|c| c.play_rect) }
}

/// Paints a page's systems with every part stacked in each: one track's staves under another's,
/// joined at the left by the system line when there are several.
fn paint_systems(ctxs: &mut [Ctx], page: &PageBox, rect: Rect, first_page: bool) {
    let Some(first) = ctxs.first() else { return };
    let (m, t, painter) = (first.m, first.t, first.pen.painter);
    let height: f32 = ctxs.iter().map(|c| c.m.system_h()).sum();
    for (si, sys) in page.systems.iter().enumerate() {
        let top = rect.top() + sys.y;
        let sys_rect = Rect::from_min_size(pos2(rect.left(), top), vec2(rect.width(), height));
        if !painter.clip_rect().intersects(sys_rect) {
            continue;
        }
        let x0 = rect.left() + m.margin_left + sys.indent;
        let mut y = top;
        let mut span: Option<(f32, f32)> = None;
        for cx in ctxs.iter_mut() {
            let own = sys.of_track(cx.cursor.track);
            cx.system(&own, x0, y, first_page && si == 0);
            let (a, b) = cx.staff_span(y);
            span = Some(span.map_or((a, b), |(s0, _)| (s0, b)));
            y += cx.m.system_h();
        }
        if ctxs.len() > 1
            && let Some((a, b)) = span
        {
            painter.line_segment([pos2(x0, a), pos2(x0, b)], Stroke::new((0.16 * m.space).max(1.0), t.bar_line));
        }
    }
}

/// The beat a click at `pos` lands on. Under Multivoice Editing several voices' beats may be
/// there: the one with a note on the clicked string wins, else the voice being edited.
fn pick(hits: &[Hit], pos: egui::Pos2, string: Option<usize>) -> Option<&Hit> {
    let mut here = hits.iter().filter(|h| h.rect.contains(pos));
    let on_string = |h: &&Hit| string.is_some_and(|st| h.strings.iter().any(|n| usize::from(*n) == st));
    let first = here.clone().next();
    here.clone().find(on_string).or_else(|| here.find(|h| h.edit)).or(first)
}

struct Hit {
    rect: Rect,
    measure: usize,
    beat: usize,
    voice: usize,
    /// The voice being edited: drags and plain clicks only use these.
    edit: bool,
    /// Strings the beat has notes on, so a click on a string picks the voice that plays there.
    strings: Vec<u8>,
    tab_top: f32,
}

/// A placed beat within one system, for effects that connect to the next note.
struct Placed<'a> {
    x: f32,
    w: f32,
    beat: &'a Beat,
}

struct Ctx<'a> {
    pen: Pen<'a>,
    m: Metrics,
    score: &'a Score,
    track: &'a Track,
    cursor: Cursor,
    t: &'static Tokens,
    hits: Vec<Hit>,
    cursor_rect: Option<Rect>,
    play: Option<(usize, usize)>,
    play_rect: Option<Rect>,
    selection: Option<Range>,
    clef: Clef,
    /// Each bar's tempo at its start and end, for drawing tempo automations, worked out once per paint.
    tempo_curves: Vec<(f64, f64)>,
    /// Each line of lyrics laid on its beats, `(measure, beat, syllable)`, worked out once per paint.
    lyrics: Vec<Vec<(usize, usize, guitario_score::Syllable)>>,
    /// Multivoice Editing: voices in colour and clickable. Never on paper.
    multivoice: bool,
}

/// Where in a bar (from its left edge) the beat starting `tick` ticks in sits: the first drawn
/// voice that has one there.
fn beat_x_at(measure: Option<&guitario_score::Measure>, mb: &crate::layout::MeasureBox, tick: u32) -> Option<f32> {
    let measure = measure?;
    mb.voices.iter().find_map(|vc| {
        let beats = &measure.voices.get(vc.voice)?.beats;
        let mut at = 0u32;
        let i = beats.iter().position(|b| {
            let here = at == tick;
            at = at.saturating_add(b.duration.ticks());
            here
        })?;
        vc.beats.get(i).copied()
    })
}

/// The tick each beat starts at within its bar.
fn onsets(beats: &[Beat]) -> Vec<u32> {
    let mut at = 0u32;
    beats
        .iter()
        .map(|b| {
            let start = at;
            at = at.saturating_add(b.duration.ticks());
            start
        })
        .collect()
}

/// Whether a stem-down chord at `down` must step right of a stem-up one at `up` sounding with it
/// (staff positions, and each one's head kind and dots): a second between them, or one pitch in
/// both with different heads or dots. Identical heads on one pitch are shared.
fn heads_clash(up: &[i32], (up_kind, up_dots): (u32, u8), down: &[i32], (kind, dots): (u32, u8)) -> bool {
    down.iter().any(|p| up.iter().any(|q| (p - q).abs() == 1 || (p == q && (up_kind != kind || up_dots != dots))))
}

/// The notehead a beat takes: whole, half or black.
fn head_kind(beat: &Beat) -> u32 {
    beat.duration.value.divisor().min(4)
}

/// The key of bar `mi`, C when there is no such bar.
fn mi_key(score: &Score, mi: usize) -> i8 {
    score.headers.get(mi).map_or(0, |h| h.key)
}

/// The track's lyrics laid on its beats, line by line.
fn sung(track: &Track) -> Vec<Vec<(usize, usize, guitario_score::Syllable)>> {
    track.lyrics.iter().map(|l| guitario_score::dispatch_lyrics(track, l)).collect()
}

impl<'a> Ctx<'a> {
    fn p(&self) -> &Painter {
        self.pen.painter
    }

    /// The ink of a voice's notes: its own colour in Multivoice Editing, black otherwise.
    fn voice_ink(&self, voice: usize) -> Color32 {
        if self.multivoice { self.t.voice_ink.get(voice).copied().unwrap_or(self.t.ink) } else { self.t.ink }
    }

    fn header(&self, page: Rect) {
        let m = &self.m;
        let meta = &self.score.meta;
        let mut y = page.top() + m.margin_top;
        let title = if meta.title.is_empty() { "Untitled" } else { meta.title.as_str() };
        let center = page.left() + m.center_x();
        self.p().text(pos2(center, y), egui::Align2::CENTER_TOP, title, serif(33.0 * m.zoom), self.t.ink);
        // Guitar Pro sets the subtitle close under the title.
        y += crate::layout::TITLE_H * m.space;
        let left = page.left() + m.margin_left;
        let right = page.right() - m.margin_right;
        for line in crate::layout::title_lines(meta) {
            use crate::layout::TitleLine as L;
            let size = |pt: f32| serif(pt * m.zoom);
            match &line {
                L::Subtitle(s) => {
                    self.p().text(pos2(center, y), egui::Align2::CENTER_TOP, s, size(20.0), self.t.ink);
                }
                L::Artist(s) => {
                    self.p().text(pos2(center, y), egui::Align2::CENTER_TOP, s, size(16.0), self.t.ink);
                }
                L::Album(s) => {
                    self.p().text(pos2(center, y), egui::Align2::CENTER_TOP, s, size(13.0), self.t.ink);
                }
                L::WordsAndMusic(s) => {
                    self.p().text(pos2(center, y), egui::Align2::CENTER_TOP, format!("Words & Music by {s}"), size(11.0), self.t.ink);
                }
                L::Credits { words, music } => {
                    if !words.is_empty() {
                        self.p().text(pos2(left, y), egui::Align2::LEFT_TOP, format!("Words by {words}"), size(11.0), self.t.ink);
                    }
                    if !music.is_empty() {
                        self.p().text(pos2(right, y), egui::Align2::RIGHT_TOP, format!("Music by {music}"), size(11.0), self.t.ink);
                    }
                }
            }
            y += line.height() * m.space;
        }
        // The copyright sits at the foot of the first page, as Guitar Pro prints it.
        if !meta.copyright.trim().is_empty() {
            let foot = page.bottom() - m.margin_bottom * 0.5;
            self.p().text(pos2(center, foot), egui::Align2::CENTER_CENTER, meta.copyright.trim(), serif(9.0 * m.zoom), self.t.ink);
        }
        if !meta.author.is_empty() {
            self.p().text(pos2(page.right() - m.margin_right, y), egui::Align2::RIGHT_TOP, format!("Tab: {}", meta.author), serif(10.0 * m.zoom), self.t.ink);
        }
        // The tuning at the top of the score, as Guitar Pro's default stylesheet shows it:
        // "Standard tuning", or each string numbered in a circle with its note, in two columns.
        let font = serif(12.0 * m.zoom);
        let line = crate::layout::LEGEND_LINE * m.space;
        match crate::layout::tuning_legend(self.track) {
            crate::layout::TuningLegend::None => {}
            crate::layout::TuningLegend::Standard => {
                self.p().text(pos2(left, y), egui::Align2::LEFT_TOP, "Standard tuning", font.clone(), self.t.ink);
                y += line;
            }
            crate::layout::TuningLegend::Strings(notes) => {
                let rows = notes.len().div_ceil(2);
                let col_w = 5.6 * m.space;
                for (i, note) in notes.iter().enumerate() {
                    let (col, row) = (i / rows.max(1), i % rows.max(1));
                    let x = left + col as f32 * col_w;
                    let cy = y + row as f32 * line + line / 2.0;
                    let r = 0.75 * m.space;
                    self.p().circle_stroke(pos2(x + r, cy), r, Stroke::new(0.8, self.t.ink));
                    self.p().text(pos2(x + r, cy), egui::Align2::CENTER_CENTER, (i + 1).to_string(), serif(7.5 * m.zoom), self.t.ink);
                    self.p().text(pos2(x + 2.0 * r + 0.3 * m.space, cy), egui::Align2::LEFT_CENTER, format!("= {note}"), font.clone(), self.t.ink);
                }
                y += rows as f32 * line;
            }
        }
        if self.track.capo > 0 {
            self.p().text(pos2(left, y), egui::Align2::LEFT_TOP, format!("Capo fret {}", self.track.capo), font, self.t.ink);
        }
        self.chord_list(page);
    }

    /// The chord list under the title: each distinct diagram of the track once, named, in rows
    /// across the page.
    fn chord_list(&self, page: Rect) {
        let m = &self.m;
        let chords = crate::layout::chord_list(self.track);
        if chords.is_empty() || m.chord_list == 0 {
            return;
        }
        let s = m.space;
        let (cell, per_row) = m.chord_cell();
        let top = page.top() + m.margin_top + m.chord_list_top(self.score);
        for (i, chord) in chords.iter().take(m.chord_list).enumerate() {
            let (row, col) = (i / per_row, i % per_row);
            let cx = page.left() + m.margin_left + (col as f32 + 0.5) * cell;
            let y = top + row as f32 * m.chord_row_h();
            self.p().text(pos2(cx, y), egui::Align2::CENTER_TOP, &chord.name, serif(10.0 * m.zoom), self.t.ink);
            self.chord_diagram(cx, y + 2.4 * s, chord);
        }
    }

    /// The top of the first staff and the bottom of the last of a system whose top is at `top`.
    fn staff_span(&self, top: f32) -> (f32, f32) {
        let m = &self.m;
        let first = if m.notation { top + m.notation_top() } else { top + m.tab_top() };
        let last = if m.tab { top + m.tab_top() + m.tab_h() } else { top + m.notation_top() + m.staff_h() };
        (first, last)
    }

    /// A key change inside a system from `old` to `new`, starting at `x`: naturals for what the
    /// old key loses, then the new key's sharps or flats.
    fn key_change(&self, old: i8, new: i8, x: f32, nt: f32) -> f32 {
        let s = self.m.space;
        let ink = self.t.ink;
        let mut x = x;
        let old_positions = self.clef.key_positions(old > 0);
        for i in crate::notation::key_cancel(old, new) {
            if let Some(p) = old_positions.get(i) {
                self.pen.glyph(g::NATURAL, pos2(x, self.nota_y(nt, *p)), ink);
                x += s;
            }
        }
        let glyph = if new > 0 { g::SHARP } else { g::FLAT };
        for p in self.clef.key_positions(new > 0).iter().take(usize::from(new.unsigned_abs().min(7))) {
            self.pen.glyph(glyph, pos2(x, self.nota_y(nt, *p)), ink);
            x += s;
        }
        x
    }

    fn nota_y(&self, top: f32, pos: i32) -> f32 {
        top + self.m.staff_h() - pos as f32 * self.m.space / 2.0
    }

    fn tab_y(&self, tab_top: f32, string: usize) -> f32 {
        tab_top + string as f32 * self.m.tab_gap
    }

    fn system(&mut self, sys: &SystemBox, x0: f32, top: f32, first: bool) {
        let (m, t) = (self.m, self.t);
        let s = m.space;
        let nt = top + m.notation_top();
        let tt = top + m.tab_top();
        let tb = tt + m.tab_h();
        let width = sys.prefix + sys.measures.iter().map(|x| x.width).sum::<f32>();
        let x1 = x0 + width;
        // The staff runs on past the last bar line to its courtesy signatures.
        let staff_end = x1 + sys.courtesy;
        let line = Stroke::new((0.13 * s).max(0.8), t.staff_line);
        let bar = Stroke::new((0.16 * s).max(1.0), t.bar_line);
        let strings = self.track.string_count();

        // Staves.
        if m.notation {
            for i in 0..5 {
                let y = nt + i as f32 * s;
                self.p().line_segment([pos2(x0, y), pos2(staff_end, y)], line);
            }
        }
        if m.tab {
            for st in 0..strings {
                let y = self.tab_y(tt, st);
                self.p().line_segment([pos2(x0, y), pos2(staff_end, y)], line);
            }
        }
        // System line and the bracket joining notation and tab.
        let sys_top = if m.notation { nt } else { tt };
        self.p().line_segment([pos2(x0, sys_top), pos2(x0, if m.tab { tb } else { nt + 4.0 * s })], bar);
        if m.notation && m.tab {
            let bx = x0 - 0.9 * s;
            self.p().line_segment([pos2(bx, nt - 0.3 * s), pos2(bx, tb + 0.3 * s)], Stroke::new(0.5 * s, t.ink));
            g::arc(self.p(), pos2(bx, nt - 0.5 * s), pos2(bx + 1.6 * s, nt - 1.0 * s), -0.2 * s, Stroke::new(0.25 * s, t.ink));
            g::arc(self.p(), pos2(bx, tb + 0.5 * s), pos2(bx + 1.6 * s, tb + 1.0 * s), 0.2 * s, Stroke::new(0.25 * s, t.ink));
        }

        // The abbreviated track name, vertical, left of the first system (Guitar Pro's default
        // stylesheet: first system, abbreviated, vertical).
        if first {
            let name_y = if !m.tab {
                nt + 2.0 * s
            } else if m.notation {
                (nt + tb) / 2.0
            } else {
                (tt + tb) / 2.0
            };
            self.vertical_text(short_name(self.track), pos2(x0 - 3.2 * s, name_y), serif(10.0 * m.zoom), t.ink);
        }

        // Clefs and key signature.
        let first_mi = sys.measures.first().map_or(0, |x| x.index);
        let key = self.score.headers.get(first_mi).map_or(0, |h| h.key);
        if m.drums {
            self.pen.glyph(g::PERC_CLEF, pos2(x0 + 0.8 * s, self.nota_y(nt, 4)), t.ink);
        } else if m.notation {
            let (c, pos) = match self.clef {
                Clef::Treble => (g::G_CLEF, 2),
                Clef::Bass => (g::F_CLEF, 6),
            };
            self.pen.glyph(c, pos2(x0 + 0.6 * s, self.nota_y(nt, pos)), t.ink);
            let sharps = key > 0;
            let glyph = if sharps { g::SHARP } else { g::FLAT };
            for (i, p) in self.clef.key_positions(sharps).iter().take(usize::from(key.unsigned_abs().min(7))).enumerate() {
                self.pen.glyph(glyph, pos2(x0 + (4.0 + i as f32) * s, self.nota_y(nt, *p)), t.ink);
            }
        }
        if m.tab {
            self.tab_clef(x0 + 1.6 * s, tt, tb);
        }

        // Courtesy signatures: what the next system opens with.
        if sys.courtesy > 0.0
            && let Some(last) = sys.measures.last()
        {
            let (key, time) = crate::layout::courtesy_of(self.score, last.index);
            let mut x = x1 + 0.6 * s;
            if let Some((old, new)) = key.filter(|_| m.notation && !m.drums) {
                x = self.key_change(old, new, x, nt);
            }
            if let Some(ts) = time {
                // Centred on its widest number, clear of the bar line.
                let wide = |n: u8| n.to_string().chars().filter_map(|c| c.to_digit(10)).map(|d| self.pen.width(g::time_digit(d))).sum::<f32>();
                let half = wide(ts.numerator).max(wide(ts.denominator)) / 2.0;
                self.time_signature(&MeasureHeader::new(ts), x + half - 0.9 * s + 0.2 * s, nt, tt, tb);
            }
        }

        // The system's first bar number (after the clef, clear of the bracket, as Guitar Pro
        // places it) and the opening tempo mark, over the top track.
        if m.top {
            self.bar_number(first_mi, x0 + sys.prefix + 0.2 * s, sys_top);
        }
        if first && m.top {
            self.tempo_mark(x0 + sys.prefix, sys_top - m.tempo_row(), self.score.tempo);
        }

        // Measures.
        // Placed beats per voice, in order along the system.
        let mut placed: Vec<(usize, Vec<Placed>)> = Vec::new();
        // Where each beat of the first voice sits, for the lyrics.
        let mut voice1_x: Vec<((usize, usize), f32)> = Vec::new();
        let last_measure = self.score.headers.len().saturating_sub(1);
        for (si, mb) in sys.measures.iter().enumerate() {
            let mx = x0 + sys.prefix + mb.x;
            // The bar before this one in the same system, for ties that cross the bar line.
            let prev = si.checked_sub(1).and_then(|k| sys.measures.get(k)).map(|p| (p, x0 + sys.prefix + p.x));
            let header = self.score.headers.get(mb.index);
            let Some(h) = header else { continue };
            let end = mx + mb.width;
            if si > 0 && m.top {
                self.bar_number(mb.index, mx + 0.2 * s, sys_top);
            }
            if mb.key_w > 0.0 && m.notation && !m.drums {
                let old = mi_key(self.score, mb.index.wrapping_sub(1));
                self.key_change(old, h.key, mx + 0.6 * s, nt);
            }
            if mb.show_time && !h.free_time {
                self.time_signature(h, mx + mb.key_w + 0.9 * s, nt, tt, tb);
            }
            if h.repeat_open {
                let lx = mx + mb.key_w + if mb.show_time { 3.6 * s } else { 0.2 * s };
                self.repeat_sign(lx, nt, tt, tb, true);
            }
            if h.repeat_close > 0 {
                self.repeat_sign(end, nt, tt, tb, false);
            }
            if h.repeat_close > 0 && m.top {
                // Clear of the bar line, where the next bar's number starts.
                self.p().text(
                    pos2(end - 0.6 * s, sys_top - 0.6 * s),
                    egui::Align2::RIGHT_BOTTOM,
                    format!("x{}", u16::from(h.repeat_close) + 1),
                    serif(9.0 * m.zoom),
                    t.ink,
                );
            }
            // A repeat sign is the bar line; otherwise the final bar or a plain one.
            if h.repeat_close == 0 && mb.index == last_measure {
                self.final_bar(end, nt, tt, tb);
            } else if h.repeat_close == 0 {
                if m.notation {
                    self.p().line_segment([pos2(end, nt), pos2(end, nt + 4.0 * s)], bar);
                }
                if m.tab {
                    self.p().line_segment([pos2(end, tt), pos2(end, tb)], bar);
                }
                if h.double_bar {
                    self.staves_lines(end - 0.6 * s, nt, tt, tb, bar);
                }
            }
            if m.top {
                self.bar_marks(h, mb.index, mx, end, sys_top, si == 0);
            }
            let measure = self.track.measures.get(mb.index);
            // Fermatas over the beats they hold, above the top staff.
            for f in &h.fermatas {
                if let Some(x) = beat_x_at(measure, mb, f.tick) {
                    let glyph = match f.kind {
                        guitario_score::FermataKind::Short => g::FERMATA_SHORT,
                        guitario_score::FermataKind::Medium => g::FERMATA,
                        guitario_score::FermataKind::Long => g::FERMATA_LONG,
                    };
                    self.pen.glyph_centered(glyph, pos2(mx + x, sys_top - 1.6 * s), t.ink);
                }
            }
            // Playback follows Voice 1; the cursor, selection and clicks use the voice being edited.
            let edit_cols = mb.cols(self.cursor.voice);
            let edit_voice = edit_cols.map_or(0, |c| c.voice);
            let band = |l: f32, r: f32| Rect::from_min_max(pos2(mx + l, sys_top - 1.2 * s), pos2(mx + r, tb + 1.2 * s));
            if let Some((pm, pb)) = self.play
                && pm == mb.index
            {
                // Playback: tint the bar, mark the sounding beat.
                let bar_rect = Rect::from_min_max(pos2(mx, sys_top - 1.2 * s), pos2(end, tb + 1.2 * s));
                self.p().rect_filled(bar_rect, 0.0, t.play_bar);
                if let Some(&(l, r)) = mb.voices.first().and_then(|v| v.spans.get(pb)) {
                    let col = band(l, r);
                    self.p().rect_filled(col, 0.0, t.play_beat);
                    self.play_rect = Some(col);
                }
            }
            if m.notation {
                self.notation_measure(h, mb, prev, mx, nt);
            }
            // Selection: one continuous band per measure, running to the bar lines where the
            // selection carries on into the neighbouring measure.
            if let Some(((m0, b0), (m1, b1))) = self.selection
                && (m0..=m1).contains(&mb.index)
            {
                let count = measure.and_then(|x| x.voices.get(edit_voice)).map_or(0, |v| v.beats.len());
                let first = if mb.index == m0 { b0 } else { 0 };
                let last = if mb.index == m1 { b1 } else { count.saturating_sub(1) };
                let span = |bi: usize| edit_cols.and_then(|c| c.spans.get(bi)).copied();
                let left = if mb.index > m0 { Some(mx) } else { span(first).map(|(l, _)| mx + l) };
                let right = if mb.index < m1 { Some(end) } else { span(last).map(|(_, r)| mx + r) };
                if let (Some(l), Some(r)) = (left, right) {
                    self.p().rect_filled(Rect::from_min_max(pos2(l, sys_top - 1.2 * s), pos2(r, tb + 1.2 * s)), 0.0, t.selection_fill);
                }
            }
            for vc in &mb.voices {
                let Some(beats) = measure.and_then(|x| x.voices.get(vc.voice)).map(|v| v.beats.as_slice()) else { continue };
                let slot = match placed.iter().position(|(v, _)| *v == vc.voice) {
                    Some(i) => i,
                    None => {
                        placed.push((vc.voice, Vec::new()));
                        placed.len() - 1
                    }
                };
                for (bi, beat) in beats.iter().enumerate() {
                    let (Some(cx), Some(&(l, r))) = (vc.beats.get(bi), vc.spans.get(bi)) else { continue };
                    let x = mx + cx;
                    if vc.voice == 0 {
                        voice1_x.push(((mb.index, bi), x));
                    }
                    let (col, w) = (band(l, r), r - l);
                    if vc.voice == edit_voice && self.cursor.measure == mb.index && self.cursor.beat == bi {
                        self.p().rect_filled(col, 2.0, t.cursor_fill);
                        if m.tab {
                            let cy = self.tab_y(tt, self.cursor.string.min(strings.saturating_sub(1)));
                            let r = Rect::from_center_size(pos2(x, cy), vec2((1.6 * m.tab_gap).min(w - 2.0).max(m.tab_gap), m.tab_gap));
                            self.p().rect_stroke(r, 2.0, Stroke::new(1.5, t.cursor_stroke), egui::StrokeKind::Outside);
                        }
                        self.cursor_rect = Some(col);
                    }
                    if vc.voice == edit_voice || self.multivoice {
                        let strings = beat.notes.iter().map(|n| n.string).collect();
                        let edit = vc.voice == edit_voice;
                        self.hits.push(Hit { rect: col, measure: mb.index, beat: bi, voice: vc.voice, edit, strings, tab_top: tt });
                    }
                    // Chord names, then beat text, above the system as Guitar Pro stacks them.
                    if vc.voice == 0 {
                        if let Some(chord) = beat.chord.as_ref() {
                            if !chord.name.is_empty() {
                                self.p().text(pos2(x, sys_top - m.chord_name_above()), egui::Align2::CENTER_BOTTOM, &chord.name, serif(10.5 * m.zoom), t.ink);
                            }
                            if m.diagrams && !chord.frets.is_empty() {
                                self.chord_diagram(x, sys_top - m.chord_name_above() + 0.4 * s, chord);
                            }
                        }
                        if let Some(text) = &beat.text {
                            self.p().text(pos2(x, sys_top - 5.1 * s), egui::Align2::CENTER_BOTTOM, text, serif_italic(8.5 * m.zoom), t.ink);
                        }
                    }
                    if let Some((_, list)) = placed.get_mut(slot) {
                        list.push(Placed { x, w, beat });
                    }
                }
            }
        }
        // Tab content after the cursor so frets sit on top of it.
        if !m.drums {
            for (voice, list) in &placed {
                let ink = self.voice_ink(*voice);
                if m.tab {
                    for (i, pb) in list.iter().enumerate() {
                        self.tab_beat(pb, list.get(..i).unwrap_or_default(), list.get(i + 1..).unwrap_or_default(), tt, ink);
                    }
                }
                // Over the tablature, or under the staff when it shows alone.
                self.effect_lanes(list, tt);
                if m.whammy {
                    self.whammy_lane(list, tt);
                }
            }
        }
        if !m.notation {
            self.tab_rhythm(sys, x0, tb);
        }
        self.lyrics_lane(top, &voice1_x, x1);
    }

    /// The lines of lyrics under the system: each syllable centred under its beat, a hyphen midway
    /// to the next syllable of its word (or after it at the end of the system).
    fn lyrics_lane(&self, top: f32, xs: &[((usize, usize), f32)], x_end: f32) {
        let m = self.m;
        if m.lyric_lines == 0 || xs.is_empty() {
            return;
        }
        let font = serif(9.5 * m.zoom);
        let (first, last) = (xs.first().map_or(0, |e| e.0.0), xs.last().map_or(0, |e| e.0.0));
        let x_of = |key: (usize, usize)| xs.iter().find(|(k, _)| *k == key).map(|(_, x)| *x);
        for (k, line) in self.lyrics.iter().enumerate().take(m.lyric_lines) {
            let y = top + m.lyrics_top() + (k as f32 + 1.0) * m.lyric_line_h();
            let here: Vec<&(usize, usize, guitario_score::Syllable)> = line.iter().filter(|(mi, _, _)| (first..=last).contains(mi)).collect();
            for (i, (mi, bi, syl)) in here.iter().enumerate() {
                let Some(x) = x_of((*mi, *bi)) else { continue };
                let r = self.p().text(pos2(x, y), egui::Align2::CENTER_BOTTOM, &syl.text, font.clone(), self.t.ink);
                if syl.hyphen {
                    // Midway to where the next syllable starts, when it is in this system.
                    let next_left = here.get(i + 1).and_then(|(m2, b2, s2)| {
                        let nx = x_of((*m2, *b2))?;
                        Some(nx - self.p().layout_no_wrap(s2.text.clone(), font.clone(), self.t.ink).size().x / 2.0)
                    });
                    let hx = match next_left {
                        Some(l) if l > r.right() + 0.8 * m.space => (r.right() + l) / 2.0,
                        _ => (r.right() + 0.6 * m.space).min(x_end),
                    };
                    self.p().text(pos2(hx, y), egui::Align2::CENTER_BOTTOM, "-", font.clone(), self.t.ink);
                }
            }
        }
    }

    /// A chord diagram centred on `cx` with its top at `top`: the lowest string on the left, a nut
    /// (or the first fret's number) at the top, a dot per fretted string, `o` for an open string and
    /// `x` for one not played, and a barre where the lowest fret is held across three or more strings.
    fn chord_diagram(&self, cx: f32, top: f32, chord: &guitario_score::Chord) {
        let s = self.m.space;
        let ink = self.t.ink;
        let n = chord.frets.len();
        if n == 0 {
            return;
        }
        let gap = 0.8 * s;
        let row = 0.9 * s;
        let first = chord.first_fret.max(1);
        let highest = chord.frets.iter().flatten().copied().max().unwrap_or(0);
        let rows = usize::from(highest.saturating_sub(first)).saturating_add(1).clamp(4, 7);
        let left = cx - crate::layout::diagram_width(n, s) / 2.0;
        let right = left + crate::layout::diagram_width(n, s);
        let y0 = top + 1.0 * s;
        let thin = Stroke::new((0.08 * s).max(0.7), ink);
        // Our strings run highest first; the diagram puts the lowest on the left.
        let column = |i: usize| left + (n - 1 - i) as f32 * gap;
        for k in 0..=rows {
            let y = y0 + k as f32 * row;
            self.p().line_segment([pos2(left, y), pos2(right, y)], thin);
        }
        for i in 0..n {
            self.p().line_segment([pos2(column(i), y0), pos2(column(i), y0 + rows as f32 * row)], thin);
        }
        if first == 1 {
            self.p().line_segment([pos2(left - 0.05 * s, y0), pos2(right + 0.05 * s, y0)], Stroke::new(0.3 * s, ink));
        } else {
            self.p().text(pos2(left - 0.3 * s, y0 + row / 2.0), egui::Align2::RIGHT_CENTER, format!("{first}fr"), serif(7.0 * self.m.zoom), ink);
        }
        let mark = 0.22 * s;
        for (i, fret) in chord.frets.iter().enumerate() {
            let x = column(i);
            match fret {
                None => {
                    let c = pos2(x, y0 - 0.5 * s);
                    self.p().line_segment([c - vec2(mark, mark), c + vec2(mark, mark)], thin);
                    self.p().line_segment([c - vec2(mark, -mark), c + vec2(mark, -mark)], thin);
                }
                Some(0) => {
                    self.p().circle_stroke(pos2(x, y0 - 0.5 * s), mark, thin);
                }
                Some(f) if *f >= first && usize::from(f - first) < rows => {
                    self.p().circle_filled(pos2(x, y0 + (f32::from(f - first) + 0.5) * row), 0.3 * s, ink);
                }
                Some(_) => {}
            }
        }
        // A barre: the lowest fret held across three or more strings, none open or muted between; a
        // chord with an open string (an open D) is fingered, not barred.
        if let Some(low) = chord.frets.iter().flatten().copied().filter(|f| *f > 0).min()
            && low >= first
            && !chord.frets.contains(&Some(0))
        {
            let held: Vec<usize> = chord.frets.iter().enumerate().filter(|(_, f)| **f == Some(low)).map(|(i, _)| i).collect();
            if let (Some(&a), Some(&b)) = (held.first(), held.last())
                && b >= a + 2
                && chord.frets.get(a..=b).is_some_and(|span| span.iter().all(|f| f.is_some_and(|f| f >= low)))
            {
                let y = y0 + (f32::from(low - first) + 0.5) * row;
                self.p().line_segment([pos2(column(b), y), pos2(column(a), y)], Stroke::new(0.5 * s, ink));
            }
        }
    }

    /// What a bar shows above its system once, over the top track: alternate endings, directions,
    /// a change of triplet feel, the section marker and a tempo change.
    fn bar_marks(&self, h: &MeasureHeader, index: usize, mx: f32, end: f32, sys_top: f32, opens_system: bool) {
        let (m, t) = (self.m, self.t);
        let s = m.space;
        // Alternate endings: a bracket over the bar, numbered where the ending starts and
        // closed where a repeat ends it.
        if h.endings != 0 {
            let y = sys_top - 3.2 * s;
            let st = Stroke::new(1.0, t.ink);
            let prev = index.checked_sub(1).and_then(|i| self.score.headers.get(i)).map_or(0, |p| p.endings);
            let starts = prev != h.endings;
            let x_start = mx + 0.3 * s;
            let x_end = end - 0.5 * s;
            self.p().line_segment([pos2(x_start, y), pos2(x_end, y)], st);
            if starts {
                self.p().line_segment([pos2(x_start, y), pos2(x_start, y + 1.6 * s)], st);
                let label: Vec<String> = (0..8).filter(|b| h.endings & (1 << b) != 0).map(|b| format!("{}.", b + 1)).collect();
                // Right of the bar number under the bracket's start.
                self.p().text(pos2(x_start + 1.6 * s, y + 0.2 * s), egui::Align2::LEFT_TOP, label.join(" "), serif(8.5 * m.zoom), t.ink);
            }
            if h.repeat_close > 0 {
                // Short, so the repeat count under it stays clear.
                self.p().line_segment([pos2(x_end, y), pos2(x_end, y + 0.8 * s)], st);
            }
        }
        // Directions: segno and coda marks at the start of the bar, Fine and jumps at its end, in
        // their own row between the endings and the section markers.
        let dir_y = sys_top - 4.2 * s;
        let mut dx = mx + 0.6 * s;
        let mark = |glyph: char, x: f32| self.pen.glyph_scaled(glyph, pos2(x, dir_y), DIRECTION_SCALE, t.ink);
        for target in &h.targets {
            match target {
                DirectionTarget::Segno => dx += mark(g::SEGNO, dx) + 0.3 * s,
                DirectionTarget::SegnoSegno => {
                    dx += mark(g::SEGNO, dx);
                    dx += mark(g::SEGNO, dx) + 0.3 * s;
                }
                DirectionTarget::Coda => dx += mark(g::CODA, dx) + 0.3 * s,
                DirectionTarget::DoubleCoda => dx += mark(g::CODA_SQUARE, dx) + 0.3 * s,
                DirectionTarget::Fine => {
                    self.p().text(pos2(end - 0.4 * s, dir_y), egui::Align2::RIGHT_BOTTOM, "Fine", serif_italic(9.5 * m.zoom), t.ink);
                }
            }
        }
        if let Some(jump) = h.jump {
            let y = if h.targets.contains(&DirectionTarget::Fine) { dir_y - 1.8 * s } else { dir_y };
            let mut x = end - 0.4 * s;
            if matches!(jump, Jump::ToCoda | Jump::ToDoubleCoda) {
                let glyph = if jump == Jump::ToCoda { g::CODA } else { g::CODA_SQUARE };
                x -= self.pen.width(glyph) * DIRECTION_SCALE;
                self.pen.glyph_scaled(glyph, pos2(x, y), DIRECTION_SCALE, t.ink);
                x -= 0.4 * s;
            }
            self.p().text(pos2(x, y), egui::Align2::RIGHT_BOTTOM, jump.label(), serif_italic(9.5 * m.zoom), t.ink);
        }
        // Triplet feel where it changes.
        let prev_feel = index.checked_sub(1).and_then(|i| self.score.headers.get(i)).map_or(TripletFeel::None, |p| p.triplet_feel);
        if h.triplet_feel != prev_feel {
            let label = match h.triplet_feel {
                TripletFeel::None => "straight",
                TripletFeel::Triplet8th => "swing 8ths (♪♪ = ♩♪³)",
                TripletFeel::Triplet16th => "swing 16ths",
                TripletFeel::Dotted8th => "dotted 8ths (♪♪ = ♪.♬)",
                TripletFeel::Dotted16th => "dotted 16ths",
                TripletFeel::Scottish8th => "Scottish 8ths",
                TripletFeel::Scottish16th => "Scottish 16ths",
            };
            // In the tempo row, after the tempo mark when the bar has one (Guitar Pro sets the
            // swing indication beside the tempo).
            let tempo_here = h.tempo.is_some() || index == 0;
            let x = mx + if tempo_here { 9.5 * s } else { 0.5 * s };
            self.p().text(pos2(x, sys_top - m.tempo_row()), egui::Align2::LEFT_BOTTOM, label, serif(10.0 * m.zoom), t.ink);
        }
        if let Some(text) = &h.marker {
            // At the top of the lane, above chord names and text: the letter boxed, then the name
            // in bold; a name in brackets frames the letter and name together (Guitar Pro).
            let (name, framed) = crate::section_dialog::display_name(text);
            let letter = h.section_letter.as_deref().map(str::trim).filter(|l| !l.is_empty());
            let font = serif_bold(13.0 * m.zoom);
            let at = pos2(mx + 0.4 * s, sys_top - m.section_row());
            let pad = vec2(0.3 * s, 0.1 * s);
            let frame = |r: Rect| self.p().rect_stroke(r, 0.0, Stroke::new(1.0, t.ink), egui::StrokeKind::Inside);
            if framed {
                let label = match letter {
                    Some(l) if name.is_empty() => l.to_string(),
                    Some(l) => format!("{l} {name}"),
                    None => name.to_string(),
                };
                let galley = self.p().layout_no_wrap(label, font, t.ink);
                let r = Rect::from_min_size(at, galley.size() + 2.0 * pad);
                frame(r);
                self.p().galley(r.min + pad, galley, t.ink);
            } else {
                let mut x = at.x;
                if let Some(l) = letter {
                    let galley = self.p().layout_no_wrap(l.to_string(), font.clone(), t.ink);
                    let r = Rect::from_min_size(pos2(x, at.y), galley.size() + 2.0 * pad);
                    frame(r);
                    self.p().galley(r.min + pad, galley, t.ink);
                    x = r.right() + 0.5 * s;
                }
                if !name.is_empty() {
                    self.p().text(pos2(x, at.y + pad.y), egui::Align2::LEFT_TOP, name, font, t.ink);
                }
            }
        }
        let marked = h.tempo.is_some() && index > 0;
        if let Some(tempo) = h.tempo
            && index > 0
        {
            self.tempo_mark(mx + 0.5 * s, sys_top - m.tempo_row(), tempo);
        }
        // A tempo automation gliding through the bar: "accel." or "rit." where the glide starts (and
        // again at the start of each system), then a dashed line to the end of the bar.
        if let Some(&(from, to)) = self.tempo_curves.get(index)
            && (to - from).abs() > 1e-9
        {
            let starts = index.checked_sub(1).and_then(|i| self.tempo_curves.get(i)).is_none_or(|p| (p.1 - p.0).abs() < 1e-9);
            let y = sys_top - m.tempo_row();
            let mut x = mx + if marked { 9.0 * s } else { 0.5 * s };
            if starts || opens_system {
                let r = self.p().text(pos2(x, y), egui::Align2::LEFT_BOTTOM, if to > from { "accel." } else { "rit." }, serif_italic(9.5 * m.zoom), t.ink);
                x = r.right() + 0.4 * s;
            }
            let dy = y - 0.5 * s;
            let mut dx = x;
            while dx < end - 0.8 * s {
                self.p().line_segment([pos2(dx, dy), pos2((dx + 0.6 * s).min(end), dy)], Stroke::new(1.0, t.ink));
                dx += 1.2 * s;
            }
        }
    }

    /// A tempo mark, `♩ = 120` in bold as Guitar Pro's stylesheet sets it.
    fn tempo_mark(&self, x: f32, y: f32, bpm: u16) {
        let s = self.m.space;
        let w = self.pen.glyph_scaled(g::MET_QUARTER_UP, pos2(x, y - 0.3 * s), 0.72, self.t.ink);
        self.p().text(pos2(x + w + 0.6 * s, y), egui::Align2::LEFT_BOTTOM, format!("= {bpm}"), serif_bold(13.5 * self.m.zoom), self.t.ink);
    }

    /// Bar `index`'s number at `x` over the staff whose top is `sys_top`: small and grey, red on
    /// screen when the bar's length does not match its time signature.
    fn bar_number(&self, index: usize, x: f32, sys_top: f32) {
        let (m, t) = (self.m, self.t);
        let ink = if self.bar_is_wrong(index) { t.bar_error } else { t.bar_number };
        self.p().text(pos2(x, sys_top - 0.6 * m.space), egui::Align2::LEFT_BOTTOM, (index + 1).to_string(), serif(9.5 * m.zoom), ink);
    }

    /// Whether a voice of bar `index` on this track is longer or shorter than the time signature
    /// asks (free time excepted; an empty extra voice is fine).
    fn bar_is_wrong(&self, index: usize) -> bool {
        let (Some(h), Some(bar)) = (self.score.headers.get(index), self.track.measures.get(index)) else { return false };
        !h.free_time && bar.voices.iter().enumerate().any(|(vi, v)| v.ticks() != h.capacity() && !(vi > 0 && v.beats.is_empty()))
    }

    /// The tablature clef: T, A and B stacked down the tab staff in a plain sans face, centred
    /// on `cx`, as Guitar Pro draws it.
    fn tab_clef(&self, cx: f32, tt: f32, tb: f32) {
        let gap = self.m.tab_gap;
        let step = ((tb - tt) / 3.3).min(1.55 * gap);
        // Arial's capitals are about 0.72 em tall: fill most of each step.
        let size = step * 0.92 / 0.72;
        let mid = (tt + tb) / 2.0;
        for (i, letter) in ["T", "A", "B"].into_iter().enumerate() {
            let y = mid + (i as f32 - 1.0) * step;
            self.p().text(pos2(cx, y), egui::Align2::CENTER_CENTER, letter, sans(size), self.t.ink);
        }
    }

    /// Text turned to read upwards, centred on `center`.
    fn vertical_text(&self, text: String, center: egui::Pos2, font: egui::FontId, ink: Color32) {
        let galley = self.p().layout_no_wrap(text, font, ink);
        let size = galley.size();
        // Turned a quarter anticlockwise about its top-left corner, the text runs up from there.
        let pos = pos2(center.x - size.y / 2.0, center.y + size.x / 2.0);
        self.p().add(egui::epaint::TextShape::new(pos, galley, ink).with_angle(-std::f32::consts::FRAC_PI_2));
    }

    fn time_signature(&self, h: &MeasureHeader, x: f32, nt: f32, tt: f32, tb: f32) {
        let s = self.m.space;
        let ts = h.time_signature;
        let draw = |num: u8, center_x: f32, y: f32, scale: f32| {
            let digits: Vec<char> = num.to_string().chars().filter_map(|c| c.to_digit(10)).map(g::time_digit).collect();
            let total: f32 = digits.iter().map(|d| self.pen.width(*d) * scale).sum();
            let mut dx = center_x - total / 2.0;
            for d in digits {
                dx += self.pen.glyph_scaled(d, pos2(dx, y), scale, self.t.ink);
            }
        };
        let cx = x + 0.9 * s;
        if self.m.notation {
            draw(ts.numerator, cx, nt + s, 1.0);
            draw(ts.denominator, cx, nt + 3.0 * s, 1.0);
        }
        // The tablature shows it only when it stands alone, as Guitar Pro does.
        if !self.m.tab || self.m.notation {
            return;
        }
        let tab_scale = ((tb - tt) / (4.0 * s)).clamp(0.8, 1.6);
        let mid = (tt + tb) / 2.0;
        draw(ts.numerator, cx, mid - (tb - tt) / 4.0, tab_scale);
        draw(ts.denominator, cx, mid + (tb - tt) / 4.0, tab_scale);
    }

    fn staves_lines(&self, x: f32, nt: f32, tt: f32, tb: f32, stroke: Stroke) {
        if self.m.notation {
            self.p().line_segment([pos2(x, nt), pos2(x, nt + 4.0 * self.m.space)], stroke);
        }
        if self.m.tab {
            self.p().line_segment([pos2(x, tt), pos2(x, tb)], stroke);
        }
    }

    fn repeat_sign(&self, x: f32, nt: f32, tt: f32, tb: f32, open: bool) {
        let s = self.m.space;
        let thick = Stroke::new(0.5 * s, self.t.ink);
        let thin = Stroke::new((0.16 * s).max(1.0), self.t.ink);
        let (xk, xn, xd) = if open { (x + 0.25 * s, x + 0.9 * s, x + 1.5 * s) } else { (x - 0.25 * s, x - 0.9 * s, x - 1.5 * s) };
        self.staves_lines(xk, nt, tt, tb, thick);
        self.staves_lines(xn, nt, tt, tb, thin);
        let r = 0.25 * s;
        if self.m.notation {
            for pos in [3, 5] {
                self.p().circle_filled(pos2(xd, self.nota_y(nt, pos)), r, self.t.ink);
            }
        }
        let mid = (tt + tb) / 2.0;
        if self.m.tab {
            for dy in [-0.5, 0.5] {
                self.p().circle_filled(pos2(xd, mid + dy * self.m.tab_gap * 1.2), r, self.t.ink);
            }
        }
    }

    fn final_bar(&self, x: f32, nt: f32, tt: f32, tb: f32) {
        let s = self.m.space;
        self.staves_lines(x - 0.25 * s, nt, tt, tb, Stroke::new(0.5 * s, self.t.ink));
        self.staves_lines(x - 0.9 * s, nt, tt, tb, Stroke::new((0.16 * s).max(1.0), self.t.ink));
    }

    // ---- standard notation -----------------------------------------------------------------

    /// Every voice of a bar on the staff. With more than one voice, Voice 1 takes the stems up and
    /// the next voice the stems down, and their rests move apart; accidentals carry across voices.
    fn notation_measure(&self, h: &MeasureHeader, mb: &crate::layout::MeasureBox, prev: Option<(&crate::layout::MeasureBox, f32)>, mx: f32, nt: f32) {
        let Some(measure) = self.track.measures.get(mb.index) else { return };
        let mut acc = Accidentals::default();
        let multi = mb.voices.len() > 1;
        // Stem-up voices' heads by onset, for the stem-down voices to step aside from.
        let up_heads: Vec<(u32, Vec<i32>, u32, u8)> = if multi {
            mb.voices
                .iter()
                .filter(|vc| vc.voice % 2 == 0)
                .filter_map(|vc| measure.voices.get(vc.voice))
                .flat_map(|v| onsets(&v.beats).into_iter().zip(&v.beats))
                .filter(|(_, b)| !b.rest)
                .map(|(t, b)| (t, self.head_positions(b, h.key), head_kind(b), b.duration.dots))
                .collect()
        } else {
            Vec::new()
        };
        let step = self.pen.width(g::HEAD_BLACK) + 0.1 * self.m.space;
        for vc in &mb.voices {
            let Some(voice) = measure.voices.get(vc.voice) else { continue };
            // A stem-down beat a second from a stem-up one, or on its pitch with another head or
            // dots, moves right of it; identical heads on one pitch are shared.
            let xs: Vec<f32> = if vc.voice % 2 == 1 {
                onsets(&voice.beats)
                    .into_iter()
                    .zip(&voice.beats)
                    .zip(&vc.beats)
                    .map(|((t, b), x)| {
                        let mine = if b.rest { Vec::new() } else { self.head_positions(b, h.key) };
                        let clash = up_heads
                            .iter()
                            .filter(|u| u.0 == t)
                            .any(|(_, theirs, kind, dots)| heads_clash(theirs, (*kind, *dots), &mine, (head_kind(b), b.duration.dots)));
                        if clash { x + step } else { *x }
                    })
                    .collect()
            } else {
                vc.beats.clone()
            };
            // Where a tie into this bar starts: the last note on that string in the previous bar.
            let incoming = |string: u8| -> Option<f32> {
                let (pmb, pmx) = prev?;
                let pv = self.track.measures.get(pmb.index)?.voices.get(vc.voice)?;
                let cols = pmb.voices.iter().find(|c| c.voice == vc.voice)?;
                let j = pv.beats.iter().rposition(|b| !b.rest && b.note_on(string).is_some())?;
                Some(pmx + cols.beats.get(j)?)
            };
            self.notation_voice(h, &voice.beats, &xs, (vc.voice, multi), (mx, nt), &incoming, &mut acc);
        }
    }

    /// The staff positions of a beat's noteheads in key `key`.
    fn head_positions(&self, beat: &Beat, key: i8) -> Vec<i32> {
        beat.notes
            .iter()
            .filter_map(|n| {
                if self.m.drums {
                    return Some(guitario_theory::drums::piece(n.fret).map_or(6, |p| i32::from(p.staff_pos)));
                }
                let p = self.track.tuning.pitch(usize::from(n.string), n.fret, self.track.capo)?;
                Some(staff_pos(spell(p.0.saturating_add(12).min(127), key), self.clef))
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn notation_voice(
        &self,
        h: &MeasureHeader,
        beats: &[Beat],
        xs: &[f32],
        (slot, multi): (usize, bool),
        (mx, nt): (f32, f32),
        incoming: &dyn Fn(u8) -> Option<f32>,
        acc: &mut Accidentals,
    ) {
        let s = self.m.space;
        let ink = self.voice_ink(slot);
        // Stems: by the notes' height alone, or by voice when the bar has several.
        let stem_up = |avg: f32| if multi { slot % 2 == 0 } else { self.m.drums || avg < 4.0 };
        // Positions and stems per beat.
        /// A notehead: staff position, accidental to print (-1 flat, 0 natural, 1 sharp) and head shape.
        type Head = (i32, Option<i8>, &'static str);
        struct Info {
            x: f32,
            /// Staff position of each note by string, for ties and slurs.
            by_string: Vec<(u8, i32)>,
            pos: Vec<Head>,
            up: bool,
            div: u32,
            /// Grace notes before the beat, and whether they are slashed (before the beat, not on it).
            graces: Vec<Head>,
            slashed: bool,
            /// Fingering by staff position: the left- and right-hand fingers.
            fingers: Vec<(i32, Option<guitario_score::Finger>, Option<guitario_score::Finger>)>,
            /// The pitch each bend reaches (when the stylesheet shows it): the bent note's staff
            /// position, then the target's, its accidental and whether it is a quarter tone higher.
            bends: Vec<(i32, i32, Option<i8>, bool)>,
        }
        let mut infos: Vec<Option<Info>> = Vec::new();
        for (bi, beat) in beats.iter().enumerate() {
            let x = mx + xs.get(bi).copied().unwrap_or(0.0);
            if beat.rest || beat.notes.is_empty() {
                let div = beat.duration.value.divisor();
                // With several voices a rest moves out of the way: up for Voice 1, down for the next.
                let shift = if !multi {
                    0
                } else if slot % 2 == 0 {
                    3
                } else {
                    -3
                };
                let pos = match div {
                    1 => 6,
                    _ => 4,
                } + shift;
                let c = g::rest_for(div);
                self.pen.glyph_centered(c, pos2(x, self.nota_y(nt, pos)), ink);
                self.dots(beat, x + 0.9 * s, nt, 5);
                infos.push(None);
                continue;
            }
            // Grace notes come before the beat, so their accidentals are spelled first.
            let graces: Vec<Head> = beat
                .notes
                .iter()
                .filter(|n| n.kind != NoteKind::Tie && !self.m.drums)
                .filter_map(|n| {
                    let g = n.effects.grace?;
                    let p = self.track.tuning.pitch(usize::from(n.string), g.fret, self.track.capo)?;
                    let sp = spell(p.0.saturating_add(12).min(127), h.key);
                    Some((staff_pos(sp, self.clef), acc.next(sp, h.key), if g.dead { "x" } else { "n" }))
                })
                .collect();
            let slashed = beat.notes.iter().filter_map(|n| n.effects.grace).any(|g| !g.on_beat);
            let keyed: Vec<(u8, Head)> = beat
                .notes
                .iter()
                .filter_map(|n| {
                    if self.m.drums {
                        // Drum notation: the kit map places each piece; cymbals take a cross.
                        let piece = guitario_theory::drums::piece(n.fret);
                        let pos = piece.map_or(6, |p| i32::from(p.staff_pos));
                        return Some((n.string, (pos, None, if piece.is_some_and(|p| p.cross) { "x" } else { "n" })));
                    }
                    let p = self.track.tuning.pitch(usize::from(n.string), n.fret, self.track.capo)?;
                    let written = p.0.saturating_add(12).min(127);
                    let sp = spell(written, h.key);
                    let head = if n.kind == NoteKind::Dead { "x" } else { "n" };
                    Some((n.string, (staff_pos(sp, self.clef), acc.next(sp, h.key), head)))
                })
                .collect();
            let fingers = keyed
                .iter()
                .filter_map(|(st, p)| {
                    let e = beat.note_on(*st)?.effects;
                    (e.left_finger.is_some() || e.right_finger.is_some()).then_some((p.0, e.left_finger, e.right_finger))
                })
                .collect();
            // Where bends end up, spelled after the notes so their accidentals follow them.
            let bends = if self.score.page.bend_pitch && !self.m.drums {
                keyed
                    .iter()
                    .filter_map(|(st, head)| {
                        let n = beat.note_on(*st).filter(|n| n.kind == NoteKind::Normal)?;
                        let q = n.effects.bend?.peak();
                        let p = self.track.tuning.pitch(usize::from(n.string), n.fret, self.track.capo)?;
                        let sp = spell(p.0.saturating_add(12).saturating_add(q / 2).min(127), h.key);
                        Some((head.0, staff_pos(sp, self.clef), acc.next(sp, h.key), q % 2 == 1))
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let by_string = keyed.iter().map(|(st, p)| (*st, p.0)).collect();
            let mut pos: Vec<Head> = keyed.into_iter().map(|(_, p)| p).collect();
            pos.sort_by_key(|p| p.0);
            let avg = pos.iter().map(|p| p.0).sum::<i32>() as f32 / pos.len().max(1) as f32;
            // Drum parts are written stems up (one voice, hands and feet together).
            infos.push(Some(Info { x, by_string, pos, up: stem_up(avg), div: beat.duration.value.divisor(), graces, slashed, fingers, bends }));
        }

        let groups = beam_groups(beats, h);
        // A beamed group shares one stem direction.
        for gr in &groups {
            let all: Vec<i32> = gr.iter().filter_map(|i| infos.get(*i).and_then(|x| x.as_ref())).flat_map(|x| x.pos.iter().map(|p| p.0)).collect();
            let avg = all.iter().sum::<i32>() as f32 / all.len().max(1) as f32;
            for i in gr {
                if let Some(Some(info)) = infos.get_mut(*i) {
                    info.up = stem_up(avg);
                }
            }
        }
        let beamed = |i: usize| groups.iter().any(|gr| gr.contains(&i));
        let head_w = self.pen.width(g::HEAD_BLACK);
        let stem = Stroke::new((0.12 * s).max(1.0), ink);
        let mut stem_ends: Vec<Option<(f32, f32)>> = vec![None; infos.len()];

        for (i, info) in infos.iter().enumerate() {
            let Some(info) = info else { continue };
            let beat = beats.get(i);
            let head = match info.div {
                1 => g::HEAD_WHOLE,
                2 => g::HEAD_HALF,
                _ => g::HEAD_BLACK,
            };
            let hw = if info.div == 1 { self.pen.width(g::HEAD_WHOLE) } else { head_w };
            let left = info.x - hw / 2.0;
            let mut prev: Option<i32> = None;
            let mut acc_col = 0;
            for (pos, accidental, kind) in &info.pos {
                let y = self.nota_y(nt, *pos);
                let shifted = prev.is_some_and(|p| pos - p == 1) && info.up;
                let hx = if shifted { left + hw } else { left };
                prev = if shifted { None } else { Some(*pos) };
                // Ledger lines.
                let ledger = Stroke::new((0.16 * s).max(1.0), ink);
                let mut lp = if *pos <= -2 { -2 } else { 10 };
                while (*pos <= -2 && lp >= *pos) || (*pos >= 10 && lp <= *pos) {
                    let ly = self.nota_y(nt, lp);
                    self.p().line_segment([pos2(hx - 0.4 * s, ly), pos2(hx + hw + 0.4 * s, ly)], ledger);
                    lp += if *pos <= -2 { -2 } else { 2 };
                }
                self.pen.glyph(if *kind == "x" { g::HEAD_X } else { head }, pos2(hx, y), ink);
                if let Some(a) = accidental {
                    let c = match a {
                        1 => g::SHARP,
                        -1 => g::FLAT,
                        _ => g::NATURAL,
                    };
                    let aw = self.pen.width(c);
                    self.pen.glyph(c, pos2(left - aw - 0.3 * s - acc_col as f32 * 1.1 * s, y), ink);
                    acc_col = (acc_col + 1) % 3;
                }
                if let Some(b) = beat {
                    let dot_pos = if pos % 2 == 0 { pos + 1 } else { *pos };
                    self.dots(b, left + hw + 0.4 * s, nt, dot_pos);
                }
            }
            // Fingering: left-hand digits beside the heads, right-hand letters stacked above the beat.
            if !info.fingers.is_empty() {
                let room = if acc_col > 0 { 1.4 * s } else { 0.0 };
                for (pos, left_finger, _) in &info.fingers {
                    if let Some(f) = left_finger {
                        self.p().text(
                            pos2(left - 0.35 * s - room, self.nota_y(nt, *pos)),
                            egui::Align2::RIGHT_CENTER,
                            f.left_mark(),
                            serif(7.5 * self.m.zoom),
                            ink,
                        );
                    }
                }
                let hi = info.pos.last().map_or(4, |p| p.0);
                let base = if info.up { hi + 8 } else { hi + 3 }.max(10);
                let mut rights: Vec<(i32, guitario_score::Finger)> = info.fingers.iter().filter_map(|(p, _, r)| r.map(|r| (*p, r))).collect();
                rights.sort_by_key(|(p, _)| *p);
                for (k, (_, f)) in rights.iter().enumerate() {
                    let y = self.nota_y(nt, base) - k as f32 * 1.3 * s;
                    self.p().text(pos2(info.x, y), egui::Align2::CENTER_BOTTOM, f.right_mark(), serif_italic(8.0 * self.m.zoom), ink);
                }
            }
            // The pitch a bend reaches: a small head after the note, slurred to it.
            for &(from, to, accidental, quarter) in &info.bends {
                self.bend_target(left + hw + 1.1 * s, nt, from, to, accidental, quarter, left + hw / 2.0);
            }
            // Grace notes: small, left of the heads and their accidentals, slurred into the lowest head.
            if !info.graces.is_empty() {
                let room = if acc_col > 0 { 1.4 * s } else { 0.0 };
                if let Some((gx, gpos)) = self.grace_group(left - 0.9 * s - room, nt, &info.graces, info.slashed) {
                    let main = info.pos.first().map_or(4, |p| p.0);
                    let arc = Stroke::new((0.14 * s).max(1.0), ink);
                    g::arc(self.p(), pos2(gx, self.nota_y(nt, gpos) + 0.6 * s), pos2(left + 0.2 * hw, self.nota_y(nt, main) + 0.7 * s), 0.5 * s, arc);
                }
            }
            // Stem.
            let (lo, hi) = (info.pos.first().map_or(4, |p| p.0), info.pos.last().map_or(4, |p| p.0));
            if info.div >= 2 {
                let (sx, y0, y1) = if info.up {
                    (left + hw - stem.width / 2.0, self.nota_y(nt, lo), self.nota_y(nt, hi.max(1) + 7).min(self.nota_y(nt, 4)))
                } else {
                    (left + stem.width / 2.0, self.nota_y(nt, hi), self.nota_y(nt, lo.min(7) - 7).max(self.nota_y(nt, 4)))
                };
                if let Some(slot) = stem_ends.get_mut(i) {
                    *slot = Some((sx, y1));
                }
                if !beamed(i) {
                    self.p().line_segment([pos2(sx, y0), pos2(sx, y1)], stem);
                    if let Some(f) = g::flag_for(info.div, info.up) {
                        self.pen.glyph(f, pos2(sx - stem.width / 2.0, y1), ink);
                    }
                }
            }
            // Articulations on the notehead side.
            if let Some(b) = beat {
                let (y, above) = if info.up { (self.nota_y(nt, lo.min(0)) + 1.5 * s, false) } else { (self.nota_y(nt, hi.max(8)) - 1.5 * s, true) };
                let mut dy = 0.0;
                if b.notes.iter().any(|n| n.effects.staccato) {
                    self.pen.glyph_centered(if above { g::STACCATO_ABOVE } else { '\u{E4A3}' }, pos2(info.x, y + dy), ink);
                    dy += if above { -s } else { s };
                }
                if b.notes.iter().any(|n| n.effects.accent) {
                    self.pen.glyph_centered(if above { g::ACCENT_ABOVE } else { '\u{E4A1}' }, pos2(info.x, y + dy), ink);
                    dy += if above { -s } else { s };
                }
                if b.notes.iter().any(|n| n.effects.heavy_accent) {
                    self.pen.glyph_centered(if above { g::MARCATO_ABOVE } else { g::MARCATO_BELOW }, pos2(info.x, y + dy), ink);
                }
            }
        }

        // Ties and slurs: arcs between noteheads, away from the stems (up for the upper voice).
        if !self.m.drums {
            let arc = Stroke::new((0.18 * s).max(1.0), ink);
            let side = |info: &Info| if multi { slot % 2 == 0 } else { !info.up };
            for (bi, beat) in beats.iter().enumerate() {
                let Some(Some(info)) = infos.get(bi) else { continue };
                let above = side(info);
                let dir = if above { -1.0 } else { 1.0 };
                for n in &beat.notes {
                    let Some(&(_, pos)) = info.by_string.iter().find(|(st, _)| *st == n.string) else { continue };
                    let y = self.nota_y(nt, pos) + dir * 0.7 * s;
                    // A tie joins this note to the last one on its string: in this bar, or the bar before.
                    if n.kind == NoteKind::Tie {
                        let from = (0..bi)
                            .rev()
                            .find(|&j| beats.get(j).is_some_and(|b| !b.rest && b.note_on(n.string).is_some()))
                            .and_then(|j| xs.get(j).map(|cx| mx + cx))
                            .or_else(|| incoming(n.string))
                            .unwrap_or(info.x - 3.2 * s);
                        g::arc(self.p(), pos2(from + 0.6 * head_w, y), pos2(info.x - 0.6 * head_w, y), dir * 0.9 * s, arc);
                    }
                }
                // A slur the user placed: from the outer head of this beat to that of a later one.
                if let Some(to) = beat.slur.and_then(|k| xs.get(bi + usize::from(k))).filter(|_| !beat.rest) {
                    let end = bi + usize::from(beat.slur.unwrap_or(0));
                    if let Some(Some(end_info)) = infos.get(end) {
                        let outer = |i: &Info| if above { i.pos.last() } else { i.pos.first() }.map_or(4, |h| h.0);
                        let lift = if above { -1.0 } else { 1.0 };
                        let (y0, y1) = (self.nota_y(nt, outer(info)) + lift * 0.9 * s, self.nota_y(nt, outer(end_info)) + lift * 0.9 * s);
                        let y = if above { y0.min(y1) } else { y0.max(y1) };
                        g::arc(self.p(), pos2(info.x + 0.3 * head_w, y), pos2(mx + to - 0.3 * head_w, y), dir * 1.4 * s, arc);
                    }
                }
                for n in &beat.notes {
                    let Some(&(_, pos)) = info.by_string.iter().find(|(st, _)| *st == n.string) else { continue };
                    let y = self.nota_y(nt, pos) + dir * 0.7 * s;
                    // A hammer-on, pull-off or legato slide is a slur to the next note on the string.
                    if n.effects.hammer || n.effects.slide == Some(Slide::Legato) {
                        let to = (bi + 1..beats.len()).find(|&j| beats.get(j).is_some_and(|b| !b.rest && b.note_on(n.string).is_some()));
                        if let Some(to) = to.and_then(|j| xs.get(j)) {
                            let y = y + dir * 0.3 * s;
                            g::arc(self.p(), pos2(info.x + 0.3 * head_w, y), pos2(mx + to - 0.3 * head_w, y), dir * 1.1 * s, arc);
                        }
                    }
                }
            }
        }

        // Beams.
        let beam_t = 0.5 * s;
        // Where the beams put the stems' ends, for what is drawn on the stems afterwards.
        let mut beamed_ends: Vec<(usize, f32, f32)> = Vec::new();
        for gr in &groups {
            let ends: Vec<(usize, f32, f32, bool, u32)> = gr
                .iter()
                .filter_map(|i| {
                    let info = infos.get(*i)?.as_ref()?;
                    let (x, y) = (*stem_ends.get(*i)?)?;
                    Some((*i, x, y, info.up, info.div))
                })
                .collect();
            let Some(&(_, x_first, y_first, up, _)) = ends.first() else { continue };
            let &(_, x_last, y_last, _, _) = ends.last().unwrap_or(&(0, x_first, y_first, up, 0));
            // The beam slopes with the outer notes, at most a staff space over the group, and lies
            // flat when an inner note reaches past both ends. It then moves out until every stem
            // is at least as long as the note alone would want.
            let inner_beyond =
                ends.iter().skip(1).take(ends.len().saturating_sub(2)).any(|e| if up { e.2 < y_first.min(y_last) } else { e.2 > y_first.max(y_last) });
            let slope = if x_last - x_first < 1.0 || inner_beyond { 0.0 } else { (y_last - y_first).clamp(-s, s) / (x_last - x_first) };
            let base = |x: f32| y_first + slope * (x - x_first);
            let offset = ends.iter().map(|e| e.2 - base(e.1));
            let offset = if up { offset.fold(0.0f32, f32::min) } else { offset.fold(0.0f32, f32::max) };
            let beam_at = |x: f32| base(x) + offset;
            beamed_ends.extend(ends.iter().map(|e| (e.0, e.1, beam_at(e.1))));
            for &(i, x, _, _, _) in &ends {
                let Some(Some(info)) = infos.get(i) else { continue };
                let (lo, hi) = (info.pos.first().map_or(4, |p| p.0), info.pos.last().map_or(4, |p| p.0));
                let y0 = self.nota_y(nt, if up { lo } else { hi });
                self.p().line_segment([pos2(x, y0), pos2(x, beam_at(x))], stem);
            }
            let dir = if up { 1.0 } else { -1.0 };
            for level in 0..4u32 {
                let need = 8u32 << level;
                let shift = dir * level as f32 * (beam_t + 0.25 * s);
                // A parallelogram along the beam line from `a` to `b`.
                let beam = |a: f32, b: f32| {
                    let (ya, yb) = (beam_at(a) + shift, beam_at(b) + shift);
                    egui::Shape::convex_polygon(vec![pos2(a, ya), pos2(b, yb), pos2(b, yb + dir * beam_t), pos2(a, ya + dir * beam_t)], ink, Stroke::NONE)
                };
                let mut k = 0;
                while let Some(&(_, x, _, _, div)) = ends.get(k) {
                    if div < need {
                        k += 1;
                        continue;
                    }
                    let mut j = k;
                    while ends.get(j + 1).is_some_and(|e| e.4 >= need) {
                        j += 1;
                    }
                    let x_end = ends.get(j).map_or(x, |e| e.1);
                    if j == k {
                        let stub = if k + 1 < ends.len() { s } else { -s };
                        self.p().add(beam(x.min(x + stub), x.max(x + stub)));
                    } else {
                        self.p().add(beam(x, x_end));
                    }
                    k = j + 1;
                }
            }
            // Tuplet number over the group.
            if let Some(b) = gr.first().and_then(|i| beats.get(*i))
                && b.duration.tuplet != (1, 1)
            {
                let cx = (x_first + x_last) / 2.0;
                let y = beam_at(cx) - dir * 1.2 * s;
                self.p().text(pos2(cx, y), egui::Align2::CENTER_CENTER, b.duration.tuplet.0.to_string(), serif(9.0 * self.m.zoom), ink);
            }
        }
        for (i, x, y) in beamed_ends {
            if let Some(slot) = stem_ends.get_mut(i) {
                *slot = Some((x, y));
            }
        }

        // Tremolo picking slashes through the stems, and trill marks above the staff.
        for (i, info) in infos.iter().enumerate() {
            let (Some(info), Some(b)) = (info, beats.get(i)) else { continue };
            if b.rest {
                continue;
            }
            let (lo, hi) = (info.pos.first().map_or(4, |p| p.0), info.pos.last().map_or(4, |p| p.0));
            if let Some(speed) = b.tremolo {
                match stem_ends.get(i).copied().flatten() {
                    // Halfway along the stem, clear of the beams and the flag.
                    Some((sx, y1)) => self.slashes(sx, (self.nota_y(nt, if info.up { lo } else { hi }) + y1) / 2.0, speed.slashes()),
                    // A whole note has no stem: the slashes sit above the head.
                    None => self.slashes(info.x, self.nota_y(nt, hi) - 1.8 * s, speed.slashes()),
                }
            }
            if b.notes.iter().any(|n| n.effects.trill.is_some()) {
                let next = infos.iter().skip(i + 1).flatten().next().map(|n| n.x - 0.8 * s);
                // Clear of the heads (and of an up-stem), but inside the lane above the staff.
                let pos = (if info.up { hi + 8 } else { hi + 3 }).clamp(12, 22);
                self.trill_mark(info.x - 0.4 * s, next.unwrap_or(info.x + 3.5 * s), self.nota_y(nt, pos));
            }
        }
    }

    /// Small grace notes whose heads end at `right`, sharing one stem up with an eighth flag, slashed
    /// when they come before the beat. Returns the heads' centre and the lowest head's staff position.
    /// A bend's target: a small head with its left edge at `x` on staff position `to` (with its
    /// accidental, raised a quarter tone when `quarter`), slurred from the bent note's head at
    /// `from_x`, `from`.
    #[allow(clippy::too_many_arguments)]
    fn bend_target(&self, x: f32, nt: f32, from: i32, to: i32, accidental: Option<i8>, quarter: bool, from_x: f32) {
        const SCALE: f32 = 0.62;
        let (s, ink) = (self.m.space, self.t.ink);
        let hw = self.pen.width(g::HEAD_BLACK) * SCALE;
        let sign = match (quarter, accidental) {
            (true, Some(1)) => Some(g::THREE_QUARTER_SHARP),
            (true, _) => Some(g::QUARTER_SHARP),
            (false, Some(1)) => Some(g::SHARP),
            (false, Some(-1)) => Some(g::FLAT),
            (false, Some(_)) => Some(g::NATURAL),
            (false, None) => None,
        };
        let aw = sign.map_or(0.0, |c| self.pen.width(c) * SCALE + 0.15 * s);
        let head_x = x + aw;
        let y = self.nota_y(nt, to);
        let thin = Stroke::new((0.1 * s).max(0.8), ink);
        let mut lp = if to <= -2 { -2 } else { 10 };
        while (to <= -2 && lp >= to) || (to >= 10 && lp <= to) {
            let ly = self.nota_y(nt, lp);
            self.p().line_segment([pos2(head_x - 0.25 * s, ly), pos2(head_x + hw + 0.25 * s, ly)], thin);
            lp += if to <= -2 { -2 } else { 2 };
        }
        if let Some(c) = sign {
            self.pen.glyph_scaled(c, pos2(x, y), SCALE, ink);
        }
        self.pen.glyph_scaled(g::HEAD_BLACK, pos2(head_x, y), SCALE, ink);
        let arc = Stroke::new((0.14 * s).max(1.0), ink);
        g::arc(self.p(), pos2(from_x, self.nota_y(nt, from) - 0.6 * s), pos2(head_x + hw / 2.0, y - 0.5 * s), -0.5 * s, arc);
    }

    fn grace_group(&self, right: f32, nt: f32, graces: &[(i32, Option<i8>, &'static str)], slashed: bool) -> Option<(f32, i32)> {
        const SCALE: f32 = 0.62;
        let (s, ink) = (self.m.space, self.t.ink);
        let mut heads = graces.to_vec();
        heads.sort_by_key(|g| g.0);
        let (lo, hi) = (heads.first()?.0, heads.last()?.0);
        let hw = self.pen.width(g::HEAD_BLACK) * SCALE;
        let left = right - hw;
        let thin = Stroke::new((0.1 * s).max(0.8), ink);
        let mut acc_col = 0;
        for (pos, accidental, kind) in &heads {
            let y = self.nota_y(nt, *pos);
            let mut lp = if *pos <= -2 { -2 } else { 10 };
            while (*pos <= -2 && lp >= *pos) || (*pos >= 10 && lp <= *pos) {
                let ly = self.nota_y(nt, lp);
                self.p().line_segment([pos2(left - 0.25 * s, ly), pos2(left + hw + 0.25 * s, ly)], thin);
                lp += if *pos <= -2 { -2 } else { 2 };
            }
            self.pen.glyph_scaled(if *kind == "x" { g::HEAD_X } else { g::HEAD_BLACK }, pos2(left, y), SCALE, ink);
            if let Some(a) = accidental {
                let c = match a {
                    1 => g::SHARP,
                    -1 => g::FLAT,
                    _ => g::NATURAL,
                };
                let aw = self.pen.width(c) * SCALE;
                self.pen.glyph_scaled(c, pos2(left - aw - 0.2 * s - acc_col as f32 * 0.8 * s, y), SCALE, ink);
                acc_col = (acc_col + 1) % 2;
            }
        }
        let sx = left + hw - thin.width / 2.0;
        let (y0, y1) = (self.nota_y(nt, lo), self.nota_y(nt, hi + 5));
        self.p().line_segment([pos2(sx, y0), pos2(sx, y1)], thin);
        if let Some(f) = g::flag_for(8, true) {
            self.pen.glyph_scaled(f, pos2(sx - thin.width / 2.0, y1), SCALE, ink);
        }
        if slashed {
            self.p().line_segment([pos2(sx - 0.6 * s, y1 + 1.3 * s), pos2(sx + 0.8 * s, y1 + 0.3 * s)], thin);
        }
        Some((left + hw / 2.0, lo))
    }

    /// Tremolo picking: short diagonal strokes across a stem, centred on `(x, y)`.
    fn slashes(&self, x: f32, y: f32, count: u8) {
        let s = self.m.space;
        let stroke = Stroke::new((0.4 * s).max(1.5), self.t.ink);
        for k in 0..count {
            let dy = (f32::from(k) - f32::from(count.saturating_sub(1)) / 2.0) * 0.8 * s;
            self.p().line_segment([pos2(x - 0.7 * s, y + dy + 0.35 * s), pos2(x + 0.7 * s, y + dy - 0.35 * s)], stroke);
        }
    }

    /// "tr" and a wavy line from `x0` to `x1` at height `y`.
    fn trill_mark(&self, x0: f32, x1: f32, y: f32) {
        let (m, s) = (self.m, self.m.space);
        let ink = self.t.effect_ink;
        let galley = self.p().layout_no_wrap("tr".to_string(), serif_italic(10.0 * m.zoom), ink);
        let w = galley.size().x;
        self.p().galley(pos2(x0, y - galley.size().y / 2.0), galley, ink);
        g::wavy(self.p(), x0 + w + 0.2 * s, x1.max(x0 + w + 2.0 * s), y, 0.3 * s, Stroke::new(1.2, ink));
    }

    fn dots(&self, beat: &Beat, x: f32, nt: f32, pos: i32) {
        for d in 0..beat.duration.dots {
            self.pen.glyph(g::DOT, pos2(x + f32::from(d) * 0.6 * self.m.space, self.nota_y(nt, pos)), self.t.ink);
        }
    }

    // ---- tablature ---------------------------------------------------------------------------

    fn fret_text(n: &Note) -> String {
        match n.kind {
            NoteKind::Dead => "X".into(),
            NoteKind::Tie => format!("({})", n.fret),
            NoteKind::Normal if n.effects.ghost => format!("({})", n.fret),
            NoteKind::Normal if n.effects.harmonic.is_some() => format!("<{}>", n.fret),
            NoteKind::Normal => n.fret.to_string(),
        }
    }

    fn tab_beat(&self, pb: &Placed, before: &[Placed], rest: &[Placed], tt: f32, ink: Color32) {
        let (m, t) = (self.m, self.t);
        let s = m.space;
        if pb.beat.rest {
            return;
        }
        // Guitar Pro's fret numbers are about 0.6 of the line gap tall.
        let font = sans(m.tab_gap * 0.88);
        let fx = Stroke::new((0.13 * s).max(1.0), t.effect_ink);
        // Brush, arpeggio and rasgueado: a line beside the chord, pointing the way the strum goes.
        if let Some(brush) = pb.beat.brush
            && pb.beat.notes.len() > 1
        {
            let (lo, hi) = (pb.beat.notes.iter().map(|n| n.string).min().unwrap_or(0), pb.beat.notes.iter().map(|n| n.string).max().unwrap_or(0));
            let (top, bottom) = (self.tab_y(tt, usize::from(lo)) - 0.4 * m.tab_gap, self.tab_y(tt, usize::from(hi)) + 0.4 * m.tab_gap);
            let x = pb.x - 1.6 * s;
            if brush.kind == BrushKind::Arpeggio {
                let mut pts = Vec::new();
                let mut y = top;
                let mut k = 0;
                while y <= bottom {
                    pts.push(pos2(x + if k % 2 == 0 { -0.25 * s } else { 0.25 * s }, y));
                    y += 0.35 * s;
                    k += 1;
                }
                self.p().add(egui::Shape::line(pts, fx));
            } else {
                self.p().line_segment([pos2(x, top), pos2(x, bottom)], fx);
            }
            // A downstroke runs from the low strings (bottom) to the high strings (top).
            let (tip, dir) = if brush.down { (top, -1.0) } else { (bottom, 1.0) };
            self.p().add(egui::Shape::convex_polygon(
                vec![pos2(x, tip + dir * 0.5 * s), pos2(x - 0.35 * s, tip - dir * 0.2 * s), pos2(x + 0.35 * s, tip - dir * 0.2 * s)],
                t.effect_ink,
                Stroke::NONE,
            ));
            if brush.kind == BrushKind::Rasgueado {
                self.p().text(pos2(x, top - 0.5 * s), egui::Align2::CENTER_BOTTOM, "rasg.", serif(7.5 * m.zoom), t.effect_ink);
            }
        }
        for n in &pb.beat.notes {
            let y = self.tab_y(tt, usize::from(n.string));
            let galley = self.p().layout_no_wrap(Self::fret_text(n), font.clone(), ink);
            let r = Rect::from_center_size(pos2(pb.x, y), vec2(galley.size().x + 2.0, m.tab_gap * 0.9));
            self.p().rect_filled(r, 0.0, t.page);
            self.p().galley(pos2(r.center().x - galley.size().x / 2.0, y - galley.size().y / 2.0), galley, ink);
            let fw = r.width() / 2.0;
            if let Some(grace) = n.effects.grace.filter(|_| n.kind != NoteKind::Tie) {
                self.tab_grace(pb.x - fw, y, n.fret, grace);
            }
            // A tied fret is joined to the last note on its string by an arc over the strings.
            if n.kind == NoteKind::Tie {
                let from = before.iter().rev().find(|q| !q.beat.rest && q.beat.note_on(n.string).is_some()).map(|q| q.x + fw * 0.4);
                let a = pos2(from.unwrap_or(pb.x - fw - 2.6 * s), y - m.tab_gap * 0.55);
                let b = pos2(pb.x - fw * 0.4, y - m.tab_gap * 0.55);
                g::arc(self.p(), a, b, -0.8 * s, fx);
            }
            // The next note on this string, for hammer-ons and slides.
            let next = rest.iter().find_map(|q| q.beat.note_on(n.string).filter(|_| !q.beat.rest).map(|nn| (q.x, nn)));
            if n.effects.hammer {
                let (x2, label) = match next {
                    Some((x2, nn)) => (x2, if nn.fret >= n.fret { "H" } else { "P" }),
                    None => (pb.x + pb.w, "H"),
                };
                let a = pos2(pb.x + fw * 0.3, y - m.tab_gap * 0.55);
                let b = pos2(x2 - fw * 0.3, y - m.tab_gap * 0.55);
                g::arc(self.p(), a, b, -0.9 * s, fx);
                self.p().text(pos2((a.x + b.x) / 2.0, a.y - 1.2 * s), egui::Align2::CENTER_BOTTOM, label, serif(8.0 * m.zoom), t.effect_ink);
            }
            if let Some(sl) = n.effects.slide {
                let h = m.tab_gap * 0.32;
                let (a, b) = match sl {
                    Slide::Shift | Slide::Legato => {
                        let (x2, up) = next.map_or((pb.x + pb.w, true), |(x2, nn)| (x2, nn.fret >= n.fret));
                        let d = if up { 1.0 } else { -1.0 };
                        (pos2(pb.x + fw + 1.0, y + h * d), pos2(x2 - fw - 1.0, y - h * d))
                    }
                    Slide::IntoFromBelow => (pos2(pb.x - fw - 2.0 * s, y + h), pos2(pb.x - fw - 1.0, y - h * 0.3)),
                    Slide::IntoFromAbove => (pos2(pb.x - fw - 2.0 * s, y - h), pos2(pb.x - fw - 1.0, y + h * 0.3)),
                    Slide::OutDown => (pos2(pb.x + fw + 1.0, y - h * 0.3), pos2(pb.x + fw + 2.0 * s, y + h)),
                    Slide::OutUp => (pos2(pb.x + fw + 1.0, y + h * 0.3), pos2(pb.x + fw + 2.0 * s, y - h)),
                };
                self.p().line_segment([a, b], fx);
                if sl == Slide::Legato {
                    g::arc(self.p(), pos2(a.x, y - m.tab_gap * 0.55), pos2(b.x, y - m.tab_gap * 0.55), -0.8 * s, fx);
                }
            }
            if let Some(bend) = n.effects.bend {
                let x0 = pb.x + fw + 1.0;
                let x1 = next.map_or(pb.x + pb.w, |(x2, _)| x2 - fw - 1.0);
                let geom = crate::bend_draw::BendBox { x0, x1, y, top: tt - 2.2 * s, s, tied: n.kind == NoteKind::Tie };
                crate::bend_draw::paint_bend(self.p(), &bend, &geom, fx, t.effect_ink, serif(8.0 * m.zoom));
            }
        }
    }

    /// A grace note in the tab: a small fret just left of the note's fret, whose left edge is at
    /// `right`, with the slur, slide or bend that takes it into the note.
    fn tab_grace(&self, right: f32, y: f32, fret: u8, grace: guitario_score::Grace) {
        let (m, t) = (self.m, self.t);
        let s = m.space;
        let fx = Stroke::new((0.13 * s).max(1.0), t.effect_ink);
        let text = if grace.dead { "X".to_string() } else { grace.fret.to_string() };
        let galley = self.p().layout_no_wrap(text, sans(m.tab_gap * 0.7), t.ink);
        let gw = galley.size().x;
        let cx = right - 0.9 * s - gw / 2.0;
        let r = Rect::from_center_size(pos2(cx, y), vec2(gw + 1.0, m.tab_gap * 0.7));
        self.p().rect_filled(r, 0.0, t.page);
        self.p().galley(pos2(cx - gw / 2.0, y - galley.size().y / 2.0), galley, t.ink);
        let to = right + 0.1 * s;
        match grace.transition {
            guitario_score::GraceTransition::None => {}
            guitario_score::GraceTransition::Hammer => g::arc(self.p(), pos2(cx, y - m.tab_gap * 0.45), pos2(to, y - m.tab_gap * 0.5), -0.5 * s, fx),
            guitario_score::GraceTransition::Slide => {
                let h = m.tab_gap * 0.25 * if fret >= grace.fret { 1.0 } else { -1.0 };
                self.p().line_segment([pos2(r.right() + 0.5, y + h), pos2(to - 0.5, y - h)], fx);
            }
            guitario_score::GraceTransition::Bend => {
                let (a, b) = (pos2(r.right(), y), pos2(to, y - m.tab_gap * 0.6));
                self.p().add(egui::epaint::QuadraticBezierShape::from_points_stroke([a, pos2(b.x, a.y), b], false, Color32::TRANSPARENT, fx));
            }
        }
    }

    /// The whammy bar's lane over the other effects: "w/bar" where a run of beats using the bar
    /// starts, each beat's curve, and vibrato with the bar as a wavy line.
    fn whammy_lane(&self, placed: &[Placed], tt: f32) {
        let (m, t) = (self.m, self.t);
        let s = m.space;
        let y = tt - m.whammy_row();
        let font = serif(8.0 * m.zoom);
        let stroke = Stroke::new((0.12 * s).max(1.0), t.effect_ink);
        let uses = |b: &Beat| !b.rest && (b.whammy.is_some() || b.bar_vibrato.is_some());
        for (i, pb) in placed.iter().enumerate() {
            if !uses(pb.beat) {
                continue;
            }
            let prev = i.checked_sub(1).and_then(|j| placed.get(j)).map(|p| p.beat);
            let x0 = pb.x - 0.4 * s;
            if !prev.is_some_and(uses) {
                // A run starts: the tag just before it.
                crate::whammy_draw::paint_tag(self.p(), x0 - 0.4 * s, y, font.clone(), t.effect_ink);
            }
            let x1 = placed.get(i + 1).map_or(pb.x + pb.w, |n| n.x - 0.4 * s).max(x0 + 2.0 * s);
            if let Some(w) = pb.beat.whammy {
                let continues = w.start() != 0 && prev.and_then(|b| b.whammy).is_some_and(|before| before.end() == w.start());
                let b = crate::whammy_draw::WhammyBox { x0, x1, y, depth: 2.2 * s, continues };
                crate::whammy_draw::paint_whammy(self.p(), &w, &b, stroke, t.effect_ink, font.clone());
            }
            if let Some(v) = pb.beat.bar_vibrato {
                let (amp, width) = match v {
                    guitario_score::BarVibrato::Slight => (0.25 * s, 1.2),
                    guitario_score::BarVibrato::Wide => (0.5 * s, 2.0),
                };
                let vy = if pb.beat.whammy.is_some() { y - 1.2 * s } else { y };
                crate::whammy_draw::paint_bar_vibrato(self.p(), x0, x1, vy, amp, Stroke::new(width, t.effect_ink));
            }
        }
    }

    /// P.M., let ring and vibrato lanes above the tab staff, with dashed extenders over runs.
    fn effect_lanes(&self, placed: &[Placed], tt: f32) {
        let (m, t) = (self.m, self.t);
        let s = m.space;
        let font = serif(8.5 * m.zoom);
        let dash = Stroke::new(1.0, t.effect_ink);
        let lanes: [(&str, BeatTest, f32); 2] =
            [("let ring", |b| b.notes.iter().any(|n| n.effects.let_ring), 4.4), ("P.M.", |b| b.notes.iter().any(|n| n.effects.palm_mute), 2.9)];
        for (label, has, lane) in lanes {
            let y = tt - lane * s;
            let mut i = 0;
            while i < placed.len() {
                if !placed.get(i).is_some_and(|p| !p.beat.rest && has(p.beat)) {
                    i += 1;
                    continue;
                }
                let mut j = i;
                while placed.get(j + 1).is_some_and(|p| !p.beat.rest && has(p.beat)) {
                    j += 1;
                }
                let (Some(a), Some(b)) = (placed.get(i), placed.get(j)) else { break };
                let x0 = a.x - 0.6 * s;
                let galley = self.p().layout_no_wrap(label.to_string(), font.clone(), t.effect_ink);
                let tw = galley.size().x;
                self.p().galley(pos2(x0, y - galley.size().y / 2.0), galley, t.effect_ink);
                let x1 = b.x + b.w / 2.0 - 0.3 * s;
                if x1 > x0 + tw + 0.5 * s {
                    g::dashed(self.p(), pos2(x0 + tw + 0.4 * s, y), pos2(x1, y), dash, 0.4 * s);
                    self.p().line_segment([pos2(x1, y - 0.5 * s), pos2(x1, y + 0.5 * s)], dash);
                }
                i = j + 1;
            }
        }
        for pb in placed {
            if pb.beat.rest {
                continue;
            }
            if pb.beat.notes.iter().any(|n| n.effects.trill.is_some()) {
                self.trill_mark(pb.x - 0.4 * s, pb.x + pb.w - 0.6 * s, tt - 1.6 * s);
            } else if pb.beat.notes.iter().any(|n| n.effects.wide_vibrato) {
                g::wavy(self.p(), pb.x - 0.4 * s, pb.x + pb.w / 2.0, tt - 1.6 * s, 0.5 * s, Stroke::new(2.0, t.effect_ink));
            } else if pb.beat.notes.iter().any(|n| n.effects.vibrato) {
                g::wavy(self.p(), pb.x - 0.4 * s, pb.x + pb.w / 2.0, tt - 1.6 * s, 0.25 * s, Stroke::new(1.2, t.effect_ink));
            }
            // Beat techniques on their own lane: tapping, slap/pop, pick strokes, fades.
            let mut marks: Vec<String> = Vec::new();
            if pb.beat.notes.iter().any(|n| n.effects.tapped) {
                marks.push("T".into());
            }
            if pb.beat.notes.iter().any(|n| n.effects.left_hand_tapped) {
                marks.push("LH T".into());
            }
            match pb.beat.technique {
                Some(BeatTechnique::Slap) => marks.push("S".into()),
                Some(BeatTechnique::Pop) => marks.push("P".into()),
                None => {}
            }
            let lane = tt - 5.9 * s;
            let mut x = pb.x;
            for mark in marks {
                let galley = self.p().layout_no_wrap(mark, font.clone(), t.effect_ink);
                let w = galley.size().x;
                self.p().galley(pos2(x - w / 2.0, lane - galley.size().y / 2.0), galley, t.effect_ink);
                x += w + 0.4 * s;
            }
            if let Some(dir) = pb.beat.stroke {
                // Above the notation staff (or the tab when notation is hidden), as Guitar Pro does.
                let y = if m.notation { tt - (m.tab_top() - m.notation_top()) - 1.4 * s } else { lane - 1.2 * s };
                self.pen.glyph_centered(if dir == StrokeDirection::Down { g::STROKE_DOWN } else { g::STROKE_UP }, pos2(pb.x, y), t.effect_ink);
            }
            if let Some(fade) = pb.beat.fade {
                let (x0, x1, y) = (pb.x - 0.5 * s, pb.x + (pb.w / 2.0).max(1.5 * s), lane + 1.2 * s);
                let h = 0.45 * s;
                let st = Stroke::new(1.0, t.effect_ink);
                let wedge = |a: f32, b: f32, opening: bool| {
                    let (tip, mouth) = if opening { (a, b) } else { (b, a) };
                    self.p().line_segment([pos2(tip, y), pos2(mouth, y - h)], st);
                    self.p().line_segment([pos2(tip, y), pos2(mouth, y + h)], st);
                };
                match fade {
                    Fade::In => wedge(x0, x1, true),
                    Fade::Out => wedge(x0, x1, false),
                    Fade::Swell => {
                        let mid = (x0 + x1) / 2.0;
                        wedge(x0, mid, true);
                        wedge(mid, x1, false);
                    }
                }
            }
        }
    }

    /// Rhythm stems and beams under the tab when standard notation is hidden.
    fn tab_rhythm(&self, sys: &SystemBox, x0: f32, tb: f32) {
        let s = self.m.space;
        let ink = self.t.ink;
        let stem = Stroke::new((0.12 * s).max(1.0), ink);
        let (y0, y1) = (tb + 1.2 * s, tb + 4.2 * s);
        for mb in &sys.measures {
            let mx = x0 + sys.prefix + mb.x;
            for vc in &mb.voices {
                let beats = self.track.measures.get(mb.index).and_then(|x| x.voices.get(vc.voice)).map(|v| v.beats.as_slice()).unwrap_or_default();
                let groups = self.score.headers.get(mb.index).map(|h| beam_groups(beats, h)).unwrap_or_default();
                let x_of = |bi: usize| vc.beats.get(bi).map(|cx| mx + cx);
                for (bi, beat) in beats.iter().enumerate() {
                    let Some(x) = x_of(bi) else { continue };
                    let div = beat.duration.value.divisor();
                    if beat.rest {
                        self.pen.glyph_centered(g::rest_for(div), pos2(x, (y0 + y1) / 2.0), ink);
                        continue;
                    }
                    if let Some(speed) = beat.tremolo {
                        // On the stem, or just under the tab for a whole note.
                        let y = if div == 1 {
                            y0 + 0.4 * s
                        } else if div == 2 {
                            (y0 + y1) / 2.0 + 0.9 * s
                        } else {
                            (y0 + y1) / 2.0
                        };
                        self.slashes(x, y, speed.slashes());
                    }
                    match div {
                        1 => {}
                        2 => {
                            self.p().line_segment([pos2(x, (y0 + y1) / 2.0), pos2(x, y1)], stem);
                        }
                        _ => {
                            self.p().line_segment([pos2(x, y0), pos2(x, y1)], stem);
                            if !groups.iter().any(|gr| gr.contains(&bi))
                                && let Some(f) = g::flag_for(div, false)
                            {
                                self.pen.glyph(f, pos2(x - stem.width / 2.0, y1), ink);
                            }
                        }
                    }
                    for d in 0..beat.duration.dots {
                        self.p().circle_filled(pos2(x + (0.6 + f32::from(d) * 0.5) * s, y1 - 0.5 * s), 0.2 * s, ink);
                    }
                }
                // Beams: one per level an eighth, a 16th… needs, with stubs for lone shorter notes.
                let beam_t = 0.45 * s;
                for gr in &groups {
                    for level in 0..4u32 {
                        let need = 8u32 << level;
                        let y = y1 - level as f32 * (beam_t + 0.25 * s);
                        let members: Vec<(f32, bool)> = gr.iter().filter_map(|&i| Some((x_of(i)?, beats.get(i)?.duration.value.divisor() >= need))).collect();
                        let mut k = 0;
                        while k < members.len() {
                            if !members.get(k).is_some_and(|m| m.1) {
                                k += 1;
                                continue;
                            }
                            let mut j = k;
                            while members.get(j + 1).is_some_and(|m| m.1) {
                                j += 1;
                            }
                            let (Some(a), Some(b)) = (members.get(k), members.get(j)) else { break };
                            let (xa, xb) = if j == k { (a.0, a.0 + if k + 1 < members.len() { s } else { -s }) } else { (a.0, b.0) };
                            self.p().rect_filled(Rect::from_min_max(pos2(xa.min(xb), y - beam_t), pos2(xa.max(xb) + stem.width / 2.0, y)), 0.0, ink);
                            k = j + 1;
                        }
                    }
                }
            }
        }
    }
}

/// The name at the left of each system after the first: the track's short name, or a Guitar
/// Pro-style abbreviation of its name (`od.guit.`, `bass` …).
/// Segno and coda marks at Guitar Pro's size, about two spaces tall.
const DIRECTION_SCALE: f32 = 0.72;

pub fn short_name(track: &Track) -> String {
    if !track.short_name.trim().is_empty() {
        return track.short_name.trim().to_string();
    }
    if track.is_drums() {
        return "drms".to_string();
    }
    let lower = track.name.to_lowercase();
    let by_program = match track.program {
        24 => Some("n-guit."),
        25 => Some("s-guit."),
        26 => Some("jz.guit."),
        27 => Some("cl.guit."),
        28 => Some("mu.guit."),
        29 => Some("od.guit."),
        30 => Some("dist.guit."),
        32..=39 => Some("bass"),
        _ => None,
    };
    match by_program {
        Some(s) if lower.contains("guitar") || lower.contains("bass") || lower.starts_with("track") => s.to_string(),
        _ => track.name.chars().take(8).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_theory::Duration;

    /// Curves painted for `score`: ties, slurs and hammer-on arcs are all quadratic Béziers.
    fn curves(score: &Score, notation: bool) -> usize {
        painted(score, notation, |s| matches!(s, egui::Shape::QuadraticBezier(_)))
    }

    /// How many painted shapes `is` accepts when `score` is drawn at 100 %.
    fn painted(score: &Score, notation: bool, is: impl Fn(&egui::Shape) -> bool) -> usize {
        painted_in(score, notation, LayoutMode::Page, false, is)
    }

    /// The same in a layout mode, with every track or the cursor's alone.
    fn painted_in(score: &Score, notation: bool, layout: LayoutMode, multitrack: bool, is: impl Fn(&egui::Shape) -> bool) -> usize {
        fn count(shape: &egui::Shape, is: &dyn Fn(&egui::Shape) -> bool) -> usize {
            match shape {
                egui::Shape::Vec(inner) => inner.iter().map(|s| count(s, is)).sum(),
                other => usize::from(is(other)),
            }
        }
        // Without notation every track shows its tablature alone.
        let mut score = score.clone();
        for t in score.tracks.iter_mut().filter(|_| !notation) {
            t.notation.standard = false;
        }
        let mut fonts = None;
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(1400.0, 800.0)).build_ui(move |ui| {
            // Fonts take effect on the frame after they are installed.
            let Some(fonts) = fonts else {
                fonts = Some(crate::fonts::install(ui.ctx()));
                return;
            };
            show(ui, &score, Cursor::default(), &View { zoom: 1.0, fonts, play: None, selection: None, layout, multitrack, multivoice: false, hidden: 0 });
        });
        harness.run_steps(3);
        harness.output().shapes.iter().map(|s| count(&s.shape, &is)).sum()
    }

    fn lines(score: &Score, notation: bool) -> usize {
        painted(score, notation, |s| matches!(s, egui::Shape::LineSegment { .. }))
    }

    #[test]
    fn a_track_can_show_standard_notation_alone() {
        let both = Score::new();
        let mut standard = Score::new();
        standard.tracks[0].notation.tab = false;
        // One empty system: the six tab lines, the tab's bar lines and the bracket go.
        let (with_tab, without) = (lines(&both, true), lines(&standard, true));
        assert!(without + 6 <= with_tab, "{without} vs {with_tab}");
        assert!(without > 5, "the five staff lines stay");
        // Neither chosen still shows the tablature, as the engine never allows.
        let mut none = Score::new();
        none.tracks[0].notation = guitario_score::Notations { standard: false, tab: false, slash: true, numbered: false };
        assert_eq!(lines(&none, true), lines(&both, false));
    }

    /// Whether a SMuFL music font is installed. Without one, symbols are painted as shapes, so
    /// tests that count them as glyphs only run where it is (CI has none; the layout behind
    /// them is checked font-free in layout::tests).
    fn music_font() -> bool {
        let found = crate::fonts::definitions().1.smufl;
        if !found {
            eprintln!("no music font: glyph counts skipped");
        }
        found
    }

    fn texts(score: &Score, notation: bool, what: &str) -> usize {
        painted(score, notation, |s| matches!(s, egui::Shape::Text(t) if t.galley.text() == what))
    }

    /// A bar of one note of the given length, with `edit` applied to its beat.
    fn one_note(value: guitario_theory::NoteValue, edit: impl FnOnce(&mut Beat)) -> Score {
        let mut s = Score::new();
        let mut beat = Beat { rest: false, notes: vec![Note::new(0, 5)], ..Beat::rest(Duration::new(value)) };
        edit(&mut beat);
        let rest = Beat::rest(Duration::new(guitario_theory::NoteValue::Half));
        s.tracks[0].measures[0].voices[0].beats = vec![beat, rest];
        s
    }

    #[test]
    fn hidden_tracks_leave_the_multitrack_view_but_never_the_cursor_track() {
        assert_eq!(shown_tracks(3, 1, false, 0), vec![1]);
        assert_eq!(shown_tracks(3, 1, true, 0), vec![0, 1, 2]);
        assert_eq!(shown_tracks(3, 1, true, 0b111), vec![1], "the cursor's track stays");
        assert_eq!(shown_tracks(3, 0, true, 0b100), vec![0, 1]);
        assert!(!is_hidden(u64::MAX, 64) && !is_hidden(u64::MAX, usize::MAX) && is_hidden(u64::MAX, 63));
    }

    #[test]
    fn the_multitrack_view_draws_every_track_and_the_layouts_draw_every_bar() {
        let mut s = Score::new();
        let note = |string: u8, fret: u8| Beat { rest: false, notes: vec![Note::new(string, fret)], ..Beat::rest(Duration::WHOLE) };
        s.tracks[0].measures[0].voices[0].beats = vec![note(0, 17)];
        s.tracks.push(guitario_score::Track::new("Bass", guitario_theory::Tuning::bass(), &s.headers));
        s.tracks[1].measures[0].voices[0].beats = vec![note(1, 19)];
        let frets = |multi: bool, layout: LayoutMode| {
            let count = |what: &'static str| painted_in(&s, true, layout, multi, move |sh| matches!(sh, egui::Shape::Text(t) if t.galley.text() == what));
            (count("17"), count("19"))
        };
        assert_eq!(frets(false, LayoutMode::Page), (1, 0), "the cursor's track alone");
        for layout in [LayoutMode::Page, LayoutMode::Screen, LayoutMode::Horizontal] {
            assert_eq!(frets(true, layout), (1, 1), "{layout:?}");
        }
        // The track names stand left of each part.
        let names =
            painted_in(&s, true, LayoutMode::Page, true, |sh| matches!(sh, egui::Shape::Text(t) if t.galley.text() == "bass" || t.galley.text() == "s-guit."));
        assert_eq!(names, 2);
    }

    #[test]
    fn beams_slope_with_the_melody_but_lie_flat_over_a_peak() {
        use guitario_theory::NoteValue;
        // The beam's corners: each beam is a four-point polygon.
        let beams = |s: &Score| {
            let mut out = Vec::new();
            let score = s.clone();
            let mut fonts = None;
            let mut harness = egui_kittest::Harness::builder().with_size(vec2(1400.0, 800.0)).build_ui(move |ui| {
                let Some(fonts) = fonts else {
                    fonts = Some(crate::fonts::install(ui.ctx()));
                    return;
                };
                show(
                    ui,
                    &score,
                    Cursor::default(),
                    &View { zoom: 1.0, fonts, play: None, selection: None, layout: LayoutMode::Page, multitrack: false, multivoice: false, hidden: 0 },
                );
            });
            harness.run_steps(3);
            fn walk(shape: &egui::Shape, out: &mut Vec<Vec<egui::Pos2>>) {
                match shape {
                    egui::Shape::Vec(inner) => inner.iter().for_each(|s| walk(s, out)),
                    egui::Shape::Path(p) if p.closed && p.points.len() == 4 => out.push(p.points.clone()),
                    _ => {}
                }
            }
            for c in &harness.output().shapes {
                walk(&c.shape, &mut out);
            }
            out
        };
        let run = |frets: &[u8]| {
            let mut s = Score::new();
            let e = Duration::new(NoteValue::Eighth);
            let mut bs: Vec<Beat> = frets.iter().map(|f| Beat { rest: false, notes: vec![Note::new(2, *f)], ..Beat::rest(e) }).collect();
            bs.push(Beat::rest(Duration::new(NoteValue::Half)));
            s.tracks[0].measures[0].voices[0].beats = bs;
            s
        };
        // A rising pair: the beam's right end is higher (smaller y) than its left.
        let rising = beams(&run(&[2, 9]));
        let b = rising.first().expect("a beam");
        assert!(b[1].y < b[0].y - 0.5, "{b:?}");
        // Rising a long way: the slope is limited to a staff space.
        let steep = beams(&run(&[0, 12]));
        let b = steep.first().expect("a beam");
        assert!(b[0].y - b[1].y <= 1.5875 * 96.0 / 25.4 + 0.5, "{b:?}");
        // Equal ends: flat.
        let flat = beams(&run(&[5, 5]));
        let b = flat.first().expect("a beam");
        assert!((b[1].y - b[0].y).abs() < 0.01, "{b:?}");
    }

    #[test]
    fn fingering_is_drawn_in_notation_only() {
        use guitario_score::Finger;
        let fingered = one_note(guitario_theory::NoteValue::Quarter, |b| {
            b.notes[0].effects.left_finger = Some(Finger::Ring);
            b.notes[0].effects.right_finger = Some(Finger::Index);
        });
        let plain = one_note(guitario_theory::NoteValue::Quarter, |_| {});
        assert_eq!(texts(&fingered, true, "3"), texts(&plain, true, "3") + 1);
        assert_eq!(texts(&fingered, true, "i"), texts(&plain, true, "i") + 1);
        assert_eq!(texts(&fingered, false, "i"), texts(&plain, false, "i"), "not in the tab");
        let thumb = one_note(guitario_theory::NoteValue::Quarter, |b| b.notes[0].effects.left_finger = Some(Finger::Thumb));
        assert_eq!(texts(&thumb, true, "T"), texts(&plain, true, "T") + 1);
    }

    #[test]
    fn multivoice_editing_colours_each_voice_on_screen_and_never_on_paper() {
        let mut s = Score::new();
        let quarter = |fret: u8, string: u8| Beat { rest: false, notes: vec![Note::new(string, fret)], ..Beat::rest(Duration::QUARTER) };
        s.tracks[0].measures[0].voices[0].beats = vec![quarter(1, 0), quarter(2, 0), quarter(3, 0), quarter(4, 0)];
        s.tracks[0].measures[0].voices.push(guitario_score::Voice { beats: vec![quarter(7, 5), quarter(8, 5), quarter(9, 5), quarter(10, 5)] });
        fn walk(shape: &egui::Shape, color: Color32) -> usize {
            match shape {
                egui::Shape::Vec(inner) => inner.iter().map(|s| walk(s, color)).sum(),
                egui::Shape::Text(t) => usize::from(t.galley.job.sections.iter().any(|sec| sec.format.color == color)),
                _ => 0,
            }
        }
        // Paints the page view (or, with `paper`, the first printed page) and counts frets in `color`.
        let inked = |multivoice: bool, paper: bool, color: Color32| {
            let mut score = s.clone();
            score.tracks[0].notation.standard = false;
            let mut fonts = None;
            let mut harness = egui_kittest::Harness::builder().with_size(vec2(1400.0, 800.0)).build_ui(move |ui| {
                let Some(fonts) = fonts else {
                    fonts = Some(crate::fonts::install(ui.ctx()));
                    return;
                };
                if paper {
                    if let Some(pl) = print_layout(&score, 0, false) {
                        paint_print_page(ui.painter(), &score, &pl, 0, (1, 1), fonts);
                    }
                } else {
                    let view = View { zoom: 1.0, fonts, play: None, selection: None, layout: LayoutMode::Page, multitrack: false, multivoice, hidden: 0 };
                    show(ui, &score, Cursor::default(), &view);
                }
            });
            harness.run_steps(3);
            harness.output().shapes.iter().map(|s| walk(&s.shape, color)).sum::<usize>()
        };
        let [one, two, ..] = tokens().voice_ink;
        assert_eq!((inked(false, false, one), inked(false, false, two)), (0, 0), "black without it");
        assert_eq!((inked(true, false, one), inked(true, false, two)), (4, 4), "four frets in each voice's colour");
        assert_eq!(inked(true, true, one), 0, "paper is always black");
        assert!(inked(true, true, tokens().ink) >= 8);
    }

    #[test]
    fn a_click_picks_the_voice_with_a_note_on_the_clicked_string() {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(20.0, 100.0));
        let hit = |voice: usize, edit: bool, strings: Vec<u8>| Hit { rect, measure: 0, beat: voice, voice, edit, strings, tab_top: 0.0 };
        let hits = vec![hit(0, true, vec![0]), hit(1, false, vec![5])];
        let at = pos2(10.0, 50.0);
        assert_eq!(pick(&hits, at, Some(5)).map(|h| h.voice), Some(1));
        assert_eq!(pick(&hits, at, Some(0)).map(|h| h.voice), Some(0));
        // No note on the string, or a click in notation: the voice being edited.
        assert_eq!(pick(&hits, at, Some(3)).map(|h| h.voice), Some(0));
        assert_eq!(pick(&hits, at, None).map(|h| h.voice), Some(0));
        assert!(pick(&hits, pos2(50.0, 50.0), None).is_none());
    }

    #[test]
    fn a_key_change_inside_a_system_draws_its_naturals_and_new_key() {
        if !music_font() {
            return;
        }
        let mut s = Score::new();
        for _ in 0..3 {
            let h = guitario_score::MeasureHeader::new(Default::default());
            s.tracks[0].measures.push(guitario_score::Measure::empty_for(&h));
            s.headers.push(h);
        }
        for h in &mut s.headers {
            h.key = 2;
        }
        let glyphs = |s: &Score, c: char| texts(s, true, &c.to_string());
        let (sharps, naturals) = (glyphs(&s, g::SHARP), glyphs(&s, g::NATURAL));
        // D major to F major in bar 3: two naturals, then one flat.
        s.headers[2].key = -1;
        s.headers[3].key = -1;
        assert_eq!(glyphs(&s, g::NATURAL), naturals + 2);
        assert_eq!(glyphs(&s, g::FLAT), 1);
        assert_eq!(glyphs(&s, g::SHARP), sharps, "the system still opens in D");
        // Tab alone shows no key.
        assert_eq!(texts(&s, false, &g::FLAT.to_string()), 0);
    }

    #[test]
    fn the_system_before_a_change_ends_with_courtesy_signatures() {
        if !music_font() {
            return;
        }
        let mut s = Score::new();
        // Enough bars for several systems; the key and the time change from bar 21.
        for _ in 0..39 {
            let h = guitario_score::MeasureHeader::new(Default::default());
            s.tracks[0].measures.push(guitario_score::Measure::empty_for(&h));
            s.headers.push(h);
        }
        let flats = |s: &Score| texts(s, true, &g::FLAT.to_string());
        let sevens = |s: &Score| texts(s, true, &g::time_digit(7).to_string());
        let m = metrics_for(&s, &s.tracks[0], 1.0, true, 0);
        let opener = crate::layout::layout_parts(&s, &[Part { index: 0, track: &s.tracks[0], metrics: m }], &m)
            .into_iter()
            .flat_map(|p| p.systems)
            .nth(1)
            .and_then(|sys| sys.measures.first().map(|b| b.index))
            .unwrap();
        for h in s.headers.iter_mut().skip(opener) {
            h.key = -2;
            h.time_signature = guitario_theory::TimeSignature::new(7, 8).unwrap();
        }
        // The time signature shows twice: as a courtesy after the last bar line of the system
        // before, and at the change; over the staff only, as Guitar Pro leaves it out of the tab
        // under standard notation. Tablature alone shows it.
        assert_eq!(sevens(&s), 2);
        assert_eq!(texts(&s, false, &g::time_digit(7).to_string()), 2);
        // Every system after the change opens with two flats; the first one, still in C, ends
        // with them as a courtesy: as many flats as when the whole piece is in the new key.
        let mut all_flat = s.clone();
        for h in &mut all_flat.headers {
            h.key = -2;
        }
        assert_eq!(flats(&s), flats(&all_flat));
        assert!(flats(&s) >= 4);
        // Free time has no time signature to warn of.
        for h in s.headers.iter_mut().skip(opener) {
            h.free_time = true;
        }
        assert_eq!(sevens(&s), 0);
    }

    #[test]
    fn a_stem_down_voice_steps_aside_from_a_second_or_a_different_head() {
        let black = (4, 0);
        // A second between the voices: the lower voice moves right.
        assert!(heads_clash(&[3], black, &[2], black));
        // The same pitch with the same head is shared; with a half note against a quarter, or a
        // dotted note against a plain one, it is not.
        assert!(!heads_clash(&[3], black, &[3], black));
        assert!(heads_clash(&[3], (2, 0), &[3], black));
        assert!(heads_clash(&[3], black, &[3], (4, 1)));
        // A third or more apart, or nothing sounding, leaves them be.
        assert!(!heads_clash(&[5, 7], black, &[3, 1], black));
        assert!(!heads_clash(&[], black, &[3], black) && !heads_clash(&[3], black, &[], black));
        // Within a chord, any one second is enough.
        assert!(heads_clash(&[6, 9], black, &[2, 5], black));
    }

    #[test]
    fn a_tempo_ramp_is_marked_accel_or_rit_where_it_starts() {
        let mut s = Score::new();
        for _ in 0..3 {
            let h = guitario_score::MeasureHeader::new(Default::default());
            s.tracks[0].measures.push(guitario_score::Measure::empty_for(&h));
            s.headers.push(h);
        }
        assert_eq!(texts(&s, true, "accel."), 0);
        // Speeding up over bars 2-3 into 180 at bar 4: one "accel." where the glide starts.
        s.headers[1].tempo = Some(100);
        s.headers[3].tempo = Some(180);
        s.headers[3].tempo_linear = true;
        assert_eq!(texts(&s, true, "accel."), 1);
        assert_eq!(texts(&s, true, "rit."), 0);
        s.headers[3].tempo = Some(60);
        assert_eq!((texts(&s, true, "rit."), texts(&s, true, "accel.")), (1, 0));
        // A jump draws neither.
        s.headers[3].tempo_linear = false;
        assert_eq!(texts(&s, true, "rit."), 0);
    }

    #[test]
    fn a_fermata_is_drawn_over_the_beat_it_holds() {
        use guitario_score::{Fermata, FermataKind};
        let note = |fret: u8| Beat { rest: false, notes: vec![Note::new(0, fret)], ..Beat::rest(Duration::QUARTER) };
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![note(1), note(2), note(3), note(4)];
        // Any shape: with the music font a fermata is a glyph, without it a dot and an arc.
        let dots = |s: &Score| painted(s, true, |_| true);
        let plain = dots(&s);
        s.headers[0].fermatas = vec![
            Fermata { tick: 0, kind: FermataKind::Short, length: 0 },
            Fermata { tick: guitario_theory::TICKS_PER_QUARTER, kind: FermataKind::Medium, length: 0 },
            Fermata { tick: 3 * guitario_theory::TICKS_PER_QUARTER, kind: FermataKind::Long, length: 0 },
        ];
        // Each fermata draws something; with no beat at a fermata's place nothing is drawn.
        let held = dots(&s);
        assert!(held >= plain + 3, "{held} shapes against {plain}");
        s.headers[0].fermatas.push(Fermata { tick: 7, kind: FermataKind::Long, length: 0 });
        assert_eq!(dots(&s), held);
    }

    #[test]
    fn lyrics_are_drawn_one_syllable_per_beat_with_hyphens() {
        use guitario_score::LyricLine;
        let mut s = Score::new();
        let note = |fret: u8| Beat { rest: false, notes: vec![Note::new(0, fret)], ..Beat::rest(Duration::QUARTER) };
        s.tracks[0].measures[0].voices[0].beats = vec![note(1), note(2), Beat::rest(Duration::QUARTER), note(3)];
        s.tracks[0].lyrics = vec![LyricLine { measure: 0, text: "[Verse] Steel ho-rizon".into() }];
        for notation in [true, false] {
            assert_eq!(texts(&s, notation, "Steel"), 1);
            assert_eq!(texts(&s, notation, "ho"), 1);
            assert_eq!(texts(&s, notation, "rizon"), 1, "the rest is skipped and the last note sings");
            assert_eq!(texts(&s, notation, "-"), 1);
            assert_eq!(texts(&s, notation, "Verse"), 0, "a comment is not sung");
        }
    }

    #[test]
    fn a_chord_diagram_draws_its_grid_dots_and_marks() {
        use guitario_score::Chord;
        let circles = |s: &Score| painted(s, true, |sh| matches!(sh, egui::Shape::Circle(_)));
        let named = one_note(guitario_theory::NoteValue::Quarter, |b| b.chord = Some(Chord::named("A5")));
        // A5: x 0 2 2 x x from the low string; three dots would be a barre, two are not.
        let a5 = one_note(guitario_theory::NoteValue::Quarter, |b| {
            b.chord = Some(Chord { name: "A5".into(), frets: vec![None, None, Some(2), Some(2), Some(0), None], first_fret: 1, ..Chord::default() })
        });
        // Each diagram is drawn twice: over its beat and in the chord list under the title.
        assert_eq!(texts(&a5, true, "A5"), 2);
        // Six strings and five fret lines plus a nut, three x marks of two strokes each.
        assert_eq!(lines(&a5, true), lines(&named, true) + 2 * (6 + 5 + 1 + 3 * 2));
        // Two dots and one open-string ring.
        assert_eq!(circles(&a5), circles(&named) + 2 * 3);
        // Up the neck: the first fret is written and there is no nut; a barre joins its strings.
        let barre = one_note(guitario_theory::NoteValue::Quarter, |b| {
            b.chord = Some(Chord { name: "D".into(), frets: vec![Some(5), Some(7), Some(7), Some(7), Some(5), None], first_fret: 5, ..Chord::default() })
        });
        assert_eq!(texts(&barre, true, "5fr"), 2);
        assert_eq!(lines(&barre, true), lines(&named, true) + 2 * (6 + 5 + 2 + 1), "grid, one x mark, the barre");
        // An open D is fingered: no barre although fret 2 is held on both sides of the third fret.
        let open_d = one_note(guitario_theory::NoteValue::Quarter, |b| {
            b.chord = Some(Chord { name: "D".into(), frets: vec![Some(2), Some(3), Some(2), Some(0), None, None], first_fret: 1, ..Chord::default() })
        });
        assert_eq!(lines(&open_d, true), lines(&named, true) + 2 * (6 + 5 + 1 + 2 * 2), "grid, nut, two x marks");
    }

    #[test]
    fn the_chord_list_names_each_distinct_diagram_once_under_the_title() {
        use guitario_score::Chord;
        let e5 = Chord { name: "E5".into(), frets: vec![None, None, None, Some(2), Some(2), Some(0)], first_fret: 1, ..Chord::default() };
        let a5 = Chord { name: "A5".into(), frets: vec![None, None, Some(2), Some(2), Some(0), None], first_fret: 1, ..Chord::default() };
        let note = |fret: u8, chord: &Chord| Beat { rest: false, notes: vec![Note::new(5, fret)], chord: Some(chord.clone()), ..Beat::rest(Duration::QUARTER) };
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![note(0, &e5), note(5, &a5), note(0, &e5), note(5, &a5)];
        // Twice over the beats and once in the list.
        assert_eq!(texts(&s, true, "E5"), 3);
        assert_eq!(texts(&s, true, "A5"), 3);
        // A name with no diagram is not listed.
        s.tracks[0].measures[0].voices[0].beats = vec![Beat { chord: Some(Chord::named("E5")), ..note(0, &e5) }];
        assert_eq!(texts(&s, true, "E5"), 1);
    }

    #[test]
    fn a_grace_note_is_a_small_fret_in_the_tab_and_a_small_slurred_note_in_notation() {
        use guitario_score::{Grace, GraceTransition};
        let plain = one_note(guitario_theory::NoteValue::Quarter, |_| {});
        let graced = |g: Grace| one_note(guitario_theory::NoteValue::Quarter, |b| b.notes[0].effects.grace = Some(g));
        // Tab only: the small fret, and a slur for a hammered grace note.
        let before = graced(Grace::new(3));
        assert_eq!(texts(&before, false, "3"), texts(&plain, false, "3") + 1);
        assert_eq!(curves(&before, false), curves(&plain, false));
        let hammered = graced(Grace { transition: GraceTransition::Hammer, ..Grace::new(3) });
        assert_eq!(curves(&hammered, false), curves(&plain, false) + 1);
        let slid = graced(Grace { transition: GraceTransition::Slide, ..Grace::new(3) });
        assert_eq!(lines(&slid, false), lines(&plain, false) + 1);
        let dead = graced(Grace { dead: true, ..Grace::new(3) });
        assert_eq!(texts(&dead, false, "X"), texts(&plain, false, "X") + 1);
        // Notation: a stem (and a flag, drawn as a line without a music font), slashed before the
        // beat but not on it, and a slur into the note.
        let on_beat = lines(&graced(Grace { on_beat: true, ..Grace::new(3) }), true);
        assert!(on_beat > lines(&plain, true));
        assert_eq!(lines(&before, true), on_beat + 1, "the slash");
        assert_eq!(curves(&before, true), curves(&plain, true) + 1);
        // A tied note never shows one.
        let tied = one_note(guitario_theory::NoteValue::Quarter, |b| {
            b.notes[0].kind = NoteKind::Tie;
            b.notes[0].effects.grace = Some(Grace::new(3));
        });
        assert_eq!(texts(&tied, false, "3"), texts(&plain, false, "3"));
    }

    #[test]
    fn the_stylesheet_can_show_the_pitch_a_bend_reaches_in_notation() {
        use guitario_score::{Bend, BendKind};
        let bent = |q: u8, kind: NoteKind| {
            one_note(guitario_theory::NoteValue::Quarter, |b| {
                b.notes[0].effects.bend = Some(Bend::to(q));
                b.notes[0].kind = kind;
            })
        };
        let shown = |mut s: Score| {
            s.page.bend_pitch = true;
            s
        };
        // Off (Guitar Pro's default): notation is as before.
        let full = bent(4, NoteKind::Normal);
        let before = curves(&full, true);
        // On: one slur more, to the small head of the target.
        assert_eq!(curves(&shown(full), true), before + 1);
        // A quarter-tone bend shows too; a tied note (a hold) does not.
        assert_eq!(curves(&shown(bent(1, NoteKind::Normal)), true), curves(&bent(1, NoteKind::Normal), true) + 1);
        let held = one_note(guitario_theory::NoteValue::Quarter, |b| {
            b.notes[0].effects.bend = Some(Bend::preset(BendKind::Hold, 4));
            b.notes[0].kind = NoteKind::Tie;
        });
        assert_eq!(curves(&shown(held.clone()), true), curves(&held, true));
        // Tab alone shows no target.
        assert_eq!(curves(&shown(bent(4, NoteKind::Normal)), false), curves(&bent(4, NoteKind::Normal), false));
    }

    #[test]
    fn tremolo_picking_draws_one_slash_per_speed_on_the_stem_in_notation_and_tab_only() {
        use guitario_score::TremoloSpeed;
        for value in
            [guitario_theory::NoteValue::Quarter, guitario_theory::NoteValue::Half, guitario_theory::NoteValue::Eighth, guitario_theory::NoteValue::Whole]
        {
            let plain = one_note(value, |_| {});
            for notation in [true, false] {
                for speed in TremoloSpeed::ALL {
                    let picked = one_note(value, |b| b.tremolo = Some(speed));
                    assert_eq!(lines(&picked, notation), lines(&plain, notation) + usize::from(speed.slashes()), "{value:?} notation={notation} {speed:?}");
                }
            }
        }
    }

    #[test]
    fn a_trill_is_marked_tr_in_notation_and_in_the_tab_lane() {
        use guitario_score::{Trill, TrillSpeed};
        let plain = one_note(guitario_theory::NoteValue::Quarter, |_| {});
        let trilled = one_note(guitario_theory::NoteValue::Quarter, |b| {
            if let Some(n) = b.notes.first_mut() {
                n.effects.trill = Some(Trill { fret: 7, speed: TrillSpeed::Sixteenth });
            }
        });
        assert_eq!(texts(&plain, true, "tr"), 0);
        assert_eq!(texts(&trilled, true, "tr"), 2, "above the staff and over the tab");
        assert_eq!(texts(&trilled, false, "tr"), 1, "tab only");
    }

    /// A bar of two quarter notes on the top string and a half rest.
    fn two_notes(second: impl FnOnce(&mut Note), first: impl FnOnce(&mut Note)) -> Score {
        let mut s = Score::new();
        let (mut a, mut b) = (Note::new(0, 3), Note::new(0, 3));
        first(&mut a);
        second(&mut b);
        let beat = |n: Note| Beat { rest: false, notes: vec![n], ..Beat::rest(Duration::QUARTER) };
        let rest = Beat::rest(Duration::new(guitario_theory::NoteValue::Half));
        s.tracks[0].measures[0].voices[0].beats = vec![beat(a), beat(b), rest];
        s
    }

    #[test]
    fn a_tie_is_drawn_in_notation_and_in_tab() {
        let tied = two_notes(|n| n.kind = NoteKind::Tie, |_| {});
        let plain = two_notes(|_| {}, |_| {});
        assert_eq!(curves(&tied, false), curves(&plain, false) + 1, "tab only: one arc over the strings");
        assert_eq!(curves(&tied, true), curves(&plain, true) + 2, "with notation: one more over the staff");
    }

    #[test]
    fn a_tie_into_the_first_beat_has_an_incoming_arc() {
        let incoming = two_notes(|_| {}, |n| n.kind = NoteKind::Tie);
        let plain = two_notes(|_| {}, |_| {});
        assert_eq!(curves(&incoming, false), curves(&plain, false) + 1);
        assert_eq!(curves(&incoming, true), curves(&plain, true) + 2);
    }

    #[test]
    fn hammer_ons_and_legato_slides_are_slurred_in_notation() {
        let plain = two_notes(|n| n.fret = 5, |_| {});
        let hammer = two_notes(|n| n.fret = 5, |n| n.effects.hammer = true);
        let legato = two_notes(|n| n.fret = 5, |n| n.effects.slide = Some(Slide::Legato));
        for (name, score) in [("hammer-on", &hammer), ("legato slide", &legato)] {
            // The tab already arcs these; notation adds a slur of its own.
            assert_eq!(curves(score, false), curves(&plain, false) + 1, "{name}: the tab arc");
            assert_eq!(curves(score, true), curves(&plain, true) + 2, "{name}: and the notation slur");
        }
    }

    #[test]
    fn a_placed_slur_is_one_arc_in_notation_and_nothing_in_the_tab() {
        let plain = two_notes(|_| {}, |_| {});
        let mut slurred = plain.clone();
        if let Some(b) = slurred.tracks[0].measures[0].voices[0].beats.first_mut() {
            b.slur = Some(1);
        }
        assert_eq!(curves(&slurred, true), curves(&plain, true) + 1);
        assert_eq!(curves(&slurred, false), curves(&plain, false));
        // One that runs off the bar (a stale value after an edit) is skipped, not a panic.
        if let Some(b) = slurred.tracks[0].measures[0].voices[0].beats.first_mut() {
            b.slur = Some(30);
        }
        assert_eq!(curves(&slurred, true), curves(&plain, true));
    }
}
