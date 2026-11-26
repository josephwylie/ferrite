//! The popups: the context menu, the permission mode menu, the plan card,
//! the checks card and the session controls card. One is up at a time,
//! Escape and a press outside close it, and it never stops a turn.
#[allow(unused_imports)]
use super::*;
use crate::cockpit::context_menu::MenuTarget;
use ferrite_core::progress::{PlanStep, ProgressEvent, StepStatus};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Menu,
    Mode,
    Plan,
    Checks,
    Session,
    Picker,
    Attach,
    Band,
}

/// Every popup a Thread's window can open.
const KINDS: [Kind; 7] = [
    Kind::Menu,
    Kind::Mode,
    Kind::Plan,
    Kind::Checks,
    Kind::Session,
    Kind::Picker,
    Kind::Attach,
];

impl Kind {
    fn selector(self) -> &'static str {
        match self {
            Kind::Menu => "context-menu",
            Kind::Mode => "mode-menu",
            Kind::Plan => "plan-card",
            Kind::Checks => "context-checks-card",
            Kind::Session => "session-controls-card",
            Kind::Picker => "model-picker",
            Kind::Attach => "choice-menu",
            Kind::Band => "composer-menu",
        }
    }
}

/// The popups up now.
fn up(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<Kind> {
    view.read_with(cx, |view, _| {
        view.popup()
            .map(|popup| match popup {
                Popup::ContextMenu(_) => Kind::Menu,
                Popup::Mode(_) => Kind::Mode,
                Popup::Plan(_) => Kind::Plan,
                Popup::Checks(_) => Kind::Checks,
                Popup::Session { .. } => Kind::Session,
                Popup::ModelPicker(_) => Kind::Picker,
                Popup::Band(_) => Kind::Band,
                Popup::Attach { .. } => Kind::Attach,
            })
            .into_iter()
            .collect()
    })
}

/// A press on the control whose selector is `selector`.
fn press(cx: &mut gpui::VisualTestContext, selector: String) {
    let at = debug_bounds(cx, selector.clone()).expect(&selector);
    cx.simulate_click(at.center(), gpui::Modifiers::none());
}

/// The control a Composer popup opens from: the model chip, the `+`, the
/// draft's Project chip.
fn control(view: &Entity<CockpitView>, kind: Kind, cx: &mut gpui::VisualTestContext) -> String {
    let key = view.read_with(cx, |view, _| view.panes[0].thread().map(|thread| thread.thread().get()));
    match kind {
        Kind::Mode => format!("mode-picker-{}", key.unwrap()),
        Kind::Picker => format!("model-picker-{}", key.unwrap()),
        Kind::Attach => "composer-attach".into(),
        Kind::Band => "band-chip-0".into(),
        _ => unreachable!("{kind:?} opens from no control"),
    }
}

fn open(view: &Entity<CockpitView>, kind: Kind, cx: &mut gpui::VisualTestContext) {
    if matches!(kind, Kind::Picker | Kind::Attach | Kind::Band) {
        let control = control(view, kind, cx);
        press(cx, control);
        tick(cx);
        assert_eq!(up(view, cx), [kind], "{kind:?} opens");
        assert!(cx.debug_bounds(kind.selector()).is_some(), "{kind:?} is drawn");
        return;
    }
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| match kind {
            Kind::Menu => view.open_context_menu(
                MenuTarget::Thread(thread),
                gpui::point(px(300.), px(200.)),
                cx,
            ),
            Kind::Mode => {
                let generation = view.cockpit.read(cx).thread(thread).unwrap().generation();
                view.toggle_mode_menu(thread, generation, cx);
            }
            Kind::Plan => view.open_plan(window, cx),
            Kind::Checks => view.open_checks(window, cx),
            Kind::Session => view.run_command(crate::palette::Command::BackgroundTasks, window, cx),
            Kind::Picker | Kind::Attach | Kind::Band => unreachable!(),
        })
    });
    tick(cx);
    assert_eq!(up(view, cx), [kind], "{kind:?} opens");
    assert!(cx.debug_bounds(kind.selector()).is_some(), "{kind:?} is drawn");
}

