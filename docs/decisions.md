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

## 2026-10-16: A grace note belongs to the note it ornaments

Guitar Pro 3-5 keeps one grace note per note (fret, dynamic, transition, length, flags); GPIF
writes grace notes as beats of their own, before the beat, that take no time. Grace beats would
make every duration sum, cursor move, selection and column in the editor skip beats that do not
count, so the model follows Guitar Pro 5: `NoteEffects::grace` (a fret, sixteenth to sixty-fourth,
before or on the beat, a transition, dead). The GPIF reader folds a grace beat into the next beat's
notes on the same string (a second grace note on one string keeps the nearer one, and one with no
struck note after it is dropped, both with a warning); the writer emits one grace beat per kind
(before or on the beat, and length) a beat's grace notes use, so mixed kinds survive the trip.

Playback: before the beat, the grace note ends where the beat starts and the note before it on the
string is cut short; on the beat, it starts with the beat and the note is delayed by its length
(never beyond the beat). A grace note on the first beat of a piece has no time to take from, so it
goes on the beat. A slid or hammered grace note leads legato into the note; a bent one bends up to
the note's pitch, which then carries on without a new pluck. A tied note takes no grace note.
The GP3-5 byte order (fret, dynamic, transition, duration, then GP5 flags) was confirmed on public
GP3, GP4 and GP5 test files, which read the same grace notes as the GP6 and GP7 copies.

## 2026-10-16: Chord diagrams come from Guitar Pro's diagram collection; names from the notes

GPIF keeps a track's chords once, as `DiagramCollection` items on the staff (on the track in
Guitar Pro 6), and a beat names one by id. A diagram's frets count from its `baseFret` (0 is an
open string whatever the base), and its strings from the lowest; the model keeps absolute frets,
highest string first, with `first_fret = baseFret + 1`, which is what Guitar Pro 5 calls the base
fret: the GP5, GP6 and GP7 copies of one public chord file read the same. The writer lists each
distinct chord of a track once and starts its diagram at the chord's first fret, or lower when a
fret sits below it; a chord without a diagram is an item with a name only.

