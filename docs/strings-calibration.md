# Bowed strings against recordings of real players

> **Last reviewed:** 2026-10-19 · **Last updated:** 2026-10-19 · **Change:** major (first calibration) · **Target:** Guitar Pro 8

The violin family (`crates/playback/src/bowed.rs`, `body.rs`) is a physical model, and a model
can sound plausible while being measurably unlike the instrument. This page records how it was
compared with recordings of real soloists and sections, what that showed, and what changed. The
comparison is repeatable; the recordings are **not** part of Guitario and are fetched only to
measure against.

## The recordings

| Library | Licence | What it gave |
|---|---|---|
| [Sonatina Symphonic Orchestra](https://github.com/peastman/sso) (Mattias Westlund, maintained by Peter Eastman) | Creative Commons Sampling Plus 1.0 | Sustained vibrato notes of a solo violin, viola, cello and double bass and of full sections (1st violins, violas, cellos, basses) **at the same pitches**: 64 notes, solo and section, the main reference |
| [VS Chamber Orchestra: Community Edition](https://github.com/sgossner/VSCO-2-CE) (Versilian Studios) | CC0 | Chamber-sized violin, viola and cello sections and a solo violin: a second opinion with other players, room and microphones |

Fetch only the sustain samples, without the rest of either library (a few tens of megabytes):

```sh
git clone --filter=blob:none --no-checkout https://github.com/peastman/sso refs/sso
cd refs/sso && mkdir -p ../flat
for f in $(git ls-tree -r HEAD --name-only | grep -E 'Samples/(1st Violins|Violin|Violas|Viola|Celli|Cello|Basses|Bass)/' \
           | grep -E -- '-sus-|/(violin|cello)-[a-g]' | tr ' ' '?'); do
  f=${f//\?/ }; git show "HEAD:$f" > "../flat/$(basename "$f")"
done
```

Then compare (the model renders the same notes, alone and by an orchestra's section):

```sh
cargo run -p guitario-playback --release --example strings_compare -- refs/flat
cargo run -p guitario-playback --release --example audition -- out ref   # the model's notes as WAV
```

## What is measured

Each note is a sustained vibrato note at mezzo-forte; the measures take two and a half seconds
a second after it starts, so the room and the attack matter little:

- **The harmonics' levels** within ±0.45 f0 of each, against the whole note. Recordings and model
  are compared note for note, lined up on the partials below 500 Hz (the rooms and microphones
  differ in level and low end), and the differences gathered in half-octave bands.
- **Fall**: a line through the first sixteen harmonics' levels against log2 of their number, in dB
  an octave; **brightness**: the harmonics' centroid, in harmonics.
- **Noise between the harmonics**: what lies more than 0.25 f0 from every harmonic.
- **Pitch spread**: how wide each strong partial is, in cents (a soloist's vibrato; a section's
  vibratos and tunings together).
- **Wander**: how much the second partial's level moves, 80 ms at a time (a section's players
  beating against each other).
- **Left-right correlation** of the stereo signal.

## What it showed, and what changed

The first comparison (2026-10-19), real against model, medians over the notes:

| | Real | Model before | Model now |
|---|---|---|---|
| Cello harmonics from 650 Hz up | — | 7 to 13 dB too strong | within 3.5 dB |
| Double bass harmonics, 250 Hz to 2 kHz | — | 4 to 15 dB too strong | within 3 dB |
| Viola harmonics, 1.3 to 3.8 kHz | — | 6 to 10 dB too strong | within 4 dB |
| Fall, cello / bass section (dB an octave) | −7.4 / −7.5 | −4.2 / −1.8 | −7.6 / −9.0 |
| Noise between harmonics, solo violin (dB) | −33 | −24 | −32 |
| Noise between harmonics, violin section (dB) | −29 | −25 | −30 |
| Pitch spread, sections (cents: violins, violas, cellos, basses) | 14, 11, 11, 6.5 | 17, 14, 11, 10 | 12, 10, 11, 6 |
| Wander of a partial, sections | 0.38 to 0.44 | 0.38 to 0.44 | 0.37 to 0.48 |
| Left-right correlation, violin section | 0.11 | 0.27 | 0.12 |

What changed in the model:

- **Each instrument's body is voiced on its own** (`VIOLIN_VOICE` … `BASS_VOICE` in `body.rs`):
  the larger bodies were a violin scaled up, which kept the violin's strong high resonances
  ("bridge hill") far too strong for instruments two to five times its size. The cello's and
  double bass's low cut moved down so their lowest fundamentals sound. The voicing changes the
  colour, not the loudness: a bowed string across the instrument's range comes out as loud as
  before. This changes the soloists too: a cello or double bass is darker, as the recordings are.
  A section's four instruments were then chosen again to sound like the soloist's
  (`BodyKind::section_instruments`).
- **Slips no longer snap to the sample clock** (`bowed.rs`): the moment the bow's grip gives way is
  placed between samples, as a band-limited step, which removed most of the hiss between the
  harmonics (about 6 dB).
- **Sections play as sections** (`bowed::Seat`): a section's players use less vibrato than a
  soloist on the same instrument (violins and violas three quarters as much, cellos nearly as
  much, double basses about half), ease into long notes and enter more loosely on them; the
  section is heard by a pair of microphones some 35 cm apart (up to 1 ms between them).

Already right, before and after: the wander of a section's partials (their slow beating) and the
pitch the sum seems to have.

## Still apart

- Viola, cello and double bass **sections are narrower** than the recordings (left-right
  correlation 0.42, 0.53 and 0.43 against 0.18, 0.14 and 0.21): their energy lies lower, where a
  real hall rather than the microphones' spacing decorrelates the two channels, and four bodies'
  radiation decides much of the rest.
- **Attacks**: the recordings' sections take 0.2 to 0.5 s to reach a sustained note's level, the
  model's 0.05 to 0.15 s; these samples are made for long pads and may start softer than
  playing does, so the model was not slowed to match.
- The **violin** is about 6 dB weaker around 950 Hz and its top octave (7 to 11 kHz) weaker than
  the recordings, where their own hiss and air may count.
- A **viola soloist's** lowest notes (C3 to F#3) are brighter than the Sonatina viola's, which
  plays without vibrato; its section is a little darker than theirs.
- None of this replaces listening: these are averages over notes held still.

The tests keep the calibration: `crates/playback/tests/strings.rs` checks each instrument's fall
against the recordings' on the same notes, `body.rs` that a section's instruments stay within a
decibel of the soloist's, and `bowed.rs` that a bowed note is as clean between its harmonics as
the measurement asked.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-19 | major | First comparison with recordings: bodies voiced per instrument, slips between samples, section vibrato, entries and microphones |
