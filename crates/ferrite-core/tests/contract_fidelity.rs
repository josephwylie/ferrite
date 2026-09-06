#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use serde_json::json;
use support::*;

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
