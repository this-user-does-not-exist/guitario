//! The whammy bar: how far the bridge is pushed or pulled while a beat rings (Effects › Whammy Bar…).
//!
//! The same kind of curve as a bend ([`crate::Bend`]), held by the beat instead of a note because
//! the bar moves every string at once. Positions run from 0 (the attack) to [`WHAMMY_END`] (the end
//! of the beat); values are quarter tones from the written pitch, negative for a dive, from
//! [`WHAMMY_MIN`] (an octave down) to [`WHAMMY_MAX`].

use crate::curve::{self, Pt};
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The position of the beat's end on a curve.
pub const WHAMMY_END: u8 = curve::CURVE_END;
/// Most points a curve holds.
pub const WHAMMY_MAX_POINTS: usize = curve::CURVE_MAX_POINTS;
/// Deepest dive: an octave down, in quarter tones.
pub const WHAMMY_MIN: i8 = -24;
/// Highest pull: an octave up, in quarter tones.
pub const WHAMMY_MAX: i8 = 24;

/// One point of a whammy curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhammyPoint {
    /// Along the beat, 0 to [`WHAMMY_END`].
    pub pos: u8,
    /// From the written pitch in quarter tones, [`WHAMMY_MIN`] to [`WHAMMY_MAX`].
    pub value: i8,
}

impl WhammyPoint {
    pub const fn new(pos: u8, value: i8) -> WhammyPoint {
        WhammyPoint { pos, value }
    }
}

/// The shapes Guitar Pro offers in the Whammy Bar window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhammyKind {
    /// From the written pitch to the depth, then held.
    Dive,
    /// To the depth and straight back.
    Dip,
    /// Back to the written pitch from where the beat before left the bar.
    Return,
    /// Kept where the beat before left the bar.
    Hold,
    /// Already pushed when the beat is struck, and held.
    Predive,
    /// Pushed before the strike, then further.
    PrediveDive,
}

impl WhammyKind {
    pub const ALL: [WhammyKind; 6] = [WhammyKind::Dive, WhammyKind::Dip, WhammyKind::Return, WhammyKind::Hold, WhammyKind::Predive, WhammyKind::PrediveDive];

    pub fn name(self) -> &'static str {
        match self {
            WhammyKind::Dive => "dive",
            WhammyKind::Dip => "dip",
            WhammyKind::Return => "return",
            WhammyKind::Hold => "hold",
            WhammyKind::Predive => "predive",
            WhammyKind::PrediveDive => "predive_dive",
        }
    }

    pub fn from_name(name: &str) -> Option<WhammyKind> {
        WhammyKind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// The name the Whammy Bar window shows.
    pub fn label(self) -> &'static str {
        match self {
            WhammyKind::Dive => "Dive",
            WhammyKind::Dip => "Dip",
            WhammyKind::Return => "Return",
            WhammyKind::Hold => "Hold",
            WhammyKind::Predive => "Pre-dive",
            WhammyKind::PrediveDive => "Pre-dive and Dive",
        }
    }
}

/// A whammy curve of 2 to [`WHAMMY_MAX_POINTS`] points whose positions rise from 0 to
/// [`WHAMMY_END`] and which leaves the written pitch somewhere. Built only through the
/// constructors, so it is always valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Whammy {
    len: u8,
    pts: [WhammyPoint; WHAMMY_MAX_POINTS],
}

impl Whammy {
    /// A curve from points in any order and range (see [`crate::Bend::from_points`]). `None` when
    /// the pitch never leaves the written one.
    pub fn from_points(points: &[WhammyPoint]) -> Option<Whammy> {
        Whammy::from_pts(&points.iter().map(|p| (p.pos, i16::from(p.value))).collect::<Vec<_>>())
    }

    fn from_pts(points: &[Pt]) -> Option<Whammy> {
        let range = i16::from(WHAMMY_MIN)..=i16::from(WHAMMY_MAX);
        let out = curve::tidy(points.iter().map(|&(pos, v)| (pos, v.clamp(*range.start(), *range.end()))));
        if out.iter().all(|p| p.1 == 0) {
            return None;
        }
        let mut pts = [WhammyPoint::new(0, 0); WHAMMY_MAX_POINTS];
        for (slot, &(pos, value)) in pts.iter_mut().zip(&out) {
            *slot = WhammyPoint::new(pos, i8::try_from(value).unwrap_or(0));
        }
        Some(Whammy { len: u8::try_from(out.len()).unwrap_or(0), pts })
    }

