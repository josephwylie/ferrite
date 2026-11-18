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
}

const KINDS: [Kind; 5] = [Kind::Menu, Kind::Mode, Kind::Plan, Kind::Checks, Kind::Session];

impl Kind {
    fn selector(self) -> &'static str {
        match self {
            Kind::Menu => "context-menu",
            Kind::Mode => "mode-menu",
            Kind::Plan => "plan-card",
            Kind::Checks => "context-checks-card",
            Kind::Session => "session-controls-card",
        }
    }
}

/// The popups up now.
fn up(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<Kind> {
    view.read_with(cx, |view, _| {
        [
            (Kind::Menu, view.context_menu.is_some()),
            (Kind::Mode, view.mode_picker.is_some()),
            (Kind::Plan, view.plan_card.is_some()),
            (Kind::Checks, view.context_checks.is_some()),
            (Kind::Session, view.session_controls.is_some()),
        ]
        .into_iter()
        .filter_map(|(kind, up)| up.then_some(kind))
        .collect()
    })
}

fn open(view: &Entity<CockpitView>, kind: Kind, cx: &mut gpui::VisualTestContext) {
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    cx.update(|window, cx| {
        view.update(cx, |view, cx| match kind {
            Kind::Menu => view.open_context_menu(
                MenuTarget::Thread(thread),
                gpui::point(px(300.), px(200.)),
                cx,
            ),
            Kind::Mode => {
                let generation = view.cockpit.thread(thread).unwrap().generation();
                view.toggle_mode_menu(thread, generation, false, cx);
            }
            Kind::Plan => view.open_plan(window, cx),
            Kind::Checks => view.open_checks(window, cx),
            Kind::Session => view.run_command(crate::palette::Command::BackgroundTasks, window, cx),
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
    assert_eq!(core.send(thread, "Ship it".into()), ferrite_core::cockpit::Admission::Delivered);
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
        for thread in view.cockpit.threads().to_vec() {
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
    for kind in [Kind::Menu, Kind::Checks, Kind::Session] {
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
#[ignore = "bug: escape with the mode menu or the plan card up stops the turn"]
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
#[ignore = "bug: escape on the mode menu denies a waiting Decision"]
fn escape_on_the_mode_menu_leaves_a_waiting_decision_unanswered(cx: &mut TestAppContext) {
    let (view, fake, cx) = window(cx, "popups-escape-decision", 1);
    fake.streams.borrow()[0].send(decision("perm_mode")).unwrap();
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let chip = debug_bounds(cx, format!("mode-picker-{}", thread.get())).expect("the mode chip");
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
#[ignore = "bug: the mode menu outlives the model picker it hangs from"]
fn the_mode_menu_from_the_model_picker_goes_before_it_and_with_it(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-mode-from-picker", 1);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let picker_open = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.model_picker_open(PaneIdentity::Thread(thread)))
    };
    let chip = debug_bounds(cx, format!("model-picker-{}", thread.get())).expect("the model chip");
    cx.simulate_click(chip.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(picker_open(cx), "the chip opens the picker");
    let from_picker = |cx: &mut gpui::VisualTestContext| {
        let button = cx.debug_bounds("model-picker-permissions").expect("the picker's mode button");
        cx.simulate_click(button.center(), gpui::Modifiers::none());
        tick(cx);
        assert_eq!(up(&view, cx), [Kind::Mode], "the picker's button opens the mode menu");
    };

    from_picker(cx);
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(up(&view, cx), [], "the first escape closes the menu");
    assert!(picker_open(cx), "and leaves the picker up");

    from_picker(cx);
    view.update(cx, |view, cx| {
        view.popover = None;
        cx.notify();
    });
    tick(cx);
    assert_eq!(up(&view, cx), [], "the picker closing takes the menu");
    assert!(cx.debug_bounds("mode-menu").is_none());
}

/// Opening any popup over any other leaves the new one up alone.
#[gpui::test]
#[ignore = "bug: two popups can be up at once"]
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
#[ignore = "bug: the plan card waits out of sight and comes back"]
fn the_plan_card_closes_when_another_thread_takes_focus(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-plan-focus", 2);
    open(&view, Kind::Plan, cx);
    view.update(cx, |view, cx| {
        view.focus_pane(1);
        cx.notify();
    });
    tick(cx);
    assert_eq!(up(&view, cx), [], "another Thread's focus closes the card");
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    assert_eq!(up(&view, cx), []);
    assert!(cx.debug_bounds("plan-card").is_none(), "the card does not come back");
}

/// A sheet opening closes whatever popup is up.
#[gpui::test]
#[ignore = "bug: the plan card stays up under Settings"]
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

/// A second press on the mode chip shuts its menu and does not open it
/// again.
#[gpui::test]
fn a_second_press_on_the_mode_chip_shuts_its_menu(cx: &mut TestAppContext) {
    let (view, _fake, cx) = window(cx, "popups-mode-chip", 1);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let chip = debug_bounds(cx, format!("mode-picker-{}", thread.get())).expect("the mode chip");
    cx.simulate_click(chip.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(up(&view, cx), [Kind::Mode], "a press opens it");
    cx.simulate_click(chip.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(up(&view, cx), [], "a second press shuts it");
    tick(cx);
    assert_eq!(up(&view, cx), [], "and it stays shut");
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
