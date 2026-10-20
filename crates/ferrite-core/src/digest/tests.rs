use std::collections::HashMap;
use std::time::Duration;

use super::*;
use crate::transcript::{Input, Todos, TurnEnd};
use crate::{SessionEvent, ToolResult};

/// The wall card folds everything the L3 recipe needs that is not an
/// O(1) read — built on change, never per frame.
#[test]
fn the_wall_card_folds_progress_result_and_context_lines() {
    let mut transcript = Transcript::default();
    for (id, subject) in [("1", "a"), ("2", "b"), ("3", "c"), ("4", "d")] {
        transcript.apply(Input::Event(SessionEvent::Progress {
            event: crate::progress::ProgressEvent::Task {
                id: id.into(),
                subject: subject.into(),
                status: Some(crate::progress::StepStatus::Pending),
                deleted: false,
            },
        }));
    }
    for task in ["1", "2", "3"] {
        transcript.apply(Input::Event(SessionEvent::Progress {
            event: crate::progress::ProgressEvent::Task {
                id: task.into(),
                subject: String::new(),
                status: Some(crate::progress::StepStatus::Completed),
                deleted: false,
            },
        }));
    }
    assert_eq!(transcript.todos(), Some(Todos { done: 3, total: 4 }));
    let card = WallCard::of(&transcript, None, None);
    assert_eq!(card.working.as_str(), "Working");

    // A red suite flips the folded flag and folds the failing line —
    // with the run's own count when its output reported one.
    transcript.apply(Input::Event(SessionEvent::ToolStarted {
        id: "test1".into(),
        name: "Bash".into(),
        input: serde_json::json!({ "command": "cargo test" }),
    }));
    transcript.apply(Input::Event(SessionEvent::ToolCompleted {
        id: "test1".into(),
        output: "test result: FAILED. 357 passed; 2 failed".into(),
        is_error: true,
        result: ToolResult::Opaque,
    }));
    let red = WallCard::of(&transcript, None, None);
    assert!(red.tests_failing);
    assert_eq!(red.failing_count, Some(2));

    // A Decision's subject becomes the alert's second line, wearing the
    // tool prefix every Decision surface shares (#22 C7).
    let decision = Decision {
        delivery: Default::default(),
        kind: Default::default(),
        policy: Default::default(),
        id: "perm".into(),
        tool_use_id: "toolu".into(),
        tool_name: "Bash".into(),
        description: "gh issue close 212".into(),
        input: serde_json::Value::Null,
        suggestions: vec![],
    };
    assert_eq!(
        WallCard::of(&transcript, Some(&decision), None)
            .context
            .as_str(),
        "Bash \u{b7} gh issue close 212"
    );

    // A closed Session's reason is promoted into the alert line itself
    // (#22 C14).
    let mut closed = Transcript::default();
    closed.apply(Input::Event(SessionEvent::Closed {
        reason: "claude CLI exited with code 1".into(),
    }));
    assert_eq!(
        WallCard::of(&closed, None, None).context.as_str(),
        "claude CLI exited with code 1"
    );
}

/// F-6: a tile's lines are one dim run each, glyph included — a call's
/// trail inline after one space, a red run's result under it, a passed
/// run's `ok`, and a pending approval's command in place of the call it
/// gates.
#[test]
fn the_wall_card_prints_calls_trails_results_and_the_decision() {
    let mut transcript = Transcript::default();
    transcript.apply(Input::Prompt("fix it".into()));
    transcript.apply(Input::Event(SessionEvent::ToolStarted {
        id: "t1".into(),
        name: "Bash".into(),
        input: serde_json::json!({ "command": "cargo test --workspace" }),
    }));
    transcript.apply(Input::Event(SessionEvent::ToolCompleted {
        id: "t1".into(),
        output: "test result: FAILED. 357 passed; 2 failed; 0 ignored".into(),
        is_error: true,
        result: ToolResult::Opaque,
    }));
    let timings: HashMap<String, ToolTiming> =
        [("t1".to_string(), ToolTiming::Done(Duration::from_secs(61)))]
            .into_iter()
            .collect();
    let card = WallCard::of(&transcript, None, Some(&timings));
    let texts: Vec<&str> = card.lines.iter().map(|line| line.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "\u{25cf} Bash(cargo test --workspace) 1m01s",
            "  \u{2514} 357 passed; 2 failed"
        ],
        "the prompt leaves no line"
    );
    assert!(card.lines.iter().all(|line| !line.wraps));

    // A passed run says `ok` in its trail and hangs nothing.
    transcript.apply(Input::Event(SessionEvent::ToolStarted {
        id: "t2".into(),
        name: "Bash".into(),
        input: serde_json::json!({ "command": "cargo test -p ferrite theme::" }),
    }));
    transcript.apply(Input::Event(SessionEvent::ToolCompleted {
        id: "t2".into(),
        output: "test result: ok. 24 passed; 0 failed".into(),
        is_error: false,
        result: ToolResult::Opaque,
    }));
    let card = WallCard::of(&transcript, None, None);
    assert_eq!(
        card.lines.last().unwrap().text.as_str(),
        "\u{25cf} Bash(cargo test -p ferrite theme::) ok"
    );

    // Prose is a line that may wrap; the call a Decision gates gives
    // way to what the Decision would run.
    transcript.apply(Input::Event(SessionEvent::TextDelta {
        text: "Six of seven stale issues closed\n\n".into(),
    }));
    let command = "gh issue close 212 --reason \"not planned\"";
    transcript.apply(Input::Event(SessionEvent::ToolStarted {
        id: "t3".into(),
        name: "Bash".into(),
        input: serde_json::json!({ "command": command }),
    }));
    let decision = Decision {
        delivery: Default::default(),
        kind: Default::default(),
        policy: Default::default(),
        id: "perm".into(),
        tool_use_id: "t3".into(),
        tool_name: "Bash".into(),
        description: String::new(),
        input: serde_json::json!({ "command": command }),
        suggestions: vec![],
    };
    let card = WallCard::of(&transcript, Some(&decision), None);
    assert_eq!(
        card.lines[card.lines.len() - 3..]
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        [
            "\u{25cf} Six of seven stale issues closed",
            "\u{25c6} Bash wants to run",
            "  $ gh issue close 212 --reason \"not planned\"",
        ]
    );
}

