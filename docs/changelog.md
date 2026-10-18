# What has landed

> **Last reviewed:** 2026-10-17 · **Last updated:** 2026-10-18 · **Change:** minor (G5: articulations) · **Target:** Guitar Pro 8

The record of finished work, by gap, with the detail of how each piece behaves. It is history, not
a plan: what is still missing is in [`gaps.md`](gaps.md), the stages are in
[`roadmap.md`](roadmap.md), and the measured state is in [`scorecard.md`](scorecard.md). Entries
are in the order they were written; each says what was built and what it was checked against.

Landed work is also visible as merged pull requests (33 between 2026-10-16 and 2026-10-17, each a
"batch" that closes one slice of a gap) and in the local dev log.

### G1. Drums: depth
- **Landed 2026-10-15:** drum-kit tracks (`track.addDrums`, `note.setDrum`, eight kit lanes),
  percussion notation (clef, kit-map positions, cross noteheads), a drum pad view, two-digit GM
  entry, synthesised GM drum playback with hi-hat choke, MIDI channel 10, ASCII drum tab, and
  `.gp` drum import by articulation index or MIDI number. Twelve demos have drum parts and
  `14-drum-grooves` covers skank, d-beat, gallop, double kick, half time, blasts, china hits and a
  triplet fill (`crates/engine/tests/demos.rs`).
- **Landed 2026-10-16:** a second voice for the feet (stems down, hands up) as Guitar Pro writes
  drums: Edit › Voice 2, drawn and played, and read from `.gp5` and `.gp`.

### G2. Print depth
- **Landed 2026-10-16:** PDF export (File › Export › PDF…, `file.exportPdf`, `guitario-cli convert
  in out.pdf [--tracks 0,2] [--tab-only]`) and printing (File › Print…, Cmd+P, `file.print` and
  `file.printers`). The pages are the page view's own engraving (`page::paint_print_page`),
  painted into a headless egui context; `crates/ui-egui/src/pdf.rs` turns the recorded shapes into
  Krilla drawing operations through craft-pdf's guarded writer, so the PDF is vector with embedded
  font subsets and selectable text. Each track starts on a new page; the header is on each track's
  first page; page numbers run through the document. The engine owns the commands and two small
  traits (`PageRenderer`, `Spooler`), so it stays wasm-clean and a fake pair tests them. Printing
  goes through craft-print (CUPS, or the native Windows path) on a worker thread, with pages
  shrunk to fit other paper. Every demo exports without warnings (`pdf.rs` tests); pages were
  checked by rasterizing with poppler.
- **Landed 2026-10-16 (second step):** File › Print Preview… (the pages exactly as the PDF has
  them, painted through the same `PrintSet`, one track or all) and File › Page Setup… (A4, A5,
  Letter, Legal; portrait or landscape; four margins in mm). The setup is `Score::page`
  (`score.setPage`, saved in the `.gt` file with a default for older files, one undo step) and
  drives the page view, the PDF's page size and the print job. It is read from and written to
  Guitar Pro 5 and GPIF files in whole millimetres (checked on 114 real files, one of them Letter).
  Keys no longer reach the score while a dialog is open.

### G3. Guitar Pro exchange
- **Landed 2026-10-18:** Guitar Pro 6 drum hits (`gp6_drum` in `crates/gp/src/gpif.rs`): the kit
  element and variation become General MIDI numbers, worked out by reading a Guitar Pro 6 file
  beside the Guitar Pro 5 copy of the same song.
- **Landed 2026-10-18:** MusicXML import and export (`crates/musicxml`, File › Import and Export ›
  MusicXML…, File › Open on `.musicxml`/`.xml`/`.mxl`, `cli convert` to `.musicxml`): tunings,
  strings and frets, four voices, tuplets, ties, repeats, endings, sections, tempo, dynamics,
  lyrics, chords, drums and the guitar effects. Every demo comes back bar for bar; MuseScore 4
  reads the export, and MuseScore's own export of Steel Horizon imports with the same rhythms and
  pitches. The demo suite ships MusicXML copies (`demos/musicxml/`).
