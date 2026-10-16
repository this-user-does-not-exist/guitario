//! L5 engine: the [`Session`] (score + cursor + history) and the command registry.
//!
//! Every user-visible action is a command with a stable id, dispatched by [`Session::run`]. The UI,
//! the CLI and any control channel call the same entry point, so nothing is reachable only from a
//! GUI.

mod ascii;
mod commands;
mod drums;
mod error;
mod params;
mod selection;
mod session;

pub use commands::{CommandSpec, registry};
pub use error::EngineError;
pub use selection::{Clip, Selection};
pub use session::{Cursor, Session};
