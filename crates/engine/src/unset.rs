//! Unset: removes one kind of effect (or every effect) from the selection, or from the beat under
//! the cursor, as the command palette's `unset <element>` does in Guitar Pro 8.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::{Beat, Dynamic, Note, NoteEffects, NoteKind};
use serde_json::{Value, json};

/// What can be unset, by name: note effects first, then beat effects, then `all`.
pub const UNSETTABLE: [&str; 33] = [
    "tie",
    "deadNote",
    "hammer",
    "slide",
    "harmonic",
    "bend",
    "vibrato",
    "wideVibrato",
    "palmMute",
    "letRing",
    "ghost",
    "accent",
    "heavyAccent",
    "staccato",
    "tapping",
    "leftHandTapping",
    "trill",
    "grace",
    "fingering",
    "whammy",
    "barVibrato",
    "tremolo",
    "stroke",
    "technique",
    "fade",
    "brush",
    "chord",
    "text",
    "dynamic",
    "slur",
    "beam",
    "effects",
    "all",
];

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd(
        "edit.unset",
        "Unset",
        "Edit",
        None,
        "effect: tie|deadNote|hammer|slide|harmonic|bend|vibrato|wideVibrato|palmMute|letRing|ghost|accent|heavyAccent|staccato|tapping|leftHandTapping|\
         trill|grace|fingering (note effects), whammy|barVibrato|tremolo|stroke|technique|fade|brush|chord|text|dynamic|slur|beam (beat effects), \
         effects (every note and beat effect) or all (effects, plus ties, dead notes, chords, text, dynamics, slurs and beams); \
         from every beat of the selection, or the beat under the cursor",
        true,
        unset,
    )]
}

/// Takes `what` off one note.
fn unset_note(n: &mut Note, what: &str) {
    let e = &mut n.effects;
    match what {
        "tie" => {
            if n.kind == NoteKind::Tie {
                n.kind = NoteKind::Normal;
            }
        }
        "deadNote" => {
            if n.kind == NoteKind::Dead {
                n.kind = NoteKind::Normal;
            }
        }
        "hammer" => e.hammer = false,
        "slide" => e.slide = None,
        "harmonic" => e.harmonic = None,
        "bend" => e.bend = None,
        "vibrato" => e.vibrato = false,
        "wideVibrato" => e.wide_vibrato = false,
        "palmMute" => e.palm_mute = false,
        "letRing" => e.let_ring = false,
        "ghost" => e.ghost = false,
        "accent" => e.accent = false,
        "heavyAccent" => e.heavy_accent = false,
        "staccato" => e.staccato = false,
        "tapping" => e.tapped = false,
        "leftHandTapping" => e.left_hand_tapped = false,
        "trill" => e.trill = None,
        "grace" => e.grace = None,
        "fingering" => (e.left_finger, e.right_finger) = (None, None),
        "effects" | "all" => {
            *e = NoteEffects::default();
            if what == "all" {
                n.kind = NoteKind::Normal;
            }
        }
        _ => {}
    }
}

/// Takes `what` off one beat and its notes.
fn unset_beat(b: &mut Beat, what: &str) {
    for n in &mut b.notes {
        unset_note(n, what);
    }
    let effects = matches!(what, "effects" | "all");
    let all = what == "all";
    if effects || what == "whammy" {
        b.whammy = None;
    }
    if effects || what == "barVibrato" {
        b.bar_vibrato = None;
    }
    if effects || what == "tremolo" {
        b.tremolo = None;
    }
    if effects || what == "stroke" {
        b.stroke = None;
    }
    if effects || what == "technique" {
        b.technique = None;
    }
    if effects || what == "fade" {
        b.fade = None;
    }
    if effects || what == "brush" {
        b.brush = None;
    }
    if all || what == "chord" {
        b.chord = None;
    }
    if all || what == "text" {
        b.text = None;
    }
    if all || what == "dynamic" {
        b.dynamic = Dynamic::default();
    }
    if all || what == "slur" {
        b.slur = None;
    }
    if all || what == "beam" {
        b.beam = None;
    }
}

