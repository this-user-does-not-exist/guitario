//! What a new document can start from besides a blank score, as Guitar Pro's Homepage offers it:
//! templates (an empty score for an instrument or a band) and the example scores, Guitario's
//! metal arrangements of public-domain pieces, built into the program.

use crate::commands::{CommandSpec, cmd};
use crate::params;
use crate::{EngineError, Session};
use guitario_score::{Score, Track};
use guitario_theory::Tuning;
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd("file.newFromTemplate", "New from Template", "File", None, "template (see file.templates)", false, new_from_template),
        cmd("file.openExample", "Open Example", "File", None, "example (see file.examples)", false, open_example),
        cmd("file.templates", "Templates", "", None, "", false, |_, _| {
            Ok(json!(TEMPLATES.iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>()))
        }),
        cmd("file.examples", "Examples", "", None, "", false, |_, _| {
            Ok(json!(EXAMPLES.iter().map(|(id, title, _)| json!({"id": id, "title": title})).collect::<Vec<_>>()))
        }),
    ]
}

/// Templates by id and name.
pub const TEMPLATES: [(&str, &str); 6] = [
    ("electricGuitar", "Electric Guitar"),
    ("acousticGuitar", "Acoustic Guitar"),
    ("sevenString", "7-String Guitar"),
    ("bass", "Bass"),
    ("drums", "Drums"),
    ("band", "Guitar, Bass and Drums"),
];

macro_rules! example {
    ($id:literal, $title:literal, $file:literal) => {
        ($id, $title, include_bytes!(concat!("../../../demos/", $file)) as &[u8])
    };
}

/// The example scores: id, title, and the `.gt` file built in.
pub const EXAMPLES: [(&str, &str, &[u8]); 15] = [
    example!("flight-of-the-bumblebee", "Flight of the Bumblebee", "01-flight-of-the-bumblebee.gt"),
    example!("funeral-march", "Funeral March", "02-funeral-march.gt"),
    example!("hall-of-the-mountain-king", "In the Hall of the Mountain King", "03-hall-of-the-mountain-king.gt"),
    example!("summer-presto", "Summer: Presto", "04-summer-presto.gt"),
    example!("toccata-and-fugue", "Toccata and Fugue in D minor", "05-toccata-and-fugue.gt"),
    example!("mars", "Mars, the Bringer of War", "06-mars.gt"),
    example!("little-fugue", "Little Fugue in G minor", "07-little-fugue.gt"),
    example!("moonlight-sonata", "Moonlight Sonata: Presto agitato", "08-moonlight-sonata.gt"),
    example!("fifth-symphony", "Symphony No. 5", "09-fifth-symphony.gt"),
    example!("lagrima", "Lágrima", "10-lagrima.gt"),
    example!("caprice-24", "Caprice No. 24", "11-caprice-24.gt"),
    example!("metal-technique-showcase", "Metal Technique Showcase", "12-metal-technique-showcase.gt"),
    example!("cello-prelude", "Cello Suite No. 1: Prelude", "13-cello-prelude.gt"),
    example!("drum-grooves", "Drum Grooves", "14-drum-grooves.gt"),
    example!("bourree-in-e-minor", "Bourrée in E minor", "15-bourree-in-e-minor.gt"),
];

/// The empty score a template starts.
pub fn template(id: &str) -> Option<Score> {
    let mut s = Score::new();
    let guitar = |name: &str, tuning: Tuning, program: u8, headers: &[guitario_score::MeasureHeader]| Track { program, ..Track::new(name, tuning, headers) };
    let tracks = match id {
        "electricGuitar" => vec![guitar("Electric Guitar", Tuning::standard(), 29, &s.headers)],
        "acousticGuitar" => vec![guitar("Acoustic Guitar", Tuning::standard(), 25, &s.headers)],
        "sevenString" => vec![guitar("7-String Guitar", Tuning::seven_string(), 30, &s.headers)],
        "bass" => vec![guitar("Bass", Tuning::bass(), 33, &s.headers)],
        "drums" => vec![Track::drums("Drums", &s.headers)],
        "band" => vec![
            guitar("Electric Guitar", Tuning::standard(), 29, &s.headers),
            guitar("Bass", Tuning::bass(), 33, &s.headers),
            Track::drums("Drums", &s.headers),
        ],
        _ => return None,
    };
    s.tracks = tracks;
    Some(s)
}

/// An example score, read from the copy built in.
pub fn example(id: &str) -> Result<Score, EngineError> {
    let (_, _, bytes) = EXAMPLES.iter().find(|(e, _, _)| *e == id).ok_or_else(|| EngineError::BadParam(format!("no example `{id}`")))?;
    let score = guitario_format::from_bytes(bytes)?;
    score.validate().map_err(|e| EngineError::invalid(e.to_string()))?;
    Ok(score)
}

fn new_from_template(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    let id = params::string(p, "template")?;
    let score = template(id).ok_or_else(|| EngineError::BadParam(format!("no template `{id}`")))?;
    s.replace_score(score, None);
    Ok(json!({}))
}

fn open_example(s: &mut Session, p: &Value) -> Result<Value, EngineError> {
    let score = example(params::string(p, "example")?)?;
    s.replace_score(score, None);
    Ok(json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_is_a_valid_score() {
        for (id, _) in TEMPLATES {
            let score = template(id).unwrap_or_else(|| panic!("{id}"));
            assert!(score.validate().is_ok(), "{id}");
        }
        assert_eq!(template("band").map(|s| s.tracks.len()), Some(3));
        assert!(template("kazoo").is_none());
    }

    #[test]
    fn every_example_opens_and_every_demo_is_an_example() {
        for (id, _, _) in EXAMPLES {
            assert!(example(id).is_ok(), "{id}");
        }
        // The demos folder holds the blank score and one file per example.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos");
        let demos = std::fs::read_dir(dir).map(|d| d.filter_map(Result::ok).filter(|e| e.path().extension().is_some_and(|x| x == "gt")).count()).unwrap_or(0);
        assert_eq!(demos, EXAMPLES.len() + 1, "a demo was added or removed: update EXAMPLES");
    }

    #[test]
    fn the_commands_start_an_untitled_unchanged_document() {
        let mut s = Session::new();
        s.run("file.openExample", &json!({"example": "little-fugue"})).unwrap();
        assert!(!s.is_dirty() && s.path().is_none());
        assert!(s.score().tracks.len() > 1);
        s.run("file.newFromTemplate", &json!({"template": "drums"})).unwrap();
        assert!(s.score().tracks[0].is_drums());
        assert!(s.run("file.openExample", &json!({"example": "nope"})).is_err());
        assert!(s.run("file.newFromTemplate", &json!({"template": 1})).is_err());
    }
}
