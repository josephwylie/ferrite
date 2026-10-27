//! The Review slot (ADR 0021): a diff beside the Thread, read by the
//! vendored review core, whose comments gather on the Composer and ride
//! the next prompt.
#[allow(unused_imports)]
use super::*;
use ferrite_reviewr::model::Scope;

/// What the checkout's one file read before the Thread's edit.
const NAV: &str = "fn row() {\n    let wide = 1;\n}\n";

fn git(repo: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// One Thread in a scratch checkout of its own whose `src/nav.rs` has one
/// uncommitted edit: its second line, `1` become `2`.
fn edited(name: &str) -> (Cockpit, Fake, std::path::PathBuf) {
    let base = scratch(name);
    let repo = repo_in(&base);
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/nav.rs"), NAV).unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "nav"]);
    std::fs::write(repo.join("src/nav.rs"), NAV.replace('1', "2")).unwrap();
    let fake = Fake::default();
    let store = Store::open(base.join("store")).unwrap();
    let mut core = Cockpit::new(store, Box::new(fake.clone()));
    core.open(
        Provider::Claude,
        WorkspaceChoice::Main {
            checkout: repo.clone(),
        },
    )
    .unwrap();
    (core, fake, repo)
}

/// The Thread's turn that made the checkout's edit: "Tidy the nav", then
/// an `Edit` of `src/nav.rs` the files-changed card lists, then its end.
fn tidied(core: &mut Cockpit, fake: &Fake, repo: &std::path::Path) {
    let thread = core.threads()[0];
    core.send(thread, "Tidy the nav".into());
    let path = repo.join("src/nav.rs").display().to_string();
    let stream = fake.streams.borrow()[0].clone();
    for event in [
        SessionEvent::ToolStarted {
            id: "edit-1".into(),
            name: "Edit".into(),
            input: serde_json::json!({ "file_path": path }),
        },
        SessionEvent::ToolCompleted {
            id: "edit-1".into(),
            output: String::new(),
            is_error: false,
            result: ferrite_core::ToolResult::FileEdit {
                path,
                hunks: vec![ferrite_core::Hunk {
                    old_start: 1,
                    old_lines: 3,
                    new_start: 1,
                    new_lines: 3,
                    lines: vec![
                        " fn row() {".into(),
                        "-    let wide = 1;".into(),
                        "+    let wide = 2;".into(),
                        " }".into(),
                    ],
                    section: None,
                }],
            },
        },
        SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        },
    ] {
        stream.send(event).unwrap();
    }
    core.pump();
}

/// Pump once, handing the Review core's turn host the turn's news, then
/// read the checkout until `done` holds there. The turn host snapshots on
/// threads of its own that say nothing when they finish (and the Review
/// core is not Ferrite's to change), so the checkout is the only witness;
/// the window is not pumped meanwhile.
fn turn_host_wrote(cx: &mut gpui::VisualTestContext, done: impl Fn() -> bool) {
    tick(cx);
    let lost = std::time::Instant::now() + ferrite_core::doorbell::LOST_AFTER;
    while !done() {
        assert!(
            std::time::Instant::now() < lost,
            "the turn host never wrote"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn open_review(view: &Entity<CockpitView>, scope: Scope, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_beside(beside::Beside::Review(scope), window, cx)
        })
    });
    until(cx, |cx| cx.debug_bounds("review-lines").is_some());
}

