//! The nav's parity behaviour: the selection following the board (N-5),
//! the scrollbar beside the rows, the one seam (N-8) and the rail.

use super::*;

fn sorted_threads(core: &Cockpit) -> Vec<ThreadId> {
    let mut threads = core.threads();
    threads.sort();
    threads
}

/// N-5: the one selection is the card of what the board shows — the
/// shown Group's focused member on its board, the Thread's own card in
/// Solo — and only that one.
#[gpui::test]
fn the_selection_follows_what_the_board_shows(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("nav-selection", 3);
    let threads = sorted_threads(&core);
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

    let selected = |view: &CockpitView| -> Vec<ThreadId> {
        view.nav_state()
            .open_rows()
            .filter(|row| row.selected)
            .map(|row| row.thread)
            .collect()
    };
    view.update(cx, |view, cx| {
        view.enter_group(group, cx);
        let focused = view.cockpit.roster().focused_thread().expect("a member");
        assert!(threads[..2].contains(&focused));
        assert_eq!(selected(view), [focused], "the Group's focused member");

        view.focus_thread(threads[2], cx);
        assert_eq!(view.cockpit.roster().view(), View::Solo);
        assert_eq!(selected(view), [threads[2]], "Solo selects the Thread's card");
    });
}

/// The nav's scrollbar sits in the column's right padding, beside the rows
/// and never over them: its strip is the `NAV_SCROLLBAR_GUTTER` at the
/// column's edge, and every row ends before it.
#[gpui::test]
fn the_nav_scrollbar_sits_beside_the_rows(cx: &mut TestAppContext) {
    use ferrite_ui::theme::NAV_SCROLLBAR_GUTTER;
    let (core, _fake) = cockpit("nav-scrollbar-gutter", 2);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    // The panel off, so the board leaves the nav its full column at 1000.
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let column = cx.debug_bounds("nav-column").expect("the column");
    let bar = cx.debug_bounds("nav-scrollbar").expect("the scrollbar");
    // The bar's strip is its layer's right `NAV_SCROLLBAR_GUTTER`: the
    // layer reaches the column's edge, past the rows' padding.
    assert_eq!(bar.right(), column.right(), "at the column's edge");
    let strip = bar.right() - px(NAV_SCROLLBAR_GUTTER);
    for thread in threads {
        let row = cx
            .debug_bounds(format!("nav-thread-{}", thread.get()).leak())
            .expect("a row");
        assert!(row.right() <= strip, "{row:?} ends before the strip");
    }
}

/// N-8 (SPEC §5.1): no seam beside the nav — the card's edge separates —
/// so the board starts at the column's right edge, open and folded.
#[gpui::test]
fn the_board_starts_at_the_nav_edge(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-one-seam", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let column = cx.debug_bounds("nav-column").expect("the column");
    assert!(cx.debug_bounds("nav-seam").is_none(), "no seam");
    assert!(cx.debug_bounds("chrome-seam").is_none(), "no seam");
    view.read_with(cx, |view, _| {
        assert!(
            (view.board_left() - f32::from(column.right())).abs() < 0.5,
            "the board starts at the column's edge"
        );
    });
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            (view.board_left() - nav::FOLDED_WIDTH).abs() < 0.5,
            "folded, beside the rail"
        );
    });
}

/// Folded, the nav is the rail (§5.1.7): one 32px cell per open Thread,
/// each with its mark; the board starts right of it, and a press on
/// a line lands on its Thread. A line's digit is the ⌘ key that focuses
/// it: Solo, only the Solo Pane's `1`.
#[gpui::test]
fn the_folded_nav_is_a_keyed_rail(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-rail", 3);
    let threads = sorted_threads(&core);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    assert!(cx.debug_bounds("nav-rail").is_none(), "open, no rail");
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    cx.executor().advance_clock(Duration::from_millis(400));
    tick(cx);
    let rail = cx.debug_bounds("nav-rail").expect("folded, the rail");
    let mut above = px(0.);
    for thread in &threads {
        let id: &'static str = format!("nav-rail-{}", thread.get()).leak();
        let line = cx.debug_bounds(id).expect("a line per open Thread");
        assert!(line.top() >= above, "in the list's order");
        above = line.bottom();
    }
    let board = cx.update(|window, cx| view.read(cx).board_bounds(window));
    assert!(
        (px(board.x) - rail.right()).abs() <= px(0.5),
        "the board starts right of the rail: {board:?}"
    );
    let keyed = |cx: &mut gpui::VisualTestContext| {
        assert!(
            cx.debug_bounds("nav-rail-key-2").is_none(),
            "one Pane, one key"
        );
        let key = cx.debug_bounds("nav-rail-key-1").expect("⌘1's line");
        threads
            .iter()
            .copied()
            .find(|thread| {
                let id: &'static str = format!("nav-rail-{}", thread.get()).leak();
                cx.debug_bounds(id)
                    .is_some_and(|line| line.top() <= key.top() && key.bottom() <= line.bottom())
            })
            .expect("on a Thread's line")
    };
    let focused = view.read_with(cx, |view, _| view.cockpit.roster().focused_thread());
    assert_eq!(Some(keyed(cx)), focused);
    let last: &'static str = format!("nav-rail-{}", threads[2].get()).leak();
    let line = cx.debug_bounds(last).unwrap();
    cx.simulate_click(line.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.cockpit.roster().focused_thread()),
        Some(threads[2])
    );
    assert_eq!(keyed(cx), threads[2], "⌘1 follows the Solo Pane");
}
