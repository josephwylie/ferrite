//! `ChangesView` in a window of its own: what it draws, what it asks of
//! the window, and how it answers the window's calls.

use std::sync::mpsc;
use std::time::Duration;

use ferrite_core::changes::Snapshot;
use ferrite_reviewr::ferrite::FileSides;
use ferrite_reviewr::model::{ChangeKind, ChangedFile};
use ferrite_ui::testing::{click, mount, record, shows};
use gpui::{Bounds, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext, Window};

use super::*;

fn line(sign: Sign, text: &str) -> Line {
    Line {
        sign,
        text: text.to_string().into(),
    }
}

#[test]
fn a_block_too_big_to_match_keeps_its_changes() {
    let side = 1_001;
    let block: Vec<Line> = (0..side)
        .map(|n| line(Sign::Removed, &format!("line {n}")))
        .chain((0..side).map(|n| line(Sign::Added, &format!("  line {n}"))))
        .collect();
    let out = without_whitespace(vec![block.clone()]);
    assert_eq!(out, vec![block]);
}

#[test]
fn hiding_whitespace_keeps_a_swap_of_two_lines() {
    let swap = vec![
        line(Sign::Removed, "x"),
        line(Sign::Removed, "y"),
        line(Sign::Added, "y"),
        line(Sign::Added, "x"),
    ];
    let out = without_whitespace(vec![swap]);
    assert_eq!(out.len(), 1, "a swap is still a change");
    assert!(out[0].iter().any(|line| line.sign != Sign::Context));
}

#[test]
fn hiding_whitespace_keeps_the_lines_in_file_order() {
    let block = vec![
        line(Sign::Removed, "let a = 1;"),
        line(Sign::Removed, "let b = 2;"),
        line(Sign::Added, "    let a = 1;"),
    ];
    let out = without_whitespace(vec![block]);
    let shown: Vec<_> = out[0]
        .iter()
        .map(|l| (l.sign, l.text.trim().to_string()))
        .collect();
    assert_eq!(
        shown,
        [
            (Sign::Context, "let a = 1;".into()),
            (Sign::Removed, "let b = 2;".into())
        ]
    );
}

#[test]
fn hiding_whitespace_drops_blank_changes_and_the_hunks_they_leave_bare() {
    let kept = vec![line(Sign::Context, "a"), line(Sign::Added, "b")];
    let bare = vec![
        line(Sign::Context, "c"),
        line(Sign::Added, "  "),
        line(Sign::Removed, ""),
    ];
    let mixed = vec![line(Sign::Added, ""), line(Sign::Removed, "x")];
    let out = without_whitespace(vec![kept.clone(), bare, mixed]);
    assert_eq!(out, [kept, vec![line(Sign::Removed, "x")]]);
}

#[test]
fn hiding_whitespace_drops_reindented_and_trailing_space_changes() {
    let reindented = vec![
        line(Sign::Context, "fn a() {"),
        line(Sign::Removed, "    let x = 1;"),
        line(Sign::Added, "\tlet x = 1; "),
        line(Sign::Context, "}"),
    ];
    let substantive = vec![
        line(Sign::Removed, "    let y = 1;"),
        line(Sign::Added, "        let y = 2;"),
    ];
    let out = without_whitespace(vec![reindented, substantive.clone()]);
    assert_eq!(out, [substantive]);
}

#[test]
fn hiding_whitespace_keeps_the_real_edit_beside_a_reindented_line() {
    let hunk = vec![
        line(Sign::Removed, "  a"),
        line(Sign::Removed, "b"),
        line(Sign::Added, "a"),
        line(Sign::Added, "c"),
    ];
    let out = without_whitespace(vec![hunk]);
    assert_eq!(
        out,
        [vec![
            line(Sign::Context, "a"),
            line(Sign::Removed, "b"),
            line(Sign::Added, "c")
        ]]
    );
}

