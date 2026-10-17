# UI parity

> **Last reviewed:** 2026-10-17 · **Last updated:** 2026-10-17 · **Change:** minor (Interpretation section that changes playback; Musical Notation section with per-track notations; inspector Information sections as in Guitar Pro 8; instrument views in Guitar Pro 8's look; palette in Guitar Pro 8's sections, dark scheme sampled from Guitar Pro 8; earlier: tab bar, global view columns, toolbar order) · **Target:** Guitar Pro 8

Guitar Pro 8's workspace, element by element. Part of [target-app parity](target-app-parity.md);
gap G8 in [gaps](gaps.md). Menu wiring is measured separately in
[parity-checklist.md](parity-checklist.md).

| Element | Guitar Pro 8 | Guitario | Status |
|---|---|---|---|
| Toolbar | home, panels, zoom, display modes, undo/redo, print, transport, LCD, loop and speed, global tonality, audio track, instrument views, tuner, line-in, Fretlight | the same order in rounded groups, toggles blue when on, right-end groups drop out on narrow windows; LCD in two rows (track colour, icon and name, count-in, metronome and settings, note under the cursor; bar, bar check, time, triplet feel, tempo, time signature, progress line); relative tonality ±12 semitones for playback; audio track, tuner, line-in and Fretlight greyed out | partial |
| Document tabs | several open documents; full-width tabs, modified dot, save and close on hover, context menu, + for a new score | the same look and buttons for the one document Guitario holds; no second document, no lock icon | partial |
| Edition palette | voice 1-4, Lyrics and Chords buttons, then note values, rhythm, dynamics, articulations, effects, bar tools in eight columns | Guitar Pro 8's ten groups row for row (bar, note, effect, notation and automation symbols): lit for the cursor's beat and note, dimmed where they do not apply, symbols Guitario cannot write yet dimmed with "(not in Guitario yet)" (clef, simile, octaviation, accidentals, nested tuplets, hairpins, pedal, whammy, mordents, turns, wah, golpe, stems, scale, slash, barre, timer, master automation, design mode); corner triangles on symbols with options; hairpins two cells wide; popups for key, time, triplet feel, endings, repeats, directions, tuplets, fermatas, harmonics, slurs, text and sections | partial |
| Score view | page, screen, horizontal layouts; multitrack | page, screen and horizontal layouts; multitrack with beats lined up across tracks | done |
| Engraving | standard, tab, slash, drums, chord diagrams, lyrics | standard, tab, or both, chosen per track (View › Notation and the inspector act on the current track), drums, chord diagrams, lyrics (see [scorecard](scorecard.md#notation-and-engraving)); no slash or numbered notation, no grand staff; bends show only in the tablature | partial |
| Inspector | Song and Track tabs: Information, Musical Notation, Sounds, Interpretation (Song: Sound Mastering) | Song and Track in the same look (full-width bold tabs, bold capital headings, "Title:" labels over near-black fields). Song: Information with Title and Artist, More ▾ for Subtitle, Album, Words, Music, Copyright, Transcriber, Notice and Instructions (Ctrl+Return for a new line), shown in the title block (Words & Music by…, copyright at the foot); Musical Notation with Stylesheet Open… (page format) and Concert tone (greyed). Track: Information row (colour swatches, instrument icon, name, short name shown at each system), Musical Notation as Guitar Pro lays it out: Notation buttons (tablature, standard; slash and numbered greyed), one staff (grand staff greyed), Tuning button with the string names (presets; the play button greyed), More ▾ with capo and transposition (greyed); Sounds (instrument, volume, pan, mute, solo); Interpretation as Guitar Pro lays it out (playing style, palm mute and accentuation sliders, Auto let ring, Auto brush and Stringed switches), each heard in playback. No tuning window, transposition, Sound Mastering, RSE/MIDI sound list | partial |
| Global view | track list with mixer controls and bar grid | Guitar Pro 8's columns: header with add, track menu, move down/up, multitrack, mute, solo, Vol., Pan., Eq.; rows with colour, instrument icon, number and name, eye (hides the track from the multitrack view), yellow mute, red solo, fader, volume automation A, pan knob (drag, double-click to centre), pan automation A (green when automated, opens the editor), equalizer; Master row carrying the sections with a bookmark; square bar cells in the track colour, grey when empty, the cursor's bar marked dark, numbers at 1, 4, 8 …; no equalizer, master volume, pan or focus (greyed), visibility not saved | partial |
| Instrument views | fretboard, keyboard, drum pad | fretboard, keyboard and drum pad, each with its toolbar button, in Guitar Pro 8's look: side column with ◀ ▶ (previous/next beat) and a gear menu (current beat, beat and bar, current and next beat; left-handed fretboard; saved); notes as yellow marks with their names, the display mode's others dimmed; hover names the pitch and octave; dark neck with an open-string strip, no fret numbers; keyboard C2-C7 with the keys the instrument cannot reach greyed, click enters the pitch on the best string; no scale display, no drum staff view | partial |
| Mixer | per-track volume, pan, effects; master | volume/pan/mute/solo in inspector and global view | partial |
| Tools | tuner, chord engine, scales, line in | chord diagram editor and namer | partial |
| Preferences | dialog | File › Preferences… (theme, interface scale) over persisted view, transport and audio settings | partial |
| Themes | light and dark | dark sampled from Guitar Pro 8's screenshots (blue-grey #292a33 panels, #525771 buttons, #568fff accent, near-black LCD and fields, slate #444751 workspace, off-white page) and light (File › Preferences…); the light one has had one pass | partial |
| Keyboard | Guitar Pro shortcuts | digits, arrows, + − . / R L X G A I P H V S B, ⌘ shortcuts; Help › Keyboard Shortcuts lists them from the menu catalog; F11 full screen | partial |
| Unsaved changes | prompt on close, New and Open | prompt (craft-quit on macOS) | done |
| Languages | about a dozen | English | missing |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-15 | major | First list |
| 2026-10-16 | minor | Score view done (page, screen, horizontal, multitrack); engraving and tools updated |
| 2026-10-17 | minor | Preferences dialog, light theme and interface scale |
| 2026-10-17 | minor | Look pass: palette eight columns wide with voices, Lyrics and Chords; global view rows with a volume slider |
| 2026-10-17 | minor | Global view: add-track button, column heading, markers row under the grid |
| 2026-10-17 | minor | Inspector look: full-width tabs and capital section headings (GP8 has Song and Track only; the Bar and Note tabs listed as missing were not in GP8) |
| 2026-10-17 | minor | Full Screen, Minimize and Keyboard Shortcuts (menus 153/173) |
| 2026-10-17 | minor | Keyboard instrument view (menus 154/173) |
| 2026-10-17 | minor | Tab bar: full-width tab with centred name, modified dot, save/close on hover, context menu, + |
| 2026-10-17 | minor | Global view in Guitar Pro 8's columns: eye, mute/solo icons, fader, pan knob, automation flags, equalizer, Master row with sections |
| 2026-10-17 | minor | Toolbar rebuilt in Guitar Pro 8's order and groups; two-row LCD with bar check and current note; relative tonality; undo/redo icons the right way round |
| 2026-10-17 | minor | Interpretation section: playing style, palm mute, accentuation, auto let ring, auto brush, stringed, all heard in playback |
| 2026-10-17 | minor | Notations per track (standard, tab or both; menus 155/173); Musical Notation section with Guitar Pro's buttons, tuning button and More |
| 2026-10-17 | minor | Inspector Information as in Guitar Pro 8: all ten song fields with More/Less, track colour, icon, name and short name; title block shows them |
| 2026-10-17 | minor | Instrument views in Guitar Pro 8's look: side column with arrows and settings, display modes, left-handed, yellow named note marks, keyboard range |
| 2026-10-17 | minor | Palette in Guitar Pro 8's ten groups with dimmed unbuilt symbols, option corners, wide hairpins; dark scheme, tab and inspector colours sampled from Guitar Pro 8 |
