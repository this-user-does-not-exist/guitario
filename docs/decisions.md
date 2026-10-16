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

## 2026-10-16: Voices share columns by time

A bar's voices are laid out together, not one after another: every moment a beat starts in any
voice is a *column*, a column's width follows the gap to the next one (so a single voice spaces
exactly as before), and each beat sits in the column it starts in. A beat's *span* runs to the
column of its voice's next beat; clicks, the cursor and the selection use spans, so a half note in
Voice 1 over two quarters in Voice 2 can be clicked anywhere along its length. Voice 1 takes stems up
and Voice 2 stems down (drums: hands up, feet down) and their rests move apart. A voice that holds
only rests is drawn only while it is the one being edited, as Guitar Pro hides an empty second voice.
Everything that edits a beat (durations, notes, the clipboard, selection) follows `Cursor::voice`;
`edit.setVoice` adds a voice to the bar (a bar-long rest) when it is missing, as one undo step.

## 2026-10-16: The `.gp` writer mirrors the reader, and is checked against it

`crates/gp/src/writegp.rs` writes the GPIF the importer reads (and the shape of real GP6 `score.gpif`
files): every bar, voice, beat and note has its own id, rhythms are shared, strings count from the
lowest, a tie marks both of its notes. Its tests round-trip every demo and, locally, 76 real files, so
it is exactly as good as the reader and the notes they share. What that cannot show is whether Guitar
Pro 8 opens the result: no copy is available here, so the exports are marked unverified until a person
opens a few. Chord names and track volume/pan are not written (the reader does not read them), and
the export says so in its warnings rather than losing them silently.

## 2026-10-16: The PDF is the page view's engraving, recorded and replayed

There is one engraving code path. `page::paint_print_page` paints a page into an egui `Painter`;
on screen that is the window, for PDF it is a headless context whose recorded `Shape`s
(`pdf.rs`) become Krilla paths and glyph runs. The alternative, a second drawing routine written
against Krilla, would have drifted from the screen within a month. What it costs: text is placed
glyph by glyph from egui's layout (no shaping beyond what egui does, which is enough for Latin
text and symbols), and a shape egui can draw but the PDF cannot (images, gradients, callbacks) is
dropped with a warning; none exist on pages today. A missing character is reported, never drawn as
a box: this found that the natural sign had no glyph in any bundled font, so it is drawn as strokes
now (on screen too).

## 2026-10-16: The engine owns the commands, the shell owns the hardware

`file.exportPdf`, `file.print` and `file.printers` live in the engine so agents and the CLI drive
them like every other action, and the engine keeps building for wasm. The engine knows two traits,
`PageRenderer` and `Spooler`, and the shell registers the real ones; a session without them says
"this build cannot print" instead of failing mysteriously. `GuitarioApp::new` registers them
itself, so no caller can forget. The CLI gets them behind a default-on `pdf` feature, because they
pull in the UI crate (and its audio libraries); a lean headless build is `--no-default-features`.

## 2026-10-16: A4 pages, shrunk to fit

Pages are A4 (Guitar Pro's default stylesheet is built for it), and the print job asks craft-print
to shrink them to the printer's paper rather than refuse a mismatch, so a Letter printer works.
Paper choice, margins and headers belong to a page setup dialog, which does not exist yet
(gap G2). Printing is synchronous in craft-print, so the dialog runs the job on a worker thread;
the job holds an `Arc` of the score as it was, so edits made meanwhile do not change what prints.

## 2026-10-16: The page setup belongs to the score

Paper, orientation and margins are `Score::page`, not a preference: the same score must print the
same on every machine, Guitar Pro keeps them in the file, and one `score.setPage` is one undo step.
`#[serde(default)]` keeps older `.gt` files loading; `validate()` refuses a page whose margins leave
under 100 mm of paper, so no file can make the layout degenerate. Four papers (A4, A5, Letter, Legal)
cover what real files hold; anything else in a Guitar Pro file opens as A4 with a warning rather than
guessing a size. Sizes and margins map to Guitar Pro's whole millimetres (checked on 114 real files:
all A4 with margins 10, 10, 15, 10 except one Letter stored as 216 x 279). Guitar Pro's default
bottom margin is 10 mm and ours is 12, so Guitar Pro's defaults read as ours and ours write as
Guitar Pro's: a file nobody changed stays unchanged in both directions. A landscape `.gp` page is
written with the sizes as the page lies plus `Orientation`, as Guitar Pro 5 does without the flag;
nobody has opened one in Guitar Pro 8 to confirm (human check).

## 2026-10-16: Dialogs own the keyboard

`handle_keys` already stood down while a text field had focus and during the unsaved-changes prompt,
but a dialog without a focused field (Print, Page Setup, Print Preview) let digits become frets and
Space start playback behind it. `modal_open()` now covers every dialog.

## 2026-10-16: Tremolo picking belongs to the beat, a trill to the note

Guitar Pro 3-5 stores tremolo picking on each note and GPIF on the beat; the slashes are drawn
on the stem and a picked chord repeats whole, so the model keeps it per beat (`Beat::tremolo`:
eighth, sixteenth or thirty-second, the number of slashes) and the GP3-5 reader lifts it from the
notes. A trill is the other way round: one string alternating with another fret
(`NoteEffects::trill`: the other fret and a speed of 16th, 32nd or 64th). Playback expands both in
the timeline (`attacks()`: contiguous, exact to the tick, at most 512 per note, so no duration can
make a huge timeline); trill notes are marked legato so the synth does not re-pluck them, and a
trill wins over tremolo picking on the same note. MIDI export follows because it reads the same
timeline.

GPIF facts are inferred, not documented: `<Tremolo>` is `1/8`, `1/4` or `1/2` for one, two or three
slashes (the order the real tremolo test file lists them in, and what other readers do);
`<Trill>` is the MIDI pitch of the other note and its period an `XProperty` in 960ths of a quarter
(240, 120, 60), from a single real sample, whose trill pitch equals its own so it is ignored. The
exports have not been opened in Guitar Pro 8 (human check), as for the rest of `.gp` export.