/// The comments on the Composer's shelf, by location and words.
fn comments(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<(String, String)> {
    view.read_with(cx, |view, cx| {
        view.panes[0]
            .composer
            .read(cx)
            .review_comments()
            .iter()
            .map(|comment| (comment.location(), comment.text.clone()))
            .collect()
    })
}

/// The uncommitted edit opens beside the Thread holding the keyboard: the
/// keys pick a range and comment on it, esc closes the slot, and the next
/// send carries the comment after the operator's words, as herdr-reviewr
/// sends a review.
#[gpui::test]
fn a_comment_on_the_uncommitted_work_rides_the_next_prompt(cx: &mut TestAppContext) {
    let (core, fake, _repo) = edited("review-uncommitted");
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    open_review(&view, Scope::Uncommitted, cx);
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
    assert_eq!(
        comments(&view, cx),
        [("src/nav.rs:2".into(), "keep it 1".into())]
    );
    assert!(
        cx.debug_bounds("review-attachment").is_some(),
        "the comments wait on the shelf"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| !view.panes[0].review.is_open()),
        "esc closes the slot"
    );
    cx.simulate_input("Please address these.");
    cx.simulate_keystrokes("enter");
    tick(cx);
    let sent = fake.sent.borrow().last().cloned().unwrap_or_default();
    assert_eq!(
        sent,
        "Please address these.\n\n\
         src/nav.rs:2\n-    let wide = 1;\n+    let wide = 2;\nkeep it 1"
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
    let (core, _fake, _repo) = edited("review-mouse");
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    open_review(&view, Scope::Uncommitted, cx);
    let comment = cx
        .debug_bounds("review-add-comment")
        .expect("beside the pick");
    cx.simulate_click(comment.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("review-comment-field").is_some());
    cx.simulate_input("why?");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        comments(&view, cx),
        [("src/nav.rs:1".into(), "why?".into())]
    );
    let review = view
        .read_with(cx, |view, _| view.panes[0].review.review())
        .expect("a review is open");
    let token = cx.debug_bounds("review-scope-branch").expect("on the bar");
    cx.simulate_click(token.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        review.read_with(cx, |review, _| review.scope()),
        Scope::Branch
    );
    assert!(
        view.read_with(cx, |view, _| view.panes[0].review.review()) == Some(review),
        "switched in place"
    );
}

/// History recall walks the line and its files only: the comments stay on
/// the shelf, never written into a recalled or restored line, and go out
/// once with the next send.
#[gpui::test]
fn history_recall_leaves_the_comments_on_the_shelf(cx: &mut TestAppContext) {
    let (mut core, fake, repo) = edited("review-history");
    tidied(&mut core, &fake, &repo);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    open_review(&view, Scope::Uncommitted, cx);
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
        sent.matches("src/nav.rs:1").count(),
        1,
        "the comments go once: {sent}"
    );
    assert_eq!(sent.matches("why?").count(), 1, "{sent}");
}

/// A shift-click stretches the pick to a range; the panel's `Review
/// changes` opens the Review beside the Pane (the panel giving way to it);
/// the shelf chip's × drops every comment.
#[gpui::test]
fn shift_click_ranges_and_the_chip_drops_the_comments(cx: &mut TestAppContext) {
    let (mut core, fake, repo) = edited("review-range");
    tidied(&mut core, &fake, &repo);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    tick(cx);
    let review = cx
        .debug_bounds("panel-review-changes")
        .expect("under the panel's changed files");
    cx.simulate_click(review.center(), gpui::Modifiers::none());
    until(cx, |cx| cx.debug_bounds("review-lines").is_some());
    assert!(
        view.read_with(cx, |view, _| view.panes[0].review.review().is_some()),
        "the button opens the Review"
    );
    assert!(
        cx.debug_bounds("panel").is_some(),
        "the panel stays with the Beside slot open"
    );
    let first = cx.debug_bounds("review-line-0").expect("drawn");
    let last = cx.debug_bounds("review-line-2").expect("drawn");
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    cx.simulate_click(last.center(), gpui::Modifiers::shift());
    tick(cx);
    cx.simulate_keystrokes("c");
    cx.simulate_input("all of it");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        comments(&view, cx),
        [("src/nav.rs:1-2".into(), "all of it".into())]
    );
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

