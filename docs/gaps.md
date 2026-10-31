# Where Guitario falls short of Guitar Pro

> **Last reviewed:** 2026-10-17 · **Last updated:** 2026-10-31 · **Change:** minor (G1: the drum step grid; beta ~168–348 h) · **Target:** Guitar Pro 8

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
| 1 | [G5](#g5-playback-realism) Playback realism | a listening pass on speakers (human) | 0 + human | pedalboard, cabinet impulse responses, effects in Guitar Pro files, audio track, MP3/OGG | 140–280 |
| 2 | [G6](#g6-editing-depth) Editing depth | done | 0 | nested tuplets, ornaments, assistants, tuning window | 80–160 |
| 3 | [G3](#g3-guitar-pro-exchange) Exchange | `.gp` checked in Guitar Pro (human) | 0 + human | PowerTab, TablEdit, `.gtp`, ASCII import, `.gpx` export | 90–180 |
| 4 | [G7](#g7-tools) Tools | a tuning check with a real guitar (human) | 0 + human | line in, MIDI capture, command palette | 70–130 |
| 5 | [G12](#g12-platforms-and-distribution) Platforms | opening a double-clicked file on macOS (human decision), the release workflow's first run, signing (human), a hands-on run on Windows and Linux | 20–40 + human | browser build, update check | 30–60 |
| 6 | [G8](#g8-ui-depth) UI depth | several documents, shortcuts audit, user book, click-to-edit, focus and track visibility | 50–95 | stylesheet window, floating panels, accessibility | 70–130 |
| 7 | [G1](#g1-drums-depth) Drums | articulations | 8–18 | kit map editing, drum mix, sampled-kit articulations | 15–30 |
| 8 | [G2](#g2-print-depth) Print | a real printer (human) | 0 + human | image export, stylesheet exchange with Guitar Pro | 10–20 |
| 9 | [G4](#g4-notation-depth) Notation | voice picking in notation, voice shortcuts, tab slurs | 6–22 | slash and numbered notation, grand staff, clefs, Design Mode | 150–300 |
| 10 | [G10](#g10-automation) Automation | control channel, screenshots, MCP, ten agent tasks | 40–80 | | |
| 11 | [G11](#g11-reliability-depth) Reliability | `.gt` and decoder fuzzing, a guard around painting | 15–35 | | |
| 12 | [G9](#g9-performance-budgets) Performance | budgets in CI, memory, startup, layout cache | 15–30 | | |
| 13 | [G13](#g13-languages) Languages | the localization framework | 15–30 | eleven translations | 40–80 + human |
| | **Total** | | **~168–348 h** | | **~705–1,380 h** |

Beta plus after-beta is **~873–1,728 h** to full parity.

## Beta gaps, in order

### G5. Playback realism
The biggest distance to Guitar Pro, and the one a listener hears first.
- **Landed 2026-10-18:** the effect chain per track (up to six of eleven original effects: amp,
  cabinet, overdrive, distortion, noise gate, compressor, ten-band equalizer, chorus, tremolo,
  delay, reverb) in place of the fixed drive, the mixer's five-band equalizer with presets, master
  volume and pan, and the three mastering slots; see the [changelog](changelog.md).
- **Landed 2026-10-18:** articulations (#111, #112): the synth plays palm mutes, dead notes,
  harmonics at their partial, slides in, out and between notes, hammer-ons and taps, staccato,
  slap and pop; a sound bank can name a program (or a keyswitch) per articulation, and legato
  notes move the sounding sample; see the [changelog](changelog.md).
- **Landed 2026-10-18:** the sampler's depth: release samples (#103), `trigger=legato` regions,
  filters with their envelope (#104), velocity curves (#106), WAV loop points and loop crossfades
  (#109), OGG Vorbis samples (#108); see the [changelog](changelog.md).
- **Landed 2026-10-18:** the sound bank in the app: the Sound Bank window and a first-run offer
  install the libraries (#114, #275), a track names its instrument (#113), and the bass plays
  Growlybass's release noises (the only one of the libraries that records more than sustain); see
  the [changelog](changelog.md).
- **Landed 2026-10-18:** master volume and pan automation (the rest of #265) and samples held as
  16-bit values with their memory in the perf budgets (the first half of #110); see the
  [changelog](changelog.md).
- **Landed 2026-10-18:** long samples stream from disk: the first second stays in memory, the
  rest is read when a note reaches it (the rest of #110); see the [changelog](changelog.md).
- **Landed 2026-10-18:** a modelled grand piano and the bowed strings (violin, viola, cello,
  double bass, sections, tremolo, pizzicato) for the programs that name them, each through a
  resonating body; slurs play in one bow stroke (PLAY-11); see the [changelog](changelog.md).
- **Landed 2026-10-19:** the bowed strings fitted to recordings of real violins, violas, cellos,
  double basses and sections (measured, not bundled): dynamics, vibrato, noise, the bodies'
  response, high notes without a growl, sections as wide as recorded ones (PLAY-11); see the
  [changelog](changelog.md).
- **Beta:** nothing an agent can finish: the listening pass below.
- **After beta:** more modelled instruments (organ, electric piano, winds, brass; PLAY-11), the
  piano's longitudinal "phantom" partials, damper and key noises and una corda, an original pedalboard (#466 to 468, #525), cabinet impulse responses loaded
  from files by convolution (#121; the cabinet is a filter model today), the effect chain and
  equalizer read from and written to Guitar Pro files, several sounds per track and sound
  automations (#461), an audio track with sync points (#483 to 486), external MIDI out (#470),
  MP3 and OGG export (#129), Line In (G7), style presets (#527).
- **Human:** a listening pass on speakers for the synth, the modelled piano and strings, and the
  sound bank (#115, #133); no agent can judge "sounds like a guitar" or "like a cello".
- **Evidence:** PLAY-8 (beta), PLAY-9/10 (1.0); PLAY-3 and PLAY-5 are done. The synth, its
  articulations and the effects are checked by tests (pitch within 4 Hz or 1.2 %, decay times,
  clean release, no clipping, each effect's response), not by ear.
- **Estimate:** 0 h + human beta, 140–280 h after.

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
- **Beta:** nothing left for an agent.
- **Human:** tune a real guitar and bass with the tuner through a microphone and an audio
  interface; check the macOS microphone permission prompt from the app bundle.
- **After beta:** Line In through the amp (#140, #487), MIDI capture (#339), Command Palette and
  Action List (#324 to 331), Expression Text (#330), scale diagrams in the score (#334), Fretlight
  (#341), the chord window's display options and fingering in the score (#318, #319), literal
  names with an apostrophe (#320).
- **Landed:** chord diagram editor and namer; the Chord window, the Scale tool and the tuner
  (2026-10-18); see the [changelog](changelog.md).
- **Evidence:** TOOL-5/6 (1.0); TOOL-2, TOOL-3 and TOOL-4 are done.
- **Estimate:** 0 h beta (+ the human check), 80–150 h after (the full 1,000-scale library is
  after beta).

### G12. Platforms and distribution
- **Landed 2026-10-18:** packages for three platforms (`cargo xtask package`), file types
  declared on each, and the release workflow (see the [changelog](changelog.md#g12-platforms-and-distribution)).
- **Beta:** opening a file double-clicked in Finder: the bundle declares the types, but the
  file arrives as an Apple Event that only an application delegate written with `unsafe` can
  receive, which the workspace forbids (decision for a human, [decisions](decisions.md);
  Windows and Linux pass the file on the command line and open it); the release workflow's first
  run on GitHub (it has never run; macOS and Windows packaging are untested); a hands-on run on
  Windows and Linux (CI builds and tests them for every pull request but nobody has used the app
  there).
- **Human:** signing material and accounts for Apple (#176) and Windows (#177), set as the
  workflow's secrets; whether to allow the one `unsafe` delegate on macOS.
- **After beta:** the browser build (#180), update check (#276).
- **Evidence:** DIST-1/4/5/6 (beta), DIST-3/7 (1.0).
- **Estimate:** 20–40 h + human beta, 30–60 h after.

### G8. UI depth
- **Beta:** several documents open at once with the tab bar already in place (#148); audit the
  default shortcuts against the manual's tables (#528 to 531, #153); Help › user book (#150, #195;
  `book/` has six pages); hover highlight and click-to-edit for the tempo, signatures, sections
  and chords (#223, #225, #228, #230, #232, #233); focus and saved track visibility (#264, #266,
  #315, #474; the track equalizer and master volume and pan landed with G5); preferences defaults for new
  scores and playback (#495 to 506); the score's context menu (#344).
- **After beta:** the Stylesheet window and its five tabs (#507 to 524), floating and docked
  panels (#147), accessibility through AccessKit (#155), the remaining click-to-edit elements
  (#224, #226, #227, #229, #231, #234 to 237), selection by dragging in the global view (#238,
  #239), the instrument-view scale display (#306), the drum staff view (#308), Homepage audio
  preview (#274), grid and scroll display modes (#311, #312).
- **Human:** re-measure the menu catalog, tunings and shortcuts from an installed Guitar Pro 8
  (#154).
- **Evidence:** UI-4/6/8/12/13/15 (beta), UI-3/5/14 (1.0).
- **Estimate:** 50–95 h beta, 70–130 h after.

### G1. Drums: depth
- **Landed 2026-10-31:** the step-grid drum editor (#98); see the [changelog](changelog.md#g1-drums-depth).
- **Beta:** articulations beyond the kit map: flams, rolls, ghost and accent noteheads, open-hat
  circles, choke (#99).
- **After beta:** per-track kit map editing and the full MIDI 27 to 87 map (#100, #362), drum mix
  (#472), stroke type (#363), entry on a staff line (#361), the drum staff view (#308).
- **Evidence:** EDIT-15 (done).
- **Estimate:** 8–18 h beta, 15–30 h after.

### G2. Print depth
- **Landed 2026-10-18:** headers and footers with tokens (#517) in File › Stylesheet… (F7),
  printing selected bars, twelve paper formats and custom sizes (#508); see the
  [changelog](changelog.md).
- **Beta:** nothing left for an agent.
- **Human:** send a page to a real printer on macOS, Windows and Linux. The Windows spooler path
  has never run, and CUPS was only tested through `lp`'s argument list.
- **After beta:** PNG and SVG pages (#36, #492), notation fonts embedded when no SMuFL font is
  installed, header and footer texts read from and written to Guitar Pro's `BinaryStylesheet`,
  font families beyond the serif and sans faces the page loads.
- **Evidence:** FILE-7 (beta, the printer), FILE-13 (1.0); FILE-11 is done.
- **Estimate:** 0 h + human beta, 10–20 h after.

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
| 2026-10-31 | minor | G1: the drum step grid (drums.grid, drums.setStep, the instrument view's Step grid; 15–30 h to 8–18 h beta); B3 has every piece |
| 2026-10-19 | minor | G5: the bowed strings fitted to recordings of real ones, solo and sections (beta unchanged: the listening pass) |
| 2026-10-18 | minor | G5: a modelled grand piano and bowed strings with resonating bodies, slurs bowed in one stroke (beta unchanged: the listening pass) |
| 2026-10-18 | minor | G2: headers and footers with tokens, selected bars, twelve papers and custom sizes (10–20 h to 0 h + human beta) |
| 2026-10-18 | minor | G5: long samples stream their tails from disk (15–35 h to 0 h + human beta); only the listening pass is left for beta |
| 2026-10-18 | minor | G12: packages for three platforms, file types declared, the release workflow (80–150 h to 20–40 h beta); macOS double-click needs a decision |
| 2026-10-18 | minor | G5: master volume and pan automation, 16-bit samples and a memory budget (25–55 h to 15–35 h beta) |
| 2026-10-18 | minor | G5: the sound bank in the app (Sound Bank window, first-run offer, installer shared with `cargo xtask sounds`), an instrument per track (#113, #114, #275; 75–155 h to 25–55 h beta); G8: the Sound Settings dialog item goes with it (55–100 h to 50–95 h) |
| 2026-10-18 | minor | G5: release samples, legato regions, filters and their envelope, velocity curves, WAV loops and crossfades, OGG samples (#103, #104, #106, #108, #109; 90–185 h to 75–155 h beta) |
| 2026-10-18 | minor | G5: articulations in the synth (palm mutes, dead notes, harmonics, slides, hammer-ons, taps, staccato, slap and pop) and per-articulation programs in the sound bank, sampled legato (#111, #112; 105–215 h to 90–185 h beta) |
| 2026-10-18 | minor | G5: effect chains, amp, cabinet, reverb, delay and the rest, track equalizer, master volume, pan and mastering (150–300 h to 105–215 h beta; cabinet impulse responses moved after beta); G8: the track equalizer and master row (60–110 h to 55–100 h) |
| 2026-10-18 | minor | G7: the tuner; no agent beta work left (20–40 h to 0) |
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
