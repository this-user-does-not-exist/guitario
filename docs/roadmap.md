# Roadmap

> **Last reviewed:** 2026-10-17 · **Last updated:** 2026-10-18 · **Change:** minor (the tuner landed: exchange and tools have only human checks left; focus item 1 is sound) · **Target:** Guitar Pro 8

Forward-looking plan: what each stage means, how to tell a stage has been reached, and what to
build next. Where we stand is in [`target-app-parity.md`](target-app-parity.md) (headline numbers,
by dimension); the ranked work list is [`gaps.md`](gaps.md); the one-page summary is
[`ROADMAP.md`](../ROADMAP.md). Measured: [`scorecard.md`](scorecard.md) (`cargo xtask scorecard`)
and [`parity-checklist.md`](parity-checklist.md) (`cargo xtask parity`: menu wiring, which says
nothing about behaviour).

Status legend: ✅ done · 🟡 in progress · ⬜ not started. Hours are Opus 5.5 agent wall-clock hours
for one agent working sequentially, and an upper bound (see
[calibration](target-app-parity.md#calibration)).

**Stage: alpha.** The alpha gate passes. The beta gate below passes 0 of 8 workflows end to end
today; ~405–810 h of work are left, ordered under [Current focus](#current-focus).

## Stages

A stage is a promise about who can use Guitario, checked by running real workflows, not by a
percentage. The percentages in the other docs say how far along we are; the gates say whether a
stage has been reached.

| Stage | Who it is for | How we know | GitHub milestone |
|---|---|---|---|
| **Alpha** (reached 2026-10-16) | us and brave testers: the essentials work end to end | the [alpha gate](#alpha-gate): six workflows | Alpha |
| **Beta** | a mainstream Guitar Pro user who is willing to switch for everyday work, and installs it themselves | the [beta gate](#beta-gate): eight workflows, the beta rows of the scorecard done, installers on three platforms | Beta |
| **1.0** | full parity with Guitar Pro 8 | the [1.0 gate](#10-gate): every scorecard row done, every menu item live, eleven languages | 1.0 Guitar Pro parity |

A *mainstream user* writes and learns tab every week: enters and edits tab, reads standard
notation, plays back with drums, loops and slows passages, opens files from others (mostly `.gp5`
and `.gp`), prints or exports PDF, bends and slides, and occasionally uses the tuner and chord
tools. An *essentials user* only enters a riff, hears it and saves it (alpha). A *full* user
also needs everything else in Guitar Pro 8.

## Alpha gate

The core workflows a typical Guitar Pro user runs every week, checked end to end on the main
platform (macOS, 0.1.0), including saving and reopening the work. Any **no**, or a **partial**
that blocks the workflow, keeps Guitario pre-alpha.

| Core workflow | Works end to end? | Evidence |
|---|---|---|
| Transcribe a riff: enter notes, rhythm and techniques, play it back, save and reopen | yes | keyboard entry, palette, playback, `.gt` round trip (`crates/engine/tests`) |
| Practise: loop a passage, slow it down, metronome and count-in | yes | selection loop, speed 25–200 %, metronome, count-in (`crates/playback` tests) |
| Arrange a band song: guitars, bass and drums, sections, repeats | yes | drum tracks, pad, notation and playback; 12 band demos with drums (`crates/engine/tests/demos.rs`) |
| Open a tab someone sent (`.gp5` or `.gp`), play it, edit it | yes | `.gp`, `.gpx`, `.gp5`, `.gp4`, `.gp3` through File › Open; 76 real files import; every voice is kept, drawn and played |
| Share: print or export PDF, or send a `.gp` back | yes | File › Export › PDF… writes vector pages for every demo; File › Print… sends them through craft-print; `.gp` and `.gp5` go back to Guitar Pro users (read back by us, not yet opened in Guitar Pro itself) |
| Read and write standard notation with ties and two voices | yes | up to four voices, ties across bars, slurs; `demos/15-two-voices.gt` |

**Result: passes.** Two human checks are open and the pass does not hide them: a page on a real
printer, and a `.gp` export opened in Guitar Pro 8. Both are also beta work (B4, B6).

## Beta gate

Beta is reached when a mainstream user can install Guitario, replace Guitar Pro for the week's
work, and nothing they do every week is missing, crashes or sounds wrong. Each workflow below is
run end to end like the alpha gate's; a **no**, or a **partial** that blocks it, keeps the stage
at alpha. Beyond the workflows, three bars must hold: `cargo xtask ci` is green on macOS, Linux
and Windows; the scorecard shows **0 open for beta** (it counts them); and every menu item is
live except the ones the scorecard stages for 1.0.

| # | Beta workflow | Today | What is missing | Gap |
|---|---|---|---|---|
| B1 | **Install and open.** Download a signed build for macOS, Windows or Linux; double-click a `.gp` or `.gt` and it opens; the sound bank installs itself on first run | no | no installers, signing, file associations or in-app sound bank install; only macOS has been run by hand | [G12](gaps.md#g12-platforms-and-distribution), [G5](gaps.md#g5-playback-realism) |
| B2 | **Hear a band.** Guitar with palm mutes, harmonics and slides, bass, and sampled drums, through an amp and room, and a guitarist accepts how it sounds | partial | sampled instruments play, but articulations, legato, amp, cabinet, reverb and delay are not built; nobody has listened on speakers | [G5](gaps.md#g5-playback-realism) |
| B3 | **Transcribe a solo.** Bends with their curves, whammy dives, grace notes, tuplets, a drum groove on a step grid, Paste Special | partial | the drum step grid (bend curves, the whammy bar and Paste Special landed 2026-10-17) | [G6](gaps.md#g6-editing-depth), [G1](gaps.md#g1-drums-depth) |
| B4 | **Exchange.** Open any Guitar Pro 3 to 8 file (at least 95 % of a real corpus without warnings); a `.gp` export opens in Guitar Pro 8; MIDI and MusicXML go in and out | partial | the `.gp` check in Guitar Pro 8 (human); MIDI import, MusicXML and Guitar Pro 6 drums landed 2026-10-18 | [G3](gaps.md#g3-guitar-pro-exchange) |
| B5 | **Practise with the tools.** Tune the guitar, look up a chord's voicings, show a scale on the fretboard | partial | a tuning check with a real guitar (human); the tuner, chord window and scale tool are done | [G7](gaps.md#g7-tools) |
| B6 | **Print properly.** Headers and footers, selected bars, other paper; a page leaves a real printer on all three platforms | partial | headers and footers, selected bars, the real-printer check (human) | [G2](gaps.md#g2-print-depth) |
| B7 | **Work for hours.** Several scores open, a 1,000-bar score inside the performance budgets in CI, autosave and recovery, every reader fuzzed | partial | several documents, CI budgets, memory and startup budgets, `.gt` and decoder fuzzing, a guard around painting | [G8](gaps.md#g8-ui-depth), [G9](gaps.md#g9-performance-budgets), [G11](gaps.md#g11-reliability-depth) |
| B8 | **Drive it by agent.** An agent completes ten scripted edit tasks over MCP and takes screenshots of the running app | no | control channel, MCP server | [G10](gaps.md#g10-automation) |

**Result: fails (0 of 8).** Closest: B4, B6 and B7 (partial with a few named pieces); farthest:
B1, B5 and B8 (nothing built) and B2 (the biggest piece).

## 1.0 gate

Full parity with Guitar Pro 8: every [scorecard](scorecard.md) row done; every item of the
[menu checklist](parity-checklist.md) live; the corpus of real files imports at 98 % without
warnings; PowerTab and TablEdit files open; the Stylesheet window, Design Mode, slash and numbered
notation, the audio track, the pedalboard and mastering are in; the app is translated into the
eleven languages Guitar Pro ships (human-reviewed); a human has compared the demo suite with
Guitar Pro 8 renders and a guitarist has compared the sound. ~690–1,350 h after beta.

## Milestones

Engineering milestones, each with a definition of done. The GitHub milestones Alpha, Beta and
1.0 group the same work by stage; M-numbers say what to build, stages say when it counts.

| M | Status | Where we are | Stage |
|---|---|---|---|
| M0 Skeleton | ✅ | workspace, xtask (layers / wasm / ci / stats / parity / scorecard / perf / sounds / version), CI on three platforms, never-crash lints | alpha |
| M1 Model + engine | ✅ | theory, score (validation, timeline), 117 commands with undo, CLI, `.gt` with atomic saves | alpha |
| M2 Tab editor | 🟡 | keyboard entry, Guitar Pro shortcuts, palette, selection and clipboard, four voices, bend curves and the whammy bar with their editors, Paste Special and All-Track Copy | beta |
| M3 Notation | 🟡 | page, screen and horizontal layouts, multitrack, everything the model holds; slash/numbered notation, grand staff and the stylesheet are missing | beta (rest 1.0) |
| M4 Playback | 🟡 | physical-model synth, SFZ sampler and sound bank, drums, mixer, automations, WAV and FLAC; no articulations, amp, cabinet or effects | beta |
| M5 Guitar Pro exchange | 🟡 | import of `.gp`, `.gpx`, `.gp5`, `.gp4`, `.gp3`; `.gp` and `.gp5` export; a human check of `.gp` exports in Guitar Pro 8 remains | beta |
| M6 Drums | 🟡 | kit tracks, pad, notation, GM playback, `.gp` drum import, two voices; step editor and articulations missing | beta |
| M7 Print, PDF, MusicXML | 🟡 | PDF export, printing, print preview, page setup; no MusicXML, headers and footers | beta |
| M8 Tools | 🟡 | chord diagram editor and namer; no tuner, chord window or scale tool | beta |
| M9 Automation | ⬜ | control channel, MCP | beta |
| M10 Distribution | 🟡 | icon in every format, CI green on three platforms; no installers, signing, file associations or release workflow | beta |

## Current focus

Ranked for the beta gate; each item links to its gap. Take the top one nobody is working on, or the
issue assigned to you. Work in **batches**: one pull request, one slice of a gap, with tests, docs
and a snapshot where it is visible. The order puts what an agent can fully verify first and
what needs a human ear or signing material last, so the human steps overlap with other work.

1. **Sound** ([G5](gaps.md#g5-playback-realism)): articulation mapping and legato, then amp,
   cabinet, reverb and delay, then the first-run sound bank install. A listening pass closes it.
   150–300 h.
2. **Release workflow and packages** ([G12](gaps.md#g12-platforms-and-distribution)): tag to
   artifacts for three platforms, an app bundle with file associations; signing waits on accounts.
   80–150 h + human.
3. **Several documents, shortcuts audit, user book** ([G8](gaps.md#g8-ui-depth)). 60–110 h.
4. **Drum step grid and articulations** ([G1](gaps.md#g1-drums-depth)); **headers, footers,
   selected bars** ([G2](gaps.md#g2-print-depth)); **voice picking** ([G4](gaps.md#g4-notation-depth)).
   30–70 h.
5. **Control channel and MCP** ([G10](gaps.md#g10-automation)); **fuzzing, CI budgets, layout
   cache** ([G11](gaps.md#g11-reliability-depth), [G9](gaps.md#g9-performance-budgets)); **the
   localization framework** ([G13](gaps.md#g13-languages)). 85–175 h.

Human steps, to schedule early because they wait on a person: tuning a real guitar with the tuner, a listening pass (#115, #133), a
real-printer page on three platforms, `.gp` exports opened in Guitar Pro 8 (#19), signing
accounts (#176, #177), and re-measuring the menus and shortcuts from an installed Guitar Pro 8
(#154).

## Releases

| Release | Scope | Status |
|---|---|---|
| 0.1 | alpha: write, arrange with drums, play, open and send Guitar Pro files, print and export PDF | ✅ 2026-10-16 |
| 0.2 "the craft" | focus items 1 to 3: the editor and exchange gaps closed, tools | next |
| 0.3 "the sound" | focus item 4: articulated, amped, roomy playback | |
| 0.4 "installable" | focus items 5 and 6: installers, several documents, the book | |
| **Beta** | the beta gate passes | |
| 1.0 | the 1.0 gate passes | |

## Milestone definitions

Each milestone has a **definition of done (DoD)** and must leave `main` green: `cargo xtask ci`
and `cargo xtask wasm`.

| M | Name | Scope (key items) | DoD / acceptance |
|---|---|---|---|
| **M0** | Skeleton | Workspace, crate stubs, lints, xtask, CI | `cargo xtask ci` green; layering enforced |
| **M1** | Model + engine | Score model, command registry with undo, CLI, native format | Every edit runs headless; `panic_hunt` green |
| **M2** | Tab editor | Entry, durations, techniques, selection, clipboard, voices 1–4, bend editor | A guitarist can transcribe a two-voice part without the mouse |
| **M3** | Notation | Engraving of everything the model holds, layouts, multitrack | Offscreen renders of the demo suite reviewed against Guitar Pro |
| **M4** | Playback | Synth, samples with articulations, amp and effects, mixer, automations, export | The demo suite plays in time; budgets met (G9); a guitarist accepts the sound (human) |
| **M5** | Guitar Pro exchange | `.gp`/`.gpx`/`.gp5`/`.gp4`/`.gp3` import, `.gp` export | ≥ 95% of a real-file corpus imports without warnings; `.gp` exports open in Guitar Pro (human check) |
| **M6** | Drums | Kit tracks, percussion notation and step editor, GM playback, import | A metal demo with drums plays and round-trips through `.gp` |
| **M7** | Print, PDF, MusicXML | craft-pdf export, craft-print, MusicXML both ways | PDF of every demo matches the page view |
| **M8** | Tools | Tuner, chord window and diagrams, scales | Tuner within ±2 cents on test tones |
| **M9** | Automation | JSON control channel, MCP server | An agent completes 10 scripted edit tasks over MCP |
| **M10** | Distribution | Signed releases on macOS, Windows, Linux; icon; file associations | Install and open a `.gp` on each platform |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-18 | minor | The tuner landed: G3 and G7 have only human checks left; focus item 1 is sound; beta ~405–810 h |
| 2026-10-18 | minor | The Scale tool landed; focus item 1 is the tuner; beta ~425–850 h |
| 2026-10-18 | minor | The Chord window landed; focus item 1 is the scale tool and tuner; beta ~440–875 h |
| 2026-10-18 | minor | Guitar Pro 6 drum hits landed: G3 has only the human check left; focus item 1 is the tools; beta ~460–910 h |
| 2026-10-18 | minor | MusicXML import and export landed: focus item 1 is Guitar Pro 6 drums; beta ~465–925 h |
| 2026-10-18 | minor | MIDI import landed: focus item 1 is MusicXML and Guitar Pro 6 drums; beta ~500–990 h |
| 2026-10-17 | minor | Unset and bend pitches in notation landed: editing has no beta work left, focus item 1 is MIDI and MusicXML; beta ~515–1,010 h |
| 2026-10-17 | minor | Paste Special, All-Track Copy/Cut and the Check Bar Durations window landed: focus item 1 is the last beta editing items; beta ~520–1,025 h |
| 2026-10-17 | minor | The whammy bar landed: focus item 1 is now Paste Special; beta ~530–1,045 h |
| 2026-10-17 | minor | Bend curves and the Bend window landed: focus item 1 is now the whammy bar and Paste Special; beta ~540–1,060 h |
| 2026-10-17 | major | Stages defined (alpha, beta, 1.0) with a beta gate of eight workflows (0 pass today) and a 1.0 gate; milestones aligned with GitHub's and given stages; M4, M10 rows refreshed (they still said no drums or samples, and no CI run); current focus re-ranked on the beta gaps (bend editor first); releases 0.2 to 0.4 named; earlier per-feature history moved to [`changelog.md`](changelog.md) |
| 2026-10-16 | minor | Alpha reached when the share workflow (PDF, print) landed; before it: ties, slurs and voices, Guitar Pro 3-6 import, drums |
| 2026-10-15 | major | Milestones M0–M10, alpha gate (fails), current focus |
