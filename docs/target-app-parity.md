# Guitario vs Guitar Pro 8: target-app parity

> **Last reviewed:** 2026-10-16 · **Last updated:** 2026-10-16 · **Change:** minor (menus 94 → 117 of 172: breadth ~37% → ~41%) · **Target:** Guitar Pro 8

The authoritative answer to "how close is Guitario to Guitar Pro, and how much work is left".
The one-page summary is [`ROADMAP.md`](../ROADMAP.md); the ranked work list is [`gaps.md`](gaps.md);
the measured checklists are [`scorecard.md`](scorecard.md) (`cargo xtask scorecard`) and
[`parity-checklist.md`](parity-checklist.md) (`cargo xtask parity`). Detail by dimension:
[file formats](file-format-parity.md), [UI](ui-parity.md).

## Headline (2026-10-15, Guitario 0.1.0)

| Number | Value | Kind |
|---|---|---|
| **Feature breadth** (does each Guitar Pro feature exist?) | **~41%** | partly measured (menus 119/172), weights below |
| **Ready for real work** (could a guitarist replace Guitar Pro with it?) | **~20%** (range 15–25%) | estimated, evidence below |
| **Mainstream user** (writes and learns tab every week) | **~25%** (20–30%) | estimated, [method](#readiness-by-audience) |
| **Essentials user** (enter a riff, hear it, save it) | **~60%** (50–70%) | estimated |
| **Stage** | **pre-alpha** | the [alpha gate](roadmap.md#alpha-gate) fails: no print or PDF, Guitar Pro exchange is import-only and partial, no ties or voices |
| Remaining to **alpha** | **~220–420 Opus 5.5 agent-hours** | estimated; G2–G4 in [gaps](gaps.md) |
| Remaining to **beta** (~75% ready) | **~1,100–1,800 Opus 5.5 agent-hours** | estimated |
| Remaining to **full parity** (~95–100%) | **~2,200–3,800 Opus 5.5 agent-hours** | estimated; sampled-instrument playback (RSE) dominates |

### Readiness by audience

| Audience | Ready % | Opus 5.5 agent-hours to ~95% | Work that dominates |
|---|---:|---:|---|
| Full target (ready for real work) | ~20% | 2,100–3,700 | Everything below, plus sampled playback, tools (tuner, chords, scales), every legacy format, distribution |
| Mainstream user (band arranger, teacher, learner) | ~25% | 1,100–1,900 | Guitar Pro 5/6/7/8 exchange both ways, print/PDF, multiple voices, ties/slurs, playback that sounds like an instrument |
| Essentials user | ~60% | 200–380 | PDF export, ties in notation, GP5 import (most tabs online are GP5) |

Each tier is a subset of the one above. A mainstream Guitar Pro user, by the tabs shared online
and the features GP's own lessons cover, weekly: enters and edits tab, reads standard notation,
plays back with drums, loops and slows passages, opens files from others (mostly `.gp5` and
`.gp`), prints or exports PDF, and occasionally uses the tuner and chord tools.

## Target measured

- **Guitar Pro 8** (Arobas Music), the current major version; the format notes in the author's
  `Guitar Pro 7 File Format` folder cover files written by GP 7.0 to 8.1.3.
- **Not measured from the installed app.** Guitar Pro is not installed on the machine this was
  written on, so unlike another app (which reads Photoshop's bundle without launching it), the
  menu catalog (`crates/ui-egui/src/menu_catalog.rs`) and these feature lists come from Guitar
  Pro's documentation, its file format (the `.gp` notes and corpus findings) and observation.
  Re-measure against an installed copy when one is available: menu resources, preset tunings,
  shortcut defaults and sound banks.
- Clean-room: behaviour and look only. No Guitar Pro code, sounds, fonts or stylesheets are used.

## Feature breadth: ~41% (weighted)

| Component | Weight | Ours / Guitar Pro | % | Kind |
|---|---:|---|---:|---|
| Menu commands | 30% | 119 / 172 catalog items live | 69% | measured (`cargo xtask parity`) |
| Notation and engraving | 15% | page view, notation + tab, drum staff, effects lanes; no ties/slurs, voices, chords, lyrics, alternate endings, layouts | ~40% | estimated ([scorecard](scorecard.md#notation-and-engraving)) |
| Instruments and track types | 10% | fretted strings in any tuning (4–12 strings, 12 presets) and a GM drum kit; no keys or other instruments | ~50% | estimated |
| Playback | 15% | physical-model guitar/bass, synthesised drums, transport, loop, speed, metronome, count-in; no samples, effects, mixer, automations | ~35% | estimated ([scorecard](scorecard.md#playback)) |
| File formats (usage-weighted) | 15% | `.gp` 7/8 import (partial), MIDI/WAV/ASCII export; no `.gp5`/`.gpx`/`.gp4`/`.gp3`, no GP/MusicXML/PDF export, no MIDI/MusicXML import | ~20% | estimated ([file-format-parity](file-format-parity.md)) |
| Tools (tuner, chord engine, scales, line in) | 10% | none | 0% | measured (absent) |
| Ecosystem and automation | 5% | engine commands, CLI, scripts; no control channel or MCP | ~30% | estimated |
| **Weighted** | 100% | | **~41%** | |

## Ready for real work, by dimension

| Dimension | % ready | Hours to full | Evidence |
|---|---:|---:|---|
| Editing | 50% | 130–230 | [scorecard › Editing](scorecard.md#editing) |
| Notation | 35% | 180–320 | ties, voices, chords, layouts missing |
| Playback | 30% | 380–750 | no samples, effects or mixer; synth checked by tests only, not by ear |
| Files and exchange | 20% | 300–500 | [file-format-parity](file-format-parity.md) |
| UI/UX fidelity | 45% | 200–350 | [ui-parity](ui-parity.md); layout matches GP, depth does not |
| Tools | 0% | 120–200 | tuner, chords, scales, line in |
| Stability | 50% | 60–120 | never-crash lints on every crate and a command fuzz test; no field reports yet, no fuzzers on readers, no autosave |
| Performance | unknown | 30–60 | nothing measured; no budgets ([gaps G9](gaps.md#g9-performance-budgets)) |
| Platforms | 15% | 80–150 + human | macOS source builds only; no releases |

## Remaining effort and calibration

Hours are Opus 5.5 agent wall-clock hours for one agent working sequentially, including tests.
**Uncalibrated:** Guitario has no merged-PR history yet, so the per-item rates are borrowed from
another app's calibration (2–9 h per checklist item, 8–20 h per format or subsystem, from its 1,004
merged PRs). Re-calibrate after the first 100 PRs here. About 70% of the work parallelizes
(independent crates and command modules); the notation engine and playback are the serial parts.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-16 | minor | Bar/track/technique commands, tuplets, triplet feel: menus 117/172, breadth ~41% |
| 2026-10-15 | minor | Drums landed (G1 mostly closed): breadth ~37%, ready ~20%, alpha ~220–420 h |
| 2026-10-15 | major | First assessment: breadth ~33%, ready ~15%, pre-alpha; menu catalog and scorecard introduced |
