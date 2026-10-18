# Guitario roadmap

**Stage: alpha** · next: beta, ~540–1,060 h away · then 1.0 (full parity), ~690–1,350 h further

> **Last reviewed:** 2026-10-17 · **Last updated:** 2026-10-17 · **Change:** major (stages now defined by gates, beta gate added, percentages recomputed from stated weights, hours re-summed from the gaps) · **Target:** Guitar Pro 8

Guitario's one-page summary of how close it is to Guitar Pro 8 at version 0.1.0. Detail:
[roadmap](docs/roadmap.md) (stages, gates, current focus), [target-app parity](docs/target-app-parity.md)
(methodology, weights, calibration), [gaps](docs/gaps.md) (the ranked work list),
[scorecard](docs/scorecard.md) and [menu checklist](docs/parity-checklist.md) (both generated and
measured), [changelog](docs/changelog.md) (what has landed), [architecture](docs/architecture.md).

Hours are Opus 5.5 agent wall-clock hours for one agent working sequentially. They are an
**upper bound**: 33 pull requests landed in 35 hours of calendar time, far faster than the
estimates implied (see [calibration](docs/target-app-parity.md#calibration)).

## Where we are

| | Value | Kind |
|---|---|---|
| **Stage** | **alpha** | measured: the [alpha gate](docs/roadmap.md#alpha-gate) passes |
| **Beta gate** | **0 of 8** workflows pass | measured: [beta gate](docs/roadmap.md#beta-gate) |
| Scorecard | 47 done, 22 partial, 24 missing; **30 open for beta**, 16 for 1.0 | measured |
| Menus | 161 of 177 live (91 %) | measured |
| Feature breadth | **~63 %** | partly measured: weights in [target-app parity](docs/target-app-parity.md#feature-breadth-63-weighted) |
| Readiness | **~54 %** (48–60) | estimated: weighted dimension average |
| Remaining to **beta** | **~540–1,060 h** | sum of the beta column in [gaps](docs/gaps.md#summary) |
| Remaining to **full parity** | **~1,230–2,410 h** | beta plus the 1.0 column |

## Stages

| Stage | Who it is for | Reached when |
|---|---|---|
| **Alpha** ✅ | essentials: enter a riff, hear it, save it, share it | the six-workflow [alpha gate](docs/roadmap.md#alpha-gate) passes (2026-10-16; a real printer and a `.gp` opened in Guitar Pro 8 are still to be checked by a person) |
| **Beta** | a mainstream user: replaces Guitar Pro for the week's work and installs it themselves | the eight-workflow [beta gate](docs/roadmap.md#beta-gate) passes, the scorecard shows 0 open for beta, CI is green on three platforms |
| **1.0** | full parity | every scorecard row done and every menu item live, eleven languages |

## The beta gate

| # | Workflow | Today | The big missing piece |
|---|---|---|---|
| B1 | Install and open on macOS, Windows, Linux | no | installers, signing, file associations, in-app sound bank |
| B2 | Hear a band: articulated guitar, bass, sampled drums through an amp | partial | articulations, amp, cabinet, reverb; a listening pass |
| B3 | Transcribe a solo: bends, whammy, step-grid drums, Paste Special | partial | whammy bar, Paste Special, step grid (bends are done) |
| B4 | Exchange: Guitar Pro 3-8, MIDI, MusicXML | partial | MusicXML, MIDI import, a `.gp` opened in Guitar Pro 8 |
| B5 | Practise with the tools | no | tuner, chord window, scale tool |
| B6 | Print properly | partial | headers and footers, selected bars, a real printer |
| B7 | Work for hours | partial | several documents, CI budgets, more fuzzing |
| B8 | Drive it by agent | no | control channel, MCP |

## By dimension

| Dimension | Weight | % ready | Beta h | Doc |
|---|---:|---:|---:|---|
| Editing | 20% | 62% | 40–80 | [scorecard](docs/scorecard.md#editing) |
| Notation | 15% | 74% | 6–22 | [scorecard](docs/scorecard.md#notation-and-engraving) |
| Playback | 15% | 42% | 150–300 | [scorecard](docs/scorecard.md#playback) |
| Files and exchange | 10% | 56% | 65–120 | [file-format-parity](docs/file-format-parity.md) |
| UI/UX fidelity | 10% | 62% | 75–140 | [ui-parity](docs/ui-parity.md) |
| Tools | 5% | 8% | 55–100 | [gaps G7](docs/gaps.md#g7-tools) |
| Stability | 10% | 65% | 15–35 | [scorecard](docs/scorecard.md#reliability) |
| Performance | 5% | 70% | 15–30 | [gaps G9](docs/gaps.md#g9-performance-budgets) |
| Platforms and automation | 10% | 25% | 120–230 | [scorecard](docs/scorecard.md#distribution) |

## Languages

English only. Guitar Pro ships in about a dozen languages; the localization framework is a beta
item, the eleven translations (human-reviewed) are 1.0 ([gaps G13](docs/gaps.md#g13-languages)).

## Upcoming

Ranked for the beta gate; the reasoning and the human steps are in [docs/roadmap.md](docs/roadmap.md#current-focus).

| Rank | Work | Gap | Beta hours |
|---|---|---|---:|
| 1 | Whammy bar, Paste Special (the bend editor landed 2026-10-17) | [G6](docs/gaps.md#g6-editing-depth) | 25–50 |
| 2 | MIDI and MusicXML import and export, Guitar Pro 6 drums | [G3](docs/gaps.md#g3-guitar-pro-exchange) | 55–100 |
| 3 | Chord window, scale tool, tuner | [G7](docs/gaps.md#g7-tools) | 55–100 |
| 4 | Sound: articulations, amp, cabinet, reverb, first-run sound bank | [G5](docs/gaps.md#g5-playback-realism) | 150–300 |
| 5 | Release workflow, installers, file associations | [G12](docs/gaps.md#g12-platforms-and-distribution) | 80–150 + human |
| 6 | Several documents, shortcuts audit, user book | [G8](docs/gaps.md#g8-ui-depth) | 60–110 |
| 7 | Drum step grid, headers and footers, voice picking | [G1](docs/gaps.md#g1-drums-depth), [G2](docs/gaps.md#g2-print-depth), [G4](docs/gaps.md#g4-notation-depth) | 30–70 |
| 8 | Control channel and MCP, fuzzing, CI budgets, localization framework | [G10](docs/gaps.md#g10-automation), [G11](docs/gaps.md#g11-reliability-depth), [G9](docs/gaps.md#g9-performance-budgets), [G13](docs/gaps.md#g13-languages) | 85–175 |