    fn pts(&self) -> Vec<Pt> {
        self.points().iter().map(|p| (p.pos, i16::from(p.value))).collect()
    }

    /// A curve of `kind` reaching `depth` quarter tones (negative dives, positive pulls; 0 is taken
    /// as a whole-tone dive). A pre-dive that goes further needs at least two quarter tones.
    pub fn preset(kind: WhammyKind, depth: i8) -> Whammy {
        let d = if depth == 0 { -4 } else { depth.clamp(WHAMMY_MIN, WHAMMY_MAX) };
        let shape: &[(u8, i8)] = match kind {
            WhammyKind::Dive => &[(0, 0), (45, d), (WHAMMY_END, d)],
            WhammyKind::Dip => &[(0, 0), (30, d), (WHAMMY_END, 0)],
            WhammyKind::Return => &[(0, d), (45, 0), (WHAMMY_END, 0)],
            WhammyKind::Hold | WhammyKind::Predive => &[(0, d), (WHAMMY_END, d)],
            WhammyKind::PrediveDive => {
                let d = if d.abs() < 2 { d.signum() * 2 } else { d };
                return Whammy::build(&[WhammyPoint::new(0, d / 2), WhammyPoint::new(45, d), WhammyPoint::new(WHAMMY_END, d)]);
            }
        };
        Whammy::build(&shape.iter().map(|&(pos, value)| WhammyPoint::new(pos, value)).collect::<Vec<_>>())
    }

    /// A curve known to be valid: falls back to the whole-tone dive.
    fn build(points: &[WhammyPoint]) -> Whammy {
        Whammy::from_points(points).unwrap_or(Whammy {
            len: 3,
            pts: {
                let mut p = [WhammyPoint::new(0, 0); WHAMMY_MAX_POINTS];
                p[1] = WhammyPoint::new(45, -4);
                p[2] = WhammyPoint::new(WHAMMY_END, -4);
                p
            },
        })
    }

    pub fn points(&self) -> &[WhammyPoint] {
        self.pts.get(..usize::from(self.len)).unwrap_or(&[])
    }

    /// The curve with at most `max` points (at least 2), for formats that hold fewer.
    pub fn thinned(&self, max: usize) -> Vec<WhammyPoint> {
        let mut v = self.pts();
        curve::thin(&mut v, max);
        v.iter().map(|&(pos, value)| WhammyPoint::new(pos, i8::try_from(value).unwrap_or(0))).collect()
    }

    /// The pitch at the attack, in quarter tones.
    pub fn start(&self) -> i8 {
        self.points().first().map_or(0, |p| p.value)
    }

    /// The pitch when the beat ends.
    pub fn end(&self) -> i8 {
        self.points().last().map_or(0, |p| p.value)
    }

    /// The value farthest from the written pitch (the deepest dive, or the highest pull when the
    /// curve never dives).
    pub fn depth(&self) -> i8 {
        self.points().iter().map(|p| p.value).max_by_key(|v| (v.unsigned_abs(), *v < 0)).unwrap_or(0)
    }

    /// The pitch `pos` along the beat (0 to [`WHAMMY_END`]), in quarter tones.
    pub fn value_at(&self, pos: f32) -> f32 {
        curve::value_at(&self.pts(), pos)
    }

    /// The pitch `fraction` of the way through the beat (0 to 1), in semitones.
    pub fn semitones_at(&self, fraction: f32) -> f32 {
        self.value_at(fraction.clamp(0.0, 1.0) * f32::from(WHAMMY_END)) / 2.0
    }

