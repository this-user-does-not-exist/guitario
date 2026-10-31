//! Voices: Guitar Pro's Edit › Voice 1 to 4. Each voice is its own line of beats in a bar; the
//! cursor edits one of them, and every beat command works on the cursor's voice.

use crate::commands::{CommandSpec, cmd};
use crate::drum_grid::lengths;
use crate::{EngineError, Session, params as p, session::voice_at_mut};
use guitario_score::{Beat, MAX_VOICES, Voice};
use guitario_theory::Duration;
use serde_json::{Value, json};
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd("edit.setVoice", "Voice", "Edit/Voice", None, "voice (1..=4)", true, set_voice),
        cmd(
            "note.moveToVoice",
            "Move Note to Voice",
            "Edit/Voice",
            None,
            "voice (1..=4): moves the cursor's note to the same moment in that voice, with its beat's length where that voice rests (its rests are split), or into its beat that starts there; refused where that voice holds a note across the moment or on the string",
            true,
            move_to_voice,
        ),
    ]
}

/// The voice param as an index.
fn voice_param(params: &Value) -> Result<usize, EngineError> {
    let voice = p::uint(params, "voice", MAX_VOICES as u64)? as usize;
    voice.checked_sub(1).ok_or_else(|| EngineError::BadParam(format!("`voice` is 1..={MAX_VOICES}")))
}

/// Rests filling `ticks`, straight if they can be, else with triplets.
fn rests(ticks: u32) -> Result<Vec<Beat>, EngineError> {
    let ds = lengths(ticks, false).or_else(|| lengths(ticks, true)).ok_or_else(|| EngineError::invalid("the rests around the note cannot be written"))?;
    Ok(ds.into_iter().map(Beat::rest).collect())
}

/// Makes `voice` hold a beat of `len` starting at `at` and returns its index: the beat already
/// starting there if it plays, else carved from the rests that cover `at..at + len`.
fn place(voice: &mut Voice, at: u32, len: Duration, capacity: u32) -> Result<usize, EngineError> {
    let mut starts = Vec::with_capacity(voice.beats.len());
    let mut t = 0u32;
    for b in &voice.beats {
        starts.push(t);
        t = t.saturating_add(b.duration.ticks());
    }
    // A bar the voice has not filled is padded with rests to its end first.
    if t < capacity {
        for r in rests(capacity - t)? {
            starts.push(t);
            t = t.saturating_add(r.duration.ticks());
            voice.beats.push(r);
        }
    }
    if let Some(i) = starts.iter().position(|s| *s == at).filter(|i| voice.beats.get(*i).is_some_and(|b| !b.rest)) {
        return Ok(i);
    }
    let end = at.saturating_add(len.ticks());
    // The run of beats that overlap the new one must all be rests.
    let first = starts.iter().rposition(|s| *s <= at).ok_or_else(|| EngineError::invalid("the note's moment is outside the bar"))?;
    let mut last = first;
    while starts.get(last + 1).is_some_and(|s| *s < end) {
        last += 1;
    }
    let run = voice.beats.get(first..=last).unwrap_or_default();
    if run.iter().any(|b| !b.rest) {
        return Err(EngineError::invalid("that voice plays a note across this moment"));
    }
    let (from, to) = (starts.get(first).copied().unwrap_or(0), starts.get(last).copied().unwrap_or(0) + run.last().map_or(0, |b| b.duration.ticks()));
    if to < end {
        return Err(EngineError::invalid("the note runs past the end of the bar in that voice"));
    }
    let mut out = rests(at - from)?;
    let index = first + out.len();
    out.push(Beat { rest: false, ..Beat::rest(len) });
    out.extend(rests(to - end)?);
    voice.beats.splice(first..=last, out);
    Ok(index)
}

/// Guitar Pro's "move note to voice": the cursor's note leaves its beat (a rest if it was the
/// last) and sounds at the same moment in `voice`. The cursor stays where it was.
fn move_to_voice(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let to = voice_param(params)?;
    let c = s.cursor;
    if to == c.voice {
        return Err(EngineError::invalid("the note is already in that voice"));
    }
    let capacity = s.capacity();
    let (at, len, note) = {
        let voice = s.voice_in(c.measure).ok_or_else(|| EngineError::invalid("the cursor is not in a measure"))?;
        let beat = voice.beats.get(c.beat).filter(|b| !b.rest).ok_or_else(|| EngineError::invalid("there is no note at the cursor"))?;
        let note = *u8::try_from(c.string).ok().and_then(|st| beat.note_on(st)).ok_or_else(|| EngineError::invalid("there is no note at the cursor"))?;
        let at: u32 = voice.beats.iter().take(c.beat).map(|b| b.duration.ticks()).sum();
        (at, beat.duration, note)
    };
    let score = Arc::make_mut(&mut s.score);
    let target = voice_at_mut(score, c.track, c.measure, to).ok_or_else(|| EngineError::invalid("the cursor is not in a measure"))?;
    let i = place(target, at, len, capacity)?;
    let beat = target.beats.get_mut(i).ok_or_else(|| EngineError::invalid("the note's beat was not made"))?;
    if beat.note_on(note.string).is_some() {
        return Err(EngineError::invalid("that voice already has a note on this string here"));
    }
    beat.notes.push(note);
    beat.notes.sort_by_key(|n| n.string);
    beat.rest = false;
    let from = s.beat_mut()?;
    from.notes.retain(|n| n.string != note.string);
    from.rest = from.notes.is_empty();
    Ok(json!({"voice": to + 1, "beat": i}))
}

