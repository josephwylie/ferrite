//! The one sheet host (ADR 0042): Settings, the Project card, the
//! `Background work` dialog and the Factory's New task sheet.
#[allow(unused_imports)]
use super::*;

fn up(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> (bool, bool) {
    view.read_with(cx, |view, cx| (view.settings_open(cx), view.project_card(cx).is_some()))
}

/// Opening one sheet over the other leaves exactly one up, and only that
/// one is drawn.
#[gpui::test]
fn opening_one_sheet_over_the_other_leaves_one_up(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("sheets-one-up", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    assert_eq!(up(&view, cx), (true, false));
    view.update(cx, |view, cx| view.open_project_creator(cx));
    tick(cx);
    assert_eq!(up(&view, cx), (false, true), "the card replaces Settings");
    assert!(cx.debug_bounds("settings-card").is_none(), "with no exit");
    assert!(cx.debug_bounds("project-editor-card").is_some());

    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    assert_eq!(up(&view, cx), (true, false), "Settings replaces the card");
    assert!(cx.debug_bounds("project-editor-card").is_none());
    assert!(cx.debug_bounds("settings-card").is_some());
}

/// Escape, the sheet's own close button and a press on the veil each
/// close either sheet.
#[gpui::test]
fn escape_the_close_button_and_the_veil_each_close_a_sheet(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("sheets-close", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let outside = gpui::point(px(20.), px(650.));

    for (open, close) in [
        (true, "settings-close"),
        (false, "project-editor-close"),
    ] {
        let show = |cx: &mut gpui::VisualTestContext| {
            view.update(cx, |view, cx| {
                if open {
                    view.toggle_settings(cx)
                } else {
                    view.open_project_creator(cx)
                }
            });
            cx.run_until_parked();
            assert_ne!(up(&view, cx), (false, false), "{close}: the sheet is up");
        };

        show(cx);
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(up(&view, cx), (false, false), "{close}: escape closes it");

        show(cx);
        let button = cx.debug_bounds(close).expect(close).center();
        cx.simulate_click(button, gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(up(&view, cx), (false, false), "{close}: its close button closes it");

        show(cx);
        cx.simulate_click(outside, gpui::Modifiers::none());
        cx.run_until_parked();
        assert_eq!(up(&view, cx), (false, false), "{close}: a press on the veil closes it");
    }
}

/// Closing Settings, by any way out, drops the connection being edited
/// at the close, though the view lives on through the sheet's fade.
#[gpui::test]
fn closing_settings_drops_the_connection_editor(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("sheets-connection-editor", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let editing = |cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| {
            if !view.settings_open(cx) {
                view.toggle_settings(cx);
            }
        });
        tick(cx);
        let connections = connections_view(&view, cx);
        let dir = view.read_with(cx, |view, _| view.prefs.dir.clone());
        connections.update_in(cx, |connections, window, cx| connections.edit(None, &dir, window, cx));
        tick(cx);
        assert!(connections.read_with(cx, |connections, _| connections.editing()));
        connections
    };
    let dropped = |connections: &Entity<ferrite_settings::connections::ConnectionsView>,
                   cx: &mut gpui::VisualTestContext,
                   how: &str| {
        view.read_with(cx, |view, cx| {
            assert!(!view.settings_open(cx), "{how} closes Settings");
            assert!(!connections.read(cx).editing(), "{how} drops the editor");
        });
    };

    let connections = editing(cx);
    view.update(cx, |view, cx| view.toggle_settings(cx));
    tick(cx);
    dropped(&connections, cx, "cmd-,");

    let connections = editing(cx);
    view.update(cx, |view, cx| {
        view.settings_sheet(cx)
            .unwrap()
            .update(cx, |_, cx| cx.emit(ferrite_settings::sheet::SettingsEvent::Closed))
    });
    tick(cx);
    dropped(&connections, cx, "the close button");

    let connections = editing(cx);
    view.update(cx, |view, cx| {
        view.sheets.update(cx, |_, cx| cx.emit(crate::cockpit::sheets::Dismissed))
    });
    tick(cx);
    dropped(&connections, cx, "the veil");

    let connections = editing(cx);
    view.update(cx, |view, cx| view.open_project_creator(cx));
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.project_card(cx).is_some());
        assert!(!connections.read(cx).editing(), "the card drops the editor");
    });
}

