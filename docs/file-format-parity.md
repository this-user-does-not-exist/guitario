# File format parity

> **Last reviewed:** 2026-10-16 · **Last updated:** 2026-10-16 · **Change:** minor (`.gp`/`.gpx` import keeps up to four voices) · **Target:** Guitar Pro 8

Guitar Pro 8's file formats, and what Guitario does with each. Part of
[target-app parity](target-app-parity.md); gaps G2 and G3 in [gaps](gaps.md).

| Format | Guitar Pro 8 | Guitario | Notes |
|---|---|---|---|
| `.gp` (Guitar Pro 7/8: ZIP + `score.gpif`) | read, write | **read (partial)** | `crates/gp`. Reads tracks, tunings, capo, programs, bars, time/key signatures, repeats, sections, tempo, rhythms with dots and tuplets, dynamics, notes and their effects; shared objects; reads drum kits (by articulation index or MIDI number); reads up to four voices per bar, alternate endings and directions; drops grace notes, lyrics and chord diagrams with warnings. Fixture: `demos/guitar-pro/import-test.gp` |
| `.gpx` (Guitar Pro 6) | read | **read (partial)** | `crates/gp/src/gpx.rs`: the BCFZ-compressed BCFS container around the same GPIF XML, with Guitar Pro 6's track layout (tuning and capo on the track, `GeneralMidi` programs). 37 real files import (local corpus). Drum hits stored as kit element and variation are dropped with a warning; non-fretted tracks (voice, winds, keys) are skipped |
| `.gp5` / `.gp4` / `.gp3` | read (and `.gp5` export) | **read, `.gp5` write** | `crates/gp/src/legacy.rs` reads the binary formats: song information, tracks with up to seven strings and their MIDI channels, two voices (GP5), bars with repeats, alternate endings (GP3/4 numbers and GP5 bit sets), markers, keys, triplet feel, GP5 directions, tempo changes from mix tables, chords, beat text, strokes, slap/pop/tapping, fades in and every note effect the model holds. Grace notes, trills, tremolo picking, whammy dives, fingering and lyrics are dropped and counted in the warnings. `crates/gp/src/write5.rs` writes 5.10 files (**File › Export › Guitar Pro 5…**). 39 real GP3-5 files import and come back unchanged through the writer; every demo round-trips (`crates/gp/tests/gp5.rs`, `demos/gp5/`) |
| `.gtp`, PowerTab `.ptb`, TablEdit `.tef` | read | missing | |
| MusicXML | read, write | missing | |
| MIDI | read, write | **write** | `crates/midi`: format 1, repeats unrolled, ties merged, mute/solo respected |
| PDF | write | missing | Planned through craft-pdf (G2) |
| Print | yes | missing | Planned through craft-print (G2) |
| WAV | write | **write** | `crates/playback` offline render, 16-bit stereo |
| MP3, FLAC, OGG | write | missing | |
| PNG / SVG images | write | missing | |
| ASCII tab | write (import too) | **write** | `crates/engine/src/ascii.rs` |
| Native | `.gp` | `.gt` | Versioned JSON, validated on load, atomic saves via craft-atomic |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-15 | major | First list |
| 2026-10-16 | minor | `.gp`/`.gpx` import keeps voices 2-4 (rest-only voices dropped) |
| 2026-10-16 | minor | `.gpx`, `.gp5`, `.gp4`, `.gp3` import and `.gp5` export; File › Open takes any Guitar Pro file (told by content) |
