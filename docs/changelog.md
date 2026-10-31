# What has landed

> **Last reviewed:** 2026-10-17 · **Last updated:** 2026-10-31 · **Change:** minor (G1: the drum step grid) · **Target:** Guitar Pro 8

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
- **Landed 2026-10-31:** the step-grid drum editor (#98). On a drum track the instrument view's
  gear offers **Pads** or **Step grid**; the grid shows the cursor's bar as ten kit pieces (and any
  other piece the bar uses) × steps of an eighth, sixteenth or thirty-second, straight or in
  triplets, with beat numbers, the cursor's and the playing beat's column lit, and a count of hits
  between steps. A click runs `drums.setStep` (`drum`, `step`, `on?`, `measure?`, `resolution?`,
  `triplet?`), one undo step a cell: a step where no beat starts splits the beat that covers it
  into plain, dotted or triplet lengths, and a bar not yet full is padded with rests, so the
  groove reads as ordinary beats in the tab and notation. `drums.grid` returns the same grid as
  JSON for agents (`crates/engine/src/drum_grid.rs`, `crates/ui-egui/src/drum_grid.rs`; snapshot
  flag `--drum-grid`).

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
- **Landed 2026-10-18:** File › Stylesheet… (F7), Guitar Pro 8's window with its Page & Score
  Format and Header & Footer tabs (File › Page Setup… opens the first). Headers and footers are
  `Score::header_footer` (`crates/score/src/header_footer.rs`, `score.setHeaderFooter`,
  `score.headerFooter`): the first page's title, subtitle, artist, album, words, music, words &
  music, transcriber, copyright, its second line and page number, and the other pages' header,
  copyright lines and page number, each with an on switch, a template of tokens (`%TITLE%` …
  `%PAGE%/%PAGES%`, `%WORDS&MUSIC%`), alignment, font, size, bold and italic. The defaults are
  Guitar Pro's, taken from the `BinaryStylesheet` of 94 real files (title 23 pt, artist 12 pt bold,
  credits 9 pt, footer 7 pt; the transcriber and the other pages' header off). A text whose tokens
  are all empty prints nothing; the words and the music share a line; words & music replaces both
  when one person wrote them; the second copyright line prints only with a copyright; left and
  right swap on even pages. The page view, the PDF and the printer draw them the same, and the
  header's height feeds pagination. Printing selected bars: `bars: [first, last]` on
  `file.exportPdf` and `file.print`, and a "Selected bars" choice in File › Print… (chosen by
  default when the selection spans bars); the first printed bar shows its time signature and the
  bars keep their numbers. Paper: A3, A4, A5, A6, B4, B5, B6, Letter, Legal, Tabloid, Executive,
  Statement or a custom size (100 to 1,000 mm a side); Guitar Pro files with another size open as
  a custom paper instead of A4. Checked by rasterizing exported pages.

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
- **Landed 2026-10-18:** the sampler's depth (#103, #104, #106, #108, #109;
  `crates/playback/src/sfz/`). Release regions (`trigger=release`) start when a note ends, quieter
  by `rt_decay` dB for each second it was held, and not after a let-ring note or a string struck
  again before the note's end; a hammer-on moved onto the next note carries its release along.
  `trigger=first` regions sound only when the note is picked, `trigger=legato` regions only for
  hammer-ons, pull-offs and legato slides, so a library's own legato samples play them. Each
  voice can run a filter: one- to six-pole low- and high-passes, band-pass and band-reject, its
  cutoff moved by `cutoff_onccN` (in cents), the key, the velocity and its own AHDSR envelope
  (`fileg_*`, recomputed every 32 samples), with resonance in dB over a Butterworth response.
  `amp_velcurve_N` points replace `amp_veltrack` with a curve. A WAV file's `smpl` loop is used
  where the SFZ names none, and loops the region unless it says `no_loop`; `loop_crossfade` fades
  a loop's end into what comes before its start, so the seam does not click. OGG Vorbis samples
  decode through lewton (pure Rust), capped like the others, a decoder failure contained as an
  error; a generated fixture and 300 corrupted copies check it. The amplitude and filter
  envelopes share one generator. Tests measure each: a 500 Hz low-pass takes a 4 kHz tone below
  5 %, the envelope opens it four octaves and closes it in half a second, curves give their
  gains, a ramp's loop seam drops from 0.8 to under 0.02 with a 2 ms crossfade, release noises at
  their key and 30 dB quieter after a note held 1.5 s longer. Not yet: streaming from disk, LFOs,
  EQ and pitch-envelope opcodes, `<curve>` headers.
