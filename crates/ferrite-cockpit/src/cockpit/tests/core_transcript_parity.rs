//! Terminal-native parity, core-transcript package: the reading grid, the
//! call rows and their groups, elbows and folds, the turn's stamp, the
//! prompt's inline chips, the hover card's placement and the Markdown knobs.
use super::*;
use ferrite_core::transcript::{ToolBlock, ToolOutput, ToolState, TurnEnd};

/// A settled or running call with its retained output.
fn call(name: &str, summary: &str, state: ToolState, output: &str) -> ToolBlock {
    ToolBlock {
        caption: None,
        background: None,
        image: false,
        findings: Vec::new(),
        call: "c".into(),
        name: name.into(),
        title: None,
        summary: summary.into(),
        state,
        diffs: Vec::new(),
        structured_preview: None,
        result_line: None,
        output: (!output.is_empty()).then(|| ToolOutput {
            text: output.into(),
            omitted_bytes: 0,
        }),
        progress: None,
    }
}

/// `✻ 41s · 1.1k tokens`: no clock time.
#[test]
fn a_turn_ends_with_its_time_and_its_tokens() {
    let end = TurnEnd {
        outcome: ferrite_core::TurnOutcome::Completed,
        elapsed_ms: Some(41_000),
        completed_at: Some("7:32 pm".into()),
        input_tokens: Some(3_200),
        output_tokens: Some(1_100),
    };
    assert_eq!(end.text(), "41s \u{b7} 1.1k tokens");
    let older = TurnEnd {
        input_tokens: None,
        output_tokens: None,
        elapsed_ms: Some(192_000),
        ..end
    };
    assert_eq!(older.text(), "3m 12s");
}

/// A test run's verdict: `ok · 38 passed`; a failed run its tally,
/// `failed · 5 passed; 1 failed`, however early its failure came.
#[test]
fn a_test_run_reads_its_verdict_on_its_elbow() {
    let passing = call(
        "Bash",
        "cargo test",
        ToolState::Ok,
        "running 38 tests\ntest a ... ok\ntest result: ok. 38 passed; 0 failed; 0 ignored",
    );
    let elbow = pane::test_elbow(&passing).expect("a test run");
    assert_eq!(elbow.line(), "ok \u{b7} 38 passed");

    let early = "running 3 tests\ntest a ... FAILED\ntest b ... ok\ntest c ... ok\n\
                 failures:\n---- a stdout ----\npanicked\n\
                 test result: FAILED. 2 passed; 1 failed";
    let early = call(
        "Bash",
        "cargo test",
        ToolState::Failed("1 failed".into()),
        early,
    );
    let elbow = pane::test_elbow(&early).expect("a test run");
    assert_eq!(elbow.line(), "failed \u{b7} 2 passed; 1 failed");

    let late = "running 6 tests\ntest a ... ok\ntest b ... ok\ntest c ... ok\ntest d ... ok\n\
                test e ... FAILED\ntest f ... ok\ntest result: FAILED. 5 passed; 1 failed";
    let late = call(
        "Bash",
        "cargo test",
        ToolState::Failed("1 failed".into()),
        late,
    );
    let elbow = pane::test_elbow(&late).expect("a test run");
    assert_eq!(elbow.line(), "failed \u{b7} 5 passed; 1 failed");

    assert!(pane::test_elbow(&call("Bash", "ls", ToolState::Ok, "a")).is_none());

    // A live `cargo test` of a crate with a unit binary and one suite: two
    // `test result:` lines, the run's tally their sum.
    let two_binaries = "running 2 tests\ntest parse::a ... ok\ntest parse::b ... ok\n\
                        test result: ok. 2 passed; 0 failed; 0 ignored\n\
                        running 342 tests\ntest case_1 ... ok\ntest case_2 ... ok\n\
                        test median_even ... FAILED\n\
                        test result: FAILED. 341 passed; 1 failed; 0 ignored";
    let elbow = pane::test_elbow(&call(
        "Bash",
        "cargo test",
        ToolState::Failed("Exit code 101".into()),
        two_binaries,
    ))
    .expect("a test run");
    assert_eq!(elbow.line(), "failed \u{b7} 343 passed; 1 failed");
    // Piped (`| grep`, `| tail`) the command exits 0; its own report
    // still fails it.
    let piped = call(
        "Bash",
        "cargo test 2>&1 | tail -3",
        ToolState::Ok,
        "test median_even ... FAILED\n\ntest result: FAILED. 341 passed; 1 failed; 0 ignored",
    );
    let elbow = pane::test_elbow(&piped).expect("a test run");
    assert_eq!(elbow.lead, ferrite_ui::theme::words::FAILED);
    assert_eq!(elbow.line(), "failed \u{b7} 341 passed; 1 failed");
}

