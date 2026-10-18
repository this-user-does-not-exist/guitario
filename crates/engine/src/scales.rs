//! The Scale tool: the scale library, and the scales the selected bars use.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use guitario_score::NoteKind;
use guitario_theory::scales::{SCALES, Tag, find_scales};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd(
            "tools.scales",
            "Scales",
            "",
            None,
            "search? (words in the name), tags? (all of: Usual, Scale, Mode, Major, Minor, Harmonic, Melodic, Pentatonic, Dominant, Blues, Arpeggio, Triad, Tetrachord, Symmetrical, Exotic, Bebop), key? (a note name: adds each scale's notes)",
            false,
            list,
        ),
        cmd(
            "tools.findScales",
            "Find Scales from Selection",
            "",
            None,
            "key? (a note name or 0..=11: only scales on it), limit? (1..=50, default 20): the scales the selected bars of every string track use, by the share of their notes",
            false,
            find,
        ),
    ]
}

fn list(_s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let words: Vec<String> = p::opt_string(params, "search").unwrap_or("").split_whitespace().map(str::to_lowercase).collect();
    let tags: Vec<Tag> = match params.get("tags").filter(|v| !v.is_null()) {
        None => Vec::new(),
        Some(v) => v
            .as_array()
            .ok_or_else(|| EngineError::BadParam("`tags` must be an array of tag names".into()))?
            .iter()
            .map(|t| t.as_str().and_then(Tag::from_label).ok_or_else(|| EngineError::BadParam(format!("unknown tag {t}"))))
            .collect::<Result<_, _>>()?,
    };
    let key = params.get("key").filter(|v| !v.is_null()).map(|k| crate::chords::pitch_class(k, "key")).transpose()?;
    let flats = params.get("key").and_then(Value::as_str).is_some_and(|k| k.ends_with('b'));
    let found: Vec<Value> = SCALES
        .iter()
        .filter(|s| tags.iter().all(|t| s.has(*t)))
        .filter(|s| words.iter().all(|w| s.name.to_lowercase().contains(w.as_str())))
        .map(|s| {
            let mut v = json!({"name": s.name, "intervals": s.intervals, "tags": s.tags.iter().map(|t| t.label()).collect::<Vec<_>>(), "steps": s.steps(), "intervalNames": s.interval_names()});
            if let (Some(k), Some(o)) = (key, v.as_object_mut()) {
                o.insert("notes".into(), json!(s.notes(k, flats)));
            }
            v
        })
        .collect();
    Ok(json!({"scales": found}))
}

fn find(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let key = params.get("key").filter(|v| !v.is_null()).map(|k| crate::chords::pitch_class(k, "key")).transpose()?;
    let limit = p::opt_uint(params, "limit", 50)?.unwrap_or(20).max(1) as usize;
    let sel = s.selection.ok_or_else(|| EngineError::invalid("select the bars to analyse first"))?;
    let ((m0, _), (m1, _)) = sel.ordered();
    let mut weights = [0.0f32; 12];
    for t in s.score.tracks.iter().filter(|t| !t.is_drums()) {
        for m in t.measures.iter().take(m1 + 1).skip(m0) {
            for b in m.voices.iter().flat_map(|v| &v.beats).filter(|b| !b.rest) {
                let len = b.duration.ticks() as f32;
                for n in b.notes.iter().filter(|n| n.kind != NoteKind::Dead) {
                    if let Some(pitch) = t.tuning.pitch(usize::from(n.string), n.fret, t.capo)
                        && let Some(w) = weights.get_mut(usize::from(pitch.0 % 12))
                    {
                        *w += len;
                    }
                }
            }
        }
    }
    if weights.iter().all(|w| *w <= 0.0) {
        return Err(EngineError::invalid("the selected bars have no notes"));
    }
    let flats = s.score.headers.get(m0).is_some_and(|h| h.key < 0);
    let found: Vec<Value> = find_scales(&weights, key, limit)
        .into_iter()
        .filter_map(|f| {
            let scale = SCALES.get(f.scale)?;
            Some(json!({"key": guitario_theory::pitch_class_name(f.root, flats), "root": f.root, "scale": scale.name, "percent": (f.percent * 10.0).round() / 10.0}))
        })
        .collect();
    Ok(json!({"bars": [m0, m1], "scales": found}))
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::json;

    #[test]
    fn the_library_filters_by_tags_and_words() {
        let mut s = Session::new();
        let r = s.run("tools.scales", &json!({"tags": ["Usual", "Scale"]})).unwrap();
        let names: Vec<&str> = r["scales"].as_array().unwrap().iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert_eq!(&names[..3], ["Major", "Natural Minor", "Ionian (1st mode of major)"]);
        assert!(!names.iter().any(|n| n.contains("Triad")));
        let r = s.run("tools.scales", &json!({"search": "dorian", "key": "F"})).unwrap();
        assert_eq!(r["scales"][0]["notes"], json!(["F", "G", "Ab", "Bb", "C", "D", "Eb"]));
        let r = s.run("tools.scales", &json!({"search": "pentatonic minor", "tags": ["pentatonic"]})).unwrap();
        assert_eq!(r["scales"][0]["name"], "Minor Pentatonic");
        assert!(s.run("tools.scales", &json!({"tags": ["Polka"]})).is_err());
        assert!(s.run("tools.scales", &json!({"tags": "Usual"})).is_err());
    }

    #[test]
    fn the_selected_bars_find_their_scale() {
        let mut s = Session::new();
        assert!(s.run("tools.findScales", &json!({})).is_err(), "nothing selected");
        // An A minor pentatonic lick on the top strings: A C D E G.
        for (string, fret) in [(0, 5), (0, 8), (1, 5), (1, 8), (2, 5), (2, 7)] {
            s.run("cursor.set", &json!({"string": string})).unwrap();
            s.run("note.setFret", &json!({"fret": fret})).unwrap();
            s.run("cursor.right", &json!({})).unwrap();
        }
        s.run("select.all", &json!({})).unwrap();
        let r = s.run("tools.findScales", &json!({"key": "A", "limit": 3})).unwrap();
        assert_eq!(r["scales"][0]["scale"], "Minor Pentatonic");
        assert_eq!(r["scales"][0]["percent"], 100.0);
        assert!(r["scales"].as_array().unwrap().iter().all(|v| v["key"] == "A"));
        let all = s.run("tools.findScales", &json!({})).unwrap();
        assert!(all["scales"].as_array().unwrap().len() >= 10);
        for bad in [json!({"key": "Z"}), json!({"limit": 99}), json!({"key": 14})] {
            assert!(s.run("tools.findScales", &bad).is_err(), "{bad}");
        }
    }
}
