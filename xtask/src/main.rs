//! Workspace tooling: `cargo xtask <command>`.
//!
//! Pure Rust (std + serde). External tools (`cargo`) are invoked through `std::process::Command`.

mod ico;
mod layers;
mod package;
mod scorecard;
mod sounds;
mod stats;
mod version;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  layers          enforce the crate dependency layering (docs/architecture.md, xtask/src/layers.rs)
  wasm            cargo check --target wasm32-unknown-unknown for the wasm-safe crates (L0-L5)
  ci              fmt --check, clippy -D warnings, test, layers, parity --check, scorecard --check
  stats           count tests and lines per crate
  perf [--report] time a 400-bar, six-track score through every stage in release and check
                  perf/budgets.toml (--report: print without failing)
  parity [--check]
                  Guitar Pro menu parity; rewrites docs/parity-checklist.md (--check: fail if stale)
  scorecard [--check]
                  regenerate docs/scorecard.md from scorecard/*.toml (--check: fail if stale)
  sounds [--list] [--dir <folder>] [--only <id>,..]
                  download open SFZ guitars, bass and drums and write the sound bank (docs/sounds.md)
  package [--target <triple>] [--no-build]
                  build in release and lay out the app for this platform in dist/: an app bundle,
                  .tar.gz and .deb, or .zip and installer (packaging/, release workflow)
  ico <out.ico> <in.png>...
                  pack PNG images into a Windows icon (packaging/icons.sh uses it)
  version [set X.Y.Z[-pre]]
                  print the workspace version, or set it (Cargo.toml + Cargo.lock)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
    let result = match args.first().map(String::as_str) {
        Some("layers") => cmd_layers(),
        Some("wasm") => cmd_wasm(),
        Some("ci") => cmd_ci(),
        Some("stats") => stats::run(&root()),
        Some("perf") => cmd_perf(rest.contains(&"--report")),
        Some("parity") => cmd_parity(rest.contains(&"--check")),
        Some("scorecard") => scorecard::run(&root(), rest.contains(&"--check")),
        Some("sounds") => sounds::run(&rest),
        Some("version") => version::run(&root(), &rest),
        Some("ico") => ico::run(&rest),
        Some("package") => package::run(&root(), &rest),
        Some("-h" | "--help" | "help") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

pub fn cargo() -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root());
    c
}

fn run(mut c: Command, what: &str) -> Result<(), String> {
    eprintln!("==> {what}");
    let status = c.status().map_err(|e| format!("{what}: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("{what} failed")) }
}

pub fn metadata() -> Result<serde_json::Value, String> {
    let out = cargo().args(["metadata", "--format-version", "1", "--no-deps"]).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())
}

fn cmd_layers() -> Result<(), String> {
    let model = layers::from_metadata(&metadata()?)?;
    let errors = layers::check(&model);
    if errors.is_empty() {
        println!("layers: {} crates, all dependencies respect the layering", model.len());
        Ok(())
    } else {
        for e in &errors {
            eprintln!("  {e}");
        }
        Err(format!("{} layering violation(s)", errors.len()))
    }
}

/// Crates that must build for the browser: everything below the shell.
const WASM_CRATES: &[&str] = &["theory", "score", "midi", "playback", "format", "gp", "engine"];

fn cmd_wasm() -> Result<(), String> {
    let mut c = cargo();
    c.args(["check", "--target", "wasm32-unknown-unknown"]);
    for k in WASM_CRATES {
        c.args(["-p", &format!("guitario-{k}")]);
    }
    run(c, "wasm32 check")
}

fn cmd_parity(check: bool) -> Result<(), String> {
    let mut c = cargo();
    c.args(["run", "-q", "-p", "guitario-ui-egui", "--example", "parity", "--"]);
    if check {
        c.arg("--check");
    }
    run(c, if check { "parity --check" } else { "parity" })
}

/// Performance budgets (crates/ui-egui/src/perf.rs, perf/budgets.toml), in a release build.
fn cmd_perf(report: bool) -> Result<(), String> {
    let mut test = cargo();
    test.args(["test", "--release", "-p", "guitario-ui-egui", "--lib", "perf::tests::budgets_hold", "--", "--ignored", "--exact", "--nocapture"]);
    if report {
        test.env("GUITARIO_PERF_REPORT", "1");
    }
    run(test, "perf")
}

fn cmd_ci() -> Result<(), String> {
    let mut fmt = cargo();
    fmt.args(["fmt", "--all", "--", "--check"]);
    run(fmt, "fmt --check")?;
    let mut clippy = cargo();
    clippy.args(["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]);
    run(clippy, "clippy")?;
    let mut test = cargo();
    test.args(["test", "--workspace"]);
    run(test, "test")?;
    cmd_layers()?;
    cmd_parity(true)?;
    scorecard::run(&root(), true)
}
