//! The driven window's hands (`crate::hands`, ADR 0038) on the Cockpit's
//! window: what `ferrite --headless` sends for `window.resize` and
//! `window.drag`.
use super::*;
use crate::cockpit::surfaces::SurfaceId;
use crate::hands;
use ferrite_ui::theme::{WINDOW_MIN_H, WINDOW_MIN_W};

fn solo<'a>(
    name: &str,
    width: f32,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, ThreadId, &'a mut gpui::VisualTestContext) {
    let (core, _fake, _checkout) = bound_cockpit(name, Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(width), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, thread, cx)
}

/// A drag's events, each given its time as the headless driver gives it.
fn drag(from: Point<Pixels>, to: Point<Pixels>, cx: &mut gpui::VisualTestContext) {
    for event in hands::drag(from, to) {
        cx.update(|window, cx| {
            window.dispatch_event(event, cx);
        });
        tick(cx);
    }
}

#[gpui::test]
fn a_resize_lays_the_window_out_again_and_stops_at_its_least_size(cx: &mut TestAppContext) {
    let (_view, _thread, cx) = solo("hands-resize", 1440., cx);
    let wide = cx.debug_bounds("composer-block").expect("a Composer at 1440");

    // Above the least width, which is wider where the app draws its own
    // titlebar.
    let asked = gpui::size(px(WINDOW_MIN_W + 60.), px(900.));
    let size = cx.update(|window, cx| hands::resize(window, cx, asked));
    tick(cx);
    assert_eq!(size, asked);
    let narrow = cx.debug_bounds("composer-block").expect("a Composer in the narrower window");
    assert_ne!(narrow, wide, "the Composer is laid out for the narrower window");
    assert!(narrow.right() <= asked.width, "{narrow:?} inside the window");

    let size = cx.update(|window, cx| hands::resize(window, cx, gpui::size(px(300.), px(200.))));
    tick(cx);
    assert_eq!(size, gpui::size(px(WINDOW_MIN_W), px(WINDOW_MIN_H)), "no smaller than the app's window goes");
    assert_eq!(cx.update(|window, _| window.viewport_size()), size);
}

#[gpui::test]
fn a_drag_of_the_panels_edge_moves_its_left_side_to_the_release(cx: &mut TestAppContext) {
    let (_view, _thread, cx) = solo("hands-drag-edge", 1900., cx);
    let before = cx.debug_bounds("panel").expect("the Panel");
    let edge = cx.debug_bounds("panel-edge").expect("its edge").center();
    let to = gpui::point(edge.x - px(100.), edge.y);
    drag(edge, to, cx);
    let after = cx.debug_bounds("panel").unwrap();
    assert_eq!(after.origin.x, to.x, "{before:?} to {after:?}");
    assert!(after.size.width > before.size.width + px(90.), "{before:?} to {after:?}");
    assert_eq!(after.right(), before.right(), "it grows to the left");
}

#[gpui::test]
fn a_tab_dragged_five_points_is_a_click_and_onto_another_moves(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("hands-drag-tab", 1900., cx);
    for id in [SurfaceId::Changes, SurfaceId::Background] {
        view.update(cx, |view, cx| view.upsert_surface(thread, id, cx));
        tick(cx);
    }
    let edge = cx.debug_bounds("panel-edge").unwrap().center();
    drag(edge, gpui::point(px(1900. - 480.), edge.y), cx);

    let changes = cx.debug_bounds("panel-tab-Changes").unwrap().center();
    drag(changes, gpui::point(changes.x + px(5.), changes.y), cx);
    let (tabs, active) = panel_tabs(&view, thread, cx);
    assert_eq!(tabs, [SurfaceId::Overview, SurfaceId::Changes, SurfaceId::Background]);
    assert_eq!(active, Some(SurfaceId::Changes), "five points is a click, which shows it");

    let changes = cx.debug_bounds("panel-tab-Changes").unwrap().center();
    let overview = cx.debug_bounds("panel-tab-Overview").unwrap().center();
    drag(changes, overview, cx);
    let (tabs, _) = panel_tabs(&view, thread, cx);
    assert_eq!(tabs, [SurfaceId::Changes, SurfaceId::Overview, SurfaceId::Background], "dragged onto Overview");
}
