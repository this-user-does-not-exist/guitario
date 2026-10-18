# Guitario vs Guitar Pro 8: target-app parity

> **Last reviewed:** 2026-10-17 · **Last updated:** 2026-10-18 · **Change:** minor (samples stream from disk: playback 70 %, readiness ~66 %; beta ~185–380 h) · **Target:** Guitar Pro 8

The authoritative answer to "how close is Guitario to Guitar Pro, and how much work is left".
The one-page summary is [`ROADMAP.md`](../ROADMAP.md); what each stage means and how to tell it
has been reached is [`roadmap.md`](roadmap.md); the ranked work list is [`gaps.md`](gaps.md); the
measured checklists are [`scorecard.md`](scorecard.md) (`cargo xtask scorecard`) and
[`parity-checklist.md`](parity-checklist.md) (`cargo xtask parity`). Detail by dimension:
[file formats](file-format-parity.md), [UI](ui-parity.md).

## Headline (2026-10-17, Guitario 0.1.0)

| Number | Value | Kind |
|---|---|---|
| **Stage** | **alpha** | measured: the [alpha gate](roadmap.md#alpha-gate) passes (two human checks open) |
| **Beta gate** | **0 of 8 workflows pass** | measured by running them: [beta gate](roadmap.md#beta-gate) |
| **Scorecard** | 57 done · 19 partial · 17 missing; **20 open for beta**, 16 for 1.0 | measured: `cargo xtask scorecard` |
| **Menu parity** | 172 of 180 items live (96 %) | measured: `cargo xtask parity` |
| **Feature breadth** (does each Guitar Pro feature exist?) | **~74 %** | partly measured, weights [below](#feature-breadth-74-weighted) |
| **Readiness** (how deep is each dimension, weighted?) | **~66 %** (range 59–71) | estimated per dimension, arithmetic [below](#readiness-by-dimension) |
| Remaining to **beta** | **~185–380 h** | sum of the beta column in [gaps](gaps.md#summary) |
| Remaining to **full parity** | **~890–1,760 h** | beta plus the 1.0 column; sampled playback and the long tail of notation dominate |

What changed in this review: the previous headline said "ready for real work ~29 %" and gave
audience percentages (essentials 76 %, mainstream 39 %) that could not be reproduced from the
dimension table (weighting those dimensions gives ~50 %), and "beta at ~75 % ready" had no
definition behind it. Stages are now defined by gates that are run, not read off a percentage;
the readiness number is kept only as a dimension average whose weights are written down. The
audiences map to stages: an *essentials user* needs alpha (reached), a *mainstream user* needs
beta, a *full* user needs 1.0 (definitions in [roadmap](roadmap.md#stages)).

## Target measured

- **Guitar Pro 8** (Arobas Music), the current major version; the format notes in the author's
  `Guitar Pro 7 File Format` folder cover files written by GP 7.0 to 8.1.3.
- **Not measured from the installed app.** Guitar Pro is not installed on the machine this was
  written on. The menu catalog (`crates/ui-egui/src/menu_catalog.rs`) and these feature lists
  come from Guitar Pro 8's documentation, its file format (the `.gp` notes and corpus findings),
  and, since 2026-10-17, a review of the Guitar Pro 8 manual's text and screenshots that produced
  307 issues (label `source: gp8-manual`). Re-measure against an installed copy when one is
  available: menu resources, preset tunings, shortcut defaults and sound banks (#154).
- Clean-room: behaviour and look only. No Guitar Pro code, sounds, fonts or stylesheets are used.

## Feature breadth: ~74% (weighted)

| Component | Weight | Ours / Guitar Pro | % | Kind |
|---|---:|---|---:|---|
| Menu commands | 30% | 172 / 180 catalog items live | 96% | measured (`cargo xtask parity`) |
| Notation and engraving | 15% | page, screen and horizontal layouts and the multitrack view; notation over tab, drum staff, effects lanes, endings, directions, chord names and diagrams, lyrics, ties, slurs, up to four voices, tremolo picking, trills, grace notes, fermatas, fingering, beaming and beam breaks, key changes and courtesy signatures, the page details of Guitar Pro 8's default stylesheet; no slash or numbered notation, grand staff, clef changes, simile, multirests or stylesheet | ~80% | estimated ([scorecard](scorecard.md#notation-and-engraving)) |
| Instruments and track types | 10% | fretted strings in any tuning (4–12 strings, presets for guitars, basses, banjo, mandolin and bowed strings) and a GM drum kit, chosen in an Add Track window; no keyboards, winds or MIDI instruments | ~50% | estimated |
| Playback | 15% | physical-model guitar and bass, synthesised drums, SFZ sampler with a sound bank, RSE or MIDI per track, interpretation settings, transport, loop, speed, metronome, count-in, live mixer, tempo, volume and pan automation, audio settings, WAV and FLAC, an effect chain per track (amp, cabinet, drive, dynamics, modulation, delay, reverb), the mixer's equalizer, master volume, pan and mastering, every articulation in the synth and a program per articulation from a bank, a sampler with release samples, filters, curves and loops; no pedalboard, MP3 or OGG export | ~67% | estimated ([scorecard](scorecard.md#playback)) |
| File formats (usage-weighted) | 15% | `.gp`, `.gpx`, `.gp5`, `.gp4`, `.gp3` import (checked on 76 real files), `.gp`, `.gp5`, MIDI, WAV, FLAC and ASCII export, PDF export, print, preview, page setup; chords, lyrics and fingering both ways; MIDI import, MusicXML import and export; no PowerTab, TablEdit or ASCII import, `.gpx` export or image export | ~70% | estimated ([file-format-parity](file-format-parity.md)) |
| Tools (tuner, chord engine, scales, line in) | 10% | chord diagram editor and namer, the Chord window, the Scale tool, the tuner; no line in, MIDI capture or command palette | ~60% | estimated |
| Ecosystem and automation | 5% | engine commands, CLI, scripts; no control channel or MCP | ~30% | estimated |
| **Weighted** | 100% | 0.30·96 + 0.15·80 + 0.10·50 + 0.15·67 + 0.15·70 + 0.10·60 + 0.05·30 | **~74%** (73.9%) | |

## Readiness by dimension

How deep each dimension is, not whether a feature exists. Weights say how much a dimension
matters to a mainstream user's week and are fixed here so the average is reproducible. Percentages
are estimates; the beta hours are the dimension's gaps in [`gaps.md`](gaps.md).

| Dimension | Weight | % ready | Beta h | After beta h | Gaps | Evidence |
|---|---:|---:|---:|---:|---|---|
| Editing | 20% | 70% | 15–30 | 95–190 | G6, G1 | [scorecard › Editing](scorecard.md#editing): bends and the whammy bar have curves and editors, Paste Special, All-Track Copy, Unset; no drum step grid (G1) |
| Notation | 15% | 74% | 6–22 | 150–300 | G4 | [scorecard › Notation](scorecard.md#notation-and-engraving): all done; voice picking in notation is left |
| Playback | 15% | 70% | 0 + human | 140–280 | G5 | sampler (release samples, filters, curves, loops, OGG) with the bank installed from the app and an instrument per track, mixer, automations, effect chains, equalizer, mastering and articulations (synth and bank) in; master automation; samples in 16 bits, long ones streamed from disk (25 MB instead of 500 for the perf score); no pedalboard; checked by tests, not by ear |
| Files and exchange | 10% | 70% | 10–20 | 100–200 | G3, G2 | [file-format-parity](file-format-parity.md) |
| UI/UX fidelity | 10% | 62% | 65–125 | 110–210 | G8, G13 | [ui-parity](ui-parity.md): workspace, palette, inspector, toolbar, global view and homepage match Guitar Pro 8; depth (several documents, preferences, stylesheet) does not |
| Tools | 5% | 65% | 0 + human | 80–150 | G7 | tuner, chord window, scales, line in |
| Stability | 10% | 65% | 15–35 | 0 | G11 | never-crash lints on every crate, a command fuzz test, a panic guard on every command, autosave and recovery, fuzzers on the Guitar Pro readers; no field reports yet, `.gt` reader not fuzzed |
| Performance | 5% | 70% | 15–30 | 0 | G9 | measured (`cargo xtask perf`, `perf/budgets.toml`): every stage within budget on a 400-bar, six-track score; not in CI, no memory or startup budgets |
| Platforms and automation | 10% | 35% | 60–120 | 30–60 | G12, G10 | CI green on macOS, Linux and Windows; app icon; packages for three platforms (`cargo xtask package`), file types declared, a release workflow that has not run yet, signing waiting on certificates; a file double-clicked in Finder does not open; no control channel or MCP |
| **Weighted** | 100% | **~66%** | **~185–380** | **~710–1,390** | | 0.20·70 + 0.15·74 + 0.15·70 + 0.10·70 + 0.10·62 + 0.05·65 + 0.10·65 + 0.05·70 + 0.10·35 = 65.6 |

Beta is not "75 % of this table": it is the beta gate. As a guide, the dimension average at beta
is about 75 % because the beta work lifts playback, editing, tools and platforms to the high
sixties or better while the 1.0 tail (stylesheet, Design Mode, translations) stays open.

## Calibration

Hours are Opus 5.5 agent wall-clock hours for one agent working sequentially, including tests.
The rates were first borrowed from another app (2–9 h per checklist item, 8–20 h per format or
subsystem, from its 1,004 merged PRs). The first measurement here, on 2026-10-17:

- **33 pull requests merged in 35 hours of calendar time** (2026-10-16 11:08 to 2026-10-17
  21:51 UTC), about one batch an hour, about 99,000 lines added and 26,000 removed.
- On 2026-10-15 the alpha gate was estimated at 220–420 h; it passed 2026-10-16.
- So the estimates are an **upper bound**, high by a factor that parallel agents explain only
  in part (about 70 % of the work parallelizes). Read the hour figures as relative sizes between
  gaps, and plan in batches: the beta gate is roughly 60 to 120 batches at the observed size.
- The figures stay in agent-hours because 33 PRs is too few to recalibrate (issue #194 asks
  for 100). The wall-clock rate does not apply to the human steps in the [roadmap](roadmap.md#current-focus):
  a listening pass, a real printer, signing accounts.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-18 | minor | Long samples stream from disk (25 MB instead of 500 MB for the perf score): playback 67% → 70%, readiness ~65% → ~66%; beta hours 200–415 → 185–380 |
| 2026-10-18 | minor | Packages for three platforms, file types declared, the release workflow: platforms 25% → 35%, readiness ~64% → ~65%; beta hours 260–530 → 200–415 |
| 2026-10-18 | minor | Master volume and pan automation, 16-bit samples with a memory budget: playback 65% → 67%; beta hours 270–550 → 260–530 |
| 2026-10-18 | minor | The sound bank installs from the app (Sound Bank window, first-run offer, `sound.installLibraries`), an instrument per track: playback 60% → 65%, readiness ~63% → ~64%; beta hours 325–655 → 270–550 |
| 2026-10-18 | minor | The sampler's depth (release samples, legato regions, filters, velocity curves, WAV loops and crossfades, OGG samples): playback 57% → 60%, breadth playback 65% → 67%; beta hours 340–685 → 325–655 |
| 2026-10-18 | minor | Articulations in the synth and the sound bank, sampled legato: playback 52% → 57%, readiness ~62% → ~63%; beta hours 355–715 → 340–685. Breadth ~66% → ~74%, mostly a correction: the Tools row still said there was no chord window, tuner or scale tool (8% → 60%) and the Files row no MIDI import or MusicXML (57% → 70%), all landed earlier on 2026-10-18; playback 60% → 65% |
| 2026-10-18 | minor | Effect chains (amp, cabinet, reverb, delay and seven more), the track equalizer, master volume, pan and mastering; Sound › Sound Settings… live: menus 172/180; playback 42% → 52%, breadth ~66%, readiness ~60% → ~62%; beta hours 405–810 → 355–715 |
| 2026-10-17 | minor | Unset, the pitch bends reach in notation: editing 68% → 70%; beta hours 520–1,025 → 515–1,010 |
| 2026-10-18 | minor | The tuner: menus 171/180; tools 50% → 65%, readiness ~60%; beta hours 425–850 → 405–810 |
| 2026-10-18 | minor | The Scale tool: menus 170/180; tools 30% → 50%, readiness ~59% → ~60%; beta hours 440–875 → 425–850 |
| 2026-10-18 | minor | The Chord window (Note › Chord…, a catalog item from GP8's menus): menus 169/180; tools 8% → 30%, readiness ~58% → ~59%; beta hours 460–910 → 440–875 |
| 2026-10-18 | minor | Guitar Pro 6 drum hits: files 68% → 70%, readiness ~57% → ~58%; beta hours 465–925 → 460–910 |
| 2026-10-18 | minor | MusicXML import and export: menus 168/179; files 60% → 68%; beta hours 500–990 → 465–925 |
| 2026-10-18 | minor | MIDI import (File › Import › MIDI…): menus 166/179; files 56% → 60%, readiness ~56% → ~57%; beta hours 515–1,010 → 500–990 |
| 2026-10-17 | minor | Paste rules, All-Track Copy and Cut (two catalog items from GP8's shortcut list), Paste Special, Check Bar Durations window: menus 165/179; editing 65% → 68%, readiness ~55% → ~56%; beta hours 530–1,045 → 520–1,025 |
| 2026-10-17 | minor | The whammy bar and vibrato with the bar (Effects › Whammy Bar…, palette): menus 162/177; editing 62% → 65%, readiness ~54% → ~55%; beta hours 540–1,060 → 530–1,045 |
| 2026-10-17 | minor | Bend curves and the Bend window (Effects › Bend…, B): menus 161/177; editing 60% → 62%; beta hours 560–1,090 → 540–1,060 |
| 2026-10-17 | major | Method fixed: breadth recomputed from stated weights (menus 160/177; ~61% to ~63%); readiness is the weighted dimension average (~54%, replacing the unreproducible ~29%) and the audience percentages are gone, replaced by stages with gates; hours re-summed from the gaps (beta ~540–1,060 h, full ~1,230–2,410 h, was ~770–1,310 and ~1,800–3,215); calibration measured (33 PRs in 35 h); the 307 manual-review issues counted in |
| 2026-10-16 | minor | FLAC export: menus 148/173 → 149/173. Breadth recomputed for the day's work: menus 81% → 86%, notation ~72% → ~78%, playback ~35% → ~50%, files ~56% → ~57%: 56.0% → 60.9% (~61%); readiness dimensions notation 64% → 68%, playback 30% → 38% |
| 2026-10-16 | minor | Live mixer, tempo, volume and pan automation, audio settings, Transpose dialogs: menus 141/173 → 148/173; performance measured for the first time (70%); panic guard and autosave: stability 50% → 60% |
| 2026-10-16 | minor | Chord list, Multivoice Editing, full-score printing, key changes, courtesy signatures; layouts and the multitrack view: breadth ~56% (54.7% → 56.0%), notation 61% → 64%, UI 45% → 50% |
| 2026-10-16 | minor | Beaming, fingering, fermatas, lyrics, chord diagrams, grace notes, tremolo picking and trills: breadth ~50% to ~55% (49.9% → 54.7%), notation 45% → 61% |
| 2026-10-16 | minor | Print preview and page setup, PDF export and printing: menus 130/173, breadth ~50%, files 45% → 54%; **alpha gate passes** |
| 2026-10-16 | minor | `.gp` export, ties, slurs and four voices, Guitar Pro 3-6 import and `.gp5` export, bar and technique commands: breadth ~41% → ~48%, ready ~22% → ~26% |
| 2026-10-15 | minor | Drums landed: breadth ~37%, ready ~20%, alpha ~220–420 h |
| 2026-10-15 | major | First assessment: breadth ~33%, ready ~15%, pre-alpha; menu catalog and scorecard introduced |
