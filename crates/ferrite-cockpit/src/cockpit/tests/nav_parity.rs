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
    let card = cx.debug_bounds("nav-card").expect("the card");
    let bar = cx.debug_bounds("nav-scrollbar").expect("the scrollbar");
    assert_eq!(bar.right(), card.right(), "at the card's edge");
    let strip = bar.right() - px(NAV_SCROLLBAR_GUTTER);
    for thread in threads {
        let row = cx
            .debug_bounds(format!("nav-thread-{}", thread.get()).leak())
            .expect("a row");
        assert!(row.right() <= strip, "{row:?} ends before the strip");
    }
}

#[gpui::test]
fn the_board_starts_past_the_card(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-one-seam", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(700.)));
    tick(cx);
    let column = cx.debug_bounds("nav-column").expect("the column");
    let card = cx.debug_bounds("nav-card").expect("the card");
    assert!(cx.debug_bounds("nav-seam").is_none(), "no seam");
    assert!(cx.debug_bounds("chrome-seam").is_none(), "no seam");
    assert_eq!(card.left(), px(12.), "in from the window's edge");
    assert_eq!(card.bottom(), px(700. - 40.), "short of the window's foot");
    assert_eq!(card.top(), px(ferrite_ui::theme::WIN_CHROME_H + 40.), "the same under the head row");
    view.read_with(cx, |view, _| {
        assert!(
            (view.nav_width() - f32::from(column.right())).abs() < 0.5,
            "the board starts at the column's edge"
        );
        assert!(
            (view.nav_width() - f32::from(card.right()) - 12.).abs() < 0.5,
            "the gap past the card"
        );
    });
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    tick(cx);
    assert!(cx.debug_bounds("nav-card").is_none(), "folded, no card");
    view.read_with(cx, |view, _| {
        assert_eq!(view.nav_width(), 0.0, "folded, the board has the window's edge");
    });
}

#[gpui::test]
fn the_edge_tab_folds_and_opens_the_nav(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{HISTORY_X, NAV_EDGE_TAB_W};
    let (core, _fake) = cockpit("nav-edge-tab", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let card = cx.debug_bounds("nav-card").expect("open, the card");
    let tab = cx.debug_bounds("nav-edge-tab").expect("the tab");
    assert!((tab.left() - card.right()).abs() <= px(1.), "on the card's edge: {tab:?}");
    assert!((tab.center().y - card.center().y).abs() <= px(0.5), "at its mid-height");
    let back = cx.debug_bounds("history-back").expect("the back arrow");
    assert!(card.contains(&back.center()), "open, back heads the card: {back:?}");
    assert!(cx.debug_bounds("nav-collapse").is_none(), "no other fold control");

    cx.simulate_click(tab.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.nav_folded()), "the tab folds it");
    assert!(cx.debug_bounds("nav-card").is_none(), "folded, the card goes away");
    let tab = cx.debug_bounds("nav-edge-tab").expect("the tab stays");
    assert_eq!(tab.left(), px(0.), "at the window's edge");
    assert_eq!(tab.size.width, px(NAV_EDGE_TAB_W));
    let back = cx.debug_bounds("history-back").expect("the back arrow");
    assert_eq!(back.left(), px(HISTORY_X), "beside the traffic lights");

    cx.simulate_click(tab.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(!view.read_with(cx, |view, _| view.nav_folded()), "and opens it");
    assert!(cx.debug_bounds("nav-card").is_some());
}