/// A right-click in the diff opens the context menu on the pick — kept
/// when the line is in it, else the line picked — and its rows act: copy
/// takes the lines' text, comment opens the field, and open reads the
/// line's file in the Panel's preview, the review staying beside the Pane.
#[gpui::test]
fn the_right_click_menu_comments_opens_and_copies(cx: &mut TestAppContext) {
    let (core, _fake, _repo) = edited("review-menu");
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    open_review(&view, Scope::Uncommitted, cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let none = gpui::Modifiers::none();
    let first = cx.debug_bounds("review-line-0").expect("drawn").center();
    let second = cx.debug_bounds("review-line-1").expect("drawn").center();
    let added = cx.debug_bounds("review-line-2").expect("drawn").center();
    let right_click = |at, cx: &mut gpui::VisualTestContext| {
        cx.simulate_mouse_down(at, gpui::MouseButton::Right, none);
        tick(cx);
    };
    // A range, then a right-click inside it: the range stays.
    cx.simulate_click(first, none);
    cx.simulate_click(second, gpui::Modifiers::shift());
    right_click(second, cx);
    let row = |verb: MenuVerb, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            let menu = view.context_menu.as_ref().expect("the menu is up");
            menu.rows
                .iter()
                .position(|row| matches!(row, Some((_, found)) if *found == verb))
                .expect("offered")
        })
    };
    view.read_with(cx, |view, _| {
        let menu = view.context_menu.as_ref().expect("a right-click opens it");
        assert_eq!(menu.target, MenuTarget::Review { thread, line: 1 });
        let labels: Vec<&str> = menu
            .rows
            .iter()
            .flatten()
            .map(|(item, _)| item.label.as_ref())
            .collect();
        assert_eq!(labels, ["Comment", "Open file at this line", "Copy"]);
    });
    let copy = row(MenuVerb::CopySelection, cx);
    view.update_in(cx, |view, window, cx| view.press_menu_row(copy, window, cx));
    assert_eq!(
        cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
        Some("fn row() {\n    let wide = 1;".into()),
        "the range's lines, no signs or numbers"
    );
    // Outside the range, the right-click picks its line first.
    right_click(added, cx);
    let comment = row(MenuVerb::Comment, cx);
    view.update_in(cx, |view, window, cx| {
        view.press_menu_row(comment, window, cx)
    });
    tick(cx);
    assert!(cx.debug_bounds("review-comment-field").is_some());
    cx.simulate_input("why 2?");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        comments(&view, cx),
        [("src/nav.rs:2".into(), "why 2?".into())]
    );
    right_click(second, cx);
    let open = row(MenuVerb::OpenAtLine, cx);
    view.update_in(cx, |view, window, cx| view.press_menu_row(open, window, cx));
    tick(cx);
    let document = view
        .read_with(cx, |view, _| view.panes[0].preview.document())
        .expect("the file opens in the preview");
    assert!(document.path.ends_with("src/nav.rs"), "{:?}", document.path);
    assert_eq!(document.line(), Some(2), "the removed line's old number");
    assert!(matches!(
        document.body,
        ferrite_pane::preview::Body::Text { .. }
    ));
    assert!(
        view.read_with(cx, |view, _| view.panes[0].review.is_open()),
        "the review stays on the board"
    );
    assert!(cx.debug_bounds("panel-preview").is_some(), "in the Panel");
}

