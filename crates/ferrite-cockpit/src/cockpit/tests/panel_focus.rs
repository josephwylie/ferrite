//! The Panel's controls are reached and worked from the keyboard alone.
//! Every key goes through the production binding table, and every control
//! is the one the head draws; the accessibility tree is not built without
//! assistive technology, so names, roles and selection are read from the
//! stops' own `Semantics`, the values the tree is given.
use super::*;
use crate::cockpit::panel::{PanelControl, Semantics};
use crate::cockpit::surfaces::SurfaceId;
use gpui::Role;

const SENTINEL: &str = "an unsent draft";

pub(super) fn focused_control(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> Option<PanelControl> {
    cx.update(|window, cx| view.read(cx).panel.focus.focused(window))
}

pub(super) fn composer_focused(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> bool {
    cx.update(|window, cx| {
        let view = view.read(cx);
        view.panes[0]
            .read(cx).composer
            .read(cx)
            .focus_target(window, cx)
            .is_focused(window)
    })
}

pub(super) fn press(keys: &str, cx: &mut gpui::VisualTestContext) {
    cx.simulate_keystrokes(keys);
    tick(cx);
}

/// A key held and released, the way a keyboard sends it. The test window
/// delivers only the press, and the click a control makes from ⏎ or space
/// is made on the release.
pub(super) fn activate(key: &str, cx: &mut gpui::VisualTestContext) {
    press(key, cx);
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse(key).unwrap(),
    });
    tick(cx);
}

/// A window on a checkout with Files and Changes open beside Overview, the
/// draft in the composer.
fn with_tabs<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    ThreadKey,
    &'a mut gpui::VisualTestContext,
) {
    let (core, _fake, _checkout) = bound_cockpit(name, Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        view.upsert_surface(thread, SurfaceId::Files, cx);
        view.upsert_surface(thread, SurfaceId::Changes, cx);
        view.panes[0]
            .composer(cx)
            .update(cx, |composer, cx| composer.set(SENTINEL.into(), cx));
    });
    tick(cx);
    (view, thread, cx)
}

#[gpui::test]
fn f6_enters_the_panel_and_arrows_rove_its_controls(cx: &mut TestAppContext) {
    let (view, thread, cx) = with_tabs("focus-rove", cx);
    assert!(
        composer_focused(&view, cx),
        "the conversation starts with the keyboard"
    );

    press("f6", cx);
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Tab(SurfaceId::Changes)),
        "F6 arrives at the shown tab"
    );
    press("left", cx);
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Tab(SurfaceId::Files))
    );
    press("home", cx);
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Tab(SurfaceId::Overview))
    );
    press("left", cx);
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Tab(SurfaceId::Overview)),
        "the row stops at its start"
    );
    press("end", cx);
    assert_eq!(focused_control(&view, cx), Some(PanelControl::Maximize));
    press("left", cx);
    assert_eq!(focused_control(&view, cx), Some(PanelControl::Plus));

    let drawn = view.read_with(cx, |view, _| view.panel.focus.drawn());
    let shown = drawn
        .iter()
        .find(|(control, _)| *control == PanelControl::Tab(SurfaceId::Changes))
        .map(|(_, semantics)| semantics.clone());
    assert_eq!(shown, Some(Semantics::tab("Changes", true)));
    let other = drawn
        .iter()
        .find(|(control, _)| *control == PanelControl::Tab(SurfaceId::Files))
        .map(|(_, semantics)| semantics.clone());
    assert_eq!(other, Some(Semantics::tab("Files", false)));
    assert!(drawn
        .iter()
        .any(|(control, semantics)| *control == PanelControl::Plus
            && semantics.role == Role::Button
            && semantics.name == "Open a surface"));
    assert_eq!(
        panel_tabs(&view, thread, cx).1,
        Some(SurfaceId::Changes),
        "moving the focus shows nothing new"
    );
    assert_eq!(
        composer_text(&view, cx),
        SENTINEL,
        "no key reached the draft"
    );
}

