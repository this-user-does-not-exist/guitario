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

## 2026-10-16: Guitar Pro files are told by content, and a real-file corpus stays local

`file.open` imports any Guitar Pro file by its extension, and `crates/gp` picks the reader from the
first bytes (ZIP, BCFZ/BCFS or a `FICHIER GUITAR PRO` banner), so a misnamed file still reads. An
import is an unsaved document, so Save never writes native JSON over a Guitar Pro file. The readers
were checked against public test files from other projects, used only as data in a local folder
(`GUITARIO_GP_CORPUS`, `crates/gp/tests/corpus.rs`); they are not committed, and no code of those
projects was read. Committed tests round-trip our own demos through the `.gp5` writer instead.

## 2026-10-16: Renamed from Guitario to Guitario

The product is **Guitario**; crates, binaries, ids and environment variables are `guitario`
(`GUITARIO_*`), and the native file extension is `.gt`. Nothing a user made is lost: `.gt`
files and the old `"guitario"` format tag still open, the first run imports Guitario's
`config.toml` through craft-settings' legacy-file hook, and a sound bank installed in Guitario's
data folder is used until Guitario has its own. Guitario keeps using the shared craft-libs crates
and shared-rules standards.
