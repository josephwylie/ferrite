//! Ferrite's in-band MCP server: how a Claude Session is offered
//! `update_plan` and `wrap_up` (always) and `show_visual` (at the
//! operator's level) with no server process of its own.
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
//! Its tools are never an operator Decision: spawn pre-allows them
//! (`--allowedTools`), and a permission request for one that comes anyway
//! is allowed here, before the Decision decoder could see it.
//!
//! The plan (`update_plan`) is the server's to validate and the stream's to
//! confirm: a call it accepts moves the plan only when the CLI's receipt
//! for it says it succeeded for Main — a subagent's call, or one the CLI
//! failed, moves nothing (§12: children do not update the main plan).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use crate::progress::{ProgressEvent, StepStatus};
use crate::update_plan::{self, Update};
use crate::visual::{self, Visuals};
use crate::SessionEvent;

use super::lock;

/// What the server answers `initialize` with when the CLI names no version.
const PROTOCOL_VERSION: &str = "2025-11-25";

pub(super) struct Server {
    visuals: Option<Visuals>,
    browser: Option<crate::browser::ThreadClient>,
    stdin: Arc<Mutex<std::process::ChildStdin>>,
    plan: Mutex<Plan>,
}

/// What the server knows of Main's plan: its steps' statuses (§12: the
/// host keeps statuses, the transcript the words), and the calls it
/// answered that the stream has not yet confirmed.
#[derive(Default)]
struct Plan {
    /// `None` until this process sets a plan: after a resume the statuses
    /// are the transcript's, and step numbers are checked against the most
    /// a plan can hold.
    statuses: Option<Vec<StepStatus>>,
    /// Main set or moved the plan this turn, so its wrap-up settles it.
    touched: bool,
    calls: HashMap<String, Call>,
}

enum Call {
    Plan(Update),
    WrapUp { blocked: bool },
}

impl Server {
    /// `visuals` is `None` (or Off) for a Session that offers no visual.
    pub(super) fn new(
        visuals: Option<Visuals>,
        browser: Option<crate::browser::ThreadClient>,
        stdin: Arc<Mutex<std::process::ChildStdin>>,
    ) -> Self {
        Self {
            visuals,
            browser,
            stdin,
            plan: Mutex::default(),
        }
    }