- **Landed 2026-10-18:** MIDI import (`crates/midi/src/import.rs`, `file.importMidi`, File ›
  Import › MIDI…, and File › Open on `.mid`/`.midi`): format 0 and 1, a track per MIDI track and
  channel, channel 10 as drums, tempo, time and key signatures, names, programs, volume and pan;
  per-quarter quantization that keeps written rhythms exact and finds triplets; chords fingered
  onto strings near the hand in a tuning that fits the range. Every demo exported to MIDI comes
  back with 99-100 % of its notes at the same moment (93 % for the technique showcase, whose
  trills and grace notes MIDI flattens). The MIDI export now writes the title and key signatures.
- **Landed 2026-10-16:** `.gp5`, `.gp4` and `.gp3` import (`crates/gp/src/legacy.rs`), `.gpx`
  (Guitar Pro 6) import (`gpx.rs`), `.gp5` export (`write5.rs`, File › Export › Guitar Pro 5…) and
  chord names. File › Open tells the format by content. Checked against a local corpus of 39 real
  GP3-5 and 37 real GP6 files (`crates/gp/tests/corpus.rs` with `GUITARIO_GP_CORPUS`): all
  import, and the GP3-5 ones come back unchanged through the GP5 writer. The corpus is public test
  data from other projects, used locally and not committed.
- **Landed 2026-10-16:** `.gp` (Guitar Pro 7/8) export (`writegp.rs`, `file.exportGp`, File ›
  Export › Guitar Pro 8…): up to four voices, ties, every effect the model carries, repeats,
  endings, directions, drum kits. Every demo and all 76 corpus files read back unchanged.
  **Not yet opened in Guitar Pro itself** (no copy here): it follows the structure of the
  importer's fixture and of real GP6 `score.gpif` files, so a human should open a few exports in
  Guitar Pro 8 before this is called done. Track volume/pan are not written (the importer does not
  read them either).
