//! Music symbols. With a SMuFL font loaded they are drawn from it at `4 × staff space` (one em is
//! four staff spaces in SMuFL); otherwise each symbol has a simple painter-drawn stand-in.

use crate::fonts::{FontSet, SMUFL};
use egui::{Color32, Painter, Pos2, Stroke, pos2, vec2};

// SMuFL code points (Standard Music Font Layout 1.4).
pub const G_CLEF: char = '\u{E050}';
pub const F_CLEF: char = '\u{E062}';
pub const TAB_CLEF_6: char = '\u{E06D}';
pub const TAB_CLEF_4: char = '\u{E06E}';
/// SMuFL unpitchedPercussionClef1: two thick bars, centred on the middle line.
pub const PERC_CLEF: char = '\u{E069}';
pub const TIME_0: u32 = 0xE080;
pub const HEAD_WHOLE: char = '\u{E0A2}';
pub const HEAD_HALF: char = '\u{E0A3}';
pub const HEAD_BLACK: char = '\u{E0A4}';
pub const HEAD_X: char = '\u{E0A9}';
pub const DOT: char = '\u{E1E7}';
pub const FLAT: char = '\u{E260}';
pub const NATURAL: char = '\u{E261}';
pub const SHARP: char = '\u{E262}';
pub const ACCENT_ABOVE: char = '\u{E4A0}';
pub const STACCATO_ABOVE: char = '\u{E4A2}';
/// articMarcatoAbove / Below: the heavy accent.
pub const MARCATO_ABOVE: char = '\u{E4AC}';
pub const MARCATO_BELOW: char = '\u{E4AD}';
/// stringsDownBow / stringsUpBow: pick strokes.
pub const STROKE_DOWN: char = '\u{E610}';
pub const STROKE_UP: char = '\u{E612}';
pub const REST_WHOLE: char = '\u{E4E3}';
pub const REST_HALF: char = '\u{E4E4}';
pub const REST_QUARTER: char = '\u{E4E5}';
pub const MET_WHOLE: char = '\u{E1D2}';
pub const MET_HALF_UP: char = '\u{E1D3}';
pub const MET_QUARTER_UP: char = '\u{E1D5}';
pub const MET_8TH_UP: char = '\u{E1D7}';
pub const MET_16TH_UP: char = '\u{E1D9}';
pub const MET_32ND_UP: char = '\u{E1DB}';
pub const MET_64TH_UP: char = '\u{E1DD}';

/// Rest glyph for a note-value divisor (1 = whole … 64).
pub fn rest_for(divisor: u32) -> char {
    match divisor {
        1 => REST_WHOLE,
        2 => REST_HALF,
        4 => REST_QUARTER,
        8 => '\u{E4E6}',
        16 => '\u{E4E7}',
        32 => '\u{E4E8}',
        _ => '\u{E4E9}',
    }
}

/// Flag glyph: 8th up is E240, down E241, then pairs for 16th, 32nd, 64th.
pub fn flag_for(divisor: u32, up: bool) -> Option<char> {
    let base = match divisor {
        8 => 0xE240,
        16 => 0xE242,
        32 => 0xE244,
        64 => 0xE246,
        _ => return None,
    };
    char::from_u32(base + u32::from(!up))
}

pub fn metronome_for(divisor: u32) -> char {
    match divisor {
        1 => MET_WHOLE,
        2 => MET_HALF_UP,
        4 => MET_QUARTER_UP,
        8 => MET_8TH_UP,
        16 => MET_16TH_UP,
        32 => MET_32ND_UP,
        _ => MET_64TH_UP,
    }
}

pub fn time_digit(d: u32) -> char {
    char::from_u32(TIME_0 + d.min(9)).unwrap_or('0')
}

#[derive(Clone, Copy)]
pub struct Pen<'a> {
    pub painter: &'a Painter,
    pub fonts: FontSet,
    /// Staff space in points.
    pub space: f32,
}

