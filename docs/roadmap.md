# Roadmap

> **Last reviewed:** 2026-10-16 · **Last updated:** 2026-10-16 · **Change:** minor (fingering; menus 136/173) · **Target:** Guitar Pro 8

Forward-looking plan: milestones, the **Current focus** list, and what's next with estimates.
Where we stand today is in [`target-app-parity.md`](target-app-parity.md) (headline numbers, by
dimension) and the ranked work list in [`gaps.md`](gaps.md); the one-page summary is
[`ROADMAP.md`](../ROADMAP.md). Measured numbers: [`scorecard.md`](scorecard.md)
(`cargo xtask scorecard`) and [`parity-checklist.md`](parity-checklist.md) (`cargo xtask parity`:
menu wiring, which says nothing about behaviour).

Status legend: ✅ done · 🟡 in progress · ⬜ not started. Hours are Opus 5.5 agent wall-clock hours
for one agent working sequentially (uncalibrated, see
[`target-app-parity.md`](target-app-parity.md#remaining-effort-and-calibration)).

**Stage: alpha** (ready for real work ~27%). The alpha gate below passes; beta is ~810–1,375 h away.

## Alpha gate

The core workflows a typical Guitar Pro user runs every week, checked end to end on the main
platform (macOS, 0.1.0), including saving and reopening the work. Any **no**, or a **partial**
that blocks the workflow, keeps Guitario pre-alpha.

| Core workflow | Works end to end? | Evidence | Hours to pass |
|---|---|---|---:|
| Transcribe a riff: enter notes, rhythm and techniques, play it back, save and reopen | yes | keyboard entry, palette, playback, `.gt` round trip (`crates/engine/tests`) | 0 |
| Practise: loop a passage, slow it down, metronome and count-in | yes | selection loop, speed 25–200 %, metronome, count-in (`crates/playback` tests) | 0 |
| Arrange a band song: guitars, bass and drums, sections, repeats | yes | drum tracks, pad, notation and playback; 12 band demos with drums (`crates/engine/tests/demos.rs`) | 0 |
| Open a tab someone sent (`.gp5` or `.gp`), play it, edit it | yes | `.gp`, `.gpx`, `.gp5`, `.gp4`, `.gp3` through File › Open; 76 real files import (G3); every voice is kept, drawn and played | 0 |
| Share: print or export PDF, or send a `.gp` back | yes | File › Export › PDF… writes A4 vector pages for every demo without warnings (`crates/ui-egui/src/pdf.rs` tests; pages checked as rasters); File › Print… sends them through craft-print (not yet tried on a real printer); `.gp` and `.gp5` go back to Guitar Pro users (`.gp` read back by us for every demo and 76 real files, not yet opened in Guitar Pro itself) | 0 |
| Read and write standard notation with ties and two voices | yes | up to four voices (Edit › Voice 1-4), ties across bars, slurs for hammer-ons and legato; `crates/ui-egui/src/page.rs` tests, `demos/15-two-voices.gt` | 0 |

**Result: passes.** Every workflow works end to end, so the stage is alpha. Two human checks are
still open and the pass does not hide them: a page on a real printer, and a `.gp` export opened
in Guitar Pro 8.

## Milestones

| M | Status | Where we are |
|---|---|---|
| M0 Skeleton | ✅ | workspace, xtask (layers / wasm / ci / stats / parity / scorecard / version), CI workflow, never-crash lints |
| M1 Model + engine | ✅ | theory, score (validation, timeline), 64 commands with undo, CLI, `.gt` with craft-atomic saves |
| M2 Tab editor | 🟡 | keyboard entry, Guitar Pro shortcuts, palette, selection and clipboard, four voices; no bend editor |
| M3 Notation | 🟡 | page view, notation over tab, beams, accidentals, effect lanes, ties, slurs, voices, chord names and diagrams, lyrics, tremolo picking, trills, grace notes, fermatas, fingering, drums; no layouts |
| M4 Playback | 🟡 | plucked-string synth, transport, loop, speed, metronome, count-in, WAV export; no drums or samples |
| M5 Guitar Pro exchange | 🟡 | import of `.gp`, `.gpx`, `.gp5`, `.gp4`, `.gp3`; `.gp` and `.gp5` export; a human check of `.gp` exports in Guitar Pro 8 and GP6 drums remain |
| M6 Drums | 🟡 | kit tracks, drum pad, notation, GM playback, `.gp` drum import, drum demos, a second voice for the feet; a step editor missing |
| M7 Print, PDF, MusicXML | 🟡 | PDF export, printing, print preview, page setup; no MusicXML, headers and footers |
| M8 Tools | 🟡 | chord diagram editor and namer; no tuner, chord dictionary or scales |
| M9 Automation | ⬜ | control channel, MCP |
| M10 Distribution | ⬜ | packaging, signing, icon, web build |

## Current focus

Ranked; each item links to its gap entry. Pick from the top unless an issue is assigned to you.

1. **Notation depth** ([G4](gaps.md#g4-notation-depth)): layouts, beaming by the signature, sloped beams, multivoice editing. 40–80 h.
2. **Guitar Pro import depth** ([G3](gaps.md#g3-guitar-pro-exchange)): Guitar Pro 6 drums, MIDI and MusicXML import, a human check of `.gp` exports. 40–80 h.
3. **Print depth** ([G2](gaps.md#g2-print-depth)): headers and footers, layouts, selected bars, a real-printer check. 10–20 h.

## What's next, with estimates

| Next milestone | Scope | Estimate |
|---|---|---:|
| 0.2 "the band" | ✅ drums landed 2026-10-15 (kit tracks, notation, playback, import, demo parts) | done |
| 0.3 "files from friends" | ✅ `.gp5`/`.gp4`/`.gp3`/`.gpx` import and `.gp5` export landed 2026-10-16, checked on a local real-file corpus | done |
| 0.4 "ties and voices" | ✅ ties, slurs and up to four voices landed 2026-10-16 | done |
| 0.5 "print it" | ✅ PDF export and printing landed 2026-10-16 | done |
| Alpha | ✅ the alpha gate passes (2026-10-16) | done |
| Beta | sampled playback for guitar/bass/drums, tools, layouts, print depth, releases on three platforms | ~810–1,375 h total |

## Milestone definitions

Each milestone has a **definition of done (DoD)** and must leave `main` green: `cargo xtask ci`
and `cargo xtask wasm`.

| M | Name | Scope (key items) | DoD / acceptance |
|---|---|---|---|
| **M0** | Skeleton | Workspace, crate stubs, lints, xtask, CI | `cargo xtask ci` green; layering enforced |
| **M1** | Model + engine | Score model, command registry with undo, CLI, native format | Every edit runs headless; `panic_hunt` green |
| **M2** | Tab editor | Entry, durations, techniques, selection, clipboard, voices 1–4, bend editor | A guitarist can transcribe a two-voice part without the mouse |
| **M3** | Notation | Engraving of everything the model holds, layouts, multitrack | Offscreen renders of the demo suite reviewed against Guitar Pro |
| **M4** | Playback | Synth, transport, mixer, automations, swing, export | The demo suite plays in time; budgets met (G9) |
| **M5** | Guitar Pro exchange | `.gp`/`.gpx`/`.gp5`/`.gp4`/`.gp3` import, `.gp` export | ≥ 95% of a real-file corpus imports without warnings; `.gp` exports open in Guitar Pro (human check) |
| **M6** | Drums | Kit tracks, percussion notation and editor, GM playback, import | A metal demo with drums plays and round-trips through `.gp` |
| **M7** | Print, PDF, MusicXML | craft-pdf export, craft-print, MusicXML both ways | PDF of every demo matches the page view |
| **M8** | Tools | Tuner, chord engine and diagrams, scales, line in | Tuner within ±2 cents on test tones |
| **M9** | Automation | JSON control channel, MCP server | An agent completes 10 scripted edit tasks over MCP |
| **M10** | Distribution | Signed releases on macOS, Windows, Linux; icon; web build | Install and open a `.gp` on each platform |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-16 | minor | Fingering for both hands (dialog, notation, Guitar Pro 3-8 both ways); menus 136/173; G4 left 40–80 h |
| 2026-10-16 | minor | Fermatas (drawn, held in playback and MIDI, GPIF both ways); G4 left 45–85 h; beta ~810–1,375 h |
| 2026-10-16 | minor | Lyrics (model, command and dialog, engraving under the tab, Guitar Pro 4-8 both ways); G4 left 50–95 h; beta ~815–1,385 h |
| 2026-10-16 | minor | Chord diagrams drawn and kept in `.gp`; Tools › Chord Diagrams… with a chord namer (M8 started); G4 left 60–115 h; beta ~830–1,410 h |
| 2026-10-16 | minor | Grace notes (model, command and dialog, playback, engraving, Guitar Pro 3-8 both ways); G4 left 70–130 h; beta ~850–1,440 h |
| 2026-10-16 | minor | Tremolo picking and trills (model, commands, playback, drawing, Guitar Pro 3-5 and GPIF); G4 left 80–145 h; beta ~860–1,455 h |
| 2026-10-16 | minor | Print preview and page setup (Letter, Legal, A5, landscape, margins; kept in `.gt`, `.gp` and `.gp5`); G2 left 10–20 h; beta ~870–1,470 h |
| 2026-10-16 | minor | PDF export and printing: the share workflow passes, so the alpha gate passes; stage alpha; beta ~880–1,490 h (G2's 60–100 h done, 20–40 h of print depth added) |
| 2026-10-16 | minor | `.gp` export: the share workflow only lacks print/PDF; beta ~920–1,550 h |
| 2026-10-16 | minor | Ties, slurs and up to four voices: the notation workflow passes; alpha ~70–120 h; print/PDF is the focus |
| 2026-10-16 | minor | Guitar Pro 3-6 import and `.gp5` export: the open-a-tab workflow passes; alpha ~130–230 h; focus re-ranked |
| 2026-10-15 | minor | Drums landed: band-song workflow passes; M6 in progress; focus re-ranked |
| 2026-10-15 | major | Milestones M0–M10, alpha gate (fails), current focus |
