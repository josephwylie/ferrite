//! An Edit or a Write has its row while its input streams (HD39).

use ferrite_core::transcript::{Body, Input, ToolState, Transcript};
use ferrite_core::{SessionEvent, ToolResult};

fn rows(transcript: &Transcript) -> usize {
    transcript
        .blocks()
        .iter()
        .filter(|block| matches!(block.body, Body::Tool(_)))
        .count()
}

/// The first draft opens a running row naming the file; later drafts grow
/// its would-be change; the settled call is the same row, not a second;
/// its result settles it.
#[test]
fn a_write_streams_into_one_row_that_its_call_settles() {
    let mut transcript = Transcript::default();
    transcript.apply(Input::Prompt("write it".into()));
    let draft = |content: &str| {
        Input::Event(SessionEvent::ToolDraft {
            id: "w".into(),
            name: "Write".into(),
            input: serde_json::json!({"file_path": "/work/notes.md", "content": content}),
        })
    };
    transcript.apply(draft("one\n"));
    let row = transcript.tool("w").expect("the draft opens the row");
    assert_eq!(row.state, ToolState::Running);
    assert!(row.summary.contains("notes.md"), "{row:?}");
    assert_eq!(row.diffs[0].added, 1);

    transcript.apply(draft("one\ntwo\nthree\n"));
    assert_eq!(transcript.tool("w").unwrap().diffs[0].added, 3);
    assert_eq!(rows(&transcript), 1);

    transcript.apply(Input::Event(SessionEvent::ToolStarted {
        id: "w".into(),
        name: "Write".into(),
        input: serde_json::json!({"file_path": "/work/notes.md", "content": "one\ntwo\nthree\n"}),
    }));
    assert_eq!(rows(&transcript), 1, "the settled call is the drafted row");
    transcript.apply(Input::Event(SessionEvent::ToolCompleted {
        id: "w".into(),
        output: "File created".into(),
        is_error: false,
        result: ToolResult::Opaque,
    }));
    assert_eq!(transcript.tool("w").unwrap().state, ToolState::Ok);
}

/// A command's input is not drafted: its row waits for the settled call.
#[test]
fn a_command_has_no_draft_row() {
    let mut transcript = Transcript::default();
    transcript.apply(Input::Event(SessionEvent::ToolDraft {
        id: "b".into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "ls"}),
    }));
    assert_eq!(rows(&transcript), 0);
}
