//! `FilesView` in a window of its own: what it draws, what it asks of the
//! window, how it answers the window's calls, and its editors writing to
//! disk. The window's part (making a Thread's Files, the Panel's row, the
//! menus, an image's lightbox) is the Cockpit's to test.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use ferrite_ui::testing::{click, mount, record, shows};
use gpui::{
    size, Bounds, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate, Modifiers,
    TestAppContext, VisualTestContext,
};

use super::*;
use crate::changes::Diff;

/// What the window keeps of Files, as a test keeps it: the Thread its
/// Panel shows, whether the tree fits beside the editor, the `⋯` menu's
/// standing and where its button was laid out, and the handles the Panel's
/// row gave the file tabs.
struct Kept {
    shown: Option<ThreadKey>,
    beside: bool,
    up: bool,
    recorded: Rc<Cell<Option<Bounds<Pixels>>>>,
    stops: HashMap<PathBuf, FocusHandle>,
    changes: Entity<ChangesView>,
}

/// The Panel's column, which Files fills under the Panel's head. Once a
/// frame it asks for the next while a tabs row is mid-ease or owes the
/// frame that checks its aim, as the Cockpit's root does.
struct Column(Entity<FilesView>);

impl Render for Column {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.0.read(cx).tabs_scrolling(cx) {
            window.request_animation_frame();
        }
        div().size_full().flex().flex_col().child(self.0.clone())
    }
}

fn thread() -> ThreadKey {
    ThreadKey::fixture(1, 7)
}

/// A checkout with a folder and two files, as the Cockpit's tests have it.
fn checkout(name: &str) -> PathBuf {
    let checkout = std::env::temp_dir().join(format!("ferrite-files-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&checkout);
    std::fs::create_dir_all(checkout.join("src")).unwrap();
    std::fs::write(checkout.join("README.md"), "r\n").unwrap();
    std::fs::write(checkout.join("src").join("lib.rs"), "l\n").unwrap();
    checkout
}

/// The production key table, as far as the actions linked here go: the
/// editor's, the Composer's and the toolkit's. The window's own commands
/// are the Cockpit's.
fn bind_keys(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let table = ferrite_ui::keymap::bindings(ferrite_ui::keymap::Platform::Mac);
        let bindings = table
            .into_iter()
            .filter_map(|(keystroke, action, context)| {
                let action = cx.build_action(action, None).ok()?;
                let context = context.map(|context| KeyBindingContextPredicate::parse(context).unwrap().into());
                Some(KeyBinding::load(&keystroke, action, context, false, None, &DummyKeyboardMapper).unwrap())
            })
            .collect::<Vec<_>>();
        cx.bind_keys(bindings);
    });
}

/// Files in a window of its own, `width` wide, showing the Thread's Files
/// in a fresh `checkout`, its tree beside the editor.
fn files_window<'a>(
    name: &str,
    width: f32,
    cx: &'a mut TestAppContext,
) -> (Entity<FilesView>, Entity<Kept>, PathBuf, &'a mut VisualTestContext) {
    cx.update(ferrite_pane::pane::init_components);
    bind_keys(cx);
    let root = checkout(name);
    let kept = cx.new(|cx| {
        let answers = cx.new(|_| ());
        let origin = Source::of(&answers, |_: &(), _: &Diff, _| None);
        Kept {
            shown: Some(thread()),
            beside: true,
            up: false,
            recorded: Rc::default(),
            stops: HashMap::new(),
            changes: cx.new(|_| ChangesView::new(origin)),
        }
    });
    let inputs = Source::of(&kept, |kept: &Kept, _: &(), _| {
        let recorded = kept.recorded.clone();
        Some(FilesInputs {
            thread: kept.shown?,
            beside: kept.beside,
            more: MenuDoor::new(kept.up, move |bounds, _, _| recorded.set(Some(bounds))),
            stops: kept.stops.clone(),
            changes: kept.changes.clone(),
        })
    });
    let (column, cx) = mount(cx, |_, cx| Column(cx.new(|_| FilesView::new(inputs))));
    let view = column.read_with(cx, |column, _| column.0.clone());
    cx.simulate_resize(size(px(width), px(800.)));
    view.update(cx, |view, cx| view.make(thread(), root.clone(), None, None, cx));
    tick(cx);
    (view, kept, root, cx)
}

fn tick(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(64));
    cx.run_until_parked();
}

fn open(view: &Entity<FilesView>, path: &Path, keep: bool, cx: &mut VisualTestContext) {
    let path = path.to_path_buf();
    view.update(cx, |view, cx| view.open(thread(), path, None, keep, true, cx));
    tick(cx);
}

