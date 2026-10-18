//! What bends and the whammy bar share: a pitch curve of a few points along a note or beat.
//!
//! A point is `(pos, value)`: `pos` from 0 (the attack) to [`CURVE_END`] (the end), `value` in
//! quarter tones away from the written pitch, signed so a whammy dive can go below it. Between
//! points the pitch moves in a straight line.

/// The position of the end on a curve: the scale Guitar Pro 3-5 use.
pub const CURVE_END: u8 = 60;
/// Most points a curve holds.
pub const CURVE_MAX_POINTS: usize = 8;

pub(crate) type Pt = (u8, i16);

/// Points in any order made into a curve: positions clamped, points at one position keep the
/// last, the ends added at 0 and [`CURVE_END`], straight runs without their middle points and the
/// least important points dropped while more than [`CURVE_MAX_POINTS`] remain. Values must already
/// be in range. Empty when there were no points.
pub(crate) fn tidy(points: impl IntoIterator<Item = Pt>) -> Vec<Pt> {
    let mut v: Vec<Pt> = points.into_iter().map(|(pos, value)| (pos.min(CURVE_END), value)).collect();
    v.sort_by_key(|p| p.0);
    let mut out: Vec<Pt> = Vec::with_capacity(v.len() + 2);
    for p in v {
        match out.last_mut() {
            Some(last) if last.0 == p.0 => *last = p,
            _ => out.push(p),
        }
    }
    let Some(&first) = out.first() else { return out };
    if first.0 > 0 {
        out.insert(0, (0, first.1));
    }
    if let Some(&last) = out.last()
        && last.0 < CURVE_END
    {
        out.push((CURVE_END, last.1));
    }
    drop_straight(&mut out);
    thin(&mut out, CURVE_MAX_POINTS);
    out
}

/// Drops the points that matter least until at most `max` (at least 2) remain.
pub(crate) fn thin(v: &mut Vec<Pt>, max: usize) {
    while v.len() > max.max(2) {
        let Some(least) = least_important(v) else { break };
        v.remove(least);
    }
}

/// The value `pos` along the curve (0 to [`CURVE_END`]); before the first point its value, after
/// the last its value.
pub(crate) fn value_at(pts: &[Pt], pos: f32) -> f32 {
    let Some(first) = pts.first() else { return 0.0 };
    if pos <= f32::from(first.0) {
        return f32::from(first.1);
    }
    for w in pts.windows(2) {
        if let [a, b] = w
            && pos <= f32::from(b.0)
        {
            let span = f32::from(b.0.saturating_sub(a.0)).max(1.0);
            return f32::from(a.1) + (f32::from(b.1) - f32::from(a.1)) * (pos - f32::from(a.0)) / span;
        }
    }
    pts.last().map_or(0.0, |p| f32::from(p.1))
}

/// One curve over two stretches held as one (a tie): the first curve over `first_ticks`, then the
/// next over `next_ticks`. A stretch without a curve holds where the one before left off (a first
/// one without a curve stays at 0). `None` when neither has a curve; the first unchanged when
/// there is no length.
pub(crate) fn join(first: Option<&[Pt]>, first_ticks: u32, next: Option<&[Pt]>, next_ticks: u32) -> Option<Vec<Pt>> {
    let total = u64::from(first_ticks) + u64::from(next_ticks);
    if total == 0 || (first.is_none() && next.is_none()) {
        return first.map(<[Pt]>::to_vec);
    }
    let map = |t: u64| u8::try_from((t * u64::from(CURVE_END) + total / 2) / total).unwrap_or(CURVE_END);
    let cut = u64::from(first_ticks);
    let scale =
        |c: &[Pt], from: u64, len: u64| -> Vec<Pt> { c.iter().map(|&(pos, value)| (map(from + len * u64::from(pos) / u64::from(CURVE_END)), value)).collect() };
    let mut pts: Vec<Pt> = Vec::new();
    match first {
        Some(c) => pts.extend(scale(c, 0, cut)),
        None => pts.extend([(0, 0), (map(cut), 0)]),
    }
    let held = pts.last().map_or(0, |p| p.1);
    match next {
        Some(c) => pts.extend(scale(c, cut, u64::from(next_ticks))),
        None => pts.extend([(map(cut), held), (CURVE_END, held)]),
    }
    Some(tidy(pts))
}

/// Removes points that lie on the straight line between their neighbours.
fn drop_straight(v: &mut Vec<Pt>) {
    let mut i = 1;
    while i + 1 < v.len() {
        let straight = match (v.get(i - 1), v.get(i), v.get(i + 1)) {
            (Some(a), Some(b), Some(c)) => {
                // b is on the line from a to c when the cross product of the steps is 0.
                let (ax, ay) = (i32::from(a.0), i32::from(a.1));
                let (bx, by) = (i32::from(b.0), i32::from(b.1));
                let (cx, cy) = (i32::from(c.0), i32::from(c.1));
                (bx - ax) * (cy - ay) == (cx - ax) * (by - ay)
            }
            _ => false,
        };
        if straight {
            v.remove(i);
        } else {
            i += 1;
        }
    }
}

/// The interior point that matters least: the one nearest the line between its neighbours.
fn least_important(v: &[Pt]) -> Option<usize> {
    (1..v.len().saturating_sub(1))
        .filter_map(|i| {
            let (a, b, c) = (v.get(i - 1)?, v.get(i)?, v.get(i + 1)?);
            let span = f32::from(c.0.saturating_sub(a.0)).max(1.0);
            let on_line = f32::from(a.1) + (f32::from(c.1) - f32::from(a.1)) * f32::from(b.0.saturating_sub(a.0)) / span;
            Some((i, (f32::from(b.1) - on_line).abs()))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidy_sorts_dedupes_adds_ends_and_thins() {
        assert!(tidy([]).is_empty());
        assert_eq!(tidy([(40, -5), (10, 2), (10, 3), (99, 1)]), [(0, 3), (10, 3), (40, -5), (60, 1)]);
        assert_eq!(tidy([(0, 0), (20, -2), (40, -4), (60, -6)]), [(0, 0), (60, -6)], "a straight run loses its middle");
        let wiggle: Vec<Pt> = (0..=20u8).map(|i| (i * 3, if i % 2 == 0 { -2 } else { 3 })).collect();
        assert!(tidy(wiggle).len() <= CURVE_MAX_POINTS);
    }

    #[test]
    fn values_between_points_are_on_the_line() {
        let c = [(0, 0), (30, -8), (60, -8)];
        assert_eq!((value_at(&c, 15.0), value_at(&c, 45.0), value_at(&c, 99.0), value_at(&[], 1.0)), (-4.0, -8.0, -8.0, 0.0));
    }
}
