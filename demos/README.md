# Guitario demo suite

Public-domain classical pieces arranged for metal band, for trying the editor, the notation and
playback, plus two original references (techniques and drum grooves). Every composer died long
ago and every arrangement here is our own, written from the original scores; nothing is copied
from a modern edition, a recording or someone else's tab. Every band piece has a drum part.

Open any `.gt` file with **File → Open** (or `cargo run -p guitario -- demos/01-flight-of-the-bumblebee.gt`),
put the cursor in a bar and press **Space**. Each score also comes as a Standard MIDI File in
`midi/`, plain-text tab in `tab/`, a Guitar Pro 5 file in `gp5/` (opens in Guitar Pro and most tab
software) and MusicXML in `musicxml/`.

| File | Piece | Arrangement | Tempo | Tuning | What to look at |
|---|---|---|---|---|---|
| `00-blank` | Template | | 120 | Standard | Eight empty bars to start from |
| `01-flight-of-the-bumblebee` | Rimsky-Korsakov, *Flight of the Bumblebee* (1900) | Thrash | 150 | Standard | The chromatic line in sixteenths over palm-muted chugs, a repeat, a tom fill |
| `02-funeral-march` | Chopin, *Funeral March* from the Piano Sonata No. 2 (1839) | Doom, in C minor | 56 | Drop C | Let-ring chords rocking between C minor and A-flat, the march as a lead with a closing bend |
| `03-hall-of-the-mountain-king` | Grieg, *In the Hall of the Mountain King* (1875) | Death metal | 112 → 200 | Drop B | The theme palm-muted in B minor, the F-sharp major answer, then the reprise tremolo picked over blast beats |
| `04-summer-presto` | Vivaldi, *Summer*, Presto, from *The Four Seasons* (1725) | Power metal | 150 | Standard | Twin leads in thirds over repeated-note pedals and falling scales, 3/4 gallops (five tracks) |
| `05-toccata-and-fugue` | Bach, *Toccata and Fugue in D minor*, BWV 565 | Djent | 60 → 128 | 7-string, drop A | The call with its fermatas in two octaves, chugged low, then a 3+3+3+3+4 figure and a pinch harmonic |
| `06-mars` | Holst, *Mars, the Bringer of War* (1914–16) | Progressive | 150 | Drop D | The 5/4 ostinato (triplet, quarters, eighths), building from one string to power chords |
| `07-little-fugue` | Bach, *Little Fugue in G minor*, BWV 578 | Classic heavy metal | 100 | Standard | The exposition: the subject alone, then the answer under the countersubject, over a galloping bass |
| `08-moonlight-sonata` | Beethoven, *Moonlight Sonata*, Presto agitato (1801) | Black metal | 150 | Standard | Rising arpeggios in C# minor (four sharps), then the harmony tremolo picked: each tremolo is one picked quarter note (thirty-second picking), not eight written notes |
| `09-fifth-symphony` | Beethoven, *Symphony No. 5*, first movement (1808) | Metalcore | 108 → 140 | Drop B | The motto in 2/4 with fermatas and a tie, then a half-time breakdown with dead notes and a pinch harmonic |
| `10-lagrima` | Tárrega, *Lágrima* | Ballad into heavy | 72 → 112 | Standard | The E major half on nylon strings with chord diagrams, then the E minor half as a distorted lead with slides |
| `11-caprice-24` | Paganini, *Caprice No. 24* (1817) | Shred | 112 | Standard | The theme in 2/4 with pull-offs, a bend, a shift slide and a long fermata at the end |
| `12-metal-technique-showcase` | Original reference | | 120 | Standard | One bar per technique: hammer-ons, slides, bends, harmonics, palm mute, tremolo picking at three speeds, trills at three speeds, dead and ghost notes, ties, triplets; then 7/8, 13/16, 6/8 and 12/8 |
| `13-cello-prelude` | Bach, *Cello Suite No. 1*, Prelude, BWV 1007 | Bass study | 84 | Bass | The opening bars on a picked bass, an octave below the cello |
| `14-drum-grooves` | Original reference | | 160 → 200 | Drum kit | Skank beat, d-beat, kick gallop, double kick, half time, blast beats, china hits, a triplet tom fill |
| `15-bourree-in-e-minor` | Bach, *Bourrée in E minor* from the Lute Suite BWV 996 | Fingerstyle | 120 | Standard | Melody in Voice 1 (stems up) over the bass in Voice 2 (stems down) on one staff, a one-beat pickup bar and a three-beat last bar |

The notes come from public-domain sources: the [Mutopia Project](https://www.mutopiaproject.org/)
editions of the Bach, Beethoven, Grieg, Paganini, Tárrega and Vivaldi pieces, and a public-domain
flute duet transcription for the Little Fugue. *Mars* uses only its opening ostinato. Arrangements
change keys, octaves, voicings and instrumentation, and end excerpts with a cadence of their own.

`guitar-pro/import-test.gp` is a small hand-built Guitar Pro 7 file used to test the importer
(drop D, repeats, a tie, a slide, a bend, a harmonic, shared objects and a grace note). Open it from
the File menu to see Guitar Pro import work.

## Regenerating

The scores are written in a compact tab notation in
[`crates/engine/examples/demos.rs`](../crates/engine/examples/demos.rs) and entered through the
engine's own commands, so they double as an editing test:

```sh
cargo run -p guitario-engine --example demos      # rewrites demos/*.gt, midi/, tab/, gp5/ and musicxml/
cargo run -p guitario-gp --example fixture        # rewrites guitar-pro/import-test.gp
cargo test -p guitario-engine --test demos        # checks every demo loads, plays and stays metal
```

Render any of them to audio with the CLI:

```sh
cargo run -p guitario-cli -- convert demos/04-summer-presto.gt summer.wav
```