#[gpui::test]
fn enter_presses_a_control_and_escape_gives_the_keyboard_back(cx: &mut TestAppContext) {
    let (view, thread, cx) = with_tabs("focus-press", cx);
    press("f6", cx);
    press("home", cx);
    activate("enter", cx);
    assert_eq!(
        panel_tabs(&view, thread, cx).1,
        Some(SurfaceId::Overview),
        "⏎ shows the focused tab"
    );
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Tab(SurfaceId::Overview))
    );

    press("end", cx);
    activate("enter", cx);
    assert!(
        view.read_with(cx, |view, _| view
            .panel
            .surfaces
            .get(thread)
            .unwrap()
            .maximized),
        "⏎ maximizes"
    );
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Maximize),
        "and the keyboard stays on it"
    );

    press("escape", cx);
    assert_eq!(focused_control(&view, cx), None);
    assert!(
        composer_focused(&view, cx),
        "esc returns to the conversation"
    );
    press("f6", cx);
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Maximize),
        "F6 comes back to the control it left"
    );
    press("tab", cx);
    assert!(composer_focused(&view, cx), "⇥ leaves for the next area");
    assert_eq!(composer_text(&view, cx), SENTINEL);
}

/// More tabs than the strip shows: the arrow keys bring each stop they land
/// on into view, without a chevron's press.
#[gpui::test]
fn arrowing_along_overflowing_tabs_scrolls_each_stop_into_view(cx: &mut TestAppContext) {
    let (view, thread, cx) = with_tabs("focus-overflow", cx);
    view.update(cx, |view, cx| {
        for n in 0..14 {
            let path = std::path::PathBuf::from(format!("/work/a-long-file-name-{n}.rs"));
            view.upsert_surface(thread, SurfaceId::File(path), cx);
        }
    });
    tick(cx);
    let shown = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
        let stop = cx.debug_bounds(selector).expect("the stop is drawn");
        let strip = view.read_with(cx, |view, _| view.panel.tabs_scroll.bounds());
        stop.left() >= strip.left() - px(1.) && stop.right() <= strip.right() + px(1.)
    };
    let settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..12 {
            cx.executor().advance_clock(std::time::Duration::from_millis(50));
            view.update(cx, |_, cx| cx.notify());
            tick(cx);
        }
    };
    press("f6", cx);
    press("home", cx);
    settle(cx);
    press("end left left", cx);
    settle(cx);
    assert!(
        shown(cx, "panel-tab-File-a-long-file-name-13.rs"),
        "the last tab, at the strip's far end, is in view"
    );
    press("right", cx);
    settle(cx);
    assert_eq!(focused_control(&view, cx), Some(PanelControl::Plus));
    let plus = cx.debug_bounds("panel-plus").expect("the + is drawn");
    let maximize = cx.debug_bounds("panel-maximize").unwrap();
    let strip = view.read_with(cx, |view, _| view.panel.tabs_scroll.bounds());
    assert!(
        plus.left() >= strip.right() && plus.right() <= maximize.left(),
        "the + is in view past the strip"
    );
    press("home", cx);
    settle(cx);
    let slid = view.read_with(cx, |view, _| view.panel.tabs_scroll.offset().x);
    assert_eq!(slid, px(0.), "and the first tab is back at the strip's head");
}

