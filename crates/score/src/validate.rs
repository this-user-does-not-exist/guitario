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
    #[error("page setup: {0}")]
    Page(String),
    #[error("measure {0} has a fermata outside the bar, two at one place, or too many")]
    Fermatas(usize),
    #[error("measure {0} has an empty beam group or too many")]
    Beams(usize),
    #[error("master: {0}")]
    Master(String),
}

/// Most fermatas one bar may hold.
pub const MAX_FERMATAS: usize = 64;

/// Longest line of lyrics accepted, in bytes.
pub const MAX_LYRICS_TEXT: usize = 100_000;

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
        self.page.check().map_err(ScoreError::Page)?;
        self.master.check().map_err(ScoreError::Master)?;
        for (mi, h) in self.headers.iter().enumerate() {
            if h.targets.iter().enumerate().any(|(i, t)| h.targets.iter().take(i).any(|u| u == t)) {
                return Err(ScoreError::Directions(mi));
            }
            if h.fermatas.len() > MAX_FERMATAS
                || h.fermatas.iter().any(|f| f.tick >= h.capacity())
                || h.fermatas.iter().enumerate().any(|(i, f)| h.fermatas.iter().take(i).any(|g| g.tick == f.tick))
            {
                return Err(ScoreError::Fermatas(mi));
            }
            if h.beams.len() > crate::MAX_BEAM_GROUPS || h.beams.contains(&0) {
                return Err(ScoreError::Beams(mi));
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
        let sound = |what: String| ScoreError::Track { track: ti, what };
        if let Some(chain) = &track.effects {
            if chain.len() > crate::MAX_TRACK_EFFECTS {
                return Err(sound(format!("has {} effects; at most {}", chain.len(), crate::MAX_TRACK_EFFECTS)));
            }
            if chain.iter().any(|e| e.kind == crate::EffectType::MixerEq) {
                return Err(sound("the mixer's equalizer is not a chain effect".into()));
            }
            for e in chain {
                e.check().map_err(sound)?;
            }
        }
        if track.eq.kind != crate::EffectType::MixerEq {
            return Err(sound(format!("its equalizer is a {}", track.eq.kind.label())));
        }
        track.eq.check().map_err(sound)?;
        if track.bank.as_ref().is_some_and(|b| b.trim().is_empty() || b.len() > crate::MAX_BANK_NAME || b.contains('\0')) {
            return Err(sound(format!("its sound bank name is empty, has a NUL or is longer than {} bytes", crate::MAX_BANK_NAME)));
        }
        if track.lyrics.len() > crate::MAX_LYRIC_LINES {
            return Err(ScoreError::Track { track: ti, what: format!("has {} lines of lyrics; at most {}", track.lyrics.len(), crate::MAX_LYRIC_LINES) });
        }
        if track.lyrics.iter().any(|l| l.text.len() > MAX_LYRICS_TEXT) {
            return Err(ScoreError::Track { track: ti, what: format!("a line of lyrics is longer than {MAX_LYRICS_TEXT} bytes") });
        }
        if track.measures.len() != self.headers.len() {
            return Err(ScoreError::MeasureCount { track: ti, found: track.measures.len(), expected: self.headers.len() });
        }
        let strings = track.string_count();
        for (mi, (measure, header)) in track.measures.iter().zip(&self.headers).enumerate() {
            let bad = |what: String| ScoreError::Measure { track: ti, measure: mi, what };
            if measure.volume.is_some_and(|v| v > 127) || measure.pan.is_some_and(|p| !(-63..=63).contains(&p)) {
                return Err(bad("has a volume or pan automation out of range".into()));
            }
            if measure.voices.is_empty() {
                return Err(bad("has no voice".into()));
            }
            if measure.voices.len() > crate::MAX_VOICES {
                return Err(bad(format!("has {} voices; at most {} are supported", measure.voices.len(), crate::MAX_VOICES)));
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
                        if let Some(trill) = note.effects.trill
                            && trill.fret > track.max_fret()
                        {
                            return Err(bad(format!("trill fret {} is above {}", trill.fret, track.max_fret())));
                        }
                        if let Some(grace) = note.effects.grace
                            && (grace.fret > track.max_fret() || track.is_drums())
                        {
                            return Err(bad(format!("grace note fret {} is not allowed here", grace.fret)));
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
        s.tracks[0].measures[0].voices = vec![crate::Voice::default(); crate::MAX_VOICES + 1];
        assert!(matches!(s.validate(), Err(ScoreError::Measure { .. })));

        let mut s = Score::new();
        let b = &mut s.tracks[0].measures[0].voices[0].beats[0];
        b.rest = false;
        let mut n = Note::new(0, 3);
        n.effects.grace = Some(crate::Grace::new(37));
        b.notes = vec![n];
        assert!(matches!(s.validate(), Err(ScoreError::Measure { what, .. }) if what.contains("grace")));

        let mut s = Score::new();
        s.tracks[0].lyrics = vec![crate::LyricLine::default(); crate::MAX_LYRIC_LINES + 1];
        assert!(matches!(s.validate(), Err(ScoreError::Track { what, .. }) if what.contains("lyrics")));

        let mut s = Score::new();
        s.headers[0].fermatas = vec![crate::Fermata { tick: 4 * guitario_theory::TICKS_PER_QUARTER, kind: crate::FermataKind::Long, length: 0 }];
        assert_eq!(s.validate(), Err(ScoreError::Fermatas(0)), "past the end of the bar");
        s.headers[0].fermatas = vec![crate::Fermata { tick: 0, kind: crate::FermataKind::Long, length: 0 }; 2];
        assert_eq!(s.validate(), Err(ScoreError::Fermatas(0)), "two at one place");

        let mut s = Score::new();
        s.headers[0].beams = vec![2, 0];
        assert_eq!(s.validate(), Err(ScoreError::Beams(0)));

        let mut s = Score::new();
        s.tempo = 5;
        assert_eq!(s.validate(), Err(ScoreError::Tempo(5)));

        let mut s = Score::new();
        s.tracks.clear();
        assert_eq!(s.validate(), Err(ScoreError::NoTracks));
    }

    #[test]
    fn page_setup_is_checked() {
        use crate::{PageSetup, Paper};
        let mut s = Score::new();
        assert_eq!(s.page, PageSetup::default());
        s.page = PageSetup { paper: Paper::Letter, landscape: true, margin_left: 0, margin_right: 0, margin_top: 0, margin_bottom: 0, bend_pitch: false };
        assert_eq!(s.validate(), Ok(()));
        // A margin wider than allowed.
        s.page = PageSetup { margin_left: 61, ..PageSetup::default() };
        assert!(matches!(s.validate(), Err(ScoreError::Page(m)) if m.contains("left margin")));
        // Margins that leave too little paper, on a small sheet.
        s.page = PageSetup { paper: Paper::A5, margin_left: 40, margin_right: 40, ..PageSetup::default() };
        assert!(matches!(s.validate(), Err(ScoreError::Page(m)) if m.contains("A5")));
        // Orientation changes which side is short.
        s.page = PageSetup { paper: Paper::A5, landscape: true, margin_top: 60, margin_bottom: 60, ..PageSetup::default() };
        assert!(matches!(s.validate(), Err(ScoreError::Page(_))));
        s.page = PageSetup { paper: Paper::Legal, ..PageSetup::default() };
        assert_eq!(s.page.size_mm(), (215.9, 355.6));
        s.page.landscape = true;
        assert_eq!(s.page.size_mm(), (355.6, 215.9));
        assert_eq!(Paper::from_name(" letter "), Some(Paper::Letter));
        assert_eq!(Paper::from_name("tabloid"), None);
    }
}