#[test]
fn test_files_are_told_by_their_path() {
    use super::is_test_path;
    for test in [
        "tests/a.rs",
        "src/cockpit/tests/pr.rs",
        "a_test.go",
        "test_a.py",
        "a.test.ts",
        "a.spec.js",
        "src/tests.rs",
    ] {
        assert!(is_test_path(test), "{test}");
    }
    for code in ["src/attest.rs", "contest.md", "src/testing.rs", "a.rs"] {
        assert!(!is_test_path(code), "{code}");
    }
}

#[test]
fn a_scope_says_what_it_reads() {
    assert_eq!(Scope::Uncommitted.read(), ReadScope::Uncommitted);
    assert_eq!(Scope::All.read(), ReadScope::All);
    assert_eq!(Scope::Against("dev".into()).read(), ReadScope::Against("dev".into()));
    assert_eq!(Scope::All.label(), "All changes");
}

fn thread() -> ThreadKey {
    ThreadKey::fixture(1, 7)
}

fn diff() -> Diff {
    Diff::Changes(thread())
}

/// Where the window reads the Thread's diff, as `Host` says it at first.
fn checkout() -> PathBuf {
    PathBuf::from("/work/app")
}

fn sides(path: &str, old: &str, new: &str) -> FileSides {
    FileSides {
        file: ChangedFile {
            path: path.into(),
            kind: ChangeKind::Modified,
            additions: 1,
            deletions: 1,
            previous_path: None,
            binary: false,
            old_size: 0,
            new_size: None,
        },
        sides: Ok((old.into(), new.into())),
    }
}

/// `a.rs` edited and `b.rs` new since the commit.
fn two_files() -> Vec<FileSides> {
    vec![sides("a.rs", "one\ntwo\n", "one\n2\n"), sides("b.rs", "", "new\n")]
}

/// `files` files of twenty lines, every other one edited: each taller than
/// the window.
fn tall_files(files: usize) -> Vec<FileSides> {
    let body = |file: usize, edited: bool| -> String {
        (0..20)
            .map(|n| if edited && n % 2 == 0 { format!("edited {file} {n}\n") } else { format!("line {file} {n}\n") })
            .collect()
    };
    (0..files).map(|file| sides(&format!("f{file:03}.rs"), &body(file, false), &body(file, true))).collect()
}

/// What the window keeps of Changes, as a test keeps it: where it reads
/// the diff and whether the `⋯` menu is up, drawing the diff as the root does.
struct Host {
    changes: Entity<ChangesView>,
    root: PathBuf,
    up: bool,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let inputs = ChangesInputs {
            head: Some("feature".into()),
            default_branch: Some("main".into()),
            more: MenuDoor::new(self.up, |_, _, _| {}),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.changes.update(cx, |changes, cx| changes.pane(diff(), inputs, cx)))
    }
}

/// The Thread's Changes in a window of their own, made and not yet read.
fn open(cx: &mut TestAppContext) -> (Entity<Host>, Entity<ChangesView>, &mut VisualTestContext) {
    let (host, cx) = mount(cx, |_, cx| {
        let origin = Source::of(&cx.entity(), |host: &Host, _: &Diff, _| Some(Origin::new(host.root.clone(), None)));
        let changes = cx.new(|_| ChangesView::new(origin));
        cx.observe(&changes, |_, _, cx| cx.notify()).detach();
        Host { changes, root: checkout(), up: false }
    });
    cx.simulate_resize(gpui::size(px(900.), px(700.)));
    let changes = host.read_with(cx, |host, _| host.changes.clone());
    assert!(changes.update(cx, |changes, cx| changes.make(diff(), cx)), "made now");
    assert!(!changes.update(cx, |changes, cx| changes.make(diff(), cx)), "made once");
    (host, changes, cx)
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(64));
        cx.run_until_parked();
    }
}

/// A read of the diff starts, and git's answer, read at `root`, lands.
fn answered_at(
    changes: &Entity<ChangesView>,
    root: PathBuf,
    files: Vec<FileSides>,
    branches: &[&str],
    cx: &mut VisualTestContext,
) {
    let (send, answer) = mpsc::channel();
    let branches = branches.iter().map(|branch| branch.to_string()).collect();
    send.send(Ok(Snapshot { files, branches, staged: Default::default() })).unwrap();
    changes.update(cx, |changes, cx| {
        let (generation, _) = changes.begin_read(diff(), true, cx).expect("made");
        changes.wait_local(diff(), generation, Origin::new(root, None), Ok(answer), cx);
    });
    settle(cx);
}