    /// Which of the window's shapes the curve is, or `None` for a free-form curve. A curve that
    /// starts away from the written pitch and stays there is a Hold when it `continues` the beat
    /// before's whammy and a Pre-dive otherwise.
    pub fn kind(&self, continues: bool) -> Option<WhammyKind> {
        let v: Vec<i16> = self.points().iter().map(|p| i16::from(p.value)).collect();
        let (s, e, d) = (i16::from(self.start()), i16::from(self.end()), i16::from(self.depth()));
        // Every point on the depth's side of the written pitch (or on it).
        let one_side = v.iter().all(|&x| x == 0 || x.signum() == d.signum());
        if !one_side {
            return None;
        }
        let away = v.windows(2).all(|w| matches!(w, [a, b] if b.abs() >= a.abs()));
        let back = v.windows(2).all(|w| matches!(w, [a, b] if b.abs() <= a.abs()));
        if s == 0 {
            if away {
                return Some(WhammyKind::Dive);
            }
            let top = v.iter().position(|&x| x == d).unwrap_or(0);
            let out = v.get(..=top).is_some_and(|a| a.windows(2).all(|w| matches!(w, [x, y] if y.abs() >= x.abs())));
            let home = v.get(top..).is_some_and(|a| a.windows(2).all(|w| matches!(w, [x, y] if y.abs() <= x.abs())));
            return (out && home && e == 0).then_some(WhammyKind::Dip);
        }
        if v.iter().all(|&x| x == s) {
            return Some(if continues { WhammyKind::Hold } else { WhammyKind::Predive });
        }
        if away {
            return Some(WhammyKind::PrediveDive);
        }
        (back && e == 0).then_some(WhammyKind::Return)
    }

    /// The same curve at another depth: a preset is rebuilt, a free-form curve is scaled.
    pub fn with_depth(&self, depth: i8) -> Whammy {
        let d = if depth == 0 { -4 } else { depth.clamp(WHAMMY_MIN, WHAMMY_MAX) };
        if let Some(kind) = self.kind(false) {
            return Whammy::preset(kind, d);
        }
        let old = f32::from(self.depth().unsigned_abs().max(1));
        let k = f32::from(d.unsigned_abs()) / old;
        let flip = if (d < 0) == (self.depth() < 0) { 1.0 } else { -1.0 };
        let scaled: Vec<WhammyPoint> = self.points().iter().map(|p| WhammyPoint::new(p.pos, (f32::from(p.value) * k * flip).round() as i8)).collect();
        Whammy::from_points(&scaled).unwrap_or_else(|| Whammy::preset(WhammyKind::Dive, d))
    }

    /// One curve for two beats played as one note (a tie): the first beat's curve over its
    /// length, then the next one's over its own; a beat without a curve holds where the bar was left.
    pub fn join(first: Option<Whammy>, first_ticks: u32, next: Option<Whammy>, next_ticks: u32) -> Option<Whammy> {
        let (a, b) = (first.map(|c| c.pts()), next.map(|c| c.pts()));
        curve::join(a.as_deref(), first_ticks, b.as_deref(), next_ticks).map_or(first, |pts| Whammy::from_pts(&pts))
    }
}

impl Serialize for Whammy {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.points().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Whammy {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Whammy, D::Error> {
        let points = Vec::<WhammyPoint>::deserialize(d)?;
        Whammy::from_points(&points).ok_or_else(|| D::Error::custom("a whammy curve needs a point away from the written pitch"))
    }
}

/// Vibrato played with the whammy bar over a beat: a wavy line after "w/bar".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarVibrato {
    Slight,
    Wide,
}

