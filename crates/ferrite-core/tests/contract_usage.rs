#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::transcript::{Input, Transcript};
use ferrite_core::{SessionEvent, UsageScope};
use serde_json::json;
use support::*;

#[test]
fn context_only_refresh_cannot_reset_output_accounting() {
    let mut t = Transcript::default();
    let usage = |output| SessionEvent::TokenUsage {
        total_tokens: 100,
        input_tokens: 80,
        cached_input_tokens: 0,
        output_tokens: output,
        reasoning_output_tokens: 0,
        context_window: Some(1000),
    };
    t.apply(Input::Prompt("Hello".into()));
    t.apply(Input::Event(usage(10)));
    t.apply(Input::Event(SessionEvent::ContextUsage {
        total_tokens: 200,
        context_window: Some(1000),
    }));
    t.apply(Input::Event(usage(15)));
    assert_eq!(
        t.turn_output_tokens(),
        15,
        "context occupancy has no output usage and must not imply a reset to zero"
    );
    assert_eq!(t.usage().unwrap().total_tokens, 100);
}

#[test]
fn native_accounting_details_keep_scope_and_all_counters() {
    for (provider, frame, scope) in [
        (
            "claude",
            json!({"type":"result","subtype":"success","session_id":"root","uuid":"usage","total_cost_usd":0.04,"usage":{"input_tokens":100,"cache_read_input_tokens":20,"cache_creation_input_tokens":0,"output_tokens":30}}),
            UsageScope::Turn,
        ),
        (
            "codex",
            json!({"method":"thread/tokenUsage/updated","params":{"threadId":"root","turnId":"turn","tokenUsage":{"last":{"totalTokens":80},"total":{"totalTokens":150,"inputTokens":100,"cachedInputTokens":20,"outputTokens":30,"reasoningOutputTokens":10},"modelContextWindow":180000}}}),
            UsageScope::Session,
        ),
    ] {
        let r = Replay::new(provider, vec![frame]);
        let a = fold(r.drain());
        let details = a
            .view()
            .main()
            .transcript()
            .usage_details()
            .expect("shared accounting details");
        assert_eq!(details.scope, scope);
        assert_eq!(details.input_tokens, 100);
        assert_eq!(details.cached_input_tokens, 20);
        assert_eq!(details.output_tokens, 30);
        if provider == "codex" {
            assert_eq!(details.reasoning_output_tokens, 10);
        } else {
            assert_eq!(a.view().main().transcript().last_cost(), Some(0.04));
        }
    }
}

#[test]
fn historical_events_cannot_replace_live_usage_or_turn_changes() {
    use ferrite_core::activity::{ActivityEvent, ActivityInput, ExecutionEvent};
    let details = |n| ferrite_core::UsageDetails {
        scope: UsageScope::Session,
        input_tokens: n,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
    };
    let mut activity = fold(vec![
        SessionEvent::UsageDetails {
            details: details(200),
        },
        SessionEvent::TurnDiff {
            turn_id: "live".into(),
            diff: "live diff".into(),
        },
    ]);
    for event in [
        ExecutionEvent::UsageDetails {
            details: details(10),
        },
        ExecutionEvent::TurnDiff {
            turn_id: "old".into(),
            diff: "old diff".into(),
        },
    ] {
        activity.apply(ActivityInput::ReplayEvent(ActivityEvent::MainContent {
            id: None,
            event,
        }));
    }
    let view = activity.view();
    let transcript = view.main().transcript();
    assert_eq!(transcript.usage_details().unwrap().input_tokens, 200);
    assert_eq!(transcript.turn_diff().unwrap().diff, "live diff");
}

#[test]
fn claude_equal_sized_messages_are_counted_separately() {
    let frame = |uuid: &str, id: &str, output: u64| json!({"type":"assistant","uuid":uuid,"session_id":"root","parent_tool_use_id":null,"message":{"id":id,"content":[],"usage":{"input_tokens":1,"output_tokens":output}}});
    let r = Replay::new(
        "claude",
        vec![
            frame("a", "m1", 10),
            frame("b", "m1", 15),
            frame("c", "m2", 15),
        ],
    );
    let a = fold(r.drain());
    assert_eq!(
        a.view().main().transcript().turn_output_tokens(),
        30,
        "message IDs, not counter magnitude, determine whether output is new"
    );
    assert_eq!(
        a.view()
            .main()
            .transcript()
            .usage_details()
            .unwrap()
            .output_tokens,
        15,
        "native accounting details remain the exact last message report"
    );
}

#[test]
fn codex_second_turn_output_excludes_previous_turns() {
    use ferrite_core::activity::ActivityInput;
    let frame = |turn: &str, output: u64| json!({"method":"thread/tokenUsage/updated","params":{"threadId":"root","turnId":turn,"tokenUsage":{"last":{"totalTokens":100},"total":{"totalTokens":100,"outputTokens":output},"modelContextWindow":1000}}});
    let r = Replay::new(
        "codex",
        vec![
            frame("one", 10),
            json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":"one","status":"completed"}}}),
            frame("two", 15),
        ],
    );
    let events = r.drain();
    let split = events
        .iter()
        .position(|e| matches!(e, SessionEvent::TurnEnded { .. }))
        .unwrap();
    let mut a = fold(events[..=split].to_vec());
    a.apply(ActivityInput::Main {
        input: Input::Prompt("again".into()),
        at: std::time::Instant::now(),
    });
    for event in events[split + 1..].iter().cloned() {
        match event {
            SessionEvent::Activity(event) => {
                a.apply(ActivityInput::Observe {
                    generation: 1,
                    event,
                    at: std::time::Instant::now(),
                });
            }
            event => {
                a.apply(ActivityInput::Main {
                    input: Input::Event(event),
                    at: std::time::Instant::now(),
                });
            }
        }
    }
    assert_eq!(
        a.view().main().transcript().turn_output_tokens(),
        5,
        "thread-wide native totals need a per-turn baseline"
    );
    assert_eq!(
        a.view()
            .main()
            .transcript()
            .usage_details()
            .unwrap()
            .output_tokens,
        15
    );
}

#[test]
fn child_accounting_never_changes_mains_context_cache() {
    let message = |uuid: &str, parent: serde_json::Value, input: u64, window: u64| json!({"type":"assistant","uuid":uuid,"session_id":"root","parent_tool_use_id":parent,"context_usage":{"total_tokens":input,"raw_max_tokens":window},"message":{"id":uuid,"content":[],"usage":{"input_tokens":input,"output_tokens":1}}});
    let r = Replay::new(
        "claude",
        vec![
            message("main", json!(null), 100, 1000),
            message("child", json!("spawn"), 500, 5000),
            json!({"type":"result","session_id":"root","subtype":"success","usage":{"input_tokens":100,"output_tokens":2}}),
        ],
    );
    let a = fold(r.drain());
    let usage = a.view().main().transcript().usage().unwrap();
    assert_eq!(usage.total_tokens, 100);
    assert_eq!(usage.context_window, Some(1000));
}
