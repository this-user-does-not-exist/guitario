//! Moving around the score and growing a selection with the keyboard, as Guitar Pro's shortcut
//! list has it: Home and End within the bar, ⌘Home and ⌘End to the first and last bar, ⌘← and
//! ⌘→ by a bar, ⌘↑ and ⌘↓ (and Tab) between tracks, the same with Shift to select, and ⌥↑ and
//! ⌥↓ to move a note to the next string at the same pitch.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Selection, Session};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd("cursor.barStart", "Beginning of Bar", "", Some("Home"), "", false, |s, _| go(s, Target::BarStart)),
        cmd("cursor.barEnd", "End of Bar", "", Some("End"), "", false, |s, _| go(s, Target::BarEnd)),
        cmd("cursor.firstBar", "First Bar", "", Some("Cmd+Home"), "", false, |s, _| go(s, Target::FirstBar)),
        cmd("cursor.lastBar", "Last Bar", "", Some("Cmd+End"), "", false, |s, _| go(s, Target::LastBar)),
        cmd("cursor.nextBar", "Next Bar", "", Some("Cmd+Right"), "", false, |s, _| go(s, Target::NextBar)),
        cmd("cursor.previousBar", "Previous Bar", "", Some("Cmd+Left"), "", false, |s, _| go(s, Target::PreviousBar)),
        cmd("cursor.nextTrack", "Next Track", "", Some("Cmd+Down"), "", false, |s, _| track(s, 1)),
        cmd("cursor.previousTrack", "Previous Track", "", Some("Cmd+Up"), "", false, |s, _| track(s, -1)),
        cmd("cursor.nextStaff", "Next Staff", "", Some("Tab"), "", false, |s, _| track(s, 1)),
        cmd("cursor.previousStaff", "Previous Staff", "", Some("Shift+Tab"), "", false, |s, _| track(s, -1)),
        cmd("select.toBarStart", "Select to Beginning of Bar", "", Some("Shift+Home"), "", false, |s, _| select(s, Target::BarStart)),
        cmd("select.toBarEnd", "Select to End of Bar", "", Some("Shift+End"), "", false, |s, _| select(s, Target::BarEnd)),
        cmd("select.toFirstBar", "Select to First Bar", "", Some("Cmd+Shift+Home"), "", false, |s, _| select(s, Target::FirstBar)),
        cmd("select.toLastBar", "Select to Last Bar", "", Some("Cmd+Shift+End"), "", false, |s, _| select(s, Target::LastBar)),
        cmd("select.nextBar", "Select to Next Bar", "", Some("Cmd+Shift+Right"), "", false, |s, _| select(s, Target::NextBar)),
        cmd("select.previousBar", "Select to Previous Bar", "", Some("Cmd+Shift+Left"), "", false, |s, _| select(s, Target::PreviousBar)),
        cmd("select.beat", "Select Beat", "", Some("Shift+Up"), "", false, select_beat),
        cmd("note.stringUp", "Move Up", "Note", Some("Alt+Up"), "", true, |s, _| move_string(s, -1)),
        cmd("note.stringDown", "Move Down", "Note", Some("Alt+Down"), "", true, |s, _| move_string(s, 1)),
    ]
}

#[derive(Clone, Copy)]
enum Target {
    BarStart,
    BarEnd,
    FirstBar,
    LastBar,
    NextBar,
    PreviousBar,
}

impl Session {
    /// Where `target` is from the cursor, as `(bar, beat)` in the cursor's voice. Moving by a bar
    /// lands on its first beat; nothing moves past the first or last bar.
    fn target(&self, target: Target) -> (usize, usize) {
        let c = self.cursor;
        let last_bar = self.score.headers.len().saturating_sub(1);
        let last_beat = |m: usize| self.beats_in(m).saturating_sub(1);
        match target {
            Target::BarStart => (c.measure, 0),
            Target::BarEnd => (c.measure, last_beat(c.measure)),
            Target::FirstBar => (0, 0),
            Target::LastBar => (last_bar, last_beat(last_bar)),
            Target::NextBar => (c.measure.saturating_add(1).min(last_bar), 0),
            Target::PreviousBar => (c.measure.saturating_sub(1), 0),
        }
    }
}

fn go(s: &mut Session, target: Target) -> Result<Value, EngineError> {
    let (m, b) = s.target(target);
    s.selection = None;
    s.cursor.measure = m;
    s.cursor.beat = b;
    s.clamp_cursor();
    Ok(s.cursor_json())
}

