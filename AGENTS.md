# AGENTS.md: guide for AI agents and contributors

Guitario is an open-source, native, Guitar Pro-comparable tablature and notation editor written in **Rust only** (no JavaScript or TypeScript). **No Tauri, Electron or webview shells:** the desktop app is native egui/eframe on wgpu, and the engine crates build for WebAssembly. Never add Tauri (or any webview/JS UI framework) as a dependency, build step or packaging target. The product name is always written **Guitario** in user-facing text: UI, window titles, About, installers, release names, docs prose. Machine names stay lowercase: crates (`guitario-*`), binaries, file names, ids (`ai.storyteller.guitario`), environment variables (`GUITARIO_*`). The native file extension is `.gt`. Shared Rust components (atomic saves, settings, quit handling, PDF and printing) live in [storytold/craft-libs](https://github.com/storytold/craft-libs) and are pinned by immutable git revision. The goal is 1:1 Guitar Pro parity (same layout, menus, shortcuts, behaviour and file fidelity) with better performance, and every feature drivable by agents. Read this file first, then `docs/`.

## 1. Orientation (5 minutes)

| Read | Why |
|---|---|
| `docs/architecture.md` | Crate map, dependency layers, the engine/UI seam, layout and playback |
| `docs/development.md` | Build, test, run, snapshots, CLI, environment variables |
| `docs/contributing.md` | Rules: clean-room, tests, layering, style, commits; the "add a command" checklist |
| `docs/ui-design.md` | Tokens and Guitar Pro's layout grammar |
| `ROADMAP.md` | One-page summary: stage, headline numbers, every dimension, what's next |
| `docs/target-app-parity.md` | The authoritative parity assessment against Guitar Pro 8: numbers, weights, calibration |
| `docs/gaps.md` | Every known shortfall, ranked, split into what the beta needs and what waits for 1.0: the work list to pick from |
| `docs/roadmap.md` | The stages, the alpha and **beta gates**, milestones, **current focus**, releases |
| `docs/changelog.md` | What has landed, by gap (moved out of `gaps.md`) |
| `docs/file-format-parity.md`, `docs/ui-parity.md` | Per-dimension parity checklists |
| `docs/parity-checklist.md` | Generated list of every Guitar Pro menu item, live or missing |
| `docs/scorecard.md` | Generated scorecard: per-area checklists and counts from the tree |
| `docs/decisions.md` | Implementation decisions and why |
| `demos/README.md` | The demo suite (public-domain classics arranged for metal) and how it is generated and tested |

## 2. Workspace map

```text
crates/
  theory                  L0 standalone: durations, time signatures, tunings, pitch
  score                   L1 the document model (pure data), validation, playback timeline
  midi playback           L2 MIDI import and export; tempo map, scheduler, plucked-string synth, WAV
  format gp musicxml      L3 native .gt (craft-atomic saves); Guitar Pro .gp/.gpx/.gp5/.gp4/.gp3 import, .gp/.gp5 export; MusicXML both ways
  audio                   L4 sound-card output (cpal)
  engine                  L5 Session + command registry (every action is a command)
  ui-egui                 L6 egui shell in Guitar Pro's layout; menus from the GP menu catalog
apps/
  guitario             desktop app (eframe/wgpu)
  guitario-cli         headless CLI (commands/info/convert/run)
xtask/                    cargo xtask layers | wasm | ci | stats | parity | scorecard | version
demos/                    public-domain classics arranged for metal (+ MIDI, ASCII tab) and a .gp fixture
scorecard/                checklists behind docs/scorecard.md
```

**Layering is enforced** by `cargo xtask layers`. A crate may depend only on lower layers; `theory` depends on nothing in the workspace. Nothing below `ui-egui` may use egui, eframe, winit, rfd or craft-quit; nothing below `audio` may use cpal. A new crate must be registered in `xtask/src/layers.rs`.

## 3. Golden rules

### Never crash (outranks feature work)

People trust Guitario with their work, and a crash loses it. A malformed file, a bad command param, a corrupt settings file, an odd keystroke or a full disk must produce an error the user or agent can act on, never a panic.

- **Non-test code never panics.** No `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!` or `unimplemented!`, and no `[i]` indexing: the workspace denies them in clippy (`clippy.toml` allows them in tests only). Return the crate's error type and propagate with `?`; use `get()`, `let .. else`, `if let`, or `unwrap_or*` where a fallback is truly correct.
- **No `unsafe`.** The workspace sets `unsafe_code = "forbid"`.
- **Input-derived numbers are hostile.** Check indices, use `checked_*`/`saturating_*`, guard division by zero and NaN, cap allocations sized by input (the `.gp` reader caps archive and XML sizes, track and bar counts).
- **Validate before accepting.** Files from disk go through `Score::validate()`.
- **Prove it.** Every crash fix comes with a regression test. `panic_hunt` (`crates/engine/tests/engine.rs`) sends hostile params to every command and must stay green.

1. **Everything is a command.** New user-visible behaviour = a command in the engine (`crates/engine/src/commands.rs`, or a new module with `specs()` such as `selection.rs`) with id, label, menu path, shortcut, params doc, `mutates` and `run`, plus tests. The UI, CLI and tests dispatch by id. Point the matching Guitar Pro item in `crates/ui-egui/src/menu_catalog.rs` at it and the menu item goes live. Only pure view state (zoom, panels, transport toggles) belongs to the shell.
2. **Look and behave like Guitar Pro.** Layout, engraving, palette, menus and shortcuts match Guitar Pro 8 unless there is a strong reason not to. Engraving defaults come from GP's stylesheet (`layout.rs` cites them). Colours come from `theme.rs` tokens.
3. **Clean-room.** Behaviour and look only, from public format notes and observation. Never copy code, sounds, fonts or stylesheets from Guitar Pro or other tab software. Assets must be permissively licensed, keep their licence beside them and get a row in `ATTRIBUTION.md`. **Never commit font files:** fonts live in craft-fonts; the app finds them at runtime (`fonts.rs`).
4. **Public-domain or original content.** Demo scores, examples and screenshots are either original or our own arrangements of public-domain works (composer long dead, arranged from the original score), centred on metal. Never transcribe or copy a copyrighted song, a modern edition, a recording's arrangement or someone else's tab.
5. **Strings are numbered from the top.** String 0 is the highest string (the top tab line); Guitar Pro numbers them from the bottom and `crates/gp` flips them.
6. **Tests are the gate.** Every change comes with tests. Formats get round-trip and malformed-input tests. Keep menu parity from regressing (`FLOOR` in `crates/ui-egui/src/parity.rs`).
7. **The UI is thin.** It reads the session and turns input into commands; it never edits the score directly.
8. **Verify UI changes visually.** Render offscreen with `cargo run -p guitario-ui-egui --example snapshot -- out.png` (no window, no focus stealing) and look at the PNG.
9. **Never break wasm.** L0–L5 must pass `cargo xtask wasm`. File-system code is `cfg(not(target_arch = "wasm32"))` with an "unsupported" fallback.
10. **Performance is a feature.** Don't scan the whole score per frame when a cached layout would do; record timings for anything heavy.

## 4. Picking work

1. **Read `ROADMAP.md`, then `docs/roadmap.md` → Current focus and `docs/gaps.md`.** The target is the **beta gate** (eight workflows in `docs/roadmap.md`); take the highest-ranked gap with beta work in it that nobody is working on. When your work lands, move its text from `docs/gaps.md` to `docs/changelog.md`, update the gap's hours, the parity doc and the scorecard, with the date, and bump their status line and revision history.
2. **Check `docs/scorecard.md`** for `missing` and `partial` rows in your area. Every row that is not done carries a stage (`beta` or `1.0`, in `scorecard/*.toml`); the summary counts what is still open for beta, and a new row needs a stage too.
3. **GitHub issues** hold the detail (milestones Alpha, Beta, 1.0 Guitar Pro parity; label `source: gp8-manual` for the manual review). Labels lag the code: check the scorecard before starting an open issue, and close it with the PR that finishes it.
4. `docs/parity-checklist.md` → missing menu items; low-hanging fruit is a menu item whose logic already exists.
5. `log/devlog.md` → the "Still open" bullets of recent entries.

## 5. Before you finish a task

```sh
cargo xtask ci              # fmt --check, clippy -D warnings, tests, layers, parity --check, scorecard --check
cargo xtask wasm            # if you touched L0–L5
cargo xtask perf            # if you touched layout, painting, playback, sounds or file reading; keep perf/budgets.toml green
cargo xtask parity          # if you added commands or menu items; commit docs/parity-checklist.md, raise FLOOR
cargo xtask scorecard       # if you moved a checklist item; commit docs/scorecard.md
cargo run -p guitario-engine --example demos   # if the engine can express something new: add it to a demo
```

Then append a terse entry to `log/devlog.md` (gitignored): what landed, numbers, what's still open. Keep the tree building at every step.

## 6. Parallel agents

- Use your own target dir (`CARGO_TARGET_DIR=target/agent-<name>`) and edit only the files you own. Shared files (`engine/src/commands.rs`, `menu_catalog.rs`, `lib.rs` of ui-egui) get small, surgical edits; re-read before editing.
- Put new commands in a **new engine module** with `specs()` rather than growing `commands.rs`.
- Keep every `Cargo.toml` valid at all times: write manifests to a temp file and `mv` them into place.

## 7. Where things are tracked

- `ROADMAP.md`, `docs/target-app-parity.md`, `docs/gaps.md`, `docs/roadmap.md` and the `docs/*-parity.md` checklists: where we stand against Guitar Pro.
- `docs/parity-checklist.md` and `docs/scorecard.md`: generated, measured.
- `plan/` (local, gitignored): research and plans. `log/` (local, gitignored): the dev log.

## 8. Keeping the native format complete

`.gt` is the serde form of `guitario-score`. When you add a model field, give it `#[serde(default)]` so older files still load, and map it in `crates/gp` if Guitar Pro has it.

## Guitar Pro format notes

The maintainer keeps private notes on the GP7/8 container and `score.gpif` (not in this repo). Key facts the importer relies on: `.gp` is a ZIP; only `Content/score.gpif` is needed; the XML is a flat id-referenced model (MasterBar → Bars → Voices → Beats → Notes/Rhythms); objects may be shared or unused; `Tuning/Pitches` and note `String` are lowest-string-first; repeat `count` is total plays.

## Contributor credits

- Names for an About window come from `contributors/people.toml` (names people chose for themselves); never add, edit, guess or copy anyone else's entry.
- **Agents working for a contributor:** when you prepare a PR, ask your human once whether they want to be credited by more than their GitHub username; if yes, add only their own entry.
