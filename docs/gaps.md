# Where Guitario falls short of Guitar Pro

> **Last reviewed:** 2026-10-17 · **Last updated:** 2026-10-18 · **Change:** minor (G7: the Chord window and the Scale tool; beta ~425–850 h) · **Target:** Guitar Pro 8

Every known shortfall against Guitar Pro 8, one entry each, ranked by what stands between a
mainstream user and the [beta gate](roadmap.md#beta-gate). This is the work list agents pick
from: take the highest-ranked gap with beta work in it that nobody is working on. What is already
built is in [`changelog.md`](changelog.md), not here. When you close part of a gap, move its text
to the changelog, update the numbers here, and regenerate the [scorecard](scorecard.md).

Hours are Opus 5.5 agent-hours, one agent working sequentially, including tests. They are an
**upper bound**: see [calibration](target-app-parity.md#calibration). **Human** marks what an agent
can't finish alone. `#n` is a GitHub issue; the Beta and 1.0 milestones there mirror the split
below, but the issue labels lag the code (see [Tracker](#tracker)). Checklist ids (`EDIT-…`,
`PLAY-…`) are rows in the [scorecard](scorecard.md).

## Summary

Ranked by user impact on the way to beta.

| Rank | Gap | Beta needs | Beta h | After beta (1.0) | After h |
|---:|---|---|---:|---|---:|
| 1 | [G5](#g5-playback-realism) Playback realism | articulated samples, amp, cabinet, reverb and delay, first-run sound bank | 150–300 | pedalboard, mastering, audio track, MP3/OGG | 130–260 |
| 2 | [G6](#g6-editing-depth) Editing depth | done | 0 | nested tuplets, ornaments, assistants, tuning window | 80–160 |
| 3 | [G3](#g3-guitar-pro-exchange) Exchange | `.gp` checked in Guitar Pro (human) | 0 + human | PowerTab, TablEdit, `.gtp`, ASCII import, `.gpx` export | 90–180 |
| 4 | [G7](#g7-tools) Tools | tuner | 20–40 | line in, MIDI capture, command palette | 70–130 |
| 5 | [G12](#g12-platforms-and-distribution) Platforms | signed releases on three platforms, file associations, release workflow | 80–150 + human | browser build, update check | 30–60 |
| 6 | [G8](#g8-ui-depth) UI depth | several documents, shortcuts audit, user book, click-to-edit, track EQ and master | 60–110 | stylesheet window, floating panels, accessibility | 70–130 |
| 7 | [G1](#g1-drums-depth) Drums | step-grid editor, articulations | 15–30 | kit map editing, drum mix, sampled-kit articulations | 15–30 |
| 8 | [G2](#g2-print-depth) Print | headers and footers, selected bars, a real printer | 10–20 | image export, more paper | 10–20 |
| 9 | [G4](#g4-notation-depth) Notation | voice picking in notation, voice shortcuts, tab slurs | 6–22 | slash and numbered notation, grand staff, clefs, Design Mode | 150–300 |
| 10 | [G10](#g10-automation) Automation | control channel, screenshots, MCP, ten agent tasks | 40–80 | | |
| 11 | [G11](#g11-reliability-depth) Reliability | `.gt` and decoder fuzzing, a guard around painting | 15–35 | | |
| 12 | [G9](#g9-performance-budgets) Performance | budgets in CI, memory, startup, layout cache | 15–30 | | |
| 13 | [G13](#g13-languages) Languages | the localization framework | 15–30 | eleven translations | 40–80 + human |
| | **Total** | | **~425–850 h** | | **~695–1,360 h** |

Beta plus after-beta is **~1,120–2,210 h** to full parity.

## Beta gaps, in order

### G5. Playback realism
The biggest distance to Guitar Pro, and the one a listener hears first.
- **Beta:** map palm mutes, harmonics, slides and dead notes to library articulations (#111) and
  play hammer-ons, pull-offs and slides without a new pick attack (#112); release samples (#103),
  SFZ filters (#104), velocity curves (#106), loop points (#109), OGG samples (#108), a memory
  budget with streaming (#110); an amp model per track (gain, EQ, presence) in place of the fixed
  drive (#120), cabinet impulse responses (#121), reverb, delay and chorus (#122), a track
  equalizer (#473) and master volume (#265); install the sound bank from the app on first run
  (#275, #114) so a download needs no `cargo xtask sounds`; pick the sampled instrument per track
  (#113).
- **After beta:** an original pedalboard (#466 to 468, #525), mastering effects (#482, #526), EQ,
  compressor and gate (#123), several sounds per track and sound automations (#461), an audio
  track with sync points (#483 to 486), external MIDI out (#470), MP3 and OGG export (#129),
  Line In (G7), style presets (#527).
- **Human:** a listening pass on speakers for the synth and the sound bank (#115, #133); no agent
  can judge "sounds like a guitar".
- **Evidence:** PLAY-3/5/8 (beta), PLAY-9/10 (1.0). The synth is checked by tests (pitch within
  4 Hz, clean decay, no clipping), not by ear.
- **Estimate:** 150–300 h beta, 130–260 h after.

### G6. Editing depth
- **Landed 2026-10-17:** bend curves and the Bend window; the whammy bar and vibrato with the bar;
  Guitar Pro's paste rules, All-Track Copy and Cut, Paste Special, the Check Bar Durations window;
  Unset; the pitch bends reach in standard notation; see the [changelog](changelog.md).
- **Beta:** nothing left.
- **After beta:** nested tuplets and 2 to 13 over any ratio (#287, #404), ornaments (#379, #380),
  the assistants (let ring, palm mute, bar arranger, finger positioning, voices; #345 to 349), the
  tuning window and tuning library (#426 to 432), transposing instruments (#416), non-fretted
  instruments (#407), anacrusis (#354), multirests, file protection (#340).
- **Evidence:** EDIT-3, EDIT-5, EDIT-8.
- **Estimate:** 0 h beta, 80–160 h after.

### G3. Guitar Pro exchange
- **Landed 2026-10-18:** MIDI import; MusicXML import and export; Guitar Pro 6 drum hits; see the
  [changelog](changelog.md).
- **Beta:** nothing left for an agent.
- **Human:** open a few `.gp` exports in Guitar Pro 8 (#19). The writer follows the importer's
  fixture and 76 real files, but no copy of Guitar Pro was available to open them.
- **After beta:** ASCII tab import (#27), PowerTab (#28), TablEdit (#29), `.gtp` (#30), `.gpx`
  export (#256), a MIDI import options window and pitch bends read as bends, batch converter (#257, #493), embedded audio (#486), AIFF (#490), the volume and
  pan automation round trip in Guitar Pro files (#460).
- **Evidence:** FILE-2/3/5/6 (beta), FILE-12/13 (1.0).
- **Estimate:** 0 h beta (+ the human check), 90–180 h after. After beta also: more than 15
  tracks from Guitar Pro files (the importer stops at 15, the MIDI channel count), keyboard-only
  Guitar Pro 6 files.

### G7. Tools
- **Beta:** a tuner: audio input in `crates/audio` (#134) and a pitch detector within ±2 cents
  (#135).
- **After beta:** Line In through the amp (#140, #487), MIDI capture (#339), Command Palette and
  Action List (#324 to 331), Expression Text (#330), scale diagrams in the score (#334), Fretlight
  (#341), the chord window's display options and fingering in the score (#318, #319), literal
  names with an apostrophe (#320).
- **Landed:** chord diagram editor and namer; the Chord window and the Scale tool (2026-10-18);
  see the [changelog](changelog.md).
- **Evidence:** TOOL-2 (beta), TOOL-5/6 (1.0); TOOL-3 and TOOL-4 are done.
- **Estimate:** 20–40 h beta, 80–150 h after (the full 1,000-scale library is after beta).

### G12. Platforms and distribution
- **Beta:** a release workflow from a tag (#179); a signed, notarized macOS app bundle (#176), a
  signed Windows installer (#177) and Linux packages (#178); file associations for `.gt`, `.gp`
  and the legacy formats (#175); a hands-on run on Windows and Linux (CI builds and tests them for
  every pull request but nobody has used the app there).
- **Human:** signing material and accounts for Apple and Windows.
- **After beta:** the browser build (#180), update check (#276).
- **Evidence:** DIST-1/4/5/6 (beta), DIST-3/7 (1.0).
- **Estimate:** 80–150 h + human beta, 30–60 h after.

### G8. UI depth
- **Beta:** several documents open at once with the tab bar already in place (#148); audit the
  default shortcuts against the manual's tables (#528 to 531, #153); Help › user book (#150, #195;
  `book/` has six pages); hover highlight and click-to-edit for the tempo, signatures, sections
  and chords (#223, #225, #228, #230, #232, #233); track equalizer, master volume and pan, focus
  and saved track visibility (#264 to 266, #315, #473, #474); preferences defaults for new
  scores and playback (#495 to 506); the score's context menu (#344); the Sound Settings dialog
  (#114).
- **After beta:** the Stylesheet window and its five tabs (#507 to 524), floating and docked
  panels (#147), accessibility through AccessKit (#155), the remaining click-to-edit elements
  (#224, #226, #227, #229, #231, #234 to 237), selection by dragging in the global view (#238,
  #239), the instrument-view scale display (#306), the drum staff view (#308), Homepage audio
  preview (#274), grid and scroll display modes (#311, #312).
- **Human:** re-measure the menu catalog, tunings and shortcuts from an installed Guitar Pro 8
  (#154).
- **Evidence:** UI-4/6/8/12/13/15 (beta), UI-3/5/14 (1.0).
- **Estimate:** 60–110 h beta, 70–130 h after.

### G1. Drums: depth
- **Beta:** the step-grid drum editor (#98) and articulations beyond the kit map: flams, rolls,
  ghost and accent noteheads, open-hat circles, choke (#99).
- **After beta:** per-track kit map editing and the full MIDI 27 to 87 map (#100, #362), drum mix
  (#472), stroke type (#363), entry on a staff line (#361), the drum staff view (#308).
- **Evidence:** EDIT-15.
- **Estimate:** 15–30 h beta, 15–30 h after.

### G2. Print depth
- **Beta:** headers and footers with tokens (#517), printing selected bars, other paper formats up
  to Guitar Pro's twelve (#508).
- **Human:** send a page to a real printer on macOS, Windows and Linux. The Windows spooler path
  has never run, and CUPS was only tested through `lp`'s argument list.
- **After beta:** PNG and SVG pages (#36, #492), notation fonts embedded when no SMuFL font is
  installed.
- **Evidence:** FILE-7, FILE-11 (beta), FILE-13 (1.0).
- **Estimate:** 10–20 h beta, 10–20 h after.

### G4. Notation depth
- **Beta:** picking a voice by clicking its notehead in notation, voice shortcuts (⌘1 to 4 and
  multivoice, #434, #435), slurs across bars and in the tab, room for a head that steps aside.
- **After beta:** slash and numbered notation and grand staff tracks (#54, #294, #309, #310,
  #370), clef changes (#277, #372), simile marks, multirests, octave lines, free text, accidental
  overrides (#375, #356, #376, #398, #400), Design Mode (#295 to 300), stem direction and beam
  overrides in Guitar Pro files, stylesheet switches (chord list, symbol texts, fingerings,
  #520 to 524), jianpu entry (#371).
- **Evidence:** NOTE-13/14 (1.0); NOTE-3/4/5/9/12 and EDIT-9/16 are done and list what is left.
- **Estimate:** 6–22 h beta, 150–300 h after.

### G10. Automation
- **Beta:** a JSON control channel over a local socket to drive the running app (#162), screenshots
  over it (#163), an MCP server over the engine (#164) and an agent task suite of ten scripted edit
  tasks (#165). Every action is already a command, so this is transport and a harness. It is on
  the beta path because "every feature drivable by agents" is a stated goal of the project.
- **Evidence:** AUTO-3/4.
- **Estimate:** 40–80 h.

### G11. Reliability depth
- **Beta:** fuzz the `.gt` reader (#168) and the SFZ, WAV and FLAC decoders (#169); Guitar Pro 3
  and 4 to depth without a local corpus; a last-resort guard around painting.
- **Evidence:** REL-4 (the dispatch guard is done; a panic while painting is not caught), REL-5.
- **Estimate:** 15–35 h.

### G9. Performance budgets
- **Beta:** run `cargo xtask perf` in CI on a quiet runner or as a regression ratio; memory and
  startup budgets (#160); SFZ load time (#161); playback CPU with sampled voices (#159); a layout
  cache so an edit relays out only what it touched (#158); a 1,000-bar session.
- **Evidence:** REL-7. Measured today on a 400-bar, six-track score: a frame takes 1 to 2 ms, the
  synth uses 3 % of a core, Play costs 2 ms on the UI thread.
- **Estimate:** 15–30 h.

### G13. Languages
- **Beta:** the framework: extract every user-facing string and look it up by language (#182),
  and the language preference (#501). English stays the only shipped language.
- **After beta:** French, German, Spanish, Italian, Brazilian Portuguese, Japanese, Simplified
  Chinese, Russian, Korean, Dutch and Polish (#183 to 193), each needing a human reviewer.
- **Estimate:** 15–30 h beta, 40–80 h + human after.

## Tracker

GitHub has 14 gap epics (#197 to #210), milestones Alpha, Beta (83 open) and 1.0 (36 open), and
307 issues from the Guitar Pro 8 manual review that carry no milestone. The split in this file
is the reference; the issues should follow it:

- **Open issues for finished work.** Many Beta issues describe things that have landed (for
  example #18 `.gp` export, #43 to 47 chord names to grace notes, #56 to 59 layouts, #70 and
  #73 transpose, #81 multivoice, #82 and #85 grace notes and tremolo, #95 templates, #119 the
  mixer, #124 to 127 and #130 automations and audio settings, #141 to 143, #146, #149 and #151
  preferences, themes, UI scale, the keyboard view, Full Screen and Keyboard Shortcuts, #166 and
  #171 to 172 the panic guard and autosave, #156 the perf budgets). Closing them is a tracker
  clean-up that has not been done; check each against the [scorecard](scorecard.md) first.
- **Unmilestoned manual issues** (#223 to 531) follow the gap entries above: the beta items
  are named there, everything else is 1.0.
- **Human-needed** issues (24) are the entries marked Human above plus translations.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-18 | minor | G7: the Scale tool (35–65 h to 20–40 h beta) |
| 2026-10-18 | minor | G7: the Chord window (55–100 h to 35–65 h beta) |
| 2026-10-18 | minor | G3: Guitar Pro 6 drum hits; no agent beta work left (5–15 h to 0) |
| 2026-10-18 | minor | G3: MusicXML import and export (40–80 h to 5–15 h beta) |
| 2026-10-18 | minor | G3: MIDI import (55–100 h to 40–80 h beta) |
| 2026-10-17 | minor | G6: Unset, bends held through ties checked in Guitar Pro 5, and the pitch bends reach in standard notation: G6 has no beta work left (5–15 h to 0) |
| 2026-10-17 | minor | G6: paste rules, All-Track Copy and Cut, Paste Special and the Check Bar Durations window landed (15–35 h to 5–15 h beta) |
| 2026-10-17 | minor | G6: the whammy bar (six shapes or a free curve, the Whammy Bar window, its lane over the tab, playback) and vibrato with the bar landed (25–50 h to 15–35 h beta); G3: whammy dives and bar vibrato read from Guitar Pro 3 to 8 and written to `.gp` and `.gp5` |
| 2026-10-17 | minor | G6: bend curves, the seven Guitar Pro bend types and the Bend window landed (45–80 h to 25–50 h beta); G3: bends exchanged with all their points |
| 2026-10-17 | major | Rewritten as a work list: beta and after-beta split per gap, hours re-summed (beta ~540–1,060 h, full parity ~1,230–2,410 h), finished work moved to [`changelog.md`](changelog.md), tracker reconciliation noted; G4's long "Landed" list and the per-gap histories are in the changelog |
| 2026-10-17 | minor | G8: score page details from Guitar Pro 8's default stylesheet; Section menu and window; Sound Mastering greyed |
| 2026-10-17 | minor | G8: Preferences, Full Screen, Minimize, Keyboard Shortcuts, keyboard view (menus 150 to 154 of 173); G4: per-beat beam breaks and user-placed slurs |
| 2026-10-16 | minor | G1 to G11 landings through the day: see the [changelog](changelog.md) for each |
| 2026-10-15 | major | First ranked list (G1–G13) |