/// Moves the cursor to a voice, adding it to the bar (one bar-long rest) when the bar lacks it.
fn set_voice(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let voice = voice_param(params)?;
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

    #[test]
    fn a_note_moves_to_another_voice_at_the_same_moment() {
        let mut s = Session::new();
        // Voice 1: a chord of two quarters, then three quarter rests.
        s.run("beat.setDuration", &json!({"value": 4})).unwrap();
        s.run("note.setFret", &json!({"fret": 3})).unwrap();
        s.run("cursor.right", &json!(null)).unwrap();
        s.run("note.setFret", &json!({"fret": 5})).unwrap();
        s.run("cursor.set", &json!({"string": 4})).unwrap();
        s.run("note.setFret", &json!({"fret": 7})).unwrap();
        // The low note of beat 2 goes to Voice 2: its rest is split around a quarter.
        let r = s.run("note.moveToVoice", &json!({"voice": 2})).unwrap();
        assert_eq!((r["voice"].as_u64(), r["beat"].as_u64()), (Some(2), Some(1)));
        let m = &s.score().tracks[0].measures[0];
        let q = guitario_theory::TICKS_PER_QUARTER;
        let v2: Vec<(u32, bool)> = m.voices[1].beats.iter().map(|b| (b.duration.ticks(), b.rest)).collect();
        assert_eq!(v2, vec![(q, true), (q, false), (2 * q, true)]);
        assert_eq!(m.voices[1].beats[1].notes[0].fret, 7);
        assert_eq!(m.voices[0].beats[1].notes.iter().map(|n| n.fret).collect::<Vec<_>>(), vec![5], "the other note stays");
        assert_eq!(s.cursor().voice, 0, "the cursor stays");
        assert!(s.score().validate().is_ok());
        // The note it leaves alone becomes a rest when moved too; both now play from Voice 2.
        s.run("cursor.set", &json!({"string": 0})).unwrap();
        s.run("note.moveToVoice", &json!({"voice": 2})).unwrap();
        let m = &s.score().tracks[0].measures[0];
        assert!(m.voices[0].beats[1].rest);
        assert_eq!(m.voices[1].beats[1].notes.iter().map(|n| n.fret).collect::<Vec<_>>(), vec![5, 7]);
        // Both in one beat sound once each, at the same moment as before.
        let played = guitario_score::timeline(s.score());
        assert_eq!(played.iter().filter(|p| p.start_tick == u64::from(q)).count(), 2);
        // One undo step each.
        s.run("edit.undo", &json!(null)).unwrap();
        assert_eq!(s.score().tracks[0].measures[0].voices[0].beats[1].notes.len(), 1);
    }

    #[test]
    fn a_note_does_not_move_onto_a_note_sounding_across_its_moment() {
        let mut s = Session::new();
        // Voice 2 holds a half note from beat 1; Voice 1's second quarter cannot join it.
        s.run("edit.setVoice", &json!({"voice": 2})).unwrap();
        s.run("beat.setDuration", &json!({"value": 2})).unwrap();
        s.run("note.setFret", &json!({"fret": 0})).unwrap();
        s.run("edit.setVoice", &json!({"voice": 1})).unwrap();
        s.run("beat.setDuration", &json!({"value": 4})).unwrap();
        s.run("note.setFret", &json!({"fret": 1})).unwrap();
        s.run("cursor.right", &json!(null)).unwrap();
        s.run("note.setFret", &json!({"fret": 2})).unwrap();
        let before = s.score().clone();
        let e = s.run("note.moveToVoice", &json!({"voice": 2})).unwrap_err().to_string();
        assert!(e.contains("across"), "{e}");
        // On the downbeat it joins Voice 2's half note, unless the string is taken.
        s.run("cursor.left", &json!(null)).unwrap();
        let e = s.run("note.moveToVoice", &json!({"voice": 2})).unwrap_err().to_string();
        assert!(e.contains("string"), "{e}");
        assert_eq!(**s.score(), *before, "refusals change nothing");
        s.run("cursor.set", &json!({"string": 2})).unwrap();
        s.run("note.setFret", &json!({"fret": 4})).unwrap();
        s.run("note.moveToVoice", &json!({"voice": 2})).unwrap();
        assert_eq!(s.score().tracks[0].measures[0].voices[1].beats[0].notes.len(), 2);
        // Hostile params and empty beats change nothing.
        for bad in [json!({"voice": 1}), json!({"voice": 0}), json!({"voice": 9}), json!({})] {
            assert!(s.run("note.moveToVoice", &bad).is_err(), "{bad}");
        }
        s.run("cursor.set", &json!({"string": 5})).unwrap();
        assert!(s.run("note.moveToVoice", &json!({"voice": 3})).is_err(), "no note on that string");
    }
}
