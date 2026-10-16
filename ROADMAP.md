# Guitario roadmap

**Stage: pre-alpha** · next: alpha, ~220–420 h away (Guitar Pro exchange, print/PDF, ties and voices)

> **Last reviewed:** 2026-10-16 · **Last updated:** 2026-10-16 · **Change:** minor (menus 94 → 117: bar, track and technique commands, tuplets, triplet feel) · **Target:** Guitar Pro 8

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
| **Feature breadth** | **~41%** | partly measured: menus 117/172, [scorecard](docs/scorecard.md) |
| **Ready for real work** | **~20%** (15–25%) | estimated: weighted by dimension below |
| Mainstream user | ~25% (20–30%) | estimated |
| Essentials user | ~60% (50–70%) | estimated |
| Remaining to **alpha** | **~220–420 h** | estimated |
| Remaining to **beta** | **~1,100–1,800 h** | estimated |
| Remaining to **full parity** | **~2,200–3,800 h** | estimated; sampled playback dominates |

### Readiness by audience

| Audience | Ready % | Opus 5.5 agent-hours to ~95% | Work that dominates |
|---|---:|---:|---|
| Full target (ready for real work) | ~20% | 2,100–3,700 | Everything below, plus sampled playback, tools, every legacy format, releases |
| Mainstream user | ~25% | 1,100–1,900 | GP exchange both ways, print/PDF, voices, ties, realistic playback |
| Essentials user | ~60% | 200–380 | PDF export, ties, `.gp5` import |

**Why pre-alpha.** Writing, arranging with drums, playing back and practising work end to end and
save and reopen, but the [alpha gate](docs/roadmap.md#alpha-gate) fails: files from other people are
only partly read (`.gp` 7/8, no `.gp5`), nothing can be printed or exported as PDF or `.gp`, and
notation has no ties or second voice.

## By dimension

| Dimension | % ready | Hours to full | Doc |
|---|---:|---:|---|
| Editing | 50% | 130–230 | [scorecard](docs/scorecard.md#editing) |
| Notation | 35% | 180–320 | [scorecard](docs/scorecard.md#notation-and-engraving) |
| Playback | 30% | 380–750 | [scorecard](docs/scorecard.md#playback) |
| Files and exchange | 20% | 300–500 | [file-format-parity](docs/file-format-parity.md) |
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
| 1 | Guitar Pro exchange: `.gp5`, `.gpx`, import depth, then `.gp` export | 150–250 h |
| 2 | Print and PDF through craft-pdf and craft-print | 60–100 h |
| 3 | Ties, slurs and two voices (also drums' hands and feet) | 60–110 h |
| 4 | Performance budgets and a control channel | 70–140 h |
| 5 | Drum depth: step editor, articulations, sampled kit | 40–80 h |

Detail and the next releases: [docs/roadmap.md](docs/roadmap.md).
