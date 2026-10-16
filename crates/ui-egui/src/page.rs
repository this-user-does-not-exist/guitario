//! The page view: white A4 pages on a grey workspace, each system a standard-notation staff over a
//! tablature staff joined by a bracket, laid out by `layout` and painted here. Clicks move the
//! cursor; the painter never edits the score directly.

use crate::fonts::{FontSet, sans, serif};
use crate::glyphs::{self as g, Pen};
use crate::layout::{Metrics, PageBox, SystemBox, layout};
use crate::notation::{Accidentals, Clef, beam_groups, spell, staff_pos};
use crate::theme::{Tokens, tokens};
use egui::{Color32, Painter, Rect, Stroke, pos2, vec2};
use guitario_engine::Cursor;
use guitario_score::{Beat, BeatTechnique, BrushKind, Fade, MeasureHeader, Note, NoteKind, Score, Slide, StrokeDirection, Track, TripletFeel};

const PAGE_GAP: f32 = 24.0;

type BeatTest = fn(&Beat) -> bool;

/// What a click on the page asks for.
pub enum Click {
    Beat { measure: usize, beat: usize, string: Option<usize>, shift: bool },
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
}

pub struct Output {
    pub click: Option<Click>,
    /// A drag across beats: from where it started to where the pointer is.
    pub drag: Option<Range>,
    pub cursor_rect: Option<Rect>,
    pub play_rect: Option<Rect>,
}

