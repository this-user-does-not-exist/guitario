use crate::Track;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScoreError {
    #[error("score has no measures")]
    NoMeasures,
    #[error("score has no tracks")]
    NoTracks,
    #[error("track {track} has {found} measures but the score has {expected}")]
    MeasureCount { track: usize, found: usize, expected: usize },
    #[error("track {track} measure {measure}: {what}")]
    Measure { track: usize, measure: usize, what: String },
    #[error("track {track}: {what}")]
    Track { track: usize, what: String },
    #[error("tempo must be 20..=400 bpm, got {0}")]
    Tempo(u16),
    #[error("measure {0} lists a direction mark twice")]
    Directions(usize),
}

impl crate::Score {
    /// Checks every invariant the engine and the exporters rely on. Files from disk are
    /// validated with this before they are accepted.
    pub fn validate(&self) -> Result<(), ScoreError> {
        if self.headers.is_empty() {
            return Err(ScoreError::NoMeasures);
        }
        if self.tracks.is_empty() {
            return Err(ScoreError::NoTracks);
        }
        for (mi, h) in self.headers.iter().enumerate() {
            if h.targets.iter().enumerate().any(|(i, t)| h.targets.iter().take(i).any(|u| u == t)) {
                return Err(ScoreError::Directions(mi));
            }
        }
        for t in self.headers.iter().filter_map(|h| h.tempo).chain([self.tempo]) {
            if !(20..=400).contains(&t) {
                return Err(ScoreError::Tempo(t));
            }
        }
        for (ti, track) in self.tracks.iter().enumerate() {
            self.validate_track(ti, track)?;
        }
        Ok(())
    }

    fn validate_track(&self, ti: usize, track: &Track) -> Result<(), ScoreError> {
        track.tuning.validate().map_err(|e| ScoreError::Track { track: ti, what: e.to_string() })?;
        if track.is_drums() && (track.capo != 0 || track.tuning.strings.iter().any(|p| *p != 0)) {
            return Err(ScoreError::Track { track: ti, what: "a drum track has no capo and its lanes are tuned to 0".into() });
        }
        if track.program > 127 || track.volume > 127 {
            return Err(ScoreError::Track { track: ti, what: "program and volume must be 0..=127".into() });
        }
        if track.measures.len() != self.headers.len() {
            return Err(ScoreError::MeasureCount { track: ti, found: track.measures.len(), expected: self.headers.len() });
        }
        let strings = track.string_count();
        for (mi, (measure, header)) in track.measures.iter().zip(&self.headers).enumerate() {
            let bad = |what: String| ScoreError::Measure { track: ti, measure: mi, what };
            if measure.voices.is_empty() {
                return Err(bad("has no voice".into()));
            }
            for voice in &measure.voices {
                if voice.ticks() > header.capacity() {
                    return Err(bad(format!("holds {} ticks but fits {}", voice.ticks(), header.capacity())));
                }
                for beat in &voice.beats {
                    if beat.duration.ticks() == 0 {
                        return Err(bad("has a zero-length beat".into()));
                    }
                    if beat.rest && !beat.notes.is_empty() {
                        return Err(bad("has a rest carrying notes".into()));
                    }
                    for (i, note) in beat.notes.iter().enumerate() {
                        if usize::from(note.string) >= strings {
                            return Err(bad(format!("uses string {} on a {strings}-string track", note.string)));
                        }
                        if note.fret > track.max_fret() {
                            return Err(bad(format!("fret {} is above {}", note.fret, track.max_fret())));
                        }
                        if beat.notes.iter().take(i).any(|n| n.string == note.string) {
                            return Err(bad(format!("has two notes on string {}", note.string)));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Beat, Note, Score};
    use guitario_theory::Duration;

    #[test]
    fn new_score_is_valid() {
        assert_eq!(Score::new().validate(), Ok(()));
    }

    #[test]
    fn rejects_broken_scores() {
        let mut s = Score::new();
        s.tracks[0].measures.pop();
        assert!(matches!(s.validate(), Err(ScoreError::MeasureCount { .. })));

        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats.push(Beat::rest(Duration::WHOLE));
        assert!(matches!(s.validate(), Err(ScoreError::Measure { .. })));

        let mut s = Score::new();
        let b = &mut s.tracks[0].measures[0].voices[0].beats[0];
        b.rest = false;
        b.notes = vec![Note::new(9, 0)];
        assert!(matches!(s.validate(), Err(ScoreError::Measure { .. })));

        let mut s = Score::new();
        s.tempo = 5;
        assert_eq!(s.validate(), Err(ScoreError::Tempo(5)));

        let mut s = Score::new();
        s.tracks.clear();
        assert_eq!(s.validate(), Err(ScoreError::NoTracks));
    }
}
