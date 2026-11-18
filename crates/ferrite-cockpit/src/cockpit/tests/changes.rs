//! The Changes pane (`cockpit/tools/changes.rs`): the checkout's diff as
//! files with their hunks, its scope, its ⋯ menu, reviewed marks.
use super::*;
use crate::cockpit::context_menu::Submenu;
use crate::cockpit::panel::Opener;
use crate::cockpit::tools::{ChangesSub, ChangesVerb, Diff};

fn git(repo: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git").args(args).current_dir(repo).output().expect("git");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A Thread whose checkout has `a.rs` edited and `b.rs` new since the commit.
fn window<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadId, &'a mut gpui::VisualTestContext) {
    let (view, _fake, _repo, thread, cx) = window_with_agent(name, cx);
    (view, thread, cx)
}

fn window_with_agent<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, Fake, std::path::PathBuf, ThreadId, &'a mut gpui::VisualTestContext) {
    let base = scratch(name);
    let repo = repo_in(&base);
    std::fs::write(repo.join("a.rs"), "one\ntwo\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "a"]);
    std::fs::write(repo.join("a.rs"), "one\n2\n").unwrap();
    std::fs::write(repo.join("b.rs"), "new\n").unwrap();
    let fake = Fake::default();
    let store = Store::open(base.join("store")).unwrap();
    let mut core = Cockpit::new(store, Box::new(fake.clone()));
    core.open(Provider::Claude, WorkspaceChoice::Main { checkout: repo.clone() }).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, fake, repo, thread, cx)
}

/// Press the door and wait for the read to land.
fn opened(view: &Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Changes, cx));
    wait_for_changes(view, thread, cx, |view, diff| view.changes_read(diff).is_some());
}

#[gpui::test]
fn changes_follow_the_agents_edits_without_a_refresh(cx: &mut TestAppContext) {
    let (view, fake, repo, thread, cx) = window_with_agent("changes-follow", cx);
    opened(&view, thread, cx);
    assert_eq!(view.read_with(cx, |view, _| view.changes_read(Diff::Changes(thread))), Some(2));
    std::fs::remove_file(repo.join("b.rs")).unwrap();
    agent_worked(&view, &fake, 0, cx);
    wait_for_changes(&view, thread, cx, |view, diff| view.changes_read(diff) == Some(1));
    assert_eq!(view.read_with(cx, |view, _| view.changes_read(Diff::Changes(thread))), Some(1));
}

#[gpui::test]
fn the_pane_lists_each_changed_file_with_its_hunks_and_stat(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-files", cx);
    opened(&view, thread, cx);
    assert_eq!(view.read_with(cx, |view, _| view.changes_read(Diff::Changes(thread))), Some(2));
    assert_eq!(
        view.read_with(cx, |view, _| view.changes_scope_label(Diff::Changes(thread))).as_deref(),
        Some("Uncommitted changes")
    );
    assert!(cx.debug_bounds("changes-file-0").is_some());
    assert!(cx.debug_bounds("changes-file-1").is_some());
    assert!(cx.debug_bounds("panel-hunks-0").is_some(), "the hunks under the head");
}

#[gpui::test]
fn marking_reviewed_folds_the_file_and_a_press_on_its_head_folds_it(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-reviewed", cx);
    opened(&view, thread, cx);
    let mark = cx.debug_bounds("changes-reviewed-0").expect("the circle");
    cx.simulate_click(mark.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("panel-hunks-0").is_none(), "reviewed folds it");
    assert!(cx.debug_bounds("panel-hunks-1").is_some(), "the other stays");
    let head = cx.debug_bounds("changes-file-1").unwrap();
    cx.simulate_click(gpui::point(head.left(), head.top()) + gpui::point(px(40.), px(8.)), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("panel-hunks-1").is_none(), "a press folds it");
}