#[gpui::test]
fn closing_a_tab_by_key_never_parks_its_thread(cx: &mut TestAppContext) {
    let (view, thread, cx) = with_tabs("focus-close", cx);
    press("f6", cx);
    press("left", cx);
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Tab(SurfaceId::Files))
    );
    press("delete", cx);
    assert_eq!(
        panel_tabs(&view, thread, cx).0,
        vec![SurfaceId::Overview, SurfaceId::Changes],
        "⌦ closes the focused tab"
    );
    let landed = focused_control(&view, cx);
    assert!(
        matches!(landed, Some(PanelControl::Tab(_))),
        "the keyboard lands on a tab, not nowhere: {landed:?}"
    );

    press("cmd-w", cx);
    let (tabs, _) = panel_tabs(&view, thread, cx);
    assert_eq!(tabs.len(), 1, "⌘W closes one more tab");
    assert!(
        view.read_with(cx, |view, cx| view.cockpit.read(cx).thread(thread).is_some()),
        "the Thread is still open"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.panes.len()),
        1,
        "and its Pane is still on the board"
    );
    press("cmd-w", cx);
    assert!(
        view.read_with(cx, |view, cx| view.cockpit.read(cx).thread(thread).is_some()
            && view.panes.len() == 1),
        "an emptied Panel's last close does not park the Thread"
    );
    assert_eq!(composer_text(&view, cx), SENTINEL);
}

#[gpui::test]
fn a_menu_from_a_control_gives_the_keyboard_back_when_it_closes(cx: &mut TestAppContext) {
    let (view, _thread, cx) = with_tabs("focus-menu", cx);
    press("f6", cx);
    press("end", cx);
    press("left", cx);
    assert_eq!(focused_control(&view, cx), Some(PanelControl::Plus));
    activate("enter", cx);
    assert!(
        cx.debug_bounds("context-menu").is_some()
            || view.read_with(cx, |view, _| view.context_menu().is_some())
    );
    press("escape", cx);
    assert!(
        view.read_with(cx, |view, _| view.context_menu().is_none()),
        "esc closes the menu first"
    );
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Plus),
        "the keyboard is back on `+`"
    );

    press("shift-f10", cx);
    assert!(
        view.read_with(cx, |view, _| view.context_menu().is_some()),
        "⇧F10 opens the stop's menu"
    );
    press("t", cx);
    assert!(view.read_with(cx, |view, _| view.context_menu().is_none()));
    assert_eq!(
        composer_text(&view, cx),
        SENTINEL,
        "the letter ran its row and typed nothing"
    );
}

/// The keyboard on Thread 1's `+`, ⌘2 shows Thread 2 with the keyboard in
/// its Composer, as a press on its nav row does. Thread 2 never had its
/// Panel entered, so F6 arrives at its shown tab; Thread 1 still returns
/// to its `+`, and ⌘[ hands the keyboard on as ⌘2 does.
#[gpui::test]
fn a_thread_switch_by_key_leaves_the_keyboard_to_the_new_composer(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, _fake) = cockpit("focus-thread-switch", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    view.update(cx, |view, cx| {
        let first = view.thread_order(cx)[0];
        view.focus_thread(first, cx);
    });
    tick(cx);
    let shown_composer_focused = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            let view = view.read(cx);
            view.panes[view.focused(cx)]
                .read(cx).composer
                .read(cx)
                .focus_target(window, cx)
                .is_focused(window)
        })
    };

    press("f6", cx);
    press("end left", cx);
    assert_eq!(focused_control(&view, cx), Some(PanelControl::Plus));

    press("cmd-2", cx);
    assert_eq!(focused_control(&view, cx), None, "no stop holds the keyboard");
    assert!(
        shown_composer_focused(cx),
        "the shown Thread's Composer has the keyboard"
    );
    press("f6", cx);
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Tab(SurfaceId::Overview)),
        "F6 arrives at the shown tab of a Panel never entered"
    );

    press("escape", cx);
    press("cmd-1", cx);
    press("f6", cx);
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Plus),
        "Thread 1 still returns to its own stop"
    );

    press("cmd-[", cx);
    assert_eq!(focused_control(&view, cx), None, "going back hands it on too");
    assert!(shown_composer_focused(cx));
}

