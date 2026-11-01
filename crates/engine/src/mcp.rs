//! An MCP server over the control protocol: JSON-RPC 2.0 messages in, tool calls on a [`Host`]
//! out. `guitario-cli mcp` reads the messages from stdin, one per line, and drives either a
//! headless session or the running app (`--connect <port>`), where the `view` and `screenshot`
//! tools appear too. Pure, like [`crate::control`].

use crate::control::Host;
use serde_json::{Value, json};

/// MCP revisions this server speaks, newest first; a client asking for another gets the newest.
pub const MCP_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Guitario is a Guitar Pro-style tablature editor. Every action is an engine command run by id: \
call `commands` (filter with `prefix` such as \"note.\" or `search`) to find ids and their params, then `run` one or `batch` \
several (all or nothing). Strings are numbered from 0 at the top (the highest string). `state` shows the score, cursor and \
selection; most commands act at the cursor, so move it with `cursor.set` {track, measure, beat, string, voice}. Undo with \
`edit.undo`. Save with `file.save` {path}.";

/// One MCP session: what the host can do beyond a headless session.
pub struct McpServer {
    /// The host is the running app: it has `view` and `screenshot`.
    app: bool,
}

impl McpServer {
    /// `app` is true when the host is the running app (see [`McpServer`]).
    pub fn new(app: bool) -> McpServer {
        McpServer { app }
    }

    /// Answers one message line; `None` for a notification, which gets no reply.
    pub fn handle_line(&mut self, host: &mut dyn Host, line: &str) -> Option<Value> {
        if line.len() > crate::control::MAX_REQUEST_BYTES {
            return Some(rpc_error(&Value::Null, -32600, "message too long"));
        }
        match serde_json::from_str::<Value>(line.trim()) {
            Ok(msg) => self.handle(host, &msg),
            Err(e) => Some(rpc_error(&Value::Null, -32700, &format!("parse error: {e}"))),
        }
    }

    pub fn handle(&mut self, host: &mut dyn Host, msg: &Value) -> Option<Value> {
        let Some(o) = msg.as_object() else {
            return Some(rpc_error(&Value::Null, -32600, "a message is a JSON-RPC 2.0 object (batches are not supported)"));
        };
        let method = o.get("method").and_then(Value::as_str);
        let Some(id) = o.get("id").cloned() else {
            // Notifications (`notifications/initialized`, `notifications/cancelled`) and stray
            // replies need no answer.
            return None;
        };
        let Some(method) = method else {
            return Some(rpc_error(&id, -32600, "a request needs a string \"method\""));
        };
        let params = o.get("params").cloned().unwrap_or(Value::Null);
        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": self.tools()})),
            "tools/call" => self.call(host, &params),
            "resources/list" => Ok(json!({"resources": []})),
            "prompts/list" => Ok(json!({"prompts": []})),
            _ => Err((-32601, format!("method not found: {}", method.chars().take(80).collect::<String>()))),
        };
        Some(match result {
            Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
            Err((code, m)) => rpc_error(&id, code, &m),
        })
    }

    fn initialize(&self, params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or_default();
        let version = MCP_VERSIONS.iter().find(|v| **v == asked).unwrap_or(&MCP_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "guitario", "title": "Guitario", "version": env!("CARGO_PKG_VERSION")},
            "instructions": INSTRUCTIONS,
        })
    }

    /// The tools, as `tools/list` returns them.
    pub fn tools(&self) -> Vec<Value> {
        let object = |props: Value, required: &[&str]| json!({"type": "object", "properties": props, "required": required});
        let mut tools = vec![
            json!({
                "name": "commands",
                "title": "List commands",
                "description": "Lists Guitario's engine commands: id, label, menu, shortcut, params and whether it changes the score. Filter with `prefix` (\"note.\", \"track.\", \"measure.\", \"cursor.\", \"select.\", \"file.\") or `search` (matches id, label and menu).",
                "inputSchema": object(json!({"prefix": {"type": "string"}, "search": {"type": "string"}}), &[]),
                "annotations": {"readOnlyHint": true},
            }),
            json!({
                "name": "run",
                "title": "Run a command",
                "description": "Runs one engine command by id with its params, e.g. {\"command\": \"note.setFret\", \"params\": {\"fret\": 5}}. A command that changes the score is one undo step and changes nothing if it fails.",
                "inputSchema": object(json!({"command": {"type": "string"}, "params": {"type": "object"}}), &["command"]),
            }),
            json!({
                "name": "batch",
                "title": "Run several commands",
                "description": "Runs commands in order, all or nothing: if one fails, the score, cursor and history are put back as before the first. Steps are [\"command.id\", {params}].",
                "inputSchema": object(json!({"steps": {"type": "array", "items": {"type": "array"}}}), &["steps"]),
            }),
            json!({
                "name": "state",
                "title": "Score state",
                "description": "The score's title, tempo, bar count and tracks, the cursor, the selection, the file path and whether it has unsaved changes. Use `run` with `score.timeline` for every note.",
                "inputSchema": object(json!({}), &[]),
                "annotations": {"readOnlyHint": true},
            }),
        ];
        if self.app {
            tools.push(json!({
                "name": "view",
                "title": "Change the view",
                "description": "Changes how the running app shows the score (not the score itself): zoom (0.25-4), layout (\"page\", \"screen\", \"horizontal\"), multitrack, multivoice, home (show the Homepage). Every field is optional; returns the view.",
                "inputSchema": object(json!({"zoom": {"type": "number"}, "layout": {"type": "string", "enum": ["page", "screen", "horizontal"]}, "multitrack": {"type": "boolean"}, "multivoice": {"type": "boolean"}, "home": {"type": "boolean"}}), &[]),
            }));
            tools.push(json!({
                "name": "screenshot",
                "title": "Screenshot",
                "description": "A PNG of the running app's window, or of the score area alone with area \"score\".",
                "inputSchema": object(json!({"area": {"type": "string", "enum": ["window", "score"]}}), &[]),
                "annotations": {"readOnlyHint": true},
            }));
        }
        tools
    }

    fn call(&mut self, host: &mut dyn Host, params: &Value) -> Result<Value, (i64, String)> {
        let name = params.get("name").and_then(Value::as_str).ok_or((-32602, "tools/call needs a tool `name`".to_string()))?;
        let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
        let method = match name {
            "commands" | "run" | "batch" | "state" => name,
            "view" | "screenshot" if self.app => name,
            "view" | "screenshot" => {
                return Ok(tool_error(
                    "screenshots and the view need the running app: start it with `guitario --control 4747` and the server with `guitario-cli mcp --connect 4747`",
                ));
            }
            _ => return Err((-32602, format!("unknown tool: {}", name.chars().take(80).collect::<String>()))),
        };
        Ok(match host.call(method, &args) {
            Ok(v) if method == "screenshot" => match v.get("png").and_then(Value::as_str) {
                Some(png) => json!({"content": [{"type": "image", "data": png, "mimeType": "image/png"}]}),
                None => tool_error("the app sent no image"),
            },
            Ok(v) => json!({"content": [{"type": "text", "text": v.to_string()}], "isError": false}),
            Err(e) => tool_error(&e.to_string()),
        })
    }
}

