//! L3 MusicXML exchange: import of `.musicxml`/`.xml` (score-partwise) and compressed `.mxl`, and
//! export to `.musicxml`.
//!
//! Written from the public MusicXML 4.0 specification (W3C Music Notation Community Group).
//! Guitar tracks carry their tuning in `<staff-details>` and each note's string and fret in
//! `<technical>`, so a tab survives the trip; a file without them is fingered as a MIDI import is.
//! Drum tracks are unpitched parts on channel 10. Anything the model cannot hold is dropped and
//! reported in the warnings instead of failing.

mod read;
mod write;

use guitario_score::{Score, ScoreError};

pub use read::import;
pub use write::export;

/// File extensions this crate imports.
pub const EXTENSIONS: [&str; 3] = ["musicxml", "mxl", "xml"];

/// Largest file read, and largest XML document after decompression.
pub const MAX_FILE: u64 = 128 * 1024 * 1024;

/// True when `path` has one of the [`EXTENSIONS`] (any case).
pub fn is_musicxml_path(path: &str) -> bool {
    std::path::Path::new(path).extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

#[derive(Debug, thiserror::Error)]
pub enum MusicXmlError {
    #[error("not a MusicXML file")]
    NotMusicXml,
    #[error("unsupported MusicXML file: {0}")]
    Unsupported(String),
    #[error("could not read the file: {0}")]
    Io(String),
    #[error("the file is too large")]
    TooLarge,
    #[error("not valid XML: {0}")]
    Xml(String),
    #[error("damaged MusicXML file: {0}")]
    Damaged(String),
    #[error("imported score is invalid: {0}")]
    Invalid(#[from] ScoreError),
}

/// A score read from MusicXML, with what was left out.
#[derive(Debug)]
pub struct Import {
    pub score: Score,
    pub warnings: Vec<String>,
}

/// A score written as MusicXML, with what could not be written.
#[derive(Debug)]
pub struct Export {
    pub xml: String,
    pub warnings: Vec<String>,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn import_file(path: &std::path::Path) -> Result<Import, MusicXmlError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| MusicXmlError::Io(e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes).map_err(|e| MusicXmlError::Io(e.to_string()))?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(MusicXmlError::TooLarge);
    }
    import(&bytes)
}

#[cfg(target_arch = "wasm32")]
pub fn import_file(_path: &std::path::Path) -> Result<Import, MusicXmlError> {
    Err(MusicXmlError::Io("files on disk are not available in the browser build; use import".into()))
}
