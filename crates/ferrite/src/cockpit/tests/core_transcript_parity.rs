//! Terminal-native parity, core-transcript package: the reading grid, the
//! banner, every call its own row, elbows and folds, the turn's stamp, the
//! prompt's inline chips, the hover card's placement and the Markdown knobs.
use super::*;
use ferrite_core::transcript::{BlockId, ToolBlock, ToolOutput, ToolState, TurnEnd};

/// A settled or running call with its retained output.
fn call(name: &str, summary: &str, state: ToolState, output: &str) -> ToolBlock {
    ToolBlock {
        call: "c".into(),
        name: name.into(),
        title: None,
        summary: summary.into(),
        state,
        diffs: Vec::new(),
        structured_result: None,
        result_line: None,
        output: (!output.is_empty()).then(|| ToolOutput {
            text: output.into(),
            omitted_bytes: 0,
        }),
        progress: None,
    }
}

/// Standard reads at 13px on a 20px line; a cell is 7.8px, so an image's
/// frame is 374.4px.
#[test]
fn standard_reads_at_13_on_a_20px_line() {
    use ferrite_core::settings::ReadingSize;
    assert_eq!(crate::theme::answer_text_size(ReadingSize::STANDARD), 13.);
    assert_eq!(crate::theme::answer_line_height(ReadingSize::STANDARD), 20.);
    let grid = pane::Grid::of(ReadingSize::STANDARD);
    assert!((grid.cell() - 7.8).abs() < 1e-4);
    assert!((crate::theme::IMAGE_CELLS * grid.cell() - 374.4).abs() < 1e-3);
}

/// `✻ Worked for 41s · 7:32 pm · ↑ 3.2k ↓ 1.1k`.
#[test]
fn a_turn_ends_with_its_time_its_clock_and_its_tokens() {
    let end = TurnEnd {
        outcome: ferrite_core::TurnOutcome::Completed,
        elapsed_ms: Some(41_000),
        completed_at: Some("7:32 pm".into()),
        input_tokens: Some(3_200),
        output_tokens: Some(1_100),
    };
    assert_eq!(
        end.text(),
        "Worked for 41s \u{b7} 7:32 pm \u{b7} \u{2191} 3.2k \u{2193} 1.1k"
    );
    let older = TurnEnd {
        input_tokens: None,
        output_tokens: None,
        elapsed_ms: Some(192_000),
        ..end
    };
    assert_eq!(older.text(), "Worked for 3m 12s \u{b7} 7:32 pm");
}

/// A test run's elbow: `ok · 38 passed`; an early failure prints its first
/// line and its first three test lines; a late one its tally and its
/// `FAILED` lines without `test `; the rest folds.
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
    assert!(elbow.preview.is_empty());
    assert_eq!(elbow.hidden, 0);

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
    assert_eq!(elbow.line(), "failed \u{b7} running 3 tests");
    assert_eq!(
        elbow.preview,
        ["test a ... FAILED", "test b ... ok", "test c ... ok"]
    );
    assert_eq!(elbow.hidden, 4);

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
    assert_eq!(elbow.preview, ["e ... FAILED"]);
    assert_eq!(elbow.hidden, 6);

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
    assert_eq!(elbow.lead, crate::theme::words::FAILED);
    assert_eq!(elbow.line(), "failed \u{b7} 341 passed; 1 failed");
}

/// A command's output folds whole behind its first line: `└ 7 issues · + 2
/// lines`.
#[test]
fn a_commands_output_folds_behind_its_first_line() {
    let lint = call("Bash", "cargo clippy", ToolState::Ok, "7 issues\nA\n\nB\n");
    assert_eq!(
        pane::command_fold(&lint),
        Some((
            "7 issues".to_string(),
            vec!["A".to_string(), "B".to_string()]
        ))
    );
    assert_eq!(
        pane::command_fold(&call("Bash", "pwd", ToolState::Ok, "/x")),
        None,
        "one line is all on the elbow"
    );
    assert_eq!(
        pane::command_fold(&call("Read", "a.rs", ToolState::Ok, "1\n2")),
        None,
        "a read is not a command"
    );
}

