# Development guide

## Prerequisites

Rust 1.95 or later (`rustup`), with `rustup target add wasm32-unknown-unknown` for `cargo xtask wasm`.
On Linux: `libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev libgl1-mesa-dev
libgtk-3-dev libasound2-dev`.

## Build and run

```sh
cargo run -p guitario                        # the editor
cargo run -p guitario -- demos/01-flight-of-the-bumblebee.gt
cargo run -p guitario-cli -- commands        # every command id with its params
cargo xtask ci                                  # fmt, clippy, tests, layers, parity, scorecard
cargo xtask perf                                # release timings on a 400-bar score vs perf/budgets.toml
```

### Packages and releases

```sh
cargo xtask package                               # this platform, in release, into dist/
cargo xtask package --target x86_64-apple-darwin  # another architecture (rustup target add first)
cargo xtask package --no-build                    # repackage what target/release holds
```

macOS gives `Guitario.app`, a `.zip` and a `.dmg`; Linux a `.tar.gz` (with `install.sh`) and a
`.deb` where `dpkg-deb` exists; Windows a `.zip` and, with Inno Setup installed, the installer.
The templates are in `packaging/`. Signing happens when these are set: `GUITARIO_SIGN_IDENTITY`
(a Developer ID Application certificate in the keychain) and `GUITARIO_NOTARY_APPLE_ID`,
`GUITARIO_NOTARY_TEAM_ID`, `GUITARIO_NOTARY_PASSWORD` (an app-specific password) on macOS;
`GUITARIO_WINDOWS_CERT` (a `.pfx`, with `GUITARIO_WINDOWS_CERT_PASSWORD`) or
`GUITARIO_WINDOWS_CERT_SHA1` (a certificate in the store) on Windows.

A release: `cargo xtask version set X.Y.Z`, commit, then push the tag `vX.Y.Z`. The release
workflow (`.github/workflows/release.yml`) checks the tag against the workspace version, builds
every platform, signs with the repository's secrets when they are set (the list is at the top of
the workflow) and drafts a GitHub release with the packages and `SHA256SUMS`; publish the draft
after looking at it. A version with a pre-release part (`0.2.0-beta.1`) drafts a pre-release.

### Faster test loops

`cargo test -p <crate> --lib --tests` skips examples and doctests; `cargo nextest run --workspace`
runs test binaries in parallel (`.config/nextest.toml`).

## Fonts

Nothing is bundled. The page uses a SMuFL font (Bravura or Leland) and Times New Roman/Arial
equivalents found in the system font folders, `CRAFT_FONTS_DIR` or `GUITARIO_FONTS_DIR`;
`GUITARIO_MUSIC_FONT` points at a music font file directly. Without them, symbols are drawn.

## Environment variables

| Variable | Effect |
|---|---|
| `GUITARIO_CONTROL` | A port: the app serves the control channel on 127.0.0.1 (as `--control <port>`; [`control.md`](control.md)) |
| `GUITARIO_CONFIG_DIR` | Where `config.toml` lives, with `recovery/` beside it for autosaved unsaved work; set but empty keeps preferences in memory and turns autosave off (tests, agents) |
| `GUITARIO_FONTS_DIR`, `CRAFT_FONTS_DIR` | Extra font folders |
| `GUITARIO_MUSIC_FONT` | A SMuFL font file to use |
| `GUITARIO_SOUNDS` | The sound-bank folder of SFZ instruments ([`sounds.md`](sounds.md)); default is the per-user data folder |
| `GUITARIO_STREAM_CACHE_MB` | The most memory the tails of long samples keep while no note plays them (default 256; [`sounds.md`](sounds.md#memory-and-streaming)) |
| `GUITARIO_SFZ_DIR`, `GUITARIO_SFZ_FILE`, `GUITARIO_SFZ_RENDER`, `GUITARIO_SFZ_OUT` | Run the real-library tests in `crates/playback/tests/real_libraries.rs` (parse a folder, print a key map, render a riff to a WAV) |
| `GUITARIO_FUZZ_CASES`, `GUITARIO_FUZZ_SEED` | Cases per reader in `crates/gp/tests/fuzz.rs` (default 400) and where its seeds start; failing inputs land in `target/fuzz-failures/` |
| `GUITARIO_GP_CORPUS`, `GUITARIO_GP_DUMP` | Import every Guitar Pro file in a local folder (`crates/gp/tests/corpus.rs`); GP3-5 files must also come back unchanged through the `.gp5` writer. `GUITARIO_GP_DUMP=1` prints what each file imported as. Real files belong to their authors: keep the corpus local |

## Headless CLI

`guitario-cli commands | info <file> | convert <in> <out> [--sounds <dir>] [--tracks 0,2] [--tab-only] [--score] | run <file|-> <script.json> [--save <path>]`. `convert` writes `.gt`, `.gp`, `.gp5`, `.musicxml`, `.pdf`, `.mid`, `.wav`, `.flac` or `.txt`. PDF and printing come from the `pdf` feature (on by default; `--no-default-features` builds a CLI without the UI crate and its audio libraries).
Inputs are `.gt`, any Guitar Pro file (`.gp`, `.gpx`, `.gp5`, `.gp4`, `.gp3`) MIDI (`.mid`, `.midi`) or MusicXML (`.musicxml`, `.xml`, `.mxl`); outputs are `.gt`, `.gp`, `.gp5`, `.musicxml`, `.pdf`, `.mid`, `.wav`, `.flac` or `.txt`.
A script is a JSON array of `["command.id", {params}]` steps.

`guitario-cli mcp [file] [--connect <port>]` is an MCP server on stdio, over a headless session or the running app; `guitario-cli send <port> <method> [params]` sends one control request to the app. Both are described in [`control.md`](control.md).

## Native format (.gt)

Versioned JSON (`format: "guitario"`, `version: 1`), validated with `Score::validate` before it
is accepted, saved through craft-atomic. Adding a model field: `#[serde(default)]` so older files
load; bump the version only for breaking changes.

## Offscreen UI snapshots (no window)

```sh
cargo run -p guitario-ui-egui --example snapshot -- out.png 1440 900
cargo run -p guitario-ui-egui --example snapshot -- out.png 1440 900 --file demos/05-toccata-and-fugue.gt --zoom 1.0
```

Flags: `--tab-only`, `--playing` (shows the playback cursor without a sound card), `--selection`,
`--track n`. Look at the PNG.

## Scorecard and menu parity

`cargo xtask scorecard` regenerates `docs/scorecard.md` from `scorecard/*.toml` and counts from the
tree; `cargo xtask parity` regenerates `docs/parity-checklist.md` from the Guitar Pro menu catalog.
CI runs both with `--check`. When parity rises, raise `FLOOR` in `crates/ui-egui/src/parity.rs`
and update the menu count in `README.md` and `ROADMAP.md`: the scorecard fails while they disagree
with the checklist.

## Demo suite

`demos/` (public-domain classics arranged for metal) is generated by `cargo run -p guitario-engine --example demos`
and checked by `crates/engine/tests/demos.rs`. The `.gp` fixture: `cargo run -p guitario-gp --example fixture`.

## Testing strategy

Pure crates (theory, score, layout, notation, playback) carry unit tests; the engine has
integration tests including `panic_hunt`, which sends hostile params to every command; formats have
round-trip and malformed-input tests; the UI is checked with offscreen renders.