fn read(changes: &Entity<ChangesView>, files: Vec<FileSides>, cx: &mut VisualTestContext) {
    answered_at(changes, checkout(), files, &["main", "dev"], cx);
}

fn files_read(changes: &Entity<ChangesView>, cx: &mut VisualTestContext) -> Option<usize> {
    changes.read_with(cx, |changes, _| changes.files_read(diff()))
}

fn scope_label(changes: &Entity<ChangesView>, cx: &mut VisualTestContext) -> Option<String> {
    changes.read_with(cx, |changes, _| changes.scope_label(diff()))
}

fn bounds(selector: &str, cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    cx.update(|window, _| window.debug_bounds(selector))
}

fn file_head(at: usize, cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    bounds(&format!("changes-file-{at}"), cx)
}

/// Which of the first `files` files the last frame laid out.
fn laid_out(files: usize, cx: &mut VisualTestContext) -> Vec<usize> {
    (0..files).filter(|&at| file_head(at, cx).is_some()).collect()
}

/// A press on a file's head, clear of its buttons.
fn press_head(at: usize, cx: &mut VisualTestContext) {
    let head = file_head(at, cx).expect("its head");
    cx.simulate_click(gpui::point(head.left() + px(40.), head.center().y), Modifiers::none());
    settle(cx);
}

fn scroll(selector: &str, by: Point<Pixels>, cx: &mut VisualTestContext) {
    let at = bounds(selector, cx).unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_event(ScrollWheelEvent { position: at.center(), delta: ScrollDelta::Pixels(by), ..Default::default() });
    settle(cx);
}

/// The first file whose head is in view below the list's top, and where.
fn first_head_in_view(files: usize, cx: &mut VisualTestContext) -> (usize, Pixels) {
    let top = bounds("changes-scroll", cx).unwrap().top();
    (0..files)
        .find_map(|at| file_head(at, cx).filter(|head| head.top() >= top).map(|head| (at, head.top())))
        .expect("a head in view")
}

/// What `selector` draws as text.
fn text(selector: &str, cx: &mut VisualTestContext) -> Vec<String> {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    let drawn = cx.update(|window, _| window.debug_text(selector));
    drawn.unwrap_or_default().iter().map(|text| text.to_string()).collect()
}

fn labels<V>(rows: Vec<Option<(menu::Item, V)>>) -> Vec<(String, bool)> {
    rows.into_iter().flatten().map(|(item, _)| (item.label.to_string(), item.checked)).collect()
}

#[gpui::test]
fn the_pane_lists_each_changed_file_with_its_hunks_and_stat(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    changes.update(cx, |changes, cx| changes.begin_read(diff(), false, cx));
    settle(cx);
    assert!(text("changes", cx).iter().any(|text| text == "Reading changes\u{2026}"), "drawn while it reads");
    assert_eq!(files_read(&changes, cx), None);
    read(&changes, two_files(), cx);
    assert_eq!(files_read(&changes, cx), Some(2));
    assert_eq!(scope_label(&changes, cx).as_deref(), Some("Uncommitted changes"));
    assert!(shows(cx, "changes-file-0") && shows(cx, "changes-file-1"));
    assert!(shows(cx, "panel-hunks-0"), "the hunks under the head");
    let head = text("changes-file-0", cx);
    assert!(head.iter().any(|text| text == "a.rs") && head.iter().any(|text| text == "+1"), "{head:?}");
}

#[gpui::test]
fn marking_reviewed_folds_the_file_and_a_press_on_its_head_folds_it(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    read(&changes, two_files(), cx);
    click(cx, "changes-reviewed-0");
    settle(cx);
    assert!(!shows(cx, "panel-hunks-0"), "reviewed folds it");
    assert!(shows(cx, "panel-hunks-1"), "the other stays");
    assert!(changes.read_with(cx, |changes, _| changes.reviewed_for_test(diff(), "a.rs")));
    press_head(1, cx);
    assert!(!shows(cx, "panel-hunks-1"), "a press folds it");
    press_head(1, cx);
    assert!(shows(cx, "panel-hunks-1"), "and another opens it");
}

