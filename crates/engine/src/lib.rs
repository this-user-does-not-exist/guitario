//! L5 engine: the [`Session`] (score + cursor + history) and the command registry.
//!
//! Every user-visible action is a command with a stable id, dispatched by [`Session::run`]. The UI,
//! the CLI and any control channel call the same entry point, so nothing is reachable only from a
//! GUI.

mod ascii;
mod bars;
mod chords;
mod commands;
mod drums;
mod error;
mod exchange;
mod params;
mod selection;
mod session;
mod techniques;
mod tools;
mod tracks;

pub use bars::direction_targets;
pub use commands::{CommandSpec, registry};
pub use error::EngineError;
pub use selection::{Clip, Selection};

/// File extensions `file.open` imports as Guitar Pro.
pub const GUITAR_PRO_EXTENSIONS: [&str; 5] = guitario_gp::EXTENSIONS;
pub use session::{Cursor, Session};
