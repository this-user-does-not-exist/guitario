//! Page layout without drawing: measure spacing, system breaks with justification, and pages.
//! Dimensions follow Guitar Pro's default stylesheet: A4 portrait, 10 mm side margins, 15 mm top,
//! a 1.5875 mm staff space.

use guitario_score::{Beat, Score, Track};

const MM: f32 = 96.0 / 25.4;

#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub zoom: f32,
    pub space: f32,
    pub tab_gap: f32,
    pub page_w: f32,
    pub page_h: f32,
    pub margin_x: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub notation: bool,
    pub strings: usize,
    /// A drum track: one percussion staff, no tablature.
    pub drums: bool,
    /// The track has chord names or beat text: a taller lane above each system holds them.
    pub annotations: bool,
}

impl Metrics {
    pub fn new(zoom: f32, notation: bool, strings: usize) -> Metrics {
        let z = zoom.clamp(0.25, 8.0);
        let space = 1.5875 * MM * z;
        Metrics {
            zoom: z,
            space,
            tab_gap: space * 1.55,
            page_w: 210.0 * MM * z,
            page_h: 297.0 * MM * z,
            margin_x: 10.0 * MM * z,
            margin_top: 15.0 * MM * z,
            margin_bottom: 12.0 * MM * z,
            notation,
            strings: strings.max(1),
            drums: false,
            annotations: false,
        }
    }

    /// Room for chord names and beat text above every system when `track` has any.
    pub fn with_annotations(self, track: &Track) -> Metrics {
        let annotations = track.measures.iter().flat_map(|m| &m.voices).flat_map(|v| &v.beats).any(|b| b.chord.is_some() || b.text.is_some());
        Metrics { annotations, ..self }
    }

    /// Metrics for a drum track: a five-line percussion staff only.
    pub fn for_drums(zoom: f32) -> Metrics {
        Metrics { drums: true, ..Metrics::new(zoom, true, 1) }
    }

    /// Room left of every system for the short track name.
    pub fn indent(&self) -> f32 {
        6.5 * self.space
    }

    /// Width available to a system (clef prefix and measures).
    pub fn content_w(&self) -> f32 {
        self.page_w - 2.0 * self.margin_x - self.indent()
    }

    /// Space above the first staff: section markers, chord names and text, directions, tempo,
    /// bar numbers.
    pub fn lane_above(&self) -> f32 {
        if self.annotations { 11.0 * self.space } else { 9.0 * self.space }
    }

    pub fn staff_h(&self) -> f32 {
        4.0 * self.space
    }

    /// Between the notation staff and the tab: effect lanes (P.M., let ring, vibrato, bends).
    pub fn staff_gap(&self) -> f32 {
        11.0 * self.space
    }

    pub fn tab_h(&self) -> f32 {
        if self.drums { 0.0 } else { self.tab_gap * self.strings.saturating_sub(1) as f32 }
    }

    /// Offset of the notation staff's top line from the system top.
    pub fn notation_top(&self) -> f32 {
        self.lane_above()
    }

    /// Offset of the tab staff's top line from the system top.
    pub fn tab_top(&self) -> f32 {
        if self.drums {
            self.lane_above() + self.staff_h()
        } else if self.notation {
            self.lane_above() + self.staff_h() + self.staff_gap()
        } else {
            self.lane_above() + 2.0 * self.space
        }
    }

    pub fn system_h(&self) -> f32 {
        let below = if self.drums {
            5.0
        } else if self.notation {
            3.5
        } else {
            7.0
        } * self.space;
        self.tab_top() + self.tab_h() + below + 4.0 * self.space
    }

