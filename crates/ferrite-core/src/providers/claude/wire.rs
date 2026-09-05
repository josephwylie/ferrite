//! One stdout line of the CLI's stream-json to at most one SessionEvent.
//!
//! The vendor extends this stream without notice, so every line is read as a
//! `Value` and anything unrecognised — new event types, changed field types,
//! outright junk — is silently nothing rather than an error.

use serde_json::Value;

use super::ClaudeCapabilities;
use crate::{Decision, Hunk, RateLimitWindow, SessionEvent, ToolResult, TurnOutcome};

/// The answer to spawn's initialize control request, if this line is it.
///
/// The response carries the CLI's whole configured surface — commands,
/// subagents, output styles, account; only the two fields Ferrite acts on are
/// lifted out, and a response missing them yields defaults rather than an
/// error, because an unknown capability must read as unknown.
pub(super) fn parse_capabilities(line: &str, request_id: &str) -> Option<ClaudeCapabilities> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "control_response" {
        return None;
    }
    let response = value.get("response")?;
    if response.get("request_id")?.as_str()? != request_id {
        return None;
    }
    let body = response.get("response")?;
    Some(ClaudeCapabilities {
        permission_mode: body
            .get("current_permission_mode")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        // Each entry: `{value, resolvedModel, displayName, description,
        // supportsEffort, supportedEffortLevels, …}`. The value is what
        // `--model` accepts. The CLI's own displayName is unversioned
        // ("Fable", "Opus (1M context)"), so the row shows the resolved id
        // groomed instead ("Fable 5.1", "Opus 5 (1M)") — the `default` alias
        // says what it stands for. The description repeats that name up
        // front, and a row that shows it once drops the repeat.
        models: body
            .get("models")
            .and_then(Value::as_array)
            .map(|models| {
                models
                    .iter()
                    .filter_map(|model| {
                        let value = model.get("value")?.as_str()?.to_string();
                        let text =
                            |key: &str| model.get(key).and_then(Value::as_str).map(str::to_string);
                        let resolved = text("resolvedModel").filter(|id| !id.is_empty());
                        let name = resolved
                            .as_deref()
                            .map(super::super::models::display_name)
                            .or_else(|| text("displayName").filter(|name| !name.is_empty()))
                            .unwrap_or_else(|| super::super::models::display_name(&value));
                        let display = if value == "default" && resolved.is_some() {
                            format!("Default · {name}")
                        } else {
                            name.clone()
                        };
                        let efforts = model
                            .get("supportedEffortLevels")
                            .and_then(Value::as_array)
                            .map(|levels| {
                                levels
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_string)
                                    .collect()
                            })
                            .unwrap_or_default();
                        Some(crate::ModelInfo {
                            display,
                            detail: super::super::models::detail_without_name(
                                &text("description").unwrap_or_default(),
                                &name,
                            ),
                            resolved,
                            efforts,
                            default_effort: None,
                            value,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        // The CLI's effective menu — one list mixing built-ins, skills,
        // project commands and plugins, `skillOverrides` already applied
        // (#23 wire study §1). Entry shape `{name, description, argumentHint,
        // aliases?}`; only what the `/` menu shows is lifted, and an entry
        // with no name could never be typed, so it is skipped.
        commands: body
            .get("commands")
            .and_then(Value::as_array)
            .map(|commands| {
                commands
                    .iter()
                    .filter_map(|command| {
                        Some(crate::SessionCommand {
                            name: command.get("name")?.as_str()?.to_string(),
                            description: command
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            path: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// `None` means "nothing Ferrite models": hook chatter, status lines,
/// assistant snapshots, rate limits, unparseable junk.
pub(super) fn parse_line(line: &str) -> Option<SessionEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    match value.get("type")?.as_str()? {
        "system" => parse_system(&value),
        "stream_event" => parse_stream_event(&value),
        "assistant" => parse_assistant(&value),
        "user" => parse_user(&value),
        "control_request" => parse_control_request(&value),
        "result" => Some(parse_result(&value)),
        _ => None,
    }
}

/// The token count a line carries beside its event, if any: every
/// `assistant` message reports its own `usage` (the prompt it was given —
/// input plus cache reads and writes — is the context in use at that
/// point), and the `result` line reports the turn's totals with the
/// model's `contextWindow`. Between results the window is read off the
/// model id: Claude's 1M models carry `[1m]` (or `-1m`), the rest are 200k.
/// The reader sends this after the line's own event, so a turn's ring
/// moves with every message and lands exactly at the result.
pub(super) fn parse_usage(line: &str) -> Option<SessionEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    let count = |usage: &Value, key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    match value.get("type")?.as_str()? {
        "assistant" => {
            let message = value.get("message")?;
            let usage = message.get("usage")?;
            let input = count(usage, "input_tokens");
            let cached = count(usage, "cache_read_input_tokens");
            let created = count(usage, "cache_creation_input_tokens");
            let output = count(usage, "output_tokens");
            let model = message.get("model").and_then(Value::as_str).unwrap_or("");
            Some(SessionEvent::TokenUsage {
                total_tokens: input + cached + created + output,
                input_tokens: input,
                cached_input_tokens: cached,
                output_tokens: output,
                reasoning_output_tokens: 0,
                context_window: Some(window_of_model(model)),
            })
        }
        "result" => {
            let usage = value.get("usage")?;
            // The turn's last message is the context in use now; the
            // top-level counts are sums over every message of the turn.
            let last = usage
                .get("iterations")
                .and_then(Value::as_array)
                .and_then(|iterations| iterations.last())
                .unwrap_or(usage);
            let input = count(last, "input_tokens");
            let cached = count(last, "cache_read_input_tokens");
            let created = count(last, "cache_creation_input_tokens");
            let output = count(last, "output_tokens");
            let window = value
                .get("modelUsage")
                .and_then(Value::as_object)
                .and_then(|models| {
                    models
                        .values()
                        .find_map(|model| model.get("contextWindow").and_then(Value::as_u64))
                });
            Some(SessionEvent::TokenUsage {
                total_tokens: input + cached + created + output,
                input_tokens: input,
                cached_input_tokens: cached,
                output_tokens: count(usage, "output_tokens"),
                reasoning_output_tokens: usage
                    .get("output_tokens_details")
                    .map(|details| count(details, "thinking_tokens"))
                    .unwrap_or(0),
                context_window: window,
            })
        }
        _ => None,
    }
}

/// Subscription windows arrive as their own line, independently of token
/// usage. Keep their provider reset instants, but normalize utilization so
/// both providers feed the same compact meter.
pub(super) fn parse_rate_limits(line: &str) -> Option<SessionEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "rate_limit_event" {
        return None;
    }
    let windows = value.get("rate_limit_info")?.get("unifiedWindows")?;
    let window = |key: &str| {
        let value = windows.get(key)?;
        Some(RateLimitWindow {
            used_fraction: value.get("utilization")?.as_f64()? as f32,
            resets_at: value.get("resetsAt").and_then(Value::as_u64),
        })
    };
    Some(SessionEvent::RateLimits {
        five_hour: window("five_hour"),
        weekly: window("seven_day"),
    })
}

/// The context window a Claude model id implies, until a result says.
fn window_of_model(model: &str) -> u64 {
    let lower = model.to_ascii_lowercase();
    if lower.contains("[1m]") || lower.ends_with("-1m") {
        1_000_000
    } else {
        200_000
    }
}

/// The CLI emits one `assistant` line per completed content block, so a line
/// carrying a `tool_use` carries exactly one — probed against 2.1.243 with two
/// parallel Bash calls, which arrived as two separate lines. That is what lets
/// one line mean at most one event.
fn parse_assistant(value: &Value) -> Option<SessionEvent> {
    let block = content_block(value, "tool_use")?;
    Some(SessionEvent::ToolStarted {
        id: block.get("id")?.as_str()?.to_string(),
        name: block.get("name")?.as_str()?.to_string(),
        input: block.get("input").cloned().unwrap_or(Value::Null),
    })
}

/// Tool results come back dressed as a user message — the CLI replays what it
/// fed the model.
fn parse_user(value: &Value) -> Option<SessionEvent> {
    let block = content_block(value, "tool_result")?;
    Some(SessionEvent::ToolCompleted {
        id: block.get("tool_use_id")?.as_str()?.to_string(),
        output: match block.get("content") {
            Some(Value::String(text)) => text.clone(),
            Some(other) => other.to_string(),
            None => String::new(),
        },
        // Absent on a successful result; only failures say so.
        is_error: block
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        result: parse_tool_result(value.get("tool_use_result")),
    })
}

/// The CLI hangs its structured result off the same line as the prose one.
/// Each arm below matches a shape in the committed captures; a payload that
/// fits none of them is Opaque rather than half-read.
fn parse_tool_result(value: Option<&Value>) -> ToolResult {
    let Some(value) = value else {
        return ToolResult::Opaque;
    };
    if let Some(patch) = value.get("structuredPatch").and_then(Value::as_array) {
        let Some(path) = value.get("filePath").and_then(Value::as_str) else {
            return ToolResult::Opaque;
        };
        return ToolResult::FileEdit {
            path: path.to_string(),
            hunks: patch.iter().filter_map(parse_hunk).collect(),
        };
    }
    if let Some(stdout) = value.get("stdout").and_then(Value::as_str) {
        return ToolResult::Command {
            stdout: stdout.to_string(),
            stderr: value
                .get("stderr")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        };
    }
    ToolResult::Opaque
}

fn parse_hunk(value: &Value) -> Option<Hunk> {
    Some(Hunk {
        old_start: value.get("oldStart")?.as_u64()? as u32,
        old_lines: value.get("oldLines")?.as_u64()? as u32,
        new_start: value.get("newStart")?.as_u64()? as u32,
        new_lines: value.get("newLines")?.as_u64()? as u32,
        lines: value
            .get("lines")?
            .as_array()?
            .iter()
            .filter_map(|line| Some(line.as_str()?.to_string()))
            .collect(),
    })
}

/// The control protocol runs both ways down the same pipes. The CLI's only
/// request against 2.1.243 is `can_use_tool`; anything else it grows later is
/// ignored, which leaves the turn hanging but never corrupts the stream.
///
/// A request Ferrite cannot fully read is still a request the CLI is blocked
/// on, so everything below `request_id` degrades to empty rather than dropping
/// the event: an operator who can see a Decision can always deny it, and the
/// turn moves. `request_id` itself stays required — without it there is no
/// answer to send, and a card promising otherwise would be a lie.
fn parse_control_request(value: &Value) -> Option<SessionEvent> {
    let request = value.get("request")?;
    if request.get("subtype")?.as_str()? != "can_use_tool" {
        return None;
    }
    Some(SessionEvent::DecisionRequested {
        decision: Decision {
            id: value.get("request_id")?.as_str()?.to_string(),
            tool_use_id: text(request, "tool_use_id"),
            tool_name: text(request, "tool_name"),
            description: text(request, "description"),
            input: request.get("input").cloned().unwrap_or(Value::Null),
            suggestions: request
                .get("permission_suggestions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        },
    })
}

/// A string field, or empty when the provider left it out.
fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn content_block<'a>(value: &'a Value, kind: &str) -> Option<&'a Value> {
    value
        .get("message")?
        .get("content")?
        .as_array()?
        .iter()
        .find(|block| block.get("type").and_then(Value::as_str) == Some(kind))
}

/// The CLI re-announces init at the head of every turn, carrying the same
/// `session_id`; Init therefore repeats rather than arriving once.
fn parse_system(value: &Value) -> Option<SessionEvent> {
    if value.get("subtype")?.as_str()? != "init" {
        return None;
    }
    Some(SessionEvent::Init {
        session_id: value.get("session_id")?.as_str()?.to_string(),
        model: value.get("model")?.as_str()?.to_string(),
    })
}

fn parse_stream_event(value: &Value) -> Option<SessionEvent> {
    let event = value.get("event")?;
    if event.get("type")?.as_str()? != "content_block_delta" {
        return None;
    }
    let delta = event.get("delta")?;
    match delta.get("type")?.as_str()? {
        "text_delta" => Some(SessionEvent::TextDelta {
            text: delta.get("text")?.as_str()?.to_string(),
        }),
        "thinking_delta" => Some(SessionEvent::ThinkingDelta {
            text: delta.get("thinking")?.as_str()?.to_string(),
        }),
        _ => None,
    }
}

/// A result line always ends a turn, even when its shape is unfamiliar: a
/// missing cost is `None`, a missing verdict is a completed turn.
fn parse_result(value: &Value) -> SessionEvent {
    let reason = value
        .get("terminal_reason")
        .and_then(Value::as_str)
        .unwrap_or("");
    let subtype = value.get("subtype").and_then(Value::as_str).unwrap_or("");
    let text = value.get("result").and_then(Value::as_str).unwrap_or("");
    let is_error = value
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let outcome = if is_interrupt(reason) {
        TurnOutcome::Interrupted
    } else if is_error {
        // `subtype` is worthless here: the committed error capture reads
        // `"subtype": "success"` on a turn that never reached the API. Only
        // `terminal_reason` classifies a failure, so it is preferred and
        // `subtype` is the fallback for a line that omits it.
        let classification = if reason.is_empty() { subtype } else { reason };
        TurnOutcome::Error(describe_error(classification, text))
    } else {
        TurnOutcome::Completed
    };

    SessionEvent::TurnEnded {
        outcome,
        cost_usd: value.get("total_cost_usd").and_then(Value::as_f64),
    }
}

/// An interrupt is only legible in `terminal_reason`. Probed against `claude`
/// 2.1.243: interrupting a streaming turn ends it with `is_error: true`,
/// `subtype: "error_during_execution"` and `terminal_reason:
/// "aborted_streaming"` — indistinguishable from a real failure without this
/// field. Its siblings in the CLI's own reason list are `aborted_tools` and
/// friends, so the whole `aborted*` family reads as an interrupt.
fn is_interrupt(terminal_reason: &str) -> bool {
    terminal_reason.starts_with("aborted")
}

fn describe_error(subtype: &str, text: &str) -> String {
    match (subtype, text) {
        ("", "") => "claude CLI reported an error with no detail".to_string(),
        ("", text) => text.to_string(),
        (subtype, "") => subtype.to_string(),
        (subtype, text) => format!("{subtype}: {text}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/claude-hello-2.1.243.jsonl");

    /// Every committed capture of `claude` 2.1.243. These are the protocol
    /// contract: a vendor release that changes the wire changes these numbers,
    /// which is the alarm working.
    const FIXTURES: &[(&str, &str)] = &[
        ("hello", FIXTURE),
        (
            "tool",
            include_str!("../../../tests/fixtures/claude-tool-2.1.243.jsonl"),
        ),
        (
            "permission-allow",
            include_str!("../../../tests/fixtures/claude-permission-allow-2.1.243.jsonl"),
        ),
        (
            "permission-deny",
            include_str!("../../../tests/fixtures/claude-permission-deny-2.1.243.jsonl"),
        ),
        (
            "error",
            include_str!("../../../tests/fixtures/claude-error-2.1.243.jsonl"),
        ),
        (
            "edit",
            include_str!("../../../tests/fixtures/claude-edit-2.1.243.jsonl"),
        ),
        (
            "permission-always",
            include_str!("../../../tests/fixtures/claude-permission-always-2.1.243.jsonl"),
        ),
        (
            "todo",
            include_str!("../../../tests/fixtures/claude-todo-2.1.243.jsonl"),
        ),
    ];

    /// Every event the committed capture of `claude` 2.1.243 yields, in order.
    fn fixture_events() -> Vec<SessionEvent> {
        FIXTURE.lines().filter_map(parse_line).collect()
    }

    /// Every assistant message reports the context it was given, with the
    /// window read off its model id; the result reports the turn's last
    /// message with the window the CLI states — which is what the ring
    /// draws.
    #[test]
    fn every_message_and_the_result_report_the_context_in_use() {
        let usages: Vec<SessionEvent> = FIXTURE.lines().filter_map(parse_usage).collect();
        assert!(usages.len() >= 2, "{usages:?}");
        let SessionEvent::TokenUsage {
            total_tokens,
            context_window,
            ..
        } = &usages[0]
        else {
            panic!("{:?}", usages[0]);
        };
        assert_eq!(
            *total_tokens,
            10 + 18865 + 4,
            "input + cache writes + output"
        );
        assert_eq!(*context_window, Some(200_000), "haiku's window, off the id");
        let SessionEvent::TokenUsage {
            total_tokens,
            context_window,
            output_tokens,
            reasoning_output_tokens,
            ..
        } = usages.last().unwrap()
        else {
            panic!("{:?}", usages.last());
        };
        assert_eq!(*total_tokens, 10 + 18865 + 48);
        assert_eq!(
            *context_window,
            Some(200_000),
            "the result's own contextWindow"
        );
        assert_eq!(*output_tokens, 48);
        assert_eq!(*reasoning_output_tokens, 39);
        assert_eq!(window_of_model("claude-opus-5[1m]"), 1_000_000);
        assert_eq!(window_of_model("claude-fable-5-1"), 200_000);
        assert_eq!(parse_usage(r#"{"type":"user"}"#), None);
    }

    #[test]
    fn unified_rate_limit_windows_are_normalized() {
        let event = parse_rate_limits(r#"{"type":"rate_limit_event","rate_limit_info":{"unifiedWindows":{"five_hour":{"utilization":0.52,"resetsAt":11},"seven_day":{"utilization":0.08,"resetsAt":22}}}}"#).unwrap();
        assert_eq!(
            event,
            SessionEvent::RateLimits {
                five_hour: Some(RateLimitWindow {
                    used_fraction: 0.52,
                    resets_at: Some(11),
                }),
                weekly: Some(RateLimitWindow {
                    used_fraction: 0.08,
                    resets_at: Some(22),
                }),
            }
        );
    }

    fn events_of(name: &str) -> Vec<SessionEvent> {
        let (_, text) = FIXTURES
            .iter()
            .find(|(fixture, _)| *fixture == name)
            .expect("known fixture");
        text.lines().filter_map(parse_line).collect()
    }

    /// Exhaustive by construction: a new SessionEvent variant fails to compile
    /// here until someone decides which fixture proves it.
    fn variant(event: &SessionEvent) -> Option<&'static str> {
        Some(match event {
            SessionEvent::Init { .. } => "Init",
            SessionEvent::TextDelta { .. } => "TextDelta",
            SessionEvent::ThinkingDelta { .. } => "ThinkingDelta",
            SessionEvent::ToolStarted { .. } => "ToolStarted",
            SessionEvent::ToolCompleted { .. } => "ToolCompleted",
            SessionEvent::DecisionRequested { .. } => "DecisionRequested",
            SessionEvent::TurnEnded { .. } => "TurnEnded",
            // Not a wire line at all: the reader thread synthesises Closed when
            // the process exits, so no capture can contain it. Proved instead
            // by `stdout_eof_closes_the_session_with_the_exit_status`.
            SessionEvent::Closed { .. } => return None,
            // Not turn lines either: the menu, the permission mode and the
            // model list ride the initialize handshake the reader correlates
            // itself. Proved by `the_capability_response_carries_the_command_menu`
            // below and the session test that watches the reader announce them.
            SessionEvent::Commands { .. } => return None,
            SessionEvent::PermissionMode { .. } => return None,
            SessionEvent::Models { .. } => return None,
            // Codex's own concept (#9); the Claude CLI never emits one.
            SessionEvent::ReasoningSummaryDelta { .. } => return None,
            // Rides beside a line's own event (`parse_usage`), proved by
            // `every_message_and_the_result_report_the_context_in_use`.
            SessionEvent::TokenUsage { .. } => return None,
            // Parsed alongside the ordinary event path by the reader.
            SessionEvent::RateLimits { .. } => return None,
        })
    }

    #[test]
    fn fixture_yields_exactly_one_init() {
        let inits: Vec<_> = fixture_events()
            .into_iter()
            .filter(|e| matches!(e, SessionEvent::Init { .. }))
            .collect();
        assert_eq!(inits.len(), 1, "expected one Init, got {inits:?}");
        let SessionEvent::Init { session_id, model } = &inits[0] else {
            unreachable!()
        };
        assert!(!session_id.is_empty());
        assert_eq!(model, "claude-haiku-4-5-20251001");
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

    #[test]
    fn fixture_carries_thinking() {
        let thinking: String = fixture_events()
            .into_iter()
            .filter_map(|e| match e {
                SessionEvent::ThinkingDelta { text } => Some(text),
                _ => None,
            })
            .collect();
        assert!(
            thinking.contains("hello ferrite"),
            "thinking deltas missing or empty: {thinking:?}"
        );
    }

    #[test]
    fn fixture_ends_with_a_completed_turn() {
        let events = fixture_events();
        assert_eq!(
            events.last(),
            Some(&SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: Some(0.03798),
            })
        );
    }

    #[test]
    fn fixture_yields_nothing_else() {
        let events = fixture_events();
        let modelled = events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    SessionEvent::Init { .. }
                        | SessionEvent::TextDelta { .. }
                        | SessionEvent::ThinkingDelta { .. }
                        | SessionEvent::TurnEnded { .. }
                )
            })
            .count();
        assert_eq!(modelled, events.len());
        // 32 lines in: the init, 7 thinking deltas, 1 text delta and the
        // result line are events; the other 22 lines are ignored.
        assert_eq!(FIXTURE.lines().count(), 32);
        assert_eq!(events.len(), 10);
    }

    /// The whole point of the fixture harness: nothing in the typed surface is
    /// aspirational — every variant a wire line can produce has a committed
    /// capture behind it.
    #[test]
    fn every_session_event_variant_is_produced_by_a_fixture() {
        let mut produced: Vec<&str> = FIXTURES
            .iter()
            .flat_map(|(_, text)| text.lines().filter_map(parse_line))
            .filter_map(|event| variant(&event))
            .collect();
        produced.sort_unstable();
        produced.dedup();
        assert_eq!(
            produced,
            [
                "DecisionRequested",
                "Init",
                "TextDelta",
                "ThinkingDelta",
                "ToolCompleted",
                "ToolStarted",
                "TurnEnded",
            ]
        );
    }

    /// Most of the stream is not Ferrite's business — hook chatter, token
    /// counters, message envelopes, rate limits. Recording how much of each
    /// capture is ignored is what proves an unknown line costs nothing.
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
                // 1 init, 7 thinking deltas, 1 text delta, 1 result.
                ("hello", 32, 10),
                // ... and 1 tool start with its completion.
                ("tool", 51, 14),
                // ... and the Decision that gated the tool.
                ("permission-allow", 52, 15),
                ("permission-deny", 60, 19),
                // A turn that never reached the API: an init and a verdict.
                ("error", 4, 2),
                // Read then Edit: two tool calls, and the patch hunks the diff
                // cards are built from.
                ("edit", 70, 16),
                // Two Writes, one Decision: the standing answer was adopted on
                // the first, and the CLI never gated the second.
                ("permission-always", 58, 15),
                // The Thread plans: three TaskCreate calls and the update that
                // ticks the first off, which is what L2's progress counts.
                ("todo", 190, 47),
            ]
        );
    }

    /// A tool call, as `claude` 2.1.243 actually reports one: the settled input
    /// arrives on the `assistant` line that closes the block, and the result
    /// comes back dressed as a user message.
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
        assert_eq!(name, "Bash");
        assert_eq!(input["command"], "echo ferrite-tool-ok");
        assert!(events.contains(&SessionEvent::ToolCompleted {
            id: id.clone(),
            output: "ferrite-tool-ok".into(),
            is_error: false,
            result: ToolResult::Command {
                stdout: "ferrite-tool-ok".into(),
                stderr: String::new(),
            },
        }));
    }

    /// A create has no hunks by definition, so only an edit proves the patch
    /// shape the diff cards are built from.
    #[test]
    fn an_edit_carries_the_patch_hunks_it_applied() {
        let hunks = events_of("edit")
            .into_iter()
            .find_map(|event| match event {
                SessionEvent::ToolCompleted {
                    result: ToolResult::FileEdit { path, hunks },
                    ..
                } if path.ends_with("ferrite-edit.txt") => Some(hunks),
                _ => None,
            })
            .expect("the edit fixture patches a file");

        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].old_start, 1);
        assert_eq!(hunks[0].new_start, 1);
        assert_eq!(hunks[0].lines, [" alpha", "-bravo", "+delta", " charlie"]);
    }

    #[test]
    fn a_command_carries_the_streams_it_wrote() {
        let streams = events_of("tool")
            .into_iter()
            .find_map(|event| match event {
                SessionEvent::ToolCompleted {
                    result: ToolResult::Command { stdout, stderr },
                    ..
                } => Some((stdout, stderr)),
                _ => None,
            })
            .expect("the tool fixture runs a command");

        assert_eq!(streams, ("ferrite-tool-ok".to_string(), String::new()));
    }

    /// Denial is not failure: the CLI feeds the operator's reason back to the
    /// model as a failed tool result and the turn runs to a normal end.
    #[test]
    fn a_denied_tool_fails_without_failing_the_turn() {
        let events = events_of("permission-deny");
        assert!(
            events.iter().any(|e| matches!(
                e,
                SessionEvent::ToolCompleted { is_error: true, output, .. }
                    if output == "Ferrite operator denied this tool"
            )),
            "no denied tool result: {events:?}"
        );
        assert!(matches!(
            events.last(),
            Some(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                ..
            })
        ));
    }

    /// An allowed tool result carries no `is_error` field at all — absence is
    /// success, and reading it as anything else would paint every tool card red.
    #[test]
    fn a_result_without_an_error_flag_is_a_success() {
        let events = events_of("permission-allow");
        assert!(
            events.iter().any(|e| matches!(
                e,
                SessionEvent::ToolCompleted {
                    is_error: false,
                    output,
                    ..
                } if output.starts_with("File created successfully")
            )),
            "no successful tool result: {events:?}"
        );
    }

    /// The initialize handshake, from the committed capture of the real
    /// exchange.
    #[test]
    fn the_capability_response_is_read_for_what_ferrite_acts_on() {
        let capabilities = parse_capabilities(
            include_str!("../../../tests/fixtures/claude-initialize-2.1.243.jsonl").trim_end(),
            "req_1",
        )
        .expect("the capture answers req_1");
        assert_eq!(capabilities.permission_mode, "bypassPermissions");
        assert!(capabilities.models.iter().any(|model| model == "haiku"));
        // The row is named by its resolved id, versioned; the description
        // loses the name it repeated; and the value resolves to a full id
        // the Init can be matched back to.
        let haiku = capabilities
            .models
            .iter()
            .find(|model| model.value == "haiku")
            .unwrap();
        assert_eq!(haiku.display, "Haiku 4.5");
        assert_eq!(haiku.detail, "Fastest for quick answers");
        assert!(haiku.is("claude-haiku-4-5-20251001"));
        assert!(haiku.efforts.is_empty(), "Haiku announces no effort levels");
    }

    /// The 2.1.259 handshake: every row's effort ladder rides
    /// `supportedEffortLevels`, the names are the resolved ids groomed, and
    /// the `default` alias says what it stands for.
    #[test]
    fn the_capability_response_carries_versioned_names_and_effort_ladders() {
        let capabilities = parse_capabilities(
            include_str!("../../../tests/fixtures/claude-initialize-2.1.259.jsonl").trim_end(),
            "req_1",
        )
        .expect("the capture answers req_1");
        let row = |value: &str| {
            capabilities
                .models
                .iter()
                .find(|model| model.value == value)
                .unwrap_or_else(|| panic!("{value} is announced"))
        };
        let ladder = ["low", "medium", "high", "xhigh", "max"];

        let default = row("default");
        assert_eq!(default.display, "Default · Opus 5 (1M)");
        assert_eq!(default.detail, "Best for everyday, complex tasks");
        assert_eq!(default.efforts, ladder);
        assert_eq!(default.default_effort, None);

        let fable = row("claude-fable-5-1[1m]");
        assert_eq!(fable.display, "Fable 5.1");
        assert_eq!(
            fable.detail,
            "Most capable for your hardest and longest-running tasks"
        );
        assert_eq!(fable.efforts, ladder);
        assert!(fable.is("claude-fable-5-1"));

        assert_eq!(row("opus[1m]").display, "Opus 5 (1M)");
        assert_eq!(row("sonnet").display, "Sonnet 5");
        assert_eq!(row("sonnet").efforts, ladder);
        assert_eq!(row("haiku").display, "Haiku 4.5");
        assert!(row("haiku").efforts.is_empty());
    }

    /// #23: the same handshake line carries the CLI's whole effective slash
    /// menu — the `/` popover's one source, never a static list. The counts
    /// and entries are the committed capture's own.
    #[test]
    fn the_capability_response_carries_the_command_menu() {
        let capabilities = parse_capabilities(
            include_str!("../../../tests/fixtures/claude-initialize-2.1.243.jsonl").trim_end(),
            "req_1",
        )
        .expect("the capture answers req_1");

        assert_eq!(capabilities.commands.len(), 47, "the capture's own count");
        let compact = capabilities
            .commands
            .iter()
            .find(|command| command.name == "compact")
            .expect("the built-ins are in the one list");
        assert_eq!(
            compact.description,
            "Free up context by summarizing the conversation so far"
        );
        // Claude commands are invoked as plain `/name args` text — there is
        // no path for a typed item to carry.
        assert!(capabilities.commands.iter().all(|c| c.path.is_none()));
    }

    /// The response to somebody else's request is not this Session's answer.
    #[test]
    fn a_capability_response_to_another_request_is_not_taken() {
        for line in [
            r#"{"type":"control_response","response":{"request_id":"req_9","response":{}}}"#,
            r#"{"type":"control_response","response":{"subtype":"success"}}"#,
            r#"{"type":"control_request","request_id":"req_1","request":{"subtype":"initialize"}}"#,
            "not json",
        ] {
            assert_eq!(
                parse_capabilities(line, "req_1"),
                None,
                "should not answer the handshake: {line}"
            );
        }
    }

    /// A handshake answer that says nothing leaves every capability unknown
    /// rather than inventing one.
    #[test]
    fn an_empty_capability_response_claims_nothing() {
        assert_eq!(
            parse_capabilities(
                r#"{"type":"control_response","response":{"request_id":"req_1","response":{}}}"#,
                "req_1"
            ),
            Some(ClaudeCapabilities::default())
        );
    }

    /// The hazard #5 inherited: a request Ferrite cannot describe is still a
    /// request the CLI is blocked on. Surfacing it with an answerable id lets
    /// the operator deny it; dropping it hangs the turn with nothing on screen.
    #[test]
    fn a_permission_request_missing_its_tool_details_is_still_answerable() {
        let line = r#"{"type":"control_request","request_id":"req_1","request":{"subtype":"can_use_tool"}}"#;

        let event = parse_line(line).expect("a malformed request must still surface");

        let SessionEvent::DecisionRequested { decision } = event else {
            panic!("expected a Decision, got {event:?}")
        };
        assert_eq!(decision.id, "req_1");
        assert!(
            decision.tool_name.is_empty(),
            "nothing was said, so nothing is named"
        );
        assert!(decision.tool_use_id.is_empty());
    }

    #[test]
    fn junk_lines_are_ignored_never_fatal() {
        for line in [
            "",
            "not json at all",
            "{}",
            r#"{"type":42}"#,
            r#"{"type":"system"}"#,
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"stream_event"}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":7}}}"#,
            r#"{"type":"control_response","response":{"subtype":"success"}}"#,
            r#"{"type":"brand_new_vendor_event","payload":{}}"#,
            // Control traffic Ferrite does not answer: hooks, MCP relays, and
            // whatever the vendor adds next. Ignoring one strands that request,
            // but the stream keeps flowing.
            r#"{"type":"control_request","request_id":"1","request":{"subtype":"hook_callback"}}"#,
            // A `can_use_tool` with nothing else in it used to sit here. It is
            // not junk — it is a turn the CLI has stopped on — and it now
            // surfaces answerable: see the test below.
            r#"{"type":"control_request","request":{"subtype":"can_use_tool"}}"#,
            // An assistant turn is only interesting when it carries a tool
            // call; text and thinking arrive as deltas, and the snapshot lines
            // that repeat them must not double them up.
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash"}]}}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"hi"}]}}"#,
            r#"{"type":"user","message":{"content":"a plain string"}}"#,
        ] {
            assert_eq!(parse_line(line), None, "line should be ignored: {line}");
        }
    }

    #[test]
    fn error_results_carry_their_detail() {
        let line = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"boom","total_cost_usd":0.01}"#;
        assert_eq!(
            parse_line(line),
            Some(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Error("error_during_execution: boom".into()),
                cost_usd: Some(0.01),
            })
        );
    }

    /// Captured from `claude` 2.1.243 by interrupting a streaming turn: the
    /// CLI calls it an error and only `terminal_reason` tells the truth.
    #[test]
    fn an_interrupted_turn_is_not_an_error() {
        let line = r#"{"is_error":true,"num_turns":2,"total_cost_usd":0,"terminal_reason":"aborted_streaming","subtype":"error_during_execution","errors":["[ede_diagnostic] result_type=user last_content_type=n/a stop_reason=null"],"type":"result","duration_ms":1839}"#;
        assert_eq!(
            parse_line(line),
            Some(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Interrupted,
                cost_usd: Some(0.0),
            })
        );
    }

    #[test]
    fn aborting_during_tool_use_is_also_an_interrupt() {
        let line = r#"{"type":"result","is_error":true,"subtype":"error_during_execution","terminal_reason":"aborted_tools"}"#;
        assert_eq!(
            parse_line(line),
            Some(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Interrupted,
                cost_usd: None,
            })
        );
    }

    #[test]
    fn a_result_without_cost_still_ends_the_turn() {
        assert_eq!(
            parse_line(r#"{"type":"result"}"#),
            Some(SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            })
        );
    }
}
