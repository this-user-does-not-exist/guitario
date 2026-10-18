//! L5 engine: the [`Session`] (score + cursor + history) and the command registry.
//!
//! Every user-visible action is a command with a stable id, dispatched by [`Session::run`]. The UI,
//! the CLI and any control channel call the same entry point, so nothing is reachable only from a
//! GUI.

mod ascii;
mod automation;
mod bars;
mod bends;
mod chords;
mod clipboard;
mod commands;
mod drums;
mod error;
mod exchange;
pub mod library;
mod lyrics;
mod pagesetup;
mod params;
mod print;
mod recover;
mod render;
mod sections;
mod selection;
mod session;
mod techniques;
mod tools;
mod tracks;
mod voices;
mod whammy;

pub use bars::direction_targets;
pub use clipboard::{Clip, TrackBars};
pub use commands::{CommandSpec, registry};
pub use error::EngineError;
pub use print::{DuplexMode, MAX_COPIES, PrintJob, PrintReceipt, PrintTask, PrinterInfo, Spooler};
pub use render::{PageRenderer, PdfDocument, PdfRequest};
pub use selection::Selection;

/// File extensions `file.open` imports as Guitar Pro.
pub const GUITAR_PRO_EXTENSIONS: [&str; 5] = guitario_gp::EXTENSIONS;
/// Native file extensions: `.gt`, and `.gt` from before the rename.
pub const NATIVE_EXTENSIONS: [&str; 2] = guitario_format::EXTENSIONS;
pub use session::{Cursor, Session};
