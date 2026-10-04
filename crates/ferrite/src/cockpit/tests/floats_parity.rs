//! The floats package's cockpit-level tests (FL-1…FL-19): the palette, its
//! keys and commands, ⌘G, the shortcuts sheet, ⇧⇥, ⌘⌫, ⌘1…⌘9, the
//! notifications list, the bell's badge, the toasts and the key contexts
//! an empty Composer line adds.
#[allow(unused_imports)]
use super::*;
use crate::palette::{Act, Command, PaletteScope, Section};

/// A Bash approval, as Claude asks for one.
fn bash(id: &str, command: &str) -> SessionEvent {
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind: Default::default(),
            policy: Default::default(),
            id: id.into(),
            tool_use_id: format!("toolu_{id}"),
            tool_name: "Bash".into(),
            description: command.into(),
            input: serde_json::json!({ "command": command }),
            suggestions: vec![],
        },
    }
}

/// The first binding the focused node's context stack resolves `keys` to.
fn resolves(keys: &str, cx: &mut gpui::VisualTestContext) -> Option<&'static str> {
    cx.update(|window, cx| {
        let stack = window.context_stack();
        let keystroke = gpui::Keystroke::parse(keys).unwrap();
        let (bindings, _) = cx
            .key_bindings()
            .borrow()
            .bindings_for_input(&[keystroke], &stack);
        bindings.first().map(|binding| binding.action().name())
    })
}

