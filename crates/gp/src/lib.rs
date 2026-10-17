//! L3 Guitar Pro exchange: import of `.gp` (Guitar Pro 7/8), `.gpx` (6) and the binary `.gp5`,
//! `.gp4` and `.gp3`; export to `.gp` and `.gp5`.
//!
//! A `.gp` file is a ZIP archive whose `Content/score.gpif` holds the music as ID-referenced XML;
//! `.gpx` keeps the same XML in its own compressed container ([`gpx`]); Guitar Pro 3-5 files are a
//! flat little-endian binary stream ([`legacy`]). The format is told by
//! the file's first bytes, not its name. Written from the public format notes and observation of
//! files; nothing is copied from other implementations. Only the music is read (not engraving
//! style or RSE settings). Anything the model cannot hold is dropped and reported in
//! [`Import::warnings`] instead of failing.

mod gpif;
mod gpx;
mod legacy;
mod page;
mod parts;
mod write5;
mod writegp;

use guitario_score::{Score, ScoreError};
use std::io::Read;

pub use gpif::parse_gpif;
pub use write5::{Export, export_gp5};
pub use writegp::export_gp;

/// Largest `PartConfiguration` read.
const MAX_PARTS: u64 = 64 * 1024;

/// Largest `score.gpif` accepted after decompression.
const MAX_GPIF: u64 = 128 * 1024 * 1024;
/// Largest file read from disk.
#[cfg(not(target_arch = "wasm32"))]
const MAX_FILE: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_MEASURES: usize = 20_000;
pub(crate) const MAX_TRACKS: usize = 15;

/// File extensions this crate imports.
pub const EXTENSIONS: [&str; 5] = ["gp", "gpx", "gp5", "gp4", "gp3"];

/// True when `path` has one of the [`EXTENSIONS`] (any case).
pub fn is_guitar_pro_path(path: &str) -> bool {
    std::path::Path::new(path).extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
}

#[derive(Debug, thiserror::Error)]
pub enum GpError {
    #[error("not a Guitar Pro file")]
    NotGuitarPro,
    #[error("unsupported Guitar Pro file: {0}")]
    Unsupported(String),
    #[error("could not read the file: {0}")]
    Io(String),
    #[error("the archive has no Content/score.gpif")]
    NoScore,
    #[error("the file is too large")]
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

/// Reads a Guitar Pro file of any supported version from memory.
pub fn import(bytes: &[u8]) -> Result<Import, GpError> {
    if bytes.starts_with(b"PK") {
        import_gp7(bytes)
    } else if gpx::looks_gpx(bytes) {
        gpx::parse(bytes)
    } else if legacy::looks_legacy(bytes) {
        legacy::parse(bytes)
    } else {
        Err(GpError::NotGuitarPro)
    }
}

fn import_gp7(bytes: &[u8]) -> Result<Import, GpError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| GpError::NotGuitarPro)?;
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
    let mut import = parse_gpif(&xml)?;
    // Which notations each track shows, when the file says and every track was read.
    if let Ok(file) = zip.by_name("Content/PartConfiguration") {
        let mut bytes = Vec::new();
        if file.take(MAX_PARTS).read_to_end(&mut bytes).is_ok()
            && let Some(flags) = parts::read(&bytes).filter(|f| f.len() == import.score.tracks.len())
        {
            for (track, bits) in import.score.tracks.iter_mut().zip(flags) {
                let n = guitario_score::Notations::from_bits(bits);
                if n.bits() != 0 {
                    track.notation = n;
                }
            }
        }
    }
    Ok(import)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn import_file(path: &std::path::Path) -> Result<Import, GpError> {
    let file = std::fs::File::open(path).map_err(|e| GpError::Io(e.to_string()))?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut bytes).map_err(|e| GpError::Io(e.to_string()))?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(GpError::TooLarge);
    }
    import(&bytes)
}

#[cfg(target_arch = "wasm32")]
pub fn import_file(_path: &std::path::Path) -> Result<Import, GpError> {
    Err(GpError::Io("files on disk are not available in the browser build; use import".into()))
}
