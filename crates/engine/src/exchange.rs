//! Exchange beyond opening a file: export to `.gp5`, `.gp` and MusicXML; MusicXML import.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd("file.exportGp5", "Export Guitar Pro 5…", "File/Export", None, "path", false, export_gp5),
        cmd("file.exportGp", "Export Guitar Pro 8…", "File/Export", None, "path", false, export_gp),
        cmd("file.exportMusicXml", "MusicXML…", "File/Export", None, "path (.musicxml or .xml)", false, export_musicxml),
        cmd("file.importMusicXml", "MusicXML…", "File/Import", None, "path (.musicxml, .xml or compressed .mxl; partwise)", false, import_musicxml),
    ]
}

/// Writes a Guitar Pro 7/8 file; what it can't hold comes back as warnings.
fn export_gp(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let path = p::string(params, "path")?;
    let out = guitario_gp::export_gp(&s.score)?;
    std::fs::write(path, &out.bytes)?;
    Ok(json!({"bytes": out.bytes.len(), "warnings": out.warnings}))
}

/// Writes MusicXML 4.0: each track a part with its tuning, strings and frets.
fn export_musicxml(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let path = p::string(params, "path")?;
    let out = guitario_musicxml::export(&s.score);
    std::fs::write(path, out.xml.as_bytes())?;
    Ok(json!({"bytes": out.xml.len(), "warnings": out.warnings}))
}

/// Imports MusicXML (plain or `.mxl`). Unsaved, like a Guitar Pro import.
pub(crate) fn import_musicxml(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let import = guitario_musicxml::import_file(std::path::Path::new(p::string(params, "path")?))?;
    s.replace_score(import.score, None);
    s.dirty = true;
    Ok(json!({"warnings": import.warnings}))
}

/// Writes a Guitar Pro 5 file; what the format cannot hold comes back as warnings.
fn export_gp5(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let path = p::string(params, "path")?;
    let out = guitario_gp::export_gp5(&s.score);
    std::fs::write(path, &out.bytes)?;
    Ok(json!({"bytes": out.bytes.len(), "warnings": out.warnings}))
}
