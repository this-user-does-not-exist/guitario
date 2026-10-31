# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

`AGENTS.md` (imported above) is the authoritative guide: golden rules, layering, never-crash policy,
how to pick work and what to run before finishing. This file adds the commands and architecture
notes that are spread across `docs/`.

## Commands

```sh
cargo run -p guitario                              # desktop editor
cargo run -p guitario -- demos/01-flight-of-the-bumblebee.gt
cargo run -p guitario-cli -- commands              # every engine command id with its params
cargo run -p guitario-cli -- run <file|-> script.json [--save out.gt]   # script = [["cmd.id", {params}], ...]
cargo run -p guitario-cli -- convert in.gp out.pdf # .gt .gp .gp5 .pdf .mid .wav .flac .txt

cargo xtask ci          # fmt --check, clippy -D warnings, tests, layers, parity --check, scorecard --check
cargo xtask layers      # dependency-layer check only
cargo xtask wasm        # L0–L5 build for wasm32-unknown-unknown
cargo xtask perf        # release timings on a 400-bar score vs perf/budgets.toml
cargo xtask package     # release packages for this platform in dist/ (app bundle, .deb, installer …)
cargo xtask parity      # regenerate docs/parity-checklist.md (then raise FLOOR in crates/ui-egui/src/parity.rs)
cargo xtask scorecard   # regenerate docs/scorecard.md from scorecard/*.toml
```

Tests (packages are named `guitario-<crate>`):

```sh
cargo test -p guitario-engine --lib --tests                 # one crate, skipping examples and doctests
cargo test -p guitario-engine --test engine panic_hunt      # one test in one integration-test file
cargo nextest run --workspace                               # parallel; doctests need `cargo test --doc`
```

Offscreen UI render (no window; look at the PNG afterwards):

```sh
cargo run -p guitario-ui-egui --example snapshot -- out.png 1440 900 --file demos/05-toccata-and-fugue.gt --zoom 1.0
# flags: --tab-only --playing --selection --track n
```

Regenerate demos after the engine gains something expressible: `cargo run -p guitario-engine --example demos`
(checked by `crates/engine/tests/demos.rs`). Set `GUITARIO_CONFIG_DIR=` (empty) to keep preferences
in memory and disable autosave in tests and agent runs; other env vars are in `docs/development.md`.

## Architecture in brief

- **Command registry.** Every action is a `CommandSpec` (`crates/engine/src/commands.rs`): id,
  label, menu path, shortcut, params doc, `mutates`, `run`. Area modules (`clipboard.rs`,
  `techniques.rs`, `tracks.rs`, `whammy.rs`, …) each expose `specs()`, chained into the registry in
  `commands.rs`. UI, CLI and tests all dispatch by id through `Session::run`. `panic_hunt` fuzzes
  every registered command automatically.
- **Undo/atomicity.** The score lives behind an `Arc`; a mutating command clones on write and the
  session keeps the previous `Arc` as one undo step. A failing command restores it exactly.
- **Menus.** `crates/ui-egui/src/menu_catalog.rs` is Guitar Pro 8's full menu tree; pointing an
  item at a command id with `c(…)` makes it live and raises measured parity.
- **Rendering.** `ui-egui/src/layout.rs` and `notation.rs` are pure (testable without a GPU):
  measure widths, system breaks, pagination, pitch spelling, beaming. `page.rs` paints and records
  hit rectangles. Print/PDF reuses the same painting: the engine defines `PageRenderer` and
  `Spooler` traits (`engine/src/render.rs`, `print.rs`), implemented in the shell by `pdf.rs`
  (headless egui → Krilla via craft-pdf) and `spool.rs`.
- **Playback.** `score::timeline` unrolls repeats and merges ties → `playback::Player` with a
  `TempoMap` and Karplus–Strong voices (plus SFZ banks) → `audio::AudioOut` on the cpal thread via
  `try_lock`, so the UI never blocks audio.
- **Formats.** `.gt` is the serde form of `guitario-score` (versioned JSON, `Score::validate`,
  craft-atomic saves; new fields need `#[serde(default)]`). `crates/gp` imports GP3–8 and exports
  `.gp5`, flipping string order (string 0 is the top line here, the bottom in Guitar Pro).