    /// Whether this Session offers `show_visual`: what spawn declares.
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
            Some("can_use_tool")
                if request["tool_name"] == visual::CLAUDE_NAME
                    || request["tool_name"] == crate::wrap_up::CLAUDE_NAME
                    || request["tool_name"] == update_plan::CLAUDE_NAME
                    || request["tool_name"] == crate::browser::tools::CLAUDE_NAME =>
            {
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

    /// The plan events a stream line confirms: a Main receipt for a call
    /// this server answered. Every line passes through here, consumed or not.
    pub(super) fn receipt(&self, value: &Value) -> Vec<SessionEvent> {
        let main = matches!(value.get("parent_tool_use_id"), None | Some(Value::Null));
        let mut plan = lock(&self.plan);
        if value["type"] == "result" && main {
            plan.touched = false;
            return Vec::new();
        }
        if value["type"] != "user" {
            return Vec::new();
        }
        let results = value["message"]["content"].as_array().into_iter().flatten();
        let mut events = Vec::new();
        for result in results.filter(|block| block["type"] == "tool_result") {
            let Some(call) = result["tool_use_id"]
                .as_str()
                .and_then(|id| plan.calls.remove(id))
            else {
                continue;
            };
            if !main || result["is_error"] == true {
                continue;
            }
            let event = match call {
                Call::Plan(Update::Replace(steps)) => {
                    plan.touched = true;
                    plan.statuses = Some(steps.iter().map(|step| step.status).collect());
                    ProgressEvent::Plan {
                        steps,
                        explanation: String::new(),
                    }
                }
                Call::Plan(Update::Steps(steps)) => {
                    plan.touched = true;
                    if let Some(statuses) = &mut plan.statuses {
                        steps.apply(statuses);
                    }
                    ProgressEvent::PlanSteps(steps)
                }
                Call::WrapUp { blocked } => {
                    if blocked || !plan.touched {
                        continue;
                    }
                    if let Some(statuses) = &mut plan.statuses {
                        statuses.fill(StepStatus::Completed);
                    }
                    ProgressEvent::PlanSettled
                }
            };
            events.push(SessionEvent::Progress { event });
        }
        events
    }

    /// Answer an `update_plan` call: accepted (held for its receipt) with
    /// any warnings, or refused with what to fix.
    fn update_plan(&self, call: &str, arguments: &Value) -> Value {
        let mut plan = lock(&self.plan);
        let (text, is_error) = match Update::parse(arguments, plan.statuses.as_deref()) {
            Ok(accepted) => {
                plan.calls
                    .insert(call.to_owned(), Call::Plan(accepted.update));
                let mut text = accepted.answer;
                for warning in accepted.warnings {
                    text.push(' ');
                    text.push_str(&warning);
                }
                (text, false)
            }
            Err(refusal) => (refusal, true),
        };
        json!({"content": [{"type": "text", "text": text}], "isError": is_error})
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
            // Load up front: by default the CLI defers MCP tools behind
            // ToolSearch, and an agent that must search for a tool first
            // rarely reaches for it unprompted.
            ("tools/list", _) => {
                let ours = [
                    (
                        crate::wrap_up::TOOL,
                        crate::wrap_up::DESCRIPTION.to_owned(),
                        crate::wrap_up::input_schema(),
                    ),
                    (
                        update_plan::TOOL,
                        update_plan::description(),
                        update_plan::input_schema(),
                    ),
                ];
                let visual = definition.iter().map(|definition| {
                    (
                        definition.name,
                        definition.description.clone(),
                        definition.input_schema.clone(),
                    )
                });
                let browser = self.browser.as_ref().map(|_| {
                    let definition = crate::browser::tools::definition();
                    (
                        definition.name,
                        definition.description,
                        definition.input_schema,
                    )
                });
                let tools: Vec<Value> = ours
                    .into_iter()
                    .chain(visual)
                    .chain(browser)
                    .map(|(name, description, schema)| {
                        json!({
                            "name": name,
                            "description": description,
                            "inputSchema": schema,
                            "_meta": {"anthropic/alwaysLoad": true},
                        })
                    })
                    .collect();
                Ok(json!({ "tools": tools }))
            }
            // The box is drawn from the call's input; the answer only says so.
            ("tools/call", _) if message["params"]["name"] == crate::wrap_up::TOOL => {
                lock(&self.plan).calls.insert(
                    call_id(message).to_owned(),
                    Call::WrapUp {
                        blocked: message["params"]["arguments"]["blocked"] == true,
                    },
                );
                Ok(json!({
                    "content": [{"type": "text", "text": crate::wrap_up::ANSWER}],
                    "isError": false,
                }))
            }
            ("tools/call", _) if message["params"]["name"] == update_plan::TOOL => {
                Ok(self.update_plan(call_id(message), &message["params"]["arguments"]))
            }
            ("tools/call", Some(_)) if message["params"]["name"] == visual::TOOL => {
                let call = call_id(message).to_owned();
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
            ("tools/call", _) if message["params"]["name"] == crate::browser::tools::TOOL => {
                let stdin = Arc::clone(&self.stdin);
                let request_id = request_id.to_owned();
                let respond = move |answer: crate::browser::tools::Answer| {
                    let mut content = vec![json!({"type": "text", "text": answer.text})];
                    if let Some(png) = answer.png_base64() {
                        content
                            .push(json!({"type": "image", "data": png, "mimeType": "image/png"}));
                    }
                    respond_mcp(
                        &stdin,
                        &request_id,
                        id,
                        Ok(json!({"content": content, "isError": answer.is_error})),
                    );
                };
                match &self.browser {
                    Some(browser) => crate::browser::tools::answer(
                        browser,
                        &message["params"]["arguments"],
                        respond,
                    ),
                    None => respond(crate::browser::tools::Answer::unavailable()),
                }
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

/// The streamed `tool_use` id a call carries, which its receipt quotes.
fn call_id(message: &Value) -> &str {
    message["params"]["_meta"]["claudecode/toolUseId"]
        .as_str()
        .unwrap_or_default()
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