fn tab_names(view: &Entity<FilesView>, cx: &mut VisualTestContext) -> Vec<(String, bool)> {
    view.read_with(cx, |view, _| view.tab_names(thread()))
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Option<Bounds<Pixels>> {
    cx.update(|window, _| window.debug_bounds(selector))
}

/// The editor of `src/lib.rs`, open in a tab of its own, and what the
/// window keeps of Files (which the test must hold: the view reads it).
fn open_editor<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<FilesView>, Entity<Kept>, PathBuf, Entity<Editor>, &'a mut VisualTestContext) {
    let (view, kept, root, cx) = files_window(name, 1200., cx);
    let file = root.join("src/lib.rs");
    open(&view, &file, true, cx);
    let editor = view.read_with(cx, |view, _| view.shown_editor(thread()).expect("a code file"));
    (view, kept, file, editor, cx)
}

/// Clicks the drawn editor, then types through the focused window as the
/// keyboard does: text as input, Enter as the bound key.
fn type_into(text: &str, cx: &mut VisualTestContext) {
    click(cx, "editor");
    tick(cx);
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            cx.simulate_keystrokes("enter");
        }
        if !line.is_empty() {
            cx.simulate_input(line);
        }
    }
    tick(cx);
}

/// Keystrokes that do not wait for background work, so a write stays in
/// flight while the user keeps typing.
fn press_without_waiting(keys: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        for key in keys.split(' ') {
            window.dispatch_keystroke(gpui::Keystroke::parse(key).unwrap(), cx);
        }
    });
}

fn pause_and_settle(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(AUTO_SAVE_AFTER);
    cx.run_until_parked();
    tick(cx);
}

fn save_state(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> SaveState {
    editor.read_with(cx, |editor, _| editor.save_state().clone())
}

// ------------------------------------------------------------ tabs

#[gpui::test]
fn a_press_previews_and_the_next_replaces_it_until_kept(cx: &mut TestAppContext) {
    let (view, _kept, root, cx) = files_window("preview", 1200., cx);
    open(&view, &root.join("src/lib.rs"), false, cx);
    assert_eq!(tab_names(&view, cx), [("lib.rs".to_string(), true)]);
    open(&view, &root.join("src/lib.rs"), true, cx);
    assert_eq!(tab_names(&view, cx), [("lib.rs".to_string(), false)], "kept");
    std::fs::write(root.join("b.rs"), "b\n").unwrap();
    std::fs::write(root.join("c.rs"), "c\n").unwrap();
    open(&view, &root.join("b.rs"), false, cx);
    open(&view, &root.join("c.rs"), false, cx);
    assert_eq!(
        tab_names(&view, cx),
        [("lib.rs".to_string(), false), ("c.rs".to_string(), true)],
        "the preview tab is replaced, the kept one stays"
    );
    assert!(shows(cx, "files-breadcrumb"));
    click(cx, "files-tab-close-1");
    assert_eq!(tab_names(&view, cx), [("lib.rs".to_string(), false)], "closed");
}

/// Each file tab is a stop of the Panel's row, as Files declares it: its
/// file, named for it and selected while it shows. The tab is drawn with
/// the handle the row hands back, so the keyboard on it is on the tab:
/// ⏎ shows its file. A tab the row gave no handle is still drawn.
#[gpui::test]
fn each_file_tab_is_drawn_as_the_stop_it_declares(cx: &mut TestAppContext) {
    let (view, kept, root, cx) = files_window("stops", 1200., cx);
    let (a, b) = (root.join("a.rs"), root.join("b.rs"));
    for file in [&a, &b] {
        std::fs::write(file, "x\n").unwrap();
        open(&view, file, true, cx);
    }
    let declared = view.read_with(cx, |view, _| view.tab_stops(thread()));
    assert_eq!(
        declared,
        [(a.clone(), Semantics::tab("a.rs", false)), (b.clone(), Semantics::tab("b.rs", true))],
        "in tab order, the shown file selected"
    );
    assert!(shows(cx, "files-tab-0"), "with no handle yet, the tab is still drawn");

    // The Panel enters them in its row and hands back each one's handle.
    let handles = cx.update(|_, cx| declared.iter().map(|(path, _)| (path.clone(), cx.focus_handle())).collect::<HashMap<_, _>>());
    kept.update(cx, |kept, cx| {
        kept.stops = handles.clone();
        cx.notify();
    });
    tick(cx);
    cx.update(|window, cx| window.focus(&handles[&a], cx));
    tick(cx);
    cx.simulate_keystrokes("enter");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
    });
    tick(cx);
    assert_eq!(view.read_with(cx, |view, _| view.active_path(thread())), Some(a.clone()), "⏎ on a.rs's stop shows it");
    assert_eq!(
        view.read_with(cx, |view, _| view.tab_stops(thread())),
        [(a.clone(), Semantics::tab("a.rs", true)), (b, Semantics::tab("b.rs", false))],
        "and its stop now says so"
    );
    assert!(cx.update(|window, _| handles[&a].is_focused(window)), "the keyboard stays on it");
}