/// The toolbar (T3's diff toolbar): 28 tall, the scope at the left and,
/// against a branch, what it compares; 14px tools at the right, each doing
/// its verb: Hide test files leaves the tests out of the list.
#[gpui::test]
fn the_toolbar_holds_the_scope_and_the_view_tools(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    let mut files = two_files();
    files.push(sides("tests/c.rs", "", "test\n"));
    read(&changes, files, cx);
    let heard = record(cx, &changes, ChangesEvent::clone);
    let bar = bounds("changes-toolbar", cx).expect("its toolbar");
    assert_eq!(bar.size.height, px(theme::SURFACE_TOOLBAR_H));
    let scope = bounds("changes-scope", cx).unwrap();
    assert!(scope.left() - bar.left() < px(12.), "the scope at the left");
    assert!(!shows(cx, "changes-comparing"), "uncommitted compares nothing");
    for tool in ["changes-refresh", "changes-fold-all", "changes-wrap", "changes-whitespace", "changes-hide-tests"] {
        let at = bounds(tool, cx).unwrap_or_else(|| panic!("{tool}"));
        assert!(at.left() > scope.right(), "{tool} at the right");
        assert_eq!(at.size.height, px(theme::NAV_HEADING_BUTTON));
    }
    assert_eq!(laid_out(3, cx), [0, 1, 2]);
    click(cx, "changes-hide-tests");
    settle(cx);
    assert_eq!(laid_out(3, cx), [0, 1], "tests/c.rs is left out");
    click(cx, "changes-fold-all");
    settle(cx);
    assert!(!shows(cx, "panel-hunks-0") && !shows(cx, "panel-hunks-1"), "collapsed");
    click(cx, "changes-fold-all");
    settle(cx);
    assert!(shows(cx, "panel-hunks-0"), "and expanded again");
    click(cx, "changes-refresh");
    assert_eq!(heard.take(), [ChangesEvent::Read { diff: diff(), keep_shown: false }]);
    changes.update(cx, |changes, cx| changes.run_verb(diff(), Verb::All, cx));
    assert_eq!(heard.take(), [ChangesEvent::Read { diff: diff(), keep_shown: false }], "a new scope reads again");
    read(&changes, two_files(), cx);
    let comparing = text("changes-comparing", cx);
    assert!(comparing.iter().any(|text| text == "Comparing feature against"), "it says so: {comparing:?}");
    assert!(comparing.iter().any(|text| text == "main"), "the window's base: {comparing:?}");
    assert!(shows(cx, "changes-base"), "the base is a dropdown");
}

/// The scope and the base are dropdowns and `⋯` a Menu button: each asks
/// the window for its menu.
#[gpui::test]
fn the_dropdowns_and_the_more_button_ask_the_window_for_their_menus(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    read(&changes, two_files(), cx);
    let heard = record(cx, &changes, ChangesEvent::clone);
    click(cx, "changes-scope");
    assert!(
        matches!(heard.take()[..], [ChangesEvent::Menu { dropdown: Dropdown::Scope, diff: asked, .. }] if asked == diff())
    );
    click(cx, "changes-more");
    assert_eq!(heard.take(), [ChangesEvent::More { diff: diff() }]);
    changes.update(cx, |changes, cx| changes.run_verb(diff(), Verb::All, cx));
    read(&changes, two_files(), cx);
    heard.take();
    click(cx, "changes-base");
    assert!(
        matches!(heard.take()[..], [ChangesEvent::Menu { dropdown: Dropdown::Base, diff: asked, .. }] if asked == diff())
    );
}

