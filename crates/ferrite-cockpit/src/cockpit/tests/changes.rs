//! The Changes pane (`cockpit/tools/changes.rs`): the checkout's diff as
//! files with their hunks, its scope, its ⋯ menu, reviewed marks.
use super::*;
use crate::cockpit::context_menu::Submenu;
use crate::cockpit::panel::Opener;
use crate::cockpit::surfaces::SurfaceId;
use ferrite_tools::changes::{Diff, Sub as ChangesSub};

fn git(repo: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git").args(args).current_dir(repo).output().expect("git");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A Thread whose checkout has `a.rs` edited and `b.rs` new since the commit.
fn window<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadKey, &'a mut gpui::VisualTestContext) {
    let (view, _fake, _repo, thread, cx) = window_with_agent(name, cx);
    (view, thread, cx)
}

fn window_with_agent<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, Fake, std::path::PathBuf, ThreadKey, &'a mut gpui::VisualTestContext) {
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
    core.open(core.first_machine(), Provider::Claude, WorkspaceChoice::Main { checkout: repo.clone() }).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, fake, repo, thread, cx)
}

/// Press the door and wait for the read to land.
fn opened(view: &Entity<CockpitView>, thread: ThreadKey, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| view.run_opener(thread, Opener::Changes, cx));
    wait_for_changes(view, thread, cx, |changes, diff| changes.files_read(diff).is_some());
}

#[gpui::test]
fn changes_follow_the_agents_edits_without_a_refresh(cx: &mut TestAppContext) {
    let (view, fake, repo, thread, cx) = window_with_agent("changes-follow", cx);
    opened(&view, thread, cx);
    assert_eq!(view.read_with(cx, |view, cx| view.changes.read(cx).files_read(Diff::Changes(thread))), Some(2));
    std::fs::remove_file(repo.join("b.rs")).unwrap();
    agent_worked(&view, &fake, 0, cx);
    wait_for_changes(&view, thread, cx, |changes, diff| changes.files_read(diff) == Some(1));
    assert_eq!(view.read_with(cx, |view, cx| view.changes.read(cx).files_read(Diff::Changes(thread))), Some(1));
}

/// The window's side of the Changes' menus: `⋯` and the scope dropdown
/// open their menus with the rows the spec names, a row picked runs on
/// the diff and reads it again, and a file's `↗` opens it in Files. The
/// branch's own commit (`c.rs`) is in All changes and not in the
/// uncommitted ones, so the list says which scope was read.
#[gpui::test]
fn the_menus_hold_what_the_spec_names_and_a_scope_rereads(cx: &mut TestAppContext) {
    let (view, _fake, repo, thread, cx) = window_with_agent("changes-menus", cx);
    git(&repo, &["switch", "-qc", "work"]);
    std::fs::write(repo.join("c.rs"), "committed on the branch\n").unwrap();
    git(&repo, &["add", "c.rs"]);
    git(&repo, &["commit", "-q", "-m", "c"]);
    opened(&view, thread, cx);
    let listed = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            ["a.rs", "b.rs", "c.rs"].into_iter().filter(|path| view.changes.read(cx).lists(thread, path)).collect::<Vec<_>>()
        })
    };
    assert_eq!(listed(&view, cx), ["a.rs", "b.rs"], "uncommitted: the worktree's edits");
    let names = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.context_menu()
                .map(|menu| menu.rows.iter().flatten().map(|(item, _)| (item.label.to_string(), item.checked)).collect::<Vec<_>>())
        })
    };
    let press = |selector: &'static str, cx: &mut gpui::VisualTestContext| {
        let at = cx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector}"));
        cx.simulate_click(at.center(), gpui::Modifiers::none());
        tick(cx);
    };
    press("changes-more", cx);
    assert_eq!(
        names(&view, cx).expect("the ⋯ menu is up"),
        [
            ("Collapse all files".into(), false),
            ("Expand all files".into(), false),
            ("Word wrap".into(), true),
            ("Hide whitespace changes".into(), false),
            ("Hide test files".into(), false),
            ("Refresh".into(), false),
        ]
    );
    press("changes-more", cx);
    assert_eq!(names(&view, cx), None, "a second press shuts it");
    press("changes-scope", cx);
    assert_eq!(
        names(&view, cx).expect("the scope menu is up"),
        [
            ("All changes".into(), false),
            ("Uncommitted changes".into(), true),
            ("Compare against".into(), false),
        ]
    );
    let branches = view.read_with(cx, |view, cx| {
        view.context_submenu_rows_for_test(MenuTarget::ChangesScope(Diff::Changes(thread)), Submenu::Changes(ChangesSub), cx)
    });
    assert!(!branches.is_empty(), "the checkout's branches");
    press("context-menu-row-0", cx);
    assert_eq!(
        view.read_with(cx, |view, cx| view.changes.read(cx).scope_label(Diff::Changes(thread))).as_deref(),
        Some("All changes")
    );
    wait_for_changes(&view, thread, cx, |changes, _| changes.lists(thread, "c.rs"));
    assert_eq!(listed(&view, cx), ["a.rs", "b.rs", "c.rs"], "all changes: the branch's commit too");
    assert!(cx.debug_bounds("changes-comparing").is_some(), "against the base, it says so");
    let c = (0..3)
        .find(|at| {
            let head: &'static str = Box::leak(format!("changes-file-{at}").into_boxed_str());
            cx.update(|window, _| window.debug_text(head)).is_some_and(|text| text.iter().any(|text| text == "c.rs"))
        })
        .expect("c.rs is listed");
    press(Box::leak(format!("changes-open-{c}").into_boxed_str()), cx);
    assert_eq!(view.read_with(cx, |view, _| view.active_surface(thread)), Some(SurfaceId::Files), "it opens in Files");
    let shown = view.read_with(cx, |view, cx| view.files.read(cx).active_path(thread));
    assert_eq!(shown.as_deref().and_then(|path| path.file_name()), Some(std::ffi::OsStr::new("c.rs")), "{shown:?}");
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
    let field = view.read_with(cx, |view, cx| view.changes.read(cx).filter(Diff::Changes(thread)).unwrap());
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

