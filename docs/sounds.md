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

## What the SFZ engine supports

Headers `<control> <global> <master> <group> <region>` with inheritance; `#define`/`$VAR`, `#include`
(several per line), `default_path`, comments; `sample`, `key`/`lokey`/`hikey`/`pitch_keycenter`,
`lovel`/`hivel`, `tune`/`transpose`/`pitch_keytrack`, `volume`/`amplitude`/`pan`/`amp_veltrack`,
`offset`/`end`, `loop_mode` (`one_shot`, `loop_continuous`, `loop_sustain`) with `loop_start`/`loop_end`,
`delay` and an AHDSR amplitude envelope (`ampeg_*`), `group`/`off_by`/`off_mode` (hi-hat choke),
round robin (`seq_length`/`seq_position`, `lorand`/`hirand`), `set_ccN`/`set_hdccN` defaults,
`locc`/`hicc` region conditions and `_onccN`/`_ccN` modulation evaluated at those defaults, and the
default keyswitch (`sw_default`/`sw_last`). Samples: WAV (16/24/32-bit, float) and FLAC. A note
outside every region borrows the nearest one, transposed.

Not yet: release-triggered samples (`trigger=release` regions are dropped), filters and EQ, LFOs,
effects, live CC, OGG samples, embedded loop points in WAV files, generators (`*sine`). Bends and
vibrato change the playback rate; hammer-ons, slides and palm mutes use plain note-on, a softer
velocity and a short gate. Samples are decoded on the UI thread when Play is first pressed after
starting the app (instruments are cached), so a large kit pauses briefly the first time.

## Safety

Sample and include paths cannot climb more than two folders above the program's own folder, and
files are size-limited (8 MB of SFZ text, 256 MB per sample, 16 M frames); a missing or broken
sample drops that region with a warning and the track keeps playing. Tests cover hostile text; `GUITARIO_SFZ_DIR`, `GUITARIO_SFZ_FILE` and `GUITARIO_SFZ_RENDER` run
`crates/playback/tests/real_libraries.rs` against libraries on disk.