#[gpui::test]
fn the_menus_hold_what_the_spec_names_and_a_scope_rereads(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-menus", cx);
    opened(&view, thread, cx);
    let names = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.context_menu()
                .unwrap()
                .rows
                .iter()
                .flatten()
                .map(|(item, _)| (item.label.to_string(), item.checked))
                .collect::<Vec<_>>()
        })
    };
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::ChangesMore(Diff::Changes(thread)), gpui::point(px(900.), px(500.)), cx)
    });
    assert_eq!(
        names(&view, cx),
        [
            ("Collapse all files".into(), false),
            ("Expand all files".into(), false),
            ("Word wrap".into(), true),
            ("Hide whitespace changes".into(), false),
            ("Hide test files".into(), false),
            ("Refresh".into(), false),
        ]
    );
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::ChangesScope(Diff::Changes(thread)), gpui::point(px(900.), px(500.)), cx)
    });
    assert_eq!(
        names(&view, cx),
        [
            ("All changes".into(), false),
            ("Uncommitted changes".into(), true),
            ("Compare against".into(), false),
        ]
    );
    let branches = view.read_with(cx, |view, _| {
        view.context_submenu_rows_for_test(MenuTarget::ChangesScope(Diff::Changes(thread)), Submenu::Changes(ChangesSub))
    });
    assert!(!branches.is_empty(), "the checkout's branches");
    view.update(cx, |view, cx| view.run_changes_verb(Diff::Changes(thread), ChangesVerb::All, cx));
    assert_eq!(
        view.read_with(cx, |view, _| view.changes_read(Diff::Changes(thread))),
        None,
        "a new scope reads again"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.changes_scope_label(Diff::Changes(thread))).as_deref(),
        Some("All changes")
    );
}

/// The toolbar (T3's diff toolbar): 28 tall, the scope at the left and,
/// against a branch, what it compares; 14px tools at the right, each
/// doing its verb: Hide test files leaves the tests out of the list.
#[gpui::test]
fn the_toolbar_holds_the_scope_and_the_view_tools(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-toolbar", cx);
    let repo = std::env::temp_dir()
        .join(format!("ferrite-view-{}-changes-toolbar", std::process::id()))
        .join("repo");
    std::fs::create_dir_all(repo.join("tests")).unwrap();
    std::fs::write(repo.join("tests/c.rs"), "test\n").unwrap();
    opened(&view, thread, cx);
    assert_eq!(view.read_with(cx, |view, _| view.changes_read(Diff::Changes(thread))), Some(3));
    let bar = cx.debug_bounds("changes-toolbar").expect("its toolbar");
    assert_eq!(bar.size.height, px(ferrite_ui::theme::SURFACE_TOOLBAR_H));
    let scope = cx.debug_bounds("changes-scope").unwrap();
    assert!(scope.left() - bar.left() < px(12.), "the scope at the left");
    assert!(cx.debug_bounds("changes-comparing").is_none(), "uncommitted compares nothing");
    for tool in ["changes-refresh", "changes-fold-all", "changes-wrap", "changes-whitespace", "changes-hide-tests"] {
        let at = cx.debug_bounds(tool).unwrap_or_else(|| panic!("{tool}"));
        assert!(at.left() > scope.right(), "{tool} at the right");
        assert_eq!(at.size.height, px(ferrite_ui::theme::SURFACE_PLUS));
    }
    let shown = |cx: &mut gpui::VisualTestContext| {
        (0..3)
            .filter(|at| cx.debug_bounds(Box::leak(format!("changes-file-{at}").into_boxed_str())).is_some())
            .count()
    };
    assert_eq!(shown(cx), 3);
    let hide = cx.debug_bounds("changes-hide-tests").unwrap();
    cx.simulate_click(hide.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(shown(cx), 2, "tests/c.rs is left out");
    let fold = cx.debug_bounds("changes-fold-all").unwrap();
    cx.simulate_click(fold.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("panel-hunks-0").is_none(), "collapsed");
    view.update(cx, |view, cx| view.run_changes_verb(Diff::Changes(thread), ChangesVerb::All, cx));
    tick(cx);
    assert!(cx.debug_bounds("changes-comparing").is_some(), "against the base, it says so");
    assert!(cx.debug_bounds("changes-base").is_some(), "the base is a dropdown");
}

#[gpui::test]
fn an_empty_uncommitted_scope_offers_the_branch_diff(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-empty", cx);
    let repo = std::env::temp_dir().join(format!("ferrite-view-{}-changes-empty", std::process::id())).join("repo");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "all"]);
    opened(&view, thread, cx);
    assert_eq!(view.read_with(cx, |view, _| view.changes_read(Diff::Changes(thread))), Some(0));
    let link = cx.debug_bounds("changes-view-branch").expect("the link is drawn");
    cx.simulate_click(link.center(), gpui::Modifiers::none());
    assert_eq!(
        view.read_with(cx, |view, _| view.changes_scope_label(Diff::Changes(thread))).as_deref(),
        Some("All changes")
    );
}

