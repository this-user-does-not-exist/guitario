//! The page view: white A4 pages on a grey workspace, each system a standard-notation staff over a
//! tablature staff joined by a bracket, laid out by `layout` and painted here. Clicks move the
//! cursor; the painter never edits the score directly.

use crate::fonts::{FontSet, sans, serif, serif_italic};
use crate::glyphs::{self as g, Pen};
use crate::layout::{LayoutMode, Metrics, PageBox, Part, SystemBox, layout, layout_parts};
use crate::notation::{Accidentals, Clef, beam_groups, spell, staff_pos};
use crate::theme::{Tokens, tokens};
use egui::{Color32, Painter, Rect, Stroke, pos2, vec2};
use guitario_engine::Cursor;
use guitario_score::{
    Beat, BeatTechnique, BrushKind, DirectionTarget, Fade, Jump, MeasureHeader, Note, NoteKind, PageSetup, Score, Slide, StrokeDirection, Track, TripletFeel,
};

const PAGE_GAP: f32 = 24.0;

type BeatTest = fn(&Beat) -> bool;

/// What a click on the page asks for.
pub enum Click {
    Beat { track: usize, measure: usize, beat: usize, string: Option<usize>, shift: bool },
}

/// An inclusive range of beats, `(measure, beat)` in score order.
pub type Range = ((usize, usize), (usize, usize));

pub struct View {
    pub zoom: f32,
    pub notation: bool,
    pub fonts: FontSet,
    /// The beat being played, shown as Guitar Pro's moving playback cursor.
    pub play: Option<(usize, usize)>,
    pub selection: Option<Range>,
    pub layout: LayoutMode,
    /// Every track, stacked in each system, instead of the cursor's track alone.
    pub multitrack: bool,
}

pub struct Output {
    pub click: Option<Click>,
    /// A drag across beats: from where it started to where the pointer is.
    pub drag: Option<Range>,
    pub cursor_rect: Option<Rect>,
    pub play_rect: Option<Rect>,
}

/// The layout metrics for `track`: a drum staff or notation over tab, with room for chord names.
fn metrics_for(track: &Track, page: &PageSetup, zoom: f32, notation: bool, voice: usize) -> Metrics {
    let m = if track.is_drums() { Metrics::for_drums(zoom) } else { Metrics::new(zoom, notation, track.string_count()) };
    m.with_page(page).with_annotations(track).with_voice(voice)
}

/// One track laid out for paper: the score's pages at 100 %.
pub struct PrintLayout {
    pub metrics: Metrics,
    pub pages: Vec<PageBox>,
}

pub fn print_layout(score: &Score, track_index: usize, notation: bool) -> Option<PrintLayout> {
    let track = score.tracks.get(track_index)?;
    let metrics = metrics_for(track, &score.page, 1.0, notation, 0);
    Some(PrintLayout { pages: layout(score, track_index, &metrics), metrics })
}

/// A document's pages: every chosen track laid out in order, numbered through the whole. The PDF,
/// the printer and the print preview all take their pages from one of these.
pub struct PrintSet {
    layouts: Vec<(usize, PrintLayout)>,
}

impl PrintSet {
    /// Lays out `tracks` (indexes, in print order). An error says what was asked that cannot be printed.
    pub fn new(score: &Score, tracks: &[usize], notation: bool) -> Result<PrintSet, String> {
        if tracks.is_empty() {
            return Err("no tracks to print".into());
        }
        let mut layouts = Vec::with_capacity(tracks.len());
        for &ti in tracks {
            let layout = print_layout(score, ti, notation).ok_or_else(|| format!("track {} does not exist", ti.saturating_add(1)))?;
            layouts.push((ti, layout));
        }
        Ok(PrintSet { layouts })
    }

    /// Pages in the whole document.
    pub fn pages(&self) -> usize {
        self.layouts.iter().map(|(_, l)| l.pages.len()).sum()
    }

    /// Page `n` (0-based): its track, that track's layout and the page's place in it.
    pub fn page(&self, n: usize) -> Option<(usize, &PrintLayout, usize)> {
        let mut before = 0;
        for (track, layout) in &self.layouts {
            let here = layout.pages.len();
            if n < before + here {
                return Some((*track, layout, n - before));
            }
            before += here;
        }
        None
    }