/// A tool that ran and failed: the model sees the message and can try again.
fn tool_error(message: &str) -> Value {
    json!({"content": [{"type": "text", "text": message}], "isError": true})
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

    fn rpc(server: &mut McpServer, s: &mut Session, id: u32, method: &str, params: Value) -> Value {
        server.handle(s, &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).unwrap_or(Value::Null)
    }

    #[test]
    fn initialize_list_and_call() {
        let (mut m, mut s) = (McpServer::new(false), Session::new());
        let init =
            rpc(&mut m, &mut s, 1, "initialize", json!({"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}));
        assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(init["result"]["serverInfo"]["name"], "guitario");
        assert!(m.handle(&mut s, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
        let newest = rpc(&mut m, &mut s, 2, "initialize", json!({"protocolVersion": "1999-01-01"}));
        assert_eq!(newest["result"]["protocolVersion"], MCP_VERSIONS[0]);
        let tools = rpc(&mut m, &mut s, 3, "tools/list", json!({}));
        let names: Vec<&str> = tools["result"]["tools"].as_array().into_iter().flatten().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(names, ["commands", "run", "batch", "state"], "no screenshot without the app");
        let r = rpc(&mut m, &mut s, 4, "tools/call", json!({"name": "run", "arguments": {"command": "note.setFret", "params": {"fret": 7}}}));
        assert_eq!(r["result"]["isError"], false, "{r}");
        let st = rpc(&mut m, &mut s, 5, "tools/call", json!({"name": "state"}));
        let text: Value = serde_json::from_str(st["result"]["content"][0]["text"].as_str().unwrap_or("null")).unwrap_or(Value::Null);
        assert_eq!(text["canUndo"], true);
    }

    #[test]
    fn errors_are_tool_errors_or_rpc_errors() {
        let (mut m, mut s) = (McpServer::new(false), Session::new());
        let bad = rpc(&mut m, &mut s, 1, "tools/call", json!({"name": "run", "arguments": {"command": "note.setFret", "params": {"fret": -4}}}));
        assert_eq!(bad["result"]["isError"], true, "{bad}");
        let shot = rpc(&mut m, &mut s, 2, "tools/call", json!({"name": "screenshot"}));
        assert_eq!(shot["result"]["isError"], true);
        assert_eq!(rpc(&mut m, &mut s, 3, "tools/call", json!({"name": "rm -rf"}))["error"]["code"], -32602);
        assert_eq!(rpc(&mut m, &mut s, 4, "sampling/createMessage", json!({}))["error"]["code"], -32601);
        for line in ["", "{", "[]", "[{\"id\":1}]", "{\"id\": 1}", "{\"id\": 1, \"method\": 3}", "\"hi\""] {
            let r = m.handle_line(&mut s, line);
            assert!(r.as_ref().is_some_and(|r| r["error"]["code"].is_i64()), "{line} → {r:?}");
        }
        assert!(m.handle_line(&mut s, "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\"}").is_none());
    }
}
