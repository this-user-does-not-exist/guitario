//! Voices: Guitar Pro's Edit › Voice 1 to 4. Each voice is its own line of beats in a bar; the
//! cursor edits one of them, and every beat command works on the cursor's voice.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p, session::voice_at_mut};
use guitario_score::MAX_VOICES;
use serde_json::Value;
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd("edit.setVoice", "Voice", "Edit/Voice", None, "voice (1..=4)", true, set_voice)]
}

/// Moves the cursor to a voice, adding it to the bar (one bar-long rest) when the bar lacks it.
fn set_voice(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let voice = p::uint(params, "voice", MAX_VOICES as u64)? as usize;
    let voice = voice.checked_sub(1).ok_or_else(|| EngineError::BadParam(format!("`voice` is 1..={MAX_VOICES}")))?;
    let c = s.cursor;
    voice_at_mut(Arc::make_mut(&mut s.score), c.track, c.measure, voice).ok_or_else(|| EngineError::invalid("the cursor is not in a measure"))?;
    s.cursor.voice = voice;
    s.selection = None;
    Ok(s.cursor_json())
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::json;

    fn voices(s: &Session, measure: usize) -> usize {
        s.score().tracks[0].measures[measure].voices.len()
    }

    #[test]
    fn a_second_voice_is_added_edited_and_sounds_with_the_first() {
        let mut s = Session::new();
        s.run("note.setFret", &json!({"fret": 3})).unwrap();
        assert_eq!(voices(&s, 0), 1);
        // Voice 2 starts as a bar-long rest and takes its own notes on the same string.
        let r = s.run("edit.setVoice", &json!({"voice": 2})).unwrap();
        assert_eq!(r["voice"], 1);
        assert_eq!(voices(&s, 0), 2);
        assert!(s.score().tracks[0].measures[0].voices[1].beats[0].rest);
        s.run("beat.setDuration", &json!({"value": 2})).unwrap();
        s.run("note.setFret", &json!({"fret": 7})).unwrap();
        let m = &s.score().tracks[0].measures[0];
        assert_eq!(m.voices[0].beats[0].notes[0].fret, 3, "voice 1 is untouched");
        assert_eq!((m.voices[1].beats[0].notes[0].fret, m.voices[1].beats[0].duration.ticks()), (7, 2 * guitario_theory::TICKS_PER_QUARTER));
        // Both voices play from the start of the bar.
        let played = guitario_score::timeline(s.score());
        assert_eq!(played.iter().filter(|p| p.start_tick == 0).count(), 2);
        // Back to voice 1: its beat is the one under the cursor again.
        s.run("edit.setVoice", &json!({"voice": 1})).unwrap();
        s.run("note.setFret", &json!({"fret": 5})).unwrap();
        assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[0].notes[0].fret, 5);
        assert_eq!(s.score().tracks[0].measures[0].voices[1].beats[0].notes[0].fret, 7);
    }

    #[test]
    fn adding_a_voice_is_one_undo_step_and_switching_back_is_none() {
        let mut s = Session::new();
        s.run("edit.setVoice", &json!({"voice": 3})).unwrap();
        assert_eq!(voices(&s, 0), 3, "voice 2 is filled in as a rest so voice 3 has a place");
        s.run("edit.setVoice", &json!({"voice": 1})).unwrap();
        s.run("edit.setVoice", &json!({"voice": 3})).unwrap();
        s.run("edit.undo", &json!(null)).unwrap();
        assert_eq!((voices(&s, 0), s.cursor().voice), (1, 0));
        assert!(s.run("edit.undo", &json!(null)).is_err(), "only the add was recorded");
    }

    #[test]
    fn hostile_voice_params_change_nothing() {
        let mut s = Session::new();
        for bad in [json!({"voice": 0}), json!({"voice": 5}), json!({"voice": -1}), json!({"voice": "two"}), json!({}), json!({"voice": 1.5})] {
            assert!(s.run("edit.setVoice", &bad).is_err(), "{bad}");
        }
        assert_eq!((voices(&s, 0), s.cursor().voice), (1, 0));
    }

    #[test]
    fn the_cursor_keeps_its_voice_across_bars_and_clipboard_copies_that_voice() {
        let mut s = Session::new();
        s.run("measure.append", &json!(null)).unwrap();
        s.run("edit.setVoice", &json!({"voice": 2})).unwrap();
        s.run("note.setFret", &json!({"fret": 9})).unwrap();
        s.run("edit.copy", &json!(null)).unwrap();
        s.run("cursor.right", &json!(null)).unwrap();
        assert_eq!((s.cursor().measure, s.cursor().voice), (1, 1));
        s.run("edit.paste", &json!(null)).unwrap();
        let m = &s.score().tracks[0].measures[1];
        assert_eq!(m.voices.len(), 2, "the paste went into voice 2 of bar 2");
        assert_eq!(m.voices[1].beats[0].notes[0].fret, 9);
        assert!(m.voices[0].beats[0].rest);
    }
}
