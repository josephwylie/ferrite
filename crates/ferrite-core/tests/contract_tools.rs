#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/provider_contract.rs"]
mod support;
use ferrite_core::{SessionEvent, ToolResult};
use serde_json::json;
use support::*;
#[test]
fn codex_command_completion_preserves_native_output_as_a_command_result() {
    let r = Replay::new(
        "codex",
        vec![
            json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"id":"cmd","type":"commandExecution","command":"cargo test","aggregatedOutput":"three tests passed\n","status":"completed","exitCode":0,"durationMs":1234}}}),
        ],
    );
    let events = r.drain();
    assert!(events.iter().any(|e|matches!(e,SessionEvent::ToolCompleted{id,result:ToolResult::Command{stdout,stderr,..},..} if id=="cmd" && stdout=="three tests passed\n" && stderr.is_empty())),"structured command output lost: {events:?}");
}

#[test]
fn codex_native_duration_and_exit_status_survive_completion() {
    let r = Replay::new(
        "codex",
        vec![
            json!({"method":"item/started","params":{"threadId":"root","turnId":"turn","item":{"id":"cmd","type":"commandExecution","command":"tests","status":"inProgress"}}}),
            json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"id":"cmd","type":"commandExecution","command":"tests","aggregatedOutput":"failed\n","status":"completed","exitCode":7,"durationMs":1234}}}),
        ],
    );
    let events = r.drain();
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::ToolCompleted {
            is_error: true,
            result: ToolResult::Command {
                exit_code: Some(7),
                duration_ms: Some(1234),
                ..
            },
            ..
        }
    )));
    let a = fold(events);
    assert_eq!(
        a.view().main().timings().get("cmd").unwrap().elapsed(),
        std::time::Duration::from_millis(1234)
    );
}
#[test]
fn claude_unrecognized_structured_tool_result_remains_available() {
    let payload = json!({"type":"future_result","items":[{"title":"Document","uri":"mcp://document"}],"provider_flag":true});
    let r = Replay::new(
        "claude",
        vec![
            json!({"type":"assistant","uuid":"call","session_id":"root","parent_tool_use_id":null,"message":{"id":"msg","role":"assistant","content":[{"type":"tool_use","id":"tool","name":"McpSearch","input":{}}]}}),
            json!({"type":"user","uuid":"result","session_id":"root","parent_tool_use_id":null,"tool_use_result":payload,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"tool","content":"Found one document"}]}}),
        ],
    );
    let a = fold(r.drain());
    assert!(a.view().main().transcript().blocks().iter().any(|block|
        matches!(&block.body, ferrite_core::transcript::Body::Tool(tool)
            if tool.structured_result.as_ref() == Some(&payload))));
}
#[test]
fn codex_multi_file_change_retains_every_native_diff() {
    let r = Replay::new(
        "codex",
        vec![
            json!({"method":"item/started","params":{"threadId":"root","turnId":"turn","item":{"id":"edit","type":"fileChange","changes":[],"status":"inProgress"}}}),
            json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"id":"edit","type":"fileChange","status":"completed","changes":[{"path":"a.txt","kind":{"type":"update","move_path":null},"diff":"@@ -1 +1 @@\n-old\n+new\n"},{"path":"b.txt","kind":{"type":"add"},"diff":"created\n"}]}}}),
        ],
    );
    let a = fold(r.drain());
    let tool = a
        .view()
        .main()
        .transcript()
        .blocks()
        .iter()
        .find_map(|b| match &b.body {
            ferrite_core::transcript::Body::Tool(t) => Some(t),
            _ => None,
        })
        .unwrap();
    assert_eq!(tool.diffs.len(), 2);
    assert_eq!(tool.diffs[0].path, "a.txt");
    assert_eq!(tool.diffs[1].path, "b.txt");
    assert_eq!((tool.diffs[0].added, tool.diffs[0].removed), (1, 1));
    assert_eq!((tool.diffs[1].added, tool.diffs[1].removed), (1, 0));
}

#[test]
fn codex_unified_diff_preserves_function_headers_and_marker_like_content() {
    let r = Replay::new(
        "codex",
        vec![
            json!({"method":"item/started","params":{"threadId":"root","turnId":"turn","item":{"id":"edit","type":"fileChange","changes":[],"status":"inProgress"}}}),
            json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"id":"edit","type":"fileChange","status":"completed","changes":[{"path":"code.txt","kind":{"type":"update","move_path":null},"diff":"--- a/code.txt\n+++ b/code.txt\n@@ -12,2 +12,2 @@ fn example()\n---literal\n+++literal\n unchanged\n"}]}}}),
        ],
    );
    let a = fold(r.drain());
    let t = a
        .view()
        .main()
        .transcript()
        .blocks()
        .iter()
        .find_map(|b| match &b.body {
            ferrite_core::transcript::Body::Tool(t) => Some(t),
            _ => None,
        })
        .unwrap();
    let h = &t.diffs[0].hunks[0];
    assert_eq!(
        (h.old_start, h.old_lines, h.new_start, h.new_lines),
        (12, 2, 12, 2)
    );
    assert_eq!(h.lines, ["---literal", "+++literal", " unchanged"]);
}

#[test]
fn codex_mcp_and_dynamic_tools_preserve_native_timing_and_structured_payload() {
    for (kind, extra, payload) in [
        ("mcpToolCall", json!({"server":"search","tool":"find","result":{"content":[],"structuredContent":{"count":2}},"error":null}), json!({"content":[],"structuredContent":{"count":2}})),
        ("dynamicToolCall", json!({"tool":"lookup","contentItems":[{"type":"inputText","text":"found"}],"success":true}), json!([{"type":"inputText","text":"found"}])),
    ] {
        let mut item = json!({"id":"call","type":kind,"status":"completed","arguments":{},"durationMs":987});
        item.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        let r = Replay::new("codex", vec![json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":item}})]);
        let a = fold(r.drain());
        assert_eq!(a.view().main().timings().get("call").expect("native duration must survive even without a start frame").elapsed(), std::time::Duration::from_millis(987));
        assert!(a.view().main().transcript().blocks().iter().any(|b| matches!(&b.body, ferrite_core::transcript::Body::Tool(t) if t.structured_result.as_ref() == Some(&payload))), "native structured payload must use shared disclosure");
    }
}

#[test]
fn failed_mcp_tools_keep_the_native_error_and_duration() {
    let r=Replay::new("codex",vec![json!({"method":"item/completed","params":{"threadId":"root","turnId":"turn","item":{"id":"failed","type":"mcpToolCall","status":"failed","server":"search","tool":"find","arguments":{},"result":null,"error":{"message":"No access"},"durationMs":42}}})]);
    let a=fold(r.drain());
    assert_eq!(a.view().main().timings().get("failed").unwrap().elapsed(),std::time::Duration::from_millis(42));
    assert!(a.view().main().transcript().blocks().iter().any(|b|matches!(&b.body,ferrite_core::transcript::Body::Tool(t) if t.structured_result.as_ref()==Some(&json!({"message":"No access"})))));
}
