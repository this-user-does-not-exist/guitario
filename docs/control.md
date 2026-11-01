# Driving Guitario from outside: the control channel and MCP

Every action in Guitario is an engine command with an id (`note.setFret`, `track.add`,
`file.exportPdf` …). Three ways reach the same commands from outside the window:

| Way | Start it | For |
|---|---|---|
| **MCP server**, headless | `guitario-cli mcp [file]` | an agent editing a score with no window: Claude Code, other MCP clients |
| **MCP server**, on the running app | `guitario --control 4747`, then `guitario-cli mcp --connect 4747` | an agent working in the app a person is looking at, with screenshots |
| **Control channel** | `guitario --control 4747`; one request with `guitario-cli send 4747 <method> [params]` | scripts, tests and other tools speaking JSON lines |

`guitario-cli run` (a script of commands applied to a file) is the fourth, older way; see
[development](development.md#headless-cli).

## With Claude Code

```sh
claude mcp add guitario -- guitario-cli mcp                       # a headless session
claude mcp add guitario -- guitario-cli mcp --connect 4747        # the running app (start it with --control 4747)
```

Use the full path to `guitario-cli` if it is not on `PATH` (`cargo build -p guitario-cli` puts it
in `target/debug/`). The server's instructions tell the model how commands, the cursor and string
numbers work.

## MCP tools

| Tool | Arguments | Returns |
|---|---|---|
| `commands` | `prefix?` (`"note."`), `search?` (matches id, label and menu) | every matching command: id, label, menu, shortcut, params, `mutates` |
| `run` | `command`, `params?` | the command's result |
| `batch` | `steps`: `[["command.id", {params}], …]` | every step's result; all or nothing |
| `state` | none | title, artist, tempo, bar count, tracks, cursor, selection, path, unsaved changes, undo and redo |
| `view` (app only) | `zoom?` (0.25–4), `layout?` (`page`, `screen`, `horizontal`), `multitrack?`, `multivoice?`, `home?` | the view |
| `screenshot` (app only) | `area?` (`window` or `score`) | a PNG image |

A command that fails comes back as a tool error (`isError: true`) with the engine's message, so
the model can correct its params; nothing is changed. The server speaks MCP revisions
2025-06-18, 2025-03-26 and 2024-11-05 over stdio, one JSON-RPC message per line.

## The control protocol

The running app listens on **127.0.0.1 only**, and only when started with `--control <port>` or
with `GUITARIO_CONTROL=<port>` in the environment (port 0 picks a free one; the app prints the
address on stderr). Any program run by the same user can connect, so leave it off when you are
not using it.

One request per line, one reply per line, in order:

```json
{"id": 1, "method": "run", "params": {"command": "note.setFret", "params": {"fret": 5}}}
{"id": 1, "result": {"ok": true}}
{"id": 2, "method": "run", "params": {"command": "note.setFret", "params": {"fret": 99}}}
{"id": 2, "error": {"message": "bad parameter: `fret` must be at most 36"}}
```

`id` is any JSON value and is echoed back. `params` may be left out.

| Method | Params | Result |
|---|---|---|
| `hello` | none | `name`, `version`, `protocol` (1), `methods`, `app: true` from the running app |
| `commands` | `prefix?`, `search?` | the command list, as the MCP tool |
| `run` | `command`, `params?` | the command's result. `file.open` and `file.new` open a tab, as File › Open and File › New do |
| `batch` | `steps` (or the array as `params`) | `{"results": […]}`. If a step fails, the score, cursor, selection, history and path are put back as before the first step and the error names the step |
| `state` | none | as the MCP tool |
| `view` | as the MCP tool | the view; every field is checked before any is applied |
| `screenshot` | `area?` (`window`, `score`), `path?` | `{width, height, png}` with the PNG in base64, or `{width, height, path}` when `path` is given (the file is written there). Pixels are physical: a Retina window is twice its size in points |

A line that is not a JSON object with a string `method` gets one error reply and the connection
is closed, so an HTTP request from a web page never gets as far as its body. A line longer than
8 MiB is refused the same way. At most eight clients connect at once. A request waits up to 30
seconds for the app; a screenshot takes two frames.

The app answers between frames, on the UI thread, so a request sees the score exactly as the
window shows it and an edit appears at once. A command run this way is an undo step like any
other: the person at the window can undo what an agent did.

## Where it lives

- `crates/engine/src/control.rs`: the protocol, the session's methods and `batch`. Pure, builds
  for wasm.
- `crates/engine/src/mcp.rs`: MCP to control-protocol translation. Pure.
- `crates/ui-egui/src/control.rs`: the socket, the hand-off to the UI thread, `view` and
  `screenshot`.
- `apps/guitario-cli/src/main.rs`: `mcp` on stdio, `send`, and the client for `--connect`.
- Tests: the protocol and hostile input (`control.rs`, `mcp.rs` in both crates), an agent driving
  the app with real screenshots (`an_agent_drives_the_app_end_to_end`), and the ten agent tasks
  over the real binary (`apps/guitario-cli/tests/agent_tasks.rs`).