fn dialog_up(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> bool {
    view.read_with(cx, |view, cx| view.background_dialog(cx).is_some())
}

fn open_dialog(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| {
        let thread = view.focused_thread(cx).unwrap();
        view.open_background_dialog(thread, cx)
    });
    tick(cx);
    assert!(dialog_up(view, cx), "the dialog is up");
}

/// The `Background work` dialog and each other sheet replace one another,
/// with no exit.
#[gpui::test]
fn the_background_work_dialog_and_each_other_sheet_replace_one_another(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("sheets-background-dialog", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    open_dialog(&view, cx);
    assert!(cx.debug_bounds("background-dialog").is_some());
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    assert_eq!(up(&view, cx), (true, false), "Settings replaces the dialog");
    assert!(!dialog_up(&view, cx));
    assert!(cx.debug_bounds("background-dialog").is_none(), "with no exit");

    open_dialog(&view, cx);
    assert_eq!(up(&view, cx), (false, false), "the dialog replaces Settings");
    assert!(cx.debug_bounds("settings-card").is_none());
    assert!(cx.debug_bounds("background-dialog").is_some());

    view.update(cx, |view, cx| view.open_project_creator(cx));
    tick(cx);
    assert_eq!(up(&view, cx), (false, true), "the card replaces the dialog");
    assert!(!dialog_up(&view, cx));
    assert!(cx.debug_bounds("background-dialog").is_none());

    open_dialog(&view, cx);
    assert_eq!(up(&view, cx), (false, false), "the dialog replaces the card");
    assert!(cx.debug_bounds("project-editor-card").is_none());
}

/// Escape, its close button and a press on the veil each close the
/// `Background work` dialog.
#[gpui::test]
fn escape_the_close_button_and_the_veil_each_close_the_background_work_dialog(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("sheets-background-dialog-close", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    open_dialog(&view, cx);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!dialog_up(&view, cx), "escape closes it");

    open_dialog(&view, cx);
    let button = cx.debug_bounds("background-dialog-close").expect("its close button").center();
    cx.simulate_click(button, gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(!dialog_up(&view, cx), "its close button closes it");

    open_dialog(&view, cx);
    cx.simulate_click(gpui::point(px(20.), px(650.)), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(!dialog_up(&view, cx), "a press on the veil closes it");
}

/// While the dialog is up the keyboard is its, not the Composer's under the
/// veil.
#[gpui::test]
fn the_background_work_dialog_holds_the_keyboard(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("sheets-background-dialog-focus", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    open_dialog(&view, cx);
    let held = cx.update(|window, cx| {
        let dialog = view.read(cx).background_dialog(cx).unwrap();
        dialog.read(cx).focus.contains_focused(window, cx)
    });
    assert!(held);
}

/// Parking the dialog's Thread closes the dialog: nothing is left to show.
#[gpui::test]
fn the_background_work_dialog_closes_with_its_thread(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("sheets-background-dialog-park", 2);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    open_dialog(&view, cx);
    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    assert!(!dialog_up(&view, cx));
}

/// A Project removed from under its card closes the card: nothing is left
/// to edit.
#[gpui::test]
fn the_project_card_closes_with_its_project(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("sheets-project-card-removed", 1);
    let project = core
        .register_project(core.first_machine(), &repo_in(&scratch("sheets-project-card-removed-repo")))
        .now().unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    view.update(cx, |view, cx| view.open_project_editor(project, cx));
    tick(cx);
    assert_eq!(up(&view, cx), (false, true), "the card is up");
    view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| cockpit.remove_project(project).now().unwrap()));
    tick(cx);
    assert_eq!(up(&view, cx), (false, false), "the card closes with its Project");
}

/// A window on a Project's Factory board, its Factory live in memory.
pub(super) fn factory_window<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let (mut core, _fake) = cockpit(name, 1);
    let project = core.project_of(core.threads()[0]).unwrap();
    core.fixture_live_factory();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    view.update(cx, |view, cx| view.open_factory(project, cx));
    tick(cx);
    (view, cx)
}

pub(super) fn new_task_up(cx: &mut gpui::VisualTestContext) -> bool {
    cx.debug_bounds("new-task-card").is_some()
}

/// What the New task sheet holds, as typed: its instructions and its PR.
fn new_task_typed(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<(String, String)> {
    view.read_with(cx, |view, cx| Some(view.new_task(cx)?.read(cx).typed(cx)))
}

pub(super) fn press(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let at = cx.debug_bounds(selector).expect(selector).center();
    cx.simulate_click(at, gpui::Modifiers::none());
    tick(cx);
}

/// Tab leaves the keyboard in the field that holds it.
#[gpui::test]
fn tab_leaves_the_keyboard_in_its_new_task_field(cx: &mut TestAppContext) {
    let (view, cx) = factory_window(cx, "sheets-new-task-tab");
    view.update(cx, |view, cx| view.open_new_task(cx));
    tick(cx);
    cx.simulate_input("a");
    cx.simulate_keystrokes("tab");
    tick(cx);
    assert!(cx.debug_bounds("new-task-instructions-focused").is_some());
    cx.simulate_input("b");
    press(cx, "new-task-pr");
    cx.simulate_input("c");
    cx.simulate_keystrokes("tab");
    tick(cx);
    assert!(cx.debug_bounds("new-task-pr-focused").is_some());
    cx.simulate_input("d");
    tick(cx);
    assert_eq!(new_task_typed(&view, cx), Some(("ab".into(), "cd".into())));
}

fn new_task_error(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, cx| {
        view.new_task(cx)?.read(cx).error().map(ToString::to_string)
    })
}

/// What was typed, the PR and the process chosen are there again after a
/// close and a reopen, with no refusal, the keyboard back on the
/// instructions, and the same request, so Start is still the same Task.
#[gpui::test]
fn the_new_task_sheet_keeps_what_was_typed_across_a_close(cx: &mut TestAppContext) {
    let (view, cx) = factory_window(cx, "sheets-new-task-keeps");
    view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_record_tasks());
        view.open_new_task(cx);
    });
    tick(cx);
    cx.simulate_input("Fix the flake ");
    press(cx, "new-task-pr");
    cx.simulate_input("acme/widgets#7");
    view.update(cx, |view, cx| view.apply_pick(crate::cockpit::workflows::Pick::Process(Some("w-1".into())), cx));
    view.update(cx, |view, cx| view.start_new_task(cx));
    tick(cx);
    assert!(new_task_error(&view, cx).is_some(), "the fixture's answer is a refusal");
    press(cx, "new-task-close");
    assert!(!new_task_up(cx), "the close button closes it");

    view.update(cx, |view, cx| view.open_new_task(cx));
    tick(cx);
    assert!(new_task_up(cx));
    assert_eq!(
        new_task_typed(&view, cx),
        Some(("Fix the flake ".into(), "acme/widgets#7".into())),
        "as typed"
    );
    assert_eq!(new_task_error(&view, cx), None, "the refusal does not come back");
    assert!(cx.debug_bounds("new-task-instructions-focused").is_some());
    view.update(cx, |view, cx| view.start_new_task(cx));
    tick(cx);
    let asks = view.read_with(cx, |view, cx| view.cockpit.read(cx).fixture_task_asks().to_vec());
    match asks.as_slice() {
        [ferrite_core::protocol::TaskAction::Start { request: first, workflow: w1, .. }, ferrite_core::protocol::TaskAction::Start { request: second, workflow: w2, .. }] => {
            assert_eq!(first, second, "one request across the close");
            assert_eq!(w1.as_deref(), Some("w-1"));
            assert_eq!(w2.as_deref(), Some("w-1"), "the process chosen stays");
        }
        other => panic!("two starts: {other:?}"),
    }
}

