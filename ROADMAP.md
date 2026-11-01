# Guitario roadmap

**Stage: alpha** · next: beta, ~99–203 h away · then 1.0 (full parity), ~705–1,380 h further

> **Last reviewed:** 2026-10-17 · **Last updated:** 2026-11-01 · **Change:** minor (saved track visibility and the focus knob: UI-4 done; beta ~99–203 h) · **Target:** Guitar Pro 8

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
| **Beta gate** | **2 of 8** workflows pass | measured: [beta gate](docs/roadmap.md#beta-gate) |
| Scorecard | 63 done, 17 partial, 14 missing; **14 open for beta**, 17 for 1.0 | measured |
| Menus | 174 of 181 live (96 %) | measured |
| Feature breadth | **~76 %** | partly measured: weights in [target-app parity](docs/target-app-parity.md#feature-breadth-76-weighted) |
| Readiness | **~69 %** (62–74) | estimated: weighted dimension average |
| Remaining to **beta** | **~99–203 h** | sum of the beta column in [gaps](docs/gaps.md#summary) |
| Remaining to **full parity** | **~804–1,583 h** | beta plus the 1.0 column |

## Stages

| Stage | Who it is for | Reached when |
|---|---|---|
| **Alpha** ✅ | essentials: enter a riff, hear it, save it, share it | the six-workflow [alpha gate](docs/roadmap.md#alpha-gate) passes (2026-10-16; a real printer and a `.gp` opened in Guitar Pro 8 are still to be checked by a person) |
| **Beta** | a mainstream user: replaces Guitar Pro for the week's work and installs it themselves | the eight-workflow [beta gate](docs/roadmap.md#beta-gate) passes, the scorecard shows 0 open for beta, CI is green on three platforms |
| **1.0** | full parity | every scorecard row done and every menu item live, eleven languages |

## The beta gate

| # | Workflow | Today | The big missing piece |
|---|---|---|---|
| B1 | Install and open on macOS, Windows, Linux | no | signing, opening a double-clicked file on macOS (a decision), a first release run (packages, file types and the sound bank install are done) |
| B2 | Hear a band: articulated guitar, bass, sampled drums through an amp | partial | a listening pass (human); articulations, the sampler's depth, streaming, amp, cabinet, effects and mastering are done |
| B3 | Transcribe a solo: bends, whammy, step-grid drums, Paste Special | yes | nothing (the step grid landed 2026-10-31) |
| B4 | Exchange: Guitar Pro 3-8, MIDI, MusicXML | partial | a `.gp` opened in Guitar Pro 8 (human; everything else is done) |
| B5 | Practise with the tools | partial | a tuning check with a real guitar (human); tuner, chord window and scale tool are done |
| B6 | Print properly | partial | a real printer (human); headers and footers, selected bars and twelve papers are done |
| B7 | Work for hours | partial | CI budgets, more fuzzing (several documents landed) |
| B8 | Drive it by agent | yes | nothing (the control channel, screenshots, MCP and ten agent tasks landed 2026-10-31; [control](docs/control.md)) |

## By dimension

| Dimension | Weight | % ready | Beta h | Doc |
|---|---:|---:|---:|---|
| Editing | 20% | 74% | 0 | [scorecard](docs/scorecard.md#editing) |
| Notation | 15% | 77% | 0 | [scorecard](docs/scorecard.md#notation-and-engraving) |
| Playback | 15% | 72% | 0 + human | [scorecard](docs/scorecard.md#playback) |
| Files and exchange | 10% | 72% | 0 + human | [file-format-parity](docs/file-format-parity.md) |
| UI/UX fidelity | 10% | 65% | 50–100 | [ui-parity](docs/ui-parity.md) |
| Tools | 5% | 65% | 0 + human | [gaps G7](docs/gaps.md#g7-tools) |
| Stability | 10% | 65% | 15–35 | [scorecard](docs/scorecard.md#reliability) |
| Performance | 5% | 70% | 15–30 | [gaps G9](docs/gaps.md#g9-performance-budgets) |
| Platforms and automation | 10% | 50% | 20–40 | [scorecard](docs/scorecard.md#distribution) |

## Languages

English only. Guitar Pro ships in about a dozen languages; the localization framework is a beta
item, the eleven translations (human-reviewed) are 1.0 ([gaps G13](docs/gaps.md#g13-languages)).

## Upcoming

Ranked for the beta gate; the reasoning and the human steps are in [docs/roadmap.md](docs/roadmap.md#current-focus).

| Rank | Work | Gap | Beta hours |
|---|---|---|---:|
| 1 | First release run, macOS file opening, signing | [G12](docs/gaps.md#g12-platforms-and-distribution) | 20–40 + human |
| 2 | Shortcuts audit, user book, click-to-edit | [G8](docs/gaps.md#g8-ui-depth) | 35–70 |
| 3 | Fuzzing, CI budgets, localization framework | [G11](docs/gaps.md#g11-reliability-depth), [G9](docs/gaps.md#g9-performance-budgets), [G13](docs/gaps.md#g13-languages) | 45–95 |
