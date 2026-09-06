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
