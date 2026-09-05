//! One stdout line of `codex app-server` JSON-RPC to at most one SessionEvent.
//!
//! The vendor extends this protocol without notice, so every line is read as
//! a `Value` and anything unrecognised — new notification methods, changed
//! field types, outright junk — is silently nothing rather than an error.
//!
//! Two kinds of line matter. Notifications and server requests are stateless
//! and go through [`parse_line`]. *Responses* only mean something against the
//! request their id echoes, so the reader correlates the two handshake
//! responses itself and reads them with [`parse_response`] and
//! [`parse_thread_response`]; every other response — turn/start
//! acknowledgements, interrupt receipts — is ignored.

use std::{io, path::Path};

use serde_json::Value;

use super::CodexCapabilities;
use crate::{Decision, ModelInfo, SessionCommand, SessionEvent, ToolResult, TurnOutcome};

/// The item types Ferrite reads as tool runs. Everything else the server
/// wraps in an item — user messages, agent messages, reasoning — either
/// arrives as deltas already or is not modelled; parsing those items too
/// would double what the deltas said.
const TOOL_ITEM_TYPES: [&str; 2] = ["commandExecution", "fileChange"];

/// What the thread/start (or thread/resume) response said: the identity the
/// Session announces and the capabilities the operator may rely on.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ThreadHandshake {
    pub thread_id: String,
    pub model: String,
    pub capabilities: CodexCapabilities,
}

/// `None` means "nothing Ferrite models": MCP startup chatter, thread status
/// flips, retry diagnostics, acknowledgement responses, unparseable junk.
pub(super) fn parse_line(line: &str) -> Option<SessionEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    let method = value.get("method")?.as_str()?;
    let params = value.get("params")?;
    match method {
        "item/agentMessage/delta" => Some(SessionEvent::TextDelta {
            text: params.get("delta")?.as_str()?.to_string(),
        }),
        "item/reasoning/summaryTextDelta" => Some(SessionEvent::ReasoningSummaryDelta {
            text: params.get("delta")?.as_str()?.to_string(),
            summary_index: params.get("summaryIndex")?.as_u64()?,
        }),
        "item/started" => parse_item(params, false),
        "item/completed" => parse_item(params, true),
        "item/commandExecution/requestApproval" => {
            parse_approval_request(&value, params, "commandExecution")
        }
        "item/fileChange/requestApproval" => parse_approval_request(&value, params, "fileChange"),
        "thread/tokenUsage/updated" => parse_token_usage(params),
        "turn/completed" => parse_turn_completed(params),
        _ => None,
    }
}

/// A tool run, as the server reports one: the item announces itself when the
/// call settles and again when it finishes. `input` is the item exactly as
/// Codex shaped it — command and cwd for an execution, the per-file changes
/// for a patch — because inventing a Ferrite schema over it would be a guess
/// that goes stale on the vendor's next release.
fn parse_item(params: &Value, completed: bool) -> Option<SessionEvent> {
    let item = params.get("item")?;
    let kind = item.get("type")?.as_str()?;
    if !TOOL_ITEM_TYPES.contains(&kind) {
        return None;
    }
    let id = item.get("id")?.as_str()?.to_string();
    if !completed {
        return Some(SessionEvent::ToolStarted {
            id,
            name: kind.to_string(),
            input: item.clone(),
        });
    }
    Some(SessionEvent::ToolCompleted {
        id,
        // What the run produced: an execution reports its merged output
        // stream; a patch has no prose, so its changes stand in as compact
        // JSON.
        output: match item.get("aggregatedOutput") {
            Some(Value::String(text)) => text.clone(),
            _ => item
                .get("changes")
                .map(|changes| changes.to_string())
                .unwrap_or_default(),
        },
        // "completed" is the only success; "failed" and "declined" both mean
        // the tool did not do its work (a declined tool fails without failing
        // the turn — see the approval-deny fixture).
        is_error: item.get("status").and_then(Value::as_str) != Some("completed"),
        // Opaque by decision, not omission: Codex merges stdout and stderr
        // into one aggregate (not the two streams `ToolResult::Command`
        // promises), and its patches arrive as per-file diff *text*, not the
        // structured hunks `FileEdit` is built from. The committed fixtures
        // carry both shapes for whoever builds Codex diff cards.
        result: ToolResult::Opaque,
    })
}

/// The server blocks the turn on a Decision: a JSON-RPC request whose answer
/// is the operator's. `id` is the server's own request id, echoed back by
/// `respond_to_decision`; `tool_use_id` names the item the gate is holding;
/// `tool_name` is the gated item's type, which the routing method carries
/// and the params never repeat.
fn parse_approval_request(value: &Value, params: &Value, tool_name: &str) -> Option<SessionEvent> {
    Some(SessionEvent::DecisionRequested {
        decision: Decision {
            id: rpc_id_string(value.get("id")?)?,
            tool_use_id: params.get("itemId")?.as_str()?.to_string(),
            tool_name: tool_name.to_string(),
            // The provider's own one-liner: an execution approval quotes the
            // command; a patch approval offers at most a reason (observed
            // null), its changes living on the tool card `tool_use_id` names.
            description: ["command", "reason"]
                .iter()
                .find_map(|key| params.get(*key)?.as_str())
                .unwrap_or_default()
                .to_string(),
            input: params.clone(),
            // The standing answers Codex offers ("acceptForSession", execpolicy
            // amendments), raw and in its own words.
            suggestions: params
                .get("availableDecisions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        },
    })
}

/// Keep the ID's JSON representation as the opaque Decision handle. Encoding
/// strings, including their quotes and escapes, distinguishes integer `0`
/// from string `"0"` without changing the provider-neutral Decision type.
fn rpc_id_string(id: &Value) -> Option<String> {
    match id {
        Value::Number(_) | Value::String(_) => Some(id.to_string()),
        _ => None,
    }
}

/// Restore only a handle emitted by the approval decoder. Invalid handles
/// fail before a reply is written; treating them as raw strings would make
/// malformed handles address a different provider request.
pub(super) fn decision_request_id(handle: &str) -> io::Result<Value> {
    match serde_json::from_str(handle) {
        Ok(id @ (Value::Number(_) | Value::String(_))) => Ok(id),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid Codex Decision request handle",
        )),
    }
}

