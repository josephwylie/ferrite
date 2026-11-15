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
            .composer
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
    ThreadId,
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
            .composer
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
        view.read_with(cx, |view, _| view.cockpit.thread(thread).is_some()),
        "the Thread is still open"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.panes.len()),
        1,
        "and its Pane is still on the board"
    );
    press("cmd-w", cx);
    assert!(
        view.read_with(cx, |view, _| view.cockpit.thread(thread).is_some()
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
            || view.read_with(cx, |view, _| view.context_menu.is_some())
    );
    press("escape", cx);
    assert!(
        view.read_with(cx, |view, _| view.context_menu.is_none()),
        "esc closes the menu first"
    );
    assert_eq!(
        focused_control(&view, cx),
        Some(PanelControl::Plus),
        "the keyboard is back on `+`"
    );

    press("shift-f10", cx);
    assert!(
        view.read_with(cx, |view, _| view.context_menu.is_some()),
        "⇧F10 opens the stop's menu"
    );
    press("t", cx);
    assert!(view.read_with(cx, |view, _| view.context_menu.is_none()));
    assert_eq!(
        composer_text(&view, cx),
        SENTINEL,
        "the letter ran its row and typed nothing"
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
    ThreadId,
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
    let transcript = view.read_with(cx, |view, _| view.panes[0].transcript().unwrap());
    transcript.read_with(cx, |transcript, _| transcript.find_state().is_some())
}

/// ⌘F with the keyboard on the Panel's controls over Files searches the
/// file, not the conversation.
#[gpui::test]
fn find_in_files_searches_the_file_not_the_transcript(cx: &mut TestAppContext) {
    let (view, thread, file, cx) = with_marked_file("focus-file-find", cx);
    view.update(cx, |view, cx| view.open_in_files(thread, file, None, true, cx));
    tick(cx);
    let editor = view.read_with(cx, |view, _| {
        view.files_editor_of(thread).expect("a code file")
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
    let editor = view.read_with(cx, |view, _| {
        view.files_editor_of(thread).expect("the press opened lib.rs")
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
            .composer
            .update(cx, |composer, cx| composer.set(SENTINEL.into(), cx));
    });
    tick(cx);
    let editor = view.read_with(cx, |view, _| view.files_editor_of(thread).unwrap());
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
