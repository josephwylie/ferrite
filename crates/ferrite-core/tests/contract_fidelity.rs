#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use serde_json::json;
use support::*;

#[test]
fn conversation_reset_never_reuses_a_live_decision_handle() {
    let request = |id: &str| json!({"type":"control_request","request_id":id,"request":{"subtype":"can_use_tool","tool_name":"Bash","tool_use_id":id,"input":{"command":"echo test"}}});
    let r = Replay::new("claude", vec![request("before"), json!({"type":"conversation_reset","session_id":"root","uuid":"reset","new_conversation_id":"fresh"}), request("after")]);
    let events = r.drain();
    let reset = events.iter().position(|e| matches!(e, ferrite_core::SessionEvent::ConversationReset { .. })).unwrap();
    let old = fold(events[..reset].to_vec()).view().pending_decisions()[0].handle.clone();
    let a = fold(events);
    assert_ne!(old, a.view().pending_decisions()[0].handle, "stale pre-reset UI actions must never target a new request");
}

#[test]
fn claude_local_command_output_preserves_all_lines_once() {
    let frame = json!({"type":"system","subtype":"local_command_output","session_id":"root","uuid":"command","content":"First line\n\nSecond paragraph"});
    let r = Replay::new("claude", vec![frame.clone(), frame]);
    assert_eq!(prose(&fold(r.drain())), ["First line", "Second paragraph"]);
}

#[test]
fn claude_child_retraction_removes_only_the_owning_childs_content() {
    let r = Replay::new("claude", vec![
        json!({"type":"assistant","uuid":"main","session_id":"root","parent_tool_use_id":null,"message":{"id":"main","content":[{"type":"text","text":"Keep Main"}]}}),
        json!({"type":"assistant","uuid":"old-child","session_id":"root","parent_tool_use_id":"spawn","message":{"id":"old","content":[{"type":"text","text":"Retracted child"}]}}),
        json!({"type":"assistant","uuid":"replacement","supersedes":["old-child"],"session_id":"root","parent_tool_use_id":"spawn","message":{"id":"new","content":[{"type":"text","text":"Replacement child"}]}}),
    ]);
    let a = fold(r.drain());
    assert_eq!(prose(&a), ["Keep Main"]);
    let key = ferrite_core::activity::AgentKey::new(ferrite_core::store::Provider::Claude, "root", "spawn");
    let child = a.view().subject(&ferrite_core::activity::Subject::Subagent(key)).unwrap();
    let text = format!("{:?}", child.transcript().blocks());
    assert!(!text.contains("Retracted child"));
    assert!(text.contains("Replacement child"));
}