/// Whether `selector` sits fully inside the Files tabs row.
fn inside_the_row(selector: &'static str, cx: &mut VisualTestContext) -> Option<bool> {
    let strip = bounds(cx, "files-tabs")?;
    let tab = bounds(cx, selector)?;
    Some(tab.left() >= strip.left() - px(1.) && tab.right() <= strip.right() + px(1.))
}

/// Twenty tabs in a narrow window.
fn twenty_tabs<'a>(name: &str, cx: &'a mut TestAppContext) -> (Entity<FilesView>, Entity<Kept>, &'a mut VisualTestContext) {
    let (view, kept, root, cx) = files_window(name, 700., cx);
    for at in 0..20 {
        let file = root.join(format!("file-number-{at}.rs"));
        std::fs::write(&file, "x\n").unwrap();
        open(&view, &file, true, cx);
    }
    tick(cx);
    (view, kept, cx)
}

/// A reveal in a row that is not drawn asks for no frames.
#[gpui::test]
fn a_reveal_in_a_hidden_row_asks_for_no_frames(cx: &mut TestAppContext) {
    let (view, kept, cx) = twenty_tabs("hidden-reveal", cx);
    kept.update(cx, |kept, cx| {
        kept.shown = None;
        cx.notify();
    });
    tick(cx);
    assert!(!shows(cx, "files-tabs"), "the row is hidden");
    view.update(cx, |view, cx| {
        view.reveal_tab(thread(), 3);
        cx.notify();
    });
    tick(cx);
    for _ in 0..4 {
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    let frames = cx.update(|window, cx| window.simulate_next_frame(cx));
    assert_eq!(frames, 0, "the hidden row asks for nothing");
}

/// A tab revealed from the far end lands whole inside the row, and the
/// chevron that appears beside it takes none of the row's width.
#[gpui::test]
fn a_revealed_tab_sits_whole_in_the_row_whatever_the_chevrons_do(cx: &mut TestAppContext) {
    let (view, _kept, cx) = twenty_tabs("reveal", cx);
    let left = bounds(cx, "files-tabs-left").expect("more lies to the left");
    for _ in 0..40 {
        if !shows(cx, "files-tabs-left") {
            break;
        }
        cx.simulate_click(left.center(), Modifiers::none());
        cx.executor().advance_clock(Duration::from_millis(500));
        tick(cx);
    }
    let row = bounds(cx, "files-tabs").unwrap().size.width;
    assert!(shows(cx, "files-tabs-right"), "at the start, more lies right");
    for at in [10, 18, 2] {
        view.update(cx, |view, cx| {
            view.reveal_tab(thread(), at);
            cx.notify();
        });
        tick(cx);
        tick(cx);
        let selector: &'static str = Box::leak(format!("files-tab-{at}").into_boxed_str());
        assert_eq!(inside_the_row(selector, cx), Some(true), "tab {at} lies whole in the row");
        assert_eq!(bounds(cx, "files-tabs").unwrap().size.width, row, "the row kept its width");
    }
}

/// A Thread with the same id on another machine is another Thread (ADR
/// 0045): its Files hold tabs of their own, closing one leaves this
/// Thread's, and the view draws the Files of the Thread it is given.
#[gpui::test]
fn a_files_view_keeps_two_machines_threads_apart(cx: &mut TestAppContext) {
    let (view, kept, root, cx) = files_window("two-machines", 1200., cx);
    open(&view, &root.join("src/lib.rs"), true, cx);
    let there = ThreadKey::fixture(u32::MAX, thread().thread().get());
    assert_ne!(thread(), there);
    view.update(cx, |view, cx| {
        view.make(there, root.clone(), None, None, cx);
        view.open(there, root.join("README.md"), None, true, true, cx);
    });
    tick(cx);
    let names = |at: ThreadKey, view: &Entity<FilesView>, cx: &mut VisualTestContext| view.read_with(cx, |view, _| view.tab_names(at));
    assert_eq!(names(thread(), &view, cx), [("lib.rs".to_string(), false)]);
    assert_eq!(names(there, &view, cx), [("README.md".to_string(), false)]);
    let crumbs = |cx: &mut VisualTestContext| shows(cx, "files-prose");
    assert!(!crumbs(cx), "this Thread's code file shows");
    kept.update(cx, |kept, cx| {
        kept.shown = Some(there);
        cx.notify();
    });
    tick(cx);
    assert!(crumbs(cx), "the other Thread's README reads rendered");
    view.update(cx, |view, cx| view.close_tab(there, 0, cx));
    assert!(names(there, &view, cx).is_empty());
    assert_eq!(names(thread(), &view, cx), [("lib.rs".to_string(), false)], "this Thread's stay");
}

// ------------------------------------------------------------ what it asks

/// A press on a file in the tree asks the window to open it, a second
/// press to keep it; a press on a folder opens it here. A right-click asks
/// for the row's menu where it fell, and the menu's verbs act on that row.
#[gpui::test]
fn the_tree_asks_the_window_to_open_a_file_and_for_a_rows_menu(cx: &mut TestAppContext) {
    let (view, _kept, root, cx) = files_window("tree-asks", 1200., cx);
    #[derive(Debug, PartialEq)]
    enum Asked {
        Open(ThreadKey, PathBuf, bool),
        Menu(ThreadKey, gpui::Point<Pixels>),
        Attach(ThreadKey, PathBuf, bool),
        Other,
    }
    let heard = record(cx, &view, |event: &FilesEvent| match event {
        FilesEvent::Open { thread, path, keep } => Asked::Open(*thread, path.clone(), *keep),
        FilesEvent::Menu { thread, at } => Asked::Menu(*thread, *at),
        FilesEvent::Attach { thread, path, remote } => Asked::Attach(*thread, path.clone(), *remote),
        _ => Asked::Other,
    });
    assert_eq!(view.read_with(cx, |view, _| view.row_names(thread())), ["src", "README.md"]);
    let readme = bounds(cx, "files-row-1").unwrap().center();
    cx.simulate_click(readme, Modifiers::none());
    tick(cx);
    assert_eq!(heard.take(), [Asked::Open(thread(), root.join("README.md"), false)]);
    cx.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Left,
        position: readme,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui::MouseUpEvent {
        button: gpui::MouseButton::Left,
        position: readme,
        modifiers: Modifiers::none(),
        click_count: 2,
    });
    tick(cx);
    assert_eq!(heard.take(), [Asked::Open(thread(), root.join("README.md"), true)], "a second press keeps it");
    assert!(view.read_with(cx, |view, _| view.tab_names(thread()).is_empty()), "the window opens it, not the tree");

    click(cx, "files-row-0");
    assert_eq!(view.read_with(cx, |view, _| view.row_names(thread())), ["src", "lib.rs", "README.md"], "src opens");
    assert!(heard.take().is_empty(), "a folder asks nothing");

    let lib = bounds(cx, "files-row-1").unwrap().center();
    cx.simulate_mouse_down(lib, gpui::MouseButton::Right, Modifiers::none());
    tick(cx);
    assert_eq!(heard.take(), [Asked::Menu(thread(), lib)]);
    let lib_path = root.join("src").join("lib.rs");
    assert_eq!(view.read_with(cx, |view, _| view.pressed(thread())), Some(lib_path.clone()));
    let relative = Path::new("src").join("lib.rs");
    for (verb, expect) in [
        (Verb::CopyAbsolute, lib_path.to_string_lossy().into_owned()),
        (Verb::CopyRelative, relative.to_string_lossy().into_owned()),
        (Verb::CopyName, "lib.rs".to_string()),
    ] {
        view.update(cx, |view, cx| view.run_verb(thread(), verb, cx));
        assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()), Some(expect));
    }
    view.update(cx, |view, cx| view.run_verb(thread(), Verb::Attach, cx));
    assert_eq!(heard.take(), [Asked::Attach(thread(), lib_path, false)]);
    let rows = |on_machine| {
        FilesView::row_rows(on_machine)
            .into_iter()
            .map(|row| row.map(|(item, row)| (item.label.to_string(), row)))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        rows(false),
        [
            Some(("Attach as context".into(), Row::Verb(Verb::Attach))),
            Some(("Copy".into(), Row::Sub(Sub::Copy))),
            None,
            Some(("Show in Finder".into(), Row::Reveal)),
        ]
    );
    assert_eq!(rows(true)[3], None, "a machine has no Finder");
    let copy = view.read_with(cx, |view, _| view.sub_rows(thread(), Sub::Copy));
    assert_eq!(
        copy.into_iter().map(|(item, verb)| (item.label.to_string(), verb)).collect::<Vec<_>>(),
        [
            ("Absolute path".to_string(), Verb::CopyAbsolute),
            ("Relative path".to_string(), Verb::CopyRelative),
            ("Filename".to_string(), Verb::CopyName),
        ]
    );
}