/// The keyboard on Thread 1's `+`, a Focus the Server asks for
/// (`ferrite thread focus 2`) shows Thread 2 with the keyboard in its
/// Composer, as ⌘2 does: Thread 2 never had its Panel entered, so F6
/// arrives at its shown tab.
#[gpui::test]
fn a_focus_the_server_asks_for_leaves_the_keyboard_to_the_new_composer(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (core, _fake) = cockpit("focus-asked-switch", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    let second = view.update(cx, |view, cx| {
        let order = view.thread_order(cx);
        view.focus_thread(order[0], cx);
        order[1]
    });
    tick(cx);
    press("f6", cx);
    press("end left", cx);
    assert_eq!(focused_control(&view, cx), Some(PanelControl::Plus));

    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_task_alert(ferrite_core::protocol::ServerMessage::Focus {
                thread: second.thread(),
                fork_of: None,
            }));
    });
    tick(cx);
    let (shown, composer_focused) = cx.update(|window, cx| {
        let view = view.read(cx);
        let pane = &view.panes[view.focused(cx)];
        (
            pane.thread(),
            pane.read(cx).composer.read(cx).focus_target(window, cx).is_focused(window),
        )
    });
    assert_eq!(shown, Some(second));
    assert_eq!(focused_control(&view, cx), None, "no stop holds the keyboard");
    assert!(composer_focused, "the shown Thread's Composer has the keyboard");
    press("f6", cx);
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Tab(SurfaceId::Overview)),
        "F6 arrives at the shown tab of a Panel never entered"
    );
}

#[gpui::test]
fn a_hidden_panel_keeps_no_focus(cx: &mut TestAppContext) {
    let (view, _thread, cx) = with_tabs("focus-hidden", cx);
    press("f6", cx);
    assert!(focused_control(&view, cx).is_some());
    press("cmd-alt-b", cx);
    assert_eq!(
        focused_control(&view, cx),
        None,
        "a closed Panel's control holds nothing"
    );
    assert!(
        composer_focused(&view, cx),
        "the keyboard is the conversation's"
    );
    press("f6", cx);
    assert!(composer_focused(&view, cx), "F6 finds no Panel to enter");
}

const ONLY_IN_THE_FILE: &str = "zebra_sentinel";