impl BarVibrato {
    pub fn from_name(name: &str) -> Option<BarVibrato> {
        match name {
            "slight" => Some(BarVibrato::Slight),
            "wide" => Some(BarVibrato::Wide),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pos: u8, value: i8) -> WhammyPoint {
        WhammyPoint::new(pos, value)
    }

    #[test]
    fn points_are_made_valid_and_may_dive() {
        let w = Whammy::from_points(&[p(40, -99), p(10, 2), p(10, 3)]).unwrap();
        assert_eq!(w.points(), &[p(0, 3), p(10, 3), p(40, WHAMMY_MIN), p(60, WHAMMY_MIN)]);
        assert!(Whammy::from_points(&[]).is_none());
        assert!(Whammy::from_points(&[p(0, 0), p(60, 0)]).is_none(), "a curve that never moves is no whammy");
        assert_eq!(Whammy::from_points(&[p(0, 0), p(60, 100)]).unwrap().end(), WHAMMY_MAX);
    }

    #[test]
    fn the_shapes_have_their_curves_and_are_recognised() {
        for depth in [-8, -1, 3, -24] {
            for (kind, continues) in [
                (WhammyKind::Dive, false),
                (WhammyKind::Dip, false),
                (WhammyKind::Return, true),
                (WhammyKind::Hold, true),
                (WhammyKind::Predive, false),
                (WhammyKind::PrediveDive, false),
            ] {
                let w = Whammy::preset(kind, depth);
                assert_eq!(w.kind(continues), Some(kind), "{kind:?} {depth}");
                assert_eq!(w.depth().signum(), depth.signum(), "{kind:?} {depth}");
            }
        }
        let dive = Whammy::preset(WhammyKind::Dive, -8);
        assert_eq!((dive.start(), dive.end(), dive.depth()), (0, -8, -8));
        assert_eq!(dive.semitones_at(1.0), -4.0, "two whole tones down");
        assert_eq!(Whammy::preset(WhammyKind::Dip, -6).end(), 0);
        assert_eq!(Whammy::preset(WhammyKind::Predive, -4).kind(true), Some(WhammyKind::Hold));
        assert_eq!(Whammy::preset(WhammyKind::Dive, 0).depth(), -4, "no depth is a whole-tone dive");
        let pd = Whammy::preset(WhammyKind::PrediveDive, -1);
        assert!(pd.depth() < pd.start() && pd.start() < 0, "a pre-dive of one quarter tone still dives further");
        assert_eq!(Whammy::from_points(&[p(0, 0), p(20, -4), p(40, 4), p(60, 0)]).unwrap().kind(false), None, "both sides is free-form");
        for k in WhammyKind::ALL {
            assert_eq!(WhammyKind::from_name(k.name()), Some(k));
        }
        assert_eq!(WhammyKind::from_name("bomb"), None);
    }

    #[test]
    fn the_depth_changes_and_the_shape_stays() {
        assert_eq!(Whammy::preset(WhammyKind::Dip, -4).with_depth(-12), Whammy::preset(WhammyKind::Dip, -12));
        assert_eq!(Whammy::preset(WhammyKind::Dive, -4).with_depth(2), Whammy::preset(WhammyKind::Dive, 2), "a dive can become a pull");
        let free = Whammy::from_points(&[p(0, 0), p(20, -4), p(40, -2), p(60, -6)]).unwrap();
        let deep = free.with_depth(-12);
        assert_eq!((deep.depth(), deep.points().len()), (-12, free.points().len()));
        assert_eq!(free.with_depth(6).depth(), 6, "flipped to a pull");
    }

    #[test]
    fn saved_as_points_and_hostile_input_is_an_error() {
        let w = Whammy::preset(WhammyKind::Dip, -6);
        let text = serde_json::to_string(&w).unwrap();
        assert_eq!(serde_json::from_str::<Whammy>(&text).unwrap(), w);
        for bad in ["0", "[]", "[{\"pos\":0,\"value\":0}]", "\"x\"", "[{\"pos\":300,\"value\":-1}]", "[{\"pos\":1,\"value\":-200}]"] {
            assert!(serde_json::from_str::<Whammy>(bad).is_err(), "{bad}");
        }
        assert_eq!(serde_json::to_string(&BarVibrato::Wide).unwrap(), "\"wide\"");
    }

    #[test]
    fn a_tie_carries_the_curve_on() {
        let q = 480;
        let held = Whammy::join(Some(Whammy::preset(WhammyKind::Dive, -8)), q, None, q).unwrap();
        assert_eq!((held.value_at(0.0), held.value_at(60.0)), (0.0, -8.0));
        let back = Whammy::join(Some(Whammy::preset(WhammyKind::Dive, -8)), q, Some(Whammy::preset(WhammyKind::Return, -8)), q).unwrap();
        assert_eq!((back.value_at(30.0), back.value_at(60.0)), (-8.0, 0.0));
        assert_eq!(Whammy::join(None, q, None, q), None);
    }

    #[test]
    fn thinning_keeps_the_ends() {
        let w = Whammy::from_points(&[p(0, 0), p(10, -3), p(20, -3), p(30, -6), p(40, -2), p(50, -2), p(60, -8)]).unwrap();
        let four = w.thinned(4);
        assert_eq!((four.len(), four.first().map(|x| x.pos), four.last().map(|x| x.pos)), (4, Some(0), Some(60)));
    }
}