/// The filter lists the matches only; ⌘P puts the keyboard in it, and its
/// ⏎ asks to open the first.
#[gpui::test]
fn the_filter_lists_matches_and_its_enter_opens_the_first(cx: &mut TestAppContext) {
    let (view, _kept, root, cx) = files_window("filter", 1200., cx);
    let heard = record(cx, &view, |event: &FilesEvent| match event {
        FilesEvent::Open { path, keep, .. } => Some((path.clone(), *keep)),
        _ => None,
    });
    view.update(cx, |view, cx| view.set_filter_for_test(thread(), "lib", cx));
    tick(cx);
    assert_eq!(view.read_with(cx, |view, _| view.row_names(thread())), ["lib.rs"], "only lib.rs matches");
    view.update_in(cx, |view, window, cx| view.go_to_file(thread(), window, cx));
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(heard.take(), [Some((root.join("src").join("lib.rs"), false))]);
}

/// The bar's doors: `Go to file` asks for ⌘P, the tree's door hides the
/// tree and shows it again, and `⋯` asks for its menu from where it was
/// laid out, up while the window says it is.
#[gpui::test]
fn the_bars_doors_act_and_ask_by_the_pointer(cx: &mut TestAppContext) {
    let (view, kept, _root, cx) = files_window("doors", 1200., cx);
    let heard = record(cx, &view, |event: &FilesEvent| match event {
        FilesEvent::GoToFile { thread } => Some(("go", *thread)),
        FilesEvent::More { thread } => Some(("more", *thread)),
        _ => None,
    });
    click(cx, "files-go");
    assert_eq!(heard.take(), [Some(("go", thread()))]);
    for shown in [false, true] {
        click(cx, "files-tree-toggle");
        assert_eq!(shows(cx, "files-tree-column"), shown);
    }
    assert!(heard.take().is_empty(), "the tree's door asks nothing");
    let drawn = bounds(cx, "files-more").expect("the bar's more door");
    let recorded = kept.read_with(cx, |kept, _| kept.recorded.get()).expect("its bounds, recorded");
    assert!(
        recorded.size == drawn.size && drawn.contains(&recorded.center()),
        "{recorded:?} is where it was drawn, {drawn:?}"
    );
    click(cx, "files-more");
    assert_eq!(heard.take(), [Some(("more", thread()))]);
}

