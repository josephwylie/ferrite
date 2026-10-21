//! Ferrite's in-band MCP server: how a Claude Session is offered
//! `show_visual` with no server process of its own.
//!
//! The CLI's SDK MCP transport, as the Agent SDK speaks it (verified against
//! 2.1.292, fixture `claude-show-visual-2.1.292`): the initialize control
//! request names the server (`sdkMcpServers: ["ferrite"]`), and the CLI then
//! tunnels JSON-RPC to it as `control_request {subtype: "mcp_message",
//! server_name, message}`, each answered by a `control_response` whose body
//! is `{mcp_response: <JSON-RPC response>}`. The model sees the tool as
//! `mcp__ferrite__show_visual`; its calls carry the streamed `tool_use` id in
//! `_meta["claudecode/toolUseId"]`.
//!
//! The tool is never an operator Decision: spawn pre-allows it
//! (`--allowedTools`), and a permission request for it that comes anyway is
//! allowed here, before the Decision decoder could see it.

use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::visual::{self, Visuals};

/// What the server answers `initialize` with when the CLI names no version.
const PROTOCOL_VERSION: &str = "2025-11-25";

pub(super) struct Server {
    visuals: Option<Visuals>,
    stdin: Arc<Mutex<std::process::ChildStdin>>,
}

impl Server {
    /// `visuals` is `None` (or Off) for a Session that offers nothing; spawn
    /// then never names the server and the CLI never calls it.
    pub(super) fn new(
        visuals: Option<Visuals>,
        stdin: Arc<Mutex<std::process::ChildStdin>>,
    ) -> Self {
        Self { visuals, stdin }
    }

    /// Whether this Session offers the tool: what spawn declares.
    pub(super) fn offered(visuals: Option<&Visuals>) -> bool {
        visuals.and_then(Visuals::definition).is_some()
    }

    /// Handle `value` if it is addressed to the server (or is a permission
    /// request for its tool). Returns whether it was — and so whether every
    /// other reader of the line must not see it.
    pub(super) fn observe(&self, value: &Value) -> bool {
        if value["type"] != "control_request" {
            return false;
        }
        let request = &value["request"];
        let Some(request_id) = value["request_id"].as_str() else {
            return false;
        };
        match request["subtype"].as_str() {
            Some("mcp_message") if request["server_name"] == visual::SERVER => {
                self.message(request_id, &request["message"]);
                true
            }
            Some("can_use_tool") if request["tool_name"] == visual::CLAUDE_NAME => {
                respond(
                    &self.stdin,
                    request_id,
                    json!({"behavior": "allow", "updatedInput": request["input"]}),
                );
                true
            }
            _ => false,
        }
    }

    fn message(&self, request_id: &str, message: &Value) {
        let id = message.get("id").cloned();
        let method = message["method"].as_str().unwrap_or_default();
        let definition = self.visuals.as_ref().and_then(Visuals::definition);
        let result = match (method, &definition) {
            ("initialize", _) => Ok(json!({
                "protocolVersion": message["params"]["protocolVersion"]
                    .as_str()
                    .unwrap_or(PROTOCOL_VERSION),
                "capabilities": {"tools": {}},
                "serverInfo": {"name": visual::SERVER, "version": env!("CARGO_PKG_VERSION")},
            })),
            ("tools/list", Some(definition)) => Ok(json!({"tools": [{
                "name": definition.name,
                "description": definition.description,
                "inputSchema": definition.input_schema,
                // Load up front: by default the CLI defers MCP tools behind
                // ToolSearch, and an agent that must search for the tool
                // first rarely reaches for it unprompted.
                "_meta": {"anthropic/alwaysLoad": true},
            }]})),
            ("tools/list", None) => Ok(json!({"tools": []})),
            ("tools/call", Some(_)) if message["params"]["name"] == visual::TOOL => {
                let call = message["params"]["_meta"]["claudecode/toolUseId"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let stdin = Arc::clone(&self.stdin);
                let request_id = request_id.to_owned();
                let visuals = self.visuals.as_ref().expect("a definition needs visuals");
                visual::answer(
                    visuals,
                    &call,
                    &message["params"]["arguments"],
                    move |answer| {
                        let mut content = vec![json!({"type": "text", "text": answer.text})];
                        if let Some(png) = answer.png_base64() {
                            content.push(
                                json!({"type": "image", "data": png, "mimeType": "image/png"}),
                            );
                        }
                        respond_mcp(
                            &stdin,
                            &request_id,
                            id,
                            Ok(json!({"content": content, "isError": answer.is_error})),
                        );
                    },
                );
                return;
            }
            ("tools/call", _) => Err((-32602, "unknown tool")),
            ("ping", _) => Ok(json!({})),
            // `notifications/initialized` and the like: no answer is owed,
            // but the transport still expects a control response — the SDK
            // sends this empty one.
            (method, _) if method.starts_with("notifications/") => {
                respond(
                    &self.stdin,
                    request_id,
                    json!({"mcp_response": {"jsonrpc": "2.0", "id": 0, "result": {}}}),
                );
                return;
            }
            _ => Err((-32601, "method not found")),
        };
        respond_mcp(&self.stdin, request_id, id, result);
    }
}

fn respond_mcp(
    stdin: &Mutex<std::process::ChildStdin>,
    request_id: &str,
    id: Option<Value>,
    result: Result<Value, (i64, &str)>,
) {
    let mut message = json!({"jsonrpc": "2.0", "id": id.unwrap_or(Value::Null)});
    match result {
        Ok(result) => message["result"] = result,
        Err((code, text)) => message["error"] = json!({"code": code, "message": text}),
    }
    respond(stdin, request_id, json!({"mcp_response": message}));
}

/// A write that fails means the CLI is gone; the reader is already turning
/// that into the Session's close.
fn respond(stdin: &Mutex<std::process::ChildStdin>, request_id: &str, body: Value) {
    let _ = super::write_stdin_line(
        stdin,
        &json!({
            "type": "control_response",
            "response": {"subtype": "success", "request_id": request_id, "response": body},
        }),
    );
}