impl Pen<'_> {
    /// Draws `c` with its SMuFL origin (left edge, baseline) at `at`. Returns its advance width.
    pub fn glyph(&self, c: char, at: Pos2, color: Color32) -> f32 {
        self.glyph_scaled(c, at, 1.0, color)
    }

    pub fn glyph_scaled(&self, c: char, at: Pos2, scale: f32, color: Color32) -> f32 {
        if self.fonts.smufl {
            let font = egui::FontId::new(4.0 * self.space * scale, egui::FontFamily::Name(SMUFL.into()));
            let galley = self.painter.layout_no_wrap(c.to_string(), font, color);
            let (baseline, advance) =
                galley.rows.first().and_then(|r| r.row.glyphs.first().map(|g| (r.pos.y + g.pos.y, g.advance_width))).unwrap_or((0.0, galley.size().x));
            self.painter.galley(at - vec2(0.0, baseline), galley, color);
            advance
        } else {
            self.fallback(c, at, scale, color)
        }
    }

    /// Draws `c` horizontally centred on `at.x`.
    pub fn glyph_centered(&self, c: char, at: Pos2, color: Color32) -> f32 {
        let w = self.width(c);
        self.glyph(c, at - vec2(w / 2.0, 0.0), color);
        w
    }

    pub fn width(&self, c: char) -> f32 {
        if self.fonts.smufl {
            let font = egui::FontId::new(4.0 * self.space, egui::FontFamily::Name(SMUFL.into()));
            let galley = self.painter.layout_no_wrap(c.to_string(), font, Color32::BLACK);
            galley.rows.first().and_then(|r| r.row.glyphs.first().map(|g| g.advance_width)).unwrap_or(galley.size().x)
        } else {
            fallback_width(c) * self.space
        }
    }

    fn fallback(&self, c: char, at: Pos2, scale: f32, color: Color32) -> f32 {
        let s = self.space * scale;
        let p = self.painter;
        let w = fallback_width(c) * s;
        let stroke = Stroke::new((0.12 * s).max(1.0), color);
        let text = |t: &str, size: f32| {
            p.text(at, egui::Align2::LEFT_BOTTOM, t, egui::FontId::proportional(size), color);
        };
        match c {
            HEAD_BLACK | HEAD_HALF | HEAD_WHOLE => {
                let r = vec2(w / 2.0, 0.42 * s);
                let center = at + vec2(w / 2.0, 0.0);
                let shape =
                    egui::epaint::EllipseShape { center, radius: r, fill: if c == HEAD_BLACK { color } else { Color32::TRANSPARENT }, stroke, angle: -0.35 };
                p.add(shape);
            }
            HEAD_X => {
                p.line_segment([at + vec2(0.0, -0.45 * s), at + vec2(w, 0.45 * s)], stroke);
                p.line_segment([at + vec2(0.0, 0.45 * s), at + vec2(w, -0.45 * s)], stroke);
            }
            DOT => {
                p.circle_filled(at + vec2(0.25 * s, 0.0), 0.2 * s, color);
            }
            REST_WHOLE => {
                p.rect_filled(egui::Rect::from_min_size(at, vec2(w, 0.5 * s)), 0.0, color);
            }
            REST_HALF => {
                p.rect_filled(egui::Rect::from_min_size(at - vec2(0.0, 0.5 * s), vec2(w, 0.5 * s)), 0.0, color);
            }
            REST_QUARTER => {
                let pts = [vec2(0.2, -1.5), vec2(0.8, -0.8), vec2(0.2, -0.2), vec2(0.8, 0.5), vec2(0.3, 0.4), vec2(0.6, 1.2)];
                p.add(egui::Shape::line(pts.iter().map(|v| at + *v * s).collect(), Stroke::new(0.25 * s, color)));
            }
            '\u{E4E6}'..='\u{E4E9}' => {
                let n = (c as u32 - 0xE4E6 + 1) as f32;
                for i in 0..n as usize {
                    p.circle_filled(at + vec2(0.35 * s + i as f32 * 0.15 * s, -0.5 * s - i as f32 * s), 0.25 * s, color);
                }
                p.line_segment([at + vec2(0.4 * s, 1.0 * s), at + vec2(1.0 * s + n * 0.15 * s, -0.6 * s - (n - 1.0) * s)], stroke);
            }
            '\u{E240}'..='\u{E247}' => {
                let up = (c as u32 - 0xE240).is_multiple_of(2);
                let n = (c as u32 - 0xE240) / 2 + 1;
                for i in 0..n {
                    let dy = i as f32 * 0.8 * s * if up { 1.0 } else { -1.0 };
                    let a = at + vec2(0.0, dy);
                    let b = a + vec2(0.9 * s, if up { 1.6 * s } else { -1.6 * s });
                    p.line_segment([a, b], Stroke::new(0.2 * s, color));
                }
            }
            SHARP => text("#", 2.6 * s),
            FLAT => text("b", 2.8 * s),
            NATURAL => text("♮", 2.6 * s),
            G_CLEF => {
                p.text(at + vec2(0.0, 1.2 * s), egui::Align2::LEFT_BOTTOM, "G", egui::FontId::proportional(5.5 * s), color);
            }
            F_CLEF => {
                p.text(at + vec2(0.0, 1.5 * s), egui::Align2::LEFT_BOTTOM, "F", egui::FontId::proportional(4.5 * s), color);
            }
            PERC_CLEF => {
                for dx in [0.3, 1.0] {
                    p.rect_filled(egui::Rect::from_min_size(at + vec2(dx * s, -s), vec2(0.45 * s, 2.0 * s)), 0.0, color);
                }
            }
            TAB_CLEF_6 | TAB_CLEF_4 => {
                for (i, l) in ["T", "A", "B"].iter().enumerate() {
                    p.text(at + vec2(0.0, (i as f32 - 1.0) * 1.9 * s), egui::Align2::LEFT_CENTER, *l, egui::FontId::proportional(2.2 * s), color);
                }
            }
            ACCENT_ABOVE => {
                p.add(egui::Shape::line(vec![at + vec2(0.0, -0.7 * s), at + vec2(w, -0.35 * s), at], stroke));
            }
            MARCATO_ABOVE | MARCATO_BELOW => {
                let d = if c == MARCATO_ABOVE { -1.0 } else { 1.0 };
                p.add(egui::Shape::line(vec![at, at + vec2(w / 2.0, d * 0.9 * s), at + vec2(w, 0.0)], Stroke::new(0.22 * s, color)));
            }
            STROKE_DOWN => {
                let r = egui::Rect::from_min_size(at - vec2(0.0, 0.9 * s), vec2(w, 0.9 * s));
                p.line_segment([r.left_bottom(), r.left_top()], stroke);
                p.line_segment([r.right_bottom(), r.right_top()], stroke);
                p.rect_filled(egui::Rect::from_min_size(r.left_top(), vec2(w, 0.25 * s)), 0.0, color);
            }
            STROKE_UP => {
                p.add(egui::Shape::line(vec![at + vec2(0.0, -1.1 * s), at, at + vec2(w, -1.1 * s)], stroke));
            }
            STACCATO_ABOVE => {
                p.circle_filled(at + vec2(0.2 * s, -0.2 * s), 0.18 * s, color);
            }
            c if (0xE080..=0xE089).contains(&(c as u32)) => {
                let d = (c as u32 - 0xE080).to_string();
                p.text(at + vec2(0.0, 1.0 * s), egui::Align2::LEFT_BOTTOM, d, egui::FontId::proportional(2.8 * s), color);
            }
            c if (0xE1D2..=0xE1DD).contains(&(c as u32)) => {
                let head = egui::epaint::EllipseShape {
                    center: at + vec2(0.55 * s, -0.4 * s),
                    radius: vec2(0.55 * s, 0.4 * s),
                    fill: if c as u32 >= 0xE1D5 { color } else { Color32::TRANSPARENT },
                    stroke,
                    angle: -0.35,
                };
                p.add(head);
                if c != MET_WHOLE {
                    p.line_segment([at + vec2(1.05 * s, -0.4 * s), at + vec2(1.05 * s, -3.4 * s)], stroke);
                }
                let flags = match c as u32 {
                    0xE1D7 => 1,
                    0xE1D9 => 2,
                    0xE1DB => 3,
                    0xE1DD => 4,
                    _ => 0,
                };
                for i in 0..flags {
                    let y = -3.4 * s + i as f32 * 0.6 * s;
                    p.line_segment([at + vec2(1.05 * s, y), at + vec2(1.8 * s, y + 1.0 * s)], stroke);
                }
            }
            _ => text("?", 2.0 * s),
        }
        w
    }
}

