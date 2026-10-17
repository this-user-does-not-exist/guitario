//! Toolbar icons drawn with the painter, so they need no icon font and scale cleanly. Shapes
//! follow Guitar Pro 8's toolbar (observed, not copied): plain glyphs on rounded button groups.

use egui::{Color32, Painter, Rect, Shape, Stroke, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Home,
    Save,
    Print,
    Undo,
    Redo,
    First,
    Prev,
    Play,
    Stop,
    Next,
    Last,
    Loop,
    Metronome,
    CountIn,
    More,
    Zoom,
    Layouts,
    Palette,
    Inspector,
    GlobalView,
    Fretboard,
    Keyboard,
    Drums,
    Quarter,
    TuningFork,
    AudioTrack,
    Tuner,
    LineIn,
    Fretlight,
    Guitar,
    Bass,
    Eye,
    Mute,
    Solo,
    Equalizer,
    Bookmark,
    Multitrack,
}

/// The icon for a track's instrument: drums, a bass (a bass program, or tuned down to G1 or
/// lower, which no guitar tuning reaches), or a guitar.
pub fn instrument(track: &guitario_score::Track) -> Icon {
    let lowest = track.tuning.strings.iter().copied().min().unwrap_or(u8::MAX);
    if track.is_drums() {
        Icon::Drums
    } else if (32..=39).contains(&track.program) || lowest <= 31 {
        Icon::Bass
    } else {
        Icon::Guitar
    }
}

/// Points on an ellipse arc from `a0` to `a1` (radians, y down), for strokes.
fn arc(c: egui::Pos2, rx: f32, ry: f32, a0: f32, a1: f32, n: usize) -> Vec<egui::Pos2> {
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * (i as f32 / n.max(1) as f32);
            c + vec2(rx * a.cos(), ry * a.sin())
        })
        .collect()
}