    /// Paints page `n` (0-based) with its top-left corner at the origin.
    pub fn paint(&self, painter: &Painter, score: &Score, n: usize, fonts: FontSet) {
        if let Some((track, layout, page)) = self.page(n) {
            paint_print_page(painter, score, track, layout, page, (n + 1, self.pages()), fonts);
        }
    }
}

/// Paints page `page` of a track's print layout with its top-left corner at the origin: the same
/// engraving the page view draws, without the cursor, selection or playback marks. `number` is
/// `(this page, pages in the document)` for the page number.
pub fn paint_print_page(painter: &Painter, score: &Score, track_index: usize, pl: &PrintLayout, page: usize, number: (usize, usize), fonts: FontSet) {
    let (Some(track), Some(boxed)) = (score.tracks.get(track_index), pl.pages.get(page)) else { return };
    let m = pl.metrics;
    let pen = Pen { painter, fonts, space: m.space };
    // A cursor on a bar that does not exist is never drawn.
    let cursor = Cursor { track: track_index, measure: usize::MAX, ..Cursor::default() };
    let mut cx = Ctx {
        pen,
        m,
        score,
        track,
        cursor,
        t: tokens(),
        hits: Vec::new(),
        cursor_rect: None,
        play: None,
        play_rect: None,
        selection: None,
        clef: Clef::for_tuning(track.tuning.strings.iter().copied().max().unwrap_or(64)),
        lyrics: sung(track),
    };
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(m.page_w, m.page_h));
    if page == 0 {
        cx.header(rect);
    }
    painter.text(
        rect.right_bottom() - vec2(m.margin_right, m.margin_bottom * 0.5),
        egui::Align2::RIGHT_CENTER,
        format!("{}/{}", number.0, number.1),
        serif(9.0 * m.zoom),
        Color32::from_gray(110),
    );
    cx.page(boxed, rect, page == 0);
}

pub fn show(ui: &mut egui::Ui, score: &Score, cursor: Cursor, view: &View) -> Output {
    let none = Output { click: None, drag: None, cursor_rect: None, play_rect: None };
    if score.tracks.get(cursor.track).is_none() {
        return none;
    }
    // The tracks drawn: the cursor's, or every track in the multitrack view (the top one carries
    // the bar's markers and directions).
    let indices: Vec<usize> = if view.multitrack { (0..score.tracks.len()).collect() } else { vec![cursor.track] };
    let window = (ui.available_width() - 2.0 * PAGE_GAP).max(1.0);
    let parts: Vec<Part> = indices
        .iter()
        .enumerate()
        .filter_map(|(k, &ti)| {
            let track = score.tracks.get(ti)?;
            let voice = if ti == cursor.track { cursor.voice } else { 0 };
            let m = metrics_for(track, &score.page, view.zoom, view.notation, voice).with_mode(view.layout, window);
            Some(Part { index: ti, track, metrics: if k == 0 { m } else { m.below() } })
        })
        .collect();
    let Some(page_m) = parts.first().map(|p| p.metrics) else { return none };
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
        let string = (!m.drums && pos.y >= h.tab_top - m.tab_gap / 2.0 && pos.y <= h.tab_top + m.tab_h() + m.tab_gap / 2.0)
            .then(|| (((pos.y - h.tab_top) / m.tab_gap).round().max(0.0) as usize).min(cx.track.string_count().saturating_sub(1)));
        let shift = ui.input(|i| i.modifiers.shift);
        click = Some(Click::Beat { track: cx.cursor.track, measure: h.measure, beat: h.beat, string, shift });
    }
    let mine = ctxs.iter().find(|c| c.cursor.track == cursor.track);
    let hit_at = |p: egui::Pos2| mine.and_then(|cx| cx.hits.iter().find(|h| h.rect.contains(p)).map(|h| (h.measure, h.beat)));
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
        let x0 = rect.left() + m.margin_left + m.indent();
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

struct Hit {
    rect: Rect,
    measure: usize,
    beat: usize,
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
    /// Each line of lyrics laid on its beats, `(measure, beat, syllable)`, worked out once per paint.
    lyrics: Vec<Vec<(usize, usize, guitario_score::Syllable)>>,
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

/// The track's lyrics laid on its beats, line by line.
fn sung(track: &Track) -> Vec<Vec<(usize, usize, guitario_score::Syllable)>> {
    track.lyrics.iter().map(|l| guitario_score::dispatch_lyrics(track, l)).collect()
}

impl<'a> Ctx<'a> {
    fn p(&self) -> &Painter {
        self.pen.painter
    }

