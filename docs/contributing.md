# Contributing

- **Language:** Rust only. No JavaScript or TypeScript.
- **Licence:** contributions are MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`). The Guitario logos in `docs/brand/` are not open source (`docs/brand/LICENSE-brand.txt`).
- **Clean-room:** do not copy code, sounds, fonts, stylesheets or other assets from Guitar Pro or other tab editors. Match behaviour and look by observation and public format notes. Only use permissively licensed assets, keep their licence next to them and list them in `ATTRIBUTION.md`. **Fonts** live in craft-fonts, never in this repo.
- **Original content:** demo scores and examples are original compositions (the suite centres on metal); never transcribe existing songs.
- **Never crash:** non-test code must not panic: no `unwrap`/`expect`/`panic!`/`unreachable!`/`todo!`/`unimplemented!`, no indexing, no `unsafe`. Errors go through `Result`; input-derived numbers are checked; every crash fix ships with a regression test.
- **Commands, not handlers:** new features are engine commands with tests, and the UI calls them (checklist below).
- **Layering:** `cargo xtask layers` must pass. Register new crates in `xtask/src/layers.rs`.
- **Tests:** required for every change. Formats need round-trip and malformed-input tests.
- **Style:** `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`. Comments explain *why*.
- **UI:** colours come from `theme.rs` tokens. Verify visually with the offscreen `snapshot` example and attach before/after images to PRs.
- **Commits:** small, focused, with a clear subject line.

## Adding a command

1. **Find the menu item.** Look it up in `crates/ui-egui/src/menu_catalog.rs` (Guitar Pro's menus). Give the command an id in the engine's style (`note.toggleTie`) and point the catalog item at it with `c(…)`: the menu item goes live and parity rises.
2. **Logic** goes in the lowest crate that fits (`theory`, `score`, `playback`), with unit tests.
3. **Command** goes in an engine module (`crates/engine/src/<area>.rs` with a `specs()` function, chained into the registry in `commands.rs`): id, label, menu path, Guitar Pro's default shortcut, a params doc string, `mutates`, and `run`, which returns JSON and changes the score in one step.
4. **Respect the selection:** effects and edits that make sense on a range apply to `selected_beats_mut()` when a selection exists.
5. **Tests:** behaviour, undo, bad params; `panic_hunt` covers the command automatically.
6. Run `cargo xtask parity` and `cargo xtask scorecard`, raise `FLOOR` if parity rose, and commit the regenerated docs.
