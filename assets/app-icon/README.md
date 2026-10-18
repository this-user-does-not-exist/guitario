# Guitario app icon

A **guitar headstock and a treble clef inside a pick outline**: white line work on black, the
clef's tail curling into the pick and three staff lines with a small pick beside it.

## Palette

Two colours, as the source drawing:

| Colour | Hex | Used for |
|---|---|---|
| Ink | `#0b0b0c` | the full-bleed field |
| White | `#ffffff` | the line work |

## Geometry

A 512-unit tile (`viewBox="0 0 512 512"`), rounded square with `rx=112`, no border; the art is
centred with its longest side 350 units. The macOS renders pad it onto Apple's 824/1024 icon grid;
the Windows and Linux renders crop 22 units off each side so the mark reads at 16–48 px.

## Provenance

The owner's original drawing (`source.png`, 768 px, white on black), vectorised with potrace by
`packaging/icons.sh --trace`: upscaled, its edge noise smoothed, traced, then placed on the tile.
The trace is reproducible: the script gives the committed SVGs byte for byte. License: see
`LICENSE.txt`.

## Files

- `source.png`: the drawing the trace comes from.
- `guitario.svg`: canonical master (traced at 3072 px).
- `guitario-small.svg`: lighter trace (traced at 1536 px); the hicolor scalable icon and the
  renders up to 64 px.
- `guitario-1024.png`: 1024 px render on the macOS grid (window and Dock icon on macOS).
- `guitario.icns`: macOS bundle icon (`CFBundleIconFile`).
- `guitario.ico`: Windows icon, 16–256 px, embedded in the `.exe` by `apps/guitario/build.rs`.
- `guitario-mark.png`: the line work alone on transparent, 192 px; the Homepage tints it.
- `hicolor/<size>/apps/ai.storyteller.guitario.png` (16–512) and `hicolor/scalable/...svg`:
  Linux icon theme.

The app sets the window icon at run time and draws the mark on its Homepage
(`crates/ui-egui/src/app_icon.rs`).

## Regenerate

Replace `guitario.svg` (and `guitario-small.svg`), then run `packaging/icons.sh`; or replace
`source.png` and run `packaging/icons.sh --trace`. It needs `resvg`, plus `potrace` and
ImageMagick for `--trace` and `iconutil` on macOS; the `.ico` is packed by `cargo xtask ico`.
