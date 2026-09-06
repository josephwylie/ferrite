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
    assert!(events.iter().any(|e|matches!(e,SessionEvent::ToolCompleted{id,result:ToolResult::Command{stdout,stderr},..} if id=="cmd" && stdout=="three tests passed\n" && stderr.is_empty())),"structured command output lost: {events:?}");
}