/// Context occupancy is the latest request (`last`); output accounting
/// remains cumulative so the Transcript can derive turn output separately.
fn parse_token_usage(params: &Value) -> Option<SessionEvent> {
    let usage = params.get("tokenUsage")?;
    let total = usage.get("total")?;
    let count = |key: &str| total.get(key).and_then(Value::as_u64).unwrap_or(0);
    Some(SessionEvent::TokenUsage {
        total_tokens: usage.get("last")?.get("totalTokens")?.as_u64()?,
        input_tokens: count("inputTokens"),
        cached_input_tokens: count("cachedInputTokens"),
        output_tokens: count("outputTokens"),
        reasoning_output_tokens: count("reasoningOutputTokens"),
        context_window: usage.get("modelContextWindow").and_then(Value::as_u64),
    })
}

/// A completed turn always ends the Session's turn, whatever its verdict.
/// `cost_usd` is `None` by construction: Codex accounts in tokens (see
/// `thread/tokenUsage/updated`), never in dollars.
fn parse_turn_completed(params: &Value) -> Option<SessionEvent> {
    let turn = params.get("turn")?;
    let status = turn.get("status").and_then(Value::as_str).unwrap_or("");
    let outcome = match status {
        "interrupted" => TurnOutcome::Interrupted,
        "failed" => TurnOutcome::Error(
            turn.get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("codex reported a failed turn with no detail")
                .to_string(),
        ),
        _ => TurnOutcome::Completed,
    };
    Some(SessionEvent::TurnEnded {
        outcome,
        cost_usd: None,
    })
}

/// The skills/list result: per-cwd entries flattened into one menu (#23).
/// Entry shape `{name, description, shortDescription?, path, scope, enabled,
/// interface?}`. Only enabled skills are offered — the menu is what can be
/// invoked — and an entry missing its name or path could never become the
/// typed `{"type":"skill"}` item an invocation needs, so it is skipped.
pub(super) fn parse_skills(result: &Value) -> Vec<SessionCommand> {
    let mut commands = Vec::new();
    let Some(data) = result.get("data").and_then(Value::as_array) else {
        return commands;
    };
    for entry in data {
        let Some(skills) = entry.get("skills").and_then(Value::as_array) else {
            continue;
        };
        for skill in skills {
            if skill.get("enabled").and_then(Value::as_bool) == Some(false) {
                continue;
            }
            let Some(name) = skill.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some(path) = skill.get("path").and_then(Value::as_str) else {
                continue;
            };
            commands.push(SessionCommand {
                name: name.to_string(),
                description: ["description", "shortDescription"]
                    .iter()
                    .find_map(|key| skill.get(*key)?.as_str())
                    .unwrap_or_default()
                    .to_string(),
                path: Some(path.to_string()),
            });
        }
    }
    commands
}