Diagrams are drawn under the names, so a track with any gets a taller lane above every system,
and a column holding one is at least as wide as the diagram. A barre is a drawing guess (Guitar
Pro stores fingering, the model does not): the lowest fret held across three or more strings with
nothing open or muted between them, and none at all when any string is open, so an open D is not
barred. The chord namer (`guitario_theory::chord_name`) matches the pitch-class set against shapes
in order of how common they are, trying the bass note as the root first, so C E G A over C is C6
and over A is Am7; another root gets a slash bass (D/F#).

## 2026-10-16: Lyrics are typed text, laid on the beats when drawn

Guitar Pro 4-5 keep lyrics as text: five lines for one track, each from a bar. GPIF keeps the same
lines per track and, when "dispatched", the syllable each beat carries. The model keeps the typed
lines (`Track::lyrics`, up to five, each a 0-based bar and text) and lays them on the beats when
they are drawn or written (`guitario_score::dispatch_lyrics`), so inserting a note moves the words
along as it does in Guitar Pro. The syllable rules follow Guitar Pro's lyrics editor as other
readers document it: whitespace and hyphens split, `+` joins words on one note, a lone `-` skips a
note, trailing `_` is dropped and `[...]` is a comment; syllables go to the first voice's beats from
the line's bar, skipping rests (ties are sung on, as alphaTab reads Guitar Pro files). The GPIF
reader prefers the typed lines and rebuilds lines from the beats' syllables only when every line is
empty (joining words on one beat with `+` and marking a skipped beat with `-`); the writer writes
both. A `.gp5` holds one track's lyrics, so the first track with lyrics goes out and any other is
reported.

Lyrics are drawn in a lane under the tab (under the drum staff on a drum track), one row per line,
rather than between the notation and the tab where Guitar Pro puts a vocal staff's words: that gap
holds the effect lanes, and under the tab the words stay in the same place with notation hidden.

## 2026-10-16: A fermata stretches time, not ticks

GPIF keeps fermatas on the master bar (a type, where in the bar as a fraction of a quarter note, and
a length), so the model does too (`MeasureHeader::fermatas`, by tick). Holding a beat longer by
moving ticks would pull the tracks apart, so playback stretches time instead: the tempo map gives
each bar its fermata holds (`Score::holds`: from the fermata to the next moment a beat starts in any
track or voice) and plays them 1.5, 2 or 3 times as long for short, medium and long. Ticks, the
timeline and every track stay as written; MIDI export writes the hold as a slower tempo and the
bar's tempo after it. What Guitar Pro's `Length` does to playback is not documented, so it is kept
for the trip back to a `.gp` file and not used. Guitar Pro 5 has no fermatas.

## 2026-10-16: Beam groups are counted in the signature's note value

Guitar Pro 5 stores "beam eighth notes by" (four sizes in eighths) with each time signature; an
eighth cannot say 13/16 as 4+4+5, so the model counts a bar's groups in its own note value
(`MeasureHeader::beams`, empty for the usual grouping) and converts at the file boundary: four
sizes are repeated to fill the bar, and a grouping that cannot be said in eighths, or needs more
than four sizes that do not repeat, is reported and written as the usual one. The usual groupings
(`default_beaming`) are one beat each when the beat is a quarter or longer, dotted quarters in
compound time, twos with a closing three in other eighth meters (7/8 as 2+2+3) and quarter notes'
worth in shorter meters with up to half a group's remainder joining the last (13/16 as 4+4+5). The
bytes this writer produces for a usual grouping read back as "usual", so files round-trip.

Beams slope with the outer notes of their group, by at most a staff space, and lie flat when an
inner note reaches past both ends; the beam then moves away from the heads until no stem is
shorter than the note alone would want. The rhythm under a tab-only staff keeps flat beams, as
Guitar Pro draws it.

## 2026-10-16: Layouts and the multitrack view share one layout pass

The layout takes parts (one track, or every track in the multitrack view) and lays their bars out
together: the columns of a bar are the moments a beat starts in any voice of any track, so tracks
line up in time, and a system is as tall as its parts stacked. The cursor's track draws the cursor,
selection and playback mark; the top part alone draws the bar number, tempo, section marker,
endings, directions and triplet feel, and the parts below it have less room above. The screen
layout is a page as wide as the window and endless in height (no page breaks, no page number, the
title kept); the horizontal layout is endless both ways with one unjustified system and no title.
Layout mode and multitrack are view state in the shell, saved in the preferences, not in the
score. Printing and PDF still lay out one track at a time in pages.

## 2026-10-17: The page follows Guitar Pro 8's default stylesheet; no status bar

The page draws what Guitar Pro 8's default stylesheet draws, read from the manual's screenshots:
bar numbers on every bar, the abbreviated track name upright on the first system only (so only
that system is indented and the others use the full width), the tuning at the top, a bold tempo,
no time signature in the tab under standard notation, and a section as its letter boxed and its
name in bold (a name in brackets framed). The section letter is its own field
(`MeasureHeader::section_letter`), as in GPIF's `<Section><Letter>`; Guitar Pro 5 has names only,
so the letter goes in front of the name there. A bar whose voice is longer or shorter than its time
signature has a red number on screen (Guitar Pro's notation errors are red); paper never shows it.
These are fixed until a stylesheet window exists.

Guitar Pro has no status bar, so Guitario dropped its own: messages show as a toast over the
bottom of the score, a few seconds (longer for errors), dismissed by a click. The cursor position
it used to show is in the LCD.

## 2026-10-17: Fixed rows above the staff, in Guitar Pro's order

Guitar Pro packs what goes above a system per system; Guitario's layout gives every system of a
track the same height, so the lane above the top track has fixed rows, top down as Guitar Pro
stacks them: tempo marks (with accel./rit. and the swing indication beside the tempo), section
markers, segno and coda, endings, bar numbers. The section row is kept only when the score has a
section (`Metrics::sections`), so scores without sections lose no height. The header keeps room
for the tuning legend (`Metrics::legend_lines`), which could run into the first system before.

## 2026-10-18: Effects are tables of parameters; a track without a chain plays its instrument's

Guitar Pro 8 gives each sound an effect chain of up to six pedals and amps, the mixer an
equalizer, and the song three mastering slots of fixed types. Guitario stores an effect as its
type, an on flag and a map of named values (`guitario_score::Effect`), and describes each type
once as a table of parameters with ranges, units and presets (`EffectType::params`, `presets`).
Validation, the engine's commands (`track.setEffect` takes `params: {name: value}`), the
`sound.effectTypes` query for agents and the UI's knobs all read that table, so a new effect is
one table and one DSP unit. The mixer's equalizer is an effect of a type no chain may hold.

`Track::effects` is optional: none means the instrument's own chain (`default_chain`, an amp and
cabinet for the overdriven and distorted programs), so files from before effects existed sound as
they did and changing the program changes the sound. A sampled instrument plays no default chain,
because its recording already carries its amp (the FreePats distorted guitar is recorded through
one). Editing the chain copies the default into the track first. A default equalizer and master
are left out of saved files, so older files and the demos are unchanged by a resave.

The cabinet is a set of filters (resonance, cone notch, presence, a steep roll-off moved by the
microphone position), not a convolution with recorded impulse responses: the filters cost little,
need no licensed assets, and sound like a speaker for practice. Loading a user's impulse response
files by partitioned convolution is after beta (issue 121). The synth is mono, so a chain runs
mono until a chorus or reverb widens it; whether a bus is mono is fixed by whether it plays a
sampled instrument, never decided per sample, so every channel's filters and delay lines stay in
step.

## 2026-10-18: Articulations: the synth plays them, a bank may record them

Guitar Pro's RSE plays palm mutes, harmonics, slides and legato from recordings of each. Guitario
plays them in two ways. The plucked-string synth models each one, because it is what everyone
hears without a sound bank: a palm mute is a lowpass inside the string loop plus a short decay (the
filter's delay is taken off the delay line at the note's pitch, or a muted note would sound flat);
a harmonic is the same string excited through a comb that cancels every partial without a node
at the touched point, so it rings at the partial for as long as that partial would (pure, not
tempered, as on a guitar); legato is the vibration already on the string, moved to the new pitch
with a little energy added. A sampled instrument takes a program per articulation from the bank,
or a keyswitch within one program; whatever a bank does not record is borrowed from the sustain
program and shaped as before (cut short, moved in pitch).

Which articulation a note is and whether it carries on its string are decided once
(`crates/playback/src/articulation.rs`) and used both to load samples and to play them, so a
program never lacks the keys the player asks of it. The timeline keeps `pitch` as the fretted
note, which the views and the clipboard need, and adds what a harmonic sounds beside it; MIDI
export uses that too. A shift slide picks the second note and a legato slide does not, as Guitar
Pro's manual defines them; before, both flowed on. Natural harmonics are looked for up to the
ninth partial within about a third of a fret, because the model stores whole frets (Guitar Pro's
2.4, 3.2 and 5.8 cannot be written); a fret without a node plays the plain note.


## 2026-10-18: The app downloads sound libraries with curl and unpacks them itself

Guitar Pro's installer offers its sound banks; Guitario's are open libraries hosted by their
authors (GitHub releases, a git repository), about 0.9 GB. The app, the engine's
`sound.installLibraries` and `cargo xtask sounds` share one installer (`crates/playback/src/install.rs`).
It downloads with `curl` rather than an HTTP and TLS stack in the app: curl ships with macOS,
Windows 10 and later and Linux desktops, follows redirects, resumes a broken download (`-C -`) and
uses the system's certificates, and the app gains no network code of its own. Unpacking is done in
Rust (the `zip` crate already in the tree, `sevenz-rust2` with its pure-Rust bzip2 for FreePats'
7z archives), because `unzip` is missing on some systems and GNU tar cannot read 7z. Both reject
entries that would land outside the library's folder. The drum kit is a git repository: cloned
with `git` when it is installed, else downloaded as GitHub's zip of it.

An install writes into `<id>.partial` and renames it only when the program is found, then marks
it complete, so a failed or cancelled install never looks installed; the archive stays (with the
address it came from) to carry on next time, and is dropped once unpacked or when it cannot be.
`bank.json` is merged, not rewritten: the installer fills a role that is empty or already points
into its library, and leaves a user's own entries (another library, articulation programs). A
track can name its instrument (`Track::bank`): a role, or an `.sfz` file, absolute or relative to
the bank's folder so a score and a bank can move together. The first-run offer is armed by the
desktop app only, so tests and offscreen renders never see it.

## 2026-10-18: Samples are kept as 16-bit values

A score's sampled instruments are decoded into memory for the keys it plays; with the default
guitars and bass, the 400-bar perf score held 1 GB as floats. The libraries ship 16- and 24-bit
recordings, so samples are now held as `i16`: exact for 16-bit files, and for 24-bit ones the
loss is below -96 dBFS, under the recordings' own noise. Memory halves, playback converts on read
at no measurable cost, and `sounds_memory_mb` in `perf/budgets.toml` watches it. Streaming the
tails of long samples from disk (#110) would cut it further and is the remaining part.

