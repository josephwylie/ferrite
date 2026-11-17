//! The Thread's row menu in Claude's order (claude/row-menu.png): `Open in
//! ›` and `Move to group ›` submenus beside it, Mark as unread, Fork, Copy
//! link, and Park as Claude's Archive.
#[allow(unused_imports)]
use super::*;

fn labels(view: &CockpitView, thread: ThreadId) -> Vec<String> {
    view.context_rows(MenuTarget::Thread(thread))
        .into_iter()
        .flatten()
        .map(|(item, _)| item.label.to_string())
        .collect()
}

fn row_of(rows: &[Option<(ferrite_ui::menu::Item, MenuVerb)>], verb: MenuVerb) -> Option<usize> {
    rows.iter()
        .position(|row| matches!(row, Some((_, found)) if *found == verb))
}

/// Claude's groups, rule between each: where it opens; its own verbs; its
/// paths; its PR; its place; its end — Park, Ferrite's archive, on `A`.
#[gpui::test]
fn the_row_menu_reads_in_claudes_order(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("row-menu-order", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        assert_eq!(
            labels(view, thread),
            [
                "Open in",
                "Pin",
                "Rename",
                "Toggle fullscreen",
                "New thread in this project",
                "Copy path",
                "Copy thread link",
                "Refresh PR status",
                "Snooze",
                "Park thread",
                "Delete thread",
            ]
        );
        let rows = view.context_rows(MenuTarget::Thread(thread));
        assert!(rows[1].is_none(), "a rule under `Open in`");
        assert!(rows
            .windows(2)
            .all(|pair| pair[0].is_some() || pair[1].is_some()));
        assert!(rows.last().unwrap().is_some(), "no rule at the foot");
        let park = rows
            .iter()
            .flatten()
            .find(|(_, verb)| *verb == MenuVerb::Park);
        assert_eq!(park.unwrap().0.shortcut.as_deref(), Some("A"));
    });
}

/// `Open in ›` opens its submenu beside the menu, `SUBMENU_GAP` off its
/// right edge and level with its row: Finder and Terminal. Resting on
/// another row closes it.
#[gpui::test]
fn open_in_opens_its_submenu_beside_the_menu(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("row-menu-open-in", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        view.open_context_menu(
            MenuTarget::Thread(thread),
            gpui::point(px(300.), px(200.)),
            cx,
        )
    });
    tick(cx);
    let parent = cx.debug_bounds("context-menu-row-0").expect("Open in");
    cx.simulate_mouse_move(parent.center(), None, gpui::Modifiers::none());
    tick(cx);
    let menu = cx.debug_bounds("context-menu").expect("the menu");
    let side = cx
        .debug_bounds("context-submenu")
        .expect("the pointer opens it");
    assert!(
        (side.left() - menu.right() - px(ferrite_ui::theme::SUBMENU_GAP)).abs() <= px(1.),
        "{side:?} beside {menu:?}"
    );
    let first = cx.debug_bounds("context-submenu-row-0").unwrap();
    assert!(
        (first.top() - parent.top()).abs() <= px(1.),
        "{first:?} level with {parent:?}"
    );
    view.read_with(cx, |view, _| {
        let (_, rows) = view.context_menu.as_ref().unwrap().sub.clone().unwrap();
        let names: Vec<_> = rows
            .iter()
            .map(|(item, _)| item.label.to_string())
            .collect();
        assert_eq!(names, ["Finder", "Terminal"]);
        assert_eq!(rows[0].1, MenuVerb::Reveal);
        assert_eq!(rows[1].1, MenuVerb::OpenTerminal);
    });
    let pin = cx.debug_bounds("context-menu-row-2").expect("Pin");
    cx.simulate_mouse_move(pin.center(), None, gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.debug_bounds("context-submenu").is_none(),
        "another row closes it"
    );
}

/// A closed menu leaves with its short exit: the state is gone at once (so
/// input is released), the face stays drawn until `MOTION_MENU_OUT_MS`
/// passes and takes no press, and with motion reduced it is gone at once.
#[gpui::test]
fn a_closed_menu_fades_out_and_takes_no_input(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let (core, _fake) = cockpit("row-menu-exit", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let open = |cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| {
            view.open_context_menu(
                MenuTarget::Thread(thread),
                gpui::point(px(300.), px(200.)),
                cx,
            )
        });
        tick(cx);
    };
    let step = |ms: u64, cx: &mut gpui::VisualTestContext| {
        cx.executor().advance_clock(Duration::from_millis(ms));
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    };
    open(cx);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    step(16, cx);
    assert!(
        view.read_with(cx, |view, _| view.context_menu.is_none()),
        "input is released at once"
    );
    assert!(cx.debug_bounds("context-menu").is_some(), "the face is still leaving");
    step(ferrite_ui::theme::MOTION_MENU_OUT_MS, cx);
    step(16, cx);
    assert!(cx.debug_bounds("context-menu").is_none(), "and gone once it has");

    open(cx);
    cx.simulate_keystrokes("escape");
    open(cx);
    step(16, cx);
    assert!(view.read_with(cx, |view, _| view.context_menu.is_some()));
    assert_eq!(
        cx.debug_bounds("context-menu").iter().count(),
        1,
        "a reopen mid-exit shows one face"
    );

    cx.update(|_, cx| cx.set_reduce_motion(true));
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(cx.debug_bounds("context-menu").is_none(), "reduced motion snaps shut");
}

