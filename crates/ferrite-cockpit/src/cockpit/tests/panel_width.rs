//! The Panel's width: the one the operator drags to is theirs, and a
//! tool's own least (Files, Changes, a file) only widens what is drawn.
use super::*;
use crate::cockpit::panel::least_w;
use crate::cockpit::surfaces::SurfaceId;
use ferrite_ui::theme::{CAPTION_ALLOWANCE, DOCK_THREAD_MIN_W, FILES_EDITOR_MIN_W};
use std::time::Duration;

fn window<'a>(
    name: &str,
    width: f32,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    ThreadId,
    &'a mut gpui::VisualTestContext,
) {
    let (core, _fake, _checkout) = bound_cockpit(name, Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(width), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, thread, cx)
}

fn drawn(cx: &mut gpui::VisualTestContext) -> f32 {
    f32::from(
        cx.debug_bounds("panel")
            .expect("the Panel is open")
            .size
            .width,
    )
}

fn chosen(view: &Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) -> f32 {
    view.read_with(cx, |view, cx| view.panel.surfaces.get(view.cockpit.read(cx).thread_key(thread)).unwrap().width)
}

/// Drag the Panel's left edge until the Panel is `width` wide.
fn drag_to(width: f32, window: f32, cx: &mut gpui::VisualTestContext) {
    let edge = cx.debug_bounds("panel-edge").expect("the edge").center();
    let to = gpui::point(px(window - width), edge.y);
    cx.simulate_mouse_down(edge, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
}

fn show(
    view: &Entity<CockpitView>,
    thread: ThreadId,
    id: SurfaceId,
    cx: &mut gpui::VisualTestContext,
) {
    view.update(cx, |view, cx| view.upsert_surface(view.cockpit.read(cx).thread_key(thread), id, cx));
    leap(cx, Duration::from_secs(1));
}

/// The operator's click on the Overview tab, once the strip's left chevron
/// has brought it back into view: showing a tool scrolls the strip to it,
/// and in a strip that overflows that can take the first tab out of sight.
fn back_to_overview(cx: &mut gpui::VisualTestContext) {
    settles(cx);
    let tab = loop {
        let tab = cx.debug_bounds("panel-tab-Overview").expect("the Overview tab");
        let strip = cx.debug_bounds("panel-tabs").expect("the strip");
        if tab.left() >= strip.left() {
            break tab.center();
        }
        let left = cx
            .debug_bounds("panel-tabs-left")
            .expect("a chevron toward the Overview tab out of view");
        cx.simulate_click(left.center(), gpui::Modifiers::none());
        settles(cx);
    };
    cx.simulate_click(tab, gpui::Modifiers::none());
    leap(cx, Duration::from_secs(1));
}

/// A drag made on a tool whose least is wider than the drag keeps what the
/// operator pulled to, not the tool's least: Overview shows it again.
#[gpui::test]
fn a_drag_on_a_wide_tool_saves_the_width_dragged_to(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("panel-width-drag", 1400., cx);
    show(&view, thread, SurfaceId::Files, cx);
    let files = drawn(cx);
    assert!(files > 500., "Files draws wide: {files}");
    let to = 380. + CAPTION_ALLOWANCE;
    drag_to(to, 1400., cx);
    assert_eq!(chosen(&view, thread, cx), to, "what the pointer chose");
    assert_eq!(
        drawn(cx),
        files,
        "Files still draws no narrower than its least"
    );
    show(&view, thread, SurfaceId::Overview, cx);
    assert_eq!(drawn(cx), to, "Overview is the width chosen");
}

/// At each window width, each tool opened and left again: the width chosen
/// is the one saved, the Panel comes back to it, and the chat keeps its
/// least. Each width is wider by the head's allowance, which the Panel
/// takes from the window.
#[gpui::test]
fn every_tool_leaves_the_chosen_width_alone_at_any_window_width(cx: &mut TestAppContext) {
    for (window_w, chosen_w) in [(800., 360.), (1000., 380.), (1400., 440.)] {
        let (window_w, chosen_w) = (window_w + CAPTION_ALLOWANCE, chosen_w + CAPTION_ALLOWANCE);
        let (view, thread, cx) = window(&format!("panel-width-{window_w}"), window_w, cx);
        drag_to(chosen_w, window_w, cx);
        let saved = chosen(&view, thread, cx);
        assert!(saved >= least_w());
        let rest = drawn(cx);
        for id in [SurfaceId::Files, SurfaceId::Changes, SurfaceId::Browser] {
            show(&view, thread, id.clone(), cx);
            if id == SurfaceId::Files {
                let editor = f32::from(cx.debug_bounds("files-editor").expect("the editor").size.width);
                assert!(
                    editor >= FILES_EDITOR_MIN_W,
                    "the editor stays readable at {window_w}: {editor}"
                );
            }
            let card = cx.update(|window, cx| view.read(cx).pane_rects(window, cx))[0].1;
            assert!(
                card.w >= DOCK_THREAD_MIN_W,
                "chat keeps its least beside {id:?} at {window_w}: {}",
                card.w
            );
            back_to_overview(cx);
            assert_eq!(chosen(&view, thread, cx), saved, "{id:?} at {window_w}");
            assert_eq!(drawn(cx), rest, "Overview again after {id:?} at {window_w}");
        }
        view.update(cx, |view, cx| view.new_terminal_surface(thread, cx));
        tick(cx);
        show(&view, thread, SurfaceId::Overview, cx);
        assert_eq!(chosen(&view, thread, cx), saved, "terminal at {window_w}");
        assert_eq!(
            drawn(cx),
            rest,
            "Overview again after a terminal at {window_w}"
        );
    }
}
