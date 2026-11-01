# Guitario architecture

> **Last reviewed:** 2026-10-17 · **Last updated:** 2026-10-17 · **Change:** minor (counts refreshed) · **Target:** Guitar Pro 8

## Current state (2026-10-17)

- **Workspace:** 9 library crates under `crates/`, 2 apps (`guitario` desktop,
  `guitario-cli`), `xtask`; ~38,300 lines of Rust in 122 files; 417 `#[test]` functions
  (`cargo xtask stats`).
- **UI toolkit:** egui/eframe on wgpu, no webview and no JavaScript.
- **Engine:** 117 commands (`CommandSpec` in `crates/engine/src/commands.rs` and module `specs()`);
  the UI, CLI and tests dispatch by id through `Session::run`.
- **Shared libraries:** craft-atomic, craft-settings, craft-quit from
  [craft-libs](https://github.com/storytold/craft-libs), pinned by revision in the root `Cargo.toml`.

## Crates and layers

A crate may depend only on lower layers. `cargo xtask layers` enforces it (`xtask/src/layers.rs`).

```text
L0  theory     durations, time signatures, tunings, pitch (standalone: no workspace deps)
L1  score      the document: Score > Track > Measure > Voice > Beat > Note; validation; playback timeline
L2  midi       Standard MIDI File import and export
L2  playback   tempo map, scheduler, plucked-string synth, WAV
L3  format     native .gt (versioned JSON, craft-atomic saves)
L3  gp         Guitar Pro import (.gp, .gpx, .gp5/.gp4/.gp3) and .gp/.gp5 export
L3  musicxml   MusicXML import (.musicxml, .xml, .mxl) and export
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

## Print and PDF

The engine cannot draw pages or reach a printer (no egui, no spooler below L6), so it defines two
small traits and the commands that use them: `PageRenderer` (score and request in, PDF bytes out;
`file.exportPdf`) and `Spooler` (printer list, submit a PDF; `file.print`, `file.printers`).
`engine/src/render.rs` and `print.rs` hold them; a fake pair tests the commands. The shell
registers `ui-egui/src/pdf.rs` and `spool.rs` (and `GuitarioApp::new` does it, so the desktop
app, tests and offscreen renders all print). `pdf.rs` paints each page with
`page::paint_print_page`, the page view's own engraving, into a headless egui context, then turns
the recorded shapes into Krilla drawing operations through craft-pdf's guarded writer: lines,
curves and rectangles become paths, and every glyph is placed where egui laid it out and drawn from
an embedded subset of the font that supplied it. `spool.rs` wraps craft-print
(CUPS, native Windows). `print_dialog.rs` runs the printer lookup and the job on worker threads;
a job (`PrintTask`) owns a snapshot of the score, so editing during a print is safe.
`page::PrintSet` lays out the chosen tracks and numbers their pages; the PDF, the preview
(`preview.rs`, painted small through a visual transform) and the printer all take pages from it.
The paper is `Score::page` (`PageSetup`: A4, A5, Letter, Legal; orientation; margins in mm), set
by `score.setPage` and `page_setup.rs`; `Metrics::with_page` turns it into layout units for the
page view and for print alike. While a dialog is open, keys do not reach the score.

## Playback

`score::timeline` flattens the score (repeats unrolled, ties merged). `playback::Player` turns it
into timed note events with a `TempoMap`, and renders them with one Karplus–Strong voice per
sounding string (a fractional delay so bends, slides and vibrato glide) and a per-track amp model.
The General MIDI program picks a modelled instrument instead (`playback/src/instrument.rs`): a
grand piano (`piano.rs`: modes per key struck by a nonlinear felt hammer) or the violin family
(`bowed.rs`: a waveguide string driven by a bow's stick-slip friction), each sounding through a
body of resonances per track (`body.rs`: soundboard, or violin, viola, cello and double bass
boxes with sympathetic strings). A string section is a bowed string per player (up to sixteen,
`Track::players`), with their own intonation, timing, vibrato and bow, on up to four matched
instruments across the stage, chords divided among them. The strings are measured against
recordings of real players ([strings-calibration](strings-calibration.md)).
`audio::AudioOut` pulls samples from a shared player slot on the sound card's thread with
`try_lock`, so the UI never blocks audio.
