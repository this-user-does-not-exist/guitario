# Guitario roadmap

**Stage: pre-alpha** · next: alpha, ~130–230 h away (print/PDF, ties and voices)

> **Last reviewed:** 2026-10-16 · **Last updated:** 2026-10-16 · **Change:** minor (Guitar Pro 3-6 import and `.gp5` export; chord names; menus 121/172) · **Target:** Guitar Pro 8

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
| **Feature breadth** | **~45%** | partly measured: menus 121/172, [scorecard](docs/scorecard.md) |
| **Ready for real work** | **~22%** (17–27%) | estimated: weighted by dimension below |
| Mainstream user | ~28% (23–33%) | estimated |
| Essentials user | ~63% (55–72%) | estimated |
| Remaining to **alpha** | **~130–230 h** | estimated |
| Remaining to **beta** | **~1,000–1,700 h** | estimated |
| Remaining to **full parity** | **~2,100–3,700 h** | estimated; sampled playback dominates |

### Readiness by audience

| Audience | Ready % | Opus 5.5 agent-hours to ~95% | Work that dominates |
|---|---:|---:|---|
| Full target (ready for real work) | ~22% | 2,000–3,600 | Everything below, plus sampled playback, tools, PowerTab and TablEdit, releases |
| Mainstream user | ~28% | 1,000–1,800 | `.gp` export, print/PDF, voices, ties, realistic playback |
| Essentials user | ~63% | 150–300 | PDF export, ties |

**Why pre-alpha.** Writing, arranging with drums, playing back and practising work end to end and
save and reopen, and files from other people open (`.gp`, `.gpx`, `.gp5`, `.gp4`, `.gp3`) and go
back as `.gp5`, but the [alpha gate](docs/roadmap.md#alpha-gate) fails: nothing can be printed or
exported as PDF, and notation has no ties or second voice.

## By dimension

| Dimension | % ready | Hours to full | Doc |
|---|---:|---:|---|
| Editing | 50% | 130–230 | [scorecard](docs/scorecard.md#editing) |
| Notation | 35% | 180–320 | [scorecard](docs/scorecard.md#notation-and-engraving) |
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
| 1 | Ties, slurs and two voices (also drums' hands and feet) | 60–110 h |
| 2 | Print and PDF through craft-pdf and craft-print | 60–100 h |
| 3 | `.gp` export and Guitar Pro import depth | 60–110 h |
| 4 | Performance budgets and a control channel | 70–140 h |
| 5 | Drum depth: step editor, articulations, sampled kit | 40–80 h |

Detail and the next releases: [docs/roadmap.md](docs/roadmap.md).