pub fn draw(p: &Painter, r: Rect, icon: Icon, color: Color32) {
    let c = r.center();
    let u = r.height().min(r.width()) / 16.0;
    let st = Stroke::new(1.4 * u.max(0.8), color);
    let thin = Stroke::new(1.0 * u.max(0.7), color);
    let at = |x: f32, y: f32| c + vec2(x * u, y * u);
    let tri = |a: egui::Pos2, b: egui::Pos2, d: egui::Pos2| Shape::convex_polygon(vec![a, b, d], color, Stroke::NONE);
    use std::f32::consts::PI;
    match icon {
        Icon::Home => {
            p.add(Shape::convex_polygon(vec![at(0.0, -6.5), at(7.0, 0.0), at(-7.0, 0.0)], color, Stroke::NONE));
            p.rect_filled(Rect::from_min_max(at(-4.8, -0.5), at(4.8, 6.0)), 0.5, color);
            p.rect_filled(Rect::from_min_max(at(-1.4, 2.0), at(1.4, 6.0)), 0.0, Color32::from_black_alpha(140));
        }
        Icon::Save => {
            p.rect_stroke(Rect::from_min_max(at(-5.5, -5.5), at(5.5, 5.5)), 1.0, st, egui::StrokeKind::Middle);
            p.rect_stroke(Rect::from_min_max(at(-3.0, -5.5), at(3.0, -1.5)), 0.0, st, egui::StrokeKind::Middle);
            p.rect_filled(Rect::from_min_max(at(-3.0, 1.5), at(3.0, 5.5)), 0.0, color);
        }
        Icon::Print => {
            // Paper in, the printer body, paper out.
            p.rect_stroke(Rect::from_min_max(at(-3.5, -6.5), at(3.5, -2.5)), 0.5, st, egui::StrokeKind::Middle);
            p.rect_filled(Rect::from_min_max(at(-7.0, -2.5), at(7.0, 3.5)), 1.5, color);
            p.rect_filled(Rect::from_min_max(at(-3.5, 1.0), at(3.5, 6.5)), 0.0, color);
            p.rect_filled(Rect::from_min_max(at(-2.3, 2.2), at(2.3, 5.3)), 0.0, Color32::from_black_alpha(150));
        }
        Icon::Undo | Icon::Redo => {
            // A hook bending back over itself, the arrowhead at the start: ↶ for undo, ↷ for redo.
            let f = if icon == Icon::Undo { 1.0 } else { -1.0 };
            let pts: Vec<_> = arc(at(0.5 * f, 0.5), 5.0 * u, 4.5 * u, PI, 2.0 * PI + 0.9, 16).into_iter().map(|q| pos2(c.x + (q.x - c.x) * f, q.y)).collect();
            p.add(Shape::line(pts, st));
            let tip = at(-4.5 * f - 0.0, 3.5);
            p.add(tri(at(-7.5 * f, 0.0), at(-1.5 * f, 0.0), tip));
        }
        Icon::Play => {
            p.circle_stroke(c, 7.2 * u, st);
            p.add(Shape::convex_polygon(vec![at(-2.4, -4.0), at(4.4, 0.0), at(-2.4, 4.0)], color, Stroke::NONE));
        }
        Icon::Stop => {
            p.circle_stroke(c, 7.2 * u, st);
            p.rect_filled(Rect::from_center_size(c, vec2(6.0 * u, 6.0 * u)), 0.5, color);
        }
        Icon::First | Icon::Last => {
            let f = if icon == Icon::First { 1.0 } else { -1.0 };
            p.rect_filled(Rect::from_center_size(at(-f * 5.0, 0.0), vec2(1.8 * u, 10.0 * u)), 0.0, color);
            p.add(tri(at(f * 5.0, -5.0), at(f * 5.0, 5.0), at(-f * 3.0, 0.0)));
        }
        Icon::Prev | Icon::Next => {
            let f = if icon == Icon::Prev { 1.0 } else { -1.0 };
            for dx in [0.0, 5.5] {
                p.add(tri(at(f * (dx + 0.5), -5.0), at(f * (dx + 0.5), 5.0), at(f * (dx - 5.0), 0.0)));
            }
        }
        Icon::Loop => {
            let pts = arc(c, 6.0 * u, 4.5 * u, -0.25 * PI, 1.6 * PI, 24);
            p.add(Shape::line(pts, st));
            p.add(tri(at(2.0, -6.8), at(6.5, -3.0), at(1.2, -1.2)));
        }
        Icon::Metronome => {
            p.add(Shape::convex_polygon(vec![at(-2.0, -6.5), at(2.0, -6.5), at(5.5, 6.5), at(-5.5, 6.5)], color, Stroke::NONE));
            let gap = Color32::from_black_alpha(170);
            p.line_segment([at(-0.5, 4.5), at(5.5, -6.0)], Stroke::new(2.6 * u, gap));
            p.line_segment([at(-0.5, 4.5), at(5.5, -6.0)], thin);
        }
        Icon::CountIn => {
            // An hourglass.
            p.line_segment([at(-5.0, -6.5), at(5.0, -6.5)], st);
            p.line_segment([at(-5.0, 6.5), at(5.0, 6.5)], st);
            p.add(Shape::closed_line(vec![at(-4.0, -6.0), at(4.0, -6.0), at(0.8, 0.0), at(4.0, 6.0), at(-4.0, 6.0), at(-0.8, 0.0)], thin));
            p.add(tri(at(-3.0, 6.0), at(3.0, 6.0), at(0.0, 2.5)));
            p.add(tri(at(-2.0, -4.0), at(2.0, -4.0), at(0.0, -1.5)));
        }
        Icon::More => {
            for y in [-5.0, 0.0, 5.0] {
                p.circle_filled(at(0.0, y), 1.4 * u, color);
            }
        }
        Icon::Zoom => {
            p.circle_stroke(at(-1.5, -1.5), 4.6 * u, st);
            p.line_segment([at(2.0, 2.0), at(6.5, 6.5)], Stroke::new(2.4 * u, color));
        }
        Icon::Layouts => {
            for (x, y) in [(-6.5, -6.5), (0.8, -6.5), (-6.5, 0.8), (0.8, 0.8)] {
                p.rect_stroke(Rect::from_min_size(at(x, y), vec2(5.7 * u, 5.7 * u)), 0.5, thin, egui::StrokeKind::Middle);
            }
        }
        Icon::Palette | Icon::Inspector | Icon::GlobalView => {
            let frame = Rect::from_min_max(at(-7.0, -6.0), at(7.0, 6.0));
            p.rect_stroke(frame, 1.5, st, egui::StrokeKind::Middle);
            let bar = match icon {
                Icon::Palette => [pos2(frame.left() + 4.0 * u, frame.top()), pos2(frame.left() + 4.0 * u, frame.bottom())],
                Icon::Inspector => [pos2(frame.right() - 4.0 * u, frame.top()), pos2(frame.right() - 4.0 * u, frame.bottom())],
                _ => [pos2(frame.left(), frame.bottom() - 4.0 * u), pos2(frame.right(), frame.bottom() - 4.0 * u)],
            };
            p.line_segment(bar, st);
        }
        Icon::Fretboard | Icon::Guitar | Icon::Bass => {
            // A headstock with tuning pegs: three a side for a guitar, two for a bass.
            let pegs: &[f32] = if icon == Icon::Bass { &[-3.0, 1.0] } else { &[-4.5, -1.5, 1.5] };
            p.add(Shape::convex_polygon(vec![at(-2.8, -7.0), at(2.8, -7.0), at(3.4, 4.0), at(-3.4, 4.0)], color, Stroke::NONE));
            p.rect_filled(Rect::from_min_max(at(-1.6, 4.0), at(1.6, 7.5)), 0.0, color);
            for y in pegs {
                p.rect_filled(Rect::from_center_size(at(-4.6, *y), vec2(2.4 * u, 1.6 * u)), 0.5, color);
                p.rect_filled(Rect::from_center_size(at(4.6, *y), vec2(2.4 * u, 1.6 * u)), 0.5, color);
            }
            let gap = Color32::from_black_alpha(150);
            for x in [-0.9, 0.9] {
                p.line_segment([at(x, -5.5), at(x, 7.5)], Stroke::new(0.6 * u, gap));
            }
        }
        Icon::Keyboard => {
            // White keys outlined, black keys filled, so it reads on dark and light chrome.
            let frame = Rect::from_min_max(at(-7.0, -6.0), at(7.0, 6.0));
            p.rect_stroke(frame, 1.0, thin, egui::StrokeKind::Middle);
            for i in 1..4 {
                let x = -7.0 + i as f32 * 3.5;
                p.line_segment([at(x, 1.0), at(x, 6.0)], thin);
            }
            for i in 0..3 {
                let x = -4.6 + i as f32 * 3.5;
                p.rect_filled(Rect::from_min_size(at(x, -6.0), vec2(2.2 * u, 7.0 * u)), 0.0, color);
            }
        }
        Icon::Drums => {
            // A snare drum with sticks.
            p.add(Shape::ellipse_filled(at(0.0, -0.5), vec2(6.5 * u, 2.2 * u), color));
            p.rect_filled(Rect::from_min_max(at(-6.5, -0.5), at(6.5, 4.5)), 0.0, color);
            p.add(Shape::ellipse_filled(at(0.0, 4.5), vec2(6.5 * u, 2.2 * u), color));
            p.add(Shape::ellipse_stroke(at(0.0, -0.5), vec2(6.5 * u, 2.2 * u), Stroke::new(0.8 * u, Color32::from_black_alpha(160))));
            p.line_segment([at(-6.0, -6.5), at(1.0, -1.5)], st);
            p.line_segment([at(6.5, -7.0), at(-0.5, -1.8)], st);
        }
        Icon::Quarter => {
            p.add(Shape::ellipse_filled(at(-1.5, 4.0), vec2(3.0 * u, 2.2 * u), color));
            p.line_segment([at(1.3, 3.6), at(1.3, -7.0)], st);
        }
        Icon::TuningFork => {
            // Two prongs on a U, and the stem.
            let mut pts = vec![at(-3.5, -7.0)];
            pts.extend(arc(at(0.0, -0.5), 3.5 * u, 3.0 * u, PI, 0.0, 10));
            pts.push(at(3.5, -7.0));
            p.add(Shape::line(pts, st));
            p.line_segment([at(0.0, 2.5), at(0.0, 7.5)], Stroke::new(2.0 * u, color));
        }
        Icon::AudioTrack => {
            for (i, h) in [1.5, 3.5, 6.0, 4.0, 7.0, 3.0, 5.0, 2.0, 1.0].iter().enumerate() {
                let x = -6.4 + i as f32 * 1.6;
                p.line_segment([at(x, -h), at(x, *h)], thin);
            }
        }
        Icon::Tuner => {
            // A sound source with waves either side.
            p.circle_filled(at(0.0, -1.0), 1.6 * u, color);
            p.line_segment([at(0.0, 0.5), at(0.0, 7.0)], st);
            for rx in [3.8, 6.6] {
                p.add(Shape::line(arc(at(0.0, -1.0), rx * u, rx * u, 0.75 * PI, 1.25 * PI, 10), thin));
                p.add(Shape::line(arc(at(0.0, -1.0), rx * u, rx * u, -0.25 * PI, 0.25 * PI, 10), thin));
            }
        }
        Icon::LineIn => {
            // A jack cable: plug, sleeve and a curling lead.
            p.line_segment([at(3.0, -7.0), at(5.5, -4.5)], st);
            p.add(Shape::convex_polygon(vec![at(1.2, -4.8), at(3.2, -2.8), at(0.2, 0.2), at(-1.8, -1.8)], color, Stroke::NONE));
            p.add(Shape::line(vec![at(-0.8, -0.8), at(-3.5, 2.5), at(-2.0, 5.0), at(-5.0, 7.0)], st));
        }
        Icon::Eye => {
            let top = arc(c, 7.0 * u, 5.0 * u, PI, 2.0 * PI, 14);
            let bottom = arc(c, 7.0 * u, 5.0 * u, 0.0, PI, 14);
            let mut outline = top;
            outline.extend(bottom.into_iter().skip(1));
            p.add(Shape::closed_line(outline, st));
            p.circle_filled(c, 2.6 * u, color);
        }
        Icon::Mute => {
            // A speaker, struck through.
            p.add(Shape::convex_polygon(vec![at(-6.0, -2.5), at(-3.0, -2.5), at(1.5, -6.5), at(1.5, 6.5), at(-3.0, 2.5), at(-6.0, 2.5)], color, Stroke::NONE));
            p.line_segment([at(-6.5, 6.5), at(6.5, -6.5)], st);
        }
        Icon::Solo => {
            // Headphones.
            p.add(Shape::line(arc(at(0.0, 1.0), 6.0 * u, 6.5 * u, PI, 2.0 * PI, 16), st));
            p.rect_filled(Rect::from_min_max(at(-7.0, 1.0), at(-3.5, 7.0)), 1.2 * u, color);
            p.rect_filled(Rect::from_min_max(at(3.5, 1.0), at(7.0, 7.0)), 1.2 * u, color);
        }
        Icon::Equalizer => {
            for (x, knob) in [(-4.5, 2.0), (0.0, -3.0), (4.5, 1.0)] {
                p.line_segment([at(x, -7.0), at(x, 7.0)], st);
                p.rect_filled(Rect::from_center_size(at(x, knob), vec2(3.6 * u, 2.2 * u)), 0.5, color);
            }
        }
        Icon::Bookmark => {
            p.add(Shape::convex_polygon(vec![at(-4.0, -7.0), at(4.0, -7.0), at(4.0, 7.0), at(0.0, 3.5)], color, Stroke::NONE));
            p.add(Shape::convex_polygon(vec![at(-4.0, -7.0), at(0.0, 3.5), at(-4.0, 7.0)], color, Stroke::NONE));
        }
        Icon::Multitrack => {
            // A system bracket beside stacked staves.
            p.add(Shape::line(vec![at(-4.0, -7.0), at(-6.0, -7.0), at(-6.0, 7.0), at(-4.0, 7.0)], st));
            for y in [-4.0, 0.0, 4.0] {
                p.line_segment([at(-3.0, y), at(7.0, y)], Stroke::new(2.2 * u, color));
            }
        }
        Icon::Fretlight => {
            let frame = Rect::from_min_max(at(-4.5, -7.0), at(4.5, 7.0));
            p.rect_stroke(frame, 0.5, thin, egui::StrokeKind::Middle);
            for i in 1..3 {
                let x = -4.5 + i as f32 * 3.0;
                p.line_segment([at(x, -7.0), at(x, 7.0)], thin);
            }
            for i in 1..5 {
                let y = -7.0 + i as f32 * 2.8;
                p.line_segment([at(-4.5, y), at(4.5, y)], thin);
            }
            p.circle_filled(at(-1.5, -2.8), 1.2 * u, color);
            p.circle_filled(at(1.5, 2.8), 1.2 * u, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrument_icons_tell_bass_from_guitar() {
        let mut s = guitario_engine::Session::new();
        s.run("track.add", &serde_json::json!({"name": "Bass", "tuning": "bass"})).unwrap();
        s.run("track.addDrums", &serde_json::json!({})).unwrap();
        let t = &s.score().tracks;
        assert_eq!(instrument(&t[0]), Icon::Guitar);
        assert_eq!(instrument(&t[1]), Icon::Bass);
        assert_eq!(instrument(&t[2]), Icon::Drums);
    }

    const ALL: [Icon; 37] = [
        Icon::Home,
        Icon::Save,
        Icon::Print,
        Icon::Undo,
        Icon::Redo,
        Icon::First,
        Icon::Prev,
        Icon::Play,
        Icon::Stop,
        Icon::Next,
        Icon::Last,
        Icon::Loop,
        Icon::Metronome,
        Icon::CountIn,
        Icon::More,
        Icon::Zoom,
        Icon::Layouts,
        Icon::Palette,
        Icon::Inspector,
        Icon::GlobalView,
        Icon::Fretboard,
        Icon::Keyboard,
        Icon::Drums,
        Icon::Quarter,
        Icon::TuningFork,
        Icon::AudioTrack,
        Icon::Tuner,
        Icon::LineIn,
        Icon::Fretlight,
        Icon::Guitar,
        Icon::Bass,
        Icon::Eye,
        Icon::Mute,
        Icon::Solo,
        Icon::Equalizer,
        Icon::Bookmark,
        Icon::Multitrack,
    ];

    /// Every icon draws something, inside its box, at any size (even a degenerate one).
    #[test]
    fn icons_draw_inside_their_box() {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            for size in [0.0, 8.0, 24.0, 64.0] {
                let r = Rect::from_min_size(pos2(100.0, 100.0), vec2(size, size));
                for icon in ALL {
                    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new((icon as u8, size as u32)));
                    let p = ui.ctx().layer_painter(layer);
                    draw(&p, r, icon, Color32::WHITE);
                }
            }
        })
        .textures_delta
        .clear();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let r = Rect::from_min_size(pos2(100.0, 100.0), vec2(32.0, 32.0));
            for icon in ALL {
                let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new(("one", icon as u8)));
                let p = ui.ctx().layer_painter(layer);
                draw(&p, r, icon, Color32::WHITE);
            }
        });
        out.textures_delta.clear();
        let mut shapes = 0;
        for clipped in &out.shapes {
            let b = clipped.shape.visual_bounding_rect();
            if b.is_positive() {
                shapes += 1;
                assert!(Rect::from_min_size(pos2(100.0, 100.0), vec2(32.0, 32.0)).expand(4.0).contains_rect(b), "{b:?}");
            }
        }
        assert!(shapes >= ALL.len(), "{shapes} shapes");
    }
}
