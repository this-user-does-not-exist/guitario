//! Guitar Pro exchange beyond opening a file: export to `.gp5`.

use crate::commands::{CommandSpec, cmd};
use crate::{EngineError, Session, params as p};
use serde_json::{Value, json};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd("file.exportGp5", "Export Guitar Pro 5…", "File/Export", None, "path", false, export_gp5)]
}

/// Writes a Guitar Pro 5 file; what the format cannot hold comes back as warnings.
fn export_gp5(s: &mut Session, params: &Value) -> Result<Value, EngineError> {
    let path = p::string(params, "path")?;
    let out = guitario_gp::export_gp5(&s.score);
    std::fs::write(path, &out.bytes)?;
    Ok(json!({"bytes": out.bytes.len(), "warnings": out.warnings}))
}
