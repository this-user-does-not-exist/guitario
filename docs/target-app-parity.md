# Guitario vs Guitar Pro 8: target-app parity

> **Last reviewed:** 2026-10-16 · **Last updated:** 2026-10-16 · **Change:** minor (beaming by the signature and sloped beams: notation 59% → 61%; breadth 54.4% → 54.7%, ~55%) · **Target:** Guitar Pro 8

The authoritative answer to "how close is Guitario to Guitar Pro, and how much work is left".
The one-page summary is [`ROADMAP.md`](../ROADMAP.md); the ranked work list is [`gaps.md`](gaps.md);
the measured checklists are [`scorecard.md`](scorecard.md) (`cargo xtask scorecard`) and
[`parity-checklist.md`](parity-checklist.md) (`cargo xtask parity`). Detail by dimension:
[file formats](file-format-parity.md), [UI](ui-parity.md).

## Headline (2026-10-16, Guitario 0.1.0)

| Number | Value | Kind |
|---|---|---|
| **Feature breadth** (does each Guitar Pro feature exist?) | **~55%** | partly measured (menus 136/173), weights below |
| **Ready for real work** (could a guitarist replace Guitar Pro with it?) | **~28%** (range 23–33%) | estimated, evidence below |
| **Mainstream user** (writes and learns tab every week) | **~38%** (33–43%) | estimated, [method](#readiness-by-audience) |
| **Essentials user** (enter a riff, hear it, save it) | **~76%** (68–84%) | estimated |
| **Stage** | **alpha** | the [alpha gate](roadmap.md#alpha-gate) passes; open human checks: a real printer, `.gp` in Guitar Pro 8 |
| Remaining to **alpha** | **reached** | |
| Remaining to **beta** (~75% ready) | **~800–1,360 Opus 5.5 agent-hours** | estimated |
| Remaining to **full parity** (~95–100%) | **~1,840–3,285 Opus 5.5 agent-hours** | estimated; sampled-instrument playback (RSE) dominates |

### Readiness by audience

| Audience | Ready % | Opus 5.5 agent-hours to ~95% | Work that dominates |
|---|---:|---:|---|
| Full target (ready for real work) | ~28% | 1,790–3,235 | Everything below, plus sampled playback, tools (tuner, chords, scales), PowerTab and TablEdit, distribution |
| Mainstream user (band arranger, teacher, learner) | ~38% | 800–1,460 | playback that sounds like an instrument, notation depth |
| Essentials user | ~76% | 50–120 | editing polish, print depth (headers and footers) |

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

## Feature breadth: ~55% (weighted)

| Component | Weight | Ours / Guitar Pro | % | Kind |
|---|---:|---|---:|---|
| Menu commands | 30% | 136 / 173 catalog items live | 79% | measured (`cargo xtask parity`) |
| Notation and engraving | 15% | page view, notation + tab, drum staff, effects lanes, alternate endings, directions, chord names and text, ties, slurs, up to four voices, tremolo picking slashes, trill marks, grace notes, chord diagrams, lyrics, fermatas, fingering, beaming by the signature and sloped beams; no layouts | ~68% | estimated ([scorecard](scorecard.md#notation-and-engraving)) |
| Instruments and track types | 10% | fretted strings in any tuning (4–12 strings, 12 presets) and a GM drum kit; no keys or other instruments | ~50% | estimated |
| Playback | 15% | physical-model guitar/bass, synthesised drums, transport, loop, speed, metronome, count-in; no samples, effects, mixer, automations | ~35% | estimated ([scorecard](scorecard.md#playback)) |
| File formats (usage-weighted) | 15% | `.gp`, `.gpx`, `.gp5`, `.gp4`, `.gp3` import (checked on 76 real files), `.gp`/`.gp5`/MIDI/WAV/ASCII export (`.gp` not yet opened in Guitar Pro itself), PDF export, print preview, page setup and printing (not yet tried on a real printer); chords and lyrics both ways in every Guitar Pro format that has them; no MusicXML export, no MIDI/MusicXML/PowerTab import | ~56% | estimated ([file-format-parity](file-format-parity.md)) |
| Tools (tuner, chord engine, scales, line in) | 10% | chord diagram editor and a chord namer (Tools › Chord Diagrams…); no chord dictionary, tuner, scales or line in | ~8% | estimated |
| Ecosystem and automation | 5% | engine commands, CLI, scripts; no control channel or MCP | ~30% | estimated |
| **Weighted** | 100% | | **~55%** | |

## Ready for real work, by dimension

| Dimension | % ready | Hours to full | Evidence |
|---|---:|---:|---|
| Editing | 55% | 110–200 | [scorecard › Editing](scorecard.md#editing) |
| Notation | 61% | 55–130 | layouts, multivoice editing, sloped beams missing |
| Playback | 30% | 380–750 | no samples, effects or mixer; synth checked by tests only, not by ear |
| Files and exchange | 56% | 130–250 | [file-format-parity](file-format-parity.md) |
| UI/UX fidelity | 45% | 200–350 | [ui-parity](ui-parity.md); layout matches GP, depth does not |
| Tools | 5% | 110–185 | tuner, chord dictionary, scales, line in |
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
| 2026-10-16 | minor | Beaming by the time signature (and per bar, kept in `.gp5`) and sloped beams: breadth ~55% (54.4% → 54.7%), notation 59% → 61%, ready ~28%, mainstream ~38% (notation depth for the batch: grace notes, chord diagrams, lyrics, fermatas, fingering, beams); beta ~800–1,360 h |
| 2026-10-16 | minor | Fingering for both hands (Note › Fingering…, notation, Guitar Pro 3-8 both ways): menus 136/173, breadth ~54% (54.1% → 54.4%), notation 58% → 59% |
| 2026-10-16 | minor | Fermatas (drawn, held in playback and MIDI, GPIF both ways): breadth ~54% (53.8% → 54.1%), notation 56% → 58%; beta ~810–1,375 h |
| 2026-10-16 | minor | Lyrics (five lines per track, Track › Lyrics…, drawn under the tab, Guitar Pro 4-8 both ways): menus 135/173, breadth ~54% (52.9% → 53.8%), notation 53% → 56%, files 55% → 56%; beta ~815–1,385 h |
| 2026-10-16 | minor | Chord diagrams drawn, edited (Tools › Chord Diagrams…) and named from their notes, and kept in `.gp`: menus 134/173, breadth ~53% (51.1% → 52.9%), notation 50% → 53%, tools 0% → 5%; beta ~830–1,410 h |
| 2026-10-16 | minor | Grace notes (model, Note › Grace Note…, playback, engraving, Guitar Pro 3-8 both ways): menus 133/173, breadth ~51% (50.5% → 51.1%), notation 47% → 50%; beta ~850–1,440 h |
| 2026-10-16 | minor | Tremolo picking and trills: menus 132/173, breadth still ~50% (49.9% → 50.5%), notation 45% → 47%; beta ~860–1,455 h |
| 2026-10-16 | minor | Print preview and page setup: menus 130/173, breadth ~50%, files 52% → 54%; beta ~870–1,470 h |
| 2026-10-16 | minor | PDF export and printing: menus 128/172, breadth ~49%, ready ~27%, files 45% → 52%; alpha gate passes; beta ~880–1,490 h (G2's 60–100 h done, 20–40 h of print depth added) |
| 2026-10-16 | minor | `.gp` export: menus 126/172, breadth ~48%, ready ~26% |
| 2026-10-16 | minor | Ties, slurs and four voices: menus 125/172, breadth ~47%, ready ~25%, alpha ~70–120 h |
| 2026-10-16 | minor | Guitar Pro 3-6 import, `.gp5` export, chord names: menus 121/172, breadth ~45%, ready ~22%, alpha ~130–230 h |
| 2026-10-16 | minor | Bar/track/technique commands, tuplets, triplet feel: menus 117/172, breadth ~41% |
| 2026-10-15 | minor | Drums landed (G1 mostly closed): breadth ~37%, ready ~20%, alpha ~220–420 h |
| 2026-10-15 | major | First assessment: breadth ~33%, ready ~15%, pre-alpha; menu catalog and scorecard introduced |