/// Start with no instructions is refused on the sheet, which stays up.
#[gpui::test]
fn the_new_task_sheet_refuses_a_start_with_no_instructions(cx: &mut TestAppContext) {
    let (view, cx) = factory_window(cx, "sheets-new-task-refused");
    view.update(cx, |view, cx| view.open_new_task(cx));
    tick(cx);
    press(cx, "new-task-start");
    assert!(new_task_up(cx));
    assert_eq!(new_task_error(&view, cx).as_deref(), Some("Say what the task is."));
}

/// A Task started closes the sheet, opens the Task's panel, and leaves
/// nothing typed for the next New task.
#[gpui::test]
fn a_task_started_closes_the_new_task_sheet_and_spends_its_words(cx: &mut TestAppContext) {
    let (view, cx) = factory_window(cx, "sheets-new-task-started");
    view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_start_tasks());
        view.open_new_task(cx);
    });
    tick(cx);
    cx.simulate_input("Write the notes");
    press(cx, "new-task-start");
    assert_eq!(new_task_error(&view, cx), None);
    assert!(!new_task_up(cx), "Start closes it");
    assert_eq!(
        view.read_with(cx, |view, cx| view.factory().unwrap().read(cx).open.clone()).as_deref(),
        Some("started-1"),
        "on the Task's panel"
    );
    view.update(cx, |view, cx| view.open_new_task(cx));
    tick(cx);
    assert_eq!(new_task_typed(&view, cx), Some((String::new(), String::new())));
}