#[test]
fn conversation_reset_clears_child_order_and_persists_the_new_resume_target() {
    use ferrite_core::{activity::Activity, store::{Store, Provider}, workspace::WorkspaceBinding};
    let r = Replay::new("claude", vec![
        json!({"type":"system","subtype":"init","session_id":"root","model":"fixture"}),
        json!({"type":"assistant","uuid":"child","session_id":"root","parent_tool_use_id":"spawn","message":{"id":"child","content":[{"type":"text","text":"Old child"}]}}),
        json!({"type":"conversation_reset","session_id":"root","uuid":"reset","new_conversation_id":"fresh"}),
    ]);
    let events = r.drain();
    let a = fold(events.clone());
    assert!(a.view().children().is_empty(), "reset must clear the child ordering as well as its records");
    let dir = std::env::temp_dir().join(format!("ferrite-reset-contract-{}", std::process::id()));
    let store = Store::open(&dir).unwrap();
    let (id, mut writer) = store.create(Provider::Claude, None, WorkspaceBinding::Main { checkout: std::env::temp_dir() }).unwrap();
    for event in &events { writer.record_event(event, None).unwrap(); }
    writer.flush().unwrap();
    let snapshot = store.load(id).unwrap();
    assert_eq!(snapshot.resume_target(), Some("fresh"));
    let mut restored = Activity::default();
    for input in snapshot.activity_inputs() { restored.apply(input); }
    assert_eq!(restored.view().main().transcript().session_id(), Some("fresh"));
    assert!(restored.view().children().is_empty());
    assert!(restored.view().pending_decisions().is_empty());
    drop(writer);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn claude_main_user_echo_is_not_a_second_operator_prompt() {
    for content in [json!("Original prompt"), json!([{"type":"text","text":"Original prompt"}])] {
        let r = Replay::new("claude", vec![json!({"type":"user","uuid":"echo","session_id":"root","parent_tool_use_id":null,"message":{"role":"user","content":content}})]);
        let a = fold(r.drain());
        assert!(!a.view().main().transcript().blocks().iter().any(|b|
            matches!(&b.body, ferrite_core::transcript::Body::Prompt(_))),
            "operator prompt is already recorded on submission; the provider echo must not duplicate it");
    }
}

#[test]
fn claude_complete_main_message_without_deltas_is_visible_once() {
    let frame = json!({"type":"assistant","uuid":"frame-1","session_id":"root","parent_tool_use_id":null,"is_meta":true,"local_command_source":"/context","message":{"id":"msg-1","role":"assistant","content":[{"type":"text","text":"Native context report"}]}});
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"system","subtype":"init","session_id":"root","model":"fixture"}),
            frame.clone(),
            frame,
        ],
    );
    assert_eq!(prose(&fold(r.drain())), ["Native context report"]);
}
#[test]
fn codex_final_snapshot_corrects_prose_without_erasing_interleaved_items() {
    let r = Replay::new(
        "codex",
        vec![
            json!({"method":"item/agentMessage/delta","params":{"threadId":"root","turnId":"turn","itemId":"a","delta":"outdated"}}),
            json!({"method":"item/agentMessage/delta","params":{"threadId":"root","turnId":"turn","itemId":"b","delta":"other"}}),
            json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"id":"a","type":"agentMessage","text":"corrected"}}}),
            json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"id":"b","type":"agentMessage","text":"other"}}}),
        ],
    );
    assert_eq!(prose(&fold(r.drain())), ["corrected", "other"]);
}
#[test]
fn codex_summary_part_added_preserves_identity_and_exact_text() {
    let r = Replay::new(
        "codex",
        vec![
            json!({"method":"item/reasoning/summaryPartAdded","params":{"threadId":"root","turnId":"turn","itemId":"r","summaryIndex":0}}),
            json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"root","turnId":"turn","itemId":"r","summaryIndex":0,"delta":"**Check**\n\nFirst"}}),
            json!({"method":"item/reasoning/summaryPartAdded","params":{"threadId":"root","turnId":"turn","itemId":"r","summaryIndex":1}}),
            json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"root","turnId":"turn","itemId":"r","summaryIndex":1,"delta":"Second"}}),
            json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"id":"r","type":"reasoning","summary":["**Check**\n\nFirst complete","Second"],"content":[]}}}),
        ],
    );
    assert_eq!(
        reasoning(&fold(r.drain())),
        ["**Check**\n\nFirst complete", "Second"]
    );
}

#[test]
fn claude_stream_and_complete_block_reconcile_by_native_identity() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"system","subtype":"init","session_id":"root","model":"fixture"}),
            json!({"type":"stream_event","session_id":"root","parent_tool_use_id":null,"event":{"type":"message_start","message":{"id":"msg"}}}),
            json!({"type":"stream_event","session_id":"root","parent_tool_use_id":null,"event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}),
            json!({"type":"stream_event","session_id":"root","parent_tool_use_id":null,"event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"partial"}}}),
            json!({"type":"stream_event","session_id":"root","parent_tool_use_id":null,"event":{"type":"content_block_stop","index":0}}),
            json!({"type":"assistant","uuid":"frame","session_id":"root","parent_tool_use_id":null,"message":{"id":"msg","content":[{"type":"text","text":"partial complete"}]}}),
        ],
    );
    assert_eq!(prose(&fold(r.drain())), ["partial complete"]);
}
#[test]
fn claude_superseded_message_is_removed_without_removing_other_messages() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"system","subtype":"init","session_id":"root","model":"fixture"}),
            json!({"type":"assistant","uuid":"old","session_id":"root","parent_tool_use_id":null,"message":{"id":"old-msg","content":[{"type":"text","text":"Retracted answer"}]}}),
            json!({"type":"assistant","uuid":"keep","session_id":"root","parent_tool_use_id":null,"message":{"id":"keep-msg","content":[{"type":"text","text":"Keep this"}]}}),
            json!({"type":"assistant","uuid":"new","supersedes":["old"],"session_id":"root","parent_tool_use_id":null,"message":{"id":"new-msg","content":[{"type":"text","text":"Replacement"}]}}),
        ],
    );
    assert_eq!(prose(&fold(r.drain())), ["Keep this", "Replacement"]);
}
#[test]
fn codex_raw_reasoning_and_summary_do_not_share_a_part_identity() {
    let r = Replay::new(
        "codex",
        vec![
            json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"root","turnId":"turn","itemId":"r","summaryIndex":0,"delta":"Summary"}}),
            json!({"method":"item/reasoning/textDelta","params":{"threadId":"root","turnId":"turn","itemId":"r","contentIndex":0,"delta":"Raw text"}}),
            json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"id":"r","type":"reasoning","summary":["Summary complete"],"content":["Raw text complete"]}}}),
        ],
    );
    assert_eq!(
        reasoning(&fold(r.drain())),
        ["Summary complete", "Raw text complete"]
    );
}