fn palette_open(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> bool {
    view.read_with(cx, |view, _| view.floats.palette.is_some())
}

/// FL-1: ⌘K opens the palette, 84 cells wide, centred on the board, its
/// head 56px under the board's top, over a veil that covers the board and
/// nothing else; esc closes it without interrupting the running turn, and
/// ⌘K toggles it.
#[gpui::test]
fn cmd_k_opens_the_palette_centred_over_the_board(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("palette-open", 1);
    let thread = core.threads()[0];
    core.send(thread, "keep working".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    assert!(palette_open(&view, cx));
    let palette = cx.debug_bounds("palette").expect("the palette is up");
    let board = cx.update(|window, cx| view.read(cx).board_bounds(window));
    assert!(
        (palette.size.width - px(crate::theme::PALETTE_W)).abs() <= px(1.),
        "84 cells: {palette:?}"
    );
    assert!(
        (palette.center().x - px(board.x + board.w / 2.)).abs() <= px(1.),
        "centred on the board: {palette:?} / {board:?}"
    );
    assert!(
        (palette.top() - px(board.y + crate::theme::PALETTE_TOP)).abs() <= px(1.),
        "56px under the board's top: {palette:?}"
    );
    let veil = cx.debug_bounds("palette-veil").expect("the veil");
    assert!(
        (veil.left() - px(board.x)).abs() <= px(1.)
            && (veil.top() - px(board.y)).abs() <= px(1.)
            && (veil.size.width - px(board.w)).abs() <= px(1.)
            && (veil.size.height - px(board.h)).abs() <= px(1.),
        "the veil covers the board only: {veil:?} / {board:?}"
    );
    let input = cx.debug_bounds("palette-input").expect("the input row");
    assert!(
        (input.size.height - px(crate::theme::PALETTE_INPUT_H)).abs() <= px(1.),
        "a row and 12px: {input:?}"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!palette_open(&view, cx), "esc closes the palette");
    assert_eq!(*fake.interrupts.borrow(), 0, "and interrupts nothing");
    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    assert!(palette_open(&view, cx));
    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    assert!(!palette_open(&view, cx), "⌘K toggles");
    // A press on the veil closes it too.
    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    let veil = cx.debug_bounds("palette-veil").unwrap();
    cx.simulate_click(
        gpui::point(veil.left() + px(8.), veil.bottom() - px(8.)),
        gpui::Modifiers::none(),
    );
    tick(cx);
    assert!(!palette_open(&view, cx), "the veil closes the palette");
    assert_eq!(*fake.interrupts.borrow(), 0);
}

/// FL-3 / FL-4: `par` keeps exactly `park thread`, `show parked` and
/// `compare with main` among the commands, in the table's order, and the
/// parked Thread among the Threads, its name lit from the start.
#[gpui::test]
#[allow(clippy::single_range_in_vec_init)]
fn par_keeps_three_commands_and_the_parked_thread(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("palette-par", 3);
    let threads = core.threads();
    core.rename_thread(threads[2], "Release notes").unwrap();
    core.park(threads[2]).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    cx.simulate_input("par");
    tick(cx);
    view.read_with(cx, |view, cx| {
        let palette = view.floats.palette.as_ref().expect("open").read(cx);
        let survivors = palette.survivors();
        let commands: Vec<&str> = survivors
            .iter()
            .filter(|row| row.section == Section::Commands)
            .map(|row| row.name.as_ref())
            .collect();
        assert_eq!(
            commands,
            ["park thread", "show parked", "compare with main"]
        );
        let threads: Vec<&str> = survivors
            .iter()
            .filter(|row| row.section == Section::Threads)
            .map(|row| row.name.as_ref())
            .collect();
        assert_eq!(threads, ["Parked \u{b7} Release notes"]);
        assert_eq!(palette.lit(0), Some(&[0..3][..]), "`Par` lit");
        let park = survivors
            .iter()
            .find(|row| row.name.as_ref() == "park thread")
            .unwrap();
        assert_eq!(park.keys.as_deref(), Some("cmd-\u{232b}"), "⌘⌫");
    });
}

/// FL-2: ↑↓ walk the rows and wrap; ⏎ lands on the cursor's Thread and
/// closes; ⇥ on a command does nothing, and on a Thread it closes.
#[gpui::test]
fn palette_keys_wrap_and_land_on_a_thread(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("palette-keys", 2);
    let threads = core.threads();
    core.rename_thread(threads[1], "Second thread").unwrap();
    let group = group_all(&mut core);
    core.enter_group(group).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    let selected = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let palette = view.floats.palette.as_ref().unwrap().read(cx);
            palette.selected().map(|row| row.act.clone())
        })
    };
    let (first, last) = view.read_with(cx, |view, cx| {
        let palette = view.floats.palette.as_ref().unwrap().read(cx);
        let rows = palette.survivors();
        (
            rows.first().map(|row| row.act.clone()),
            rows.last().map(|row| row.act.clone()),
        )
    });
    assert_eq!(selected(cx), first, "the cursor opens on the first row");
    cx.simulate_keystrokes("up");
    tick(cx);
    assert_eq!(selected(cx), last, "↑ wraps to the last");
    cx.simulate_keystrokes("down");
    tick(cx);
    assert_eq!(selected(cx), first, "↓ wraps back");

    // ⇥ on a command row stays.
    cx.simulate_input("settings");
    tick(cx);
    assert_eq!(selected(cx), Some(Act::Command(Command::Settings)));
    cx.simulate_keystrokes("tab");
    tick(cx);
    assert!(palette_open(&view, cx), "⇥ previews only a Thread");
    cx.simulate_keystrokes("escape");
    tick(cx);

    // ⏎ on a Thread lands on it.
    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    cx.simulate_input("Second");
    tick(cx);
    assert_eq!(selected(cx), Some(Act::Thread(threads[1])));
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(!palette_open(&view, cx));
    assert_eq!(
        view.read_with(cx, |view, _| view.focused_thread()),
        Some(threads[1])
    );

    // ⇥ on a Thread opens it beside, and closes.
    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    cx.simulate_input("Second");
    tick(cx);
    cx.simulate_keystrokes("tab");
    tick(cx);
    assert!(!palette_open(&view, cx), "⇥ on a Thread closes the palette");
}

