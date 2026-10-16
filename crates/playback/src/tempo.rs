use guitario_score::{Score, playback_order};
use guitario_theory::TICKS_PER_QUARTER;

/// One measure as played (repeats unrolled).
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub measure: usize,
    pub start_tick: u64,
    pub ticks: u32,
    pub start_sec: f64,
    pub sec_per_tick: f64,
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
            segments.push(Segment { measure: mi, start_tick: tick, ticks, start_sec: sec, sec_per_tick });
            tick += u64::from(ticks);
            sec += f64::from(ticks) * sec_per_tick;
        }
        TempoMap { segments, total_sec: sec }
    }

    fn segment_at_tick(&self, tick: u64) -> Option<&Segment> {
        let i = self.segments.partition_point(|s| s.start_tick <= tick);
        self.segments.get(i.saturating_sub(1))
    }

    pub fn tick_to_sec(&self, tick: u64) -> f64 {
        self.segment_at_tick(tick).map_or(0.0, |s| s.start_sec + (tick.saturating_sub(s.start_tick)) as f64 * s.sec_per_tick)
    }

    /// Seconds per tick at `tick`.
    pub fn rate_at(&self, tick: u64) -> f64 {
        self.segment_at_tick(tick).map_or(60.0 / (120.0 * f64::from(TICKS_PER_QUARTER)), |s| s.sec_per_tick)
    }

    /// Every beat of a track in playback order, for following the cursor during playback.
    pub fn beat_times(&self, score: &Score, track: usize) -> Vec<BeatTime> {
        let mut out = Vec::new();
        let Some(t) = score.tracks.get(track) else { return out };
        for seg in &self.segments {
            let Some(voice) = t.measures.get(seg.measure).and_then(|m| m.voices.first()) else { continue };
            let mut at = 0u32;
            for (bi, b) in voice.beats.iter().enumerate() {
                out.push(BeatTime { sec: seg.start_sec + f64::from(at) * seg.sec_per_tick, measure: seg.measure, beat: bi });
                at += b.duration.ticks();
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