/// The `⋯` menu holds its five settings, checked as they stand, and each
/// takes effect.
#[gpui::test]
fn the_more_menu_holds_only_its_five_settings_and_they_take_effect(cx: &mut TestAppContext) {
    let (view, _kept, _root, cx) = files_window("more", 1200., cx);
    let rows = |view: &Entity<FilesView>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.more_rows(thread())
                .into_iter()
                .flatten()
                .map(|(item, row)| (item.label.to_string(), item.checked, row))
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(
        rows(&view, cx),
        [
            ("Show file tree".into(), true, Row::Verb(Verb::ToggleTree)),
            ("Preview tabs".into(), true, Row::Verb(Verb::TogglePreviewTabs)),
            ("Auto save".into(), true, Row::Verb(Verb::ToggleAutoSave)),
            ("Word wrap".into(), true, Row::Verb(Verb::ToggleWrap)),
            ("Tab size".into(), false, Row::Sub(Sub::TabSize)),
        ]
    );
    for verb in [Verb::ToggleTree, Verb::TogglePreviewTabs, Verb::ToggleAutoSave, Verb::ToggleWrap] {
        view.update(cx, |view, cx| view.run_verb(thread(), verb, cx));
    }
    tick(cx);
    assert!(!shows(cx, "files-tree-column"), "the tree hides");
    assert!(rows(&view, cx).iter().all(|(_, checked, _)| !checked), "each setting is off");
    assert_eq!(view.read_with(cx, |view, _| view.wraps(thread())), Some(false));
    view.update(cx, |view, cx| view.run_verb(thread(), Verb::ToggleTree, cx));
    tick(cx);
    assert!(shows(cx, "files-tree-column"));
    let sizes = view.read_with(cx, |view, _| view.sub_rows(thread(), Sub::TabSize));
    assert_eq!(sizes.iter().filter(|(item, _)| item.checked).count(), 1);
    assert_eq!(sizes.iter().find(|(item, _)| item.checked).map(|(_, verb)| *verb), Some(Verb::TabSize(4)));
    view.update(cx, |view, cx| view.run_verb(thread(), Verb::TabSize(2), cx));
    let sizes = view.read_with(cx, |view, _| view.sub_rows(thread(), Sub::TabSize));
    assert_eq!(sizes.iter().find(|(item, _)| item.checked).map(|(_, verb)| *verb), Some(Verb::TabSize(2)));
}