/// A file's stage button and its `↗` ask the window to stage it and to
/// open it in Files.
#[gpui::test]
fn a_files_buttons_ask_the_window_to_stage_and_open_it(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    read(&changes, two_files(), cx);
    let heard = record(cx, &changes, ChangesEvent::clone);
    click(cx, "changes-stage-0");
    assert_eq!(heard.take(), [ChangesEvent::Stage { diff: diff(), path: "a.rs".into(), stage: true }]);
    click(cx, "changes-open-1");
    assert_eq!(heard.take(), [ChangesEvent::Open { thread: thread(), path: "b.rs".into() }]);
    assert!(shows(cx, "panel-hunks-0"), "neither press folds the file");
}

/// A stage git refused names its file and git's word; the list is read
/// again, and Refresh clears the refusal.
#[gpui::test]
fn a_refused_stage_names_its_file_and_git_and_refresh_clears_it(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    read(&changes, two_files(), cx);
    let heard = record(cx, &changes, ChangesEvent::clone);
    let (send, outcome) = mpsc::channel();
    send.send(Err(std::io::Error::other("index.lock exists"))).unwrap();
    changes.update(cx, |changes, cx| {
        let generation = changes.begin_stage(diff()).expect("made");
        changes.wait_stage(diff(), generation, Origin::new(checkout(), None), "a.rs".into(), true, Ok(outcome), cx);
    });
    settle(cx);
    assert_eq!(heard.take(), [ChangesEvent::Read { diff: diff(), keep_shown: true }], "the list is read again");
    let said = changes.read_with(cx, |changes, _| changes.stage_failure_for_test(diff())).expect("the refusal");
    assert!(said.contains("a.rs") && said.contains("index.lock"), "{said}");
    read(&changes, two_files(), cx);
    let shown = text("changes-stage-failure", cx);
    assert!(shown.iter().any(|text| text == "Couldn't stage a.rs: index.lock exists"), "{shown:?}");
    click(cx, "changes-refresh");
    assert!(changes.read_with(cx, |changes, _| changes.stage_failure_for_test(diff())).is_none());
    assert!(!shows(cx, "changes-stage-failure"));
}

#[gpui::test]
fn an_empty_uncommitted_scope_offers_the_branch_diff(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    read(&changes, Vec::new(), cx);
    assert_eq!(files_read(&changes, cx), Some(0));
    let heard = record(cx, &changes, ChangesEvent::clone);
    click(cx, "changes-view-branch");
    assert_eq!(scope_label(&changes, cx).as_deref(), Some("All changes"));
    assert_eq!(heard.take(), [ChangesEvent::Read { diff: diff(), keep_shown: false }]);
    read(&changes, Vec::new(), cx);
    assert!(!shows(cx, "changes-view-branch"), "the whole branch offers nothing more");
}

/// The menus hold what the spec names, each ticking what stands, and a
/// branch picked from `Compare against ›` is the scope.
#[gpui::test]
fn the_menus_hold_what_the_spec_names_and_tick_what_stands(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    read(&changes, two_files(), cx);
    let more = |cx: &mut VisualTestContext| changes.read_with(cx, |changes, _| labels(changes.more_rows(diff())));
    assert_eq!(
        more(cx),
        [
            ("Collapse all files".into(), false),
            ("Expand all files".into(), false),
            ("Word wrap".into(), true),
            ("Hide whitespace changes".into(), false),
            ("Hide test files".into(), false),
            ("Refresh".into(), false),
        ]
    );
    let scope = changes.read_with(cx, |changes, _| changes.scope_rows(diff()));
    assert_eq!(scope.last().cloned().flatten().map(|(_, row)| row), Some(Row::Sub(Sub)), "Compare against ›");
    assert_eq!(
        labels(scope),
        [("All changes".into(), false), ("Uncommitted changes".into(), true), ("Compare against".into(), false)]
    );
    let branches = changes.read_with(cx, |changes, _| changes.branch_rows(diff()));
    assert_eq!(
        branches.iter().map(|(item, verb)| (item.label.to_string(), *verb)).collect::<Vec<_>>(),
        [("main".into(), Verb::Compare(0)), ("dev".into(), Verb::Compare(1))]
    );
    changes.update(cx, |changes, cx| {
        changes.run_verb(diff(), Verb::Compare(1), cx);
        changes.run_verb(diff(), Verb::ToggleWrap, cx);
        changes.run_verb(diff(), Verb::ToggleTests, cx);
    });
    assert_eq!(scope_label(&changes, cx).as_deref(), Some("Compared with dev"));
    let ticked = changes.read_with(cx, |changes, _| changes.branch_rows(diff()));
    assert_eq!(ticked.iter().map(|(item, _)| item.checked).collect::<Vec<_>>(), [false, true]);
    assert_eq!(more(cx)[2], ("Word wrap".into(), false));
    assert_eq!(more(cx)[4], ("Hide test files".into(), true));
}