/// A window on a checkout whose `src/lib.rs` holds a word only the file
/// has, far below its first screenful.
fn with_marked_file<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    ThreadKey,
    std::path::PathBuf,
    &'a mut gpui::VisualTestContext,
) {
    let (core, _fake, checkout) = bound_cockpit(name, Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let file = checkout.join("src/lib.rs");
    let mut source = "// filler\n".repeat(150);
    source.push_str(&format!("const MARK: &str = \"{ONLY_IN_THE_FILE}\";\n"));
    std::fs::write(&file, source).unwrap();
    (view, thread, file, cx)
}

fn file_find_count(
    editor: &Entity<crate::cockpit::tools::Editor>,
    cx: &mut gpui::VisualTestContext,
) -> Option<String> {
    editor.read_with(cx, |editor, _| editor.find_count_for_test())
}

fn transcript_find_open(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> bool {
    let transcript = view.read_with(cx, |view, cx| view.panes[0].read(cx).transcript().unwrap());
    transcript.read_with(cx, |transcript, _| transcript.find_state().is_some())
}

/// ⌘F with the keyboard on the Panel's controls over Files searches the
/// file, not the conversation.
#[gpui::test]
fn find_in_files_searches_the_file_not_the_transcript(cx: &mut TestAppContext) {
    let (view, thread, file, cx) = with_marked_file("focus-file-find", cx);
    view.update(cx, |view, cx| view.open_in_files(thread, file, None, true, cx));
    tick(cx);
    let editor = view.read_with(cx, |view, cx| {
        view.files.read(cx).shown_editor(thread).expect("a code file")
    });

    press("f6", cx);
    assert!(
        focused_control(&view, cx).is_some(),
        "the keyboard is on the Panel's controls"
    );
    press("cmd-f", cx);
    assert!(
        cx.debug_bounds("find-bar").is_some(),
        "⌘F opens the bar over the file"
    );
    cx.simulate_input(ONLY_IN_THE_FILE);
    tick(cx);
    assert_eq!(file_find_count(&editor, cx).as_deref(), Some("1/1"));

    press("escape", cx);
    assert!(cx.debug_bounds("find-bar").is_none(), "esc closes the bar");
    assert!(
        cx.update(|window, cx| editor.read(cx).holds_keyboard(window, cx)),
        "the keyboard is back in the file"
    );
}

/// The first find in a long code file brings its hit into view, as ⏎ does
/// for the ones after it.
#[gpui::test]
fn the_first_find_hit_in_a_long_file_is_in_view(cx: &mut TestAppContext) {
    let (view, thread, file, cx) = with_marked_file("focus-file-first-hit", cx);
    let mut source = "// filler\n".repeat(600);
    source.push_str(&format!("const MARK: &str = \"{ONLY_IN_THE_FILE}\";\n"));
    source.push_str(&"// after\n".repeat(100));
    std::fs::write(&file, source).unwrap();
    view.update(cx, |view, cx| view.open_in_files(thread, file, None, true, cx));
    tick(cx);
    let editor = view.read_with(cx, |view, cx| {
        view.files.read(cx).shown_editor(thread).expect("a code file")
    });

    press("f6", cx);
    press("cmd-f", cx);
    cx.simulate_input(ONLY_IN_THE_FILE);
    for _ in 0..3 {
        tick(cx);
    }
    assert_eq!(file_find_count(&editor, cx).as_deref(), Some("1/1"));
    assert!(
        editor.read_with(cx, |editor, _| editor.line_in_view_for_test(601)),
        "the first hit is on screen"
    );
}

/// A hit past a stretch the view never drew still lands on screen: read to
/// the end and back first, so only the file's two ends have been laid out.
#[gpui::test]
fn a_find_hit_past_unseen_lines_is_in_view(cx: &mut TestAppContext) {
    let (view, thread, file, cx) = with_marked_file("focus-file-unseen-hit", cx);
    let mut source = "// filler\n".repeat(690);
    source.push_str(&format!("const MARK: &str = \"{ONLY_IN_THE_FILE}\";\n"));
    source.push_str(&"// after\n".repeat(10));
    std::fs::write(&file, source).unwrap();
    view.update(cx, |view, cx| view.open_in_files(thread, file, None, true, cx));
    tick(cx);
    let editor = view.read_with(cx, |view, cx| {
        view.files.read(cx).shown_editor(thread).expect("a code file")
    });

    press("f6", cx);
    for keys in ["cmd-down", "cmd-up"] {
        press(keys, cx);
        for _ in 0..3 {
            tick(cx);
        }
    }
    press("cmd-f", cx);
    cx.simulate_input(ONLY_IN_THE_FILE);
    for _ in 0..3 {
        tick(cx);
    }
    assert_eq!(file_find_count(&editor, cx).as_deref(), Some("1/1"));
    assert!(
        editor.read_with(cx, |editor, _| editor.line_in_view_for_test(691)),
        "the hit is on screen"
    );
}

/// ⌘↓ and ⌘↑ with the keyboard on the Panel's controls read the open code
/// file to its end and back to its start.
#[gpui::test]
fn command_arrows_go_to_the_ends_of_a_code_file(cx: &mut TestAppContext) {
    let (view, thread, file, cx) = with_marked_file("focus-code-ends", cx);
    std::fs::write(&file, "// filler\n".repeat(600)).unwrap();
    view.update(cx, |view, cx| view.open_in_files(thread, file, None, true, cx));
    tick(cx);
    let editor = view.read_with(cx, |view, cx| {
        view.files.read(cx).shown_editor(thread).expect("a code file")
    });
    press("f6", cx);
    let top = |cx: &mut gpui::VisualTestContext| editor.read_with(cx, |e, _| e.top_line_for_test());
    assert_eq!(top(cx), 1);
    press("cmd-down", cx);
    for _ in 0..3 {
        tick(cx);
    }
    assert!(
        editor.read_with(cx, |editor, _| editor.line_in_view_for_test(600)),
        "⌘↓ reads to the end"
    );
    press("cmd-up", cx);
    for _ in 0..3 {
        tick(cx);
    }
    assert_eq!(top(cx), 1, "⌘↑ goes back to the start");
}

/// ⌘F over a prose file (a README) searches that document: the bar opens
/// over it, the conversation's stays shut, and the first hit is in view.
#[gpui::test]
fn find_in_a_prose_file_searches_the_document_and_shows_the_hit(cx: &mut TestAppContext) {
    let (view, thread, _file, cx) = with_marked_file("focus-prose-find", cx);
    let readme = view.read_with(cx, |view, cx| view.thread_path(thread, cx).unwrap()).join("README.md");
    let pad = "A paragraph that only pads the document.\n\n".repeat(80);
    std::fs::write(&readme, format!("# Notes\n\n{pad}The {ONLY_IN_THE_FILE} is here.\n")).unwrap();
    view.update(cx, |view, cx| view.open_in_files(thread, readme, None, true, cx));
    tick(cx);
    assert!(cx.debug_bounds("files-prose").is_some(), "the README reads as prose");

    press("f6", cx);
    press("cmd-f", cx);
    assert!(cx.debug_bounds("find-bar").is_some(), "⌘F opens the bar over the document");
    assert!(!transcript_find_open(&view, cx), "the conversation's bar stays shut");
    cx.simulate_input(ONLY_IN_THE_FILE);
    for _ in 0..6 {
        view.update(cx, |_, cx| cx.notify());
        tick(cx);
    }
    let (count, painted) = view
        .update(cx, |view, cx| view.files.update(cx, |files, _| files.prose_find_for_test(thread)))
        .expect("the bar is open");
    assert_eq!(count.as_deref(), Some("1/1"));
    let painted = painted.expect("the hit painted");
    let shown = cx.debug_bounds("files-prose").unwrap();
    assert!(
        painted.top() >= shown.top() && painted.bottom() <= shown.bottom(),
        "the hit is in view: {painted:?} in {shown:?}"
    );

    press("escape", cx);
    assert!(cx.debug_bounds("find-bar").is_none(), "esc closes the bar");
}

/// F6 from a document read in the Panel's body goes to the Panel's
/// controls, not away from the Panel; a second one leaves.
#[gpui::test]
fn f6_from_a_document_in_the_panel_enters_its_controls(cx: &mut TestAppContext) {
    let (view, thread, _file, cx) = with_marked_file("focus-f6-readme", cx);
    let readme = view.read_with(cx, |view, cx| view.thread_path(thread, cx).unwrap()).join("README.md");
    std::fs::write(&readme, "# Hello\n\nbody\n").unwrap();
    view.update(cx, |view, cx| view.open_in_files(thread, readme, None, true, cx));
    tick(cx);
    let prose = cx.debug_bounds("files-prose").expect("the README reads as prose");
    cx.simulate_click(prose.center(), gpui::Modifiers::none());
    tick(cx);

    press("f6", cx);
    assert!(
        focused_control(&view, cx).is_some(),
        "the first F6 lands on the Panel's controls"
    );
    press("f6", cx);
    assert!(composer_focused(&view, cx), "the next one leaves for the conversation");
}

/// A file opened by a press in the tree: the press leaves the keyboard in
/// the Files surface, so ⌘F finds in that file.
#[gpui::test]
fn find_after_opening_a_file_from_the_tree_searches_the_file(cx: &mut TestAppContext) {
    let (view, thread, _file, cx) = with_marked_file("focus-tree-find", cx);
    view.update(cx, |view, cx| view.run_opener(thread, crate::cockpit::panel::Opener::Files, cx));
    tick(cx);
    for row in ["files-row-0", "files-row-1"] {
        let at = cx.debug_bounds(row).unwrap_or_else(|| panic!("{row} is drawn"));
        cx.simulate_click(at.center(), gpui::Modifiers::none());
        tick(cx);
    }
    let editor = view.read_with(cx, |view, cx| {
        view.files.read(cx).shown_editor(thread).expect("the press opened lib.rs")
    });

    press("cmd-f", cx);
    assert!(cx.debug_bounds("find-bar").is_some(), "⌘F opens the bar");
    assert!(
        !transcript_find_open(&view, cx),
        "the conversation's bar stays shut"
    );
    cx.simulate_input(ONLY_IN_THE_FILE);
    tick(cx);
    assert_eq!(file_find_count(&editor, cx).as_deref(), Some("1/1"));
    assert_eq!(composer_text(&view, cx), "", "no letter reached the draft");
}

/// Another surface on show: ⌘F from the Panel's controls is the
/// conversation's, even with a file open behind it in Files.
#[gpui::test]
fn find_from_the_controls_over_another_surface_searches_the_transcript(cx: &mut TestAppContext) {
    let (view, thread, file, cx) = with_marked_file("focus-other-find", cx);
    view.update(cx, |view, cx| {
        view.open_in_files(thread, file, None, true, cx);
        view.panes[0]
            .composer(cx)
            .update(cx, |composer, cx| composer.set(SENTINEL.into(), cx));
    });
    tick(cx);
    let editor = view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(thread).unwrap());
    let overview = cx.debug_bounds("panel-tab-Overview").expect("the Overview tab");
    cx.simulate_click(overview.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Overview));

    press("f6", cx);
    assert!(focused_control(&view, cx).is_some());
    press("cmd-f", cx);
    assert!(transcript_find_open(&view, cx), "the conversation's bar opens");
    cx.simulate_input("hello");
    tick(cx);
    assert_eq!(file_find_count(&editor, cx), None, "the hidden file is not searched");
    assert_eq!(composer_text(&view, cx), SENTINEL, "the typing was the bar's");

    press("escape", cx);
    assert!(!transcript_find_open(&view, cx), "esc closes the bar");
    assert!(composer_focused(&view, cx), "and typing returns to the composer");
}

