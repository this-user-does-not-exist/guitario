# Sampled instruments (SFZ)

Guitario plays drums, bass and guitars from open [SFZ](https://sfzformat.com) instruments when a
sound bank is installed, and falls back to its built-in synthesiser per track otherwise. The code is
`crates/playback/src/sfz/` (parser, decoder, voices) and `crates/playback/src/bank.rs` (which track
plays which instrument).

## Install the recommended libraries

```sh
cargo xtask sounds --list          # what and how big
cargo xtask sounds                 # download everything (about 0.9 GB) and write bank.json
cargo xtask sounds --only karoryfer-growlybass,muldjordkit
```

The folder is `$GUITARIO_SOUNDS`, else `~/Library/Application Support/Guitario/sounds` (macOS),
`%APPDATA%\Guitario\sounds` (Windows) or `~/.local/share/guitario/sounds` (Linux). The app reads
it when it starts. Needs `curl` and `unzip`, `tar` (bsdtar reads 7z) or `git`. The libraries are never
committed to this repository.

| Role | Library | Licence | Size |
|---|---|---|---|
| `guitar_distorted` (GM 29, 30) | [FreePats FSBS Electric Guitar Distorted #1](https://freepats.zenvoid.org/ElectricGuitar/distorted-electric-guitar.html): a Fender through an amp and effects rack | CC0 | 300 MB |
| `guitar_clean` (GM 24–28, 31) | [FreePats FSBS Electric Guitar Clean #1](https://freepats.zenvoid.org/ElectricGuitar/clean-electric-guitar.html) | CC0 | 120 MB |
| `bass` (GM 32–39) | [Karoryfer Growlybass](https://github.com/sfzinstruments/karoryfer.growlybass): Squier Jazz Bass, direct | CC0 | 160 MB |
| `drums` | [MuldjordKit](https://github.com/sfzinstruments/DrumGizmo.MuldjordKit): Tama Superstar, recorded for a death metal album | CC BY 4.0 (credit Lars Muldjord; mapping by kinwie) | 340 MB |

Other open libraries that fit and are not installed by default (all CC0, from
[sfzinstruments](https://github.com/sfzinstruments)): `karoryfer.black-and-blue-basses` (960 MB, the
best bass, too big for a default), `karoryfer.black-and-green-guitars` (clean/twang, 460 MB),
`karoryfer.emilyguitar` (a rock guitar, 100 MB), `karoryfer.big-rusty-drums`, `virtuosity_drums`
(a jazz kit). FreePats Distorted #2 (130 MB) is a smaller alternative to #1. Nobody on the team has
listened to these on speakers yet; swap entries in `bank.json` freely.

## bank.json

```json
{ "drums": "muldjordkit/DrumGizmo/MuldjordKit/Stereo/DrumGizmo MuldjordKit.sfz",
  "bass": "karoryfer-growlybass/Growlybass/growlybass_clean.sfz",
  "guitar_clean": "...", "guitar_distorted": "..." }
```

Paths are relative to the folder. Any role may be left out. Use it from the CLI with
`guitario-cli convert song.gt song.wav --sounds <folder>` (the `file.exportWav` command takes
a `sounds` parameter); the desktop app uses the default folder.

### Articulations

A role can name a program per articulation instead of one program. `sustain` plays every note
that has no program of its own; the others are `palm_mute`, `dead` (muted strings), `harmonic`
(natural, artificial, pinch, tapped and semi-harmonics, played at the pitch they sound) and
`legato` (hammer-ons, pull-offs and legato slides). A library that switches articulations by key
takes the program and the keyswitch to hold, as a note name or MIDI number:

```json
{ "guitar_distorted": {
    "sustain":   "dist/Programs/sustain.sfz",
    "palm_mute": "dist/Programs/palm-muted.sfz",
    "dead":      { "program": "dist/Programs/all.sfz", "keyswitch": "c1" },
    "harmonic":  { "program": "dist/Programs/all.sfz", "keyswitch": 26 } },
  "bass": "growlybass/growlybass_clean.sfz" }
```

Each articulation's program loads only the keys its notes play. What the libraries installed by
`cargo xtask sounds` record beyond sustain has not been mapped yet; a bank written by hand can
use the form above today. Without an articulation program, the sustain program plays the note:
palm mutes and dead notes are cut short (a heavier palm mute sooner), a harmonic plays the
sustain sample at its pitch, and a hammer-on, pull-off or legato slide moves the sample already
sounding on the string to the new pitch without a new attack (a shift slide picks again).

## What the SFZ engine supports

Headers `<control> <global> <master> <group> <region>` with inheritance; `#define`/`$VAR`, `#include`
(several per line), `default_path`, comments; `sample`, `key`/`lokey`/`hikey`/`pitch_keycenter`,
`lovel`/`hivel`, `tune`/`transpose`/`pitch_keytrack`, `volume`/`amplitude`/`pan`/`amp_veltrack`,
`offset`/`end`, `loop_mode` (`one_shot`, `loop_continuous`, `loop_sustain`) with `loop_start`/`loop_end`,
`delay` and an AHDSR amplitude envelope (`ampeg_*`), `group`/`off_by`/`off_mode` (hi-hat choke),
round robin (`seq_length`/`seq_position`, `lorand`/`hirand`), `set_ccN`/`set_hdccN` defaults,
`locc`/`hicc` region conditions and `_onccN`/`_ccN` modulation evaluated at those defaults, and the
default keyswitch (`sw_default`/`sw_last`), or the one a bank names for an articulation. Samples:
WAV (16/24/32-bit, float) and FLAC. A note outside every region borrows the nearest one,
transposed.

Not yet: release-triggered samples (`trigger=release` regions are dropped), SFZ filters and EQ
opcodes, LFOs, live CC (so articulations a library selects by controller cannot be mapped), OGG
samples, embedded loop points in WAV files, generators (`*sine`). Bends, slides, vibrato and
legato notes change the playback rate. Samples decode in the background whenever the score needs
new notes, and Play waits for them.

## The built-in synth's articulations

Without a sound bank (or for a role it does not map), the plucked-string synth plays every
articulation itself (`crates/playback/src/synth.rs`, `player.rs`):

- **Palm mute**: a lowpass inside the string loop takes the highs off and the note dies in 0.7 s
  (a light mute) to 0.15 s (the track's heaviest), its pitch kept true by taking the filter's delay
  off the string's length.
- **Dead note**: a click that dies in 45 ms.
- **Harmonics**: the string is excited through a comb that keeps only the partial's multiples, so
  it rings at the partial over its own length, longer than the fretted note at that pitch. A
  natural harmonic sounds the partial whose node is at its fret (2 at the 12th, 3 at the 7th and
  19th, 4 at the 5th, 5 at the 4th, 9th and 16th …, pure rather than tempered); artificial and
  tapped harmonics an octave over the fretted note; a pinch harmonic two octaves over it with a
  little of the note, a semi-harmonic with more.
- **Slides**: a shift or legato slide glides over the last 30 % of the first note (30 to 180 ms) and
  arrives on the second; a shift slide then picks it, a legato slide carries on. A slide in starts
  three semitones below or above and reaches the note within 120 ms; a slide out falls or rises
  eight semitones over the last half of the note (up to 0.35 s) while fading out.
- **Hammer-ons and pull-offs** carry on the ringing string with a little of the finger's energy;
  **tapped** notes do the same, or pluck a silent string softly without a pick click.
- **Staccato** sounds for half the note. A note's end lifts the finger: it fades by 60 dB in a
  quarter second at any pitch (before, a low string rang on for seconds after its note ended).
- **Slap and pop** are brighter, and a harder note is brighter than a soft one.

MIDI export follows the same rules for harmonics (the partial, and the note under a
semi-harmonic) and staccato. Bends, slides and the whammy bar are not written to MIDI as pitch
bends yet.

## Effects after the instrument

Every track runs an effect chain after its instrument, then the mixer's equalizer, volume and pan;
the master bus runs its compressor, reverb and equalizer, then the master volume and pan
(`crates/playback/src/fx/`, [decisions](decisions.md)). A track that has not set its own chain
plays its instrument's: the synth's overdriven (GM 29) and distorted (GM 30) guitars go through an
amp and cabinet; a sampled instrument plays none, its recording has its own amp. So with the
FreePats distorted guitar installed, add an amp only if you want more gain on top of it.

```sh
guitario-cli run song.gt fx.json --save song.gt
# fx.json:
[["track.setEffect", {"track": 0, "slot": 0, "type": "amp", "preset": "Metal Rhythm"}],
 ["track.setEffect", {"track": 0, "slot": 1, "type": "cabinet", "params": {"model": 2}}],
 ["track.setEq", {"track": 1, "preset": "Bass"}],
 ["score.setMastering", {"slot": "reverb", "on": true, "preset": "Studio"}]]
```

`guitario-cli run - types.json` with `[["sound.effectTypes", {}]]` lists every type's parameters, ranges and
presets. WAV and FLAC exports include the effects and their tails.

## Safety

Sample and include paths cannot climb more than two folders above the program's own folder, and
files are size-limited (8 MB of SFZ text, 256 MB per sample, 16 M frames); a missing or broken
sample drops that region with a warning and the track keeps playing. Tests cover hostile text; `GUITARIO_SFZ_DIR`, `GUITARIO_SFZ_FILE` and `GUITARIO_SFZ_RENDER` run
`crates/playback/tests/real_libraries.rs` against libraries on disk.