- **Landed 2026-10-16:** grace notes read and written in Guitar Pro 3-5 (fret, transition, length,
  dead and on-beat flags) and GPIF (grace beats before the beat, folded into the next beat's notes
  on the same string; one grace beat per kind on export). The byte order was checked on public
  GP3, GP4, GP5, GP6 and GP7 test files (alphaTab's test data, used locally and not committed):
  all five read the same grace notes.
- **Landed 2026-10-16:** chord names and diagrams in GPIF both ways (`DiagramCollection` items on
  the staff, frets counted from the diagram's `baseFret`; a beat's `<Chord>` names an item). The
  same chord file in GP5, GP6 and GP7 (alphaTab's public test data, used locally) reads the same
  eight chords.
- **Landed 2026-10-16:** lyrics both ways: Guitar Pro 4-5 (one track's five lines, 1-based bars;
  a second track's lyrics are left out of a `.gp5` with a warning) and GPIF (the lines as typed with
  their bar `Offset`, and the syllable on each beat; a file whose lines are empty is read from the
  beats' syllables).
- **Landed 2026-10-17:** bend curves in every Guitar Pro format that has them. Guitar Pro 3-5 read and
  write every point (positions 0 to 60, 25 per quarter tone; the same layout will hold the whammy
  bar). Guitar Pro 7/8 hold an origin, a destination and one plateau of two middle offsets, with
  offsets as percentages of the note; the reader keeps the points in order and ignores the middle
  points Guitar Pro leaves behind, a destination before the end holds, and the writer trims a flat
  end and the least important points of a richer curve (`gpif_bend` says what comes back). The
  old writer put the destination at 60 % of the note, now 100 %. Checked on real `.gp5` and `.gp`
  files, including a set of bend test files.
- **Landed 2026-10-17:** whammy dives and vibrato with the bar from every Guitar Pro format. Guitar
  Pro 7/8 write a beat's `<Whammy>` element (origin, middle and destination, values in hundredths
  of a whole tone down to −600, offsets in percent) with a `WhammyBar` property; Guitar Pro 6 the
  same numbers as `WhammyBar…` properties; Guitar Pro 4-5 the bend's structure behind beat-effect
  flag 0x04 (a dive is `(0, 0) (45, v) (60, v)`); Guitar Pro 3 one depth per beat, read as a dip.
  `VibratoWTremBar` (slight or wide) is read and written in `.gp`; a `.gp5` export drops it with a
  warning. `.gp` and `.gp5` exports write the curves (`gpif_whammy` says what comes back). Checked
  on the local corpus (225 files): one `.gp3` and one `.gp5` no longer lose their dives, and the
  round-trip results are unchanged. `<WhammyExtend/>` (four uses) is not read yet.
### G4. Notation depth
- **Landed 2026-10-16:** chord names (model, `beat.setChord`, Guitar Pro import and export) and
  beat text drawn above the system.
- **Landed 2026-10-16:** up to four voices per bar (Edit › Voice 1-4, `edit.setVoice`): voices
  share columns by time so notes that sound together line up, Voice 1 takes stems up and Voice 2
  stems down with rests moved apart, every voice plays and exports; ties drawn in notation and tab
  (across bars and system breaks); slurs for hammer-ons, pull-offs and legato slides.
- **Landed 2026-10-16:** tremolo picking (per beat: `beat.setTremolo`, Note › Tremolo Picking…;
  one to three slashes through the stem, above a whole note, under the tab when notation is hidden)
  and trills (per note: `note.setTrill`, Note › Trill…; "tr" and a wavy line above the staff and in
  the tab lane). Both play (the beat's notes are picked again and again; a trill alternates its
  two frets legato, at most 512 attacks per note), export to MIDI, and are read and written in
  Guitar Pro 3-5 (`.gp5` and the byte-level effects) and in GPIF. Demos 08 and 12 use them.
- **Landed 2026-10-16:** grace notes, per note (`note.setGrace`, Note › Grace Note…): a fret,
  sixteenth to sixty-fourth, before or on the beat, struck, slid, bent or hammered, or dead. They
  play (before the beat the note before them is cut short; on the beat they delay the note; the
  first beat of a piece takes them on the beat), export to MIDI and ASCII tab (`5h7`, `5/7`,
  `5b7`, `(5)7`), and are drawn small: a slashed eighth before the beat (unslashed on it) slurred
  into its note, and a small fret in the tab with its slur, slide or bend; the layout keeps room
  for them. Demos 11 and 12 use them.
- **Landed 2026-10-16:** chord diagrams drawn under the chord names in a taller lane (nut or first
  fret, dots, open and muted marks, a barre when the lowest fret is held across three or more
  strings and nothing is open); a column holding one is wide enough. The ballad demo has four.
- **Landed 2026-10-16:** lyrics: five lines per track, each from a bar (`track.setLyrics`, Track ›
  Lyrics…), laid one syllable per beat of the first voice skipping rests, with Guitar Pro's
  conventions (spaces and hyphens split, `+` joins words on one note, a lone `-` skips a note,
  trailing `_` dropped, `[comments]` not sung). Drawn in a lane under the tab, a row per line, with
  hyphens between the syllables of a word; columns widen for long syllables. Demo 04's verse is sung.
- **Landed 2026-10-16:** fermatas (short, medium, long) on the bar at a beat's place
  (`beat.setFermata`, a palette button): drawn over the beat, held in playback by stretching time
  in every track (1.5, 2 and 3 times the beat; the tempo map, not the ticks, so tracks stay
  together), written to MIDI as a slower tempo for the held beat, and read and written in GPIF
  (type, offset as a fraction of a quarter, Guitar Pro's length kept). Guitar Pro 5 has none: a
  `.gp5` export says they were left out. The solo demo ends on one.
- **Landed 2026-10-16:** fingering for both hands per note (`note.setFingering`, Note ›
  Fingering…): T and 1-4 beside the notehead, p i m a c stacked above the beat, in standard
  notation; read and written in Guitar Pro 3-5 (note flag 0x80) and GPIF (`LeftFingering`,
  `RightFingering`), the same on public GP4, GP5, GP6 and GP7 test files.
- **Landed 2026-10-16:** beams by the bar's grouping (`measure.setBeaming`, counted in the
  signature's note value) or the signature's usual one (one beat each in simple time, dotted
  quarters in compound time, 2+2+3 in 7/8, 4+4+5 in 13/16), kept in Guitar Pro 5 files ("beam
  eighth notes by"); and sloped beams (with the outer notes, at most a staff space, flat over an
  inner peak, every stem long enough). Demo 06 beams its 7/8 as 3+2+2.
- **Landed 2026-10-16:** the chord list under the title (each distinct diagram once, in order of
  first use, in the page view, print and PDF) and Edit › Multivoice Editing (voices in their own
  colours on screen; a click on another voice's note in the tab moves the cursor into that voice).
- **Landed 2026-10-16:** the full score on paper: every track stacked in each system in the PDF,
  the print job and the preview (`together`, "As a full score", `convert --score`), chosen
  by default when the multitrack view is on.
- **Landed 2026-10-16:** key changes inside a system: naturals for what the old key loses, then
  the new key, before any time signature (NOTE-3 done). Demo 06's change to E♭ shows.
- **Landed 2026-10-16:** courtesy key and time signatures at the end of the system before a change.
- **Landed 2026-10-16:** a stem-down voice's head steps right of a stem-up voice's a second away,
  or on the same pitch with another head or dots; identical heads on one pitch are shared.
- **Landed 2026-10-17:** per-beat beam breaks and joins (`beat.setBeam` with `break`, `join` or
  `auto`; `Beat::beam`, honoured by `notation::beam_groups`, so beamed stems, flags and the tab-only
  rhythm follow) and user-placed slurs (`beat.setSlur`: from the cursor beat to a later note in the
  same bar and voice; `Beat::slur`, an arc over the outer heads on the side away from the stems,
  notation only; a stale one after an edit is skipped, not a panic). Both have palette buttons
  (Beam and Slur popups next to the fermata; not yet checked by eye, the snapshot cuts the
  palette off).

### G5. Playback realism
- **Landed 2026-10-16:** an SFZ sampler and per-role sound bank ([`sounds.md`](sounds.md)) with
  open CC0/CC-BY libraries for guitars, bass and drums (`cargo xtask sounds`).
- **Landed 2026-10-16:** Sound › Mixer…: a strip per track (instrument, pan, fader, mute, solo),
  live while playing (faders heard while dragged, one undo step on release; mute and solo silence
  the track's bus, so they can change mid-song); samples decode on every core in the background.
- **Landed 2026-10-16:** tempo automation: gradual tempo changes (Guitar Pro's linear tempo
  points), played exactly, edited in Sound › Automations › Tempo Automation… with the curve,
  marked accel./rit., exported to MIDI and GPIF and read from Guitar Pro 7/8 files.
- **Landed 2026-10-16:** volume and pan automation per track (Sound › Automations › Volume /
  Pan Automation…), played over the mixer and exported to MIDI; Guitar Pro exchange of them waits
  on a confirmed mapping of the channel-strip values.
- **Landed 2026-10-16:** Sound › Audio Settings… (output device and buffer size, saved); FLAC
  export (File › Export › Audio…, lossless, pure Rust).
- **Landed 2026-10-18:** the effect chain, the track equalizer and the master section, laid out as
  Guitar Pro 8's manual shows them (Track Sound, Mixer, Sound Mastering). A track's chain holds up
  to six effects (`Track::effects`, `crates/score/src/sound.rs`) of eleven original models in
  `crates/playback/src/fx/`: compressor, noise gate, overdrive, distortion, amp (Clean, Crunch,
  Lead and Bass voicings: one to three clipping stages, bass, middle and treble, presence,
  master), cabinet (1x12 and 2x12 open, 4x12 closed, 4x10 bass, a microphone from the cone's
  centre to its edge; a filter model), ten-band equalizer, chorus, tremolo, delay (up to 2 s, the
  repeats darkened by a tone knob) and reverb (Schroeder and Moorer's combs and all-passes with a
  pre-delay), each with presets. A track without its own chain plays its instrument's: the
  overdriven and distorted programs get an amp and cabinet (and a gate) in place of the old fixed
  drive; a sampled instrument plays none, its recording already has its amp. Every effect is a
  table of parameters (`EffectType::params`), so validation, the commands and the knobs share it.
  The mixer's equalizer (`Track::eq`) has Guitar Pro's five bands: low shelf, low-medium and
  high-medium peaks with their frequencies, a medium peak with its Q, high shelf, an on switch
  and presets for instrument types. The master (`Score::master`) has volume, pan and the three
  mastering slots in Guitar Pro's fixed order: compressor, reverb, ten-band equalizer, off by
  default so older files sound as they did. Commands: `track.setEffect`, `track.removeEffect`,
  `track.moveEffect`, `track.resetEffects`, `track.setEq`, `score.setMaster`,
  `score.setMastering`, and the queries `track.sound` and `sound.effectTypes` (every type's
  parameters, ranges, units and presets). The inspector's Sounds section lists the chain (icon
  opens the knobs, the name opens the library of presets and types, power, Move Up, Move Down,
  Delete, and Reset to the instrument's chain); the Song tab's Sound Mastering rows are live; the
  global view's Eq. button opens the equalizer popup (lit while it shapes the sound) and the
  Master row's fader and pan work; the Mixer window has an EQ button per strip and a master
  fader; Sound › Sound Settings… opens the track inspector. Knobs are heard while they turn and
  make one undo step when let go; a knob moved during playback keeps the reverb's tail. WAV and
  FLAC exports render the effects and wait for their tails. The synth runs its effects in mono
  until a chorus or reverb widens it: rendering a six-track 400-bar score costs 30 ms a second
  of sound (budget 50). Not yet: cabinet impulse responses from files, the chain in Guitar Pro
  files, master automation, the pedalboard; nobody has listened on speakers.
- **Landed 2026-10-18:** articulations (#111, #112). The synth plays each one
  (`crates/playback/src/synth.rs`, `player.rs`): palm mutes put a lowpass in the string loop and
  die in 0.7 s to 0.15 s by the track's palm-mute weight, with the filter's delay taken off the
  string so the pitch stays true; dead notes are a 45 ms click; harmonics excite the string
  through a comb that keeps only the partial's multiples, so they ring at the partial (natural
  harmonics by the node at the fret, pure: 2 at the 12th, 3 at the 7th and 19th, 4 at the 5th, 5
  at the 4th, 9th and 16th …; artificial and tapped an octave over the fretted note; pinch and
  semi two octaves over it with some of the note) and longer than a fretted note at that pitch;
  shift and legato slides glide over the end of the first note and arrive on the second, which a
  shift slide picks again; slides in scoop from three semitones away, slides out fall or rise
  eight while fading; hammer-ons, pull-offs and taps carry on the string with a little of the
  finger's energy (a tap on a silent string plucks it without a pick click); staccato plays half
  the note; slap and pop, and harder notes, are brighter. A note's end now fades it by 60 dB in a
  quarter second at any pitch, where a low string used to ring for seconds after its note. The
  timeline carries what a harmonic sounds (`PlayedHarmonic`, `Harmonic::sound`,
  `natural_harmonic`) and the beat's technique, keeping `pitch` the fretted note. A sound bank's
  role can name a program per articulation (`sustain`, `palm_mute`, `dead`, `harmonic`,
  `legato`), each a file or a keyswitch held in one (`parse_switched`); each loads only the keys
  its notes play (`used_keys_by_articulation`), and an articulation without one plays from the
  sustain program, as before for palm mutes and dead notes. Without legato samples, a hammer-on,
  pull-off or legato slide moves the sample already sounding to the new pitch instead of starting
  a softer one. MIDI export sounds harmonics at their partial (a semi-harmonic with the note under
  it) and staccato at half length; the three demos with such notes were regenerated. Checked by
  tests: every pitch within 1.2 % (harmonics, palm mutes, slides), decay and release times,
  plucks counted, programs chosen. Not yet: the libraries `cargo xtask sounds` installs are
  mapped for sustain only, bends are not written to MIDI as pitch bends, and nobody has listened.

### G6. Editing depth
- **Landed 2026-10-16:** tuplets 3:2 to 10:8 (exact at 20160 ticks per quarter), clear bar, double
  bars, free time, triplet feel (played), duplicate and reorder tracks, wide vibrato, heavy accent,
  tapping, slap/pop, pick strokes, fade in/out and swell, brush, arpeggio and rasgueado (played);
  alternate endings and directions (segno, coda, Fine, D.C./D.S./To Coda) edited, played, drawn
  and imported from `.gp`.
- **Landed 2026-10-16:** `track.transpose` (±24 semitones; a note that runs off its string moves to
  the nearest free one; trills and grace notes follow; nothing changes unless every note fits).
  Track › Transpose Track… and Edit › Transpose… (any interval, for the selection, the track or
  every track but drums; `edit.transpose`, `track.transpose` with `tracks`); chord diagrams are
  not transposed.
- **Landed 2026-10-17:** bend curves and the Bend window. A note's bend is a curve of two to eight
  points (`Bend` in `crates/score/src/bend.rs`: positions 0 to 60 along the note, heights in quarter
  tones up to three tones), always valid by construction and saved as a plain height when it is the
  old full-step shape, so older `.gt` files load. Guitar Pro's seven types (bend, release,
  bend/release, hold, prebend, prebend/bend, prebend/release) are presets of it. `note.setBend`
  takes a height with a type, or the points (one undo step); Effects › Bend… (B) opens the Bend
  window: the seven types, a height from ¼ to 3, and a graph whose points drag, add (double-click the
  graph) and remove (double-click a point). The tab draws every curve as Guitar Pro does: a curved
  arrow up with its height (¼, ½, ¾, full, 1¼ … 3) over each rise, a downward arrow for a release, a
  vertical arrow from the fret for a pre-bend, a line for a hold. Playback follows the curve (synth
  and samples, from the first frame, so a pre-bend starts bent), and a tie carries it on: the next
  note holds the pitch or follows its own curve (a release). Shown in `snapshot --bends [--bend]`.
- **Landed 2026-10-17:** the whammy bar and vibrato with the bar. A beat's whammy is a curve of the
  bend's kind (`Whammy` in `crates/score/src/whammy.rs`; the two share `curve.rs`): positions 0 to
  60 along the beat, values in quarter tones from an octave down to an octave up. Six shapes are
  presets of it (dive, dip, return, hold, pre-dive, pre-dive and dive), recognised again when a
  curve matches one. `beat.setWhammy` takes a depth with a shape, or the points, for the beat or
  the selection (rests skipped, drums refused); `beat.setBarVibrato` sets slight or wide vibrato with
  the bar. Effects › Whammy Bar… and the palette open the Whammy Bar window: the shapes (Hold and
  Return only after a beat that uses the bar), a depth from −6 to +6 tones, and a graph with the
  written pitch on its middle line. The palette's two bridge-vibrato buttons are live. A track that
  uses the bar gets a lane above the other effects: "w/bar" where a run starts, the curve dropping
  for a dive and rising for a pull, the depth (−¼ … −6, +½ …) where it turns, and a wavy line for
  the vibrato. Playback moves every note of the beat along the curve, on top of any bend: at rest
  before it, held after it, joined through ties, shared by tremolo picks; vibrato with the bar
  shakes the beat's notes. Shown in `snapshot --whammies [--whammy]`.
- **Landed 2026-10-17:** the clipboard as Guitar Pro 8's manual describes it (`clipboard.rs`, moved out
  of `selection.rs`). A one-bar clip overwrites from the cursor; a clip of several bars fills the
  empty bars from the cursor's first, then inserts new bars (before the next bar with notes, with
  the cursor bar's meter and key); between drum and pitched tracks only the rhythm is pasted, as
  rests. All-Track Copy and Cut (⇧⌘C, ⇧⌘X; `edit.copyAllTracks`, `edit.cutAllTracks`) take whole
  bars of every track with their headers; pasting them inserts the bars on every track (a track
  that cannot hold a bar's notes gets its rhythm), and cutting every bar leaves one empty bar.
  Edit › Paste Special… (`edit.pasteSpecial`): 1 to 100 times, Replace (from the cursor, a
  fragment that does not fit moving to the next bar) or Insert (new bars before the cursor's bar),
  into any tracks; an all-track clip always inserts on every track. A paste may add at most 10,000
  bars. Tools › Check Bar Durations (F4) opens a window listing the track, bar and voice of every
  bar that does not fill its time signature, too short or too long in quarters; a click puts the
  cursor there and Check Again reruns it. The menu catalog gains GP8's All-Track Copy and Cut.
- **Landed 2026-10-17:** `edit.unset` (`unset.rs`), the command palette's "unset": one effect by name
  (any note or beat effect, ties, dead notes, chords, text, dynamics, slurs, beams), `effects` for
  every effect, or `all`, from the selection or the cursor's beat in one undo step. The stylesheet
  option "show the pitch bends reach" (`PageSetup.bend_pitch`, `score.setPage bendPitch`, a check
  box in Page Setup; off as in Guitar Pro's default): standard notation draws a small head at the
  bend's peak after the bent note, slurred to it, with its accidental, or a quarter-tone sharp for
  an odd number of quarter tones (`snapshot --bends --bend-pitch`). A bend held or let down
  through ties already played and drew as Guitar Pro does; a test now proves Guitar Pro 5 keeps
  it.
### G7. Tools
- **Landed 2026-10-18:** the tuner (toolbar tuning fork, Tools › Tuner): audio input
  (`guitario_audio::AudioIn`), a needle for one string and a bar per string of the track's tuning
  for a strum, as Guitar Pro 8 draws them; `tools.tune`. Within ±2 cents on test tones from a
  five-string bass's low B to the top of the neck.
- **Landed 2026-10-18:** the Scale tool (Tools › Scale Tool…, in Guitar Pro 8's layout): a library
  of 68 scales, modes, arpeggios and tetrachords with tags and a search, key note and spelling,
  the scale's notes, steps and intervals, a play button; the scale shown on the fretboard and
  keyboard views; Find Scales from Selection (`tools.findScales`), on every key note or one.
- **Landed 2026-10-18:** the Chord window (Note › Chord…, `A`, in Guitar Pro 8's layout): build a
  chord from root, type, extension, altered 5th/9th/11th/13th, bass or inversion and omitted
  tones; every diagram that plays it on the track's tuning (any string count, capo), fingered
  with barres and sorted by difficulty (`guitario_theory::voicings`, `chord.build`); a play button;
  built chords keep their structure and transpose. Guitar Pro 8's shortcuts for accents (`;` and
  `:`) replace `A`, which opens the window.
- **Landed 2026-10-16:** Tools › Chord Diagrams…: a clickable diagram for the beat under the
  cursor (fingers, open and muted strings, first fret), filled from the beat's notes and named by a
  chord namer (`guitario_theory::chord_name`: triads, power chords, sixths, sevenths, ninths,
  elevenths, suspensions, altered sevenths and voicings without the fifth, the bass note as root
  first, else a slash chord). `beat.chordFromNotes` does the same headless.

### G8. UI depth
- **Landed 2026-10-16:** View › Layout › Page, Screen and Horizontal, and View › Multitrack: every
  track stacked in each system with its beats lined up in time across tracks, the bar's markers and
  directions over the top track, a system line joining the tracks, and clicks on any track moving
  the cursor there. Both are saved in the preferences. Printing and PDF stay one track at a time.
- **Landed 2026-10-17:** File › Preferences… (theme dark or light, interface scale 75–200 %, saved);
  the real app now opens the persistent preferences store (it had been running on an in-memory one,
  so nothing was saved and autosave and crash recovery were off).
- **Landed 2026-10-17:** View › Full Screen (F11), Window › Minimize, Help › Keyboard Shortcuts (a
  window generated from the menu catalog).
- **Landed 2026-10-17:** View › Instrument View › Keyboard (C2-C7, click a key to enter a pitch,
  lit for the beat; `keyboard.rs`).
- **Landed 2026-10-17:** the toolbar in Guitar Pro 8's order and groups (home, panels, zoom and
  display-mode spinners, undo/redo, print, transport, LCD, loop and relative speed, relative
  tonality, audio track, instrument views, tuner, line-in, Fretlight), right-end groups dropping
  out on narrow windows. The LCD has Guitar Pro's two rows: track colour, icon and name (click to
  pick a track), count-in, metronome and their settings, the note under the cursor; bar, bar
  check (written against expected beats, green, orange or red), time, triplet feel, tempo (click
  for the tempo automation) and time signature over a progress line. Relative tonality shifts
  playback ±12 semitones (synth and samples, live) without touching the score.
- **Landed 2026-10-17:** the global view in Guitar Pro 8's columns: a header (add track, track
  menu, move down/up, multitrack, mute, solo, Vol., Pan., Eq.), rows with the instrument icon, an
  eye that hides the track from the multitrack view, yellow mute and red solo, a fader, a pan knob
  (drag; double-click centres), volume and pan automation flags (green when automated; a click
  opens the editor) and the equalizer; a Master row carrying the sections; square bar cells with
  the cursor's bar marked as in Guitar Pro.
- **Landed 2026-10-17:** the score page as Guitar Pro 8's default stylesheet draws it: bar numbers
  on every bar (red on screen for a bar of the wrong length), the abbreviated track name upright on
  the first system, only the first system indented, a plain T-A-B clef, no time signature in the
  tab under standard notation, the tuning at the top ("Standard tuning" or circled strings), a bold
  tempo, and sections as a boxed letter and a name (framed when bracketed). The Section menu
  (Edit Section…, Next and Previous Section) and Guitar Pro's Section window; section letters kept
  apart from names in `.gp` both ways. Sound Mastering in the Song inspector (greyed); messages as a
  toast instead of a status bar, which Guitar Pro does not have.

### G9. Performance budgets
- **Landed 2026-10-16:** `cargo xtask perf` and `perf/budgets.toml`: a 400-bar, six-track score
  timed in release through opening, saving, `.gp` import, layout, page-view frames (single and
  multitrack, tessellation included), the playback timeline, the player and audio rendering, and
  the sound bank. Everything fits: a frame takes 1-2 ms, the synth 3 % of a core. The one stall
  found, decoding samples on the UI thread when Play was pressed (5.5 s), is gone: samples decode
  on all cores (1.0 s) on a worker thread whenever the score needs new notes, an instrument is
  reused for any subset of the notes it was decoded for, and Play waits ("Loading sounds…")
  instead of freezing; Play now costs 2 ms on the UI thread.

### G10. Automation

### G11. Reliability depth
- **Landed 2026-10-16:** a last-resort panic guard in `Session::run` (a panicking command puts
  everything back and reports an internal error; REL-4), and autosave with crash recovery (every
  30 s while unsaved, a heartbeat per running Guitario, a Recover / Discard / Later prompt at start,
  `file.recover`; REL-6).
- **Landed 2026-10-16:** mutation fuzzing of every Guitar Pro reader (`crates/gp/tests/fuzz.rs`,
  in every test run; `GUITARIO_FUZZ_CASES` for depth; 150,000 cases found nothing).