/// A prompt's files flow after its words as inline chips: its words
/// escaped (they are never markup), then one chip link per file.
#[test]
fn a_prompts_files_flow_inline_after_its_words() {
    let line = ferrite_core::prompt_files::compose(
        "Fix the [nav] jitter",
        &[std::path::PathBuf::from("/tmp/nav-jitter.png")],
    );
    let (text, files) = pane::prompt_text(&line);
    assert_eq!(files.len(), 1);
    let pane::PromptText::Markdown(source) = text else {
        panic!("a prompt with files is Markdown");
    };
    assert_eq!(
        source,
        "Fix the \\[nav\\] jitter [nav\\-jitter\\.png](<ferrite-chip:/tmp/nav-jitter.png>)"
    );
    let (plain, files) = pane::prompt_text("Just words");
    assert!(files.is_empty());
    assert!(matches!(plain, pane::PromptText::Literal(words) if words == "Just words"));
}

/// The hover card hangs `HOVER_CARD_GAP` under its path, flips above it
/// near the window's foot, and keeps `HOVER_CARD_EDGE` from the window's
/// right edge.
#[test]
fn the_hover_card_hangs_under_its_path_and_flips_above_near_the_foot() {
    use gpui::{point, size, Bounds};
    let card = size(px(780.), px(166.));
    let viewport = size(px(1440.), px(900.));
    let anchor = Bounds::new(point(px(200.), px(300.)), size(px(120.), px(20.)));
    assert_eq!(
        crate::hover_card::hang_origin(anchor, card, viewport),
        point(px(200.), px(320. + crate::theme::HOVER_CARD_GAP))
    );
    let low = Bounds::new(point(px(200.), px(800.)), size(px(120.), px(20.)));
    assert_eq!(
        crate::hover_card::hang_origin(low, card, viewport).y,
        px(800. - crate::theme::HOVER_CARD_GAP - 166.)
    );
    let right = Bounds::new(point(px(1400.), px(300.)), size(px(30.), px(20.)));
    assert_eq!(
        crate::hover_card::hang_origin(right, card, viewport).x,
        px(1440. - 16. - 780.)
    );
}

/// Answers underline a link only under the pointer, right-align a column
/// of numbers, and frame their images like a prompt's.
#[test]
fn answers_underline_links_on_hover_align_numbers_and_frame_images() {
    let style = crate::rich::style(px(crate::theme::FS_UI));
    assert!(style.link_underline_on_hover());
    assert!(style.numeric_columns_right());
    let cell = crate::theme::tx_cell(crate::theme::FS_PROSE);
    assert_eq!(
        style.image().size.width,
        Some(px(crate::theme::IMAGE_CELLS * cell).into())
    );
}

/// The banner heads the transcript; every call is its own row; a later edit
/// to a file already diffed in its turn folds its diff behind `+ show diff`
/// until it is opened.
#[gpui::test]
fn the_banner_heads_the_rows_and_a_later_edit_folds_its_diff(cx: &mut TestAppContext) {
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
    assert!(cx.debug_bounds("transcript-banner").is_some(), "the banner");
    assert!(cx.debug_bounds("tool-row-edit-1").is_some());
    assert!(cx.debug_bounds("tool-row-edit-2").is_some(), "its own row");
    let runs = view.read_with(cx, |view, _| view.selection.registered(thread));
    assert!(
        runs.iter()
            .any(|(block, _, _, text)| *block == BlockId::BANNER && text.contains("1 turn")),
        "the banner's facts select like any row: {runs:?}"
    );
    assert!(
        runs.iter().any(|(_, _, _, text)| text == "new one"),
        "the first edit's diff shows"
    );
    assert!(
        !runs.iter().any(|(_, _, _, text)| text == "new two"),
        "a later edit to the same file folds its diff"
    );
    view.update(cx, |view, cx| {
        view.panes[0].toggle_tool(&pane::DisclosureId::Diff("edit-2".into()));
        cx.notify();
    });
    tick(cx);
    let runs = view.read_with(cx, |view, _| view.selection.registered(thread));
    assert!(
        runs.iter().any(|(_, _, _, text)| text == "new two"),
        "`+ show diff` opens it"
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
    let bar = cx.debug_bounds("test-bar").expect("the bar");
    assert_eq!(bar.size.height, px(crate::theme::TEST_BAR_H));
}