#[gpui::test]
fn a_file_longer_than_the_overview_cap_is_reviewable_to_its_last_line(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    let body: String = (0..300).map(|n| format!("changed {n}\n")).collect();
    read(&changes, vec![sides("a.rs", "one\ntwo\n", &body)], cx);
    let last = bounds("panel-hunk-0-301", cx).expect("the 302nd line of the file is drawn");
    assert!(last.size.width > px(0.));
    assert!(!shows(cx, "panel-hunk-0-302"), "and no more than the diff holds");
}

#[gpui::test]
fn an_unwrapped_long_line_scrolls_sideways_to_its_tail(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    let wide = format!("{}TAIL", "abcdefghij".repeat(60));
    read(&changes, vec![sides("a.rs", "one\ntwo\n", &format!("one\n{wide}\n"))], cx);
    assert!(!shows(cx, "panel-hunks-scroll-0"), "wrapped lines need no scroller");
    click(cx, "changes-wrap");
    settle(cx);
    let scroller = bounds("panel-hunks-scroll-0", cx).expect("unwrapped: a scroller");
    let line = bounds("panel-hunk-0-2", cx).expect("the long line");
    assert_eq!(line.size.height, px(theme::LH_PANEL_DIFF), "one row");
    assert!(line.size.width > scroller.size.width, "the line keeps its width");
    scroll("panel-hunks-scroll-0", gpui::point(px(-5_000.), px(0.)), cx);
    let after = bounds("panel-hunk-0-2", cx).unwrap();
    assert!(after.left() < line.left(), "scrolled sideways");
    assert!(after.right() <= scroller.right() + px(1.), "the tail is in view");
}

#[gpui::test]
fn a_reviewed_file_changed_afterwards_is_unreviewed_and_open_again(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    read(&changes, two_files(), cx);
    click(cx, "changes-reviewed-0");
    settle(cx);
    assert!(!shows(cx, "panel-hunks-0"), "reviewed folds it");
    read(&changes, two_files(), cx);
    assert!(!shows(cx, "panel-hunks-0"), "the same content stays reviewed");
    let mut files = two_files();
    files[0] = sides("a.rs", "one\ntwo\n", "one\nTWO\n");
    read(&changes, files, cx);
    assert!(shows(cx, "panel-hunks-0"), "other content shows its hunks again");
    assert!(!changes.read_with(cx, |changes, _| changes.reviewed_for_test(diff(), "a.rs")));
}

#[gpui::test]
fn the_filter_lists_only_matches_and_clearing_it_lists_all_again(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    read(&changes, tall_files(30), cx);
    assert!(shows(cx, "changes-filter"), "the filter is drawn");
    let field = changes.read_with(cx, |changes, _| changes.filter(diff()).unwrap());
    field.update(cx, |field, cx| field.set("f027".to_string(), cx));
    settle(cx);
    assert_eq!(laid_out(30, cx), [27], "only f027.rs matches");
    field.update(cx, |field, cx| field.set("nothing".to_string(), cx));
    settle(cx);
    assert!(laid_out(30, cx).is_empty());
    let note = text("changes", cx);
    assert!(note.iter().any(|text| text == "No files match"), "{note:?}");
    field.update(cx, |field, cx| field.set(String::new(), cx));
    settle(cx);
    assert_eq!(laid_out(30, cx).first(), Some(&0), "every file again, from the first");
}

