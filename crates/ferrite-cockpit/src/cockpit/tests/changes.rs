//! The Changes pane (`cockpit/tools/changes.rs`): the checkout's diff as
//! files with their hunks, its scope, its ⋯ menu, reviewed marks.
use super::*;
use crate::cockpit::context_menu::Submenu;
use crate::cockpit::tools::{ChangesSub, ChangesVerb, Tool};

fn git(repo: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git").args(args).current_dir(repo).output().expect("git");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A Thread whose checkout has `a.rs` edited and `b.rs` new since the commit.
fn window<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadId, &'a mut gpui::VisualTestContext) {
    let base = scratch(name);
    let repo = repo_in(&base);
    std::fs::write(repo.join("a.rs"), "one\ntwo\n").unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "a"]);
    std::fs::write(repo.join("a.rs"), "one\n2\n").unwrap();
    std::fs::write(repo.join("b.rs"), "new\n").unwrap();
    let fake = Fake::default();
    let store = Store::open(base.join("store")).unwrap();
    let mut core = Cockpit::new(store, Box::new(fake));
    core.open(Provider::Claude, WorkspaceChoice::Main { checkout: repo }).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, thread, cx)
}

/// Press the door and wait for the read to land.
fn opened(view: &Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| view.press_tool(Tool::Changes, cx));
    for _ in 0..400 {
        tick(cx);
        if view.read_with(cx, |view, _| view.changes_read(thread).is_some()) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the read never landed");
}

#[gpui::test]
fn the_pane_lists_each_changed_file_with_its_hunks_and_stat(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-files", cx);
    opened(&view, thread, cx);
    assert_eq!(view.read_with(cx, |view, _| view.changes_read(thread)), Some(2));
    assert_eq!(
        view.read_with(cx, |view, _| view.changes_scope_label(thread)).as_deref(),
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
            view.context_menu
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .flatten()
                .map(|(item, _)| (item.label.to_string(), item.checked))
                .collect::<Vec<_>>()
        })
    };
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::ChangesMore(thread), gpui::point(px(900.), px(500.)), cx)
    });
    assert_eq!(
        names(&view, cx),
        [
            ("Collapse all files".into(), false),
            ("Expand all files".into(), false),
            ("Word wrap".into(), true),
            ("Hide whitespace changes".into(), false),
            ("Refresh".into(), false),
        ]
    );
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::ChangesScope(thread), gpui::point(px(900.), px(500.)), cx)
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
        view.context_submenu_rows_for_test(MenuTarget::ChangesScope(thread), Submenu::Changes(ChangesSub))
    });
    assert!(!branches.is_empty(), "the checkout's branches");
    view.update(cx, |view, cx| view.run_changes_verb(thread, ChangesVerb::All, cx));
    assert_eq!(
        view.read_with(cx, |view, _| view.changes_read(thread)),
        None,
        "a new scope reads again"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.changes_scope_label(thread)).as_deref(),
        Some("All changes")
    );
}