/// A prompt's files flow after its words as inline chips: its words
/// escaped (they are never markup), then one chip link per file; an image
/// is its thumbnail, never a chip.
#[test]
fn a_prompts_files_flow_inline_after_its_words() {
    let line = ferrite_core::prompt_files::compose(
        "Fix the [nav] jitter",
        &[
            std::path::PathBuf::from("/tmp/nav-jitter.png"),
            std::path::PathBuf::from("/tmp/nav-notes.md"),
        ],
    );
    let (text, files) = pane::prompt_text(&line, &[]);
    assert_eq!(files.len(), 2);
    let pane::PromptText::Markdown(source) = text else {
        panic!("a prompt with files is Markdown");
    };
    assert_eq!(
        source,
        "Fix the \\[nav\\] jitter [nav\\-notes\\.md](<ferrite-chip:/tmp/nav-notes.md>)"
    );
    let line = ferrite_core::prompt_files::compose(
        "Look",
        &[std::path::PathBuf::from("/tmp/nav-jitter.png")],
    );
    assert!(
        matches!(pane::prompt_text(&line, &[]).0, pane::PromptText::Literal(words) if words == "Look")
    );
    let (plain, files) = pane::prompt_text("Just words", &[]);
    assert!(files.is_empty());
    assert!(matches!(plain, pane::PromptText::Literal(words) if words == "Just words"));
}

/// A prompt's `code` is Markdown code — Claude's red chip — while the rest
/// of its words stay escaped; a backtick never closed stays a backtick.
#[test]
fn a_prompts_code_is_drawn_as_code() {
    let (text, files) = pane::prompt_text(
        "Run `ls /definitely-missing-dir`, then ``a ` b`` (once) and `open",
        &[],
    );
    assert!(files.is_empty());
    let pane::PromptText::Markdown(source) = text else {
        panic!("a prompt that quotes code is Markdown");
    };
    assert_eq!(
        source,
        "Run `ls /definitely-missing-dir`\\, then ``a ` b`` \\(once\\) and \\`open"
    );
    assert!(matches!(
        pane::prompt_text("an `unclosed tick", &[]).0,
        pane::PromptText::Literal(words) if words == "an `unclosed tick"
    ));
}

/// Answers underline a link at rest (the Claude app's links), keep a
/// column of numbers left-aligned as Claude's tables do, and frame their
/// images like a prompt's.
#[test]
fn answers_underline_links_keep_columns_left_and_frame_images() {
    let style = ferrite_pane::rich::style(px(ferrite_ui::theme::FS_UI));
    assert!(!style.link_underline_on_hover());
    assert!(!style.numeric_columns_right());
    let cell = ferrite_ui::theme::tx_cell(ferrite_ui::theme::FS_PROSE);
    assert_eq!(
        style.image().size.width,
        Some(px(ferrite_ui::theme::IMAGE_CELLS * cell).into())
    );
}

/// The transcript opens on its prompt, no banner; edits fold into one group
/// (`Edited 2 files`); an edit's diff shows once its call is opened.
#[gpui::test]
fn edits_fold_and_an_edits_diff_waits_to_be_opened(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("ct-banner-rows", 1);
    let thread = core.threads()[0];
    core.send(thread, "Fix the nav".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    for (id, old, new) in [
        ("edit-1", "-old one", "+new one"),
        ("edit-2", "-old two", "+new two"),
    ] {
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolStarted {
                id: id.into(),
                name: "Edit".into(),
                input: serde_json::json!({ "file_path": "src/nav.rs" }),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolCompleted {
                id: id.into(),
                output: String::new(),
                is_error: false,
                result: ferrite_core::ToolResult::FileEdit {
                    path: "src/nav.rs".into(),
                    hunks: vec![ferrite_core::Hunk {
                        old_start: 1,
                        old_lines: 1,
                        new_start: 1,
                        new_lines: 1,
                        lines: vec![old.into(), new.into()],
                        section: None,
                    }],
                },
            })
            .unwrap();
    }
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    open_folds(&view, cx);
    assert!(cx.debug_bounds("transcript-banner").is_none(), "no banner");
    assert!(cx.debug_bounds("tool-group-edit-1").is_some(), "one group");
    let runs = view.read_with(cx, |view, _| view.selection.registered(thread));
    assert!(
        !runs
            .iter()
            .any(|(_, _, _, text)| text == "new one" || text == "new two"),
        "no diff shows until its call is opened"
    );
    view.update(cx, |view, cx| {
        view.panes[0].toggle_tool(&pane::DisclosureId::Group("edit-1".into()));
        view.panes[0].toggle_tool(&pane::DisclosureId::Tool("edit-2".into()));
        cx.notify();
    });
    tick(cx);
    let runs = view.read_with(cx, |view, _| view.selection.registered(thread));
    assert!(
        runs.iter().any(|(_, _, _, text)| text == "new two"),
        "the opened edit shows its diff"
    );
    assert!(
        !runs.iter().any(|(_, _, _, text)| text == "new one"),
        "the other stays shut"
    );
}

/// A running suite shows `running N tests`, its bar and `done/total`.
#[gpui::test]
fn a_running_test_suite_shows_its_bar_and_count(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("ct-test-bar", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "suite".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cargo test" }),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolOutputDelta {
            id: "suite".into(),
            text: "running 4 tests\ntest a ... ok\n".into(),
        })
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("tool-test-progress").is_some());
    assert!(cx.debug_bounds("test-bar").is_some(), "the bar");
}

