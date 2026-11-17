//! A turn's wrap-up box (HD58): the agent's `wrap_up` call becomes the
//! turn's closing box, never a tool row.

use ferrite_core::transcript::{Body, Input, Transcript};
use ferrite_core::wrap_up::{WrapUp, CLAUDE_NAME};
use ferrite_core::{SessionEvent, ToolResult};

#[test]
fn a_wrap_up_call_is_the_turns_box_not_a_tool_row() {
    let mut transcript = Transcript::default();
    transcript.apply(Input::Prompt("fix it".into()));
    transcript.apply(Input::Event(SessionEvent::ToolStarted {
        id: "w".into(),
        name: CLAUDE_NAME.into(),
        input: serde_json::json!({
            "recap": "Fixed the parser; tests pass.",
            "ask": "Should I also bump the version?",
            "next": ["Commit these changes", "Open a PR", "Merge it"],
        }),
    }));
    transcript.apply(Input::Event(SessionEvent::ToolCompleted {
        id: "w".into(),
        output: ferrite_core::wrap_up::ANSWER.into(),
        is_error: false,
        result: ToolResult::Opaque,
    }));
    let bodies: Vec<&Body> = transcript.blocks().iter().map(|block| &block.body).collect();
    assert!(
        !bodies.iter().any(|body| matches!(body, Body::Tool(_) | Body::Notice(_))),
        "{bodies:?}"
    );
    let wraps: Vec<&WrapUp> = bodies
        .iter()
        .filter_map(|body| match body {
            Body::WrapUp(wrap) => Some(wrap),
            _ => None,
        })
        .collect();
    assert_eq!(
        wraps,
        [&WrapUp {
            call: "w".into(),
            recap: "Fixed the parser; tests pass.".into(),
            ask: Some("Should I also bump the version?".into()),
            blocked: false,
            next: vec!["Commit these changes".into(), "Open a PR".into()],
        }]
    );
}
