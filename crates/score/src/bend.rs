//! Bend curves: how far a string is pulled while a note rings.
//!
//! A curve is a few points along the note. `pos` runs from 0 (the attack) to [`BEND_END`] (the end
//! of the note), the scale Guitar Pro 3-5 use; `value` is the pitch above the fretted note in
//! quarter tones (4 is a full step, [`BEND_MAX`] is three steps). Between points the pitch moves in
//! a straight line. A curve that starts above 0 is a pre-bend: the string is already pulled when
//! the note is struck. A curve is at most [`BEND_MAX_POINTS`] points so a note stays `Copy`.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::curve::{self, Pt};

/// The position of a note's end on a curve.
pub const BEND_END: u8 = curve::CURVE_END;
/// Most points a curve holds.
pub const BEND_MAX_POINTS: usize = curve::CURVE_MAX_POINTS;
/// Highest bend: three whole tones, in quarter tones.
pub const BEND_MAX: u8 = 12;

/// One point of a curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BendPoint {
    /// Along the note, 0 to [`BEND_END`].
    pub pos: u8,
    /// Above the fretted pitch in quarter tones, 0 to [`BEND_MAX`].
    pub value: u8,
}

impl BendPoint {
    pub const fn new(pos: u8, value: u8) -> BendPoint {
        BendPoint { pos, value }
    }
}

/// Guitar Pro's seven bend types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BendKind {
    /// Pulled up from the fretted pitch and held.
    Bend,
    /// Let down to the fretted pitch from a bend held by the note this one is tied to.
    Release,
    /// Pulled up, then let down.
    BendRelease,
    /// Kept bent, for a note tied to a bent one.
    Hold,
    /// Already pulled when the note is struck.
    Prebend,
    /// Pulled before the strike, then pulled further.
    PrebendBend,
    /// Pulled before the strike, then let down.
    PrebendRelease,
}

impl BendKind {
    pub const ALL: [BendKind; 7] =
        [BendKind::Bend, BendKind::Release, BendKind::BendRelease, BendKind::Hold, BendKind::Prebend, BendKind::PrebendBend, BendKind::PrebendRelease];

    pub fn name(self) -> &'static str {
        match self {
            BendKind::Bend => "bend",
            BendKind::Release => "release",
            BendKind::BendRelease => "bend_release",
            BendKind::Hold => "hold",
            BendKind::Prebend => "prebend",
            BendKind::PrebendBend => "prebend_bend",
            BendKind::PrebendRelease => "prebend_release",
        }
    }

    pub fn from_name(name: &str) -> Option<BendKind> {
        BendKind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// The name the Bend window shows.
    pub fn label(self) -> &'static str {
        match self {
            BendKind::Bend => "Bend",
            BendKind::Release => "Release",
            BendKind::BendRelease => "Bend/Release",
            BendKind::Hold => "Hold",
            BendKind::Prebend => "Prebend",
            BendKind::PrebendBend => "Prebend/Bend",
            BendKind::PrebendRelease => "Prebend/Release",
        }
    }
}

/// A bend curve of 2 to [`BEND_MAX_POINTS`] points whose positions rise from 0 to [`BEND_END`] and
/// whose highest value is above 0. Built only through the constructors, so it is always valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bend {
    len: u8,
    pts: [BendPoint; BEND_MAX_POINTS],
}

impl Bend {
    /// A curve from points in any order and range: positions and values are clamped, points at
    /// one position keep the last, the ends are added at 0 and [`BEND_END`], straight runs lose
    /// their middle points and the least important points go when more than
    /// [`BEND_MAX_POINTS`] remain. `None` when the pitch never leaves the fretted one.
    pub fn from_points(points: &[BendPoint]) -> Option<Bend> {
        let out = curve::tidy(points.iter().map(|p| (p.pos, i16::from(p.value.min(BEND_MAX)))));
        if out.iter().all(|p| p.1 == 0) {
            return None;
        }
        let mut pts = [BendPoint::new(0, 0); BEND_MAX_POINTS];
        for (slot, &(pos, value)) in pts.iter_mut().zip(&out) {
            *slot = BendPoint::new(pos, u8::try_from(value).unwrap_or(0));
        }
        Some(Bend { len: u8::try_from(out.len()).unwrap_or(0), pts })
    }