    fn header(&self, page: Rect) {
        let m = &self.m;
        let meta = &self.score.meta;
        let mut y = page.top() + m.margin_top;
        let title = if meta.title.is_empty() { "Untitled" } else { meta.title.as_str() };
        let center = page.left() + m.center_x();
        self.p().text(pos2(center, y), egui::Align2::CENTER_TOP, title, serif(33.0 * m.zoom), self.t.ink);
        y += 10.0 * m.space;
        if !meta.artist.is_empty() {
            self.p().text(pos2(center, y), egui::Align2::CENTER_TOP, &meta.artist, serif(16.0 * m.zoom), self.t.ink);
        }
        if !meta.author.is_empty() {
            self.p().text(pos2(page.right() - m.margin_right, y), egui::Align2::RIGHT_TOP, format!("Tab: {}", meta.author), serif(10.0 * m.zoom), self.t.ink);
        }
        // Tuning legend for anything but standard tuning, as Guitar Pro does.
        let tuning = &self.track.tuning;
        let mut legend = Vec::new();
        if !self.track.is_drums() && tuning.strings != guitario_theory::Tuning::standard().strings {
            let names: Vec<&str> = tuning.strings.iter().rev().map(|p| guitario_theory::note_name(*p)).collect();
            legend.push(format!("Tuning: {}", names.join(" ")));
        }
        if self.track.capo > 0 {
            legend.push(format!("Capo fret {}", self.track.capo));
        }
        for (i, l) in legend.iter().enumerate() {
            self.p().text(pos2(page.left() + m.margin_left, y + i as f32 * 2.6 * m.space), egui::Align2::LEFT_TOP, l, serif(10.0 * m.zoom), self.t.ink);
        }
    }

    /// The top of the first staff and the bottom of the last of a system whose top is at `top`.
    fn staff_span(&self, top: f32) -> (f32, f32) {
        let m = &self.m;
        let first = if m.notation { top + m.notation_top() } else { top + m.tab_top() };
        let last = if m.drums { top + m.notation_top() + m.staff_h() } else { top + m.tab_top() + m.tab_h() };
        (first, last)
    }