// ------------------------------------------------------------ the tree

#[gpui::test]
fn the_tree_reads_git_status_off_the_ui_thread(cx: &mut TestAppContext) {
    let (view, _kept, root, cx) = files_window("status", 1200., cx);
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git").args(args).current_dir(&root).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "a"]);
    let readme = root.join("README.md");
    std::fs::write(&readme, "changed\n").unwrap();
    view.update(cx, |view, cx| view.refresh(thread(), cx));
    assert_eq!(
        view.read_with(cx, |view, _| view.status_for_test(thread(), &readme)),
        None,
        "the UI thread did not wait for git"
    );
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |view, _| view.status_for_test(thread(), &readme)), Some(Status::Modified));
}

fn rows_drawn(cx: &mut VisualTestContext) -> usize {
    (0..12)
        .filter(|at| shows(cx, &format!("files-row-{at}")))
        .count()
}

#[gpui::test]
fn the_refresh_door_shows_a_file_made_after_the_tree_was_read(cx: &mut TestAppContext) {
    let (_view, _kept, root, cx) = files_window("refresh", 1200., cx);
    let before = rows_drawn(cx);
    std::fs::write(root.join("made-later.rs"), "x\n").unwrap();
    tick(cx);
    assert_eq!(rows_drawn(cx), before, "nothing reads the disk by itself");
    click(cx, "files-refresh");
    assert_eq!(rows_drawn(cx), before + 1, "the new file's row");
}

// ------------------------------------------------------------ reading

#[gpui::test]
fn prose_reads_rendered_until_its_source_is_asked_for(cx: &mut TestAppContext) {
    let (view, _kept, root, cx) = files_window("prose", 1200., cx);
    open(&view, &root.join("README.md"), false, cx);
    assert!(shows(cx, "files-prose"));
    assert!(view.read_with(cx, |view, _| view.shown_editor(thread()).is_none()));
    click(cx, "files-show-source");
    assert!(!shows(cx, "files-prose"));
    assert!(shows(cx, "editor"));
    assert!(view.read_with(cx, |view, _| view.shown_editor(thread()).is_some()));
}

#[gpui::test]
fn prose_edits_in_its_source_saves_and_reads_the_edit_rendered(cx: &mut TestAppContext) {
    let (view, _kept, root, cx) = files_window("prose-edit", 1200., cx);
    for name in ["README.md", "notes.txt"] {
        let file = root.join(name);
        std::fs::write(&file, "start\n").unwrap();
        open(&view, &file, false, cx);
        click(cx, "files-show-source");
        type_into("// hi\n", cx);
        pause_and_settle(cx);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "// hi\nstart\n", "{name} saved");
        click(cx, "files-show-rendered");
        assert!(shows(cx, "files-prose"), "{name} reads rendered again");
        let markup = view.read_with(cx, |view, _| view.rendered_markup(thread()));
        assert!(markup.is_some_and(|markup| markup.contains("hi")), "{name} shows the edit");
    }
}

#[gpui::test]
fn a_line_for_a_markdown_file_opens_its_source_there(cx: &mut TestAppContext) {
    let (view, _kept, root, cx) = files_window("prose-line", 1200., cx);
    let file = root.join("README.md");
    let text: String = (1..=120).map(|line| format!("line {line}\n")).collect();
    std::fs::write(&file, text).unwrap();
    open(&view, &file, false, cx);
    assert!(shows(cx, "files-prose"));
    view.update(cx, |view, cx| view.open(thread(), file.clone(), Some(80), false, true, cx));
    tick(cx);
    assert!(!shows(cx, "files-prose"), "the source shows the line");
    let editor = view.read_with(cx, |view, _| view.shown_editor(thread()).expect("source"));
    assert_eq!(editor.read_with(cx, |editor, _| editor.caret_line_for_test()), 80);
    click(cx, "files-show-rendered");
    assert!(shows(cx, "files-prose"));
    assert_eq!(tab_names(&view, cx).len(), 1, "the file stayed one tab");
}

// ------------------------------------------------------------ the editor

#[gpui::test]
fn typing_edits_the_file_and_auto_save_writes_it_after_a_pause(cx: &mut TestAppContext) {
    let (view, _kept, root, cx) = files_window("edit", 1200., cx);
    let file = root.join("src/lib.rs");
    open(&view, &file, false, cx);
    let editor = view.read_with(cx, |view, _| view.shown_editor(thread()).expect("a code file"));
    type_into("// hi\n", cx);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "l\n", "not yet");
    assert_eq!(tab_names(&view, cx), [("lib.rs".to_string(), false)], "typing keeps a preview tab");
    cx.executor().advance_clock(AUTO_SAVE_AFTER);
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "// hi\nl\n");
    assert!(!editor.read_with(cx, |editor, _| editor.is_dirty()));
}

