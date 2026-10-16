//! Track › Lyrics…: up to five lines of lyrics per track, each from a bar, laid one syllable per
//! beat of the first voice (see `guitario_score::syllables`).

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session};
use guitario_score::{LyricLine, MAX_LYRIC_LINES, MAX_LYRICS_TEXT};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd(
        "track.setLyrics",
        "Lyrics",
        "Track",
        None,
        "lines: [{text, measure? (0-based bar of the first syllable, default 0)}] (at most 5; replaces the cursor track's lyrics; [] clears)",
        true,
        set_lyrics,
    )]
}

fn set_lyrics(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let list = params.get("lines").and_then(Value::as_array).ok_or_else(|| EngineError::BadParam("`lines` must be an array".into()))?;
    if list.len() > MAX_LYRIC_LINES {
        return Err(EngineError::BadParam(format!("at most {MAX_LYRIC_LINES} lines of lyrics, not {}", list.len())));
    }
    let bars = s.score.headers.len();
    let mut lines = Vec::with_capacity(list.len());
    for (i, l) in list.iter().enumerate() {
        let text = l.get("text").and_then(Value::as_str).ok_or_else(|| EngineError::BadParam(format!("line {}: `text` must be a string", i + 1)))?;
        if text.len() > MAX_LYRICS_TEXT {
            return Err(EngineError::BadParam(format!("line {}: longer than {MAX_LYRICS_TEXT} bytes", i + 1)));
        }
        let measure = match l.get("measure") {
            None | Some(Value::Null) => 0,
            Some(v) => v
                .as_u64()
                .and_then(|m| usize::try_from(m).ok())
                .filter(|m| *m < bars)
                .ok_or_else(|| EngineError::BadParam(format!("line {}: `measure` must be a bar of the score, 0..{bars}", i + 1)))?,
        };
        lines.push(LyricLine { measure, text: text.to_string() });
    }
    // Empty lines at the end carry nothing.
    while lines.last().is_some_and(|l| l.text.trim().is_empty()) {
        lines.pop();
    }
    let track = s.track_mut()?;
    track.lyrics = lines;
    Ok(json!({"lines": track.lyrics.len()}))
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::{Value, json};

    #[test]
    fn lyrics_are_set_per_track_trimmed_and_undone() {
        let mut s = Session::new();
        s.run("measure.append", &Value::Null).unwrap();
        let r = s.run("track.setLyrics", &json!({"lines": [{"text": "Steel ho-ri-zon"}, {"text": "burn", "measure": 1}, {"text": "  "}]})).unwrap();
        assert_eq!(r["lines"], 2, "the blank last line is dropped");
        let l = &s.score().tracks[0].lyrics;
        assert_eq!((l[0].measure, l[0].text.as_str(), l[1].measure), (0, "Steel ho-ri-zon", 1));
        s.run("edit.undo", &Value::Null).unwrap();
        assert!(s.score().tracks[0].lyrics.is_empty());
        for bad in [
            json!({}),
            json!({"lines": "text"}),
            json!({"lines": [{"text": 5}]}),
            json!({"lines": [{"text": "a", "measure": 2}]}),
            json!({"lines": [{"text": "a", "measure": -1}]}),
            json!({"lines": [{}, {}, {}, {}, {}, {}]}),
            json!({"lines": [{"text": "x".repeat(guitario_score::MAX_LYRICS_TEXT + 1)}]}),
        ] {
            assert!(s.run("track.setLyrics", &bad).is_err(), "{bad}");
        }
        assert!(s.score().tracks[0].lyrics.is_empty(), "a refused call changes nothing");
        s.run("track.setLyrics", &json!({"lines": [{"text": "a"}]})).unwrap();
        s.run("track.setLyrics", &json!({"lines": []})).unwrap();
        assert!(s.score().tracks[0].lyrics.is_empty());
    }
}
