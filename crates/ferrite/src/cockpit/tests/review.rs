//! The Review slot (ADR 0021): a diff beside the Thread whose comments
//! gather on the Composer and ride the next prompt.
#[allow(unused_imports)]
use super::*;
use ferrite_core::review::Scope;

/// One Thread that took a prompt and made one edit in its checkout.
fn edited(name: &str) -> (Cockpit, Fake) {
    let (mut core, fake) = cockpit(name, 1);
    let thread = core.threads()[0];
    core.send(thread, "Tidy the nav".into());
    let path = here().join("src/nav.rs").display().to_string();
    let stream = fake.streams.borrow()[0].clone();
    stream
        .send(SessionEvent::ToolStarted {
            id: "edit-1".into(),
            name: "Edit".into(),
            input: serde_json::json!({ "file_path": path }),
        })
        .unwrap();
    stream
        .send(SessionEvent::ToolCompleted {
            id: "edit-1".into(),
            output: String::new(),
            is_error: false,
            result: ferrite_core::ToolResult::FileEdit {
                path,
                hunks: vec![ferrite_core::Hunk {
                    old_start: 7,
                    old_lines: 2,
                    new_start: 7,
                    new_lines: 2,
                    lines: vec![
                        " fn row() {".into(),
                        "-    let wide = 1;".into(),
                        "+    let wide = 2;".into(),
                    ],
                    section: None,
                }],
            },
        })
        .unwrap();
    stream
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    core.pump();
    (core, fake)
}

fn open_review(view: &Entity<CockpitView>, scope: Scope, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_beside(beside::Beside::Review(scope), window, cx)
        })
    });
    tick(cx);
}

/// The last turn's edit opens beside the Thread holding the keyboard: the
/// keys pick a range and comment on it, esc closes the slot, and the next
/// send carries the comment after the operator's words.
#[gpui::test]
fn a_comment_on_the_last_turn_rides_the_next_prompt(cx: &mut TestAppContext) {
    let (core, fake) = edited("review-last-turn");
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    open_review(&view, Scope::LastTurn, cx);
    assert!(
        cx.debug_bounds("review-lines").is_some(),
        "the diff is drawn"
    );
    assert!(
        cx.debug_bounds("reader-head").is_some(),
        "in the reader slot"
    );
    cx.simulate_keystrokes("down shift-down c");
    tick(cx);
    assert!(cx.debug_bounds("review-comment-field").is_some());
    cx.simulate_input("keep it 1");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(
        cx.debug_bounds("review-comment").is_some(),
        "the comment hangs under its lines"
    );
    view.read_with(cx, |view, cx| {
        let composer = view.panes[0].composer.read(cx);
        let [comment] = composer.review_comments() else {
            panic!("one comment: {:?}", composer.review_comments());
        };
        assert_eq!(comment.location(), "src/nav.rs:8");
        assert_eq!(comment.quote, ["-    let wide = 1;", "+    let wide = 2;"]);
    });
    assert!(
        cx.debug_bounds("review-attachment").is_some(),
        "the comments wait on the shelf"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.panes[0].preview.document().is_none()),
        "esc closes the slot"
    );
    cx.simulate_input("Please address these.");
    cx.simulate_keystrokes("enter");
    tick(cx);
    let sent = fake.sent.borrow().last().cloned().unwrap_or_default();
    assert_eq!(
        sent,
        "Please address these.\n\nReview comments on your changes:\n\n\
         src/nav.rs:8\n```diff\n-    let wide = 1;\n+    let wide = 2;\n```\nkeep it 1"
    );
    assert!(
        view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).is_empty()),
        "sent, the shelf is clear"
    );
}

/// The mouse does it all too: a click picks a line, `+ comment` opens the
/// field, and a scope's token switches the review to it.
#[gpui::test]
fn the_mouse_picks_comments_and_switches_scope(cx: &mut TestAppContext) {
    let (core, _fake) = edited("review-mouse");
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    open_review(&view, Scope::LastTurn, cx);
    let comment = cx
        .debug_bounds("review-add-comment")
        .expect("beside the pick");
    cx.simulate_click(comment.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("review-comment-field").is_some());
    cx.simulate_input("why?");
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, cx| {
        let comments = view.panes[0].composer.read(cx).review_comments().to_vec();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].location(), "src/nav.rs:7");
    });
    let review = view
        .read_with(cx, |view, _| view.panes[0].preview.review())
        .expect("a review is open");
    // The uncommitted scope reads this checkout for real, off the UI thread.
    let token = cx
        .debug_bounds("review-scope-uncommitted")
        .expect("on the bar");
    cx.simulate_click(token.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        review.read_with(cx, |review, _| review.scope()),
        Scope::Uncommitted
    );
    assert!(
        view.read_with(cx, |view, _| view.panes[0].preview.review()) == Some(review),
        "switched in place"
    );
}

/// History recall walks the line and its files only: the comments stay on
/// the shelf, never written into a recalled or restored line, and go out
/// once with the next send.
#[gpui::test]
fn history_recall_leaves_the_comments_on_the_shelf(cx: &mut TestAppContext) {
    let (core, fake) = edited("review-history");
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    open_review(&view, Scope::LastTurn, cx);
    cx.simulate_keystrokes("c");
    cx.simulate_input("why?");
    cx.simulate_keystrokes("enter escape");
    tick(cx);
    let draft = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let composer = view.panes[0].composer.read(cx);
            (
                composer.text().to_string(),
                composer.review_comments().len(),
            )
        })
    };
    cx.simulate_keystrokes("up");
    tick(cx);
    assert_eq!(
        draft(cx),
        ("Tidy the nav".into(), 1),
        "the old prompt, comments kept"
    );
    cx.simulate_keystrokes("down");
    tick(cx);
    assert_eq!(
        draft(cx),
        (String::new(), 1),
        "the draft back, comments not in it"
    );
    cx.simulate_keystrokes("enter");
    tick(cx);
    let sent = fake.sent.borrow().last().cloned().unwrap_or_default();
    assert_eq!(
        sent.matches("Review comments on your changes:").count(),
        1,
        "the comments go once: {sent}"
    );
    assert_eq!(sent.matches("why?").count(), 1, "{sent}");
}

/// A shift-click stretches the pick to a range; the files-changed card's
/// `review` opens the Review; the shelf chip's × drops every comment.
#[gpui::test]
fn shift_click_ranges_and_the_chip_drops_the_comments(cx: &mut TestAppContext) {
    let (core, _fake) = edited("review-range");
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        view.changed_files_card = Some(thread);
        cx.notify();
    });
    tick(cx);
    let review = cx
        .debug_bounds("changed-files-review")
        .expect("on the card's head");
    cx.simulate_click(review.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |view, _| view.panes[0].preview.review().is_some()),
        "the card opens the Review"
    );
    // The card opens the uncommitted scope; the last turn holds the edit.
    open_review(&view, Scope::LastTurn, cx);
    let first = cx.debug_bounds("review-line-0").expect("drawn");
    let last = cx.debug_bounds("review-line-2").expect("drawn");
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    cx.simulate_click(last.center(), gpui::Modifiers::shift());
    tick(cx);
    cx.simulate_keystrokes("c");
    cx.simulate_input("all of it");
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, cx| {
        let comments = view.panes[0].composer.read(cx).review_comments().to_vec();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].location(), "src/nav.rs:7-8");
    });
    let drop = cx
        .debug_bounds("drop-review-comments")
        .expect("the chip's ×");
    cx.simulate_click(drop.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).is_empty()),
        "the × drops them all"
    );
    assert!(cx.debug_bounds("review-comment").is_none());
}