/// The menu is worked from the keyboard: ↓ and ↑ walk its rows (past the
/// rules, wrapping), → steps into a submenu and ← out of it, ⏎ presses the
/// cursor row and esc closes the menu.
#[gpui::test]
fn the_row_menu_is_walked_with_the_arrow_keys(cx: &mut TestAppContext) {
    use crate::cockpit::context_menu::MenuCursor;
    let (core, _fake) = cockpit("row-menu-keys", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::Thread(thread), gpui::point(px(300.), px(200.)), cx)
    });
    tick(cx);
    let cursor = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.context_menu.as_ref().map(|open| open.cursor))
    };
    assert_eq!(cursor(cx), Some(MenuCursor::Idle));
    cx.simulate_keystrokes("down");
    tick(cx);
    assert_eq!(cursor(cx), Some(MenuCursor::Row(0)), "down lands on the first row");
    cx.simulate_keystrokes("down");
    tick(cx);
    assert_eq!(cursor(cx), Some(MenuCursor::Row(2)), "and steps over the rule");
    cx.simulate_keystrokes("up up");
    tick(cx);
    let last = view.read_with(cx, |view, _| {
        view.context_menu.as_ref().unwrap().rows.len() - 1
    });
    assert!(
        matches!(cursor(cx), Some(MenuCursor::Row(at)) if at >= last - 1),
        "up wraps to the end"
    );
    cx.simulate_keystrokes("down");
    tick(cx);
    assert_eq!(cursor(cx), Some(MenuCursor::Row(0)), "down wraps to the start");
    cx.simulate_keystrokes("right");
    tick(cx);
    assert_eq!(cursor(cx), Some(MenuCursor::SubRow(0)), "right enters Open in");
    assert!(cx.debug_bounds("context-submenu").is_some());
    cx.simulate_keystrokes("down");
    tick(cx);
    assert_eq!(cursor(cx), Some(MenuCursor::SubRow(1)));
    cx.simulate_keystrokes("left");
    tick(cx);
    assert_eq!(cursor(cx), Some(MenuCursor::Row(0)), "left returns to the parent");
    assert!(cx.debug_bounds("context-submenu").is_none());
    cx.simulate_keystrokes("down down enter");
    tick(cx);
    assert_eq!(cursor(cx), None, "enter pressed the row and closed the menu");
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::Thread(thread), gpui::point(px(300.), px(200.)), cx)
    });
    tick(cx);
    cx.simulate_keystrokes("down escape");
    tick(cx);
    assert_eq!(cursor(cx), None, "escape closes it");
}

/// An open menu joins the root's key context instead of replacing it, so the
/// keymap decides what stands down. The Cockpit's own ↵, ⎋ and ⇥ do (the
/// menu's ↵ presses its row, ⎋ closes it), and a chord bound for every
/// context, ⌘,, still acts.
#[gpui::test]
fn an_open_menu_joins_the_root_context_and_global_chords_still_act(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("row-menu-context", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::Thread(thread), gpui::point(px(300.), px(200.)), cx)
    });
    tick(cx);
    let root = cx.update(|window, _| window.context_stack());
    assert!(
        root.iter()
            .any(|context| context.contains("Ferrite") && context.contains("ContextMenu")),
        "the menu is added to the root's context, not put in its place: {root:?}"
    );
    cx.simulate_keystrokes("tab");
    tick(cx);
    assert!(
        view.read_with(cx, |view, _| view.context_menu.is_some()),
        "the Cockpit's ⇥ stands down for the open menu"
    );
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.settings_open, "⌘, is bound for every context and still acts");
    });
}