    fn pts(&self) -> Vec<Pt> {
        self.points().iter().map(|p| (p.pos, i16::from(p.value))).collect()
    }

    fn from_pts(pts: &[Pt]) -> Option<Bend> {
        let v: Vec<BendPoint> = pts.iter().map(|&(pos, value)| BendPoint::new(pos, u8::try_from(value.max(0)).unwrap_or(BEND_MAX))).collect();
        Bend::from_points(&v)
    }

    /// The plain bend: up to `quarter_tones` by the middle of the note and held. At least a
    /// quarter tone and at most [`BEND_MAX`].
    pub fn to(quarter_tones: u8) -> Bend {
        Bend::shape(&[(0, 0), (30, 1), (BEND_END, 1)], quarter_tones.clamp(1, BEND_MAX))
    }

    /// A curve of Guitar Pro's `kind` reaching `quarter_tones` (1 to [`BEND_MAX`]; a pre-bend that
    /// is pulled further needs at least 2).
    pub fn preset(kind: BendKind, quarter_tones: u8) -> Bend {
        let q = quarter_tones.clamp(1, BEND_MAX);
        match kind {
            BendKind::Bend => Bend::to(q),
            BendKind::Release => Bend::shape(&[(0, 1), (30, 0), (BEND_END, 0)], q),
            BendKind::BendRelease => Bend::shape(&[(0, 0), (30, 1), (BEND_END, 0)], q),
            BendKind::Hold | BendKind::Prebend => Bend::shape(&[(0, 1), (BEND_END, 1)], q),
            BendKind::PrebendBend => {
                let q = q.max(2);
                let pre = (q / 2).max(1);
                Bend::build(&[BendPoint::new(0, pre), BendPoint::new(30, q), BendPoint::new(BEND_END, q)])
            }
            BendKind::PrebendRelease => Bend::shape(&[(0, 1), (30, 1), (BEND_END, 0)], q),
        }
    }

    /// Points whose value is 0 or 1 scaled to `q`.
    fn shape(points: &[(u8, u8)], q: u8) -> Bend {
        let pts: Vec<BendPoint> = points.iter().map(|&(pos, unit)| BendPoint::new(pos, unit * q)).collect();
        Bend::build(&pts)
    }

    /// A curve known to be valid: falls back to the plain full-step bend.
    fn build(points: &[BendPoint]) -> Bend {
        Bend::from_points(points).unwrap_or(Bend {
            len: 3,
            pts: {
                let mut p = [BendPoint::new(0, 0); BEND_MAX_POINTS];
                p[1] = BendPoint::new(30, 4);
                p[2] = BendPoint::new(BEND_END, 4);
                p
            },
        })
    }

    pub fn points(&self) -> &[BendPoint] {
        self.pts.get(..usize::from(self.len)).unwrap_or(&[])
    }

    /// The curve with at most `max` points (at least 2): the points that matter least go first.
    /// For formats that hold fewer points than a note can have.
    pub fn thinned(&self, max: usize) -> Vec<BendPoint> {
        let mut v = self.pts();
        curve::thin(&mut v, max);
        v.iter().map(|&(pos, value)| BendPoint::new(pos, u8::try_from(value).unwrap_or(0))).collect()
    }

    /// The pitch at the attack, in quarter tones.
    pub fn start(&self) -> u8 {
        self.points().first().map_or(0, |p| p.value)
    }

    /// The pitch when the note ends.
    pub fn end(&self) -> u8 {
        self.points().last().map_or(0, |p| p.value)
    }

    /// The highest pitch the curve reaches.
    pub fn peak(&self) -> u8 {
        self.points().iter().map(|p| p.value).max().unwrap_or(0)
    }