#[test]
fn captured_claude_thinking_then_text_have_distinct_stream_block_identities() {
    let frames = include_str!("fixtures/claude-hello-2.1.243.jsonl")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let r = Replay::new("claude", frames);
    let a = fold(r.drain());
    assert_eq!(prose(&a), ["hello ferrite"]);
    assert_eq!(
        reasoning(&a).len(),
        1,
        "completed thinking must reconcile with its stream"
    );
    assert!(reasoning(&a)[0].starts_with("The user is asking me"));
}
#[test]
fn claude_content_block_start_can_carry_the_entire_visible_text() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"system","subtype":"init","session_id":"root","model":"fixture"}),
            json!({"type":"stream_event","session_id":"root","parent_tool_use_id":null,"event":{"type":"message_start","message":{"id":"msg"}}}),
            json!({"type":"stream_event","session_id":"root","parent_tool_use_id":null,"event":{"type":"content_block_start","index":3,"content_block":{"type":"text","text":"Already present"}}}),
            json!({"type":"assistant","uuid":"frame","session_id":"root","parent_tool_use_id":null,"message":{"id":"msg","content":[{"type":"text","text":"Already present"}]}}),
            json!({"type":"stream_event","session_id":"root","parent_tool_use_id":null,"event":{"type":"content_block_stop","index":3}}),
        ],
    );
    assert_eq!(prose(&fold(r.drain())), ["Already present"]);
}

#[test]
fn claude_refusal_fallback_retracts_top_level_native_message_uuids() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"system","subtype":"init","session_id":"root","model":"fixture"}),
            json!({"type":"assistant","uuid":"old","session_id":"root","parent_tool_use_id":null,"message":{"id":"old-msg","content":[{"type":"text","text":"Retracted answer"}]}}),
            json!({"type":"system","subtype":"model_refusal_fallback","session_id":"root","uuid":"fallback","retracted_message_uuids":["old"]}),
        ],
    );
    assert!(prose(&fold(r.drain())).is_empty());
}
#[test]
fn claude_retraction_before_a_multiblock_delivery_suppresses_every_block() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"system","subtype":"init","session_id":"root","model":"fixture"}),
            json!({"type":"assistant","uuid":"replacement","supersedes":["late"],"session_id":"root","parent_tool_use_id":null,"message":{"id":"replacement-msg","content":[{"type":"text","text":"Keep replacement"}]}}),
            json!({"type":"assistant","uuid":"late","session_id":"root","parent_tool_use_id":null,"message":{"id":"late-msg","content":[{"type":"text","text":"Stale first"},{"type":"text","text":"Stale second"}]}}),
        ],
    );
    assert_eq!(prose(&fold(r.drain())), ["Keep replacement"]);
}
#[test]
fn codex_reused_summary_item_id_in_another_turn_does_not_replace_history() {
    let mut frames = vec![];
    for (turn, text) in [("one", "First reasoning"), ("two", "Second reasoning")] {
        frames.push(json!({"method":"item/reasoning/summaryTextDelta","params":{"threadId":"root","turnId":turn,"itemId":"r","summaryIndex":0,"delta":text}}));
        frames.push(json!({"method":"item/completed","params":{"threadId":"root","turnId":turn,"item":{"id":"r","type":"reasoning","summary":[text],"content":[]}}}));
        frames.push(json!({"method":"turn/completed","params":{"threadId":"root","turn":{"id":turn,"status":"completed"}}}));
    }
    let r = Replay::new("codex", frames);
    assert_eq!(
        reasoning(&fold(r.drain())),
        ["First reasoning", "Second reasoning"]
    );
}
#[test]
fn claude_conversation_reset_immediately_clears_visible_old_content() {
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"system","subtype":"init","session_id":"root","model":"fixture"}),
            json!({"type":"assistant","uuid":"old","session_id":"root","parent_tool_use_id":null,"message":{"id":"old-msg","content":[{"type":"text","text":"Old conversation"}]}}),
            json!({"type":"conversation_reset","session_id":"root","uuid":"reset","new_conversation_id":"fresh"}),
        ],
    );
    let a = fold(r.drain());
    assert!(prose(&a).is_empty());
    assert_eq!(a.view().main().transcript().session_id(), Some("fresh"));
}
