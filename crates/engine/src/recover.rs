//! Crash recovery: reopening a score the shell autosaved before Guitario closed without saving it.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use serde_json::{Value, json};
use std::path::PathBuf;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd(
        "file.recover",
        "Recover Unsaved Score",
        "",
        None,
        "path (the autosaved .gt), original? (the file it was edited from; Save writes there)",
        false,
        recover,
    )]
}

/// Opens an autosaved score as unsaved work: the title shows it changed, Save goes to the file it
/// came from (or asks, when it had none), and the history starts here.
fn recover(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let path = PathBuf::from(p::string(params, "path")?);
    let original = p::opt_string(params, "original").filter(|o| !o.trim().is_empty()).map(PathBuf::from);
    // Only a native file comes back to its place; anything else is saved under a new name.
    let original =
        original.filter(|o| o.extension().and_then(|e| e.to_str()).is_some_and(|e| guitario_format::EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e))));
    let score = guitario_format::open(&path)?;
    s.replace_score(score, original.clone());
    s.dirty = true;
    Ok(json!({"original": original.map(|o| o.display().to_string())}))
}

#[cfg(test)]
mod tests {
    use crate::Session;
    use serde_json::json;

    #[test]
    fn a_recovered_score_is_unsaved_and_saves_back_where_it_came_from() {
        let dir = std::env::temp_dir().join(format!("gt-recover-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let auto = dir.join("autosave.gt");
        let mut s = Session::new();
        s.run("note.setFret", &json!({"fret": 9})).unwrap();
        guitario_format::save(s.score(), &auto).unwrap();

        let mut r = Session::new();
        let original = dir.join("riff.gt");
        r.run("file.recover", &json!({"path": auto.display().to_string(), "original": original.display().to_string()})).unwrap();
        assert_eq!(**r.score(), **s.score());
        assert!(r.is_dirty() && !r.can_undo());
        assert_eq!(r.path(), Some(original.as_path()));
        r.run("file.save", &serde_json::Value::Null).unwrap();
        assert!(!r.is_dirty() && original.exists());

        // An imported Guitar Pro file is not written over; no original asks for a name.
        r.run("file.recover", &json!({"path": auto.display().to_string(), "original": "/x/song.gp5"})).unwrap();
        assert_eq!(r.path(), None);
        r.run("file.recover", &json!({"path": auto.display().to_string()})).unwrap();
        assert!(r.path().is_none() && r.is_dirty());
        // A missing or broken file is an error and leaves the session alone.
        let before = (**r.score()).clone();
        assert!(r.run("file.recover", &json!({"path": dir.join("nope.gt").display().to_string()})).is_err());
        std::fs::write(dir.join("bad.gt"), b"{not json").unwrap();
        assert!(r.run("file.recover", &json!({"path": dir.join("bad.gt").display().to_string()})).is_err());
        assert!(r.run("file.recover", &json!({"path": 5})).is_err());
        assert_eq!(**r.score(), before);
        std::fs::remove_dir_all(dir).ok();
    }
}