/// FL-2: a Thread's row reads its scope and state, the board's Threads
/// carry their ⌘ digit in head order, and the rows run Project by Project.
#[gpui::test]
fn thread_rows_name_their_scope_state_and_digit(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("palette-rows", 2);
    let threads = core.threads();
    core.send(threads[0], "work".into());
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Working on it".into(),
        })
        .unwrap();
    core.pump();
    let group = group_all(&mut core);
    core.apply_group(GroupChange::Rename {
        group,
        title: "Perf sweep".into(),
    })
    .unwrap();
    core.enter_group(group).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let order: Vec<ThreadId> = cx.update(|window, cx| {
        let view = view.read(cx);
        view.board_order(window)
            .into_iter()
            .map(|index| view.panes[index].thread().unwrap())
            .collect()
    });
    let rows = cx.update(|window, cx| view.read(cx).palette_thread_rows(window));
    for (at, thread) in order.iter().enumerate() {
        let row = rows
            .iter()
            .find(|row| row.act == Act::Thread(*thread))
            .expect("every Thread has a row");
        assert_eq!(
            row.keys.as_deref(),
            Some(format!("cmd-{}", at + 1).as_str())
        );
        assert!(
            row.context.starts_with("perf sweep"),
            "the shown Group names its Threads: {:?}",
            row.context
        );
    }
    let working = rows
        .iter()
        .find(|row| row.act == Act::Thread(threads[0]))
        .unwrap();
    assert_eq!(working.context.as_ref(), "perf sweep \u{b7} working");
    assert_eq!(working.dot, Some(crate::palette::Dot::Working));
}

/// FL-5: ⌘G opens the palette on the Groups alone; ⏎ enters the Group. ⌘⇧G
/// is New Group.
#[gpui::test]
fn cmd_g_opens_the_groups_and_cmd_shift_g_founds_one(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("palette-groups", 3);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    core.focus_thread(threads[2]);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-g");
    tick(cx);
    view.read_with(cx, |view, cx| {
        let palette = view.floats.palette.as_ref().expect("⌘G opens").read(cx);
        assert_eq!(palette.scope, PaletteScope::Groups);
        let rows = palette.survivors();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].section, Section::Groups);
        assert!(
            rows[0].context.ends_with("2 threads"),
            "{:?}",
            rows[0].context
        );
    });
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.cockpit.roster().view()),
        View::Group(group)
    );

    // ⌘⇧G: the focused loose Thread and a new draft, founding a Group.
    view.update(cx, |view, cx| {
        let index = view.pane_for(threads[2]);
        if let Some(index) = index {
            view.focus_pane(index);
        }
        view.land_on_thread(threads[2], cx);
    });
    tick(cx);
    let before = view.read_with(cx, |view, _| view.panes.len());
    cx.simulate_keystrokes("cmd-shift-g");
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.panes.len()) > before,
        "New Group opens its draft"
    );
}

/// FL-6: ⌘⌫ on an empty line parks the Thread; with text it deletes to
/// the line's start.
#[gpui::test]
fn cmd_backspace_parks_from_an_empty_line_and_deletes_with_text(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("cmd-backspace", 2);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let open = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.cockpit.threads().len())
    };
    cx.simulate_input("abc def");
    tick(cx);
    cx.simulate_keystrokes("cmd-backspace");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(
            view.panes[view.focused()].composer.read(cx).is_empty(),
            "text: delete to start"
        );
    });
    assert_eq!(open(cx), 2, "nothing parked while the line had text");
    cx.simulate_keystrokes("cmd-backspace");
    tick(cx);
    assert_eq!(open(cx), 1, "an empty line parks");
}

/// FL-8: `?` on an empty line opens the shortcuts sheet (esc closes it);
/// with text on the line `?` opens nothing.
#[gpui::test]
fn question_mark_opens_the_shortcuts_sheet_on_an_empty_line(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("shortcuts-sheet", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let open = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.floats.shortcuts.is_some())
    };
    assert_eq!(resolves("?", cx), Some("shortcuts::Toggle"));
    cx.simulate_keystrokes("?");
    tick(cx);
    assert!(open(cx));
    assert!(cx.debug_bounds("shortcuts-sheet").is_some());
    assert!(
        cx.debug_bounds("palette-veil").is_some(),
        "the palette's veil"
    );
    cx.simulate_keystrokes("down");
    tick(cx);
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!open(cx));
    assert_eq!(*fake.interrupts.borrow(), 0);
    cx.simulate_input("why");
    tick(cx);
    assert_eq!(resolves("?", cx), None, "a typed `?` is just a character");
}