/// ⌘↓ and ⌘↑ with the keyboard on the Panel's controls read the open prose
/// file to its end and back to its start, as Home and End do elsewhere.
#[gpui::test]
fn command_arrows_go_to_the_ends_of_a_prose_file(cx: &mut TestAppContext) {
    let (view, thread, file, cx) = with_marked_file("focus-prose-ends", cx);
    let readme = file.parent().unwrap().parent().unwrap().join("README.md");
    std::fs::write(&readme, "A paragraph of prose.\n\n".repeat(200)).unwrap();
    view.update(cx, |view, cx| view.open_in_files(thread, readme, None, true, cx));
    tick(cx);
    press("f6", cx);
    let offset = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.files.read(cx).prose_scroll(thread).unwrap().offset().y
        })
    };
    assert_eq!(offset(&view, cx), px(0.));
    press("cmd-down", cx);
    let end = offset(&view, cx);
    assert!(end < px(0.), "⌘↓ reads to the end");
    press("cmd-up", cx);
    assert_eq!(offset(&view, cx), px(0.), "⌘↑ goes back to the start");
}

/// ⌘↓ and ⌘↑ in the open Markdown preview read it to its end and back, and
/// PageDown pages it: the same paging keys the transcript answers.
#[gpui::test]
fn command_arrows_go_to_the_ends_of_the_preview(cx: &mut TestAppContext) {
    let dir = scratch("preview-ends-file");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notes.md");
    std::fs::write(&file, "A paragraph of prose.\n\n".repeat(200)).unwrap();
    let (core, _fake) = cockpit("preview-ends", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    super::find::read_beside(&view, file, cx);
    let offset = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.preview_tab(0, cx).markdown_scroll.offset().y
        })
    };
    for _ in 0..8 {
        view.update(cx, |_, cx| cx.notify());
        tick(cx);
    }
    assert_eq!(offset(&view, cx), px(0.));
    press("pagedown", cx);
    let page = offset(&view, cx);
    assert!(page < px(0.), "PageDown pages the preview");
    press("cmd-down", cx);
    assert!(offset(&view, cx) < page, "⌘↓ reads to the end");
    press("cmd-up", cx);
    assert_eq!(offset(&view, cx), px(0.), "⌘↑ goes back to the start");
}

