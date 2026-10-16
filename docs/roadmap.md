# Roadmap

> **Last reviewed:** 2026-10-15 · **Last updated:** 2026-10-15 · **Change:** minor (drums landed: the band-song workflow passes the alpha gate, M6 in progress, current focus re-ranked) · **Target:** Guitar Pro 8

Forward-looking plan: milestones, the **Current focus** list, and what's next with estimates.
Where we stand today is in [`target-app-parity.md`](target-app-parity.md) (headline numbers, by
dimension) and the ranked work list in [`gaps.md`](gaps.md); the one-page summary is
[`ROADMAP.md`](../ROADMAP.md). Measured numbers: [`scorecard.md`](scorecard.md)
(`cargo xtask scorecard`) and [`parity-checklist.md`](parity-checklist.md) (`cargo xtask parity`:
menu wiring, which says nothing about behaviour).

Status legend: ✅ done · 🟡 in progress · ⬜ not started. Hours are Opus 5.5 agent wall-clock hours
for one agent working sequentially (uncalibrated, see
[`target-app-parity.md`](target-app-parity.md#remaining-effort-and-calibration)).

**Stage: pre-alpha** (ready for real work ~20%). Alpha needs the gate below: ~220–420 h.

## Alpha gate

The core workflows a typical Guitar Pro user runs every week, checked end to end on the main
platform (macOS, 0.1.0), including saving and reopening the work. Any **no**, or a **partial**
that blocks the workflow, keeps Guitario pre-alpha.

| Core workflow | Works end to end? | Evidence | Hours to pass |
|---|---|---|---:|
| Transcribe a riff: enter notes, rhythm and techniques, play it back, save and reopen | yes | keyboard entry, palette, playback, `.gt` round trip (`crates/engine/tests`) | 0 |
| Practise: loop a passage, slow it down, metronome and count-in | yes | selection loop, speed 25–200 %, metronome, count-in (`crates/playback` tests) | 0 |
| Arrange a band song: guitars, bass and drums, sections, repeats | yes | drum tracks, pad, notation and playback; 12 band demos with drums (`crates/engine/tests/demos.rs`) | 0 |
| Open a tab someone sent (`.gp5` or `.gp`), play it, edit it | **partial (blocking)** | `.gp` 7/8 import with drums; `.gp5`/`.gpx` not read (G3) | 100–160 |
| Share: print or export PDF, or send a `.gp` back | **no** | MIDI, WAV and ASCII only (G2, G3) | 80–140 |
| Read and write standard notation with ties and two voices | **partial (blocking)** | notation renders; no ties/slurs or voices (G4) | 60–110 |

**Result: fails.** Two workflows are blocked and one is impossible, so the stage is pre-alpha.

## Milestones

| M | Status | Where we are |
|---|---|---|
| M0 Skeleton | ✅ | workspace, xtask (layers / wasm / ci / stats / parity / scorecard / version), CI workflow, never-crash lints |
| M1 Model + engine | ✅ | theory, score (validation, timeline), 64 commands with undo, CLI, `.gt` with craft-atomic saves |
| M2 Tab editor | 🟡 | keyboard entry, Guitar Pro shortcuts, palette, selection and clipboard; one voice only, no bend editor |
| M3 Notation | 🟡 | page view, notation over tab, beams, accidentals, effect lanes; no ties, voices, chords, drums |
| M4 Playback | 🟡 | plucked-string synth, transport, loop, speed, metronome, count-in, WAV export; no drums or samples |
| M5 Guitar Pro exchange | 🟡 | `.gp` 7/8 import (partial); legacy formats and export missing |
| M6 Drums | 🟡 | kit tracks, drum pad, notation, GM playback, `.gp` drum import, drum demos; two voices and a step editor missing |
| M7 Print, PDF, MusicXML | ⬜ | |
| M8 Tools | ⬜ | tuner, chord engine, scales |
| M9 Automation | ⬜ | control channel, MCP |
| M10 Distribution | ⬜ | packaging, signing, icon, web build |

## Current focus

Ranked; each item links to its gap entry. Pick from the top unless an issue is assigned to you.

1. **Guitar Pro exchange** ([G3](gaps.md#g3-guitar-pro-exchange)): `.gp5` import first (most shared
   tabs), then `.gpx`, then `.gp` export; a real-file corpus. 150–250 h.
2. **Print and PDF** ([G2](gaps.md#g2-print-and-pdf-export)) through craft-pdf and craft-print. 60–100 h.
3. **Ties, slurs and voices** ([G4](gaps.md#g4-notation-depth)), which also gives drums their two voices ([G1](gaps.md#g1-drums-depth)). 60–110 h for the alpha part.

## What's next, with estimates

| Next milestone | Scope | Estimate |
|---|---|---:|
| 0.2 "the band" | ✅ drums landed 2026-10-15 (kit tracks, notation, playback, import, demo parts) | done |
| 0.3 "files from friends" | `.gp5` and `.gpx` import, `.gp` import depth, a real-file corpus | 120–200 h |
| 0.4 "print it" | PDF export and printing; ties, slurs and two voices | 120–210 h |
| Alpha | The alpha gate passes | ~220–420 h total |
| Beta | `.gp` export, sampled playback for guitar/bass/drums, tools, layouts, releases on three platforms | ~1,100–1,800 h total |

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
| 2026-10-15 | minor | Drums landed: band-song workflow passes; M6 in progress; focus re-ranked |
| 2026-10-15 | major | Milestones M0–M10, alpha gate (fails), current focus |
