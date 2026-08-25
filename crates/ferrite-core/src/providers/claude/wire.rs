//! One stdout line of the CLI's stream-json to at most one SessionEvent.
//!
//! The vendor extends this stream without notice, so every line is read as a
//! `Value` and anything unrecognised — new event types, changed field types,
//! outright junk — is silently nothing rather than an error.

use serde_json::Value;

use crate::{SessionEvent, TurnOutcome};

/// `None` means "nothing Ferrite models": hook chatter, status lines,
/// assistant snapshots, rate limits, unparseable junk.
pub(super) fn parse_line(line: &str) -> Option<SessionEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    match value.get("type")?.as_str()? {
        "system" => parse_system(&value),
        "stream_event" => parse_stream_event(&value),
        "result" => Some(parse_result(&value)),
        _ => None,
    }
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
    let subtype = value.get("subtype").and_then(Value::as_str).unwrap_or("");
    let text = value.get("result").and_then(Value::as_str).unwrap_or("");
    let is_error = value
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let outcome = if is_interrupt(value) {
        TurnOutcome::Interrupted
    } else if is_error {
        TurnOutcome::Error(describe_error(subtype, text))
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
fn is_interrupt(value: &Value) -> bool {
    value
        .get("terminal_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| reason.starts_with("aborted"))
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

    /// Every event the committed capture of `claude` 2.1.243 yields, in order.
    fn fixture_events() -> Vec<SessionEvent> {
        FIXTURE.lines().filter_map(parse_line).collect()
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