#[gpui::test]
fn the_filter_narrows_the_files_listed(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-filter", cx);
    opened(&view, thread, cx);
    assert!(cx.debug_bounds("changes-file-0").is_some() && cx.debug_bounds("changes-file-1").is_some());
    let field = view.read_with(cx, |view, _| view.changes_filter(Diff::Changes(thread)).unwrap());
    view.update(cx, |_, cx| field.update(cx, |field, cx| field.set("b.rs".to_string(), cx)));
    tick(cx);
    let shown = (0..2).filter(|at| cx.debug_bounds(Box::leak(format!("changes-file-{at}").into_boxed_str())).is_some()).count();
    assert_eq!(shown, 1, "only b.rs matches");
}

#[gpui::test]
fn a_press_on_the_filter_keeps_the_keys_typed_there(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-filter-typed", cx);
    opened(&view, thread, cx);
    let filter = cx.debug_bounds("changes-filter").expect("the filter is drawn");
    cx.simulate_click(filter.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("b.rs");
    tick(cx);
    let field = view.read_with(cx, |view, _| view.changes_filter(Diff::Changes(thread)).unwrap());
    let composer = view.read_with(cx, |view, _| view.panes[0].composer.clone());
    assert_eq!(composer.read_with(cx, |composer, _| composer.text().to_string()), "");
    assert_eq!(field.read_with(cx, |field, _| field.text().to_string()), "b.rs");
}

/// The Changes filter, pressed, with `typed` typed into it.
fn typed_into_the_filter(typed: &str, cx: &mut gpui::VisualTestContext) {
    let filter = cx.debug_bounds("changes-filter").expect("the filter is drawn");
    cx.simulate_click(filter.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input(typed);
    tick(cx);
}

fn filter_text(view: &Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) -> String {
    let field = view.read_with(cx, |view, _| view.changes_filter(Diff::Changes(thread)).unwrap());
    field.read_with(cx, |field, _| field.text().to_string())
}

#[gpui::test]
fn a_sheet_opening_over_the_filter_takes_the_keyboard(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-filter-sheet", cx);
    opened(&view, thread, cx);
    typed_into_the_filter("b", cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    let sheet_has_keys = cx.update(|window, cx| {
        let view = view.read(cx);
        let focus = view.sheets.read(cx).up().expect("Settings is up").focus(cx);
        focus.contains_focused(window, cx)
    });
    assert!(sheet_has_keys);
    cx.simulate_input("x");
    tick(cx);
    assert_eq!(filter_text(&view, thread, cx), "b");
}

#[gpui::test]
fn a_menu_over_the_filter_holds_the_keys_and_gives_them_back_as_it_closes(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-filter-menu", cx);
    opened(&view, thread, cx);
    typed_into_the_filter("b", cx);
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::ChangesMore(Diff::Changes(thread)), gpui::point(px(900.), px(500.)), cx)
    });
    tick(cx);
    assert!(cx.update(|window, cx| view.read(cx).focus.is_focused(window)), "the root holds the keys");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.context_menu().is_none()));
    cx.simulate_input(".rs");
    tick(cx);
    assert_eq!(filter_text(&view, thread, cx), "b.rs");
}

