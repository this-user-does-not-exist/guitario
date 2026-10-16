//! Page layout without drawing: measure spacing, system breaks with justification, and pages.
//! Dimensions follow Guitar Pro's default stylesheet: A4 portrait, 10 mm side margins, 15 mm top,
//! a 1.5875 mm staff space.

use guitario_score::{Beat, Measure, PageSetup, Score, Track};

const MM: f32 = 96.0 / 25.4;

#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub zoom: f32,
    pub space: f32,
    pub tab_gap: f32,
    pub page_w: f32,
    pub page_h: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub notation: bool,
    pub strings: usize,
    /// A drum track: one percussion staff, no tablature.
    pub drums: bool,
    /// The track has chord names or beat text: a taller lane above each system holds them.
    pub annotations: bool,
    /// The voice being edited, 0-based. A voice with only rests is drawn when it is this one.
    pub voice: usize,
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
            margin_left: 10.0 * MM * z,
            margin_right: 10.0 * MM * z,
            margin_top: 15.0 * MM * z,
            margin_bottom: 12.0 * MM * z,
            notation,
            strings: strings.max(1),
            drums: false,
            annotations: false,
            voice: 0,
        }
    }

    /// The paper, orientation and margins of `page`, at this zoom.
    pub fn with_page(self, page: &PageSetup) -> Metrics {
        let (w, h) = page.size_mm();
        let px = |mm: f32| mm * MM * self.zoom;
        Metrics {
            page_w: px(w),
            page_h: px(h),
            margin_left: px(f32::from(page.margin_left)),
            margin_right: px(f32::from(page.margin_right)),
            margin_top: px(f32::from(page.margin_top)),
            margin_bottom: px(f32::from(page.margin_bottom)),
            ..self
        }
    }

    /// Where the centre of the printable width lies, from the page's left edge.
    pub fn center_x(&self) -> f32 {
        self.margin_left + (self.page_w - self.margin_left - self.margin_right) / 2.0
    }

    /// Draws the voice being edited even while it holds only rests.
    pub fn with_voice(self, voice: usize) -> Metrics {
        Metrics { voice, ..self }
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
        self.page_w - self.margin_left - self.margin_right - self.indent()
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

/// Where one voice's beats sit in a bar, relative to the bar's left edge.
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceCols {
    /// The voice's index in the bar (0 is Voice 1).
    pub voice: usize,
    /// Where each beat's notehead or fret goes.
    pub beats: Vec<f32>,
    /// The stretch of the bar each beat covers, `(left, right)`: from its own column to the
    /// column of the voice's next beat.
    pub spans: Vec<(f32, f32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeasureBox {
    pub index: usize,
    /// Left edge, relative to the system's staff start (after the clef/key prefix).
    pub x: f32,
    pub width: f32,
    /// The voices drawn in this bar, Voice 1 first. Beats that start together share a column.
    pub voices: Vec<VoiceCols>,
    pub show_time: bool,
}

impl MeasureBox {
    /// The columns of voice `voice`, or of Voice 1 when that voice is not drawn here.
    pub fn cols(&self, voice: usize) -> Option<&VoiceCols> {
        self.voices.iter().find(|v| v.voice == voice).or_else(|| self.voices.first())
    }
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

/// Indexes of the voices drawn in a bar: Voice 1 always, the others when they hold a note or
/// are the voice being edited.
pub fn drawn_voices(measure: &Measure, active: usize) -> Vec<usize> {
    measure
        .voices
        .iter()
        .enumerate()
        .filter(|(i, v)| !v.beats.is_empty() && (*i == 0 || *i == active || v.beats.iter().any(|b| !b.rest)))
        .map(|(i, _)| i)
        .collect()
}

/// Width of a column whose next onset is `gap` ticks away, plus room for wide frets and chords.
fn column_natural(gap: u32, beats: &[&Beat], space: f32) -> f32 {
    // Width grows with the log of the length, from 2.7 spaces for a sixteenth.
    let sixteenth = (guitario_theory::TICKS_PER_QUARTER / 4) as f32;
    let mut w = space * (2.7 + 1.15 * (gap.max(1) as f32 / sixteenth).log2()).max(1.9);
    if beats.iter().any(|b| b.notes.iter().any(|n| n.fret >= 10)) {
        w += 0.5 * space;
    }
    if beats.iter().any(|b| b.notes.len() > 1) {
        w += 0.3 * space;
    }
    w
}

struct VoiceNat {
    voice: usize,
    /// The column each beat starts in.
    col_of: Vec<usize>,
}

/// Room left of a beat for its grace notes, in staff spaces.
pub const GRACE_ROOM: f32 = 3.0;

struct Natural {
    fixed: f32,
    /// Column widths: one per moment a beat starts in any voice.
    cols: Vec<f32>,
    /// Room kept at the left of each column for grace notes, inside its width.
    pads: Vec<f32>,
    voices: Vec<VoiceNat>,
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
    let measure = track.measures.get(mi);
    let drawn = measure.map(|x| drawn_voices(x, m.voice)).unwrap_or_default();
    // Every beat with the tick it starts at, per drawn voice.
    let timed: Vec<(usize, Vec<(u64, &Beat)>)> = drawn
        .iter()
        .filter_map(|&vi| {
            let voice = measure?.voices.get(vi)?;
            let mut at = 0u64;
            let beats = voice
                .beats
                .iter()
                .map(|b| {
                    let start = at;
                    at += u64::from(b.duration.ticks());
                    (start, b)
                })
                .collect();
            Some((vi, beats))
        })
        .collect();
    let mut onsets: Vec<u64> = timed.iter().flat_map(|(_, bs)| bs.iter().map(|(t, _)| *t)).collect();
    onsets.sort_unstable();
    onsets.dedup();
    let mut pads = Vec::with_capacity(onsets.len());
    let cols: Vec<f32> = onsets
        .iter()
        .enumerate()
        .map(|(ci, &t)| {
            let here: Vec<&Beat> = timed.iter().flat_map(|(_, bs)| bs.iter().filter(move |(s, _)| *s == t).map(|(_, b)| *b)).collect();
            let pad = if here.iter().any(|b| !b.rest && b.notes.iter().any(|n| n.effects.grace.is_some())) { GRACE_ROOM * s } else { 0.0 };
            pads.push(pad);
            let gap = match onsets.get(ci + 1) {
                Some(&next) => u32::try_from(next - t).unwrap_or(u32::MAX),
                None => here.iter().map(|b| b.duration.ticks()).min().unwrap_or(1),
            };
            let mut w = column_natural(gap, &here, s);
            if onsets.len() == 1 && here.iter().all(|b| b.rest) {
                w = w.max(8.0 * s);
            }
            w + pad
        })
        .collect();
    let voices = timed.iter().map(|(vi, bs)| VoiceNat { voice: *vi, col_of: bs.iter().filter_map(|(t, _)| onsets.binary_search(t).ok()).collect() }).collect();
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
    Natural { fixed, cols, pads, voices, show_time, lead }
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
    let total = |n: &Natural| n.fixed + n.cols.iter().sum::<f32>();

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
        let beats_sum: f32 = row.iter().filter_map(|i| naturals.get(*i)).map(|n| n.cols.iter().sum::<f32>()).sum();
        let last_and_short = si + 1 == count && used < avail * 0.7;
        let extra = if last_and_short || beats_sum <= 0.0 { 0.0 } else { (avail - used).max(0.0) };
        let stretch = if beats_sum > 0.0 { 1.0 + extra / beats_sum } else { 1.0 };
        let mut x = 0.0;
        let mut measures = Vec::new();
        for mi in row {
            let Some(n) = naturals.get(mi) else { continue };
            let col_w: Vec<f32> = n.cols.iter().map(|w| w * stretch).collect();
            // Left edge of each column, with one more entry for the right edge of the last.
            let mut edges = Vec::with_capacity(col_w.len() + 1);
            let mut bx = n.lead;
            edges.push(bx);
            for w in &col_w {
                bx += w;
                edges.push(bx);
            }
            let edge = |c: usize| edges.get(c).copied().unwrap_or(bx);
            let pad = |c: usize| n.pads.get(c).copied().unwrap_or(0.0);
            let voices = n
                .voices
                .iter()
                .map(|v| {
                    // A beat sits in the middle of its column, right of any room kept for grace notes.
                    let beats = v.col_of.iter().map(|&c| (edge(c) + pad(c) + edge(c + 1)) / 2.0).collect();
                    let spans = v.col_of.iter().enumerate().map(|(i, &c)| (edge(c), edge(v.col_of.get(i + 1).copied().unwrap_or(col_w.len())))).collect();
                    VoiceCols { voice: v.voice, beats, spans }
                })
                .collect();
            let width = n.fixed + col_w.iter().sum::<f32>();
            measures.push(MeasureBox { index: mi, x, width, voices, show_time: n.show_time });
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
    fn a_beat_with_a_grace_note_keeps_room_on_its_left() {
        use guitario_score::{Beat, Grace, Note};
        use guitario_theory::Duration;
        let mut s = score(1);
        let plain = |fret: u8| Beat { rest: false, notes: vec![Note::new(0, fret)], ..Beat::rest(Duration::QUARTER) };
        s.tracks[0].measures[0].voices[0].beats = vec![plain(1), plain(2), plain(3), plain(4)];
        let m = Metrics::new(1.0, true, 6);
        let room = |s: &Score| {
            let pages = layout(s, 0, &m);
            let v = &pages[0].systems[0].measures[0].voices[0];
            // From where each beat's stretch starts to its column.
            v.beats.iter().zip(&v.spans).map(|(x, (l, _))| x - l).collect::<Vec<f32>>()
        };
        let before = room(&s);
        s.tracks[0].measures[0].voices[0].beats[2].notes[0].effects.grace = Some(Grace::new(5));
        let after = room(&s);
        assert!(after[2] >= before[2] + GRACE_ROOM * m.space * 0.99, "{before:?} {after:?}");
        assert!((after[1] - before[1]).abs() < 0.01, "other beats are not moved off-centre");
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
    fn every_paper_lays_out_inside_its_margins() {
        use guitario_score::Paper;
        let s = score(40);
        for paper in Paper::ALL {
            for landscape in [false, true] {
                let page = PageSetup { paper, landscape, margin_left: 5, margin_right: 25, margin_top: 20, margin_bottom: 10 };
                let m = Metrics::new(1.0, true, 6).with_page(&page);
                let (w, h) = page.size_mm();
                assert!((m.page_w - w * MM).abs() < 0.01 && (m.page_h - h * MM).abs() < 0.01);
                let pages = layout(&s, 0, &m);
                let order: Vec<usize> = pages.iter().flat_map(|p| &p.systems).flat_map(|s| &s.measures).map(|x| x.index).collect();
                assert_eq!(order, (0..40).collect::<Vec<_>>(), "{paper:?} landscape={landscape}");
                for page in &pages {
                    for sys in &page.systems {
                        let used: f32 = sys.measures.iter().map(|x| x.width).sum::<f32>() + sys.prefix;
                        assert!(used <= m.content_w() + 0.5, "{paper:?}: {used} > {}", m.content_w());
                        // A system stays above the bottom margin unless it is alone on a page too short for it.
                        assert!(sys.y + m.system_h() <= m.page_h - m.margin_bottom + 0.5 || page.systems.len() == 1, "{paper:?} landscape={landscape}");
                    }
                }
            }
        }
    }

    #[test]
    fn margins_move_the_printable_width_and_its_centre() {
        let even = Metrics::new(1.0, true, 6);
        let page = PageSetup { margin_left: 30, margin_right: 10, ..PageSetup::default() };
        let m = even.with_page(&page);
        assert!((even.content_w() - m.content_w() - 20.0 * MM).abs() < 0.01);
        // The centre of the printable width sits right of the page centre when the left margin is wider.
        assert!(m.center_x() > m.page_w / 2.0);
        assert!((even.center_x() - even.page_w / 2.0).abs() < 0.01);
        // Zoom scales the paper and the margins together.
        let zoomed = Metrics::new(2.0, true, 6).with_page(&page);
        assert!((zoomed.page_w - 2.0 * m.page_w).abs() < 0.01 && (zoomed.margin_left - 2.0 * m.margin_left).abs() < 0.01);
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

    fn voiced_score() -> Score {
        use guitario_score::{Note, Voice};
        use guitario_theory::{Duration, NoteValue};
        let mut s = score(1);
        let note = |d: Duration| Beat { rest: false, notes: vec![Note::new(0, 5)], ..Beat::rest(d) };
        let (q, h) = (Duration::QUARTER, Duration::new(NoteValue::Half));
        // Voice 1: two halves. Voice 2: a quarter, a quarter, then a half: onsets 0, q, 2q.
        s.tracks[0].measures[0].voices = vec![Voice { beats: vec![note(h), note(h)] }, Voice { beats: vec![note(q), note(q), note(h)] }];
        s
    }

    #[test]
    fn voices_share_a_column_when_their_beats_start_together() {
        let s = voiced_score();
        let pages = layout(&s, 0, &Metrics::new(1.0, true, 6));
        let mb = &pages[0].systems[0].measures[0];
        let (v1, v2) = (&mb.voices[0], &mb.voices[1]);
        assert_eq!((v1.voice, v2.voice), (0, 1));
        // Voice 1 beat 0 and voice 2 beat 0 start together; so do voice 1 beat 1 and voice 2 beat 2.
        assert_eq!(v1.beats[0], v2.beats[0]);
        assert_eq!(v1.beats[1], v2.beats[2]);
        // Voice 2's middle beat falls between them.
        assert!(v2.beats[0] < v2.beats[1] && v2.beats[1] < v2.beats[2]);
        // A beat's span runs to its voice's next beat: voice 1's first half covers voice 2's first two.
        assert_eq!(v1.spans[0].0, v2.spans[0].0);
        assert_eq!(v1.spans[0].1, v2.spans[1].1);
        assert_eq!(v1.spans[1].1, v2.spans[2].1, "both end at the last column");
    }

    #[test]
    fn a_voice_with_only_rests_is_drawn_only_while_it_is_being_edited() {
        use guitario_score::Voice;
        let mut s = voiced_score();
        let rest = guitario_score::Beat::rest(guitario_theory::Duration::WHOLE);
        s.tracks[0].measures[0].voices[1] = Voice { beats: vec![rest] };
        let measure = &s.tracks[0].measures[0];
        assert_eq!(drawn_voices(measure, 0), vec![0]);
        assert_eq!(drawn_voices(measure, 1), vec![0, 1]);
        // A bar that lacks the voice has nothing to draw for it.
        s.tracks[0].measures[0].voices.truncate(1);
        assert_eq!(drawn_voices(&s.tracks[0].measures[0], 1), vec![0]);
        let pages = layout(&s, 0, &Metrics::new(1.0, true, 6).with_voice(1));
        assert_eq!(pages[0].systems[0].measures[0].voices.len(), 1);
    }

    #[test]
    fn spans_tile_each_voice_and_one_voice_lays_out_as_before() {
        let s = voiced_score();
        let pages = layout(&s, 0, &Metrics::new(1.0, true, 6));
        let mb = &pages[0].systems[0].measures[0];
        for v in &mb.voices {
            for w in v.spans.windows(2) {
                assert_eq!(w[0].1, w[1].0, "no gap or overlap between a voice's beats");
            }
            for (c, (l, r)) in v.beats.iter().zip(&v.spans) {
                assert!(l <= c && c <= r);
            }
        }
        // One voice: every beat's span is its own column and the centre is in the middle of it.
        let one = score(2);
        let pages = layout(&one, 0, &Metrics::new(1.0, true, 6));
        let v = &pages[0].systems[0].measures[0].voices[0];
        assert_eq!(v.beats.len(), 1);
        assert!(((v.spans[0].0 + v.spans[0].1) / 2.0 - v.beats[0]).abs() < 0.001);
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
        assert!((column_natural(sixteenth.duration.ticks(), &[&sixteenth], m.space) - 2.7 * m.space).abs() < 0.01);
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