/// A window whose Threads have a plan, a PR with checks and a Session
/// with native controls, the first in a turn: every popup can open on it.
fn window<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    threads: usize,
) -> (Entity<CockpitView>, Fake, &'a mut gpui::VisualTestContext) {
    let (mut core, fake) = cockpit(name, threads);
    let thread = core.threads()[0];
    assert_eq!(core.send(thread, "Ship it".into()).now().unwrap(), ferrite_core::prompt_queue::Admission::Delivered);
    *fake.native_controls.borrow_mut() = true;
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    for stream in fake.streams.borrow().iter() {
        stream
            .send(SessionEvent::PermissionMode {
                mode: "default".into(),
            })
            .unwrap();
        stream
            .send(SessionEvent::Progress {
                event: ProgressEvent::Plan {
                    steps: vec![
                        PlanStep {
                            text: "Read the config".into(),
                            status: StepStatus::Completed,
                        },
                        PlanStep {
                            text: "Rebuild the index".into(),
                            status: StepStatus::Pending,
                        },
                    ],
                    explanation: String::new(),
                },
            })
            .unwrap();
    }
    view.update(cx, |view, cx| {
        for thread in view.cockpit.read(cx).threads().to_vec() {
            view.facts.pr_answered(thread, Some(pr_with_checks()));
        }
        cx.notify();
    });
    tick(cx);
    (view, fake, cx)
}

/// A press on nothing that takes it, at the transcript's left.
fn press_nowhere(cx: &mut gpui::VisualTestContext) {
    let board = cx.debug_bounds("board").expect("the board");
    cx.simulate_click(gpui::point(board.left() + px(60.), board.center().y), gpui::Modifiers::none());
    tick(cx);
}

/// Escape closes the context menu, the checks card and the session
/// controls card, and stops no turn.
#[gpui::test]
fn escape_closes_a_menu_or_card_and_stops_no_turn(cx: &mut TestAppContext) {
    let (view, fake, cx) = window(cx, "popups-escape", 1);
    for kind in [Kind::Menu, Kind::Checks, Kind::Session, Kind::Picker, Kind::Attach] {
        open(&view, kind, cx);
        cx.simulate_keystrokes("escape");
        tick(cx);
        assert_eq!(up(&view, cx), [], "escape closes {kind:?}");
        assert_eq!(*fake.interrupts.borrow(), 0, "escape on {kind:?} stops no turn");
    }
}

/// Escape with the permission mode menu or the plan card up closed
/// neither: it stopped the turn.
#[gpui::test]
fn escape_closes_the_mode_menu_and_the_plan_card_and_stops_no_turn(cx: &mut TestAppContext) {
    let (view, fake, cx) = window(cx, "popups-escape-mode", 1);
    for kind in [Kind::Mode, Kind::Plan] {
        open(&view, kind, cx);
        cx.simulate_keystrokes("escape");
        tick(cx);
        assert_eq!(*fake.interrupts.borrow(), 0, "escape on {kind:?} stops no turn");
        assert_eq!(up(&view, cx), [], "escape closes {kind:?}");
    }
}