    /// The pitch `pos` along the note (0 to [`BEND_END`]), in quarter tones.
    pub fn value_at(&self, pos: f32) -> f32 {
        curve::value_at(&self.pts(), pos)
    }

    /// The pitch `fraction` of the way through the note (0 to 1), in semitones.
    pub fn semitones_at(&self, fraction: f32) -> f32 {
        self.value_at(fraction.clamp(0.0, 1.0) * f32::from(BEND_END)) / 2.0
    }

    /// The height of the plain bend this curve is, if it is exactly that.
    pub fn plain_height(&self) -> Option<u8> {
        let q = self.peak();
        (*self == Bend::to(q)).then_some(q)
    }

    /// Which of Guitar Pro's seven types the curve is, or `None` for a free-form curve. A curve
    /// that starts and stays bent is a Hold when `tied` (the note continues a bent one) and a
    /// Prebend otherwise; one that starts bent and is let down within the first half is a Release
    /// when `tied` and a Prebend/Release otherwise.
    pub fn kind(&self, tied: bool) -> Option<BendKind> {
        let v: Vec<u8> = self.points().iter().map(|p| p.value).collect();
        let rising = v.windows(2).all(|w| matches!(w, [a, b] if a <= b));
        let falling = v.windows(2).all(|w| matches!(w, [a, b] if a >= b));
        let (s, e, p) = (self.start(), self.end(), self.peak());
        if s == 0 {
            if rising {
                return Some(BendKind::Bend);
            }
            // Up to the peak, then down to where it began.
            let top = v.iter().position(|&x| x == p).unwrap_or(0);
            let up = v.get(..=top).is_some_and(|a| a.windows(2).all(|w| matches!(w, [x, y] if x <= y)));
            let down = v.get(top..).is_some_and(|a| a.windows(2).all(|w| matches!(w, [x, y] if x >= y)));
            return (up && down && e == 0).then_some(BendKind::BendRelease);
        }
        if v.iter().all(|&x| x == s) {
            return Some(if tied { BendKind::Hold } else { BendKind::Prebend });
        }
        if rising {
            return Some(BendKind::PrebendBend);
        }
        if falling && e == 0 {
            let down_by = self.points().iter().find(|pt| pt.value == 0).map_or(BEND_END, |pt| pt.pos);
            return Some(if down_by <= BEND_END / 2 && tied { BendKind::Release } else { BendKind::PrebendRelease });
        }
        None
    }

    /// The same curve at another height: a preset is rebuilt, a free-form curve is scaled.
    pub fn with_peak(&self, quarter_tones: u8) -> Bend {
        let q = quarter_tones.clamp(1, BEND_MAX);
        if let Some(kind) = self.kind(false) {
            return Bend::preset(kind, q);
        }
        let old = f32::from(self.peak().max(1));
        let scaled: Vec<BendPoint> = self.points().iter().map(|p| BendPoint::new(p.pos, (f32::from(p.value) * f32::from(q) / old).round() as u8)).collect();
        Bend::from_points(&scaled).unwrap_or_else(|| Bend::to(q))
    }

    /// One curve for a note held through a tie: the first note's curve over its length, then the
    /// next note's over its own. A note without a curve holds where the one before left the string
    /// (a first note without one starts straight).
    pub fn join(first: Option<Bend>, first_ticks: u32, next: Option<Bend>, next_ticks: u32) -> Option<Bend> {
        let (a, b) = (first.map(|c| c.pts()), next.map(|c| c.pts()));
        curve::join(a.as_deref(), first_ticks, b.as_deref(), next_ticks).map_or(first, |pts| Bend::from_pts(&pts))
    }
}

/// A plain bend is saved as its height (as older files hold it), anything else as its points.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Saved {
    Height(u8),
    Points(Vec<BendPoint>),
}

