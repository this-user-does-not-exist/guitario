//! The whammy bar in its lane above the tab: "w/bar" at the start of a run, then the curve as a
//! line that drops for a dive and rises for a pull, from a rest line, with the depth written where
//! the line turns (−½, −1, −2½ …). Vibrato with the bar is a wavy line on the rest line.

use egui::{Color32, FontId, Painter, Stroke, pos2, vec2};
use guitario_score::{WHAMMY_END, WHAMMY_MAX, Whammy};

/// Where a beat's whammy is drawn: from `x0` to `x1`, the rest line at `y`, a full-octave dive or
/// pull `depth` below or above it.
pub(crate) struct WhammyBox {
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
    pub depth: f32,
    /// The beat before left the bar where this curve starts, so no drop from the rest line.
    pub continues: bool,
}

/// What Guitar Pro writes at a whammy point of `quarter_tones`: −¼, −½, −1, −1½ … +½.
pub(crate) fn whammy_label(quarter_tones: i8) -> String {
    let q = quarter_tones.unsigned_abs();
    if q == 0 {
        return "0".to_string();
    }
    let sign = if quarter_tones < 0 { "-" } else { "+" };
    let frac = match q % 4 {
        1 => "¼",
        2 => "½",
        3 => "¾",
        _ => "",
    };
    match q / 4 {
        0 => format!("{sign}{frac}"),
        w => format!("{sign}{w}{frac}"),
    }
}

/// How far from the rest line `quarter_tones` is drawn, as a share of `depth` (down positive).
fn offset(quarter_tones: i8) -> f32 {
    let share = (f32::from(quarter_tones.unsigned_abs()) / f32::from(WHAMMY_MAX.unsigned_abs())).sqrt().clamp(0.0, 1.0);
    if quarter_tones < 0 { share } else { -share }
}

/// Paints "w/bar" with its right edge at `x`, on the rest line `y`.
pub(crate) fn paint_tag(p: &Painter, x: f32, y: f32, font: FontId, ink: Color32) {
    let galley = p.layout_no_wrap("w/bar".to_string(), font, ink);
    p.galley(pos2(x - galley.size().x, y - galley.size().y / 2.0), galley, ink);
}

/// Paints `w` into `b`: the curve and the depth at every turn and at the end.
pub(crate) fn paint_whammy(p: &Painter, w: &Whammy, b: &WhammyBox, stroke: Stroke, ink: Color32, font: FontId) {
    let x_of = |pos: u8| b.x0 + (b.x1 - b.x0) * f32::from(pos) / f32::from(WHAMMY_END);
    let y_of = |q: i8| b.y + b.depth * offset(q);
    let pts = w.points();
    let mut line: Vec<egui::Pos2> = pts.iter().map(|pt| pos2(x_of(pt.pos), y_of(pt.value))).collect();
    if w.start() != 0 && !b.continues {
        // Already pushed when struck: straight down (or up) from the rest line.
        line.insert(0, pos2(b.x0, b.y));
    }
    p.add(egui::Shape::line(line, stroke));
    // The depth where the line turns or levels off, and at its end unless back at rest.
    let mut last_label = if b.continues { Some(w.start()) } else { None };
    for (i, pt) in pts.iter().enumerate() {
        let prev = i.checked_sub(1).and_then(|j| pts.get(j));
        let next = pts.get(i + 1);
        let turns = match (prev, next) {
            (Some(a), Some(c)) => (pt.value - a.value).signum() != (c.value - pt.value).signum(),
            (None, Some(_)) => pt.value != 0,
            (_, None) => true,
        };
        if !turns || pt.value == 0 || last_label == Some(pt.value) {
            continue;
        }
        last_label = Some(pt.value);
        let at = pos2(x_of(pt.pos), y_of(pt.value));
        let (align, nudge) = if pt.value < 0 { (egui::Align2::CENTER_TOP, vec2(0.0, 1.0)) } else { (egui::Align2::CENTER_BOTTOM, vec2(0.0, -1.0)) };
        p.text(at + nudge, align, whammy_label(pt.value), font.clone(), ink);
    }
}

/// Paints vibrato with the bar from `x0` to `x1` on the rest line `y`.
pub(crate) fn paint_bar_vibrato(p: &Painter, x0: f32, x1: f32, y: f32, amp: f32, stroke: Stroke) {
    crate::glyphs::wavy(p, x0, x1, y, amp, stroke);
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::WhammyKind;

    #[test]
    fn labels_follow_guitar_pro() {
        let all: Vec<String> = [-1, -2, -3, -4, -5, -6, -8, -24, 2, 4].into_iter().map(whammy_label).collect();
        assert_eq!(all, ["-¼", "-½", "-¾", "-1", "-1¼", "-1½", "-2", "-6", "+½", "+1"]);
        assert_eq!(whammy_label(0), "0");
    }

    #[test]
    fn deeper_dives_are_drawn_lower_and_pulls_higher_within_the_lane() {
        let mut last = -1.0;
        for q in (WHAMMY_MAX.saturating_neg()..=0).rev() {
            let o = offset(q);
            assert!(o >= last && (0.0..=1.0).contains(&o), "{q}");
            last = o;
        }
        assert!(offset(4) < 0.0 && offset(WHAMMY_MAX) >= -1.0);
    }

    fn texts(w: &Whammy, continues: bool) -> Vec<String> {
        let ctx = egui::Context::default();
        let mut texts = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = ui.painter().clone();
            let b = WhammyBox { x0: 10.0, x1: 60.0, y: 50.0, depth: 20.0, continues };
            paint_whammy(&p, w, &b, Stroke::new(1.0, Color32::WHITE), Color32::WHITE, FontId::proportional(8.0));
        });
        out.textures_delta.clear();
        fn walk(s: &egui::Shape, texts: &mut Vec<String>) {
            match s {
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, texts)),
                egui::Shape::Text(t) => texts.push(t.galley.text().to_string()),
                _ => {}
            }
        }
        for c in &out.shapes {
            walk(&c.shape, &mut texts);
        }
        texts
    }

    #[test]
    fn each_shape_is_labelled_where_it_turns() {
        assert_eq!(texts(&Whammy::preset(WhammyKind::Dive, -4), false), ["-1"]);
        assert_eq!(texts(&Whammy::preset(WhammyKind::Dip, -6), false), ["-1½"], "the dip's bottom; back at rest needs no label");
        assert_eq!(texts(&Whammy::preset(WhammyKind::Predive, -2), false), ["-½"]);
        assert!(texts(&Whammy::preset(WhammyKind::Hold, -2), true).is_empty(), "a hold repeats nothing");
        assert!(texts(&Whammy::preset(WhammyKind::Return, -8), true).is_empty());
        assert_eq!(texts(&Whammy::preset(WhammyKind::PrediveDive, -8), false), ["-1", "-2"]);
        assert_eq!(texts(&Whammy::preset(WhammyKind::Dive, 2), false), ["+½"]);
    }
}