/// With a Decision waiting and the Composer empty, Escape on the mode
/// menu denied the Decision.
#[gpui::test]
fn escape_on_the_mode_menu_leaves_a_waiting_decision_unanswered(cx: &mut TestAppContext) {
    let (view, fake, cx) = window(cx, "popups-escape-decision", 1);
    fake.streams.borrow()[0].send(decision("perm_mode")).unwrap();
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let chip = debug_bounds(cx, format!("mode-picker-{}", thread.thread().get())).expect("the mode chip");
    cx.simulate_click(chip.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(up(&view, cx), [Kind::Mode]);
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(fake.answered.borrow().is_empty(), "the Decision still waits");
    assert_eq!(up(&view, cx), [], "escape closes the menu");
}

/// The mode menu opened from the model picker's footer is the picker's:
/// Escape closes the menu alone, and the picker closing takes the menu.
#[gpui::test]
fn the_mode_menu_from_the_model_picker_goes_before_it_and_with_it(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-mode-from-picker", 1);
    open(&view, Kind::Picker, cx);
    let from_picker = |cx: &mut gpui::VisualTestContext| {
        press(cx, "model-picker-permissions".into());
        tick(cx);
        assert!(cx.debug_bounds("mode-menu").is_some(), "the picker's button opens the mode menu");
        assert!(up(&view, cx).contains(&Kind::Picker), "over the picker");
    };

    from_picker(cx);
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(cx.debug_bounds("mode-menu").is_none(), "the first escape closes the menu");
    assert_eq!(up(&view, cx), [Kind::Picker], "and leaves the picker up");

    from_picker(cx);
    press_nowhere(cx);
    assert_eq!(up(&view, cx), [], "the picker closing takes the menu");
    assert!(cx.debug_bounds("mode-menu").is_none());
}

/// Opening any popup over any other leaves the new one up alone.
#[gpui::test]
fn opening_a_popup_over_another_leaves_one_up(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-one-up", 1);
    for under in KINDS {
        for over in KINDS.into_iter().filter(|over| *over != under) {
            open(&view, under, cx);
            open(&view, over, cx);
            assert_eq!(up(&view, cx), [over], "{over:?} over {under:?}");
            press_nowhere(cx);
        }
    }
}

/// The plan card is its Thread's: another Thread taking focus closes it,
/// and coming back does not bring it back.
#[gpui::test]
fn the_plan_card_closes_when_another_thread_takes_focus(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-plan-focus", 2);
    open(&view, Kind::Plan, cx);
    view.update(cx, |view, cx| {
        view.focus_pane(1, cx);
        cx.notify();
    });
    tick(cx);
    assert_eq!(up(&view, cx), [], "another Thread's focus closes the card");
    view.update(cx, |view, cx| {
        view.focus_pane(0, cx);
        cx.notify();
    });
    tick(cx);
    assert_eq!(up(&view, cx), []);
    assert!(cx.debug_bounds("plan-card").is_none(), "the card does not come back");
}

/// A sheet opening closes whatever popup is up.
#[gpui::test]
fn a_sheet_opening_closes_the_popup(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-sheet", 1);
    for kind in KINDS {
        open(&view, kind, cx);
        cx.simulate_keystrokes("cmd-,");
        tick(cx);
        assert!(view.read_with(cx, |view, cx| view.settings_open(cx)), "{kind:?}: Settings opens");
        assert_eq!(up(&view, cx), [], "Settings closes {kind:?}");
        cx.simulate_keystrokes("cmd-,");
        tick(cx);
    }
}

/// A press outside any popup closes it.
#[gpui::test]
fn a_press_outside_closes_a_popup(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-press-outside", 1);
    for kind in KINDS {
        open(&view, kind, cx);
        press_nowhere(cx);
        assert_eq!(up(&view, cx), [], "a press outside closes {kind:?}");
    }
}

/// A second press on the control a popup opened from shuts it and does
/// not open it again: the mode chip, the model chip, the `+`.
#[gpui::test]
fn a_second_press_on_its_control_shuts_a_popup(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-mode-chip", 1);
    for kind in [Kind::Mode, Kind::Picker, Kind::Attach] {
        let control = control(&view, kind, cx);
        press(cx, control.clone());
        tick(cx);
        assert_eq!(up(&view, cx), [kind], "a press opens {kind:?}");
        press(cx, control);
        tick(cx);
        assert_eq!(up(&view, cx), [], "a second press shuts {kind:?}");
        tick(cx);
        assert_eq!(up(&view, cx), [], "and {kind:?} stays shut");
    }
}

/// With a Decision waiting and the Composer empty, Escape on the model
/// picker or the `+` menu closes it and answers nothing, and the keyboard
/// is the Composer's again.
#[gpui::test]
fn escape_on_a_composer_popup_leaves_a_waiting_decision_unanswered(cx: &mut TestAppContext) {
    let (view, fake, cx) = window(cx, "popups-escape-composer", 1);
    fake.streams.borrow()[0].send(decision("perm_composer")).unwrap();
    tick(cx);
    let composer = view.read_with(cx, |view, cx| view.panes[0].read(cx).composer.focus_handle(cx));
    for kind in [Kind::Picker, Kind::Attach] {
        open(&view, kind, cx);
        cx.simulate_keystrokes("escape");
        tick(cx);
        assert!(fake.answered.borrow().is_empty(), "{kind:?}: the Decision still waits");
        assert_eq!(*fake.interrupts.borrow(), 0, "{kind:?}: escape stops no turn");
        assert_eq!(up(&view, cx), [], "escape closes {kind:?}");
        assert!(
            cx.update(|window, _| composer.is_focused(window)),
            "{kind:?}: the Composer has the keyboard back"
        );
    }
}

/// The model picker's search line has the keyboard while it is up: what
/// is typed filters the list and never reaches the Composer under it.
#[gpui::test]
fn typing_in_the_model_picker_filters_it(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-picker-typing", 1);
    open(&view, Kind::Picker, cx);
    assert!(cx.debug_bounds("model-picker-row-0").is_some(), "the picker lists models");
    cx.simulate_input("zzzz");
    tick(cx);
    assert!(cx.debug_bounds("model-picker-row-0").is_none(), "nothing matches zzzz");
    assert_eq!(composer_text(&view, cx), "", "the Composer heard none of it");
    assert_eq!(up(&view, cx), [Kind::Picker]);
}

/// The `+` menu's keys are its own: the arrows walk it and Enter picks
/// `Slash commands`, which starts a command on the line.
#[gpui::test]
fn the_attach_menu_takes_the_arrows_and_enter(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-attach-keys", 1);
    open(&view, Kind::Attach, cx);
    cx.simulate_keystrokes("down down enter");
    tick(cx);
    assert!(!up(&view, cx).contains(&Kind::Attach), "the pick closes the + menu");
    assert_eq!(composer_text(&view, cx), "/", "Slash commands starts one on the line");
}

/// A draft with its band, the Project chip pressable.
fn draft_window<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let (core, _fake) = cockpit(name, 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    view.update(cx, |view, cx| view.open_draft(DraftTarget::Main, cx));
    tick(cx);
    (view, cx)
}

/// A draft's Project menu is a popup like any other: its chip opens it and
/// shuts it, the arrows and Enter pick in it, and Escape, a press outside
/// or a sheet opening close it.
#[gpui::test]
fn a_band_menu_opens_and_closes_as_a_popup(cx: &mut TestAppContext) {
    let (view, cx) = draft_window(cx, "popups-band");
    let chip = control(&view, Kind::Band, cx);

    open(&view, Kind::Band, cx);
    press(cx, chip.clone());
    tick(cx);
    assert_eq!(up(&view, cx), [], "a second press on the chip shuts it");

    open(&view, Kind::Band, cx);
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(up(&view, cx), [], "escape closes it");

    open(&view, Kind::Band, cx);
    press_nowhere(cx);
    assert_eq!(up(&view, cx), [], "a press outside closes it");

    open(&view, Kind::Band, cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    assert!(view.read_with(cx, |view, cx| view.settings_open(cx)), "Settings opens");
    assert_eq!(up(&view, cx), [], "Settings closes it");
    cx.simulate_keystrokes("cmd-,");
    tick(cx);

    open(&view, Kind::Band, cx);
    view.update(cx, |view, cx| {
        let at = gpui::point(px(300.), px(200.));
        view.open_context_menu(MenuTarget::Parked, at, cx)
    });
    tick(cx);
    assert_eq!(up(&view, cx), [Kind::Menu], "a context menu over it leaves one up");
}

/// The Workspace menu's arrows and Enter pick a workspace for the draft.
#[gpui::test]
fn a_band_menu_takes_the_arrows_and_enter(cx: &mut TestAppContext) {
    let (view, cx) = draft_window(cx, "popups-band-keys");
    press(cx, "band-chip-1".into());
    tick(cx);
    assert_eq!(up(&view, cx), [Kind::Band], "the Workspace chip opens its menu");
    cx.simulate_keystrokes("down enter");
    tick(cx);
    assert_eq!(up(&view, cx), [], "the pick closes it");
    let target = view.read_with(cx, |view, cx| {
        view.panes[view.focused(cx)].read(cx).draft().unwrap().binding.target().clone()
    });
    assert!(!matches!(target, DraftTarget::Main), "the pick moved the draft off the checkout");
}

/// A Factory menu opened from a sheet goes with the sheet: a press on the
/// veil closes both, and the sheet closing by any other way takes it.
#[gpui::test]
fn a_menu_opened_from_a_sheet_goes_with_the_sheet(cx: &mut TestAppContext) {
    let (view, cx) = super::sheets::factory_window(cx, "popups-sheet-menu");
    let open = |cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| view.open_new_task(cx));
        tick(cx);
        super::sheets::press(cx, "new-task-process");
        assert!(cx.debug_bounds("context-menu").is_some(), "the Process menu");
    };

    open(cx);
    cx.simulate_click(gpui::point(px(20.), px(880.)), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(!super::sheets::new_task_up(cx), "a press on the veil closes the sheet");
    assert_eq!(up(&view, cx), [], "and the menu");

    open(cx);
    view.update(cx, |view, cx| view.hide_sheet(cx));
    tick(cx);
    assert!(!super::sheets::new_task_up(cx));
    assert_eq!(up(&view, cx), [], "the sheet closing takes the menu");
}

/// A press on a context menu's submenu, drawn beside the menu, is the
/// menu's own: its row runs.
#[gpui::test]
fn a_press_on_a_submenu_row_runs_it(cx: &mut TestAppContext) {
    use crate::cockpit::context_menu::Submenu;
    let (view, _fake, cx) = window(cx, "popups-submenu", 1);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    open(&view, Kind::Menu, cx);
    let snooze = view.read_with(cx, |view, _| {
        view.context_menu()
            .unwrap()
            .rows
            .iter()
            .position(|row| matches!(row, Some((_, MenuVerb::Submenu(Submenu::Snooze)))))
            .expect("the Snooze row")
    });
    let parent = debug_bounds(cx, format!("context-menu-row-{snooze}")).expect("Snooze");
    cx.simulate_mouse_move(parent.center(), None, gpui::Modifiers::none());
    tick(cx);
    let row = cx.debug_bounds("context-submenu-row-0").expect("the pointer opens the submenu");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(up(&view, cx), [], "the row closes the menu");
    assert!(
        view.read_with(cx, |view, cx| view.cockpit.read(cx).thread(thread).unwrap().snooze().is_some()),
        "and its verb ran"
    );
}

/// A card takes the keyboard whole while it is up: a key typed reaches no
/// field under it, and Escape gives the keyboard back to the Composer.
#[gpui::test]
fn a_card_takes_the_keyboard_and_gives_it_back(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-keyboard", 1);
    let composer = view.read_with(cx, |view, cx| view.panes[0].read(cx).composer.focus_handle(cx));
    cx.update(|window, cx| window.focus(&composer, cx));
    tick(cx);
    for kind in [Kind::Plan, Kind::Checks, Kind::Session] {
        open(&view, kind, cx);
        cx.simulate_keystrokes("x");
        tick(cx);
        assert_eq!(composer_text(&view, cx), "", "{kind:?}: the key reached no field");
        assert_eq!(up(&view, cx), [kind], "{kind:?} stays up");
        cx.simulate_keystrokes("escape");
        tick(cx);
        assert!(
            cx.update(|window, _| composer.is_focused(window)),
            "{kind:?}: the Composer has the keyboard back"
        );
    }
}

/// A card hanging from a Pane's head hangs under the Thread details card
/// drawn in that Pane, not over it.
#[gpui::test]
fn a_head_card_hangs_under_the_details_card(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-under-details", 1);
    cx.simulate_resize(gpui::size(px(2400.), px(900.)));
    tick(cx);
    let details = cx.debug_bounds("details-card").expect("the details card, inline");
    for kind in [Kind::Plan, Kind::Checks] {
        open(&view, kind, cx);
        let card = cx.debug_bounds(kind.selector()).unwrap();
        assert!(
            !card.intersects(&details),
            "{kind:?} at {card:?} covers the details card at {details:?}"
        );
        assert_eq!(card.right(), details.right(), "{kind:?} keeps the details card's right edge");
        cx.simulate_keystrokes("escape");
        tick(cx);
    }
}

/// The pointer resting on a control, long past a tooltip's delay.
fn rest_on(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_move(at, None, gpui::Modifiers::none());
    cx.simulate_mouse_move(at + gpui::point(px(1.), px(0.)), None, gpui::Modifiers::none());
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
}

/// A control whose popup is up shows no tooltip over it: the mode chip,
/// the model chip, the `+` and the model picker's permissions button. At
/// rest with nothing up, each shows its own.
#[gpui::test]
fn a_control_whose_popup_is_up_shows_no_tooltip(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-no-tooltip", 1);
    let key = view.read_with(cx, |view, _| view.panes[0].thread().unwrap().thread().get());
    let mut controls = vec![
        format!("mode-picker-{key}"),
        format!("model-picker-{key}"),
        "composer-attach".to_string(),
    ];
    controls.push(crate::cockpit::model_picker::PICKER_PERMISSIONS.to_string());
    for control in controls {
        let picker_first = control == crate::cockpit::model_picker::PICKER_PERMISSIONS;
        if picker_first {
            let chip = debug_bounds(cx, format!("model-picker-{key}")).expect("the model chip");
            cx.simulate_click(chip.center(), gpui::Modifiers::none());
            tick(cx);
        }
        let at = debug_bounds(cx, control.clone()).expect(&control).center();
        rest_on(cx, at);
        assert!(cx.debug_bounds("tooltip").is_some(), "{control}: its tooltip, at rest");
        cx.simulate_click(at, gpui::Modifiers::none());
        tick(cx);
        rest_on(cx, at);
        assert!(cx.debug_bounds("tooltip").is_none(), "{control}: no tooltip over its popup");
        cx.simulate_keystrokes("escape");
        tick(cx);
        if picker_first {
            cx.simulate_keystrokes("escape");
            tick(cx);
        }
        press_nowhere(cx);
    }
}

/// The second of two Panes asks for itself from its cell (`PaneEvent`):
/// a right press opens its own Thread's menu, files over its box are its
/// own, its `+` opens its own menu and `/` lands in its own box, its
/// details card's right press and PR row are its own Thread's, its
/// Review's head dragged onto the cell swaps the two on its board, and a
/// new draft's hero word drops that draft's Project menu: none of it the
/// first Pane's.
#[gpui::test]
fn the_second_panes_cell_asks_for_itself(cx: &mut TestAppContext) {
    use crate::cockpit::surfaces::SurfaceId;
    use ferrite_core::layout::Zone;
    let (view, _fake, cx) = window(cx, "popups-second-pane", 2);
    view.update(cx, |view, cx| {
        view.focus_pane(1, cx);
        cx.notify();
    });
    tick(cx);
    let (first, second) = view.read_with(cx, |view, _| (view.panes[0].identity, view.panes[1].identity));
    let thread = second.thread().unwrap();
    let key = thread.thread().get();
    let cell = debug_bounds(cx, format!("view-thread-{key}")).expect("the second Pane is shown");

    let right = |at: gpui::Point<Pixels>, cx: &mut gpui::VisualTestContext| {
        cx.simulate_event(gpui::MouseDownEvent {
            button: gpui::MouseButton::Right,
            position: at,
            modifiers: gpui::Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        cx.simulate_event(gpui::MouseUpEvent {
            button: gpui::MouseButton::Right,
            position: at,
            modifiers: gpui::Modifiers::none(),
            click_count: 1,
        });
        tick(cx);
    };
    right(cell.center(), cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.context_menu().map(|menu| menu.target)),
        Some(MenuTarget::Pane(thread)),
        "its cell's right press opens its own Thread's menu"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);

    let dir = scratch("popups-second-pane-files");
    std::fs::create_dir_all(&dir).unwrap();
    let notes = dir.join("notes.txt");
    std::fs::write(&notes, "notes").unwrap();
    let editor = cx.debug_bounds("focused-prompt-editor").expect("its Composer").center();
    cx.simulate_event(gpui::FileDropEvent::Entered {
        position: editor,
        paths: gpui::ExternalPaths(vec![notes.clone()].into()),
    });
    tick(cx);
    assert_eq!(view.read_with(cx, |view, _| view.file_drop_over), Some(second), "the files are over its box");
    cx.simulate_event(gpui::FileDropEvent::Pending { position: editor });
    cx.simulate_event(gpui::FileDropEvent::Submit { position: editor });
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.file_drop_over, None, "the drop ends the hover");
        assert_eq!(view.panes[1].read(cx).composer.read(cx).files(), [notes.clone()]);
        assert!(view.panes[0].read(cx).composer.read(cx).files().is_empty());
    });

    press(cx, "composer-attach".into());
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| matches!(view.popup(), Some(Popup::Attach { pane, .. }) if *pane == second)),
        "its + opens its own menu"
    );
    press(cx, "choice-row-2".into());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[1].read(cx).composer.read(cx).text(), "/", "the / lands in its box");
        assert_eq!(view.panes[0].read(cx).composer.read(cx).text(), "");
    });
    cx.simulate_keystrokes("escape");
    tick(cx);

    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    let card = cx.debug_bounds("details-card").expect("its details card");
    right(card.center(), cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.context_menu().map(|menu| menu.target)),
        Some(MenuTarget::Details(thread)),
        "the card's right press opens its own card's menu"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    let pr = (0..4)
        .filter_map(|at| debug_bounds(cx, format!("details-item-branch-{at}")))
        .last()
        .expect("its PR's row");
    cx.simulate_click(pr.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        let number = pr_with_checks().number;
        assert_eq!(view.active_surface(thread), Some(SurfaceId::PullRequest(number)), "its PR opens in its Panel");
        assert_ne!(view.active_surface(first.thread().unwrap()), Some(SurfaceId::PullRequest(number)));
    });
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);

    view.update(cx, |view, cx| {
        let composer = view.panes[1].read(cx).composer.clone();
        let reading = view.prefs.settings.reading_size;
        let review = cx.new(|cx| {
            ferrite_review::ReviewView::new(composer, ferrite_reviewr::model::Scope::Uncommitted, reading, cx)
        });
        view.panes[1].review.show(review, "main".into());
        cx.notify();
    });
    for _ in 0..64 {
        let frames = cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        if frames == 0 {
            break;
        }
        cx.executor().advance_clock(Duration::from_millis(16));
    }
    let pane_x = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            view.read(cx)
                .pane_rects(window, cx)
                .into_iter()
                .find(|(at, _)| *at == 1)
                .expect("the second Pane is on the board")
                .1
                .x
        })
    };
    let before = pane_x(cx);
    let slot = cx.debug_bounds("review-slot").expect("its Review beside it");
    let cell = debug_bounds(cx, format!("view-thread-{key}")).expect("the second Pane");
    let head = gpui::point(slot.center().x, slot.top() + px(12.));
    cx.simulate_mouse_down(head, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(head + gpui::point(px(-20.), px(0.)), Some(gpui::MouseButton::Left), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_mouse_move(cell.center(), Some(gpui::MouseButton::Left), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.dragging, Some(crate::cockpit::reader_leaf(thread)), "its Review is lifted");
        assert_eq!(
            view.drop_preview,
            Some((crate::cockpit::pane_leaf(second), Zone::Swap)),
            "over its cell's middle, the drop would swap the two"
        );
    });
    cx.simulate_mouse_up(cell.center(), gpui::MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!((view.dragging, view.drop_preview), (None, None), "the drop ends the drag");
    });
    assert!(pane_x(cx) > before, "the Review and the Pane swapped: {before} -> {}", pane_x(cx));

    cx.simulate_keystrokes("cmd-n");
    tick(cx);
    let draft = view.read_with(cx, |view, cx| view.panes[view.focused(cx)].identity);
    assert!(matches!(draft, PaneIdentity::Draft(_)), "a draft, the third Pane");
    press(cx, "start-hero-project".into());
    tick(cx);
    view.read_with(cx, |view, _| {
        let menu = view.band_menu().expect("the hero word drops a menu");
        assert_eq!((menu.pane, menu.kind.chip), (draft, ferrite_pane::pane::BandChip::Project));
    });
}