fn track(s: &mut Session, step: isize) -> Result<Value, EngineError> {
    let to = s.cursor.track.checked_add_signed(step).filter(|t| *t < s.score.tracks.len());
    if let Some(t) = to {
        s.selection = None;
        s.cursor.track = t;
        s.cursor.voice = 0;
        s.clamp_cursor();
    }
    Ok(s.cursor_json())
}

/// Grows the selection (or starts one at the cursor) to `target`.
fn select(s: &mut Session, target: Target) -> Result<Value, EngineError> {
    let c = s.cursor;
    let anchor = s.selection.filter(|x| x.track == c.track).map_or((c.measure, c.beat), |x| x.anchor);
    let (m, b) = s.target(target);
    // Selecting forwards by a bar takes in the whole bar it lands on.
    let head = match target {
        Target::NextBar => (m, s.beats_in(m).saturating_sub(1)),
        _ => (m, b),
    };
    s.cursor.measure = head.0;
    s.cursor.beat = head.1;
    s.selection = Some(Selection { track: c.track, anchor, head });
    Ok(json!({"selection": s.selection}))
}

/// Selects the beat under the cursor alone.
fn select_beat(s: &mut Session, _: &Value) -> Result<Value, EngineError> {
    let c = s.cursor;
    s.selection = Some(Selection { track: c.track, anchor: (c.measure, c.beat), head: (c.measure, c.beat) });
    Ok(json!({"selection": s.selection}))
}

