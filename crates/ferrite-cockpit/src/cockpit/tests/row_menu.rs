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
                "Copy link",
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

/// Mark as unread, for a Thread off the screen that has been read: its
/// newest Notice needs the operator again, until it is next seen.
#[gpui::test]
fn mark_as_unread_brings_back_a_read_threads_notice(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("row-menu-unread", 3);
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
    view.update(cx, |view, cx| view.enter_group(group, cx));
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