#[gpui::test]
fn a_fold_keeps_the_place_in_the_list(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    read(&changes, tall_files(30), cx);
    scroll("changes-scroll", gpui::point(px(0.), px(-2_000.)), cx);
    let (at, top) = first_head_in_view(30, cx);
    assert!(at > 0, "scrolled past the first file");
    press_head(at, cx);
    assert!(!shows(cx, &format!("panel-hunks-{at}")), "folded");
    assert_eq!(file_head(at, cx).map(|head| head.top()), Some(top), "the folded head stays put");
    assert!(file_head(at + 1, cx).is_some_and(|next| next.top() < top + px(60.)), "the next file comes up under it");
    read(&changes, tall_files(30), cx);
    assert_eq!(first_head_in_view(30, cx), (at, top), "a read again keeps the place");
}

/// Only the files in view are laid out: a diff six times the size lays out
/// the same files.
#[gpui::test]
fn the_list_lays_out_the_files_in_view_whatever_the_size_of_the_diff(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    read(&changes, tall_files(20), cx);
    let small = laid_out(20, cx);
    assert!(!small.is_empty() && small.len() < 20, "the window holds fewer than 20 files: {small:?}");
    read(&changes, tall_files(120), cx);
    assert_eq!(laid_out(120, cx), small, "the same files in view, the rest not laid out");
}

/// A file revealed (a Files tab's `Show changes`) comes into view, unfolded.
#[gpui::test]
fn a_file_revealed_far_down_the_list_comes_into_view_unfolded(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    read(&changes, tall_files(30), cx);
    changes.update(cx, |changes, cx| changes.toggle_fold_for_test(diff(), "f025.rs", cx));
    assert!(changes.read_with(cx, |changes, _| changes.lists(thread(), "f025.rs")));
    assert!(!changes.read_with(cx, |changes, _| changes.lists(thread(), "f031.rs")));
    changes.update(cx, |changes, cx| changes.reveal(diff(), "f025.rs", cx));
    settle(cx);
    let list = bounds("changes-scroll", cx).unwrap();
    let head = file_head(25, cx).expect("f025.rs is laid out");
    assert!(head.top() >= list.top() && head.bottom() <= list.bottom(), "its head is in view");
    assert!(shows(cx, "panel-hunks-25"), "unfolded");
}

/// An answer read where the window no longer reads the diff (the Thread
/// moved to another checkout meanwhile) is dropped, as is a late answer
/// for an older read.
#[gpui::test]
fn an_answer_from_elsewhere_or_from_an_older_read_is_dropped(cx: &mut TestAppContext) {
    let (host, changes, cx) = open(cx);
    host.update(cx, |host, _| host.root = PathBuf::from("/work/moved"));
    read(&changes, two_files(), cx);
    assert_eq!(files_read(&changes, cx), None, "read at the old checkout: dropped");
    answered_at(&changes, PathBuf::from("/work/moved"), two_files(), &[], cx);
    assert_eq!(files_read(&changes, cx), Some(2), "read where the window reads it now");
    let (send, older) = mpsc::channel();
    send.send(Ok(Snapshot { files: Vec::new(), branches: Vec::new(), staged: Default::default() })).unwrap();
    changes.update(cx, |changes, cx| {
        let (generation, _) = changes.begin_read(diff(), true, cx).unwrap();
        changes.begin_read(diff(), true, cx).unwrap();
        changes.wait_local(diff(), generation, Origin::new(PathBuf::from("/work/moved"), None), Ok(older), cx);
    });
    settle(cx);
    assert_eq!(files_read(&changes, cx), Some(2), "an older read's answer is dropped, the list kept");
}

/// The window forgets the diffs of Threads it no longer keeps.
#[gpui::test]
fn a_thread_forgotten_takes_its_diffs(cx: &mut TestAppContext) {
    let (_, changes, cx) = open(cx);
    assert!(changes.read_with(cx, |changes, _| changes.holds(diff())));
    assert_eq!(changes.read_with(cx, |changes, _| changes.held(thread())), 1);
    changes.update(cx, |changes, _| changes.forget(|kept| kept != thread()));
    assert!(!changes.read_with(cx, |changes, _| changes.holds(diff())));
}