/// The model/list result as picker rows (#25): `{data: [{id, model,
/// displayName, description, hidden, supportedReasoningEfforts:
/// [{reasoningEffort, description}], defaultReasoningEffort, isDefault,
/// …}]}` on 0.144.4. The id is what thread/start's `model` accepts. The
/// server's displayName is hyphenated ("GPT-5.6-Sol"), so the row shows
/// the id groomed instead ("GPT-5.6 Sol"). Hidden rows are the server's
/// own business and stay off the menu; the default row leads, so a
/// Thread on the provider's default reads its ladder off the first row.
pub(super) fn parse_models(result: &Value) -> Vec<ModelInfo> {
    let mut models = Vec::new();
    let Some(data) = result.get("data").and_then(Value::as_array) else {
        return models;
    };
    for entry in data {
        if entry.get("hidden").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        let text = |key: &str| entry.get(key).and_then(Value::as_str).map(str::to_string);
        let row = ModelInfo {
            display: super::super::models::display_name(id),
            detail: text("description").unwrap_or_default(),
            resolved: text("model").filter(|model| model != id),
            efforts: entry
                .get("supportedReasoningEfforts")
                .and_then(Value::as_array)
                .map(|efforts| {
                    efforts
                        .iter()
                        .filter_map(|effort| effort.get("reasoningEffort")?.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            default_effort: text("defaultReasoningEffort"),
            value: id.to_string(),
        };
        if entry.get("isDefault").and_then(Value::as_bool) == Some(true) {
            models.insert(0, row);
        } else {
            models.push(row);
        }
    }
    models
}

/// One operator prompt as turn/start input items (#23). The server never
/// intercepts slash text — the wire study proved `/name args` goes to the
/// model as prose — so the translation is Ferrite's:
///
/// - a leading `/name` naming a listed skill becomes a typed
///   `{"type":"skill","name","path"}` item (exact, case-sensitive match —
///   mirroring the Claude CLI's own dispatch rules) and leaves the items;
/// - every whitespace-delimited `@path` token that names a file under the
///   Session's cwd rides as a `{"type":"mention","name","path"}` item —
///   decoration and persistence; the model reads the file itself;
/// - what remains is one `{"type":"text"}` item, kept verbatim (mention
///   tokens included — plain `@path` text is proven harmless).
pub(super) fn input_items(text: &str, skills: &[SessionCommand], cwd: Option<&Path>) -> Vec<Value> {
    let mut items = Vec::new();
    let mut rest = text;
    if let Some(after) = text.strip_prefix('/') {
        // The command token ends at the first whitespace; `"/ name"` names
        // nothing.
        let name = after.split(char::is_whitespace).next().unwrap_or("");
        let listed = skills
            .iter()
            .find(|skill| skill.name == name && skill.path.is_some());
        if let Some(skill) = listed {
            items.push(serde_json::json!({
                "type": "skill",
                "name": skill.name,
                "path": skill.path.as_deref().expect("filtered on is_some"),
            }));
            rest = after[name.len()..].trim_start();
        }
    }
    if let Some(cwd) = cwd {
        for token in rest.split_whitespace() {
            let Some(candidate) = token.strip_prefix('@') else {
                continue;
            };
            if candidate.is_empty() {
                continue;
            }
            let path = cwd.join(candidate);
            if !path.is_file() {
                continue;
            }
            let path = path.display().to_string();
            // The same file mentioned twice rides once.
            if items
                .iter()
                .any(|item| item["type"] == "mention" && item["path"] == path.as_str())
            {
                continue;
            }
            items.push(serde_json::json!({
                "type": "mention",
                "name": candidate.rsplit('/').next().unwrap_or(candidate),
                "path": path,
            }));
        }
    }
    if !rest.is_empty() || items.is_empty() {
        items.push(serde_json::json!({"type": "text", "text": rest}));
    }
    items
}

/// Is this line the response to request `id`? `Some(Ok)` carries its result,
/// `Some(Err)` the server's error message — a refused thread/start must fail
/// spawn with the server's own words, not a timeout. Server requests use a
/// separate ID space: a matching ID never makes a method-bearing frame a
/// response to one of our requests.
pub(super) fn parse_response(line: &str, id: u64) -> Option<Result<Value, String>> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("method").is_some() || value.get("id")?.as_u64()? != id {
        return None;
    }
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unexplained JSON-RPC error");
        return Some(Err(format!("{message} ({error})")));
    }
    Some(Ok(value.get("result")?.clone()))
}

/// The thread/start (or thread/resume) result: the Session's identity and its
/// feature detection. Only what Ferrite acts on is lifted out; the response
/// also carries instruction sources, workspace roots and — on resume — the
/// thread's whole recorded history under `thread.turns`, which nothing reads
/// yet (Thread revival is the store's business, not the Session's).
pub(super) fn parse_thread_response(result: &Value) -> Option<ThreadHandshake> {
    let thread_id = result.get("thread")?.get("id")?.as_str()?.to_string();
    let text = |key: &str| {
        result
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Some(ThreadHandshake {
        thread_id,
        model: text("model"),
        capabilities: CodexCapabilities {
            model: text("model"),
            model_provider: text("modelProvider"),
            // A plain mode on 0.149.1; the schema also allows a granular
            // object, kept as its compact JSON rather than flattened to "".
            approval_policy: match result.get("approvalPolicy") {
                Some(Value::String(mode)) => mode.clone(),
                Some(other) => other.to_string(),
                None => String::new(),
            },
            // The policy object's tag ("readOnly", "workspaceWrite",
            // "dangerFullAccess"); its roots and network flags are not acted
            // on yet.
            sandbox: result
                .get("sandbox")
                .and_then(|s| s.get("type"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            reasoning_effort: result
                .get("reasoningEffort")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/codex-hello-0.149.1.jsonl");

    /// Every committed capture of `codex app-server` 0.149.1. These are the
    /// protocol contract: a vendor release that changes the wire changes these
    /// numbers, which is the alarm working.
    const FIXTURES: &[(&str, &str)] = &[
        ("hello", FIXTURE),
        (
            "tool",
            include_str!("../../../tests/fixtures/codex-tool-0.149.1.jsonl"),
        ),
        (
            "approval-allow",
            include_str!("../../../tests/fixtures/codex-approval-allow-0.149.1.jsonl"),
        ),
        (
            "approval-deny",
            include_str!("../../../tests/fixtures/codex-approval-deny-0.149.1.jsonl"),
        ),
        (
            "approval-always",
            include_str!("../../../tests/fixtures/codex-approval-always-0.149.1.jsonl"),
        ),
        (
            "approval-patch",
            include_str!("../../../tests/fixtures/codex-approval-patch-0.149.1.jsonl"),
        ),
        (
            "interrupt",
            include_str!("../../../tests/fixtures/codex-interrupt-0.149.1.jsonl"),
        ),
        (
            "resume",
            include_str!("../../../tests/fixtures/codex-resume-0.149.1.jsonl"),
        ),
        (
            "error",
            include_str!("../../../tests/fixtures/codex-error-0.149.1.jsonl"),
        ),
    ];

    const INITIALIZE: &str = include_str!("../../../tests/fixtures/codex-initialize-0.149.1.jsonl");

    /// The skills/list response, in the shape the #23 wire study captured
    /// (per-cwd entries; `{name, description, shortDescription?, path, scope,
    /// enabled, interface?}`), paths neutralised like every other fixture.
    const SKILLS: &str = include_str!("../../../tests/fixtures/codex-skills-0.149.1.jsonl");

    /// The model/list response, as `codex app-server` 0.144.4 answered the
    /// probe (Ferrite's pin is later, but the shape is the schema's).
    const MODELS: &str = include_str!("../../../tests/fixtures/codex-models-0.144.4.jsonl");

    /// The model/list result as the reader correlates it: request id 4 is
    /// the one the session numbers it with.
    fn models_result() -> Value {
        MODELS
            .lines()
            .find_map(|line| parse_response(line, 4))
            .expect("the fixture answers request 4")
            .expect("with a result, not an error")
    }

    /// The captured menu, row for row: the default leads, names are the ids
    /// groomed, each row's own effort ladder and default ride along, and a
    /// hidden row never reaches the picker.
    #[test]
    fn the_model_list_becomes_picker_rows_with_their_effort_ladders() {
        let models = parse_models(&models_result());
        let values: Vec<&str> = models.iter().map(|row| row.value.as_str()).collect();
        assert_eq!(
            values,
            [
                "gpt-5.6-sol",
                "gpt-5.6-terra",
                "gpt-5.6-luna",
                "gpt-5.5",
                "gpt-5.4",
                "gpt-5.4-mini",
                "gpt-5.3-codex-spark",
            ]
        );
        let sol = &models[0];
        assert_eq!(sol.display, "GPT-5.6 Sol");
        assert_eq!(sol.detail, "Reliable agentic workhorse for everyday tasks.");
        assert_eq!(
            sol.efforts,
            ["low", "medium", "high", "xhigh", "max", "ultra"]
        );
        assert_eq!(sol.default_effort.as_deref(), Some("low"));
        assert_eq!(sol.resolved, None, "id and model agree");
        let luna = &models[2];
        assert_eq!(luna.display, "GPT-5.6 Luna");
        assert_eq!(luna.efforts, ["low", "medium", "high", "xhigh", "max"]);
        let mini = &models[5];
        assert_eq!(mini.display, "GPT-5.4 Mini");
        assert_eq!(mini.efforts, ["low", "medium", "high", "xhigh"]);
        assert_eq!(mini.default_effort.as_deref(), Some("medium"));

        // A hidden row and a nameless one stay off the menu; a non-leading
        // default is moved to the front.
        let doctored = serde_json::json!({"data": [
            {"id": "gpt-x", "hidden": true},
            {"model": "nameless"},
            {"id": "gpt-5.4", "hidden": false},
            {"id": "gpt-5.6-sol", "isDefault": true},
        ]});
        let rows = parse_models(&doctored);
        let values: Vec<&str> = rows.iter().map(|row| row.value.as_str()).collect();
        assert_eq!(values, ["gpt-5.6-sol", "gpt-5.4"]);
        assert!(rows[1].efforts.is_empty());
        assert_eq!(
            parse_models(&serde_json::json!({})),
            Vec::<ModelInfo>::new()
        );
    }

    /// The skills/list result as the reader correlates it: request id 3 is
    /// the one the session numbers it with.
    fn skills_result() -> Value {
        SKILLS
            .lines()
            .find_map(|line| parse_response(line, 3))
            .expect("the fixture answers request 3")
            .expect("with a result, not an error")
    }

    fn fixture_events() -> Vec<SessionEvent> {
        FIXTURE.lines().filter_map(parse_line).collect()
    }

    fn events_of(name: &str) -> Vec<SessionEvent> {
        let (_, text) = FIXTURES
            .iter()
            .find(|(fixture, _)| *fixture == name)
            .expect("known fixture");
        text.lines().filter_map(parse_line).collect()
    }

    /// The thread/start response in a capture, as the reader would correlate
    /// it: request id 2 is the one the session numbers it with.
    fn handshake_of(text: &str) -> ThreadHandshake {
        let result = text
            .lines()
            .find_map(|line| parse_response(line, 2))
            .expect("the capture answers request 2")
            .expect("with a result, not an error");
        parse_thread_response(&result).expect("a thread and its capabilities")
    }

    /// Exhaustive by construction: a new SessionEvent variant fails to compile
    /// here until someone decides which codex fixture proves it — or records
    /// that no codex line can.
    fn variant(event: &SessionEvent) -> Option<&'static str> {
        Some(match event {
            // Attributed observations require the stateful ancestry decoder;
            // its real child captures are covered in activity::tests.
            SessionEvent::Activity(_) => return None,
            SessionEvent::Init { .. } => "Init",
            SessionEvent::TextDelta { .. } => "TextDelta",
            SessionEvent::ReasoningSummaryDelta { .. } => "ReasoningSummaryDelta",
            SessionEvent::ToolStarted { .. } => "ToolStarted",
            SessionEvent::ToolCompleted { .. } => "ToolCompleted",
            SessionEvent::DecisionRequested { .. } => "DecisionRequested",
            SessionEvent::TokenUsage { .. } => "TokenUsage",
            SessionEvent::TurnEnded { .. } => "TurnEnded",
            // The skills menu: a correlated response like Init, read by the
            // reader against its own request id, so it is chained into the
            // produced list from the SKILLS fixture the same way.
            SessionEvent::Commands { .. } => "Commands",
            // Claude's concept (#23): the mode chip reads Claude's initialize
            // handshake; Codex's approval policy lives in its capabilities
            // and is deliberately not surfaced as this event.
            SessionEvent::PermissionMode { .. } => return None,
            // Claude's concept too (#25): Codex announces only its serving
            // model, never a list, so its picker offers no model rows.
            SessionEvent::Models { .. } => return None,
            // Claude's concept: Codex never streams raw chain-of-thought, only
            // summaries of it, so no codex line may ever produce this — that
            // is the capability difference, stated rather than papered over.
            SessionEvent::ThinkingDelta { .. } => return None,
            // Not a wire line at all: the reader thread synthesises Closed
            // when the process exits, so no capture can contain it. Proved by
            // the session tests instead.
            SessionEvent::Closed { .. } => return None,
        })
    }

    #[test]
    fn fixture_text_deltas_concatenate_to_the_answer() {
        let text: String = fixture_events()
            .into_iter()
            .filter_map(|e| match e {
                SessionEvent::TextDelta { text } => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(text, "hello ferrite");
    }

    /// Codex's reasoning arrives as model-authored summaries, not thinking:
    /// the fixture's one summary part, exactly as the wire spelled it.
    #[test]
    fn fixture_carries_a_reasoning_summary() {
        let summaries: Vec<_> = fixture_events()
            .into_iter()
            .filter_map(|e| match e {
                SessionEvent::ReasoningSummaryDelta {
                    text,
                    summary_index,
                } => Some((text, summary_index)),
                _ => None,
            })
            .collect();
        assert_eq!(
            summaries,
            [("**Confirming exact output requirement**".to_string(), 0)]
        );
    }

    /// The latest context size and cumulative output counters from the wire.
    #[test]
    fn context_usage_tracks_the_latest_window_while_output_totals_keep_growing() {
        let mut params = serde_json::json!({"tokenUsage": {
            "total": {"totalTokens": 900_000, "inputTokens": 850_000, "outputTokens": 50_000},
            "last": {"totalTokens": 124_000}, "modelContextWindow": 200_000
        }});
        for current in [124_000, 31_000, 0] {
            params["tokenUsage"]["last"]["totalTokens"] = current.into();
            let event = parse_line(
                &serde_json::json!({
                    "method": "thread/tokenUsage/updated", "params": params
                })
                .to_string(),
            )
            .unwrap();
            assert!(matches!(event, SessionEvent::TokenUsage {
                total_tokens, output_tokens: 50_000, context_window: Some(200_000), ..
            } if total_tokens == current));
        }
    }

    #[test]
    fn fixture_reports_token_usage_against_the_context_window() {
        let usage: Vec<_> = fixture_events()
            .into_iter()
            .filter(|e| matches!(e, SessionEvent::TokenUsage { .. }))
            .collect();
        assert_eq!(
            usage,
            [SessionEvent::TokenUsage {
                total_tokens: 14740,
                input_tokens: 14703,
                cached_input_tokens: 4480,
                output_tokens: 37,
                reasoning_output_tokens: 28,
                context_window: Some(258400),
            }]
        );
    }

    #[test]
    fn fixture_ends_with_a_completed_turn_and_no_dollar_cost() {
        let events = fixture_events();
        assert_eq!(
            events.last(),
            Some(&SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            })
        );
    }

    /// The whole point of the fixture harness: nothing in the typed surface is
    /// aspirational — every variant a codex line can produce has a committed
    /// capture behind it. Init is proved through the handshake path, which is
    /// how the reader actually reads it.
    #[test]
    fn every_codex_session_event_variant_is_produced_by_a_fixture() {
        let handshake = handshake_of(INITIALIZE);
        let init = SessionEvent::Init {
            session_id: handshake.thread_id,
            model: handshake.model,
        };
        let menu = SessionEvent::Commands {
            commands: parse_skills(&skills_result()),
        };
        let mut produced: Vec<&str> = FIXTURES
            .iter()
            .flat_map(|(_, text)| text.lines().filter_map(parse_line))
            .chain([init, menu])
            .filter_map(|event| variant(&event))
            .collect();
        produced.sort_unstable();
        produced.dedup();
        assert_eq!(
            produced,
            [
                "Commands",
                "DecisionRequested",
                "Init",
                "ReasoningSummaryDelta",
                "TextDelta",
                "TokenUsage",
                "ToolCompleted",
                "ToolStarted",
                "TurnEnded",
            ]
        );
    }

    /// Most of the stream is not Ferrite's business — MCP startup chatter,
    /// thread status flips, remote-control state, retry diagnostics.
    /// Recording how much of each capture is ignored is what proves an
    /// unknown line costs nothing.
    #[test]
    fn every_fixture_ignores_far_more_lines_than_it_models() {
        let counted: Vec<(&str, usize, usize)> = FIXTURES
            .iter()
            .map(|(name, text)| {
                (
                    *name,
                    text.lines().count(),
                    text.lines().filter_map(parse_line).count(),
                )
            })
            .collect();
        assert_eq!(
            counted,
            [
                // 3 text deltas, 1 summary delta, 1 usage, 1 turn end.
                ("hello", 29, 6),
                // ... plus a command run and its completion.
                ("tool", 46, 22),
                // ... plus the Decision that gated the command.
                ("approval-allow", 44, 17),
                ("approval-deny", 66, 37),
                // The same command twice, gated once: the execpolicy amendment
                // was accepted, so the repeat ran unasked.
                ("approval-always", 75, 45),
                // The same gate for a patch instead of a command.
                ("approval-patch", 52, 20),
                ("interrupt", 25, 3),
                ("resume", 32, 9),
                // Ten retry errors and a warning, all ignored: only the failed
                // turn itself is an event.
                ("error", 22, 1),
            ]
        );
    }

    /// A command run, as `codex` 0.149.1 actually reports one: the settled
    /// command arrives when the item starts, and the merged output stream
    /// comes back on its completion.
    #[test]
    fn the_tool_fixture_yields_a_start_and_a_completion_that_agree() {
        let events = events_of("tool");
        let SessionEvent::ToolStarted { id, name, input } = events
            .iter()
            .find(|e| matches!(e, SessionEvent::ToolStarted { .. }))
            .expect("a tool start")
        else {
            unreachable!()
        };
        assert_eq!(name, "commandExecution");
        assert_eq!(input["command"], "/bin/zsh -lc 'echo ferrite-tool-ok'");
        assert!(events.contains(&SessionEvent::ToolCompleted {
            id: id.clone(),
            output: "ferrite-tool-ok\n".into(),
            is_error: false,
            result: ToolResult::Opaque,
        }));
    }

    /// A Decision: the server stops and asks whether a command may run.
    /// Replayed from the committed `approval-allow` capture, which recorded
    /// the real JSON-RPC request.
    #[test]
    fn an_approval_request_arrives_as_a_decision_naming_its_tool_call() {
        let events = events_of("approval-allow");
        let decisions: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, SessionEvent::DecisionRequested { .. }))
            .collect();
        assert_eq!(decisions.len(), 1, "expected one Decision: {events:?}");
        let SessionEvent::DecisionRequested {
            decision:
                Decision {
                    id,
                    tool_use_id,
                    tool_name,
                    description,
                    input,
                    suggestions,
                },
        } = decisions[0]
        else {
            unreachable!()
        };

        // The server's own request id, in its own (integer) id space.
        assert_eq!(id, "0");
        assert_eq!(tool_name, "commandExecution");
        assert_eq!(
            description,
            "/bin/zsh -lc \"printf 'ok' > ferrite-perm.txt\""
        );
        assert_eq!(input["cwd"], "/workspace");
        // The standing answers 0.149.1 offers: accept, accept with an
        // execpolicy amendment, decline.
        assert_eq!(suggestions.len(), 3);
        assert!(suggestions.contains(&serde_json::json!("accept")));

        // The Decision names the tool card it blocks, so a Pane can render it
        // in place instead of as a free-floating prompt.
        assert!(
            events.iter().any(|e| matches!(
                e,
                SessionEvent::ToolStarted { id, .. } if id == tool_use_id
            )),
            "no ToolStarted for {tool_use_id}: {events:?}"
        );
    }

    #[test]
    fn decision_handles_preserve_id_types_and_string_escapes() {
        let ids = [
            serde_json::json!(0),
            serde_json::json!("0"),
            serde_json::json!(-1),
            serde_json::json!("-1"),
            serde_json::json!(i64::MAX),
            serde_json::json!(""),
            serde_json::json!("request \"quoted\"\n\\slash"),
            serde_json::json!("\"0\""),
        ];
        for method in [
            "item/commandExecution/requestApproval",
            "item/fileChange/requestApproval",
        ] {
            let mut handles = std::collections::HashSet::new();
            for id in &ids {
                let line = serde_json::json!({
                    "method": method, "id": id, "params": {"itemId": "gated-item"}
                });
                let Some(SessionEvent::DecisionRequested { decision }) =
                    parse_line(&line.to_string())
                else {
                    panic!("valid request did not produce a Decision: {line}");
                };
                assert_eq!(decision_request_id(&decision.id).unwrap(), *id);
                assert!(handles.insert(decision.id), "request IDs collided: {id}");
            }
        }
    }

    #[test]
    fn invalid_decision_ids_are_not_accepted_or_reinterpreted_as_strings() {
        for id in [
            serde_json::json!(null),
            serde_json::json!(true),
            serde_json::json!([]),
            serde_json::json!({}),
        ] {
            let line = serde_json::json!({
                "method": "item/commandExecution/requestApproval", "id": id,
                "params": {"itemId": "gated-item"}
            });
            assert!(parse_line(&line.to_string()).is_none());
            assert_eq!(
                decision_request_id(&id.to_string()).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
        for handle in ["", "raw-request-id", "01", "\"unterminated", "0 trailing"] {
            assert_eq!(
                decision_request_id(handle).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    /// The same gate for a patch: no command to quote (and a null reason), so
    /// the Decision's description is honestly empty — the changes live on the
    /// fileChange tool card it points at.
    #[test]
    fn a_patch_approval_is_a_decision_on_the_file_change_item() {
        let events = events_of("approval-patch");
        let SessionEvent::DecisionRequested {
            decision:
                Decision {
                    tool_use_id,
                    tool_name,
                    description,
                    suggestions,
                    ..
                },
        } = events
            .iter()
            .find(|e| matches!(e, SessionEvent::DecisionRequested { .. }))
            .expect("a Decision")
        else {
            unreachable!()
        };
        assert_eq!(tool_name, "fileChange");
        assert_eq!(description, "");
        assert_eq!(suggestions, &Vec::<Value>::new());

        let SessionEvent::ToolStarted { input, .. } = events
            .iter()
            .find(|e| matches!(e, SessionEvent::ToolStarted { id, .. } if id == tool_use_id))
            .expect("the gated fileChange item")
        else {
            unreachable!()
        };
        assert_eq!(input["changes"][0]["path"], "/workspace/ferrite-patch.txt");
        assert_eq!(input["changes"][0]["kind"]["type"], "add");
    }

    /// Denial is not failure: the declined command completes as an error and
    /// the turn runs to a normal end with the model talking about it.
    #[test]
    fn a_declined_tool_fails_without_failing_the_turn() {
        let events = events_of("approval-deny");
        assert!(
            events.iter().any(|e| matches!(
                e,
                SessionEvent::ToolCompleted { is_error: true, output, .. } if output.is_empty()
            )),
            "no declined tool result: {events:?}"
        );
        assert_eq!(
            events.last(),
            Some(&SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            })
        );
    }

    /// Captured by interrupting a streaming turn: unlike Claude, the server
    /// says "interrupted" in so many words.
    #[test]
    fn an_interrupted_turn_ends_as_interrupted() {
        assert_eq!(
            events_of("interrupt").last(),
            Some(&SessionEvent::TurnEnded {
                outcome: TurnOutcome::Interrupted,
                cost_usd: None,
            })
        );
    }

    /// A failed turn has to say what failed, in the server's own words.
    #[test]
    fn a_failed_turn_ends_with_the_reason_the_server_gave() {
        let events = events_of("error");
        let Some(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Error(reason),
            cost_usd: None,
        }) = events.last()
        else {
            panic!("expected a failed turn: {events:?}");
        };
        assert!(
            reason.contains("401 Unauthorized"),
            "reason should quote the server: {reason}"
        );
    }

    /// The resume capture is the model answering from history a fresh process
    /// never saw: the codeword planted by the (unrecorded) setup turn.
    #[test]
    fn the_resume_fixture_answers_from_the_previous_process_history() {
        let text: String = events_of("resume")
            .into_iter()
            .filter_map(|e| match e {
                SessionEvent::TextDelta { text } => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(text, "ferrite-resume-ok");
    }

    /// Feature detection from the committed initialize capture: exactly what
    /// this install said it would do — nothing phantom.
    #[test]
    fn the_thread_response_is_read_for_what_ferrite_acts_on() {
        let handshake = handshake_of(INITIALIZE);
        assert!(!handshake.thread_id.is_empty());
        assert_eq!(handshake.model, "gpt-5.4-mini");
        assert_eq!(
            handshake.capabilities,
            CodexCapabilities {
                model: "gpt-5.4-mini".into(),
                model_provider: "openai".into(),
                approval_policy: "on-request".into(),
                sandbox: "workspaceWrite".into(),
                // The recording machine's configured effort, reported rather
                // than assumed.
                reasoning_effort: Some("xhigh".into()),
            }
        );
    }

    /// The resume response carries the same handshake shape — and the thread
    /// id is the one that was resumed, which is what makes it a resume.
    #[test]
    fn the_resume_response_names_the_resumed_thread() {
        let (_, resume) = FIXTURES
            .iter()
            .find(|(name, _)| *name == "resume")
            .expect("known fixture");
        let handshake = handshake_of(resume);
        let host = include_str!("../../../tests/fixtures/codex-resume-0.149.1.host.jsonl");
        let request: Value = host
            .lines()
            .find(|line| line.contains("thread/resume"))
            .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
            .expect("the capture resumed a thread");
        assert_eq!(
            request["params"]["threadId"].as_str(),
            Some(handshake.thread_id.as_str()),
            "the recorded resume asked for a different thread"
        );
    }

    /// An empty response claims nothing: every capability reads as unknown
    /// rather than invented — but without a thread id there is no session,
    /// so that is not a handshake at all.
    #[test]
    fn a_thread_response_without_a_thread_is_no_handshake() {
        assert_eq!(parse_thread_response(&serde_json::json!({})), None);
        assert_eq!(
            parse_thread_response(&serde_json::json!({"thread": {"id": "t"}})),
            Some(ThreadHandshake {
                thread_id: "t".into(),
                model: String::new(),
                capabilities: CodexCapabilities {
                    model: String::new(),
                    model_provider: String::new(),
                    approval_policy: String::new(),
                    sandbox: String::new(),
                    reasoning_effort: None,
                },
            })
        );
    }

    /// Responses are correlated, never guessed: the wrong id, a notification,
    /// or junk is not the answer, and a JSON-RPC error comes back as the
    /// server's own words.
    #[test]
    fn responses_answer_only_their_own_request() {
        assert_eq!(parse_response(r#"{"id":3,"result":{}}"#, 2), None);
        assert_eq!(
            parse_response(r#"{"method":"thread/started","params":{}}"#, 2),
            None
        );
        assert_eq!(parse_response("not json", 2), None);
        assert_eq!(
            parse_response(r#"{"id":2,"result":{"thread":{}}}"#, 2),
            Some(Ok(serde_json::json!({"thread": {}})))
        );
        let error = parse_response(
            r#"{"id":2,"error":{"code":-32600,"message":"no such model"}}"#,
            2,
        );
        let Some(Err(detail)) = error else {
            panic!("an error response must surface: {error:?}");
        };
        assert!(detail.contains("no such model"), "detail: {detail}");
    }

    #[test]
    fn server_requests_do_not_answer_host_requests_with_matching_ids() {
        for id in [1, 2, 3, 4] {
            let request = serde_json::json!({
                "id": id, "method": "item/commandExecution/requestApproval",
                "params": {"itemId": "gated-item"}
            });
            assert_eq!(parse_response(&request.to_string(), id), None);
            assert!(matches!(
                parse_line(&request.to_string()),
                Some(SessionEvent::DecisionRequested { .. })
            ));
            for payload in [
                serde_json::json!({"result": {}}),
                serde_json::json!({"error": {"message": "error"}}),
            ] {
                let mut mixed = request.clone();
                mixed
                    .as_object_mut()
                    .unwrap()
                    .extend(payload.as_object().unwrap().clone());
                assert_eq!(
                    parse_response(&mixed.to_string(), id),
                    None,
                    "a method-bearing frame cannot be a host response"
                );
            }
        }
    }

    #[test]
    fn response_envelopes_require_a_result_or_error() {
        assert_eq!(parse_response(r#"{"id":3}"#, 3), None);
        assert_eq!(parse_response(r#"{"id":3,"params":{}}"#, 3), None);
        assert_eq!(parse_response(r#"{"id":"3","result":null}"#, 3), None);
        assert_eq!(
            parse_response(r#"{"id":3,"result":null}"#, 3),
            Some(Ok(Value::Null))
        );
        assert!(
            parse_response(r#"{"id":3,"error":{"message":"refused"}}"#, 3)
                .unwrap()
                .unwrap_err()
                .contains("refused")
        );
    }

    #[test]
    fn junk_lines_are_ignored_never_fatal() {
        for line in [
            "",
            "not json at all",
            "{}",
            r#"{"method":42}"#,
            r#"{"method":"item/agentMessage/delta"}"#,
            r#"{"method":"item/agentMessage/delta","params":{"delta":7}}"#,
            r#"{"method":"brand/new/vendor/method","params":{}}"#,
            // Items Ferrite does not model: the deltas already carried these.
            r#"{"method":"item/started","params":{"item":{"type":"agentMessage","id":"m","text":"hi"}}}"#,
            r#"{"method":"item/completed","params":{"item":{"type":"reasoning","id":"r"}}}"#,
            r#"{"method":"item/started","params":{"item":{"type":"webSearch","id":"w","query":"q"}}}"#,
            // Server requests Ferrite does not answer (yet): ignoring one
            // strands that request, but the stream keeps flowing.
            r#"{"method":"item/tool/requestUserInput","id":9,"params":{}}"#,
            // An approval with no id could never be answered, so it is not a
            // Decision.
            r#"{"method":"item/commandExecution/requestApproval","params":{"itemId":"c"}}"#,
            // Responses are the reader's business only while it holds a
            // pending request; a stray one is nothing.
            r#"{"id":7,"result":{}}"#,
            // The turn lifecycle lines Ferrite reads nothing from.
            r#"{"method":"turn/started","params":{"turn":{"id":"t"}}}"#,
            r#"{"method":"thread/started","params":{"thread":{"id":"t"}}}"#,
            r#"{"method":"error","params":{"error":{"message":"Reconnecting... 2/5"},"willRetry":true}}"#,
        ] {
            assert_eq!(parse_line(line), None, "line should be ignored: {line}");
        }
    }

    /// A turn that ends in an unfamiliar shape still ends: missing status
    /// reads as completed, a failure without detail says so.
    #[test]
    fn unfamiliar_turn_completions_still_end_the_turn() {
        assert_eq!(
            parse_line(r#"{"method":"turn/completed","params":{"turn":{}}}"#),
            Some(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            })
        );
        assert_eq!(
            parse_line(r#"{"method":"turn/completed","params":{"turn":{"status":"failed"}}}"#),
            Some(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Error("codex reported a failed turn with no detail".into()),
                cost_usd: None,
            })
        );
    }

    /// #23: the skills/list result flattens to the `/` menu — enabled skills
    /// only, name + description + the path a typed invocation needs.
    #[test]
    fn the_skills_response_is_read_as_the_command_menu() {
        let commands = parse_skills(&skills_result());
        let names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "probe-codex-skill",
                "probe-body",
                "browser:control-in-app-browser"
            ],
            "disabled skills are not offered"
        );
        assert_eq!(
            commands[0].description,
            "Probe skill for wire test; reply MARKER-CODEX-SKILL when invoked"
        );
        assert_eq!(
            commands[0].path.as_deref(),
            Some("/workspace/.codex/skills/probe-codex-skill/SKILL.md"),
            "the path is what the typed skill item carries"
        );
    }

    /// A menu that says nothing offers nothing — junk shapes included.
    #[test]
    fn unreadable_skills_responses_offer_no_menu() {
        for result in [
            serde_json::json!({}),
            serde_json::json!({"data": "not an array"}),
            serde_json::json!({"data": [{"cwd": "/workspace"}]}),
            // A skill without a path could never be invoked as a typed item.
            serde_json::json!({"data": [{"skills": [{"name": "pathless"}]}]}),
            serde_json::json!({"data": [{"skills": [{"path": "/nameless/SKILL.md"}]}]}),
        ] {
            assert_eq!(parse_skills(&result), [], "should offer nothing: {result}");
        }
    }

    /// #23 wire law: a picked skill rides as the typed item, never as slash
    /// text — the exact items the live probe proved end-to-end
    /// (`[{"type":"skill",…},{"type":"text",…}]`, args only in the text).
    #[test]
    fn a_leading_listed_skill_becomes_the_typed_item_with_its_args_as_text() {
        let skills = parse_skills(&skills_result());

        assert_eq!(
            input_items("/probe-body follow the skill", &skills, None),
            [
                serde_json::json!({
                    "type": "skill",
                    "name": "probe-body",
                    "path": "/workspace/.codex/skills/probe-body/SKILL.md",
                }),
                serde_json::json!({"type": "text", "text": "follow the skill"}),
            ]
        );
        // Bare invocation: the pointer item alone, no empty text item.
        assert_eq!(
            input_items("/probe-body", &skills, None),
            [serde_json::json!({
                "type": "skill",
                "name": "probe-body",
                "path": "/workspace/.codex/skills/probe-body/SKILL.md",
            })]
        );
    }

    /// Everything that is not a listed leading skill stays plain text — the
    /// same rules the Claude CLI applies to its own dispatch (leading token
    /// only, case-sensitive, whole name).
    #[test]
    fn unlisted_or_malformed_slash_text_stays_plain_text() {
        let skills = parse_skills(&skills_result());
        for text in [
            "/definitely-not-a-command foo",
            "/PROBE-BODY shout",        // case-sensitive
            "/probe-bod short",         // whole-token match only
            "/ probe-body spaced",      // `"/ name"` names nothing
            "say ok about /probe-body", // leading token only
        ] {
            assert_eq!(
                input_items(text, &skills, None),
                [serde_json::json!({"type": "text", "text": text})],
                "{text}"
            );
        }
    }

    /// #23 wire law: a picked file rides as a `{"type":"mention"}` item —
    /// resolved against the Session's cwd, decoration beside the verbatim
    /// text — and a token that names no file rides as nothing.
    #[test]
    fn at_tokens_naming_real_files_ride_as_mention_items() {
        let dir = std::env::temp_dir().join(format!("ferrite-mention-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::write(dir.join("docs/notes.txt"), "MAGIC\n").unwrap();

        let mentioned = dir.join("docs/notes.txt").display().to_string();
        assert_eq!(
            input_items("what is in @docs/notes.txt ?", &[], Some(&dir)),
            [
                serde_json::json!({"type": "mention", "name": "notes.txt", "path": mentioned}),
                serde_json::json!({"type": "text", "text": "what is in @docs/notes.txt ?"}),
            ]
        );
        // A missing file, a bare @, an interior @ — plain text, no item.
        for text in ["read @docs/absent.txt", "just an @", "mail a@b.example"] {
            assert_eq!(
                input_items(text, &[], Some(&dir)),
                [serde_json::json!({"type": "text", "text": text})],
                "{text}"
            );
        }
        // With no cwd there is nothing to resolve against.
        assert_eq!(
            input_items("read @docs/notes.txt", &[], None),
            [serde_json::json!({"type": "text", "text": "read @docs/notes.txt"})]
        );
    }

    /// A skill and a mention compose: skill first, mentions, then the args
    /// text with its tokens kept verbatim; the same file twice rides once.
    #[test]
    fn skills_and_mentions_compose_into_one_items_array() {
        let dir = std::env::temp_dir().join(format!("ferrite-mention-mix-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), "x\n").unwrap();
        let skills = parse_skills(&skills_result());

        let mentioned = dir.join("notes.txt").display().to_string();
        assert_eq!(
            input_items(
                "/probe-body read @notes.txt then @notes.txt again",
                &skills,
                Some(&dir)
            ),
            [
                serde_json::json!({
                    "type": "skill",
                    "name": "probe-body",
                    "path": "/workspace/.codex/skills/probe-body/SKILL.md",
                }),
                serde_json::json!({"type": "mention", "name": "notes.txt", "path": mentioned}),
                serde_json::json!({
                    "type": "text",
                    "text": "read @notes.txt then @notes.txt again",
                }),
            ]
        );
    }
}