#[gpui::test]
fn another_tab_shown_takes_the_keys_from_the_filter_back_to_the_composer(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-filter-tab", cx);
    opened(&view, thread, cx);
    typed_into_the_filter("b", cx);
    let overview = cx.debug_bounds("panel-tab-Overview").expect("the Overview tab");
    cx.simulate_click(overview.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("hi");
    tick(cx);
    assert_eq!(composer_text(&view, cx), "hi");
    assert_eq!(filter_text(&view, thread, cx), "b");
}

#[gpui::test]
fn f6_from_the_filter_steps_to_the_panels_controls(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-filter-f6", cx);
    opened(&view, thread, cx);
    typed_into_the_filter("b", cx);
    cx.simulate_keystrokes("f6");
    tick(cx);
    assert!(cx.update(|window, cx| view.read(cx).panel_control_focused(window)));
    cx.simulate_keystrokes("f6");
    tick(cx);
    cx.simulate_input("hi");
    tick(cx);
    assert_eq!(composer_text(&view, cx), "hi");
    assert_eq!(filter_text(&view, thread, cx), "b");
}

#[gpui::test]
fn moving_to_another_thread_takes_the_keys_from_the_filter_with_it(cx: &mut TestAppContext) {
    let (view, _fake, repo, thread, cx) = window_with_agent("changes-filter-thread", cx);
    view.update(cx, |view, cx| {
        view.cockpit.open(Provider::Claude, WorkspaceChoice::Main { checkout: repo }).unwrap();
        view.sync_panes(cx);
    });
    tick(cx);
    let other = view.read_with(cx, |view, _| {
        view.panes.iter().filter_map(|pane| pane.thread()).find(|&id| id != thread).unwrap()
    });
    view.update(cx, |view, cx| view.focus_thread(thread, cx));
    tick(cx);
    opened(&view, thread, cx);
    typed_into_the_filter("b", cx);
    view.update(cx, |view, cx| view.focus_thread(other, cx));
    tick(cx);
    cx.simulate_input("hi");
    tick(cx);
    let composer = view.read_with(cx, |view, _| view.panes[view.pane_for(other).unwrap()].composer.clone());
    assert_eq!(composer.read_with(cx, |composer, _| composer.text().to_string()), "hi");
    assert_eq!(filter_text(&view, thread, cx), "b");
}

#[gpui::test]
fn the_stage_button_moves_a_file_into_and_out_of_the_index(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-stage", cx);
    let repo = std::env::temp_dir().join(format!("ferrite-view-{}-changes-stage", std::process::id())).join("repo");
    opened(&view, thread, cx);
    let staged = |repo: &std::path::Path| {
        let out = std::process::Command::new("git").args(["diff", "--cached", "--name-only"]).current_dir(repo).output().unwrap();
        String::from_utf8_lossy(&out.stdout).lines().map(str::to_owned).collect::<Vec<_>>()
    };
    assert!(staged(&repo).is_empty());
    let button = cx.debug_bounds("changes-stage-0").expect("a stage button on the first file");
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    let started = std::time::Instant::now();
    while staged(&repo).is_empty() && started.elapsed() < READ_LOST_AFTER {
        tick(cx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(staged(&repo), ["a.rs"]);
}

fn repo_of(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("ferrite-view-{}-{name}", std::process::id())).join("repo")
}

/// Wait until the Changes read lands again after `press`.
fn reread(view: &Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) {
    wait_for_changes(view, thread, cx, |view, diff| view.changes_read(diff).is_some());
}

const READ_LOST_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

fn wait_for_changes(
    view: &Entity<CockpitView>,
    thread: ThreadId,
    cx: &mut gpui::VisualTestContext,
    ready: impl Fn(&CockpitView, Diff) -> bool,
) {
    // By the clock, not by a count of looks: git reading a large diff on a
    // loaded runner takes longer than any count that is quick here.
    let started = std::time::Instant::now();
    loop {
        tick(cx);
        if view.read_with(cx, |view, _| ready(view, Diff::Changes(thread))) {
            return;
        }
        assert!(started.elapsed() < READ_LOST_AFTER, "the read never landed");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[gpui::test]
fn a_file_longer_than_the_overview_cap_is_reviewable_to_its_last_line(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-long", cx);
    let body: String = (0..300).map(|n| format!("changed {n}\n")).collect();
    std::fs::write(repo_of("changes-long").join("a.rs"), body).unwrap();
    opened(&view, thread, cx);
    let last = cx.debug_bounds("panel-hunk-0-301").expect("the 302nd line of the first file is drawn");
    assert!(last.size.width > px(0.));
    assert!(cx.debug_bounds("panel-hunk-0-302").is_none(), "and no more than the diff holds");
}

#[gpui::test]
fn an_unwrapped_long_line_scrolls_sideways_to_its_tail(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-wide", cx);
    let wide = format!("{}TAIL", "abcdefghij".repeat(60));
    std::fs::write(repo_of("changes-wide").join("a.rs"), format!("one\n{wide}\n")).unwrap();
    opened(&view, thread, cx);
    assert!(cx.debug_bounds("panel-hunks-scroll-0").is_none(), "wrapped lines need no scroller");
    let wrap = cx.debug_bounds("changes-wrap").unwrap();
    cx.simulate_click(wrap.center(), gpui::Modifiers::none());
    tick(cx);
    let scroller = cx.debug_bounds("panel-hunks-scroll-0").expect("unwrapped: a scroller");
    let line = cx.debug_bounds("panel-hunk-0-2").expect("the long line");
    assert_eq!(line.size.height, px(ferrite_ui::theme::LH_PANEL_DIFF), "one row");
    assert!(line.size.width > scroller.size.width, "the line keeps its width");
    let before = cx.debug_bounds("panel-hunk-0-2").unwrap().left();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: scroller.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(-5_000.), px(0.))),
        ..Default::default()
    });
    cx.run_until_parked();
    tick(cx);
    let after = cx.debug_bounds("panel-hunk-0-2").unwrap();
    assert!(after.left() < before, "scrolled sideways");
    assert!(after.right() <= scroller.right() + px(1.), "the tail is in view");
}