pub fn show(ui: &mut egui::Ui, score: &Score, cursor: Cursor, view: &View) -> Output {
    let Some(track) = score.tracks.get(cursor.track) else { return Output { click: None, drag: None, cursor_rect: None, play_rect: None } };
    let m = if track.is_drums() { Metrics::for_drums(view.zoom) } else { Metrics::new(view.zoom, view.notation, track.string_count()) };
    let pages = layout(score, cursor.track, &m);
    let width = (m.page_w + 2.0 * PAGE_GAP).max(ui.available_width());
    let height = pages.len() as f32 * (m.page_h + PAGE_GAP) + PAGE_GAP;
    let (response, painter) = ui.allocate_painter(vec2(width, height), egui::Sense::click_and_drag());
    let t = tokens();
    painter.rect_filled(response.rect, 0.0, t.workspace);
    let left = response.rect.left() + (width - m.page_w) / 2.0;
    let pen = Pen { painter: &painter, fonts: view.fonts, space: m.space };
    let mut cx = Ctx {
        pen,
        m,
        score,
        track,
        cursor,
        t,
        hits: Vec::new(),
        cursor_rect: None,
        play: view.play,
        play_rect: None,
        selection: view.selection,
        clef: Clef::for_tuning(track.tuning.strings.iter().copied().max().unwrap_or(64)),
    };

    for (pi, page) in pages.iter().enumerate() {
        let top = response.rect.top() + PAGE_GAP + pi as f32 * (m.page_h + PAGE_GAP);
        let rect = Rect::from_min_size(pos2(left, top), vec2(m.page_w, m.page_h));
        if !painter.clip_rect().intersects(rect) {
            continue;
        }
        painter.rect_filled(rect.translate(vec2(3.0, 4.0)), 0.0, Color32::from_black_alpha(70));
        painter.rect_filled(rect, 0.0, t.page);
        if pi == 0 {
            cx.header(rect);
        }
        painter.text(
            rect.right_bottom() - vec2(m.margin_x, m.margin_bottom * 0.5),
            egui::Align2::RIGHT_CENTER,
            format!("{}/{}", pi + 1, pages.len()),
            serif(9.0 * m.zoom),
            Color32::from_gray(110),
        );
        cx.page(page, rect, pi == 0);
    }

    let mut click = None;
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
        && let Some(h) = cx.hits.iter().find(|h| h.rect.contains(pos))
    {
        let string = (!m.drums && pos.y >= h.tab_top - m.tab_gap / 2.0 && pos.y <= h.tab_top + m.tab_h() + m.tab_gap / 2.0)
            .then(|| (((pos.y - h.tab_top) / m.tab_gap).round().max(0.0) as usize).min(track.string_count().saturating_sub(1)));
        let shift = ui.input(|i| i.modifiers.shift);
        click = Some(Click::Beat { measure: h.measure, beat: h.beat, string, shift });
    }
    let hit_at = |p: egui::Pos2| cx.hits.iter().find(|h| h.rect.contains(p)).map(|h| (h.measure, h.beat));
    let drag = if response.dragged() {
        let origin = ui.input(|i| i.pointer.press_origin());
        match (origin.and_then(hit_at), response.interact_pointer_pos().and_then(hit_at)) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        }
    } else {
        None
    };
    Output { click, drag, cursor_rect: cx.cursor_rect, play_rect: cx.play_rect }
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
        self.p().text(pos2(page.center().x, y), egui::Align2::CENTER_TOP, title, serif(33.0 * m.zoom), self.t.ink);
        y += 10.0 * m.space;
        if !meta.artist.is_empty() {
            self.p().text(pos2(page.center().x, y), egui::Align2::CENTER_TOP, &meta.artist, serif(16.0 * m.zoom), self.t.ink);
        }
        if !meta.author.is_empty() {
            self.p().text(pos2(page.right() - m.margin_x, y), egui::Align2::RIGHT_TOP, format!("Tab: {}", meta.author), serif(10.0 * m.zoom), self.t.ink);
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
            self.p().text(pos2(page.left() + m.margin_x, y + i as f32 * 2.6 * m.space), egui::Align2::LEFT_TOP, l, serif(10.0 * m.zoom), self.t.ink);
        }
    }

    fn page(&mut self, page: &PageBox, rect: Rect, first_page: bool) {
        for (si, sys) in page.systems.iter().enumerate() {
            let top = rect.top() + sys.y;
            let sys_rect = Rect::from_min_size(pos2(rect.left(), top), vec2(self.m.page_w, self.m.system_h()));
            if !self.p().clip_rect().intersects(sys_rect) {
                continue;
            }
            self.system(sys, rect.left() + self.m.margin_x + self.m.indent(), top, first_page && si == 0);
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

        // Bar number and the opening tempo mark.
        self.p().text(pos2(x0 + 0.2 * s, sys_top - 1.0 * s), egui::Align2::LEFT_BOTTOM, (first_mi + 1).to_string(), serif(8.0 * m.zoom), t.bar_number);
        if first {
            self.tempo_mark(x0 + sys.prefix, sys_top - 3.6 * s, self.score.tempo);
        }

        // Measures.
        let mut placed: Vec<Placed> = Vec::new();
        let last_measure = self.score.headers.len().saturating_sub(1);
        for mb in &sys.measures {
            let mx = x0 + sys.prefix + mb.x;
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
                self.p().text(
                    pos2(end, sys_top - 0.6 * s),
                    egui::Align2::RIGHT_BOTTOM,
                    format!("x{}", u16::from(h.repeat_close) + 1),
                    serif(9.0 * m.zoom),
                    t.ink,
                );
            } else if mb.index == last_measure {
                self.final_bar(end, nt, tt, tb);
            } else {
                if m.notation {
                    self.p().line_segment([pos2(end, nt), pos2(end, nt + 4.0 * s)], bar);
                }
                self.p().line_segment([pos2(end, tt), pos2(end, tb)], bar);
                if h.double_bar {
                    self.staves_lines(end - 0.6 * s, nt, tt, tb, bar);
                }
            }
            // Triplet feel where it changes.
            let prev_feel = mb.index.checked_sub(1).and_then(|i| self.score.headers.get(i)).map_or(TripletFeel::None, |p| p.triplet_feel);
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
                let r = Rect::from_min_size(pos2(mx + 0.4 * s, sys_top - 8.6 * s), galley.size() + vec2(0.8 * s, 0.4 * s));
                self.p().rect_stroke(r, 0.0, Stroke::new(1.0, t.ink), egui::StrokeKind::Inside);
                self.p().galley(r.min + vec2(0.4 * s, 0.2 * s), galley, t.ink);
            }
            if let Some(tempo) = h.tempo
                && mb.index > 0
            {
                self.tempo_mark(mx + 0.5 * s, sys_top - 3.6 * s, tempo);
            }
            let beats = self.track.measures.get(mb.index).and_then(|x| x.voices.first()).map(|v| v.beats.as_slice()).unwrap_or_default();
            if let Some((pm, pb)) = self.play
                && pm == mb.index
            {
                // Playback: tint the bar, mark the sounding beat.
                let bar_rect = Rect::from_min_max(pos2(mx, sys_top - 1.2 * s), pos2(end, tb + 1.2 * s));
                self.p().rect_filled(bar_rect, 0.0, t.play_bar);
                if let (Some(cx), Some(w)) = (mb.beats.get(pb), mb.beat_w.get(pb)) {
                    let col = Rect::from_min_max(pos2(mx + cx - w / 2.0, sys_top - 1.2 * s), pos2(mx + cx + w / 2.0, tb + 1.2 * s));
                    self.p().rect_filled(col, 0.0, t.play_beat);
                    self.play_rect = Some(col);
                }
            }
            if m.notation {
                self.notation_measure(h, beats, mb, mx, nt);
            }
            // Selection: one continuous band per measure, running to the bar lines where the
            // selection carries on into the neighbouring measure.
            if let Some(((m0, b0), (m1, b1))) = self.selection
                && (m0..=m1).contains(&mb.index)
            {
                let first = if mb.index == m0 { b0 } else { 0 };
                let last = if mb.index == m1 { b1 } else { beats.len().saturating_sub(1) };
                let edge = |bi: usize, right: bool| mb.beats.get(bi).zip(mb.beat_w.get(bi)).map(|(c, w)| mx + c + if right { w / 2.0 } else { -w / 2.0 });
                let left = if mb.index > m0 { Some(mx) } else { edge(first, false) };
                let right = if mb.index < m1 { Some(end) } else { edge(last, true) };
                if let (Some(l), Some(r)) = (left, right) {
                    self.p().rect_filled(Rect::from_min_max(pos2(l, sys_top - 1.2 * s), pos2(r, tb + 1.2 * s)), 0.0, t.selection_fill);
                }
            }
            for (bi, beat) in beats.iter().enumerate() {
                let (Some(cx), Some(w)) = (mb.beats.get(bi), mb.beat_w.get(bi)) else { continue };
                let x = mx + cx;
                let col = Rect::from_min_max(pos2(x - w / 2.0, sys_top - 1.2 * s), pos2(x + w / 2.0, tb + 1.2 * s));
                if self.cursor.measure == mb.index && self.cursor.beat == bi {
                    self.p().rect_filled(col, 2.0, t.cursor_fill);
                    if !m.drums {
                        let cy = self.tab_y(tt, self.cursor.string.min(strings.saturating_sub(1)));
                        let r = Rect::from_center_size(pos2(x, cy), vec2((1.6 * m.tab_gap).min(*w - 2.0).max(m.tab_gap), m.tab_gap));
                        self.p().rect_stroke(r, 2.0, Stroke::new(1.5, t.cursor_stroke), egui::StrokeKind::Outside);
                    }
                    self.cursor_rect = Some(col);
                }
                self.hits.push(Hit { rect: col, measure: mb.index, beat: bi, tab_top: tt });
                placed.push(Placed { x, w: *w, beat });
            }
        }
        // Tab content after the cursor so frets sit on top of it.
        if !m.drums {
            for (i, pb) in placed.iter().enumerate() {
                self.tab_beat(pb, placed.get(i + 1..).unwrap_or_default(), tt);
            }
            self.effect_lanes(&placed, tt);
        }
        if !m.notation {
            self.tab_rhythm(sys, x0, tb);
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

    fn notation_measure(&self, h: &MeasureHeader, beats: &[Beat], mb: &crate::layout::MeasureBox, mx: f32, nt: f32) {
        let s = self.m.space;
        let ink = self.t.ink;
        let mut acc = Accidentals::default();
        // Positions and stems per beat.
        struct Info {
            x: f32,
            pos: Vec<(i32, Option<i8>, &'static str)>,
            up: bool,
            div: u32,
        }
        let mut infos: Vec<Option<Info>> = Vec::new();
        for (bi, beat) in beats.iter().enumerate() {
            let x = mx + mb.beats.get(bi).copied().unwrap_or(0.0);
            if beat.rest || beat.notes.is_empty() {
                let div = beat.duration.value.divisor();
                let pos = match div {
                    1 => 6,
                    _ => 4,
                };
                let c = g::rest_for(div);
                self.pen.glyph_centered(c, pos2(x, self.nota_y(nt, pos)), ink);
                self.dots(beat, x + 0.9 * s, nt, 5);
                infos.push(None);
                continue;
            }
            let mut pos: Vec<(i32, Option<i8>, &'static str)> = beat
                .notes
                .iter()
                .filter_map(|n| {
                    if self.m.drums {
                        // Drum notation: the kit map places each piece; cymbals take a cross.
                        let piece = guitario_theory::drums::piece(n.fret);
                        let pos = piece.map_or(6, |p| i32::from(p.staff_pos));
                        return Some((pos, None, if piece.is_some_and(|p| p.cross) { "x" } else { "n" }));
                    }
                    let p = self.track.tuning.pitch(usize::from(n.string), n.fret, self.track.capo)?;
                    let written = p.0.saturating_add(12).min(127);
                    let sp = spell(written, h.key);
                    let head = if n.kind == NoteKind::Dead { "x" } else { "n" };
                    Some((staff_pos(sp, self.clef), acc.next(sp, h.key), head))
                })
                .collect();
            pos.sort_by_key(|p| p.0);
            let avg = pos.iter().map(|p| p.0).sum::<i32>() as f32 / pos.len().max(1) as f32;
            // Drum parts are written stems up (one voice, hands and feet together).
            infos.push(Some(Info { x, pos, up: self.m.drums || avg < 4.0, div: beat.duration.value.divisor() }));
        }

        let groups = beam_groups(beats, h.time_signature);
        // A beamed group shares one stem direction.
        for gr in &groups {
            let all: Vec<i32> = gr.iter().filter_map(|i| infos.get(*i).and_then(|x| x.as_ref())).flat_map(|x| x.pos.iter().map(|p| p.0)).collect();
            let avg = all.iter().sum::<i32>() as f32 / all.len().max(1) as f32;
            for i in gr {
                if let Some(Some(info)) = infos.get_mut(*i) {
                    info.up = self.m.drums || avg < 4.0;
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

        // Beams.
        let beam_t = 0.5 * s;
        for gr in &groups {
            let ends: Vec<(usize, f32, f32, bool, u32)> = gr
                .iter()
                .filter_map(|i| {
                    let info = infos.get(*i)?.as_ref()?;
                    let (x, y) = (*stem_ends.get(*i)?)?;
                    Some((*i, x, y, info.up, info.div))
                })
                .collect();
            let Some(&(_, _, _, up, _)) = ends.first() else { continue };
            let beam_y = if up { ends.iter().map(|e| e.2).fold(f32::MAX, f32::min) } else { ends.iter().map(|e| e.2).fold(f32::MIN, f32::max) };
            for &(i, x, _, _, _) in &ends {
                let Some(Some(info)) = infos.get(i) else { continue };
                let (lo, hi) = (info.pos.first().map_or(4, |p| p.0), info.pos.last().map_or(4, |p| p.0));
                let y0 = self.nota_y(nt, if up { lo } else { hi });
                self.p().line_segment([pos2(x, y0), pos2(x, beam_y)], stem);
            }
            let dir = if up { 1.0 } else { -1.0 };
            for level in 0..4u32 {
                let need = 8u32 << level;
                let y = beam_y + dir * level as f32 * (beam_t + 0.25 * s);
                let rect = |a: f32, b: f32| Rect::from_min_max(pos2(a.min(b), y.min(y + dir * beam_t)), pos2(a.max(b), y.max(y + dir * beam_t)));
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
                        self.p().rect_filled(rect(x, x + stub), 0.0, ink);
                    } else {
                        self.p().rect_filled(rect(x, x_end), 0.0, ink);
                    }
                    k = j + 1;
                }
            }
            // Tuplet number over the group.
            if let Some(b) = gr.first().and_then(|i| beats.get(*i))
                && b.duration.tuplet != (1, 1)
            {
                let xs: Vec<f32> = ends.iter().map(|e| e.1).collect();
                let cx = (xs.iter().copied().fold(f32::MAX, f32::min) + xs.iter().copied().fold(f32::MIN, f32::max)) / 2.0;
                let y = beam_y - dir * 1.2 * s;
                self.p().text(pos2(cx, y), egui::Align2::CENTER_CENTER, b.duration.tuplet.0.to_string(), serif(9.0 * self.m.zoom), ink);
            }
        }
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

    fn tab_beat(&self, pb: &Placed, rest: &[Placed], tt: f32) {
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
            if pb.beat.notes.iter().any(|n| n.effects.wide_vibrato) {
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
            let beats = self.track.measures.get(mb.index).and_then(|x| x.voices.first()).map(|v| v.beats.as_slice()).unwrap_or_default();
            let ts = self.score.headers.get(mb.index).map(|h| h.time_signature).unwrap_or_default();
            let groups = beam_groups(beats, ts);
            let x_of = |bi: usize| mb.beats.get(bi).map(|cx| mx + cx);
            for (bi, beat) in beats.iter().enumerate() {
                let Some(x) = x_of(bi) else { continue };
                let div = beat.duration.value.divisor();
                if beat.rest {
                    self.pen.glyph_centered(g::rest_for(div), pos2(x, (y0 + y1) / 2.0), ink);
                    continue;
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
