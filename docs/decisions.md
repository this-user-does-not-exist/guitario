# Implementation decisions

## 2026-10-15: Strings are numbered from the top

String 0 is the highest-pitched string, the top line of a tab staff, as a guitarist reads tab.
Guitar Pro numbers strings from the lowest; `crates/gp` flips them on import (and must flip them
back on export). Tunings are stored highest first for the same reason.

## 2026-10-15: Drums as kit lanes

A drum track is a fretted track with eight kit lanes "tuned" to 0, so a note's fret is its General
MIDI drum number (Guitar Pro 5 stored drums the same way). The kit map (`theory::drums`) gives each
piece a lane chosen so pieces played together (kick, snare, hi-hat, crash, ride, pedal) never share
one, a staff position and a notehead. MIDI, the timeline, undo and the clipboard work unchanged;
only notation, playback voices and entry know about drums. Guitar Pro 7/8 store articulations
instead; the importer maps them through their MIDI numbers.

## 2026-10-15: Clef from the highest string

Standard notation uses treble clef (octave up) for any guitar, including drop and 7-string
tunings, and bass clef when the top string is below E3 (a bass guitar). Choosing by the lowest
string gave 7-string guitars a bass clef.

## 2026-10-15: Fonts are found, not bundled

Under the suite's font rule nothing is committed: the page looks for a SMuFL music font (Bravura or
Leland, both SIL OFL) and Times New Roman/Arial equivalents (Guitar Pro's stylesheet defaults) on
the system or in `CRAFT_FONTS_DIR`/`GUITARIO_FONTS_DIR`, and draws simpler symbols without them.
Moving Bravura into craft-fonts is the long-term answer.

## 2026-10-15: Physical-model playback first

Guitar Pro's RSE plays recorded instruments. Guitario starts with a Karplus–Strong string model
and a simple amp: no sample library to license, bends and slides fall out of the model, and it is
deterministic for tests. Sampled playback is gap G5.

## 2026-10-15: Shared craft-libs crates

Atomic saves (`craft-atomic`), settings (`craft-settings`) and the platform quit (`craft-quit`) come
from craft-libs at a pinned revision instead of app-local code. `craft-ui` is not used yet: its
docking and tab widgets fit apps with dockable panels; adopt it with the multitrack and floating
panel work (G8).
