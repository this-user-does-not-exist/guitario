use guitario_score::{Beat, NoteKind, Score, Track};
use guitario_theory::note_name;

const WIDTH: usize = 78;

/// Plain-text tab for every track, wrapped at 78 columns.
pub fn render(score: &Score) -> String {
    let mut out = String::new();
    if !score.meta.title.is_empty() {
        out.push_str(&format!("{}\n", score.meta.title));
        if !score.meta.artist.is_empty() {
            out.push_str(&format!("{}\n", score.meta.artist));
        }
        out.push('\n');
    }
    for track in &score.tracks {
        out.push_str(&format!("{} (tuning: {}, capo {})\n\n", track.name, track.tuning.name, track.capo));
        render_track(track, &mut out);
    }
    out
}

fn render_track(track: &Track, out: &mut String) {
    let rows = track.string_count();
    // Drum tab names each kit lane and marks hits: `x` for cymbals and hi-hats, `o` for drums.
    let labels: Vec<String> = if track.is_drums() {
        ["CY", "RD", "HH", "TT", "SD", "FT", "HF", "BD"].iter().take(rows).map(|l| (*l).to_string()).collect()
    } else {
        track.tuning.strings.iter().map(|p| note_name(*p).to_string()).collect()
    };
    let label_w = labels.iter().map(String::len).max().unwrap_or(1);
    let mut system: Vec<String> = vec![String::new(); rows];
    let flush = |system: &mut Vec<String>, out: &mut String| {
        if system.iter().all(String::is_empty) {
            return;
        }
        for (label, line) in labels.iter().zip(system.iter()) {
            out.push_str(&format!("{label:<label_w$}|{line}\n"));
        }
        out.push('\n');
        system.iter_mut().for_each(String::clear);
    };
    for measure in &track.measures {
        let mut cols: Vec<String> = vec![String::new(); rows];
        // One column per moment a beat starts in any voice, so voices line up as they sound.
        let mut columns: std::collections::BTreeMap<u64, Vec<&Beat>> = std::collections::BTreeMap::new();
        for voice in &measure.voices {
            let mut at = 0u64;
            for beat in &voice.beats {
                columns.entry(at).or_default().push(beat);
                at += u64::from(beat.duration.ticks());
            }
        }
        for beats in columns.values() {
            let cell = |string: usize| -> String {
                let note = beats.iter().filter(|b| !b.rest).find_map(|b| b.notes.iter().find(|n| usize::from(n.string) == string));
                note.map_or(String::new(), |n| match n.kind {
                    NoteKind::Dead => "x".to_string(),
                    NoteKind::Tie => format!("({})", n.fret),
                    NoteKind::Normal if track.is_drums() => {
                        if guitario_theory::drums::piece(n.fret).is_some_and(|p| p.cross) {
                            "x".into()
                        } else {
                            "o".into()
                        }
                    }
                    NoteKind::Normal => n.fret.to_string(),
                })
            };
            let w = (0..rows).map(|s| cell(s).len()).max().unwrap_or(0).max(1) + 1;
            for (s, col) in cols.iter_mut().enumerate() {
                col.push_str(&format!("{:-<w$}", cell(s)));
            }
        }
        let measure_w = cols.first().map_or(0, String::len) + 1;
        if system.first().map_or(0, String::len) + measure_w + label_w + 1 > WIDTH {
            flush(&mut system, out);
        }
        for (line, col) in system.iter_mut().zip(&cols) {
            line.push_str(col);
            line.push('|');
        }
    }
    flush(&mut system, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::{Beat, Note};
    use guitario_theory::Duration;

    #[test]
    fn renders_a_chord() {
        let mut s = Score::new();
        s.tracks[0].measures[0].voices[0].beats = vec![Beat { rest: false, notes: vec![Note::new(0, 0), Note::new(5, 12)], ..Beat::rest(Duration::WHOLE) }];
        let t = render(&s);
        assert!(t.contains("E|0--|"), "{t}");
        assert!(t.lines().any(|l| l.starts_with("E|12-|")), "{t}");
    }

    #[test]
    fn voices_share_columns_by_time() {
        use guitario_score::Voice;
        let mut s = Score::new();
        let note = |string: u8, fret: u8, d: Duration| Beat { rest: false, notes: vec![Note::new(string, fret)], ..Beat::rest(d) };
        let half = Duration::new(guitario_theory::NoteValue::Half);
        s.tracks[0].measures[0].voices = vec![
            Voice { beats: vec![note(0, 3, half), note(0, 5, half)] },
            Voice { beats: vec![note(5, 0, Duration::QUARTER), note(5, 2, Duration::QUARTER), note(5, 4, half)] },
        ];
        let t = render(&s);
        // Voice 1 starts at 0 and on beat 3, voice 2 at 0, 1 and 2: three columns, each two wide.
        let rows: Vec<&str> = t.lines().filter(|l| l.contains('|')).collect();
        assert_eq!(rows.first().copied(), Some("E|3---5-|"), "{t}");
        assert_eq!(rows.get(5).copied(), Some("E|0-2-4-|"), "{t}");
    }

    #[test]
    fn drum_tab_names_lanes_and_marks_hits() {
        let mut s = Score::new();
        s.tracks = vec![guitario_score::Track::drums("Drums", &s.headers)];
        s.tracks[0].measures[0].voices[0].beats =
            vec![Beat { rest: false, notes: vec![Note::new(2, 42), Note::new(4, 38), Note::new(7, 36)], ..Beat::rest(Duration::WHOLE) }];
        let t = render(&s);
        assert!(t.contains("HH|x-|"), "{t}");
        assert!(t.contains("SD|o-|"), "{t}");
        assert!(t.contains("BD|o-|"), "{t}");
    }

    #[test]
    fn wraps_long_scores() {
        let mut s = Score::new();
        for _ in 0..40 {
            let h = s.headers[0].clone();
            crate::session::append_measure(&mut s, h);
        }
        assert!(render(&s).lines().all(|l| l.len() <= 90));
    }
}