/// A view of its own drawn in the Panel, which registers one stop as it
/// renders: after the root's render has returned, as a Surface moved into
/// its own view will.
struct ChildStop {
    focus: std::rc::Rc<crate::cockpit::panel::PanelFocus>,
}

impl Render for ChildStop {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let semantics = Semantics::button("A child's stop");
        let handle = self
            .focus
            .register(PanelControl::PrPrimary, semantics.clone(), cx);
        crate::cockpit::panel::controls::focusable(
            div().id("child-stop").child("child"),
            &handle,
            &semantics,
        )
    }
}

#[gpui::test]
fn a_stop_a_child_view_draws_keeps_its_place_in_the_row(cx: &mut TestAppContext) {
    let (view, _thread, cx) = with_tabs("focus-child", cx);
    let focus = view.read_with(cx, |view, _| view.panel.focus.clone());
    let child = cx.new(|_| ChildStop { focus });
    view.update(cx, |view, cx| {
        view.panel.test_child = Some(child.into());
        cx.notify();
    });
    tick(cx);
    let handle = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.panel.focus.handle(&PanelControl::PrPrimary))
    };
    let first = handle(cx).expect("the child's stop has a handle");
    view.update(cx, |_, cx| cx.notify());
    tick(cx);
    assert_eq!(
        handle(cx),
        Some(first),
        "a redraw keeps the child's stop's handle"
    );

    press("f6", cx);
    press("end", cx);
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::PrPrimary),
        "the child's stop is the row's last"
    );
    view.update(cx, |_, cx| cx.notify());
    tick(cx);
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::PrPrimary),
        "and keeps the keyboard across a redraw"
    );
    press("left", cx);
    assert_eq!(focused_control(&view, cx), Some(PanelControl::Maximize));
    press("right", cx);
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::PrPrimary),
        "the arrows step back onto it"
    );
}