fn unset(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let what = p::string(params, "effect")?;
    if !UNSETTABLE.contains(&what) {
        return Err(EngineError::BadParam(format!("`effect` cannot be `{what}`: it must be one of {}", UNSETTABLE.join(", "))));
    }
    let what = what.to_string();
    let mut beats = 0;
    crate::techniques::each_beat(s, |b| {
        unset_beat(b, &what);
        beats += 1;
    })?;
    Ok(json!({"ok": true, "beats": beats}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use guitario_score::{Bend, Whammy, WhammyKind};

    fn beats(s: &Session) -> &[Beat] {
        &s.score().tracks[0].measures[0].voices[0].beats
    }

    /// Two quarter notes with palm mute, a bend, a whammy and an accent, the second tied.
    fn busy() -> Session {
        let mut s = Session::new();
        for (beat, kind) in [(0, "normal"), (1, "tie")] {
            s.run("cursor.set", &json!({"measure": 0, "beat": beat, "string": 1})).unwrap();
            s.run("beat.setDuration", &json!({"value": 4})).unwrap();
            s.run("note.setFret", &json!({"fret": 7})).unwrap();
            s.run("note.setKind", &json!({"kind": kind})).ok();
            for fx in ["palmMute", "accent"] {
                s.run("note.toggleEffect", &json!({"effect": fx})).unwrap();
            }
            s.run("note.setBend", &json!({"quarterTones": 4})).unwrap();
            s.run("beat.setWhammy", &json!({"depth": -4})).unwrap();
            s.run("beat.setDynamic", &json!({"dynamic": "ff"})).unwrap();
            s.run("cursor.right", &json!(null)).unwrap();
        }
        s.run("select.set", &json!({"fromMeasure": 0, "fromBeat": 0, "toMeasure": 0, "toBeat": 1})).unwrap();
        s
    }

    #[test]
    fn one_effect_goes_from_the_selection_and_the_rest_stay() {
        let mut s = busy();
        s.run("edit.unset", &json!({"effect": "palmMute"})).unwrap();
        assert!(beats(&s).iter().take(2).all(|b| !b.notes[0].effects.palm_mute && b.notes[0].effects.accent));
        s.run("edit.unset", &json!({"effect": "whammy"})).unwrap();
        assert!(beats(&s).iter().take(2).all(|b| b.whammy.is_none() && b.notes[0].effects.bend == Some(Bend::to(4))));
        s.run("edit.unset", &json!({"effect": "tie"})).unwrap();
        assert_eq!(beats(&s)[1].notes[0].kind, NoteKind::Normal);
        s.run("edit.undo", &json!(null)).unwrap();
        assert_eq!(beats(&s)[1].notes[0].kind, NoteKind::Tie, "one undo step per unset");
    }

    #[test]
    fn effects_clears_every_effect_and_all_also_ties_and_dynamics() {
        let mut s = busy();
        s.run("edit.unset", &json!({"effect": "effects"})).unwrap();
        for b in beats(&s).iter().take(2) {
            assert_eq!((b.notes[0].effects, b.whammy, b.dynamic), (NoteEffects::default(), None, Dynamic::Ff));
        }
        assert_eq!(beats(&s)[1].notes[0].kind, NoteKind::Tie, "effects leaves ties");
        s.run("edit.unset", &json!({"effect": "all"})).unwrap();
        assert!(beats(&s).iter().take(2).all(|b| b.notes[0].kind == NoteKind::Normal && b.dynamic == Dynamic::default()));
    }

    #[test]
    fn without_a_selection_the_cursor_beat_and_bad_names_are_refused() {
        let mut s = busy();
        s.run("select.clear", &json!(null)).unwrap();
        s.run("cursor.set", &json!({"measure": 0, "beat": 0})).unwrap();
        s.run("edit.unset", &json!({"effect": "bend"})).unwrap();
        assert_eq!((beats(&s)[0].notes[0].effects.bend, beats(&s)[1].notes[0].effects.bend), (None, Some(Bend::to(4))));
        assert_eq!(beats(&s)[1].whammy, Some(Whammy::preset(WhammyKind::Dive, -4)));
        let before = (**s.score()).clone();
        for bad in [json!({"effect": "everything"}), json!({"effect": 3}), json!({})] {
            assert!(s.run("edit.unset", &bad).is_err(), "{bad}");
            assert_eq!(**s.score(), before);
        }
        // Every listed name is accepted.
        for name in UNSETTABLE {
            assert!(s.run("edit.unset", &json!({"effect": name})).is_ok(), "{name}");
        }
    }
}