/// A long Edit's diff, opened, draws unified in its card under a header
/// naming the file and stops at `HUNK_MAX_ROWS` with `… +N lines`; the
/// other edit's stays shut.
#[gpui::test]
fn a_long_edit_diff_caps_and_folds(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("ct-long-diff", 1);
    let thread = core.threads()[0];
    core.send(thread, "Rewrite the nav".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    super::hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(1000.)));
    // Forty one-line changes: 80 lines, so the cap lands mid-hunk.
    let long: Vec<String> = (0..40)
        .flat_map(|n| [format!("-old {n}"), format!("+new {n}")])
        .collect();
    for (id, lines) in [
        ("long-1", long.clone()),
        ("long-2", vec!["-again".into(), "+later".into()]),
    ] {
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolStarted {
                id: id.into(),
                name: "Edit".into(),
                input: serde_json::json!({ "file_path": "src/nav.rs" }),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolCompleted {
                id: id.into(),
                output: String::new(),
                is_error: false,
                result: ferrite_core::ToolResult::FileEdit {
                    path: "src/nav.rs".into(),
                    hunks: vec![ferrite_core::Hunk {
                        old_start: 1,
                        old_lines: 40,
                        new_start: 1,
                        new_lines: 40,
                        lines: lines.clone(),
                        section: None,
                    }],
                },
            })
            .unwrap();
    }
    tick(cx);
    assert!(cx.debug_bounds("diff-box").is_none(), "shut, no diff");
    view.update(cx, |view, cx| {
        view.panes[0].toggle_tool(&pane::DisclosureId::Group("long-1".into()));
        view.panes[0].toggle_tool(&pane::DisclosureId::Tool("long-1".into()));
        cx.notify();
    });
    tick(cx);
    assert!(
        cx.debug_bounds("diff-box").is_some(),
        "the diff sits in its box"
    );
    assert!(
        cx.debug_bounds("diff-header").is_some(),
        "the card names its file"
    );
    assert!(
        cx.debug_bounds("diff-omitted").is_some(),
        "the cap names what it left out"
    );
    let runs = view.read_with(cx, |view, _| view.selection.registered(thread));
    let shown = |text: &str| runs.iter().any(|(_, _, _, run)| run == text);
    assert!(shown("old 0") && shown("new 0"), "both sides of a change");
    assert!(
        !shown("old 20"),
        "past HUNK_MAX_ROWS ({}) nothing draws",
        ferrite_ui::theme::HUNK_MAX_ROWS
    );
    assert!(!shown("later"), "the other edit's diff stays shut");
}

/// A folded command's call line gives way in a narrow Pane: cut at the
/// row's edge, and its `›` stays in the row after it.
#[gpui::test]
fn a_long_first_line_leaves_its_fold_in_the_row(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("ct-fold-head", 1);
    let thread = core.threads()[0];
    core.send(thread, "Summarise".into());
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let first = "a-p06 skip Tribe Market (Level Group OMO43465) ".repeat(4);
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "summary".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "python3 prep.py --summary" }),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolCompleted {
            id: "summary".into(),
            output: format!("{first}\n+ 12 bills ready"),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    tick(cx);
    open_live(cx);
    for width in [1200., 700.] {
        cx.simulate_resize(gpui::size(px(width), px(900.)));
        tick(cx);
        let row = cx.debug_bounds("tool-row-summary").expect("the call's row");
        let chevron = cx.debug_bounds("disclosure-chevron").expect("its `›`");
        assert!(
            chevron.right() <= row.right() + px(0.5),
            "at {width}px the `›` stays in the row: {chevron:?} / {row:?}"
        );
        let head = cx.debug_bounds("tool-call").expect("the call line");
        assert!(
            head.right() <= chevron.left(),
            "{head:?} before {chevron:?}"
        );
    }
}