/// The headless driver (ADR 0038) presses a key the way a keyboard does,
/// down then up: ⏎ on a tab shows it from the release and counts as
/// taken, and a key nothing takes is still reported untaken.
#[cfg(feature = "headless")]
#[test]
fn a_driven_enter_presses_the_focused_tab() {
    let (core, _fake, _checkout) = bound_cockpit("driven-enter", Provider::Claude);
    let cx = crate::headless::app(ferrite_ui::keymap::bindings(ferrite_ui::keymap::PLATFORM));
    let mut driver = crate::headless::Driver::open(cx, |cx| CockpitView::new(core, cx));
    let thread = driver.update(|view, _, cx| {
        let thread = view.panes[0].thread().unwrap();
        view.prefs.settings.panel_shown = true;
        view.upsert_surface(thread, SurfaceId::Files, cx);
        thread
    });
    driver.tick(16);
    let shown = |driver: &mut crate::headless::Driver| {
        driver.read(|view, _| view.active_surface(thread))
    };

    assert_eq!(driver.keys(&["f6", "home"]), Ok(vec![true, true]));
    assert_eq!(
        driver.update(|view, window, _| view.panel.focus.focused(window)),
        Some(PanelControl::Tab(SurfaceId::Overview))
    );
    assert_eq!(driver.keys(&["f19"]), Ok(vec![false]), "nothing takes f19");
    assert_eq!(shown(&mut driver), Some(SurfaceId::Files));

    assert_eq!(driver.keys(&["enter"]), Ok(vec![true]), "⏎ is taken");
    assert_eq!(shown(&mut driver), Some(SurfaceId::Overview), "⏎ shows the tab");
    driver.close();
}