#[gpui::test]
fn typing_replaces_selected_file_text_through_platform_input(cx: &mut TestAppContext) {
    let (_view, _kept, file, _editor, cx) = open_editor("replace-selection", cx);
    type_into("", cx);
    cx.simulate_keystrokes("cmd-a");
    tick(cx);
    cx.simulate_input("replacement");
    tick(cx);
    cx.simulate_keystrokes("cmd-s");
    tick(cx);
    assert_eq!(std::fs::read_to_string(file).unwrap(), "replacement");
}

#[gpui::test]
fn a_refused_write_keeps_the_buffer_and_retry_writes_it(cx: &mut TestAppContext) {
    let (view, _kept, file, editor, cx) = open_editor("refused", cx);
    editor.update(cx, |editor, _| editor.refuse_writes());
    type_into("// hi\n", cx);
    pause_and_settle(cx);
    assert!(matches!(save_state(&editor, cx), SaveState::Failed(_)));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "l\n");
    assert!(shows(cx, "editor-status"), "the failure shows");

    click(cx, "files-tab-close-0");
    pause_and_settle(cx);
    assert_eq!(tab_names(&view, cx).len(), 1, "closing keeps the unsaved tab");
    assert!(editor.read_with(cx, |e, _| e.text().contains("// hi")));

    editor.update(cx, |editor, _| editor.allow_writes());
    click(cx, "editor-retry");
    pause_and_settle(cx);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "// hi\nl\n");
    assert_eq!(save_state(&editor, cx), SaveState::Saved);
    assert!(!shows(cx, "editor-status"));
}

#[gpui::test]
fn discarding_a_failed_buffer_closes_the_tab(cx: &mut TestAppContext) {
    let (view, _kept, file, editor, cx) = open_editor("discard", cx);
    editor.update(cx, |editor, _| editor.refuse_writes());
    type_into("// hi\n", cx);
    click(cx, "files-tab-close-0");
    pause_and_settle(cx);
    assert_eq!(tab_names(&view, cx).len(), 1);
    click(cx, "editor-discard");
    assert!(tab_names(&view, cx).is_empty());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "l\n");
}

#[gpui::test]
fn closing_a_dirty_tab_writes_it_then_closes(cx: &mut TestAppContext) {
    let (view, _kept, file, _editor, cx) = open_editor("close-dirty", cx);
    type_into("// hi\n", cx);
    click(cx, "files-tab-close-0");
    tick(cx);
    assert!(tab_names(&view, cx).is_empty());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "// hi\nl\n");
}

/// A buffer with no edits of its own takes its file's new text, its caret
/// keeping its line and column; an edited one is left to its owner.
#[gpui::test]
fn a_clean_buffer_follows_its_file_and_keeps_its_caret(cx: &mut TestAppContext) {
    let (view, _kept, file, editor, cx) = open_editor("follow-caret", cx);
    std::fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();
    view.update(cx, |view, cx| view.follow_disk(thread(), cx));
    tick(cx);
    click(cx, "editor");
    cx.simulate_keystrokes("down down");
    tick(cx);
    let caret = editor.read_with(cx, |e, _| e.caret_for_test());
    assert!(caret > 0, "the caret is off the first line");
    std::fs::write(&file, "a\nBETA\ngamma\n").unwrap();
    view.update(cx, |view, cx| view.follow_disk(thread(), cx));
    tick(cx);
    assert_eq!(editor.read_with(cx, |e, _| e.text().to_string()), "a\nBETA\ngamma\n");
    assert_eq!(
        editor.read_with(cx, |e, _| e.caret_for_test()),
        caret - "lpha".len(),
        "the caret keeps its line and column when a line above it shrinks"
    );
    type_into("// mine\n", cx);
    std::fs::write(&file, "agent again\n").unwrap();
    view.update(cx, |view, cx| view.follow_disk(thread(), cx));
    tick(cx);
    assert!(editor.read_with(cx, |e, _| e.text().contains("// mine")), "an edited buffer is left to its owner");
}