#[gpui::test]
fn a_reviewed_file_changed_afterwards_is_unreviewed_and_open_again(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-stale", cx);
    opened(&view, thread, cx);
    let mark = cx.debug_bounds("changes-reviewed-0").unwrap();
    cx.simulate_click(mark.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("panel-hunks-0").is_none(), "reviewed folds it");
    let again = |text: &str, cx: &mut gpui::VisualTestContext| {
        std::fs::write(repo_of("changes-stale").join("a.rs"), text).unwrap();
        let refresh = cx.debug_bounds("changes-refresh").unwrap();
        cx.simulate_click(refresh.center(), gpui::Modifiers::none());
        reread(&view, thread, cx);
    };
    again("one\n2\n", cx);
    assert!(cx.debug_bounds("panel-hunks-0").is_none(), "the same content stays reviewed");
    again("one\nTWO\n", cx);
    assert!(cx.debug_bounds("panel-hunks-0").is_some(), "other content shows its hunks again");
    let state = cx.debug_bounds("changes-reviewed-0").unwrap();
    assert!(state.size.width > px(0.));
    view.read_with(cx, |view, _| assert!(!view.changes_reviewed_for_test(Diff::Changes(thread), "a.rs")));
}

#[gpui::test]
fn a_refused_stage_names_its_file_and_git_and_a_retry_stages_it(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-locked", cx);
    let repo = repo_of("changes-locked");
    opened(&view, thread, cx);
    let lock = repo.join(".git/index.lock");
    std::fs::write(&lock, "").unwrap();
    let button = cx.debug_bounds("changes-stage-0").unwrap();
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    view.update(cx, |view, cx| view.reread_changes(Diff::Changes(thread), cx));
    wait_for_changes(&view, thread, cx, |view, diff| view.changes_stage_failure_for_test(diff).is_some());
    assert!(cx.debug_bounds("changes-stage-failure").is_some(), "the refusal is shown");
    let said = view.read_with(cx, |view, _| view.changes_stage_failure_for_test(Diff::Changes(thread))).unwrap();
    assert!(said.contains("a.rs") && said.contains("index.lock"), "{said}");
    std::fs::remove_file(&lock).unwrap();
    let button = cx.debug_bounds("changes-stage-0").unwrap();
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    wait_for_changes(&view, thread, cx, |view, diff| view.changes_stage_failure_for_test(diff).is_none());
    assert!(cx.debug_bounds("changes-stage-failure").is_none(), "the retry cleared it");
    let out = std::process::Command::new("git").args(["diff", "--cached", "--name-only"]).current_dir(&repo).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "a.rs");
}