impl Serialize for Bend {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.plain_height() {
            Some(q) => Saved::Height(q).serialize(s),
            None => Saved::Points(self.points().to_vec()).serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for Bend {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Bend, D::Error> {
        match Saved::deserialize(d)? {
            Saved::Height(0) => Err(D::Error::custom("a bend needs a height above 0")),
            Saved::Height(q) => Ok(Bend::to(q)),
            Saved::Points(p) => Bend::from_points(&p).ok_or_else(|| D::Error::custom("a bend curve needs a point above the fretted pitch")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pos: u8, value: u8) -> BendPoint {
        BendPoint::new(pos, value)
    }

    #[test]
    fn a_plain_bend_goes_up_by_the_middle_and_is_held() {
        let b = Bend::to(4);
        assert_eq!(b.points(), &[p(0, 0), p(30, 4), p(60, 4)]);
        assert_eq!((b.start(), b.peak(), b.end(), b.plain_height()), (0, 4, 4, Some(4)));
        assert_eq!(b.value_at(15.0), 2.0);
        assert_eq!(b.semitones_at(0.5), 2.0, "a full step is two semitones");
        assert_eq!(b.semitones_at(2.0), 2.0, "past the end holds");
        assert_eq!(Bend::to(0).peak(), 1, "at least a quarter tone");
        assert_eq!(Bend::to(99).peak(), BEND_MAX);
    }

    #[test]
    fn points_are_made_valid() {
        // Out of order, out of range, doubled, with no ends.
        let b = Bend::from_points(&[p(40, 99), p(10, 2), p(10, 3)]).unwrap();
        assert_eq!(b.points(), &[p(0, 3), p(10, 3), p(40, 12), p(60, 12)], "clamped, sorted, the last of a doubled position, ends added");
        assert!(Bend::from_points(&[]).is_none());
        assert!(Bend::from_points(&[p(0, 0), p(60, 0)]).is_none(), "a curve that never leaves the fretted pitch is no bend");
        // A straight run loses its middle.
        let b = Bend::from_points(&[p(0, 0), p(20, 2), p(40, 4), p(60, 6)]).unwrap();
        assert_eq!(b.points(), &[p(0, 0), p(60, 6)]);
        // Too many points thin out, keeping the shape's corners.
        let wiggle: Vec<BendPoint> = (0..=20u8).map(|i| p(i * 3, if i % 2 == 0 { 2 } else { 3 + i % 5 })).collect();
        let b = Bend::from_points(&wiggle).unwrap();
        assert!(b.points().len() <= BEND_MAX_POINTS);
        assert!(b.points().windows(2).all(|w| matches!(w, [a, c] if a.pos < c.pos)));
        assert_eq!((b.points().first().map(|x| x.pos), b.points().last().map(|x| x.pos)), (Some(0), Some(60)));
    }

    #[test]
    fn the_seven_types_have_their_shapes_and_are_recognised() {
        let q = 4;
        for (kind, tied, start, end) in [
            (BendKind::Bend, false, 0, 4),
            (BendKind::Release, true, 4, 0),
            (BendKind::BendRelease, false, 0, 0),
            (BendKind::Hold, true, 4, 4),
            (BendKind::Prebend, false, 4, 4),
            (BendKind::PrebendBend, false, 2, 4),
            (BendKind::PrebendRelease, false, 4, 0),
        ] {
            let b = Bend::preset(kind, q);
            assert_eq!((b.start(), b.end(), b.peak()), (start, end, 4), "{kind:?}");
            assert_eq!(b.kind(tied), Some(kind), "{kind:?}");
        }
        // The same curve is a prebend or a hold depending on the note it belongs to.
        assert_eq!(Bend::preset(BendKind::Prebend, 4).kind(true), Some(BendKind::Hold));
        assert_eq!(Bend::preset(BendKind::Hold, 4).kind(false), Some(BendKind::Prebend));
        // A prebend/bend of one quarter tone still bends further.
        let b = Bend::preset(BendKind::PrebendBend, 1);
        assert!(b.peak() > b.start());
        // Names round trip.
        for k in BendKind::ALL {
            assert_eq!(BendKind::from_name(k.name()), Some(k));
        }
        assert_eq!(BendKind::from_name("nope"), None);
        // A free-form curve is none of them.
        assert_eq!(Bend::from_points(&[p(0, 0), p(20, 4), p(40, 2), p(60, 5)]).unwrap().kind(false), None);
    }

    #[test]
    fn the_height_changes_and_the_shape_stays() {
        assert_eq!(Bend::preset(BendKind::BendRelease, 4).with_peak(8), Bend::preset(BendKind::BendRelease, 8));
        let free = Bend::from_points(&[p(0, 0), p(20, 4), p(40, 2), p(60, 5)]).unwrap();
        let big = free.with_peak(10);
        assert_eq!(big.peak(), 10);
        assert_eq!(big.points().len(), free.points().len());
    }

    #[test]
    fn saved_as_a_height_when_plain_and_points_otherwise() {
        let plain = serde_json::to_string(&Bend::to(4)).unwrap();
        assert_eq!(plain, "4");
        assert_eq!(serde_json::from_str::<Bend>("4").unwrap(), Bend::to(4), "files from before curves load");
        let curve = Bend::preset(BendKind::BendRelease, 6);
        let text = serde_json::to_string(&curve).unwrap();
        assert!(text.starts_with('['), "{text}");
        assert_eq!(serde_json::from_str::<Bend>(&text).unwrap(), curve);
        // Hostile input is an error, never a panic.
        for bad in ["0", "[]", "[{\"pos\":0,\"value\":0}]", "\"x\"", "[{\"pos\":300,\"value\":0}]", "-1"] {
            assert!(serde_json::from_str::<Bend>(bad).is_err(), "{bad}");
        }
        // A wild but well-formed curve is made valid.
        let wild = serde_json::from_str::<Bend>("[{\"pos\":9,\"value\":200},{\"pos\":9,\"value\":1}]");
        assert!(wild.is_err() || wild.is_ok_and(|b| b.points().len() >= 2));
    }

    #[test]
    fn thinning_keeps_the_ends_and_the_corners() {
        let b = Bend::from_points(&[p(0, 0), p(10, 3), p(20, 3), p(30, 6), p(40, 2), p(50, 2), p(60, 8)]).unwrap();
        let four = b.thinned(4);
        assert_eq!(four.len(), 4);
        assert_eq!((four.first().map(|x| x.pos), four.last().map(|x| x.pos)), (Some(0), Some(60)));
        assert!(four.iter().any(|x| x.value == 6), "the peak stays");
        assert_eq!(b.thinned(99), b.points());
        assert_eq!(b.thinned(0).len(), 2, "never fewer than the two ends");
    }

    #[test]
    fn a_tie_carries_the_curve_over_both_notes() {
        let q = 480;
        // A bend that is held through a tie with no curve of its own.
        let held = Bend::join(Some(Bend::to(4)), q, None, q).unwrap();
        assert_eq!(held.start(), 0);
        assert_eq!((held.value_at(7.5), held.value_at(60.0)), (2.0, 4.0), "up by the middle of the first note, then held");
        // Bend then release on the tied note.
        let both = Bend::join(Some(Bend::to(4)), q, Some(Bend::preset(BendKind::Release, 4)), q).unwrap();
        assert_eq!((both.value_at(30.0), both.value_at(60.0)), (4.0, 0.0), "held to the join, let down by the end");
        // No curve on the first note, a bend on the second.
        let late = Bend::join(None, q, Some(Bend::to(4)), q).unwrap();
        assert_eq!((late.value_at(25.0), late.value_at(60.0)), (0.0, 4.0));
        assert_eq!(Bend::join(None, q, None, q), None);
        assert_eq!(Bend::join(Some(Bend::to(4)), 0, None, 0), Some(Bend::to(4)), "no length leaves it alone");
    }
}