fn filter_text(view: &Entity<CockpitView>, thread: ThreadKey, cx: &mut gpui::VisualTestContext) -> String {
    let field = view.read_with(cx, |view, cx| view.changes.read(cx).filter(Diff::Changes(thread)).unwrap());
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
fn a_sheet_closing_over_the_filter_gives_it_back_the_keys(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-filter-sheet-closed", cx);
    opened(&view, thread, cx);
    typed_into_the_filter("b", cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    assert!(view.read_with(cx, |view, cx| view.sheet_up(cx)), "Settings is up");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!view.read_with(cx, |view, cx| view.sheet_up(cx)), "Settings closed");
    cx.simulate_input(".rs");
    tick(cx);
    assert_eq!(filter_text(&view, thread, cx), "b.rs");
    assert_eq!(composer_text(&view, cx), "");
}

/// A sheet closing over a field no longer drawn leaves the keyboard to the
/// pump, which hands it to the Composer.
#[gpui::test]
fn a_sheet_closing_after_the_filter_went_gives_the_composer_the_keys(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("changes-filter-sheet-gone", cx);
    opened(&view, thread, cx);
    typed_into_the_filter("b", cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    view.update(cx, |view, cx| view.upsert_surface(thread, crate::cockpit::surfaces::SurfaceId::Overview, cx));
    tick(cx);
    assert!(cx.debug_bounds("changes-filter").is_none(), "the filter is not drawn");
    cx.simulate_keystrokes("escape");
    tick(cx);
    cx.simulate_input("hi");
    tick(cx);
    assert_eq!(composer_text(&view, cx), "hi");
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
    assert!(cx.update(|window, cx| view.read(cx).panel_control_focused(window, cx)));
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
        view.cockpit.update(cx, |cockpit, _| cockpit.open(cockpit.first_machine(), Provider::Claude, WorkspaceChoice::Main { checkout: repo }).unwrap());
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
fn reread(view: &Entity<CockpitView>, thread: ThreadKey, cx: &mut gpui::VisualTestContext) {
    wait_for_changes(view, thread, cx, |changes, diff| changes.files_read(diff).is_some());
}

const READ_LOST_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

fn wait_for_changes(
    view: &Entity<CockpitView>,
    thread: ThreadKey,
    cx: &mut gpui::VisualTestContext,
    ready: impl Fn(&ferrite_tools::changes::ChangesView, Diff) -> bool,
) {
    // By the clock, not by a count of looks: git reading a large diff on a
    // loaded runner takes longer than any count that is quick here.
    let started = std::time::Instant::now();
    loop {
        tick(cx);
        if view.read_with(cx, |view, cx| ready(view.changes.read(cx), Diff::Changes(thread))) {
            return;
        }
        assert!(started.elapsed() < READ_LOST_AFTER, "the read never landed");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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
    view.update(cx, |view, cx| view.read_diff(Diff::Changes(thread), true, cx));
    wait_for_changes(&view, thread, cx, |changes, diff| changes.stage_failure_for_test(diff).is_some());
    assert!(cx.debug_bounds("changes-stage-failure").is_some(), "the refusal is shown");
    let said = view.read_with(cx, |view, cx| view.changes.read(cx).stage_failure_for_test(Diff::Changes(thread))).unwrap();
    assert!(said.contains("a.rs") && said.contains("index.lock"), "{said}");
    std::fs::remove_file(&lock).unwrap();
    let button = cx.debug_bounds("changes-stage-0").unwrap();
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    wait_for_changes(&view, thread, cx, |changes, diff| changes.stage_failure_for_test(diff).is_none());
    assert!(cx.debug_bounds("changes-stage-failure").is_none(), "the retry cleared it");
    let out = std::process::Command::new("git").args(["diff", "--cached", "--name-only"]).current_dir(&repo).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "a.rs");
}

/// A Thread whose checkout has `files` files of twenty lines, each with
/// its every other line edited since the commit: each file is taller than
/// the window. The checkout comes back too.
fn window_of_files<'a>(
    name: &str,
    files: usize,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadKey, std::path::PathBuf, &'a mut gpui::VisualTestContext) {
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
    core.open(core.first_machine(), Provider::Claude, WorkspaceChoice::Main { checkout: repo.clone() }).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, thread, repo, cx)
}

fn file_head(at: usize, cx: &mut gpui::VisualTestContext) -> Option<gpui::Bounds<gpui::Pixels>> {
    cx.debug_bounds(Box::leak(format!("changes-file-{at}").into_boxed_str()))
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
fn a_refresh_keeps_the_place_in_the_list(cx: &mut TestAppContext) {
    let (view, thread, _repo, cx) = window_of_files("changes-refresh-place", 30, cx);
    opened(&view, thread, cx);
    scroll_down(2_000., cx);
    let (at, top) = first_head_in_view(30, cx);
    assert!(at > 0, "scrolled past the first file");
    let refresh = cx.debug_bounds("changes-refresh").unwrap();
    cx.simulate_click(refresh.center(), gpui::Modifiers::none());
    reread(&view, thread, cx);
    tick(cx);
    assert_eq!(first_head_in_view(30, cx), (at, top), "the same file at the same place");
    view.update(cx, |view, cx| view.read_diff(Diff::Changes(thread), true, cx));
    reread(&view, thread, cx);
    tick(cx);
    assert_eq!(first_head_in_view(30, cx), (at, top), "and through the agent's reread");
}

/// A Files tab's `Show changes`, pressed, brings its file into view
/// unfolded, though it is folded far down the list.
#[gpui::test]
fn show_changes_brings_a_file_far_down_the_list_into_view(cx: &mut TestAppContext) {
    let (view, thread, repo, cx) = window_of_files("changes-show", 30, cx);
    opened(&view, thread, cx);
    view.update(cx, |view, cx| view.changes.update(cx, |changes, cx| changes.toggle_fold_for_test(Diff::Changes(thread), "f025.rs", cx)));
    view.update(cx, |view, cx| view.open_in_files(thread, repo.join("f025.rs"), None, true, cx));
    tick(cx);
    assert!(cx.debug_bounds("changes").is_none(), "Files shows in the Panel");
    let link = cx.debug_bounds("files-show-changes").expect("f025.rs's tab offers Show changes");
    cx.simulate_click(link.center(), gpui::Modifiers::none());
    tick(cx);
    tick(cx);
    let list = cx.debug_bounds("changes-scroll").unwrap();
    let head = file_head(25, cx).expect("f025.rs is laid out");
    assert!(head.top() >= list.top() && head.bottom() <= list.bottom(), "its head is in view");
    assert!(cx.debug_bounds("panel-hunks-25").is_some(), "unfolded");
}
