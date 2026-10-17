//! Guitar Pro's Section menu: moving the cursor from one section marker to the next. Sections
//! themselves are set with `measure.setMarker` (Section › Edit Section…).

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session};
use serde_json::Value;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd("cursor.nextSection", "Next Section", "Section", Some("Cmd+Alt+Right"), "", false, |s, _| jump(s, true)),
        cmd("cursor.previousSection", "Previous Section", "Section", Some("Cmd+Alt+Left"), "", false, |s, _| jump(s, false)),
    ]
}

/// The bar of the next (or previous) section start from bar `from`, if there is one.
pub fn section_from(score: &guitario_score::Score, from: usize, forward: bool) -> Option<usize> {
    let starts = |i: &usize| score.headers.get(*i).is_some_and(|h| h.marker.is_some());
    if forward { (from.saturating_add(1)..score.headers.len()).find(starts) } else { (0..from.min(score.headers.len())).rev().find(starts) }
}

/// Moves the cursor to the first beat of the next or previous section; a cursor inside a section
/// goes back to that section's start first, as Guitar Pro does.
fn jump(s: &mut Session, forward: bool) -> Result<Value, EngineError> {
    let c = s.cursor;
    let target = if forward || c.beat > 0 && s.score.headers.get(c.measure).is_some_and(|h| h.marker.is_some()) {
        if forward { section_from(&s.score, c.measure, true) } else { Some(c.measure) }
    } else {
        section_from(&s.score, c.measure, false)
    };
    let measure = target.ok_or_else(|| EngineError::invalid(if forward { "no section after the cursor" } else { "no section before the cursor" }))?;
    s.cursor.measure = measure;
    s.cursor.beat = 0;
    s.selection = None;
    Ok(s.cursor_json())
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::{Value, json};

    fn at(s: &Session) -> (usize, usize) {
        (s.cursor.measure, s.cursor.beat)
    }

    #[test]
    fn next_and_previous_section_walk_the_markers() {
        let mut s = Session::new();
        for _ in 0..5 {
            s.run("measure.append", &Value::Null).unwrap();
        }
        for (bar, name) in [(1, "Verse"), (4, "Chorus")] {
            s.run("cursor.set", &json!({"measure": bar, "beat": 0})).unwrap();
            s.run("measure.setMarker", &json!({"text": name, "letter": "A"})).unwrap();
        }
        s.run("cursor.set", &json!({"measure": 0, "beat": 0})).unwrap();
        s.run("cursor.nextSection", &Value::Null).unwrap();
        assert_eq!(at(&s), (1, 0));
        s.run("cursor.nextSection", &Value::Null).unwrap();
        assert_eq!(at(&s), (4, 0));
        assert!(s.run("cursor.nextSection", &Value::Null).is_err(), "nothing after the last section");
        assert_eq!(at(&s), (4, 0), "a refused move leaves the cursor");
        s.run("cursor.set", &json!({"measure": 5, "beat": 0})).unwrap();
        s.run("cursor.previousSection", &Value::Null).unwrap();
        assert_eq!(at(&s), (4, 0));
        s.run("cursor.previousSection", &Value::Null).unwrap();
        assert_eq!(at(&s), (1, 0));
        assert!(s.run("cursor.previousSection", &Value::Null).is_err());
    }

    #[test]
    fn a_section_has_a_letter_and_a_name_and_clears_when_both_are_empty() {
        let mut s = Session::new();
        s.run("measure.setMarker", &json!({"text": "[Pre-Chorus]", "letter": "B"})).unwrap();
        let h = &s.score().headers[0];
        assert_eq!((h.section_letter.as_deref(), h.marker.as_deref()), (Some("B"), Some("[Pre-Chorus]")));
        s.run("measure.setMarker", &json!({"text": "", "letter": "C"})).unwrap();
        let h = &s.score().headers[0];
        assert_eq!((h.section_letter.as_deref(), h.marker.as_deref()), (Some("C"), Some("")));
        assert!(s.run("measure.setMarker", &json!({"text": "Solo", "letter": "ABCDE"})).is_err());
        s.run("measure.setMarker", &json!({"text": ""})).unwrap();
        let h = &s.score().headers[0];
        assert_eq!((h.section_letter.as_deref(), h.marker.as_deref()), (None, None));
    }
}
