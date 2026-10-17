# Guitario roadmap

**Stage: alpha** · next: beta, ~770–1,310 h away

> **Last reviewed:** 2026-10-16 · **Last updated:** 2026-10-16 · **Change:** minor (chord list, Multivoice Editing, full-score printing, key changes, courtesy signatures, transpose dialogs, panic guard, autosave and crash recovery, performance budgets, live mixer, tempo, volume and pan automation, audio settings, FLAC export; menus 149/173) · **Target:** Guitar Pro 8

Guitario's one-page summary of how close it is to Guitar Pro 8 at version 0.1.0. Detail:
[target-app parity](docs/target-app-parity.md) (methodology, weights, evidence), [gaps](docs/gaps.md)
(the ranked work list), [roadmap](docs/roadmap.md) (milestones, current focus),
[scorecard](docs/scorecard.md) and [menu checklist](docs/parity-checklist.md) (both generated and
measured), [architecture](docs/architecture.md).

Hours are Opus 5.5 agent wall-clock hours for one agent working sequentially. They are
**uncalibrated**: Guitario has no PR history yet, so the rates come from another app's
(see [calibration](docs/target-app-parity.md#remaining-effort-and-calibration)). About 70% of the
work parallelizes.

## Headline numbers

| | Value | Kind |
|---|---|---|
| **Feature breadth** | **~61%** | partly measured: menus 149/173, [scorecard](docs/scorecard.md) |
| **Ready for real work** | **~29%** (24–34%) | estimated: weighted by dimension below |
| Mainstream user | ~39% (34–44%) | estimated |
| Essentials user | ~76% (68–84%) | estimated |
| Remaining to **alpha** | **reached** | the gate passes; two human checks open |
| Remaining to **beta** | **~770–1,310 h** | estimated |
| Remaining to **full parity** | **~1,800–3,215 h** | estimated; sampled playback dominates |

### Readiness by audience

| Audience | Ready % | Opus 5.5 agent-hours to ~95% | Work that dominates |
|---|---:|---:|---|
| Full target (ready for real work) | ~29% | 1,750–3,165 | Everything below, plus sampled playback, tools, PowerTab and TablEdit, releases |
| Mainstream user | ~39% | 770–1,410 | realistic playback, notation depth |
| Essentials user | ~76% | 50–120 | editing polish, print depth |

**Why alpha.** Writing, arranging with drums, playing back and practising work end to end and
save and reopen, files from other people open (`.gp`, `.gpx`, `.gp5`, `.gp4`, `.gp3`) and go back
as `.gp` or `.gp5`, and a score prints or exports as a PDF, so the [alpha gate](docs/roadmap.md#alpha-gate)
passes. Two human checks are open: a page on a real printer, and a `.gp` export opened in Guitar
Pro 8. Beta needs sampled sound, notation depth, tools and releases on three platforms.

## By dimension

| Dimension | % ready | Hours to full | Doc |
|---|---:|---:|---|
| Editing | 55% | 110–200 | [scorecard](docs/scorecard.md#editing) |
| Notation | 68% | 30–80 | [scorecard](docs/scorecard.md#notation-and-engraving) |
| Playback | 38% | 320–650 | [scorecard](docs/scorecard.md#playback) |
| Files and exchange | 56% | 130–250 | [file-format-parity](docs/file-format-parity.md) |
| UI/UX fidelity | 50% | 180–320 | [ui-parity](docs/ui-parity.md) |
| Tools | 5% | 110–185 | [gaps G7](docs/gaps.md#g7-tools) |
| Stability | 60% | 45–90 | [scorecard](docs/scorecard.md#reliability) |
| Performance | 70% | 15–30 | [gaps G9](docs/gaps.md#g9-performance-budgets) |
| Platforms | 15% | 80–150 + human | [scorecard](docs/scorecard.md#distribution) |

## Languages

English only. Guitar Pro ships in about a dozen languages ([gaps G13](docs/gaps.md#g13-languages)).

## Upcoming

| Rank | Milestone | Estimate |
|---|---|---:|
| 1 | Notation depth: user-placed slurs, per-beat beam breaks, picking voices in notation | 8–28 h |
| 2 | Guitar Pro import depth, MIDI and MusicXML import, a human check of `.gp` exports | 40–80 h |
| 3 | Print depth: headers and footers, layouts, selected bars, a real-printer check | 10–20 h |
| 4 | Performance budgets and a control channel | 70–140 h |
| 5 | Drum depth: step editor, articulations, sampled kit | 40–80 h |

Detail and the next releases: [docs/roadmap.md](docs/roadmap.md).