/// One derivation for every surface that names a Decision (L1 card, L2
/// cell, wall alert) — and the unreadable-request fallback holds even
/// when the provider names no tool at all.
#[test]
fn every_decision_surface_shares_one_subject_derivation() {
    let decision = |tool: &str, description: &str| Decision {
        delivery: Default::default(),
        kind: Default::default(),
        policy: Default::default(),
        id: "perm".into(),
        tool_use_id: "toolu".into(),
        tool_name: tool.into(),
        description: description.into(),
        input: serde_json::Value::Null,
        suggestions: vec![],
    };
    let full = decision("Bash", "gh issue close 212");
    assert_eq!(
        decision_subject(&full).as_str(),
        "Bash \u{b7} gh issue close 212"
    );
    assert!(!decision_subject(&full).contains(':'), "no colon label");
    // No description: the tool's name is the subject.
    let bare = decision("Write", "");
    assert_eq!(decision_subject(&bare).as_str(), "Write");
    // No tool at all: the honest fallback, on both lines.
    let unreadable = decision("", "");
    assert_eq!(
        decision_subject(&unreadable).as_str(),
        "unreadable permission request"
    );
    // The wall's alert context runs through the same derivation.
    let transcript = Transcript::default();
    assert_eq!(
        WallCard::of(&transcript, Some(&unreadable), None)
            .context
            .as_str(),
        "unreadable permission request"
    );
}

/// What a tile's line says is one spelling per Body — a completed turn
/// leaves no row, a docked Decision's notice keeps only its lead
/// phrase, a call reads as the transcript spells it.
#[test]
fn the_tile_text_is_its_visible_text() {
    use crate::transcript::TurnEnd;
    assert_eq!(
        tail_text(&Body::Prompt("  Fix the board  ".into()), false).as_deref(),
        Some("Fix the board")
    );
    let notice = Body::Notice("asks 1 question \u{b7} Which layout?".into());
    assert_eq!(
        tail_text(&notice, false).as_deref(),
        Some("asks 1 question \u{b7} Which layout?")
    );
    assert_eq!(tail_text(&notice, true).as_deref(), Some("asks 1 question"));
    let done = Body::TurnEnd(TurnEnd {
        outcome: TurnOutcome::Completed,
        elapsed_ms: Some(18_000),
        completed_at: None,
        input_tokens: None,
        output_tokens: None,
    });
    assert_eq!(tail_text(&done, false), None, "no `Worked for` row");
    let stopped = TurnEnd {
        outcome: TurnOutcome::Interrupted,
        elapsed_ms: Some(4_100),
        completed_at: None,
        input_tokens: None,
        output_tokens: None,
    };
    assert_eq!(
        tail_text(&Body::TurnEnd(stopped.clone()), false),
        Some(stopped.text())
    );
    assert_eq!(tail_text(&Body::Prompt("   ".into()), false), None);
}

#[test]
fn a_finished_turn_reads_its_length_as_the_tile_says_it() {
    assert_eq!(worked_label(Duration::from_secs(192)), "3m 12s");
    assert_eq!(worked_label(Duration::from_secs(41)), "41s");
}