- **Landed 2026-10-18:** the sound bank in the app (#113, #114, #275). One installer
  (`crates/playback/src/install.rs`) serves the app, the engine and `cargo xtask sounds`: it
  downloads with `curl` (resuming a download that stopped, starting afresh when the address
  changed), unpacks zip and 7z archives in Rust (`zip`, and `sevenz-rust2` with its pure-Rust
  bzip2 and deflate), clones the drum kit with `git` or downloads GitHub's zip of it, writes each
  library's licence and credit beside it, and merges `bank.json` so a user's own entries stay.
  The **Sound Bank** window (Sound › Audio Settings… › Sound Bank…, and the inspector's Bank row)
  shows the folder, what plays each kind of track, each library with its licence, size and an
  Install button, Install All with progress and Cancel, and the credits the installed libraries
  need; installing runs on a worker thread and the bank is reopened when it ends. At first start
  without a bank, the desktop app offers to download them (Download, Not Now, Don't Ask Again,
  remembered in the preferences). A track can name its instrument (`Track::bank`, saved in the
  score and left out while unset): a role or an `.sfz` file, chosen from the inspector's new Bank
  row or with `track.setBank`; a role the bank lacks or a missing file plays the synth and says so.
  `sound.bank` reports the bank (folder, roles, libraries, credits) and `sound.installLibraries`
  installs named libraries for agents and scripts. The bass now plays Growlybass's `dirty` program,
  sustain with release noises; of the four libraries it is the only one that records more than
  sustain. Checked by installing three libraries for real (Growlybass's zip, both FreePats 7z,
  which needed the bzip2 and deflate codecs), by file:// installs in the tests (zip, 7z, a
  resumed download, stale and broken archives, cancel), and by renders of the window, the offer
  and the inspector. Computing what a score needs from the bank once per Play and finding legato
  notes in timeline order made handing instruments to the player 2.9 ms (was 12.3) and the check
  after an edit 3.0 ms on a 400-bar score. Not yet: the drum kit's download from inside this
  build's sandbox (GitHub's repository archives were not reachable), a memory budget.
