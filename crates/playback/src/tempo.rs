use guitario_score::{Hold, Score, playback_order};
use guitario_theory::TICKS_PER_QUARTER;

/// One measure as played (repeats unrolled).
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub measure: usize,
    pub start_tick: u64,
    pub ticks: u32,
    pub start_sec: f64,
    pub sec_per_tick: f64,
    /// Fermatas: stretches of the bar that last longer than written.
    pub holds: Vec<Hold>,
}

impl Segment {
    /// Seconds from the start of the bar to `offset` ticks into it, fermatas included.
    pub fn sec_at(&self, offset: u32) -> f64 {
        let extra: f64 = self.holds.iter().map(|h| f64::from(offset.saturating_sub(h.at).min(h.ticks)) * (h.stretch - 1.0).max(0.0)).sum();
        (f64::from(offset) + extra) * self.sec_per_tick
    }

    /// Seconds the whole bar lasts.
    pub fn duration(&self) -> f64 {
        self.sec_at(self.ticks)
    }

    /// Seconds per tick at `offset` ticks into the bar.
    pub fn rate_at(&self, offset: u32) -> f64 {
        let held = self.holds.iter().find(|h| offset >= h.at && offset < h.at.saturating_add(h.ticks));
        self.sec_per_tick * held.map_or(1.0, |h| h.stretch.max(1.0))
    }
}

/// When a beat of one track sounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeatTime {
    pub sec: f64,
    pub measure: usize,
    pub beat: usize,
}

#[derive(Clone, Debug, Default)]
pub struct TempoMap {
    pub segments: Vec<Segment>,
    pub total_sec: f64,
}

impl TempoMap {
    pub fn new(score: &Score) -> TempoMap {
        let mut segments = Vec::new();
        let (mut tick, mut sec) = (0u64, 0.0f64);
        for mi in playback_order(score) {
            let ticks = score.headers.get(mi).map_or(0, |h| h.capacity());
            let sec_per_tick = 60.0 / (f64::from(score.tempo_at(mi).max(1)) * f64::from(TICKS_PER_QUARTER));
            let seg = Segment { measure: mi, start_tick: tick, ticks, start_sec: sec, sec_per_tick, holds: score.holds(mi) };
            tick += u64::from(ticks);
            sec += seg.duration();
            segments.push(seg);
        }
        TempoMap { segments, total_sec: sec }
    }

    fn segment_at_tick(&self, tick: u64) -> Option<&Segment> {
        let i = self.segments.partition_point(|s| s.start_tick <= tick);
        self.segments.get(i.saturating_sub(1))
    }

    pub fn tick_to_sec(&self, tick: u64) -> f64 {
        self.segment_at_tick(tick).map_or(0.0, |s| {
            let offset = tick.saturating_sub(s.start_tick);
            // Past the last bar time runs on at its tempo.
            match u32::try_from(offset).ok().filter(|o| *o <= s.ticks) {
                Some(o) => s.start_sec + s.sec_at(o),
                None => s.start_sec + s.duration() + (offset - u64::from(s.ticks)) as f64 * s.sec_per_tick,
            }
        })
    }

    /// Seconds per tick at `tick`.
    pub fn rate_at(&self, tick: u64) -> f64 {
        self.segment_at_tick(tick)
            .map_or(60.0 / (120.0 * f64::from(TICKS_PER_QUARTER)), |s| s.rate_at(u32::try_from(tick.saturating_sub(s.start_tick)).unwrap_or(u32::MAX)))
    }

    /// Every beat of a track in playback order, for following the cursor during playback.
    pub fn beat_times(&self, score: &Score, track: usize) -> Vec<BeatTime> {
        let mut out = Vec::new();
        let Some(t) = score.tracks.get(track) else { return out };
        for seg in &self.segments {
            let Some(voice) = t.measures.get(seg.measure).and_then(|m| m.voices.first()) else { continue };
            let mut at = 0u32;
            for (bi, b) in voice.beats.iter().enumerate() {
                out.push(BeatTime { sec: seg.start_sec + seg.sec_at(at), measure: seg.measure, beat: bi });
                at = at.saturating_add(b.duration.ticks());
            }
        }
        out
    }
}

/// The beat sounding at `sec` (the last one that started), by binary search.
pub fn locate(beats: &[BeatTime], sec: f64) -> Option<BeatTime> {
    let i = beats.partition_point(|b| b.sec <= sec);
    beats.get(i.checked_sub(1)?).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::MeasureHeader;

    #[test]
    fn seconds_follow_tempo_changes_and_repeats() {
        let mut s = Score::new();
        s.tempo = 60;
        let h = s.headers[0].clone();
        s.headers.push(MeasureHeader { tempo: Some(120), ..h.clone() });
        s.tracks[0].measures.push(guitario_score::Measure::empty_for(&h));
        let m = TempoMap::new(&s);
        // 4 beats at 60 bpm then 4 at 120.
        assert!((m.total_sec - 6.0).abs() < 1e-9);
        assert!((m.tick_to_sec(u64::from(4 * TICKS_PER_QUARTER)) - 4.0).abs() < 1e-9);
        assert!((m.tick_to_sec(u64::from(5 * TICKS_PER_QUARTER)) - 4.5).abs() < 1e-9);
        s.headers[1].repeat_close = 1;
        assert!((TempoMap::new(&s).total_sec - 12.0).abs() < 1e-9);
    }

    #[test]
    fn a_fermata_holds_its_beat_in_every_track_and_moves_what_follows() {
        use guitario_score::{Beat, Fermata, FermataKind, Note};
        use guitario_theory::Duration;
        let mut s = Score::new();
        s.tempo = 60;
        let note = Beat { rest: false, notes: vec![Note::new(0, 5)], ..Beat::rest(Duration::QUARTER) };
        s.tracks[0].measures[0].voices[0].beats = vec![note.clone(), note.clone(), note.clone(), note];
        let q = u64::from(TICKS_PER_QUARTER);
        // A long fermata on beat 2 holds it three times as long: 1 + 3 + 1 + 1 seconds.
        s.headers[0].fermatas = vec![Fermata { tick: TICKS_PER_QUARTER, kind: FermataKind::Long, length: 0 }];
        let m = TempoMap::new(&s);
        assert!((m.total_sec - 6.0).abs() < 1e-9, "{}", m.total_sec);
        assert!((m.tick_to_sec(q) - 1.0).abs() < 1e-9);
        assert!((m.tick_to_sec(q + q / 2) - 2.5).abs() < 1e-9, "halfway through the held beat");
        assert!((m.tick_to_sec(2 * q) - 4.0).abs() < 1e-9);
        assert!((m.rate_at(q) / m.rate_at(0) - 3.0).abs() < 1e-9);
        let beats = m.beat_times(&s, 0);
        assert_eq!(beats.iter().map(|b| b.sec.round() as i64).collect::<Vec<_>>(), vec![0, 1, 4, 5]);
        // Past the end time runs on at the bar's tempo.
        assert!((m.tick_to_sec(5 * q) - 7.0).abs() < 1e-9);
    }

    #[test]
    fn beat_lookup() {
        let s = Score::new();
        let m = TempoMap::new(&s);
        let beats = m.beat_times(&s, 0);
        assert_eq!(beats.len(), 1);
        assert_eq!(locate(&beats, 0.5).map(|b| b.measure), Some(0));
        assert_eq!(locate(&beats, -1.0), None);
        assert!(m.beat_times(&s, 5).is_empty());
    }
}
