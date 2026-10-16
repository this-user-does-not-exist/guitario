# Where Guitario falls short of Guitar Pro

> **Last reviewed:** 2026-10-16 · **Last updated:** 2026-10-16 · **Change:** minor (G2: PDF export, printing, print preview and page setup landed; G3: `.gp` (Guitar Pro 7/8) export landed; earlier G4: ties, slurs and multiple voices; G1: drums' second voice) · **Target:** Guitar Pro 8

Every known shortfall against Guitar Pro 8, one entry each, ranked by user impact on the way to
alpha and beta. This is the work list agents pick from: take the highest-ranked entry that nobody
is working on, and when you close part of one, update its evidence and estimate here and the
matching parity doc. Numbers come from [`target-app-parity.md`](target-app-parity.md); checklist ids
(`EDIT-…`, `NOTE-…`, `FILE-…`) are rows in [`scorecard.md`](scorecard.md).

"Blocks alpha" marks what fails the [alpha gate](roadmap.md#alpha-gate) today.

Hours are Opus 5.5 agent-hours, one agent working sequentially, including tests (uncalibrated,
see [calibration](target-app-parity.md#remaining-effort-and-calibration)). **Human** marks what an
agent can't finish alone.

## Summary

| # | Gap | Kind | Impact | Estimate | Parity doc |
|---|---|---|---|---:|---|
| G1 | Drums: depth (step editor, articulations) | features, playback | medium | 30–60 h | [scorecard](scorecard.md#editing) |
| G2 | Print depth: headers and footers, layouts, selected bars | file format | low-medium | 10–20 h | [file-format-parity](file-format-parity.md) |
| G3 | Guitar Pro exchange (import depth, MusicXML, MIDI import) | file format | medium-high | 40–80 h | [file-format-parity](file-format-parity.md) |
| G4 | Notation depth (diagrams, lyrics, grace notes, layouts) | features | high | 90–160 h | [scorecard](scorecard.md#notation-and-engraving) |
| G5 | Playback realism (samples, effects, mixer, automations) | playback | high | 300–600 h | [scorecard](scorecard.md#playback) |
| G6 | Editing depth (bend editor, tuplets, paste special, track tools) | features | medium-high | 80–140 h | [scorecard](scorecard.md#editing) |
| G7 | Tools: tuner, chord engine, scales, line in | features | medium | 120–200 h | [target-app-parity](target-app-parity.md#feature-breadth-33-weighted) |
| G8 | UI depth: layouts, multitrack, keyboard and drum views, preferences, themes | UI | medium | 120–200 h | [ui-parity](ui-parity.md) |
| G9 | Performance budgets | performance | medium | 30–60 h | [scorecard](scorecard.md) |
| G10 | Automation: control channel and MCP | ecosystem | medium | 40–80 h | [scorecard](scorecard.md#automation) |
| G11 | Reliability depth: panic guard, reader fuzzers, autosave | stability | medium | 40–80 h | [scorecard](scorecard.md#reliability) |
| G12 | Platforms and distribution | platforms | low now, blocks beta | 80–150 h + human | [scorecard](scorecard.md#distribution) |
| G13 | Languages | localization | low now | 40–80 h + human | [ui-parity](ui-parity.md) |

## Feature gaps

### G1. Drums: depth
- **Landed 2026-10-15:** drum-kit tracks (`track.addDrums`, `note.setDrum`, eight kit lanes),
  percussion notation (clef, kit-map positions, cross noteheads), a drum pad view, two-digit GM
  entry, synthesised GM drum playback with hi-hat choke, MIDI channel 10, ASCII drum tab, and
  `.gp` drum import by articulation index or MIDI number. Twelve demos have drum parts and
  `14-drum-grooves` covers skank, d-beat, gallop, double kick, half time, blasts, china hits and a
  triplet fill (`crates/engine/tests/demos.rs`).
- **Landed 2026-10-16:** a second voice for the feet (stems down, hands up) as Guitar Pro writes
  drums: Edit › Voice 2, drawn and played, and read from `.gp5` and `.gp`.
- **Missing:** a step-grid drum editor; articulations beyond the kit map (flams, rolls, ghost and
  accent noteheads, open-hat circles); sampled drums.
- **Estimate:** 30–60 h.

### G2. Print depth
- **Landed 2026-10-16:** PDF export (File › Export › PDF…, `file.exportPdf`, `guitario-cli convert
  in out.pdf [--tracks 0,2] [--tab-only]`) and printing (File › Print…, Cmd+P, `file.print` and
  `file.printers`). The pages are the page view's own engraving (`page::paint_print_page`),
  painted into a headless egui context; `crates/ui-egui/src/pdf.rs` turns the recorded shapes into
  Krilla drawing operations through craft-pdf's guarded writer, so the PDF is vector with embedded
  font subsets and selectable text. Each track starts on a new page; the header is on each track's
  first page; page numbers run through the document. The engine owns the commands and two small
  traits (`PageRenderer`, `Spooler`), so it stays wasm-clean and a fake pair tests them. Printing
  goes through craft-print (CUPS, or the native Windows path) on a worker thread, with pages
  shrunk to fit other paper. Every demo exports without warnings (`pdf.rs` tests); pages were
  checked by rasterizing with poppler.
- **Landed 2026-10-16 (second step):** File › Print Preview… (the pages exactly as the PDF has
  them, painted through the same `PrintSet`, one track or all) and File › Page Setup… (A4, A5,
  Letter, Legal; portrait or landscape; four margins in mm). The setup is `Score::page`
  (`score.setPage`, saved in the `.gt` file with a default for older files, one undo step) and
  drives the page view, the PDF's page size and the print job. It is read from and written to
  Guitar Pro 5 and GPIF files in whole millimetres (checked on 114 real files, one of them Letter).
  Keys no longer reach the score while a dialog is open.
- **Missing:** headers and footers; printing selected bars; layout choices (page, screen,
  multitrack) that Guitar Pro prints; other paper than the four; images and gradients in a page
  (none exist yet); fonts that carry notation symbols (the fallback shapes are used when no SMuFL
  font is installed). A Guitar Pro file with another paper size opens as A4 with a warning.
- **Human:** send a page to a real printer on macOS, Windows and Linux; the Windows spooler path
  has never run, and CUPS was only tested through `lp`'s argument list.
- **Evidence:** FILE-7 and FILE-11 partial.
- **Estimate:** 10–20 h.

### G3. Guitar Pro exchange
- **Landed 2026-10-16:** `.gp5`, `.gp4` and `.gp3` import (`crates/gp/src/legacy.rs`), `.gpx`
  (Guitar Pro 6) import (`gpx.rs`), `.gp5` export (`write5.rs`, File › Export › Guitar Pro 5…) and
  chord names. File › Open tells the format by content. Checked against a local corpus of 39 real
  GP3-5 and 37 real GP6 files (`crates/gp/tests/corpus.rs` with `GUITARIO_GP_CORPUS`): all
  import, and the GP3-5 ones come back unchanged through the GP5 writer. The corpus is public test
  data from other projects, used locally and not committed.
- **Landed 2026-10-16:** `.gp` (Guitar Pro 7/8) export (`writegp.rs`, `file.exportGp`, File ›
  Export › Guitar Pro 8…): up to four voices, ties, every effect the model carries, repeats,
  endings, directions, drum kits. Every demo and all 76 corpus files read back unchanged.
  **Not yet opened in Guitar Pro itself** (no copy here): it follows the structure of the
  importer's fixture and of real GP6 `score.gpif` files, so a human should open a few exports in
  Guitar Pro 8 before this is called done. Chord names and track volume/pan are not written (the
  importer does not read them either).
- **Missing:** Guitar Pro 6 drum hits (kit element and variation, no MIDI number in the file); grace notes,
  trills, tremolo picking, whammy, fingering and lyrics on import (the model lacks them, G4);
  MusicXML and MIDI import.
- **Evidence:** FILE-3 partial, FILE-4 and FILE-10 done, FILE-6 missing.
- **Estimate:** 40–80 h (+ a human to check `.gp` exports in Guitar Pro).

### G4. Notation depth
- **Landed 2026-10-16:** chord names (model, `beat.setChord`, Guitar Pro import and export) and
  beat text drawn above the system.
- **Landed 2026-10-16:** up to four voices per bar (Edit › Voice 1-4, `edit.setVoice`): voices
  share columns by time so notes that sound together line up, Voice 1 takes stems up and Voice 2
  stems down with rests moved apart, every voice plays and exports; ties drawn in notation and tab
  (across bars and system breaks); slurs for hammer-ons, pull-offs and legato slides.
- **Missing:** chord diagrams, lyrics, tremolo slashes, grace notes, fermatas, a Multivoice Editing
  mode, user-placed slurs, mid-system key changes, beaming by the time signature's grouping,
  sloped beams, and offsetting heads where two voices share a pitch.
- **Evidence:** NOTE-5 and EDIT-9 done; NOTE-3/4/9/12 and EDIT-16 partial.
- **Estimate:** 90–160 h.

### G5. Playback realism
- **Landed 2026-10-16:** an SFZ sampler and per-role sound bank ([`sounds.md`](sounds.md)) with
  open CC0/CC-BY libraries for guitars, bass and drums (`cargo xtask sounds`).
- **Missing:** release samples, background loading, per-track instrument choice in the UI, mixer
  and sample-based articulations (palm mute, harmonics, slides); amp and effect
  chains per track, a mixer panel, automations (tempo ramps, volume, pan), swing / triplet feel,
  MP3/FLAC/OGG export.
- **Evidence:** PLAY-3/5/7 partial, PLAY-6 missing. The synth is checked by tests (pitch within 4 Hz,
  clean decay, no clipping), not yet by listening on real speakers.
- **Estimate:** 250–500 h; a library tuned to Guitar Pro's articulations needs a listening pass
  (**human**).

### G6. Editing depth
- **Landed 2026-10-16:** tuplets 3:2 to 10:8 (exact at 20160 ticks per quarter), clear bar, double
  bars, free time, triplet feel (played), duplicate and reorder tracks, wide vibrato, heavy accent,
  tapping, slap/pop, pick strokes, fade in/out and swell, brush, arpeggio and rasgueado (played);
  alternate endings and directions (segno, coda, Fine, D.C./D.S./To Coda) edited, played, drawn
  and imported from `.gp`.
- **Missing:** bend-curve editor (pre-bend, release, multi-point), nested tuplets, Paste Special,
  transpose track.
- **Evidence:** EDIT-3/5/8/13 in the scorecard.
- **Estimate:** 80–140 h.

### G7. Tools
- **Missing:** tuner (needs microphone input), chord engine and diagrams, scale tool, line in,
  bar-duration checker.
- **Estimate:** 120–200 h.

### G8. UI depth
- **Missing:** page/screen/horizontal layouts, multitrack view, keyboard and drum-pad instrument
  views, a Preferences dialog, light theme and UI scale, Bar and Note tabs in the inspector,
  floating panels.
- **Evidence:** [ui-parity](ui-parity.md); UI-3/5/8/9 in the scorecard.
- **Estimate:** 120–200 h.

### G9. Performance budgets
- **Missing:** any measurement. Layout and painting run every frame over the visible pages;
  nothing has been timed on a long score (300+ bars, 6+ tracks).
- **Plan:** `perf/budgets.toml` and `cargo xtask perf`, as another app does.
- **Estimate:** 30–60 h.

### G10. Automation
- **Missing:** a JSON control channel to drive and screenshot the running app, and an MCP server.
  Every action is already a command, so this is transport only.
- **Estimate:** 40–80 h.

### G11. Reliability depth
- **Missing:** a last-resort panic guard around command dispatch and imports in the shell; fuzz
  targets for the `.gp` and `.gt` readers; autosave and crash recovery.
- **Estimate:** 40–80 h.

### G12. Platforms and distribution
- **Missing:** releases (signed and notarized macOS, signed Windows, Linux packages), an app icon,
  file associations, the browser build. CI exists but has not run (no remote yet).
- **Estimate:** 80–150 h + human (signing material, store accounts).

### G13. Languages
- **Missing:** everything beyond English; Guitar Pro ships in about a dozen languages.
- **Estimate:** 40–80 h + human review.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-15 | major | First ranked list (G1–G13) |
| 2026-10-16 | minor | G6: bar, track and technique commands landed (menus 94 → 117) |
| 2026-10-16 | minor | G5: SFZ sampled instruments for drums, bass and guitars landed (estimate 300–600 h to 250–500 h) |
| 2026-10-16 | minor | G3: GP3-6 import and `.gp5` export landed, no longer blocks alpha (150–250 h to 60–110 h); G4: chord names |
| 2026-10-16 | minor | G4: ties, slurs and up to four voices landed (150–250 h to 90–160 h); G1: drums' second voice (40–80 h to 30–60 h) |
| 2026-10-16 | minor | G3: `.gp` (Guitar Pro 7/8) export landed, unverified in Guitar Pro itself (60–110 h to 40–80 h) |
