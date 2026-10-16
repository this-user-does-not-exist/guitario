# Guitario roadmap

**Stage: pre-alpha** · next: alpha, ~70–120 h away (print/PDF)

> **Last reviewed:** 2026-10-16 · **Last updated:** 2026-10-16 · **Change:** minor (ties, slurs and four voices; menus 125/172) · **Target:** Guitar Pro 8

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
| **Feature breadth** | **~47%** | partly measured: menus 125/172, [scorecard](docs/scorecard.md) |
| **Ready for real work** | **~25%** (20–30%) | estimated: weighted by dimension below |
| Mainstream user | ~32% (27–37%) | estimated |
| Essentials user | ~68% (60–76%) | estimated |
| Remaining to **alpha** | **~70–120 h** | estimated |
| Remaining to **beta** | **~950–1,600 h** | estimated |
| Remaining to **full parity** | **~2,000–3,550 h** | estimated; sampled playback dominates |

### Readiness by audience

| Audience | Ready % | Opus 5.5 agent-hours to ~95% | Work that dominates |
|---|---:|---:|---|
| Full target (ready for real work) | ~25% | 1,950–3,500 | Everything below, plus sampled playback, tools, PowerTab and TablEdit, releases |
| Mainstream user | ~32% | 950–1,700 | `.gp` export, print/PDF, realistic playback |
| Essentials user | ~68% | 100–220 | PDF export |

**Why pre-alpha.** Writing, arranging with drums, playing back and practising work end to end and
save and reopen, and files from other people open (`.gp`, `.gpx`, `.gp5`, `.gp4`, `.gp3`) and go
back as `.gp5`, but the [alpha gate](docs/roadmap.md#alpha-gate) fails: nothing can be printed or
exported as PDF.

## By dimension

| Dimension | % ready | Hours to full | Doc |
|---|---:|---:|---|
| Editing | 55% | 110–200 | [scorecard](docs/scorecard.md#editing) |
| Notation | 45% | 110–220 | [scorecard](docs/scorecard.md#notation-and-engraving) |
| Playback | 30% | 380–750 | [scorecard](docs/scorecard.md#playback) |
| Files and exchange | 35% | 220–380 | [file-format-parity](docs/file-format-parity.md) |
| UI/UX fidelity | 45% | 200–350 | [ui-parity](docs/ui-parity.md) |
| Tools | 0% | 120–200 | [gaps G7](docs/gaps.md#g7-tools) |
| Stability | 50% | 60–120 | [scorecard](docs/scorecard.md#reliability) |
| Performance | not measured | 30–60 | [gaps G9](docs/gaps.md#g9-performance-budgets) |
| Platforms | 15% | 80–150 + human | [scorecard](docs/scorecard.md#distribution) |

## Languages

English only. Guitar Pro ships in about a dozen languages ([gaps G13](docs/gaps.md#g13-languages)).

## Upcoming

| Rank | Milestone | Estimate |
|---|---|---:|
| 1 | Print and PDF through craft-pdf and craft-print | 60–100 h |
| 2 | `.gp` export and Guitar Pro import depth | 60–110 h |
| 3 | Notation depth: chord diagrams, lyrics, grace notes, layouts | 90–160 h |
| 4 | Performance budgets and a control channel | 70–140 h |
| 5 | Drum depth: step editor, articulations, sampled kit | 40–80 h |

Detail and the next releases: [docs/roadmap.md](docs/roadmap.md).