/// A row's `⋯` menu hangs under its button, its right edge on the
/// button's. Where the window has less room below than the rows need, the
/// menu keeps that place and scrolls; where the room cannot show two rows
/// it hangs over the button instead. Never past the window's edge, never
/// on its own button.
#[gpui::test]
fn a_menu_from_a_button_stays_in_the_window(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("row-menu-in-window", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let gap = px(ferrite_ui::theme::MENU_UNDER_GAP);
    let open_under = |cx: &mut gpui::VisualTestContext, top: f32| {
        let button =
            gpui::Bounds::new(gpui::point(px(300.), px(top)), gpui::size(px(24.), px(24.)));
        view.update(cx, |view, cx| {
            view.open_context_menu_under(MenuTarget::Thread(thread), button, cx)
        });
        tick(cx);
        let menu = cx.debug_bounds("context-menu").expect("the menu");
        view.update(cx, |view, cx| {
            view.context_menu = None;
            cx.notify();
        });
        tick(cx);
        (button, menu)
    };

    let (button, whole) = open_under(cx, 100.);
    assert!(
        (whole.top() - button.bottom() - gap).abs() <= px(1.),
        "under it: {whole:?} {button:?}"
    );
    assert!((whole.right() - button.right()).abs() <= px(1.));

    let rows = px(2. * (ferrite_ui::theme::FLOAT_ROW_H + ferrite_ui::theme::FLOAT_PAD));
    let no_room_below = 800. - f32::from(whole.size.height);
    for top in [no_room_below, 560.] {
        let (button, menu) = open_under(cx, top);
        assert!(
            (menu.top() - button.bottom() - gap).abs() <= px(1.),
            "still under it from {top}: {menu:?} {button:?}"
        );
        assert!(
            menu.bottom() <= px(800.),
            "in the window from {top}: {menu:?}"
        );
        assert!(
            menu.size.height < whole.size.height,
            "scrolls in the room it has from {top}: {menu:?}"
        );
    }

    for top in [700., 770.] {
        let (button, menu) = open_under(cx, top);
        assert!(
            menu.top() >= px(0.) && menu.bottom() <= px(800.),
            "in the window from {top}: {menu:?}"
        );
        assert!(
            (button.top() - menu.bottom() - gap).abs() <= px(1.),
            "over its button, clear of it from {top}: {menu:?} {button:?}"
        );
        assert!(
            (menu.right() - button.right()).abs() <= px(1.),
            "on the button's right edge from {top}: {menu:?} {button:?}"
        );
        assert!(
            menu.size.height >= rows,
            "tall enough to use from {top}: {menu:?}"
        );
    }
}

/// Mark as unread, for a Thread off the screen that has been read: its
/// newest Notice needs the operator again, until it is next seen.
#[gpui::test]
fn mark_as_unread_brings_back_a_read_threads_notice(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("row-menu-unread", 3);
    let threads = core.threads();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    fake.streams.borrow()[2]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    assert!(
        row_of(
            &view.read_with(cx, |view, _| view
                .context_rows(MenuTarget::Thread(threads[2]))),
            MenuVerb::MarkUnread
        )
        .is_none(),
        "already unread"
    );
    view.update(cx, |view, _| view.cockpit.mark_notices_read());
    tick(cx);
    let row = view.read_with(cx, |view, _| {
        assert!(!view.cockpit.notifications().attention(threads[2]), "read");
        row_of(
            &view.context_rows(MenuTarget::Thread(threads[2])),
            MenuVerb::MarkUnread,
        )
        .expect("offered once read")
    });
    view.update_in(cx, |view, window, cx| {
        view.open_context_menu(
            MenuTarget::Thread(threads[2]),
            gpui::point(px(10.), px(10.)),
            cx,
        );
        view.press_menu_row(row, window, cx);
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.cockpit.notifications().attention(threads[2]),
            "unread again"
        );
    });
}

/// Copy link copies the Thread in the form the CLI takes; `Move to group ›`
/// lists the other Groups, and its row moves the Thread into one.
#[gpui::test]
fn copy_link_and_move_to_group(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("row-menu-move", 3);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update_in(cx, |view, window, cx| {
        view.run_menu_verb(
            MenuTarget::Thread(threads[2]),
            MenuVerb::CopyLink,
            window,
            cx,
        )
    });
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(format!("ferrite thread focus {}", threads[2].get()))
    );
    let parent = view.read_with(cx, |view, _| {
        let move_to = MenuVerb::Submenu(super::super::context_menu::Submenu::MoveToGroup);
        assert!(
            row_of(&view.context_rows(MenuTarget::Thread(threads[0])), move_to).is_none(),
            "no other Group to move to"
        );
        row_of(&view.context_rows(MenuTarget::Thread(threads[2])), move_to)
            .expect("a loose Thread can join the Group")
    });
    view.update_in(cx, |view, window, cx| {
        view.open_context_menu(
            MenuTarget::Thread(threads[2]),
            gpui::point(px(10.), px(10.)),
            cx,
        );
        view.press_menu_row(parent, window, cx);
        let (_, rows) = view
            .context_menu
            .as_ref()
            .unwrap()
            .sub
            .clone()
            .expect("open");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, MenuVerb::JoinGroup(group));
        view.press_submenu_row(parent, 0, window, cx);
        assert!(view.context_menu.is_none(), "the pick closes the menu");
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.groups().of(threads[2]).map(|g| g.id),
            Some(group)
        );
    });
}