/// Leaving the Factory takes the sheet, and what was typed in it, with it.
#[gpui::test]
fn leaving_the_factory_takes_the_new_task_sheet_and_its_words(cx: &mut TestAppContext) {
    let (view, cx) = factory_window(cx, "sheets-new-task-leave");
    let project = view.read_with(cx, |view, _| view.factory_project().unwrap());
    view.update(cx, |view, cx| view.open_new_task(cx));
    tick(cx);
    cx.simulate_input("Gone");
    tick(cx);
    view.update(cx, |view, cx| {
        let thread = view.focused_thread(cx).unwrap();
        view.focus_thread(thread, cx)
    });
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.factory().is_none()), "the Factory left");
    assert!(!new_task_up(cx), "and the sheet with it");
    assert!(view.read_with(cx, |view, cx| view.new_task(cx).is_none()), "closed, not only undrawn");
    view.update(cx, |view, cx| {
        view.open_factory(project, cx);
        view.open_new_task(cx);
    });
    tick(cx);
    assert_eq!(new_task_typed(&view, cx), Some((String::new(), String::new())));
}

/// Escape, its close button and a press on the veil each close the New
/// task sheet. A menu it opened closes before it does.
#[gpui::test]
fn escape_the_close_button_and_the_veil_each_close_the_new_task_sheet(cx: &mut TestAppContext) {
    let (view, cx) = factory_window(cx, "sheets-new-task-close");
    let open = |cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| view.open_new_task(cx));
        tick(cx);
        assert!(new_task_up(cx), "the sheet is up");
    };

    open(cx);
    press(cx, "new-task-process");
    assert!(cx.debug_bounds("context-menu").is_some(), "the Process menu");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(cx.debug_bounds("context-menu").is_none(), "escape closes the menu first");
    assert!(new_task_up(cx), "and leaves the sheet up");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!new_task_up(cx), "escape closes it");

    open(cx);
    press(cx, "new-task-close");
    cx.run_until_parked();
    assert!(!new_task_up(cx), "its close button closes it");

    open(cx);
    cx.simulate_click(gpui::point(px(20.), px(880.)), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(!new_task_up(cx), "a press on the veil closes it");
}

/// The New task sheet and Settings replace one another with no exit, and
/// what was typed stays for the sheet's next opening.
#[gpui::test]
fn the_new_task_sheet_and_another_sheet_replace_one_another(cx: &mut TestAppContext) {
    let (view, cx) = factory_window(cx, "sheets-new-task-replace");
    view.update(cx, |view, cx| view.open_new_task(cx));
    tick(cx);
    cx.simulate_input("Kept");
    tick(cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    assert_eq!(up(&view, cx), (true, false), "Settings replaces the sheet");
    assert!(!new_task_up(cx), "with no exit");
    assert!(view.read_with(cx, |view, cx| view.new_task(cx).is_none()));

    view.update(cx, |view, cx| view.open_new_task(cx));
    tick(cx);
    assert_eq!(up(&view, cx), (false, false), "the sheet replaces Settings");
    assert!(cx.debug_bounds("settings-card").is_none());
    assert_eq!(new_task_typed(&view, cx), Some(("Kept".into(), String::new())));
}