    fn page(&mut self, page: &PageBox, rect: Rect, first_page: bool) {
        for (si, sys) in page.systems.iter().enumerate() {
            let top = rect.top() + sys.y;
            let sys_rect = Rect::from_min_size(pos2(rect.left(), top), vec2(self.m.page_w, self.m.system_h()));
            if !self.p().clip_rect().intersects(sys_rect) {
                continue;
            }
            self.system(sys, rect.left() + self.m.margin_left + self.m.indent(), top, first_page && si == 0);
        }
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
        let line = Stroke::new((0.13 * s).max(0.8), t.staff_line);
        let bar = Stroke::new((0.16 * s).max(1.0), t.bar_line);
        let strings = self.track.string_count();

        // Staves.
        if m.notation {
            for i in 0..5 {
                let y = nt + i as f32 * s;
                self.p().line_segment([pos2(x0, y), pos2(x1, y)], line);
            }
        }
        if !m.drums {
            for st in 0..strings {
                let y = self.tab_y(tt, st);
                self.p().line_segment([pos2(x0, y), pos2(x1, y)], line);
            }
        }
        // System line and the bracket joining notation and tab.
        let sys_top = if m.notation { nt } else { tt };
        self.p().line_segment([pos2(x0, sys_top), pos2(x0, if m.drums { nt + 4.0 * s } else { tb })], bar);
        if m.notation && !m.drums {
            let bx = x0 - 0.9 * s;
            self.p().line_segment([pos2(bx, nt - 0.3 * s), pos2(bx, tb + 0.3 * s)], Stroke::new(0.5 * s, t.ink));
            g::arc(self.p(), pos2(bx, nt - 0.5 * s), pos2(bx + 1.6 * s, nt - 1.0 * s), -0.2 * s, Stroke::new(0.25 * s, t.ink));
            g::arc(self.p(), pos2(bx, tb + 0.5 * s), pos2(bx + 1.6 * s, tb + 1.0 * s), 0.2 * s, Stroke::new(0.25 * s, t.ink));
        }

        // Short track name in the left margin.
        let short = short_name(self.track);
        let name_y = if m.drums {
            nt + 2.0 * s
        } else if m.notation {
            (nt + tb) / 2.0
        } else {
            (tt + tb) / 2.0
        };
        self.p().text(pos2(x0 - 1.8 * s, name_y), egui::Align2::RIGHT_CENTER, short, serif(9.0 * m.zoom), t.ink);

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
        let tab_clef = if strings <= 4 { g::TAB_CLEF_4 } else { g::TAB_CLEF_6 };
        if !m.drums {
            self.pen.glyph_scaled(tab_clef, pos2(x0 + 0.8 * s, (tt + tb) / 2.0), (m.tab_h() / (4.0 * s)).clamp(0.8, 1.8), t.ink);
        }

        // Bar number and the opening tempo mark, over the top track.
        if m.top {
            self.p().text(pos2(x0 + 0.2 * s, sys_top - 1.0 * s), egui::Align2::LEFT_BOTTOM, (first_mi + 1).to_string(), serif(8.0 * m.zoom), t.bar_number);
        }
        if first && m.top {
            self.tempo_mark(x0 + sys.prefix, sys_top - 3.6 * s, self.score.tempo);
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
            if mb.show_time && !h.free_time {
                self.time_signature(h, mx + 0.9 * s, nt, tt, tb);
            }
            if h.repeat_open {
                let lx = mx + if mb.show_time { 3.6 * s } else { 0.2 * s };
                self.repeat_sign(lx, nt, tt, tb, true);
            }
            if h.repeat_close > 0 {
                self.repeat_sign(end, nt, tt, tb, false);
            }
            if h.repeat_close > 0 && m.top {
                self.p().text(
                    pos2(end, sys_top - 0.6 * s),
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
                self.p().line_segment([pos2(end, tt), pos2(end, tb)], bar);
                if h.double_bar {
                    self.staves_lines(end - 0.6 * s, nt, tt, tb, bar);
                }
            }
            if m.top {
                self.bar_marks(h, mb.index, mx, end, sys_top);
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
                    if vc.voice == edit_voice {
                        if self.cursor.measure == mb.index && self.cursor.beat == bi {
                            self.p().rect_filled(col, 2.0, t.cursor_fill);
                            if !m.drums {
                                let cy = self.tab_y(tt, self.cursor.string.min(strings.saturating_sub(1)));
                                let r = Rect::from_center_size(pos2(x, cy), vec2((1.6 * m.tab_gap).min(w - 2.0).max(m.tab_gap), m.tab_gap));
                                self.p().rect_stroke(r, 2.0, Stroke::new(1.5, t.cursor_stroke), egui::StrokeKind::Outside);
                            }
                            self.cursor_rect = Some(col);
                        }
                        self.hits.push(Hit { rect: col, measure: mb.index, beat: bi, tab_top: tt });
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
            for (_, list) in &placed {
                for (i, pb) in list.iter().enumerate() {
                    self.tab_beat(pb, list.get(..i).unwrap_or_default(), list.get(i + 1..).unwrap_or_default(), tt);
                }
                self.effect_lanes(list, tt);
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
    fn bar_marks(&self, h: &MeasureHeader, index: usize, mx: f32, end: f32, sys_top: f32) {
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
                self.p().text(pos2(x_start + 0.4 * s, y + 0.2 * s), egui::Align2::LEFT_TOP, label.join(" "), serif(8.5 * m.zoom), t.ink);
            }
            if h.repeat_close > 0 {
                // Short, so the repeat count under it stays clear.
                self.p().line_segment([pos2(x_end, y), pos2(x_end, y + 0.8 * s)], st);
            }
        }
        // Directions: segno and coda marks at the start of the bar, Fine and jumps at its end.
        let dir_y = sys_top - 4.8 * s;
        let mut dx = mx + 0.6 * s;
        for target in &h.targets {
            match target {
                DirectionTarget::Segno => dx += self.pen.glyph(g::SEGNO, pos2(dx, dir_y), t.ink) + 0.3 * s,
                DirectionTarget::SegnoSegno => {
                    dx += self.pen.glyph(g::SEGNO, pos2(dx, dir_y), t.ink);
                    dx += self.pen.glyph(g::SEGNO, pos2(dx, dir_y), t.ink) + 0.3 * s;
                }
                DirectionTarget::Coda => dx += self.pen.glyph(g::CODA, pos2(dx, dir_y), t.ink) + 0.3 * s,
                DirectionTarget::DoubleCoda => dx += self.pen.glyph(g::CODA_SQUARE, pos2(dx, dir_y), t.ink) + 0.3 * s,
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
                x -= self.pen.width(glyph);
                self.pen.glyph(glyph, pos2(x, y), t.ink);
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
            self.p().text(pos2(mx + 0.5 * s, sys_top - 5.6 * s), egui::Align2::LEFT_BOTTOM, label, serif(8.5 * m.zoom), t.ink);
        }
        if let Some(text) = &h.marker {
            let galley = self.p().layout_no_wrap(text.clone(), serif(10.0 * m.zoom), t.ink);
            // At the top of the lane, above chord names and text.
            let r = Rect::from_min_size(pos2(mx + 0.4 * s, sys_top - m.lane_above() + 0.4 * s), galley.size() + vec2(0.8 * s, 0.4 * s));
            self.p().rect_stroke(r, 0.0, Stroke::new(1.0, t.ink), egui::StrokeKind::Inside);
            self.p().galley(r.min + vec2(0.4 * s, 0.2 * s), galley, t.ink);
        }
        if let Some(tempo) = h.tempo
            && index > 0
        {
            self.tempo_mark(mx + 0.5 * s, sys_top - 3.6 * s, tempo);
        }
    }

    fn tempo_mark(&self, x: f32, y: f32, bpm: u16) {
        let s = self.m.space;
        let w = self.pen.glyph_scaled(g::MET_QUARTER_UP, pos2(x, y), 0.8, self.t.ink);
        self.p().text(pos2(x + w + 0.3 * s, y), egui::Align2::LEFT_BOTTOM, format!("= {bpm}"), serif(11.0 * self.m.zoom), self.t.ink);
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
        if self.m.drums {
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
        if !self.m.drums {
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
        if !self.m.drums {
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
        for vc in &mb.voices {
            let Some(voice) = measure.voices.get(vc.voice) else { continue };
            // Where a tie into this bar starts: the last note on that string in the previous bar.
            let incoming = |string: u8| -> Option<f32> {
                let (pmb, pmx) = prev?;
                let pv = self.track.measures.get(pmb.index)?.voices.get(vc.voice)?;
                let cols = pmb.voices.iter().find(|c| c.voice == vc.voice)?;
                let j = pv.beats.iter().rposition(|b| !b.rest && b.note_on(string).is_some())?;
                Some(pmx + cols.beats.get(j)?)
            };
            self.notation_voice(h, &voice.beats, &vc.beats, (vc.voice, multi), (mx, nt), &incoming, &mut acc);
        }
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
        let ink = self.t.ink;
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
            let by_string = keyed.iter().map(|(st, p)| (*st, p.0)).collect();
            let mut pos: Vec<Head> = keyed.into_iter().map(|(_, p)| p).collect();
            pos.sort_by_key(|p| p.0);
            let avg = pos.iter().map(|p| p.0).sum::<i32>() as f32 / pos.len().max(1) as f32;
            // Drum parts are written stems up (one voice, hands and feet together).
            infos.push(Some(Info { x, by_string, pos, up: stem_up(avg), div: beat.duration.value.divisor(), graces, slashed, fingers }));
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

    fn tab_beat(&self, pb: &Placed, before: &[Placed], rest: &[Placed], tt: f32) {
        let (m, t) = (self.m, self.t);
        let s = m.space;
        if pb.beat.rest {
            return;
        }
        let font = sans(m.tab_gap * 1.02);
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
            let galley = self.p().layout_no_wrap(Self::fret_text(n), font.clone(), t.ink);
            let r = Rect::from_center_size(pos2(pb.x, y), vec2(galley.size().x + 2.0, m.tab_gap * 0.9));
            self.p().rect_filled(r, 0.0, t.page);
            self.p().galley(pos2(r.center().x - galley.size().x / 2.0, y - galley.size().y / 2.0), galley, t.ink);
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
            if let Some(q) = n.effects.bend {
                let a = pos2(pb.x + fw + 1.0, y);
                let top = tt - 2.2 * s;
                let b = pos2(pb.x + fw + 1.6 * s, top);
                self.p().add(egui::epaint::QuadraticBezierShape::from_points_stroke([a, pos2(b.x, a.y), b], false, Color32::TRANSPARENT, fx));
                self.p().add(egui::Shape::convex_polygon(
                    vec![b + vec2(0.0, -0.6 * s), b + vec2(-0.4 * s, 0.2 * s), b + vec2(0.4 * s, 0.2 * s)],
                    t.effect_ink,
                    Stroke::NONE,
                ));
                let label = match q {
                    1 => "¼".to_string(),
                    2 => "½".to_string(),
                    4 => "full".to_string(),
                    6 => "1½".to_string(),
                    8 => "2".to_string(),
                    q => format!("{:.2}", f32::from(q) / 4.0),
                };
                self.p().text(b - vec2(0.0, 0.8 * s), egui::Align2::CENTER_BOTTOM, label, serif(8.0 * m.zoom), t.effect_ink);
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

/// Guitar Pro-style abbreviated track name for the left margin (`od.guit.`, `bass` …).
pub fn short_name(track: &Track) -> String {
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
        let score = score.clone();
        let mut fonts = None;
        let mut harness = egui_kittest::Harness::builder().with_size(vec2(1400.0, 800.0)).build_ui(move |ui| {
            // Fonts take effect on the frame after they are installed.
            let Some(fonts) = fonts else {
                fonts = Some(crate::fonts::install(ui.ctx()));
                return;
            };
            show(ui, &score, Cursor::default(), &View { zoom: 1.0, notation, fonts, play: None, selection: None, layout, multitrack });
        });
        harness.run_steps(3);
        harness.output().shapes.iter().map(|s| count(&s.shape, &is)).sum()
    }

    fn lines(score: &Score, notation: bool) -> usize {
        painted(score, notation, |s| matches!(s, egui::Shape::LineSegment { .. }))
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
                    &View { zoom: 1.0, notation: true, fonts, play: None, selection: None, layout: LayoutMode::Page, multitrack: false },
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
    fn a_fermata_is_drawn_over_the_beat_it_holds() {
        use guitario_score::{Fermata, FermataKind};
        let note = |fret: u8| Beat { rest: false, notes: vec![Note::new(0, fret)], ..Beat::rest(Duration::QUARTER) };
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![note(1), note(2), note(3), note(4)];
        let dots = |s: &Score| painted(s, true, |sh| matches!(sh, egui::Shape::Circle(_)));
        let plain = dots(&s);
        s.headers[0].fermatas = vec![
            Fermata { tick: 0, kind: FermataKind::Short, length: 0 },
            Fermata { tick: guitario_theory::TICKS_PER_QUARTER, kind: FermataKind::Medium, length: 0 },
            Fermata { tick: 3 * guitario_theory::TICKS_PER_QUARTER, kind: FermataKind::Long, length: 0 },
        ];
        // Each fallback fermata has its dot; with no beat at a fermata's place nothing is drawn.
        assert_eq!(dots(&s), plain + 3);
        s.headers[0].fermatas.push(Fermata { tick: 7, kind: FermataKind::Long, length: 0 });
        assert_eq!(dots(&s), plain + 3);
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
            b.chord = Some(Chord { name: "A5".into(), frets: vec![None, None, Some(2), Some(2), Some(0), None], first_fret: 1 })
        });
        assert_eq!(texts(&a5, true, "A5"), 1);
        // Six strings and five fret lines plus a nut, three x marks of two strokes each.
        assert_eq!(lines(&a5, true), lines(&named, true) + 6 + 5 + 1 + 3 * 2);
        // Two dots and one open-string ring.
        assert_eq!(circles(&a5), circles(&named) + 3);
        // Up the neck: the first fret is written and there is no nut; a barre joins its strings.
        let barre = one_note(guitario_theory::NoteValue::Quarter, |b| {
            b.chord = Some(Chord { name: "D".into(), frets: vec![Some(5), Some(7), Some(7), Some(7), Some(5), None], first_fret: 5 })
        });
        assert_eq!(texts(&barre, true, "5fr"), 1);
        assert_eq!(lines(&barre, true), lines(&named, true) + 6 + 5 + 2 + 1, "grid, one x mark, the barre");
        // An open D is fingered: no barre although fret 2 is held on both sides of the third fret.
        let open_d = one_note(guitario_theory::NoteValue::Quarter, |b| {
            b.chord = Some(Chord { name: "D".into(), frets: vec![Some(2), Some(3), Some(2), Some(0), None, None], first_fret: 1 })
        });
        assert_eq!(lines(&open_d, true), lines(&named, true) + 6 + 5 + 1 + 2 * 2, "grid, nut, two x marks");
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
}
