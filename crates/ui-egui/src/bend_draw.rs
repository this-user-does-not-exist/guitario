//! Bend arrows in the tab: a curve for every rise, a downward arrow for every release, a vertical
//! arrow for a pre-bend, a plain line for a hold, and the height written over each peak
//! (¼, ½, ¾, full, 1¼ … 3).

use egui::{Color32, FontId, Painter, Stroke, pos2, vec2};
use guitario_score::{BEND_END, Bend};

/// Where a bend is drawn: from `x0` (just right of the fret number) to `x1` (the next note), on the
/// string line `y`, rising no higher than `top`.
pub(crate) struct BendBox {
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
    pub top: f32,
    /// One drawing unit (the zoom).
    pub s: f32,
    /// The note continues a bent one, so a start above the string needs no arrow of its own.
    pub tied: bool,
}

/// What Guitar Pro writes over a bend of `quarter_tones`: ¼, ½, ¾, full, 1¼ … 3.
pub(crate) fn bend_label(quarter_tones: u8) -> String {
    let whole = quarter_tones / 4;
    let frac = match quarter_tones % 4 {
        1 => "¼",
        2 => "½",
        3 => "¾",
        _ => "",
    };
    match (whole, frac) {
        (0, f) => f.to_string(),
        (1, "") => "full".to_string(),
        (w, f) => format!("{w}{f}"),
    }
}

/// How high a pitch of `quarter_tones` is drawn above the string, as a share of the way to `top`.
fn level(quarter_tones: u8) -> f32 {
    (f32::from(quarter_tones) / 12.0).sqrt().clamp(0.0, 1.0)
}

fn arrow_head(p: &Painter, tip: egui::Pos2, up: bool, s: f32, ink: Color32) {
    let d = if up { 1.0 } else { -1.0 };
    p.add(egui::Shape::convex_polygon(
        vec![tip + vec2(0.0, -0.6 * s * d), tip + vec2(-0.4 * s, 0.2 * s * d), tip + vec2(0.4 * s, 0.2 * s * d)],
        ink,
        Stroke::NONE,
    ));
}

/// Paints `bend` into `b`.
pub(crate) fn paint_bend(p: &Painter, bend: &Bend, b: &BendBox, stroke: Stroke, ink: Color32, font: FontId) {
    let y_of = |q: u8| b.y - (b.y - b.top) * level(q);
    let start = bend.start();
    let pre = start > 0 && !b.tied;
    let x_from = if pre { b.x0 + 2.5 * b.s } else { b.x0 };
    let x_to = b.x1.max(x_from + 6.0 * b.s).min(x_from + 34.0 * b.s);
    let x_of = |pos: u8| x_from + (x_to - x_from) * f32::from(pos) / f32::from(BEND_END);
    if pre {
        // Already pulled when the note is struck: straight up from the fret.
        let top = pos2(b.x0, y_of(start));
        p.line_segment([pos2(b.x0, b.y - 0.3 * b.s), top], stroke);
        arrow_head(p, top, true, b.s, ink);
        p.text(top - vec2(0.0, 0.8 * b.s), egui::Align2::CENTER_BOTTOM, bend_label(start), font.clone(), ink);
    }
    for w in bend.points().windows(2) {
        let [a, c] = w else { continue };
        let (pa, pc) = (pos2(x_of(a.pos), y_of(a.value)), pos2(x_of(c.pos), y_of(c.value)));
        if a.value == c.value {
            // Held: a straight line, and the start of a tied note draws one from the fret.
            let from = if a.pos == 0 && b.tied { pos2(b.x0, pa.y) } else { pa };
            p.line_segment([from, pc], stroke);
            continue;
        }
        // Starts level, ends upright: a rise or a release.
        p.add(egui::epaint::QuadraticBezierShape::from_points_stroke([pa, pos2(pc.x, pa.y), pc], false, Color32::TRANSPARENT, stroke));
        let up = c.value > a.value;
        arrow_head(p, pc, up, b.s, ink);
        if up {
            p.text(pc - vec2(0.0, 0.8 * b.s), egui::Align2::CENTER_BOTTOM, bend_label(c.value), font.clone(), ink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::BendKind;

    #[test]
    fn labels_follow_guitar_pro() {
        let all: Vec<String> = (1..=12).map(bend_label).collect();
        assert_eq!(all, ["¼", "½", "¾", "full", "1¼", "1½", "1¾", "2", "2¼", "2½", "2¾", "3"]);
        assert_eq!(bend_label(0), "");
    }

    #[test]
    fn higher_bends_are_drawn_higher_and_never_above_the_top() {
        let mut last = -1.0;
        for q in 0..=12 {
            let l = level(q);
            assert!(l >= last && (0.0..=1.0).contains(&l));
            last = l;
        }
    }

    /// Shapes and texts a bend paints.
    fn painted(bend: &Bend, tied: bool) -> (usize, Vec<String>) {
        let ctx = egui::Context::default();
        let mut shapes = 0;
        let mut texts = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = ui.painter().clone();
            let b = BendBox { x0: 10.0, x1: 60.0, y: 100.0, top: 60.0, s: 1.0, tied };
            paint_bend(&p, bend, &b, Stroke::new(1.0, Color32::WHITE), Color32::WHITE, FontId::proportional(8.0));
        });
        out.textures_delta.clear();
        for c in &out.shapes {
            fn walk(s: &egui::Shape, n: &mut usize, texts: &mut Vec<String>) {
                match s {
                    egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, n, texts)),
                    egui::Shape::Text(t) => texts.push(t.galley.text().to_string()),
                    _ => *n += 1,
                }
            }
            walk(&c.shape, &mut shapes, &mut texts);
        }
        (shapes, texts)
    }

    #[test]
    fn a_bend_draws_its_arrow_and_its_height() {
        let (shapes, texts) = painted(&Bend::to(4), false);
        assert_eq!(texts, ["full"]);
        assert!(shapes >= 3, "a curve, its head and the held line: {shapes}");
    }

    #[test]
    fn a_pre_bend_draws_an_arrow_from_the_fret_and_a_release_draws_none_up() {
        let (_, texts) = painted(&Bend::preset(BendKind::Prebend, 8), false);
        assert_eq!(texts, ["2"], "the pre-bend is labelled once");
        let (_, texts) = painted(&Bend::preset(BendKind::PrebendRelease, 4), false);
        assert_eq!(texts, ["full"], "the release carries no label");
        let (_, texts) = painted(&Bend::preset(BendKind::Release, 4), true);
        assert!(texts.is_empty(), "a tied release continues a bend, so nothing is labelled: {texts:?}");
        let (_, texts) = painted(&Bend::preset(BendKind::BendRelease, 2), false);
        assert_eq!(texts, ["½"]);
    }

    #[test]
    fn a_hold_on_a_tied_note_is_a_line() {
        let (shapes, texts) = painted(&Bend::preset(BendKind::Hold, 4), true);
        assert!(texts.is_empty());
        assert_eq!(shapes, 1);
    }
}