#[gpui::test]
fn an_external_edit_is_not_overwritten_until_the_user_chooses(cx: &mut TestAppContext) {
    let (_view, _kept, file, editor, cx) = open_editor("external", cx);
    std::fs::write(&file, "agent\n").unwrap();
    type_into("// hi\n", cx);
    pause_and_settle(cx);
    assert_eq!(save_state(&editor, cx), SaveState::Conflict);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "agent\n", "the agent's edit survives");
    assert!(shows(cx, "editor-overwrite"));

    click(cx, "editor-reload");
    assert_eq!(editor.read_with(cx, |e, _| e.text().to_string()), "agent\n");
    assert!(!editor.read_with(cx, |e, _| e.is_dirty()));

    std::fs::write(&file, "agent again\n").unwrap();
    type_into("// mine\n", cx);
    pause_and_settle(cx);
    click(cx, "editor-overwrite");
    pause_and_settle(cx);
    assert!(std::fs::read_to_string(&file).unwrap().contains("// mine"));
    assert_eq!(save_state(&editor, cx), SaveState::Saved);
}

#[gpui::test]
fn a_failed_reload_says_so_and_keeps_the_buffer(cx: &mut TestAppContext) {
    let (_view, _kept, file, editor, cx) = open_editor("reload-fails", cx);
    std::fs::write(&file, "agent\n").unwrap();
    type_into("// hi\n", cx);
    pause_and_settle(cx);
    std::fs::remove_file(&file).unwrap();
    click(cx, "editor-reload");
    assert!(matches!(save_state(&editor, cx), SaveState::ReloadFailed(_)));
    assert!(editor.read_with(cx, |e, _| e.is_dirty()), "the buffer survives");
    assert!(!shows(cx, "editor-discard"));
    assert!(shows(cx, "editor-reload"));
}

#[gpui::test]
fn a_late_write_does_not_mark_newer_edits_saved(cx: &mut TestAppContext) {
    let (_view, _kept, file, editor, cx) = open_editor("late", cx);
    type_into("a", cx);
    press_without_waiting("cmd-s", cx);
    assert!(matches!(save_state(&editor, cx), SaveState::Saving(_)));
    press_without_waiting("b", cx);
    cx.run_until_parked();
    assert!(editor.read_with(cx, |e, _| e.is_dirty()), "the first write does not cover the second edit");
    assert_ne!(save_state(&editor, cx), SaveState::Saved);
    let first = std::fs::read_to_string(&file).unwrap();
    assert!(first.contains('a') && !first.contains('b'), "{first:?}");
    pause_and_settle(cx);
    assert!(std::fs::read_to_string(&file).unwrap().contains("ab"));
    assert_eq!(save_state(&editor, cx), SaveState::Saved);
}

#[gpui::test]
fn a_quit_writes_what_auto_save_had_not_yet(cx: &mut TestAppContext) {
    let (view, _kept, file, _editor, cx) = open_editor("quit", cx);
    type_into("// hi\n", cx);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "l\n", "not yet");
    view.update(cx, |view, cx| view.flush_for_quit(cx));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "// hi\nl\n");
}

/// A gone Thread's edit still reaches the disk, and its Files go once it
/// has; one the disk refused is still counted when leaving its machine.
#[gpui::test]
fn a_gone_threads_edit_is_kept_until_the_disk_has_it(cx: &mut TestAppContext) {
    let (view, _kept, file, editor, cx) = open_editor("gone", cx);
    editor.update(cx, |editor, cx| editor.type_text("// kept\n", cx));
    view.update(cx, |view, cx| view.forget(|_| false, cx));
    assert!(view.read_with(cx, |view, _| !view.holds(thread())), "the Thread's Files are gone");
    pause_and_settle(cx);
    assert!(std::fs::read_to_string(&file).unwrap().starts_with("// kept\n"), "the edit is on disk");
    let machine = thread().machine();
    assert_eq!(view.read_with(cx, |view, cx| view.unsaved_at_leaving(machine, cx)), None);
    view.update(cx, |view, cx| view.forget(|_| false, cx));

    let other = checkout("gone-refused");
    view.update(cx, |view, cx| {
        view.make(thread(), other.clone(), None, None, cx);
        view.open(thread(), other.join("src/lib.rs"), None, true, true, cx);
    });
    tick(cx);
    let refused = view.read_with(cx, |view, _| view.shown_editor(thread()).unwrap());
    assert_ne!(refused, editor);
    refused.update(cx, |editor, _| editor.refuse_writes());
    refused.update(cx, |editor, cx| editor.type_text("// lost\n", cx));
    view.update(cx, |view, cx| view.forget(|_| false, cx));
    pause_and_settle(cx);
    assert_eq!(view.read_with(cx, |view, cx| view.unsaved_at_leaving(machine, cx)), Some(1), "named when leaving");
    let elsewhere = ThreadKey::fixture(2, 7).machine();
    assert_eq!(view.read_with(cx, |view, cx| view.unsaved_at_leaving(elsewhere, cx)), None, "only by its own machine");
}