/// FL-9: ⇧⇥ steps the Session's permission modes through its own control.
#[gpui::test]
fn shift_tab_cycles_the_permission_mode(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("mode-cycle", 1);
    *fake.native_controls.borrow_mut() = true;
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    assert_eq!(resolves("shift-tab", cx), Some("status::CycleMode"));
    cx.simulate_keystrokes("shift-tab");
    tick(cx);
    assert!(fake
        .controls
        .borrow()
        .contains(&ferrite_core::SessionControl::SetPermissionMode {
            mode: "native-mode".into()
        }));
}

/// FL-16 / FL-17: the list hangs 4px under the bell with its first row
/// selected; ↑↓ wrap, ⌫ dismisses the cursor's row, esc closes without
/// interrupting, `mark all read` reads everything and removes nothing; the
/// badge counts the unread waiting requests only.
#[gpui::test]
fn the_notifications_list_walks_dismisses_and_marks_read(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("bell-list", 6);
    let threads = core.threads();
    for thread in &threads[3..6] {
        core.send(*thread, "go".into());
    }
    core.focus_thread(threads[0]);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let stream = |thread: ThreadId, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.cockpit
                .threads()
                .into_iter()
                .position(|open| open == thread)
                .unwrap()
        })
    };
    for (n, thread) in threads[1..3].iter().enumerate() {
        let at = stream(*thread, cx);
        fake.streams.borrow()[at]
            .send(bash(&format!("req{n}"), "gh issue close 212 --reason x"))
            .unwrap();
    }
    for thread in &threads[3..6] {
        let at = stream(*thread, cx);
        fake.streams.borrow()[at]
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
    }
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.notifications().unread_requests(),
            2,
            "2 waiting; 3 unread completions never count"
        );
    });
    assert!(cx.debug_bounds("notifications-badge").is_some());
    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    let door = cx.debug_bounds("notifications-bell").unwrap();
    let panel = cx
        .debug_bounds("notifications-panel")
        .expect("the list is down");
    assert!(
        (panel.top() - (door.bottom() + px(crate::theme::NOTICE_PANEL_GAP))).abs() <= px(1.),
        "4px under the bell: {panel:?} / {door:?}"
    );
    assert!(
        (panel.left() - door.left()).abs() <= px(1.),
        "on the bell's left"
    );
    let rows = view.read_with(cx, |view, _| view.bell_rows());
    assert_eq!(rows.len(), 5);
    assert!(rows[0].request() && rows[1].request(), "requests first");
    assert_eq!(
        rows[0].detail.text(),
        "Bash wants to run gh issue close 212"
    );
    assert_eq!(view.read_with(cx, |view, _| view.bell.cursor), 0);
    cx.simulate_keystrokes("up");
    tick(cx);
    assert_eq!(view.read_with(cx, |view, _| view.bell.cursor), 4, "↑ wraps");
    cx.simulate_keystrokes("down");
    tick(cx);
    assert_eq!(view.read_with(cx, |view, _| view.bell.cursor), 0, "↓ wraps");
    cx.simulate_keystrokes("backspace");
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.bell_rows().len()),
        4,
        "⌫ dismisses"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!view.read_with(cx, |view, _| view.bell.open));
    assert_eq!(*fake.interrupts.borrow(), 0);
    cx.simulate_keystrokes("cmd-i");
    tick(cx);
    let mark = cx
        .debug_bounds("notifications-mark-read")
        .expect("`mark all read` always shows");
    cx.simulate_click(mark.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        let notifications = view.cockpit.notifications();
        assert_eq!(notifications.unread(), 0);
        assert_eq!(notifications.unread_requests(), 0);
        assert_eq!(view.bell_rows().len(), 4, "nothing is removed");
    });
    assert!(
        cx.debug_bounds("notifications-badge").is_none(),
        "the badge clears"
    );
}

