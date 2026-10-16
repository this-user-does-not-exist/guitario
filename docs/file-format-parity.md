# File format parity

> **Last reviewed:** 2026-10-16 · **Last updated:** 2026-10-16 · **Change:** minor (beam groupings in `.gp5`) · **Target:** Guitar Pro 8

Guitar Pro 8's file formats, and what Guitario does with each. Part of
[target-app parity](target-app-parity.md); gaps G2 and G3 in [gaps](gaps.md).

| Format | Guitar Pro 8 | Guitario | Notes |
|---|---|---|---|
| `.gp` (Guitar Pro 7/8: ZIP + `score.gpif`) | read, write | **read (partial), write** | `crates/gp`. Reads tracks, tunings, capo, programs, bars, time/key signatures, repeats, sections, tempo, rhythms with dots and tuplets, dynamics, notes and their effects; shared objects; reads drum kits (by articulation index or MIDI number); reads up to four voices per bar, alternate endings and directions; reads grace beats as the grace notes of the next beat's notes (a grace note with no struck note after it on its string is dropped with a warning); reads chord names and diagrams (the staff's `DiagramCollection`, frets from each diagram's `baseFret`); reads lyrics (the lines as typed, or rebuilt from the beats' syllables when those are empty) fermatas (type, offset in quarter notes, length) and fingering (`LeftFingering`, `RightFingering`). Fixture: `demos/guitar-pro/import-test.gp`. **Export** (`writegp.rs`, File › Export › Guitar Pro 8…) writes the same model back: up to four voices, ties and hammer-ons marked at both ends, every effect, grace notes (as grace beats before their beat, one per kind), chord names and diagrams (one `DiagramCollection` item per distinct chord), lyrics (the lines and each beat's syllable), fermatas, fingering, repeats, endings, directions, drum kits; track volume/pan and chords on drum tracks are left out with a warning. It reads back unchanged for every demo and all 76 real files tried, but has **not been opened in Guitar Pro itself** yet (human check) |
| `.gpx` (Guitar Pro 6) | read | **read (partial)** | `crates/gp/src/gpx.rs`: the BCFZ-compressed BCFS container around the same GPIF XML, with Guitar Pro 6's track layout (tuning and capo on the track, `GeneralMidi` programs). 37 real files import (local corpus). Drum hits stored as kit element and variation are dropped with a warning; non-fretted tracks (voice, winds, keys) are skipped |
| `.gp5` / `.gp4` / `.gp3` | read (and `.gp5` export) | **read, `.gp5` write** | `crates/gp/src/legacy.rs` reads the binary formats: song information, tracks with up to seven strings and their MIDI channels, two voices (GP5), bars with repeats, alternate endings (GP3/4 numbers and GP5 bit sets), markers, keys, triplet feel, GP5 directions, tempo changes from mix tables, chords, beat text, strokes, slap/pop/tapping, fades in and every note effect the model holds. Tremolo picking (one byte per note, kept per beat), trills (the other fret and a period) and grace notes (fret, transition, length, and in GP5 the dead and on-beat flags) are read and written; lyrics (one track's five lines in GP4-5; a second track's are left out of a `.gp5` with a warning) are read and written; fingering (two bytes behind note flag 0x80) and beam groupings ("beam eighth notes by", written with each time signature and kept by the bars after it) are read and written; whammy dives are dropped and counted in the warnings, and a `.gp5` export reports fermatas it cannot hold. `crates/gp/src/write5.rs` writes 5.10 files (**File › Export › Guitar Pro 5…**). 39 real GP3-5 files import and come back unchanged through the writer; every demo round-trips (`crates/gp/tests/gp5.rs`, `demos/gp5/`) |
| `.gtp`, PowerTab `.ptb`, TablEdit `.tef` | read | missing | |
| MusicXML | read, write | missing | |
| MIDI | read, write | **write** | `crates/midi`: format 1, repeats unrolled, ties merged, mute/solo respected |
| PDF | write | **write** | `crates/ui-egui/src/pdf.rs`: File › Export › PDF… and `guitario-cli convert in out.pdf`. Vector pages on the score's paper (A4, A5, Letter or Legal; portrait or landscape; four margins) from the page view's own engraving, embedded font subsets, selectable text, one track or all, with or without notation; each track starts a page. Headers and footers, other paper and layout choices are missing (G2) |
| Print | yes | **yes** | File › Print… (Cmd+P) and `file.print`: printer, copies, collate, sides, black and white, through craft-print (CUPS, native Windows); pages shrink to fit other paper. File › Print Preview… shows the pages as the PDF has them and File › Page Setup… sets the paper; not yet tried on a real printer (human check) |
| WAV | write | **write** | `crates/playback` offline render, 16-bit stereo |
| MP3, FLAC, OGG | write | missing | |
| PNG / SVG images | write | missing | |
| ASCII tab | write (import too) | **write** | `crates/engine/src/ascii.rs` |
| Native | `.gp` | `.gt` | Versioned JSON, validated on load, atomic saves via craft-atomic |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-15 | major | First list |
| 2026-10-16 | minor | Beam groupings read and written in `.gp5` (four sizes in eighths, repeated to fill the bar; the usual grouping reads as none); `.gp` export reports them as left out |
| 2026-10-16 | minor | Fingering read and written in `.gp5` (note flag 0x80: left and right hand, -1 none, 0 thumb to 4 little) and GPIF (`LeftFingering`/`RightFingering`: P I M A C); public GP4, GP5, GP6 and GP7 files agree |
| 2026-10-16 | minor | Fermatas read and written in GPIF (`MasterBar/Fermatas`: `Short`/`Medium`/`Long`, `Offset` as a fraction of a quarter, `Length`; checked on a public GP7 test file); `.gp5` reports them as left out |
| 2026-10-16 | minor | Lyrics read and written in `.gp5`/`.gp4` (one track's five lines) and GPIF (`Lyrics/Line/Text` with `Offset`, and per-beat `Lyrics/Line` syllables; read from the beats when the lines are empty, as in a public GP7 test file) |
| 2026-10-16 | minor | Chord names and diagrams read and written in GPIF (`DiagramCollection`, `baseFret`-relative frets; checked on public GP5, GP6 and GP7 copies of one chord file) |
| 2026-10-16 | minor | Grace notes read and written in `.gp5` (fret, dynamic, transition, length, flags; GP3/4 mark a dead one by fret 255) and in GPIF (`<GraceNotes>` beats, `BeforeBeat` or `OnBeat`); checked on public GP3-7 test files |
| 2026-10-16 | minor | Tremolo picking and trills read and written in `.gp5` (checked on the real corpus: no longer dropped) and in `.gp`/GPIF (`<Tremolo>` 1/8, 1/4, 1/2 and `<Trill>` as a pitch; inferred from real files and one sample, not yet opened in Guitar Pro 8) |
| 2026-10-16 | minor | Print preview and page setup (`score.setPage`, `preview.rs`, `page_setup.rs`); the page setup is read and written in `.gp5` and `.gp` (`crates/gp/src/page.rs`), checked on a real Letter file |
| 2026-10-16 | minor | PDF export and printing (`crates/ui-egui/src/pdf.rs`, `spool.rs`, `print_dialog.rs`; engine `file.exportPdf`, `file.print`, `file.printers`) |
| 2026-10-16 | minor | `.gp` export (`crates/gp/src/writegp.rs`); every demo and 76 real files round-trip; not yet opened in Guitar Pro itself |
| 2026-10-16 | minor | `.gp`/`.gpx` import keeps voices 2-4 (rest-only voices dropped) |
| 2026-10-16 | minor | `.gpx`, `.gp5`, `.gp4`, `.gp3` import and `.gp5` export; File › Open takes any Guitar Pro file (told by content) |
