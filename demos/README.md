# Guitario demo suite

Original metal compositions for trying the editor, the notation and playback. Nothing here is
copied from existing songs. Every band piece has a drum part.

Open any `.gt` file with **File → Open** (or `cargo run -p guitario -- demos/01-thrash-attack.gt`),
put the cursor in a bar and press **Space**. Each score also comes as a Standard MIDI File in
`midi/`, plain-text tab in `tab/` and a Guitar Pro 5 file in `gp5/` (opens in Guitar Pro and most tab
software).

| File | Style | Tempo | Tuning | What to look at |
|---|---|---|---|---|
| `00-blank` | Template | 120 | Standard | Eight empty bars to start from |
| `01-thrash-attack` | Thrash | 190 | Standard | Downpicked sixteenth chugs in E phrygian, power-chord hits, a hammer-on pentatonic run, repeats |
| `02-funeral-procession` | Doom | 56 | Drop C | Whole-note chords with let ring and vibrato, a slow bending lead, three tracks |
| `03-burial-ground` | Death metal | 200 | Drop B | Thirty-second tremolo chugs, chromatic descents, a half-time breakdown with a tempo change |
| `04-steel-horizon` | Power metal | 168 | Standard | Twin harmony leads in thirds, sweeping arpeggios, galloping rhythm guitar and bass (four tracks) |
| `05-seven-strings-deep` | Djent | 128 | 7-string, drop A | A 3+3+3+3+4 polyrhythm in sixteenths, a half-time breakdown, a pinch harmonic |
| `06-odd-meter-assault` | Progressive | 150 | Drop D | 7/8, 5/4, 13/16, 6/8 and 12/8, tempo changes and a key change |
| `07-iron-gallop` | Classic heavy metal | 168 | Standard | Harmonised twin leads in A minor over a galloping bass |
| `08-frostbitten-dawn` | Black metal | 190 | Standard | Tremolo-picked melody in F# minor (three sharps), open-fifth tremolo chords; each tremolo is one picked quarter note (thirty-second picking), not eight written notes |
| `09-breakdown-protocol` | Metalcore | 140 | Drop B | Syncopated palm-muted chugs, dead notes, slide-out, pinch harmonic |
| `10-ashes-to-distortion` | Ballad into heavy | 84 → 112 | Standard | A fingerpicked nylon intro with chord names that the distorted band joins at bar 5 |
| `11-fretboard-fury` | Shred solo | 140 | Standard | Bends, legato runs, a shift slide, natural harmonic, triplet pentatonics over a gallop |
| `12-metal-technique-showcase` | Reference | 120 | Standard | One bar per technique: hammer-ons, slides, bends, harmonics, palm mute, tremolo picking at three speeds, trills at three speeds, dead and ghost notes, ties, triplets |
| `13-bass-rumble` | Bass study | 180 | Bass | Pick gallops, a climbing line, a fill, with a rock beat |
| `14-drum-grooves` | Drum reference | 160 → 200 | Drum kit | Skank beat, d-beat, kick gallop, double kick, half time, blast beats, china hits, a triplet tom fill |
| `15-two-voices` | Fingerstyle study | 92 | Standard | Melody in Voice 1 (stems up) over a bass in Voice 2 (stems down) sharing one staff and tab, a note tied over a bar line |

`guitar-pro/import-test.gp` is a small hand-built Guitar Pro 7 file used to test the importer
(drop D, repeats, a tie, a slide, a bend, a harmonic, shared objects and a grace note). Open it from
the File menu to see Guitar Pro import work.

## Regenerating

The scores are written in a compact tab notation in
[`crates/engine/examples/demos.rs`](../crates/engine/examples/demos.rs) and entered through the
engine's own commands, so they double as an editing test:

```sh
cargo run -p guitario-engine --example demos      # rewrites demos/*.gt, midi/, tab/ and gp5/
cargo run -p guitario-gp --example fixture        # rewrites guitar-pro/import-test.gp
cargo test -p guitario-engine --test demos        # checks every demo loads, plays and is metal
```

Render any of them to audio with the CLI:

```sh
cargo run -p guitario-cli -- convert demos/04-steel-horizon.gt steel-horizon.wav
```