/// A Thread whose checkout has `files` files of twenty lines, each with
/// its every other line edited since the commit: each file is taller than
/// the window.
fn window_of_files<'a>(
    name: &str,
    files: usize,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadId, &'a mut gpui::VisualTestContext) {
    let base = scratch(name);
    let repo = repo_in(&base);
    let body = |file: usize, edited: bool| -> String {
        (0..20).map(|n| if edited && n % 2 == 0 { format!("edited {file} {n}\n") } else { format!("line {file} {n}\n") }).collect()
    };
    for file in 0..files {
        std::fs::write(repo.join(format!("f{file:03}.rs")), body(file, false)).unwrap();
    }
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "files"]);
    for file in 0..files {
        std::fs::write(repo.join(format!("f{file:03}.rs")), body(file, true)).unwrap();
    }
    let store = Store::open(base.join("store")).unwrap();
    let mut core = Cockpit::new(store, Box::new(Fake::default()));
    core.open(Provider::Claude, WorkspaceChoice::Main { checkout: repo.clone() }).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, thread, cx)
}

fn file_head(at: usize, cx: &mut gpui::VisualTestContext) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(Box::leak(format!("changes-file-{at}").into_boxed_str()))
}

/// Which of the first `files` files the last frame laid out.
fn laid_out(files: usize, cx: &mut gpui::VisualTestContext) -> Vec<usize> {
    (0..files).filter(|&at| file_head(at, cx).is_some()).collect()
}

fn scroll_down(by: f32, cx: &mut gpui::VisualTestContext) {
    let list = cx.debug_bounds("changes-scroll").expect("the list");
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: list.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-by))),
        ..Default::default()
    });
    cx.run_until_parked();
    tick(cx);
}

/// The first file whose head is in view below the list's top, and where.
fn first_head_in_view(files: usize, cx: &mut gpui::VisualTestContext) -> (usize, gpui::Pixels) {
    let top = cx.debug_bounds("changes-scroll").unwrap().top();
    (0..files)
        .find_map(|at| file_head(at, cx).filter(|head| head.top() >= top).map(|head| (at, head.top())))
        .expect("a head in view")
}

