//! L3 the native `.gt` file: versioned JSON, validated on load, written atomically.

use guitario_score::{Score, ScoreError};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FORMAT: &str = "guitario";
/// Native file extensions.
pub const EXTENSIONS: [&str; 1] = ["gt"];
pub const VERSION: u32 = 1;
/// Refuse absurd inputs before parsing.
const MAX_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("not a Guitario file")]
    NotGuitario,
    #[error("file version {0} is newer than this app understands ({VERSION})")]
    TooNew(u32),
    #[error("file is too large")]
    TooLarge,
    #[error("invalid score: {0}")]
    Invalid(#[from] ScoreError),
    #[error("malformed file: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("files on disk are not available in the browser build; use from_bytes/to_bytes")]
    Unsupported,
}

#[derive(Serialize)]
struct FileOut<'a> {
    format: &'static str,
    version: u32,
    score: &'a Score,
}

#[derive(Deserialize)]
struct FileIn {
    format: String,
    version: u32,
    score: Score,
}

pub fn to_bytes(score: &Score) -> Result<Vec<u8>, FormatError> {
    Ok(serde_json::to_vec_pretty(&FileOut { format: FORMAT, version: VERSION, score })?)
}

pub fn from_bytes(bytes: &[u8]) -> Result<Score, FormatError> {
    if bytes.len() > MAX_BYTES {
        return Err(FormatError::TooLarge);
    }
    let file: FileIn = serde_json::from_slice(bytes)?;
    if file.format != FORMAT {
        return Err(FormatError::NotGuitario);
    }
    if file.version > VERSION {
        return Err(FormatError::TooNew(file.version));
    }
    file.score.validate()?;
    Ok(file.score)
}

/// Saves through `craft-atomic`: the bytes are staged beside the target, flushed and
/// renamed, so a crash, a full disk or a locked file never truncates the previous save.
#[cfg(not(target_arch = "wasm32"))]
pub fn save(score: &Score, path: &Path) -> Result<(), FormatError> {
    let bytes = to_bytes(score)?;
    Ok(craft_atomic::atomic_write(path, &bytes)?)
}

#[cfg(target_arch = "wasm32")]
pub fn save(_score: &Score, _path: &Path) -> Result<(), FormatError> {
    Err(FormatError::Unsupported)
}

#[cfg(target_arch = "wasm32")]
pub fn open(_path: &Path) -> Result<Score, FormatError> {
    Err(FormatError::Unsupported)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn open(path: &Path) -> Result<Score, FormatError> {
    if std::fs::metadata(path)?.len() > MAX_BYTES as u64 {
        return Err(FormatError::TooLarge);
    }
    from_bytes(&std::fs::read(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut s = Score::new();
        s.meta.title = "Test".into();
        assert_eq!(from_bytes(&to_bytes(&s).unwrap()).unwrap(), s);
    }

    #[test]
    fn other_format_tags_are_refused() {
        let mut v: serde_json::Value = serde_json::from_slice(&to_bytes(&Score::new()).unwrap()).unwrap();
        assert_eq!(v["format"], "guitario");
        v["format"] = "something-else".into();
        assert!(matches!(from_bytes(v.to_string().as_bytes()), Err(FormatError::NotGuitario)));
    }

    #[test]
    fn files_saved_before_page_setup_load_with_the_default_page() {
        let mut s = Score::new();
        s.page.paper = guitario_score::Paper::Letter;
        let mut v: serde_json::Value = serde_json::from_slice(&to_bytes(&s).unwrap()).unwrap();
        assert_eq!(v["score"]["page"]["paper"], "letter");
        if let Some(score) = v["score"].as_object_mut() {
            score.remove("page");
        }
        assert_eq!(from_bytes(v.to_string().as_bytes()).unwrap().page, guitario_score::PageSetup::default());
        // A page the layout could not use is refused like any other broken file.
        v["score"]["page"] = serde_json::json!({"margin_left": 200});
        assert!(matches!(from_bytes(v.to_string().as_bytes()), Err(FormatError::Invalid(_))));
        // A partial page setup keeps the defaults for what it leaves out.
        v["score"]["page"] = serde_json::json!({"landscape": true});
        let page = from_bytes(v.to_string().as_bytes()).unwrap().page;
        assert!(page.landscape && page.paper == guitario_score::Paper::A4 && page.margin_top == 15);
    }

    #[test]
    fn rejects_garbage() {
        assert!(from_bytes(b"").is_err());
        assert!(from_bytes(b"{}").is_err());
        assert!(matches!(from_bytes(br#"{"format":"x","version":1,"score":null}"#), Err(FormatError::Parse(_))));
    }

    #[test]
    fn rejects_future_and_invalid() {
        let s = Score::new();
        let mut v: serde_json::Value = serde_json::from_slice(&to_bytes(&s).unwrap()).unwrap();
        v["version"] = 99.into();
        assert!(matches!(from_bytes(v.to_string().as_bytes()), Err(FormatError::TooNew(99))));
        v["version"] = 1.into();
        v["score"]["tempo"] = 1.into();
        assert!(matches!(from_bytes(v.to_string().as_bytes()), Err(FormatError::Invalid(_))));
    }

    #[test]
    fn atomic_save_and_open() {
        let dir = std::env::temp_dir().join(format!("gc-fmt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.gt");
        save(&Score::new(), &p).unwrap();
        assert_eq!(open(&p).unwrap(), Score::new());
        // Nothing is left staged beside the file.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
