# Guitario architecture

> **Last reviewed:** 2026-10-15 · **Last updated:** 2026-10-15 · **Change:** major (first version) · **Target:** Guitar Pro 8

## Current state (2026-10-15)

- **Workspace:** 10 library crates under `crates/`, 2 apps (`guitario` desktop,
  `guitario-cli`), `xtask`; ~10,600 lines of Rust in 57 files; 92 `#[test]` functions
  (`cargo xtask stats`).
- **UI toolkit:** egui/eframe on wgpu, no webview and no JavaScript.
- **Engine:** 64 commands (`CommandSpec` in `crates/engine/src/commands.rs` and module `specs()`);
  the UI, CLI and tests dispatch by id through `Session::run`.
- **Shared libraries:** craft-atomic, craft-settings, craft-quit from
  [craft-libs](https://github.com/storytold/craft-libs), pinned by revision in the root `Cargo.toml`.

## Crates and layers

A crate may depend only on lower layers. `cargo xtask layers` enforces it (`xtask/src/layers.rs`).

```text
L0  theory     durations, time signatures, tunings, pitch (standalone: no workspace deps)
L1  score      the document: Score > Track > Measure > Voice > Beat > Note; validation; playback timeline
L2  midi       Standard MIDI File export
L2  playback   tempo map, scheduler, plucked-string synth, WAV
L3  format     native .gt (versioned JSON, craft-atomic saves)
L3  gp         Guitar Pro import (.gp, .gpx, .gp5/.gp4/.gp3) and .gp5 export
L4  audio      sound-card output (cpal): the only platform audio below the shell
L5  engine     Session (score, cursor, selection, clipboard, undo) and the command registry
L6  ui-egui    the shell: Guitar Pro layout, page engraving, menus from the GP menu catalog
    apps       guitario (desktop), guitario-cli (headless)
```

Rules: nothing below L6 uses egui, eframe, winit, rfd or craft-quit; nothing below L4 uses cpal;
L0–L5 build for `wasm32-unknown-unknown` (`cargo xtask wasm`), with file access returning
"unsupported" in the browser.

## The engine/UI seam

The score is immutable behind an `Arc`; a mutating command clones on write, and the session keeps
the previous `Arc` as one undo step. A failed command restores it exactly, so no command can leave
a half-edited score. The UI reads `session.score()` every frame and never edits it directly:
clicks and keys become commands (`cursor.set`, `note.setFret`, …).

## Page layout

`ui-egui/src/layout.rs` is pure (tested without a GPU): measure widths from beat durations, system
breaks with justification, pagination on A4 with Guitar Pro's default stylesheet values (1.5875 mm
staff space, 10 mm side margins). `notation.rs` (pure) spells pitches, places them on the staff and
groups beams. `page.rs` paints, and records hit rectangles that clicks turn into cursor moves.

## Playback

`score::timeline` flattens the score (repeats unrolled, ties merged). `playback::Player` turns it
into timed note events with a `TempoMap`, and renders them with one Karplus–Strong voice per
sounding string (a fractional delay so bends, slides and vibrato glide) and a per-track amp model.
`audio::AudioOut` pulls samples from a shared player slot on the sound card's thread with
`try_lock`, so the UI never blocks audio.