- **Landed 2026-10-18:** master volume and pan automation (the rest of #265): points on the master
  bar (`MeasureHeader::master`, left out of files while empty, not copied to new bars), jumping or
  gliding like a track's, `score.setMasterAutomation`, played over the master's own volume and pan
  from wherever playback starts (`Score::master_curves`, the player's master ramps); the global
  view's Master row has live A buttons, lit while automated, that open the Volume and Pan
  Automation window on the Master (which its track list also offers). Samples are held as 16-bit
  values: the 400-bar, six-track perf score's decoded guitars and bass went from 1,000 MB to 500
  MB, recorded as `sounds_memory_mb` in the perf budgets (#110, first half), and a real Growlybass
  render is unchanged. Not yet: streaming long samples from disk; master automation in MIDI and
  Guitar Pro files (track automation is not exchanged either).
- **Landed 2026-10-18:** streaming from disk (the rest of #110). A sample longer than 2 s keeps
  its first second in memory and a `Stream` for the rest (`crates/playback/src/sfz/stream.rs`),
  which decodes the file again and keeps the frames after the head. A voice on the sound card's
  thread (`Player::set_live`, set by the app's players) asks the streamer thread for its tail
  0.6 s of sample before the head ends, takes it without waiting (`try_lock`), and plays silence if
  it is late, counting it; offline players (exports, tests, the CLI) load the tail when the note
  reaches it, so their output is bit for bit what the whole sample gives (tested, with
  resampling and transposition). The cache keeps tails up to 256 MB (`GUITARIO_STREAM_CACHE_MB`),
  drops the least recently used first and anything idle for a minute or whose instrument is gone,
  and never a tail a voice is playing; a file that disappears fails once, not on every note.
  Measured: the 400-bar perf score's instruments take 25 MB instead of 500 MB
  (`sounds_memory_mb` budget 1,000 to 100), the live render costs 30-35 ms per second of sound
  (budget 100), and four demos played at real speed for 15 s from a cold cache had no late tail,
  with at most 118 MB of tails and the worst 512-frame block at 6.3 ms of its 10.7 ms.
- **Landed 2026-10-18:** a modelled grand piano and the bowed strings (PLAY-11), played for the
  General MIDI programs that name them (`crates/playback/src/instrument.rs`): grand, bright,
  electric grand and honky-tonk pianos (0 to 3); violin, viola, cello and double bass (40 to 43,
  110 fiddle); tremolo, pizzicato and two string-section programs (44, 45, 48, 49), whose notes
  are played by four players each, a little apart in tuning, time and place, on the body that
  suits their pitch. The inspector's and Add Track's Sound lists offer them; Guitar Pro piano and
  string tracks play them. No samples are involved.
  The piano (`piano.rs`) is a bank of modes per key: partials stretched by the strings'
  stiffness (Fletcher's inharmonicity across the keyboard) and a stretched tuning, two or three
  unison strings a fraction of a cent apart that beat, and a slow aftersound under the prompt
  sound for the two-stage decay; losses rise with frequency and vary partial by partial. A felt
  hammer (Chaigne and Askenfelt's masses, stiffness and exponents, with hysteresis) is simulated
  against the moving strings each sample, so loudness and brightness come from how hard it
  strikes (contact 4 ms soft to 2 ms loud at A1, 2 to 1.4 ms at middle C, 1.5 to 0.6 ms at C7; the
  spectral centroid of middle C rises by 40 % from p to ff). A key struck again hits its sounding
  strings; letting go drops the damper (60 dB in a fraction of a second; the top keys have none);
  notes that let ring hold the sustain pedal, and every string from A0 to C7 then rings in
  sympathy in the soundboard. Bass keys left to the left, treble to the right; the hammer's knock
  sounds in the case; partials that have died away are dropped from the work.
  The bowed string (`bowed.rs`) is a waveguide (cubic Lagrange delays, the bridge's losses per
  second) bowed through a hyperbolic friction junction solved exactly each sample with the
  hysteresis rule, so attack, bow changes and rosin noise come from the stick-slip motion: one
  full slip per period from 65 Hz to 2.6 kHz at every dynamic (tested), pitch within 4 cents with
  the voice correcting its own stopped length by timing its slips. The player is modelled too:
  louder is a faster bow, nearer the bridge and harder; each new stroke changes bow direction
  (through standstill on a sounding string); notes under a slur (`PlayedBeat::slurred`, new) or a
  hammer-on are one stroke, crossing strings without a new bite; vibrato starts a moment into
  held notes (about 0.2 semitone at 5.4 to 5.8 Hz, a little irregular) and never on an open
  string; open strings ring on after the bow leaves.
  Bodies (`body.rs`, one per track and kind) are banks of resonances: the violin's signature
  modes (A0, CBR, B1-, B1+) over dense irregular ones under a bridge hill, scaled for viola, cello
  and double bass, and a piano soundboard; each mode radiates left and right differently, and the
  violin family's four open strings ring in sympathy. The piano gets a studio room and the strings
  a hall as their default chain (`default_chain`). Levels sit with the plucked synth's. Measured:
  the 400-bar perf score with its pitched tracks as piano, string section, violin and cello renders at 161 ms of work per second of sound (budget `render_modelled_per_second` 250; the band synth stays at 37). `cargo run -p guitario-playback --release --example audition -- <dir>` renders
  original phrases for each instrument to WAV. Checked by tests and spectra, not yet by ear.
- **Landed 2026-10-18:** the piano's tuning and tone after a first listen ("too honky-tonk, too
  plastic"). The stretched tuning is now worked out from the model's own strings as a tuner does
  (temperament octave first, then each octave on the lower note's second partial, the bass on its
  fourth): octaves meet within a hundredth of a cent, where the fixed Railsback curve had them up
  to 8 cents apart; a grand's unisons are a quarter of a cent apart, not up to two cents; the
  pedal's sympathetic strings are tuned to the stretched keys and their loops corrected for their
  low-pass (they rang up to 10 cents flat, a detuned halo). Against the plastic sound: a diffuse
  soundboard and case (an eight-line feedback delay network, 0.45 s low, highs five times faster)
  under the discrete resonances, the felt's thud as force-shaped noise (-28 dB of the note in the
  bass, -18 dB in the treble over its first 20 ms), and phantom partials from the strings'
  stretching (-25 dB in the bass at ff, -37 dB at p, none above C6).
- **Landed 2026-10-18:** the piano's bass after a second listen ("the low end still sounds too
  plastic"): its spectrum hardly changed as it rang (A0 and E1 had the same balance of partials at
  2 s as at the blow). Partials now decay at rates rising in proportion to their frequency, from a
  corner of 250 Hz in the bass to 4.5 kHz at the top, as measured on real strings: a bass note starts
  bright and darkens within a second or two, middle C keeps its tenth partial for about 2 s and its
  twentieth for about 1 s (a first try with steeper losses left a bell-like electric piano, the
  third listen's verdict). Each partial's aftersound sits 0.3 Hz (bass) to 0.1 Hz (treble) off its
  prompt sound, so partials beat slowly and shallowly at their own rates; the soundboard radiates
  the lowest fundamentals weakly (a 75 Hz roll-off, E1's fundamental 34 dB under its strongest
  partial).
- **Landed 2026-10-18:** a fourth listen heard the midrange turn bell-like, electric-piano-like, as
  the ballad got louder. Found by rendering it with each part switched off: the output stage was a
  `tanh` over the whole range, so a loud pedalled passage's sustained partials intermodulated into
  clangorous tones (-36.5 dB in the forte against -45 dB soft); it is now straight up to 0.75 and
  rounds off only above (`soft_limit`). A high partial's aftersound now fades above about 1.2 kHz,
  weaker and shorter, as on real strings (left as strong as a low one, the slightly sharp high
  partials rang on like a bell's: as the last chord rings, the highs are now 45 dB under the mids
  instead of 34). The pedal's sympathetic strings are darker (they rang pure, exactly harmonic
  upper partials, tine-like) and their loops are solved for their pitch exactly. The soundboard
  network's lines are longer and its decay shorter so its resonances overlap rather than ring one
  by one. String sections and pizzicato sit 2 dB lower.
- **Landed 2026-10-19:** the piano is fitted to recordings of two real grands instead of tuned by
  ear: the University of Iowa Electronic Music Studios' Steinway B and the Salamander Grand (a
  Yamaha C5), every minor third from A0 to C8 at pp, mf and ff, measured partial by partial
  (frequency, level at the blow, prompt and aftersound decay) and set beside the model's own keys
  rendered and measured the same way (`audition -- <dir> --keys C4@mf …`). The recordings were
  measured, not used. What changed: inharmonicity is about 1e-4 from the bass to the tenor (it was
  twice that); the stretch follows the partials a tuner listens to further out (10:5 at A0, 3:1.5
  by C7), 22 cents flat at A0 and 9 sharp at C7 as on both grands (2.2 cents rms, from 3.9); a note
  falls 20 to 25 dB in its first second or two and then hangs on in its aftersound (middle C: 2.6 s,
  then 27 s at -22 dB; it rang 6.5 s at one rate), partials fall at about their fundamental's rate
  up to ten kilohertz (the treble's and the upper partials' aftersound had died early), and the low
  partials drain faster than the high ones in the tenor; the spectrum leans 1.6 dB per octave of
  partials darker at ff and 4 dB more per unit of velocity below it (the model was 3 to 4 dB per
  octave too bright at pp and mf, and hardly darkened with softer playing: middle C's centroid now
  460 Hz at pp, 830 at ff), applied at the bridge so the hammer does not sharpen against it; each
  register is as loud against middle C as on the grands. Measured against the mean of the two:
  decay envelopes by band within 3 dB median (8 before; the two grands differ from each other by 4),
  spectra within 5 dB (7 to 9 between the grands). As a note rings its highs (1.5 to 6 kHz) stay 20
  to 30 dB under its mids, as on both grands; the previous change had taken them to 45 to 60 dB.
- **Landed 2026-10-19:** the bowed strings fitted to recordings of real ones (PLAY-11), solo
  first, then sections. The references are VSCO 2 Community Edition (CC0) and Sonatina Symphonic
  Orchestra (CC Sampling Plus 1.0), measured and never bundled (`ATTRIBUTION.md`); the method is
  in [decisions](decisions.md). `cargo run -p guitario-playback --release --example notes -- <dir>
  <program> <midi,...> [dynamics] [seconds]` renders held notes, dry (`WET=1` keeps the hall), to
  set against them. Solo violin, first model → now → recordings: vibrato 5.6 → 5.2 Hz (5.2), 15
  → 8.7 cents at piano and 15 → 13.5 at forte (7.8, 14.3); forte 4.8 → 12–13 dB louder than piano
  (12.7) with its harmonics from the ninth up 0 → 6–7 dB stronger (7); noise between the harmonics
  23 → 31–34 dB under them (31 to 35); the body's response, estimated from every note's harmonics,
  4.1 → 0.5 dB from the recordings' (rms, 250 Hz to 10 kHz; the two libraries are 2.9 dB apart)
  and each note's third-octave balance 7.5 → 2.3 dB; under a high note's fundamental -7 dB at A6 (a growl louder than the note)
  → -27 to -44 (-21 to -40); the open G's fundamental -5 → -23 dB under its strongest harmonic
  (-21 to -30); piano attacks 0.03 → 0.25 s; each harmonic's level swinging 0.9 → 1.7 dB dry and
  2.6 with the hall (2.6 to 4.2 recorded in a room), a third of it slower than 2.5 Hz as the bow
  wanders. Viola, cello and double bass bodies have curves of their own fitted the same way
  (1.2, 2.0 and 2.0 dB rms over each one's range; the two double basses recorded are 4.8 dB
  apart); the double bass vibrates at 3.8 Hz and 4 cents, as recorded. The model changes behind it: the bow nearer the bridge and much faster loudly, a
  shelf on the bridge's force for the sharper corner a harder bow keeps, the bow further along a
  string stopped short and lighter on it high up (no more anomalous low frequencies) and a little
  further out and firmer on a low string drawn loudly, the lowest strings' highs lost a little
  faster (one slip a period from 46 Hz up at every dynamic, over eight draws of the bow's noise;
  the first model slipped irregularly below 100 Hz for some draws and its test tried one), the slip
  placed between samples (a band-limited step), high strings at two or three times the output
  rate and a high-pass under each note's fundamental, rosin noise in the band the string carries,
  the arm's slow wander in the bow's place, speed and weight, body modes spread nearer evenly in
  frequency and sharper, and a Chebyshev cliff below the air mode. Sections, then: recorded ones
  are as good as uncorrelated between left and right (0.05); the model's was 0.72 and is now about
  0 (its cellos 0.2; recorded cello sections 0.1 to 0.5), its left and right halves playing through their own bodies (the same modes, radiated
  differently, the far side through a few milliseconds of all-passes); its players are further
  apart in tuning (line width 10.3 cents, recorded 9.9 to 13.9) and time (up to 40 ms on long
  notes), vibrate narrower (heard as one, 8.3 → 5.5 cents; recorded 5.2 to 5.8) and swell in more
  slowly; its tone is 2.4 dB from the recorded violin sections' and 3.2 from the cello sections'
  (rms; the two libraries are 2.1 and 5.1 dB apart). Tests hold the model to the
  recordings: forte's level and brightness, nothing growling under high notes, the open G's weak
  fundamental, vibrato with the dynamic, a section wide and a soloist narrower, high strings
  oversampled. Levels: a forte bowed note sits where it did (piano is now 12 dB under it, not 5),
  fortissimo a few dB over forte, mezzo-forte 3 dB lower than before on every instrument (their
  balance kept by a trim per body); the auditions' levels are within 3 dB of what they were. The perf score's modelled render costs 224 to 244 ms of work per second of sound
  (budget 250; it varies from run to run) (the piano's retuning had already taken it from 161 to about 220; a string is now cheaper,
  worked out partly every 8 samples, and sections dearer). Still not judged by ear.

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

### G12. Platforms and distribution
- **Landed 2026-10-16:** the app icon in every format (`assets/app-icon`: `.icns`, `.ico` in the
  Windows executable, the hicolor theme, the window icon) and CI on macOS, Linux and Windows.
- **Landed 2026-10-18:** packages (`cargo xtask package [--target <triple>] [--no-build]`,
  templates in `packaging/`, output in `dist/`). macOS: `Guitario.app` with both binaries, the
  icon and an `Info.plist` that declares the Guitario score type (`ai.storyteller.guitario.score`
  for `.gt`, owned), Guitar Pro (`.gp` `.gpx` `.gp5` `.gp4` `.gp3`), MIDI and
  MusicXML, and the microphone use the tuner needs, zipped with `ditto` and as a `.dmg`. Linux:
  a `.tar.gz` (binaries, desktop entry, MIME types `application/x-guitario` and
  `application/x-guitar-pro`, AppStream metadata, icons, `install.sh` for `~/.local` or a prefix,
  `--remove`) and a `.deb` (depends only on ALSA; the windowing and GPU libraries are loaded at
  start). Windows: a `.zip` and an Inno Setup installer (`packaging/windows/guitario.iss`):
  per-user by default, `.gt` owned, the others under Open With, Guitar Pro files optionally made
  Guitario's. The window's app id is `ai.storyteller.guitario`, so Linux desktops match it to the
  desktop entry. Signing is built in and runs when its variables are set: `codesign` for the
  hardened runtime with `packaging/macos/entitlements.plist`, `notarytool` and `stapler` for the
  app and the disk image, `signtool` with a `.pfx` or a certificate thumbprint on Windows.
  Checked here: the `.deb` installs on Ubuntu 24.04 (`dpkg -i`; the MIME database learns both
  types, `guitario-cli` runs) and removes cleanly, `install.sh` installs and removes under a
  prefix, `desktop-file-validate` and `appstreamcli validate --pedantic` pass. The release
  workflow (`.github/workflows/release.yml`): a tag `v<version>` (checked against the workspace
  version) builds the packages for Apple silicon and Intel macOS, Linux x86-64 and Windows x86-64
  and drafts a GitHub release with them and `SHA256SUMS`; a pull request that changes the
  packaging, or a run by hand, builds them as artifacts. Everything compiles before the signing
  secrets enter the environment.