/// FL-18: a request's toast answers with its buttons — `1 allow`, `3
/// deny` — through the approval answer, and leaves after each.
#[gpui::test]
fn a_toasts_quick_answers_answer_the_request_and_leave(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("toast-answers", 2);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let (off, at) = view.read_with(cx, |view, _| {
        let threads = view.cockpit.threads();
        let at = threads
            .iter()
            .position(|thread| Some(*thread) != view.focused_thread())
            .unwrap();
        (threads[at], at)
    });
    fake.streams.borrow()[at]
        .send(bash("one", "gh issue close 212 --reason x"))
        .unwrap();
    tick(cx);
    tick(cx);
    let allow: &'static str = format!("toast-allow-{}", off.get()).leak();
    let button = cx.debug_bounds(allow).expect("`1 allow`");
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(fake
        .answered
        .borrow()
        .iter()
        .any(|(id, answer)| id == "one" && matches!(answer, DecisionAnswer::Allow { .. })));
    view.read_with(cx, |view, _| assert!(view.bell.toasts().is_empty()));

    fake.streams.borrow()[at]
        .send(bash("two", "rm -rf target"))
        .unwrap();
    tick(cx);
    tick(cx);
    let deny: &'static str = format!("toast-deny-{}", off.get()).leak();
    let button = cx.debug_bounds(deny).expect("`3 deny`");
    cx.simulate_click(button.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(fake
        .answered
        .borrow()
        .iter()
        .any(|(id, answer)| id == "two" && matches!(answer, DecisionAnswer::Deny { .. })));
    view.read_with(cx, |view, _| assert!(view.bell.toasts().is_empty()));
}

/// FL-19 / FL-12: an empty Composer line under a Decision resolves the
/// Decision keys (↑↓ ⏎ esc ⇥), the number keys and `?`; with text on the
/// line every key edits it, and a typed number is a character.
#[gpui::test]
fn decision_keys_resolve_only_on_an_empty_line(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("decision-keys", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    fake.streams.borrow()[0].send(decision("keys")).unwrap();
    tick(cx);
    for (keys, action) in [
        ("up", "decision::SelectPrevious"),
        ("down", "decision::SelectNext"),
        ("enter", "decision::Confirm"),
        ("escape", "decision::Dismiss"),
        ("tab", "decision::Amend"),
        ("1", "cockpit::PickOption1"),
        ("3", "cockpit::PickOption3"),
        ("cmd-backspace", "cockpit::CloseThread"),
    ] {
        assert_eq!(resolves(keys, cx), Some(action), "{keys} on an empty line");
    }
    cx.simulate_input("go");
    tick(cx);
    assert_eq!(
        resolves("enter", cx),
        Some("cockpit::Submit"),
        "text: ⏎ sends"
    );
    assert_eq!(
        resolves("cmd-backspace", cx),
        Some("composer::DeleteToStart"),
        "text: ⌘⌫ deletes"
    );
    cx.simulate_keystrokes("1");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.panes[view.focused()].composer.read(cx).text(),
            "go1",
            "a typed number is a character"
        );
    });
}

/// FL-19: at wall range the number keys answer the flagged Thread.
#[gpui::test]
fn wall_numbers_bind_the_quick_answers(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    cx.update(|cx| {
        let stack = vec![gpui::KeyContext::parse("Ferrite Wall").unwrap()];
        for (keys, action) in [
            ("1", "cockpit::PickOption1"),
            ("2", "cockpit::PickOption2"),
            ("3", "cockpit::PickOption3"),
            ("y", "cockpit::Allow"),
        ] {
            let keystroke = gpui::Keystroke::parse(keys).unwrap();
            let (bindings, _) = cx
                .key_bindings()
                .borrow()
                .bindings_for_input(&[keystroke], &stack);
            assert_eq!(
                bindings.first().map(|binding| binding.action().name()),
                Some(action),
                "{keys} on the wall"
            );
        }
    });
}

/// FL-13: every model with a ladder shows exactly one inverse step — the
/// effort in force, else the CLI's own.
#[gpui::test]
fn the_effort_ladder_always_has_a_chosen_step(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("effort-ladder", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    for model in ferrite_core::providers::models::fallback(Provider::Claude) {
        view.update(cx, |view, _| {
            let _ = view.cockpit.set_model(
                thread,
                Some(model.value.clone()).filter(|value| value != "default"),
            );
        });
        tick(cx);
        let ladder = view.read_with(cx, |view, _| view.effort_ladder(thread));
        match ladder {
            Some((steps, labels, chosen)) => {
                assert_eq!(steps.len(), labels.len());
                assert!(chosen.is_some(), "{} shows its level", model.value);
            }
            None => assert!(model.efforts.is_empty(), "{} has no ladder", model.value),
        }
    }
}
