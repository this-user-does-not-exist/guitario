//! Toolbar icons drawn with the painter, so they need no icon font and scale cleanly.

use egui::{Color32, Painter, Rect, Shape, Stroke, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    New,
    Open,
    Save,
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
    Palette,
    Inspector,
    GlobalView,
    Fretboard,
    Notation,
}

pub fn draw(p: &Painter, r: Rect, icon: Icon, color: Color32) {
    let c = r.center();
    let u = r.height().min(r.width()) / 16.0;
    let st = Stroke::new(1.4 * u.max(0.8), color);
    let at = |x: f32, y: f32| c + vec2(x * u, y * u);
    match icon {
        Icon::New => {
            p.add(Shape::closed_line(vec![at(-4.5, -6.0), at(2.0, -6.0), at(4.5, -3.5), at(4.5, 6.0), at(-4.5, 6.0)], st));
            p.line_segment([at(2.0, -6.0), at(2.0, -3.5)], st);
            p.line_segment([at(2.0, -3.5), at(4.5, -3.5)], st);
        }
        Icon::Open => {
            p.add(Shape::closed_line(vec![at(-6.0, -4.5), at(-2.0, -4.5), at(-1.0, -3.0), at(6.0, -3.0), at(6.0, 5.0), at(-6.0, 5.0)], st));
            p.line_segment([at(-6.0, -1.0), at(6.0, -1.0)], st);
        }
        Icon::Save => {
            p.rect_stroke(Rect::from_min_max(at(-5.5, -5.5), at(5.5, 5.5)), 1.0, st, egui::StrokeKind::Middle);
            p.rect_stroke(Rect::from_min_max(at(-3.0, -5.5), at(3.0, -1.5)), 0.0, st, egui::StrokeKind::Middle);
            p.rect_filled(Rect::from_min_max(at(-3.0, 1.5), at(3.0, 5.5)), 0.0, color);
        }
        Icon::Undo | Icon::Redo => {
            let f = if icon == Icon::Undo { 1.0 } else { -1.0 };
            let pts: Vec<_> = (0..=12)
                .map(|i| {
                    let a = std::f32::consts::PI * (i as f32 / 12.0);
                    at(-f * 4.5 * a.cos(), 1.0 - 4.5 * a.sin())
                })
                .collect();
            p.add(Shape::line(pts, st));
            p.add(Shape::convex_polygon(
                vec![at(f * 4.5 - f * 2.5, 1.0), at(f * 4.5 + f * 2.5, 1.0), at(f * 4.5, 4.5)].into_iter().map(|q| q - vec2(0.0, 0.0)).collect(),
                color,
                Stroke::NONE,
            ));
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(vec![at(-4.0, -6.0), at(6.0, 0.0), at(-4.0, 6.0)], color, Stroke::NONE));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(10.0 * u, 10.0 * u)), 1.0, color);
        }
        Icon::First | Icon::Last => {
            let f = if icon == Icon::First { 1.0 } else { -1.0 };
            p.rect_filled(Rect::from_center_size(at(-f * 5.0, 0.0), vec2(1.8 * u, 10.0 * u)), 0.0, color);
            p.add(Shape::convex_polygon(vec![at(f * 5.0, -5.0), at(f * 5.0, 5.0), at(-f * 3.0, 0.0)], color, Stroke::NONE));
        }
        Icon::Prev | Icon::Next => {
            let f = if icon == Icon::Prev { 1.0 } else { -1.0 };
            for dx in [0.0, 5.0] {
                p.add(Shape::convex_polygon(
                    vec![at(f * (1.0 + dx) - f * 1.0, -5.0), at(f * (1.0 + dx) - f * 1.0, 5.0), at(f * (dx - 5.0), 0.0)],
                    color,
                    Stroke::NONE,
                ));
            }
        }
        Icon::Loop => {
            let pts: Vec<_> = (0..=20)
                .map(|i| {
                    let a = std::f32::consts::TAU * (0.1 + 0.8 * i as f32 / 20.0);
                    at(5.5 * a.cos(), 4.0 * a.sin())
                })
                .collect();
            p.add(Shape::line(pts, st));
            p.add(Shape::convex_polygon(vec![at(3.0, -5.5), at(6.5, -3.0), at(2.5, -1.0)], color, Stroke::NONE));
        }
        Icon::Metronome => {
            p.add(Shape::closed_line(vec![at(-2.0, -6.0), at(2.0, -6.0), at(5.0, 6.0), at(-5.0, 6.0)], st));
            p.line_segment([at(0.0, 3.0), at(4.0, -5.0)], st);
        }
        Icon::CountIn => {
            p.text(c, egui::Align2::CENTER_CENTER, "1·2·3", egui::FontId::proportional(8.5 * u), color);
        }
        Icon::Palette | Icon::Inspector | Icon::GlobalView => {
            let frame = Rect::from_min_max(at(-6.5, -5.5), at(6.5, 5.5));
            p.rect_stroke(frame, 1.0, st, egui::StrokeKind::Middle);
            let fill = match icon {
                Icon::Palette => Rect::from_min_max(frame.min, pos2(frame.left() + 4.0 * u, frame.bottom())),
                Icon::Inspector => Rect::from_min_max(pos2(frame.right() - 4.0 * u, frame.top()), frame.max),
                _ => Rect::from_min_max(pos2(frame.left(), frame.bottom() - 3.5 * u), frame.max),
            };
            p.rect_filled(fill, 0.0, color);
        }
        Icon::Fretboard => {
            for i in 0..4 {
                let y = -4.5 + i as f32 * 3.0;
                p.line_segment([at(-7.0, y), at(7.0, y)], Stroke::new(0.9 * u.max(0.8), color));
            }
            for x in [-3.0, 1.5, 5.5] {
                p.line_segment([at(x, -5.5), at(x, 5.5)], st);
            }
            p.rect_filled(Rect::from_center_size(at(-7.0, 0.0), vec2(1.6 * u, 11.0 * u)), 0.0, color);
        }
        Icon::Notation => {
            for i in 0..5 {
                let y = -4.0 + i as f32 * 2.0;
                p.line_segment([at(-7.0, y), at(7.0, y)], Stroke::new(0.8, color));
            }
            p.circle_filled(at(-1.0, 3.0), 1.6 * u, color);
            p.line_segment([at(0.5, 3.0), at(0.5, -6.0)], st);
        }
    }
}

/// A flat icon button in Guitar Pro's style: no frame until hovered, accent when toggled on.
pub fn button(ui: &mut egui::Ui, icon: Icon, size: f32, on: bool, enabled: bool, tip: &str) -> egui::Response {
    let t = crate::theme::tokens();
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    let p = ui.painter();
    if on {
        p.rect_filled(rect, 4.0, t.accent.gamma_multiply(0.45));
    } else if resp.hovered() && enabled {
        p.rect_filled(rect, 4.0, t.control_hover);
    }
    let color = if !enabled {
        t.text_dim.gamma_multiply(0.5)
    } else if on {
        Color32::WHITE
    } else {
        t.text
    };
    draw(p, rect.shrink(size * 0.18), icon, color);
    resp.on_hover_text(tip)
}