    pub fn header_h(&self, score: &Score) -> f32 {
        let mut h = 14.0 * self.space;
        if !score.meta.artist.is_empty() {
            h += 4.0 * self.space;
        }
        h
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeasureBox {
    pub index: usize,
    /// Left edge, relative to the system's staff start (after the clef/key prefix).
    pub x: f32,
    pub width: f32,
    /// Beat centres relative to `x`.
    pub beats: Vec<f32>,
    /// Beat widths after justification.
    pub beat_w: Vec<f32>,
    pub show_time: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SystemBox {
    pub measures: Vec<MeasureBox>,
    /// Top of the system relative to the page's top edge.
    pub y: f32,
    /// Width of the clef + key signature prefix.
    pub prefix: f32,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct PageBox {
    pub systems: Vec<SystemBox>,
}

fn beat_natural(b: &Beat, space: f32) -> f32 {
    // Width grows with the log of the length, from 2.7 spaces for a sixteenth.
    let sixteenth = (guitario_theory::TICKS_PER_QUARTER / 4) as f32;
    let ticks = b.duration.ticks().max(1) as f32;
    let mut w = space * (2.7 + 1.15 * (ticks / sixteenth).log2()).max(1.9);
    if b.notes.iter().any(|n| n.fret >= 10) {
        w += 0.5 * space;
    }
    if b.notes.len() > 1 {
        w += 0.3 * space;
    }
    w
}

struct Natural {
    fixed: f32,
    beats: Vec<f32>,
    show_time: bool,
    lead: f32,
}

fn natural(score: &Score, track: &Track, mi: usize, m: &Metrics) -> Natural {
    let s = m.space;
    let header = score.headers.get(mi);
    let prev = mi.checked_sub(1).and_then(|i| score.headers.get(i));
    let show_time = match (header, prev) {
        (Some(h), Some(p)) => h.time_signature != p.time_signature,
        (Some(_), None) => true,
        _ => false,
    };
    let beats: Vec<&Beat> = track.measures.get(mi).and_then(|x| x.voices.first()).map(|v| v.beats.iter().collect()).unwrap_or_default();
    let mut widths: Vec<f32> = beats.iter().map(|b| beat_natural(b, s)).collect();
    if let ([only], [w]) = (beats.as_slice(), widths.as_mut_slice())
        && only.rest
    {
        *w = w.max(8.0 * s);
    }
    let mut lead = 1.8 * s;
    if show_time {
        lead += 3.0 * s;
    }
    if header.is_some_and(|h| h.repeat_open) {
        lead += 1.4 * s;
    }
    let mut fixed = lead + 1.2 * s;
    if header.is_some_and(|h| h.repeat_close > 0) {
        fixed += 1.4 * s;
    }
    Natural { fixed, beats: widths, show_time, lead }
}

/// The clef + key signature prefix at the start of every system.
pub fn system_prefix(score: &Score, first_measure: usize, m: &Metrics) -> f32 {
    let key = score.headers.get(first_measure).map_or(0, |h| h.key.unsigned_abs().min(7));
    if m.drums {
        4.4 * m.space
    } else if m.notation {
        (4.2 + f32::from(key) * 1.0 + 0.6) * m.space
    } else {
        4.6 * m.space
    }
}

pub fn layout(score: &Score, track_index: usize, m: &Metrics) -> Vec<PageBox> {
    let Some(track) = score.tracks.get(track_index) else { return vec![] };
    let naturals: Vec<Natural> = (0..score.headers.len()).map(|mi| natural(score, track, mi, m)).collect();
    let total = |n: &Natural| n.fixed + n.beats.iter().sum::<f32>();

    // Break into systems.
    let mut systems: Vec<(Vec<usize>, f32)> = Vec::new();
    let mut i = 0;
    while i < naturals.len() {
        let prefix = system_prefix(score, i, m);
        let avail = m.content_w() - prefix;
        let mut used = 0.0;
        let mut row = Vec::new();
        while let Some(n) = naturals.get(i) {
            let w = total(n);
            if !row.is_empty() && used + w > avail {
                break;
            }
            used += w;
            row.push(i);
            i += 1;
        }
        systems.push((row, prefix));
    }

    // Justify and paginate.
    let mut pages = vec![PageBox::default()];
    let mut y = m.margin_top + m.header_h(score);
    let count = systems.len();
    for (si, (row, prefix)) in systems.into_iter().enumerate() {
        let avail = m.content_w() - prefix;
        let used: f32 = row.iter().filter_map(|i| naturals.get(*i)).map(total).sum();
        let beats_sum: f32 = row.iter().filter_map(|i| naturals.get(*i)).map(|n| n.beats.iter().sum::<f32>()).sum();
        let last_and_short = si + 1 == count && used < avail * 0.7;
        let extra = if last_and_short || beats_sum <= 0.0 { 0.0 } else { (avail - used).max(0.0) };
        let stretch = if beats_sum > 0.0 { 1.0 + extra / beats_sum } else { 1.0 };
        let mut x = 0.0;
        let mut measures = Vec::new();
        for mi in row {
            let Some(n) = naturals.get(mi) else { continue };
            let beat_w: Vec<f32> = n.beats.iter().map(|w| w * stretch).collect();
            let mut bx = n.lead;
            let beats = beat_w
                .iter()
                .map(|w| {
                    let c = bx + w / 2.0;
                    bx += w;
                    c
                })
                .collect();
            let width = n.fixed + beat_w.iter().sum::<f32>();
            measures.push(MeasureBox { index: mi, x, width, beats, beat_w, show_time: n.show_time });
            x += width;
        }
        if y + m.system_h() > m.page_h - m.margin_bottom && pages.last().is_some_and(|p| !p.systems.is_empty()) {
            pages.push(PageBox::default());
            y = m.margin_top;
        }
        if let Some(page) = pages.last_mut() {
            page.systems.push(SystemBox { measures, y, prefix });
        }
        y += m.system_h();
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::MeasureHeader;

    fn score(n: usize) -> Score {
        let mut s = Score::new();
        for _ in 1..n {
            s.headers.push(MeasureHeader::new(Default::default()));
            let h = s.headers[0].clone();
            s.tracks[0].measures.push(guitario_score::Measure::empty_for(&h));
        }
        s
    }

    #[test]
    fn every_measure_is_placed_once_in_order() {
        let s = score(60);
        let m = Metrics::new(1.0, true, 6);
        let pages = layout(&s, 0, &m);
        let order: Vec<usize> = pages.iter().flat_map(|p| &p.systems).flat_map(|s| &s.measures).map(|m| m.index).collect();
        assert_eq!(order, (0..60).collect::<Vec<_>>());
        assert!(pages.len() > 1, "60 empty bars should need more than one page");
    }

    #[test]
    fn systems_fit_the_page_width() {
        let s = score(30);
        let m = Metrics::new(1.5, true, 6);
        for sys in layout(&s, 0, &m).iter().flat_map(|p| &p.systems) {
            let w: f32 = sys.measures.iter().map(|x| x.width).sum();
            assert!(w + sys.prefix <= m.content_w() + 0.5, "{w} + {} > {}", sys.prefix, m.content_w());
            assert!(sys.y + m.system_h() <= m.page_h);
        }
    }

    #[test]
    fn first_measure_shows_time_signature() {
        let pages = layout(&score(2), 0, &Metrics::new(1.0, false, 6));
        let ms = &pages[0].systems[0].measures;
        assert!(ms[0].show_time);
        assert!(!ms[1].show_time);
    }

    #[test]
    fn drum_systems_are_one_staff_high() {
        let g = Metrics::new(1.0, true, 6);
        let d = Metrics::for_drums(1.0);
        assert_eq!(d.tab_h(), 0.0);
        assert!(d.system_h() < g.system_h() * 0.7, "{} vs {}", d.system_h(), g.system_h());
        let pages = layout(&score(40), 0, &d);
        assert!(pages.iter().flat_map(|p| &p.systems).all(|s| s.y + d.system_h() <= d.page_h));
    }

    #[test]
    fn a_bar_of_eighths_leaves_room_for_more_bars() {
        // Spacing is relative to the tick resolution: a sixteenth is 2.7 spaces whatever
        // TICKS_PER_QUARTER is, and four bars of straight eighths share one A4 system.
        let m = Metrics::new(1.0, true, 6);
        let sixteenth = Beat {
            rest: false,
            notes: vec![guitario_score::Note::new(0, 0)],
            ..Beat::rest(guitario_theory::Duration::new(guitario_theory::NoteValue::Sixteenth))
        };
        assert!((beat_natural(&sixteenth, m.space) - 2.7 * m.space).abs() < 0.01);
        let mut s = score(8);
        let eighth = Beat { rest: false, notes: vec![guitario_score::Note::new(0, 5)], ..Beat::rest(guitario_theory::Duration::EIGHTH) };
        for m in &mut s.tracks[0].measures {
            m.voices[0].beats = vec![eighth.clone(); 8];
        }
        let pages = layout(&s, 0, &m);
        assert!(pages[0].systems[0].measures.len() >= 3, "{} bars on the first system", pages[0].systems[0].measures.len());
    }

    #[test]
    fn missing_track_is_empty() {
        assert!(layout(&score(2), 9, &Metrics::new(1.0, true, 6)).is_empty());
    }
}
