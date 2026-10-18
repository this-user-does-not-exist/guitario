# Sampled instruments (SFZ)

Guitario plays drums, bass and guitars from open [SFZ](https://sfzformat.com) instruments when a
sound bank is installed, and falls back to its built-in synthesiser per track otherwise. The code is
`crates/playback/src/sfz/` (parser, decoder, voices) and `crates/playback/src/bank.rs` (which track
plays which instrument).

## Install the recommended libraries

**From the app:** the first time Guitario starts without a sound bank it offers to download the
libraries (Download, Not Now, Don't Ask Again). Any time later, open the **Sound Bank** window
(Sound › Audio Settings… › Sound Bank…, or the Bank row of the track inspector's Sounds › Sound
Bank…): it shows the folder, what plays each kind of track, each library with its licence and size
and an Install button, Install All, progress while downloading (Cancel keeps what came; the next
attempt carries on from there), and the credits the installed libraries' licences ask for. Playback
never waits on it: a track the bank does not cover plays the built-in synthesiser.

**From a terminal** (or an agent: the `sound.installLibraries` command with `ids`, and `sound.bank`
to see what is there):

```sh
cargo xtask sounds --list          # what and how big
cargo xtask sounds                 # download everything (about 0.9 GB) and write bank.json
cargo xtask sounds --only karoryfer-growlybass,muldjordkit
```

Both run the same installer (`crates/playback/src/install.rs`). The folder is `$GUITARIO_SOUNDS`,
else `~/Library/Application Support/Guitario/sounds` (macOS), `%APPDATA%\Guitario\sounds`
(Windows) or `~/.local/share/guitario/sounds` (Linux). Downloads go through `curl` (part of macOS,
Windows 10 and later and Linux desktops) and resume where they stopped; the zip and 7z archives are
unpacked by Guitario itself; the drum kit, a git repository, is cloned when `git` is installed and
downloaded as GitHub's zip of it otherwise. Each library gets a `SOURCE.txt` with its title,
licence and credit, and `bank.json` is updated only for the roles the installer filled before, so
your own entries stay. The libraries are never committed to this repository.

| Role | Library | Licence | Size |
|---|---|---|---|
| `guitar_distorted` (GM 29, 30) | [FreePats FSBS Electric Guitar Distorted #1](https://freepats.zenvoid.org/ElectricGuitar/distorted-electric-guitar.html): a Fender through an amp and effects rack | CC0 | 300 MB |
| `guitar_clean` (GM 24–28, 31) | [FreePats FSBS Electric Guitar Clean #1](https://freepats.zenvoid.org/ElectricGuitar/clean-electric-guitar.html) | CC0 | 120 MB |
| `bass` (GM 32–39) | [Karoryfer Growlybass](https://github.com/sfzinstruments/karoryfer.growlybass): Squier Jazz Bass, direct; the `dirty` program, sustain with release noises | CC0 | 160 MB |
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
  "bass": "karoryfer-growlybass/Growlybass/growlybass_dirty.sfz",
  "guitar_clean": "...", "guitar_distorted": "..." }
```

Paths are relative to the folder. Any role may be left out.

### A track's own instrument

A track plays the role its instrument picks (the General MIDI program, or the drum kit) unless it
names another: the Bank row in the track inspector's Sounds offers Automatic, each role with what
plays it, an `.sfz` file of your own, and the Sound Bank window. The choice is saved with the score
(`Track::bank`; `track.setBank` with a role, a path absolute or relative to the bank's folder, or
null). A role the bank does not fill or a file that is not there plays the built-in synthesiser
and says so. A file chosen this way plays every articulation from itself. Use it from the CLI with
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
velocity curves (`amp_velcurve_N`, straight between the points, 0 at velocity 0 and 1 at 127
unless given), `offset`/`end`, `loop_mode` (`no_loop`, `one_shot`, `loop_continuous`,
`loop_sustain`) with `loop_start`/`loop_end` and `loop_crossfade` (the loop's end fades into what
comes before its start), `delay` and an AHDSR amplitude envelope (`ampeg_*`), filters (`fil_type`
`lpf_1p`, `lpf_2p`, `lpf_4p`, `lpf_6p`, the same high-passes, `bpf_2p`, `brf_2p`; `cutoff` with its
`cutoff_onccN` modulation in cents, `resonance` in dB, `fil_keytrack`/`fil_keycenter`,
`fil_veltrack`, and the filter envelope `fileg_delay`/`attack`/`hold`/`decay`/`sustain`/`release`/
`depth`), `trigger` (`attack`; `release` regions start when the note ends, `rt_decay` dB quieter
for each second it was held, and not after a let-ring note; `first` and `legato`, the latter
playing hammer-ons, pull-offs and legato slides), `group`/`off_by`/`off_mode` (hi-hat choke),
round robin (`seq_length`/`seq_position`, `lorand`/`hirand`), `set_ccN`/`set_hdccN` defaults,
`locc`/`hicc` region conditions and `_onccN`/`_ccN` modulation evaluated at those defaults, and the
default keyswitch (`sw_default`/`sw_last`), or the one a bank names for an articulation. Samples:
WAV (16/24/32-bit, float; the loop a `smpl` chunk marks is used where the SFZ names none, and makes
the region loop unless it says `no_loop`), FLAC and OGG Vorbis (lewton, pure Rust). A note outside
every region borrows the nearest one, transposed.

Not yet: EQ opcodes (`eq1_*`), LFOs, the pitch envelope, `<curve>` headers and `curvecc`, live CC (so
articulations a library selects by controller cannot be mapped), generators (`*sine`). Bends, slides,
vibrato and legato notes change the playback rate. Samples decode in the background whenever the
score needs new notes, and Play waits for them.

### Memory and streaming

Samples are held as 16-bit values (exact for 16-bit recordings, half the memory of floats). A
sample longer than 2 s keeps only its first second in memory; the rest, its tail, is decoded
from the file again when a note gets near it (`crates/playback/src/sfz/stream.rs`). The FreePats
guitars ring for about 16 s a note, so held whole a guitar takes about 450 MB and streamed 23 MB;
the 400-bar perf score's instruments take 25 MB instead of 500 MB (`sounds_memory_mb` in
`perf/budgets.toml`). While the app plays, a voice asks the streamer thread for its tail 0.6 s
before the head runs out and never waits for it: a tail that comes late plays as silence until
it arrives and counts as a late tail (none in 15 s of four demos played at real speed on a
four-core machine, `sounds_late_tails` in the perf budgets). Exports and tests load a tail when
the note reaches it, so they are exact. Tails stay cached up to 256 MB (`GUITARIO_STREAM_CACHE_MB`),
the least recently used going first; a tail a note is playing is never dropped, and one unused
for a minute is.

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
the master bus runs its compressor, reverb and equalizer, then the master volume and pan (their
automation, from the global view's Master row or `score.setMasterAutomation`, goes over them)
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
