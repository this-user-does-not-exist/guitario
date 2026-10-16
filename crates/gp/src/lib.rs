//! L3 Guitar Pro 7/8 (`.gp`) import.
//!
//! A `.gp` file is a ZIP archive whose `Content/score.gpif` holds the music as ID-referenced XML.
//! Written from the public format notes and observation of files; nothing is copied from other
//! implementations. Only the music is read (not engraving style or RSE settings). Anything the
//! model cannot hold is dropped and reported in [`Import::warnings`] instead of failing.

mod gpif;

use guitario_score::{Score, ScoreError};
use std::io::Read;

pub use gpif::parse_gpif;

/// Largest `score.gpif` accepted after decompression.
const MAX_GPIF: u64 = 128 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum GpError {
    #[error("not a Guitar Pro 7/8 file (no ZIP container)")]
    NotZip,
    #[error("the archive has no Content/score.gpif")]
    NoScore,
    #[error("score.gpif is too large")]
    TooLarge,
    #[error("score.gpif is not valid XML: {0}")]
    Xml(String),
    #[error("unsupported or damaged score: {0}")]
    Damaged(String),
    #[error("imported score is invalid: {0}")]
    Invalid(#[from] ScoreError),
}

#[derive(Debug)]
pub struct Import {
    pub score: Score,
    pub warnings: Vec<String>,
}

/// Reads a `.gp` file from memory.
pub fn import(bytes: &[u8]) -> Result<Import, GpError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| GpError::NotZip)?;
    let file = zip.by_name("Content/score.gpif").map_err(|_| GpError::NoScore)?;
    if file.size() > MAX_GPIF {
        return Err(GpError::TooLarge);
    }
    let mut xml = String::new();
    // `take` bounds the read even if the header lied about the size.
    file.take(MAX_GPIF + 1).read_to_string(&mut xml).map_err(|e| GpError::Xml(e.to_string()))?;
    if xml.len() as u64 > MAX_GPIF {
        return Err(GpError::TooLarge);
    }
    parse_gpif(&xml)
}

pub fn import_file(path: &std::path::Path) -> Result<Import, GpError> {
    let bytes = std::fs::read(path).map_err(|_| GpError::NotZip)?;
    import(&bytes)
}
