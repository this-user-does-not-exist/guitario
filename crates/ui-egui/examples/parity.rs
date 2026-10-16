//! Writes `docs/parity-checklist.md` (`cargo xtask parity`); `--check` fails if it is stale.

use std::process::ExitCode;

fn main() -> ExitCode {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/parity-checklist.md");
    let text = guitario_ui_egui::parity::report();
    let (live, total) = guitario_ui_egui::parity::live_count();
    if std::env::args().any(|a| a == "--check") {
        if std::fs::read_to_string(&out).unwrap_or_default() == text {
            println!("docs/parity-checklist.md is up to date ({live}/{total})");
            return ExitCode::SUCCESS;
        }
        eprintln!("docs/parity-checklist.md is stale: run `cargo xtask parity` and commit it");
        return ExitCode::FAILURE;
    }
    match std::fs::write(&out, text) {
        Ok(()) => {
            println!("menu parity {live}/{total}; wrote docs/parity-checklist.md");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}: {e}", out.display());
            ExitCode::FAILURE
        }
    }
}
