<h1 align="center">Guitario</h1>

<p align="center">
  <b>Tablature, notation and playback; an open-source, clean-room take on Guitar Pro, rebuilt in pure Rust.</b><br>
  Tab and standard notation on real pages, Guitar Pro's keys and palette, playback with loops and<br>
  slow-down, and Guitar Pro files, in a native app written entirely in Rust. Open source, offline, and yours.
</p>

<p align="center">
  <img alt="100% Rust" src="https://img.shields.io/badge/100%25-Rust-b7410e?style=flat-square&logo=rust">
  <img alt="macOS · Windows · Linux" src="https://img.shields.io/badge/macOS%20%C2%B7%20Windows%20%C2%B7%20Linux-native-2f7bf5?style=flat-square">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-3a3a3a?style=flat-square">
  <img alt="Status: pre-alpha" src="https://img.shields.io/badge/status-pre--alpha-d69e2e?style=flat-square">
</p>

<br>

<p align="center">
  <img src="docs/images/guitario-steel-horizon.png" alt="Guitario showing Steel Horizon, an original power-metal demo: twin lead guitars in standard notation over tab, with the edition palette, the inspector, the fretboard and a four-track global view" width="100%">
  <br>
  <sub><i>Steel Horizon</i>, one of the original metal demos: twin harmony leads, rhythm guitar and bass.</sub>
</p>

> [!WARNING]
> Guitario is **pre-alpha**. Writing, arranging with drums, playing and practising work end to
> end; printing and full Guitar Pro exchange do not yet. See [ROADMAP.md](ROADMAP.md).

<p align="center">
  <a href="#features">Features</a> ·
  <a href="#demos">Demos</a> ·
  <a href="#built-for-agents">Agents</a> ·
  <a href="#under-the-hood">Under the hood</a> ·
  <a href="#get-started">Get started</a>
</p>

<br>

<table>
  <tr>
    <td width="25%" valign="top">
      <h3>🎸 Familiar by design</h3>
      Guitar Pro's layout, menus and keys: type a fret, press <code>P</code> for palm mute, <code>H</code> for a hammer-on, Space to play. If you know Guitar Pro, you already know Guitario.
    </td>
    <td width="25%" valign="top">
      <h3>🎼 Real engraving</h3>
      Standard notation over tab on A4 pages, with beams, accidentals, bends, slides and palm-mute lanes, drawn with the SMuFL music font.
    </td>
    <td width="25%" valign="top">
      <h3>🔁 Practice tools</h3>
      Loop a selection, slow it to 25 % without changing pitch, metronome and count-in, with the playing note on the fretboard.
    </td>
    <td width="25%" valign="top">
      <h3>🤖 Agent-ready</h3>
      Every action is a command, so you can drive the same engine from the UI, the CLI, scripts and tests.
    </td>
  </tr>
</table>

<br>

## Features

Every screenshot here is the real app rendering one of the original demo scores offscreen.

<table>
  <tr>
    <td width="50%" valign="top">
      <img src="docs/images/guitario-odd-meters.png" alt="Odd Meter Assault: 7/8, 5/4, 13/16, 6/8 and 12/8 bars in drop D with palm-mute and let-ring lanes" width="100%">
      <h3>Notation that reads like Guitar Pro</h3>
      Standard notation and tab joined by a bracket, time signatures on both staves, key signatures and accidentals, beamed rhythms, rests and tuplets, section boxes, tempo marks and repeats. Tab shows P.M. and let-ring lanes, vibrato, bends with their height, slides and hammer-on arcs. Any tuning from 4 to 12 strings, with drop, D and C standard, Drop B and seven-string presets.
    </td>
    <td width="50%" valign="top">
      <img src="docs/images/guitario-playback.png" alt="Playback: the sounding beat highlighted on the page, the chord on the fretboard and the bar marked in the global view" width="100%">
      <h3>Hear it, loop it, slow it down</h3>
      Space plays from the cursor. A plucked-string model plays palm mutes, dead notes, bends, vibrato and legato slides, through an amp model for overdriven tracks. Loop a selection, change the speed, add a metronome and a count-in, export WAV or MIDI.
    </td>
  </tr>
</table>

**Editing:** fret entry from the keyboard or the fretboard, Guitar Pro's single-key techniques, an edition palette that shows the current beat, selection with copy, cut, paste and transpose, undo for every edit, and a Guitar Pro menu bar whose unbuilt items are greyed out ([menu parity](docs/parity-checklist.md): 117 of 172).

**Drums:** drum-kit tracks with a percussion staff, a drum pad, Guitar Pro's two-digit drum entry (3 then 6 is a kick), synthesised General MIDI drums and drum tab export.

**Files:** opens Guitar Pro 7 and 8 `.gp` files including drum tracks, saves `.gt` atomically, exports MIDI, WAV and ASCII tab.

## Demos

[`demos/`](demos/README.md) holds 15 original metal pieces, from thrash, doom and death metal to djent on a seven-string, power-metal twin leads, black-metal blast beats, odd meters and a shred solo, with drum parts on every band piece and a drum-groove reference, each with MIDI and tab copies. They are generated from code and tested on every change.

## Built for agents

Every edit is an engine command with a stable id and JSON params, so the UI, the CLI and tests use one entry point:

```sh
cargo run -p guitario-cli -- commands                       # every command, with its params
cargo run -p guitario-cli -- convert song.gp song.wav       # .gp/.gt to .gt/.mid/.wav/.txt
cargo run -p guitario-cli -- run - riff.json --save riff.gt
```

A script is a JSON array of `["command.id", {params}]` steps. A control channel and an MCP server are on the [roadmap](docs/gaps.md#g10-automation).

## Under the hood

Pure Rust, layered crates (enforced by `cargo xtask layers`), a pure-data score model, and an engine where every action is a command with undo. Never-crash lints on every crate and a fuzz test over every command. Shared sibling apps libraries for atomic saves, settings and quitting. Details: [architecture](docs/architecture.md).

## Get started

```sh
cargo run --release -p guitario
cargo run --release -p guitario -- demos/01-thrash-attack.gt
```

Then click a beat, type a fret number and press Space. The [user guide](book/src/SUMMARY.md) covers editing, playback and shortcuts; [development](docs/development.md) covers building and testing.

## Documentation

[ROADMAP](ROADMAP.md) · [target-app parity](docs/target-app-parity.md) · [gaps](docs/gaps.md) · [roadmap](docs/roadmap.md) · [scorecard](docs/scorecard.md) · [architecture](docs/architecture.md) · [contributing](docs/contributing.md) · [AGENTS.md](AGENTS.md)

## Security

See [SECURITY.md](SECURITY.md).

---

## License and credits

Guitario is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Copyright (c) 2026 the Guitario contributors. Required notices are in [NOTICE](NOTICE).

Bundled assets are listed with their author, source and license in [ATTRIBUTION.md](ATTRIBUTION.md).
Every demo score is an original composition.

<sub>Guitar Pro is a trademark of Arobas Music. Guitario is an independent, open-source project and is not affiliated with, sponsored by or endorsed by Arobas Music; the name is used only to describe the workflows it is compatible with.</sub>