/// Moves the note under the cursor to the next string up (-1) or down (+1), keeping its pitch.
fn move_string(s: &mut Session, step: isize) -> Result<Value, EngineError> {
    let c = s.cursor;
    let track = s.score.tracks.get(c.track).ok_or_else(|| EngineError::invalid("no such track"))?;
    if track.is_drums() {
        return Err(EngineError::invalid("drum hits have no strings to move between"));
    }
    let to = c.string.checked_add_signed(step).filter(|t| *t < track.string_count()).ok_or_else(|| EngineError::invalid("there is no string that way"))?;
    let (Some(&from_pitch), Some(&to_pitch)) = (track.tuning.strings.get(c.string), track.tuning.strings.get(to)) else {
        return Err(EngineError::invalid("there is no string that way"));
    };
    let max = track.max_fret();
    let beat = s.beat_mut()?;
    if beat.notes.iter().any(|n| usize::from(n.string) == to) {
        return Err(EngineError::invalid("the next string already has a note on this beat"));
    }
    let note = beat.notes.iter_mut().find(|n| usize::from(n.string) == c.string).ok_or_else(|| EngineError::invalid("no note under the cursor"))?;
    let fret = (i32::from(note.fret) + i32::from(from_pitch) - i32::from(to_pitch)).clamp(-1, i32::from(max) + 1);
    let fret = u8::try_from(fret).ok().filter(|f| *f <= max).ok_or_else(|| EngineError::invalid("the note cannot be played on that string"))?;
    note.fret = fret;
    note.string = u8::try_from(to).unwrap_or(note.string);
    beat.notes.sort_by_key(|n| n.string);
    s.cursor.string = to;
    Ok(s.cursor_json())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bars(n: usize) -> Session {
        let mut s = Session::new();
        for _ in 1..n {
            s.run("measure.append", &Value::Null).unwrap();
        }
        // Four quarter beats in the first bar.
        s.run("beat.setDuration", &json!({"value": 4})).unwrap();
        for _ in 0..3 {
            s.run("cursor.right", &Value::Null).unwrap();
        }
        assert_eq!(s.beats_in(0), 4);
        s.run("cursor.set", &json!({"measure": 0, "beat": 1})).unwrap();
        s
    }

    fn at(s: &Session) -> (usize, usize, usize) {
        let c = s.cursor();
        (c.track, c.measure, c.beat)
    }

    #[test]
    fn home_end_and_bar_jumps() {
        let mut s = bars(4);
        let before = s.score().clone();
        s.run("cursor.barEnd", &Value::Null).unwrap();
        assert_eq!(at(&s), (0, 0, 3));
        s.run("cursor.barStart", &Value::Null).unwrap();
        assert_eq!(at(&s), (0, 0, 0));
        s.run("cursor.lastBar", &Value::Null).unwrap();
        assert_eq!(at(&s).1, 3);
        s.run("cursor.firstBar", &Value::Null).unwrap();
        assert_eq!(at(&s), (0, 0, 0));
        s.run("cursor.nextBar", &Value::Null).unwrap();
        s.run("cursor.nextBar", &Value::Null).unwrap();
        assert_eq!(at(&s), (0, 2, 0));
        for _ in 0..5 {
            s.run("cursor.nextBar", &Value::Null).unwrap();
        }
        assert_eq!(at(&s).1, 3, "never past the last bar");
        for _ in 0..5 {
            s.run("cursor.previousBar", &Value::Null).unwrap();
        }
        assert_eq!(at(&s), (0, 0, 0));
        assert_eq!(*s.score(), before, "moving edits nothing");
    }

    #[test]
    fn tracks_and_staves() {
        let mut s = bars(2);
        s.run("track.add", &json!({"name": "Bass", "tuning": "bass"})).unwrap();
        s.run("cursor.set", &json!({"track": 0, "measure": 1, "string": 5})).unwrap();
        s.run("cursor.nextTrack", &Value::Null).unwrap();
        assert_eq!((at(&s).0, at(&s).1, s.cursor().string), (1, 1, 3), "same bar, the string clamped to the bass");
        s.run("cursor.nextTrack", &Value::Null).unwrap();
        assert_eq!(at(&s).0, 1, "no track below the last");
        s.run("cursor.previousStaff", &Value::Null).unwrap();
        assert_eq!(at(&s).0, 0);
        s.run("cursor.previousTrack", &Value::Null).unwrap();
        assert_eq!(at(&s).0, 0);
        s.run("cursor.nextStaff", &Value::Null).unwrap();
        assert_eq!(at(&s).0, 1);
    }

    #[test]
    fn selections_grow_by_bar_and_to_the_ends() {
        let mut s = bars(4);
        s.run("select.toBarEnd", &Value::Null).unwrap();
        assert_eq!(s.selection().map(|x| x.ordered()), Some(((0, 1), (0, 3))));
        s.run("select.toBarStart", &Value::Null).unwrap();
        assert_eq!(s.selection().map(|x| x.ordered()), Some(((0, 0), (0, 1))), "the anchor stays");
        s.run("select.clear", &Value::Null).unwrap();
        s.run("select.nextBar", &Value::Null).unwrap();
        s.run("select.nextBar", &Value::Null).unwrap();
        assert_eq!(s.selection().map(|x| x.ordered().1.0), Some(2));
        s.run("select.toLastBar", &Value::Null).unwrap();
        assert_eq!(s.selection().map(|x| x.ordered().1.0), Some(3));
        s.run("select.toFirstBar", &Value::Null).unwrap();
        assert_eq!(s.selection().map(|x| x.ordered()), Some(((0, 0), (0, 0))), "back to the anchor at the first beat");
        s.run("cursor.set", &json!({"measure": 2, "beat": 0})).unwrap();
        s.run("select.previousBar", &Value::Null).unwrap();
        assert_eq!(s.selection().map(|x| x.ordered()), Some(((1, 0), (2, 0))));
        s.run("select.beat", &Value::Null).unwrap();
        assert_eq!(s.selection().map(|x| x.ordered()), Some(((1, 0), (1, 0))));
    }

    #[test]
    fn a_note_moves_to_the_next_string_at_the_same_pitch() {
        let mut s = Session::new();
        // Fret 5 on the B string (string 1) is E4, fret 0 on the high E (string 0).
        s.run("cursor.set", &json!({"string": 1})).unwrap();
        s.run("note.setFret", &json!({"fret": 5})).unwrap();
        s.run("note.stringUp", &Value::Null).unwrap();
        let notes = |s: &Session| s.score().tracks[0].measures[0].voices[0].beats[0].notes.iter().map(|n| (n.string, n.fret)).collect::<Vec<_>>();
        assert_eq!((notes(&s), s.cursor().string), (vec![(0, 0)], 0));
        assert!(s.run("note.stringUp", &Value::Null).is_err(), "no string above the top");
        s.run("note.stringDown", &Value::Null).unwrap();
        s.run("note.stringDown", &Value::Null).unwrap();
        assert_eq!(notes(&s), vec![(2, 9)], "G string, fret 9");
        s.run("edit.undo", &Value::Null).unwrap();
        assert_eq!(notes(&s), vec![(1, 5)]);
        // Below the open string the note cannot go; onto an occupied string it may not.
        s.run("cursor.set", &json!({"string": 1})).unwrap();
        s.run("note.setFret", &json!({"fret": 0})).unwrap();
        assert!(s.run("note.stringDown", &Value::Null).is_ok(), "B open is G string fret 4");
        s.run("cursor.set", &json!({"string": 3})).unwrap();
        s.run("note.setFret", &json!({"fret": 2})).unwrap();
        assert!(s.run("note.stringUp", &Value::Null).is_err(), "string 2 is taken");
        s.run("cursor.set", &json!({"string": 4})).unwrap();
        assert!(s.run("note.stringUp", &Value::Null).is_err(), "no note under the cursor");
    }
}
