//! The nav's parity behaviour: the selection following the board (N-5),
//! the scrollbar beside the rows, the one seam (N-8) and the rail.

use super::*;

fn sorted_threads(core: &Cockpit) -> Vec<ThreadKey> {
    let mut threads = core.threads();
    threads.sort();
    threads
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
            .debug_bounds(format!("nav-thread-{}", thread.thread().get()).leak())
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
    assert_eq!(card.left(), px(0.), "at the window edge");
    assert_eq!(card.bottom(), px(700.), "at the window foot");
    assert_eq!(card.top(), px(ferrite_ui::theme::NAV_HEADER_H), "under the sidebar topbar");
    view.read_with(cx, |view, _| {
        assert!(
            (view.nav_width() - f32::from(column.right())).abs() < 0.5,
            "the board starts at the column's edge"
        );
        assert!(
            (view.nav_width() - f32::from(card.right())).abs() < 0.5,
            "the sidebar meets the board"
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
    assert!(back.bottom() <= card.top(), "open, back sits in the titlebar: {back:?}");
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

#[gpui::test]
fn resizing_the_sidebar_persists_and_resets_without_losing_collapse(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-resize", 1);
    let title: &'static str = format!("nav-title-{}", core.threads()[0].thread().get()).leak();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let dir = scratch("nav-resize-settings");
    std::fs::create_dir_all(&dir).unwrap();
    view.update(cx, |view, _| {
        view.prefs.dir = dir.clone();
        view.prefs.settings.nav_width_px = None;
    });
    hide_panel(&view, cx);
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.0), px(900.0)));
    tick(cx);
    let before = cx.debug_bounds("nav-card").unwrap();
    let title_width = cx.debug_bounds(title).unwrap().size.width;
    assert_eq!(before.size.width, px(256.0));
    let rail = cx.debug_bounds("nav-resize-rail").unwrap();
    let start = rail.center();
    let end = start + gpui::point(px(64.0), px(0.0));
    cx.simulate_mouse_down(start, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(cx.debug_bounds("nav-card").unwrap().size.width, px(320.0));
    assert_eq!(cx.debug_bounds(title).unwrap().size.width, title_width + px(64.0));
    assert_eq!(view.read_with(cx, |view, _| view.prefs.settings.nav_width_px), None);
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: end,
        modifiers: gpui::Modifiers::none(),
        click_count: 1,
    });
    tick(cx);
    assert_eq!(ferrite_core::settings::Settings::load(&dir).unwrap().nav_width_px, Some(320.0));
    assert_eq!(cx.debug_bounds("nav-titlebar").unwrap().size.width, px(320.0));
    assert_eq!(cx.debug_bounds("board").unwrap().left(), px(320.0));

    cx.simulate_resize(gpui::size(px(900.0), px(900.0)));
    tick(cx);
    assert_eq!(cx.debug_bounds("nav-card").unwrap().size.width, px(260.0));
    assert_eq!(ferrite_core::settings::Settings::load(&dir).unwrap().nav_width_px, Some(320.0));
    cx.simulate_resize(gpui::size(px(1440.0), px(900.0)));
    tick(cx);
    assert_eq!(cx.debug_bounds("nav-card").unwrap().size.width, px(320.0));
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    tick(cx);
    assert!(cx.debug_bounds("nav-resize-rail").is_none());
    view.update(cx, |view, cx| view.set_nav_collapsed(false, cx));
    tick(cx);
    assert_eq!(cx.debug_bounds("nav-card").unwrap().size.width, px(320.0));

    let at = cx.debug_bounds("nav-resize-rail").unwrap().center();
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::none(),
        click_count: 2,
    });
    tick(cx);
    assert_eq!(cx.debug_bounds("nav-card").unwrap().size.width, px(256.0));
    assert_eq!(ferrite_core::settings::Settings::load(&dir).unwrap().nav_width_px, None);
    assert!(!view.read_with(cx, |view, _| view.nav_folded()));
}

#[gpui::test]
fn collapsing_the_sidebar_cancels_an_uncommitted_resize(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("nav-resize-cancel", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    hold_nav_open(&view, cx);
    view.update(cx, |view, _| view.prefs.settings.nav_width_px = Some(300.0));
    cx.simulate_resize(gpui::size(px(1440.0), px(900.0)));
    tick(cx);
    let start = cx.debug_bounds("nav-resize-rail").unwrap().center();
    cx.simulate_mouse_down(start, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(start + gpui::point(px(80.0), px(0.0)), Some(MouseButton::Left), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(cx.debug_bounds("nav-card").unwrap().size.width, px(380.0));
    view.update(cx, |view, cx| {
        view.set_nav_collapsed(true, cx);
        view.finish_nav_resize(cx);
        assert_eq!(view.prefs.settings.nav_width_px, Some(300.0));
    });
    tick(cx);
    view.update(cx, |view, cx| view.set_nav_collapsed(false, cx));
    tick(cx);
    assert_eq!(cx.debug_bounds("nav-card").unwrap().size.width, px(300.0));
}