fn fallback_width(c: char) -> f32 {
    match c {
        HEAD_WHOLE => 1.7,
        HEAD_BLACK | HEAD_HALF | HEAD_X => 1.18,
        G_CLEF | F_CLEF => 2.7,
        TAB_CLEF_6 | TAB_CLEF_4 => 1.6,
        PERC_CLEF => 1.8,
        REST_WHOLE | REST_HALF => 1.1,
        SHARP | NATURAL => 1.0,
        FLAT => 0.9,
        DOT => 0.5,
        _ => 1.2,
    }
}

/// Draws a line from `a` to `b` in short dashes.
pub fn dashed(p: &Painter, a: Pos2, b: Pos2, stroke: Stroke, dash: f32) {
    let len = a.distance(b);
    if len <= 0.0 || dash <= 0.0 {
        return;
    }
    let dir = (b - a) / len;
    let mut t = 0.0;
    while t < len {
        let e = (t + dash).min(len);
        p.line_segment([a + dir * t, a + dir * e], stroke);
        t += dash * 2.0;
    }
}

/// A shallow arc (tie, slur, hammer-on) from `a` to `b`, bulging by `height` (negative is up).
pub fn arc(p: &Painter, a: Pos2, b: Pos2, height: f32, stroke: Stroke) {
    let mid = pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0 + height * 1.33);
    p.add(egui::epaint::QuadraticBezierShape::from_points_stroke([a, mid, b], false, Color32::TRANSPARENT, stroke));
}

/// A wavy vibrato line from `x0` to `x1` at `y`.
pub fn wavy(p: &Painter, x0: f32, x1: f32, y: f32, amp: f32, stroke: Stroke) {
    let period = amp * 3.0;
    if x1 <= x0 || period <= 0.0 {
        return;
    }
    let mut pts = Vec::new();
    let mut x = x0;
    while x <= x1 {
        pts.push(pos2(x, y + amp * ((x - x0) / period * std::f32::consts::TAU).sin()));
        x += period / 8.0;
    }
    p.add(egui::Shape::line(pts, stroke));
}
