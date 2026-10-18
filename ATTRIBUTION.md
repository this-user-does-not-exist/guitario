# Attribution

Every bundled third-party or original asset, with its author, source and licence. Add a row in the
same change that adds an asset, and keep the licence file next to the asset.

| Path | Title | Author | Source | Licence |
|---|---|---|---|---|
| `demos/*.gt`, `demos/midi/*`, `demos/tab/*` | Demo scores | Guitario contributors (original compositions) | `crates/engine/examples/demos.rs` | MIT OR Apache-2.0 |
| `docs/images/guitario-*.png` | Screenshots of the demo scores | Guitario contributors (offscreen renders of original scores) | `crates/ui-egui/examples/snapshot.rs` | MIT OR Apache-2.0 |
| `assets/app-icon/` (all files) | Guitario app icon (headstock and treble clef in a pick) | The project owner (drawing, vectorised with potrace) | Original work, see [`assets/app-icon/README.md`](assets/app-icon/README.md) | MIT OR Apache-2.0, [`assets/app-icon/LICENSE.txt`](assets/app-icon/LICENSE.txt) |
| `demos/guitar-pro/import-test.gp` | Guitar Pro import fixture | Guitario contributors | `crates/gp/examples/fixture.rs` | MIT OR Apache-2.0 |
| `crates/playback/tests/data/sine-440.ogg` | OGG Vorbis decoder fixture (half a second of a 440 Hz sine) | Guitario contributors (synthesised, encoded with oggenc) | `crates/playback/src/sfz/sample.rs` tests | MIT OR Apache-2.0 |

## Used at run time, not bundled

| What | Licence | How it is found |
|---|---|---|
| Bravura or Leland (SMuFL music fonts) | SIL Open Font License 1.1 | system font folders, `CRAFT_FONTS_DIR`, `GUITARIO_FONTS_DIR`, `GUITARIO_MUSIC_FONT` (`crates/ui-egui/src/fonts.rs`) |
| Times New Roman, Arial or Liberation/DejaVu equivalents | their own | system font folders |
| SFZ instruments installed from the Sound Bank window or `cargo xtask sounds` (not bundled; each with its licence and credit in `SOURCE.txt`) | FreePats FSBS electric guitars: CC0 1.0; Karoryfer Growlybass: CC0 1.0; MuldjordKit: **CC BY 4.0, credit Lars Muldjord (recording) and kinwie (SFZ mapping)** | `GUITARIO_SOUNDS` or the per-user data folder; details in `docs/sounds.md` |
