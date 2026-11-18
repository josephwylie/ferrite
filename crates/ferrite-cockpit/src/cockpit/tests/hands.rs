//! The driven window's hands (`crate::hands`, ADR 0038) on the Cockpit's
//! window: what `ferrite --headless` sends for `window.resize`.
use super::*;
use crate::hands;
use ferrite_ui::theme::{WINDOW_MIN_H, WINDOW_MIN_W};

fn solo<'a>(name: &str, cx: &'a mut TestAppContext) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let (core, _fake, _checkout) = bound_cockpit(name, Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    (view, cx)
}

#[gpui::test]
fn a_resize_lays_the_window_out_again_and_stops_at_its_least_size(cx: &mut TestAppContext) {
    let (_view, cx) = solo("hands-resize", cx);
    let wide = cx.debug_bounds("composer-block").expect("a Composer at 1440");

    let size = cx.update(|window, cx| hands::resize(window, cx, gpui::size(px(700.), px(900.))));
    tick(cx);
    assert_eq!(size, gpui::size(px(700.), px(900.)));
    let narrow = cx.debug_bounds("composer-block").expect("a Composer at 700");
    assert_ne!(narrow, wide, "the Composer is laid out for the narrower window");
    assert!(narrow.right() <= px(700.), "{narrow:?} inside the window");

    let size = cx.update(|window, cx| hands::resize(window, cx, gpui::size(px(300.), px(200.))));
    tick(cx);
    assert_eq!(size, gpui::size(px(WINDOW_MIN_W), px(WINDOW_MIN_H)), "no smaller than the app's window goes");
    assert_eq!(cx.update(|window, _| window.viewport_size()), size);
}