#[gpui::test]
fn a_fold_keeps_the_place_in_the_list(cx: &mut TestAppContext) {
    let (view, thread, cx) = window_of_files("changes-fold-place", 30, cx);
    opened(&view, thread, cx);
    scroll_down(2_000., cx);
    let (at, top) = first_head_in_view(30, cx);
    assert!(at > 0, "scrolled past the first file");
    let head = file_head(at, cx).unwrap();
    cx.simulate_click(gpui::point(head.left() + px(40.), head.center().y), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds(Box::leak(format!("panel-hunks-{at}").into_boxed_str())).is_none(), "folded");
    assert_eq!(file_head(at, cx).map(|head| head.top()), Some(top), "the folded head stays put");
    assert!(file_head(at + 1, cx).is_some_and(|next| next.top() < top + px(60.)), "the next file comes up under it");
}

#[gpui::test]
fn a_refresh_keeps_the_place_in_the_list(cx: &mut TestAppContext) {
    let (view, thread, cx) = window_of_files("changes-refresh-place", 30, cx);
    opened(&view, thread, cx);
    scroll_down(2_000., cx);
    let (at, top) = first_head_in_view(30, cx);
    assert!(at > 0, "scrolled past the first file");
    let refresh = cx.debug_bounds("changes-refresh").unwrap();
    cx.simulate_click(refresh.center(), gpui::Modifiers::none());
    reread(&view, thread, cx);
    tick(cx);
    assert_eq!(first_head_in_view(30, cx), (at, top), "the same file at the same place");
    view.update(cx, |view, cx| view.reread_changes(Diff::Changes(thread), cx));
    reread(&view, thread, cx);
    tick(cx);
    assert_eq!(first_head_in_view(30, cx), (at, top), "and through the agent's reread");
}

#[gpui::test]
fn the_filter_lists_only_matches_and_clearing_it_lists_all_again(cx: &mut TestAppContext) {
    let (view, thread, cx) = window_of_files("changes-filter-many", 30, cx);
    opened(&view, thread, cx);
    let field = view.read_with(cx, |view, _| view.changes_filter(Diff::Changes(thread)).unwrap());
    view.update(cx, |_, cx| field.update(cx, |field, cx| field.set("f027".to_string(), cx)));
    tick(cx);
    assert_eq!(laid_out(30, cx), [27], "only f027.rs matches");
    view.update(cx, |_, cx| field.update(cx, |field, cx| field.set(String::new(), cx)));
    tick(cx);
    assert_eq!(laid_out(30, cx).first(), Some(&0), "every file again, from the first");
}

/// Only the files in view are laid out: a diff six times the size lays
/// out the same files.
#[gpui::test]
fn the_list_lays_out_the_files_in_view_whatever_the_size_of_the_diff(cx: &mut TestAppContext) {
    let shown = |name: &str, files: usize, cx: &mut TestAppContext| {
        let (view, thread, cx) = window_of_files(name, files, cx);
        opened(&view, thread, cx);
        laid_out(files, cx)
    };
    let small = shown("changes-in-view-20", 20, cx);
    let large = shown("changes-in-view-120", 120, cx);
    assert!(!small.is_empty() && small.len() < 20, "the window holds fewer than 20 files: {small:?}");
    assert_eq!(large, small, "the same files in view, the rest not laid out");
}

/// A Files tab's `Show changes` brings its file into view, unfolded.
#[gpui::test]
fn show_changes_brings_a_file_far_down_the_list_into_view(cx: &mut TestAppContext) {
    let (view, thread, cx) = window_of_files("changes-show", 30, cx);
    opened(&view, thread, cx);
    view.update(cx, |view, cx| view.toggle_fold_for_test(Diff::Changes(thread), "f025.rs", cx));
    view.update(cx, |view, cx| view.show_changes_of(thread, "f025.rs", cx));
    tick(cx);
    tick(cx);
    let list = cx.debug_bounds("changes-scroll").unwrap();
    let head = file_head(25, cx).expect("f025.rs is laid out");
    assert!(head.top() >= list.top() && head.bottom() <= list.bottom(), "its head is in view");
    assert!(cx.debug_bounds("panel-hunks-25").is_some(), "unfolded");
}