/// A turn snapshots its checkout as it begins, reviewed or not (the
/// vendored turn tracking), and once it ends having changed something the
/// last turn reads only that: not the edit already there, nor what a
/// baseline an earlier session left would show.
#[gpui::test]
fn the_last_turn_reads_what_changed_since_the_prompt(cx: &mut TestAppContext) {
    let (core, fake, repo) = edited("review-last-turn");
    const BASE: &str = "refs/worktree/reviewr/turn-base";
    git(&repo, &["update-ref", BASE, "HEAD^{tree}"]);
    let baseline = || {
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "--quiet", BASE])
            .current_dir(&repo)
            .output()
            .expect("git");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    let stale = baseline();
    // The tree the turn's start will snapshot: the checkout as it stands,
    // written to objects of its own so the repo learns of it only from
    // the snapshot.
    let scratch = repo.parent().unwrap().join("start-tree");
    std::fs::create_dir_all(scratch.join("objects")).unwrap();
    let start = {
        let out = std::process::Command::new("sh")
            .args(["-c", "git add -A && git write-tree"])
            .current_dir(&repo)
            .env("GIT_INDEX_FILE", scratch.join("index"))
            .env("GIT_OBJECT_DIRECTORY", scratch.join("objects"))
            .env(
                "GIT_ALTERNATE_OBJECT_DIRECTORIES",
                repo.join(".git/objects"),
            )
            .output()
            .expect("git");
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    let snapshotted = || {
        std::process::Command::new("git")
            .args(["cat-file", "-e", &start])
            .current_dir(&repo)
            .status()
            .expect("git")
            .success()
    };
    assert!(!snapshotted(), "the premise: no snapshot before the turn");
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    view.update(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        view.cockpit.send(thread, "Add notes".into());
    });
    // The turn's write lands once its start is snapshotted, never in it.
    turn_host_wrote(cx, snapshotted);
    std::fs::write(repo.join("notes.md"), "fresh\n").unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    turn_host_wrote(cx, || baseline() != stale);
    assert_eq!(
        baseline(),
        start,
        "the turn's start replaced the old baseline"
    );
    open_review(&view, Scope::LastTurn, cx);
    let review = view
        .read_with(cx, |view, _| view.panes[0].review.review())
        .expect("a review is open");
    until(cx, |cx| {
        review.read_with(cx, |review, _| {
            review.place(0).map(|(path, _)| path.to_string())
        }) == Some("notes.md".into())
    });
    review.read_with(cx, |review, _| {
        assert_eq!(review.place(0), Some(("notes.md", 1)));
        assert_eq!(review.place(1), None, "only the turn's own change");
    });
}

/// The panel's file block unfolds to its hunks, read through the Review
/// core's Uncommitted scope off the UI thread: right after the click the
/// read is still out (it answers only when the executor's clock steps),
/// and the next frames draw the edit with its line of context either side.
#[gpui::test]
fn unfolding_a_panel_file_reads_its_hunks_off_the_ui_thread(cx: &mut TestAppContext) {
    use crate::cockpit::panel::hunks::{Shown, Sign};
    let (mut core, fake, repo) = edited("panel-hunks");
    tidied(&mut core, &fake, &repo);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    tick(cx);
    assert!(
        cx.debug_bounds("panel-file-0").is_some(),
        "the premise: nav.rs's block"
    );
    assert!(
        cx.debug_bounds("panel-hunks-0").is_none(),
        "folded by default"
    );
    let fold = cx
        .debug_bounds("panel-file-fold-0")
        .expect("the head's chevron");
    cx.simulate_click(fold.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let path = view.read_with(cx, |view, _| {
        view.facts.get(thread).unwrap().changed_files[0]
            .path
            .clone()
    });
    assert!(
        view.read_with(cx, |view, _| view.panel_hunks(thread, &path).is_none()),
        "the read is out on the background executor, not done in the click"
    );
    until(cx, |cx| cx.debug_bounds("panel-hunk-0-3").is_some());
    let lines = view.read_with(cx, |view, _| match view.panel_hunks(thread, &path) {
        Some(Shown::Hunks(hunks)) => hunks
            .iter()
            .flatten()
            .map(|line| {
                let sign = match line.sign {
                    Sign::Context => ' ',
                    Sign::Added => '+',
                    Sign::Removed => '-',
                };
                format!("{sign}{}", line.text)
            })
            .collect::<Vec<_>>(),
        other => panic!("hunks, not {other:?}"),
    });
    assert_eq!(
        lines,
        [
            " fn row() {",
            "-    let wide = 1;",
            "+    let wide = 2;",
            " }"
        ]
    );
    cx.simulate_click(fold.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("panel-hunks-0").is_none(), "folded again");
}
