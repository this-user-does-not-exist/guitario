# File format parity

> **Last reviewed:** 2026-10-15 · **Last updated:** 2026-10-15 · **Change:** major (first list) · **Target:** Guitar Pro 8

Guitar Pro 8's file formats, and what Guitario does with each. Part of
[target-app parity](target-app-parity.md); gaps G2 and G3 in [gaps](gaps.md).

| Format | Guitar Pro 8 | Guitario | Notes |
|---|---|---|---|
| `.gp` (Guitar Pro 7/8: ZIP + `score.gpif`) | read, write | **read (partial)** | `crates/gp`. Reads tracks, tunings, capo, programs, bars, time/key signatures, repeats, sections, tempo, rhythms with dots and tuplets, dynamics, notes and their effects; shared objects; reads drum kits (by articulation index or MIDI number); drops voices 2–4, grace notes, alternate endings, directions, lyrics and chord diagrams with warnings. Fixture: `demos/guitar-pro/import-test.gp` |
| `.gpx` (Guitar Pro 6) | read | missing | Compressed container around the same GPIF XML; mostly a container reader on top of `crates/gp` |
| `.gp5` / `.gp4` / `.gp3` | read (and `.gp5` export) | missing | Binary formats; the most common files shared online |
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
