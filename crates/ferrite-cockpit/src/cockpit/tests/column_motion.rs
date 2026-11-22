//! The nav's and the panel's columns (`columns.rs`): each slides open and
//! shut, the board following the width on screen, a reversal turns around
//! from where it is, what slides out takes no input, and reduced motion
//! lands at once. And the panel's width: a tool's floor is never the
//! operator's saved width.

use super::*;
use crate::cockpit::surfaces::{SurfaceId, Surfaces};
use crate::cockpit::panel::least_w;
use ferrite_ui::theme::{CAPTION_ALLOWANCE, DOCK_THREAD_MIN_W, MOTION_DUR_BASE_MS};

fn window<'a>(
    name: &str,
    width: f32,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    ferrite_ui::motion::testing::drive();
    let (core, _fake) = cockpit(name, 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let dir = scratch(&format!("{name}-settings"));
    std::fs::create_dir_all(&dir).unwrap();
    view.update(cx, |view, _| {
        view.prefs.dir = dir;
        view.prefs.settings.panel_shown = true;
        view.nav_forced_open = true;
    });
    cx.simulate_resize(gpui::size(px(width), px(900.)));
    tick(cx);
    settle(cx);
    (view, cx)
}

fn frame(cx: &mut gpui::VisualTestContext) -> usize {
    let frames = cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    frames
}

fn at(ms: u64, cx: &mut gpui::VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    frame(cx);
}

fn settle(cx: &mut gpui::VisualTestContext) {
    for _ in 0..64 {
        if frame(cx) == 0 {
            return;
        }
        cx.executor().advance_clock(Duration::from_millis(16));
    }
    panic!("the window never stopped asking for frames");
}

fn panel_w(cx: &mut gpui::VisualTestContext) -> f32 {
    cx.debug_bounds("panel")
        .map_or(0.0, |panel| f32::from(panel.size.width))
}

fn board_left(cx: &mut gpui::VisualTestContext) -> f32 {
    f32::from(cx.debug_bounds("board").expect("the board").left())
}

fn card_right(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> f32 {
    let card = cx.update(|window, cx| view.read(cx).pane_rects(window, cx))[0].1;
    card.x + card.w
}

fn toggle_panel(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| view.toggle_panel_now(cx));
    cx.run_until_parked();
}

fn fold_nav(folded: bool, view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| {
        view.nav_forced_open = false;
        view.set_nav_collapsed(folded, cx);
    });
    cx.run_until_parked();
}

/// Closing, the panel's column narrows through widths between its own and
/// none while the board takes the room, then leaves the window; opening
/// is the same the other way, and nothing asks for a frame at rest.
#[gpui::test]
fn the_panel_slides_open_and_shut(cx: &mut TestAppContext) {
    let (view, cx) = window("column-panel-slide", 1600., cx);
    assert_eq!(panel_w(cx), least_w(), "first paint is at rest");

    toggle_panel(&view, cx);
    at(MOTION_DUR_BASE_MS / 4, cx);
    let closing = panel_w(cx);
    assert!(
        closing > 0.0 && closing < least_w() - 20.0,
        "mid-close: {closing}"
    );
    let right = card_right(&view, cx);
    assert!(
        right > 1600. - least_w() + 20.0 && right < 1600.,
        "the board follows: {right}"
    );
    assert!((1600. - right - closing).abs() < 0.5, "to the drawn edge");

    at(MOTION_DUR_BASE_MS, cx);
    assert_eq!(cx.debug_bounds("panel"), None, "let go once shut");
    assert_eq!(card_right(&view, cx), 1600.);
    assert_eq!(frame(cx), 0, "at rest, nothing asks for a frame");

    toggle_panel(&view, cx);
    at(MOTION_DUR_BASE_MS / 4, cx);
    let opening = panel_w(cx);
    assert!(
        opening > 0.0 && opening < least_w() - 20.0,
        "mid-open: {opening}"
    );
    at(MOTION_DUR_BASE_MS, cx);
    assert_eq!(panel_w(cx), least_w());
    assert_eq!(frame(cx), 0);
}

/// A flip mid-slide continues from the width on screen: neither a jump to
/// full nor to shut.
#[gpui::test]
fn a_reversal_starts_from_the_current_frame(cx: &mut TestAppContext) {
    let (view, cx) = window("column-panel-reverse", 1600., cx);
    toggle_panel(&view, cx);
    at(MOTION_DUR_BASE_MS / 2, cx);
    let before = panel_w(cx);
    assert!(before > 0.0 && before < least_w(), "mid-close: {before}");

    toggle_panel(&view, cx);
    at(1, cx);
    let after = panel_w(cx);
    assert!(
        (after - before).abs() < 10.0,
        "turned around from {before}, not from rest or shut: {after}"
    );
    at(MOTION_DUR_BASE_MS / 4, cx);
    assert!(panel_w(cx) > after, "and now opening");
    at(MOTION_DUR_BASE_MS, cx);
    assert_eq!(panel_w(cx), least_w());
}

/// Drag the panel's edge to `x` with the real pointer, and let go.
fn drag_edge(x: f32, cx: &mut gpui::VisualTestContext) {
    let edge = cx.debug_bounds("panel-edge").expect("the panel's edge");
    let none = gpui::Modifiers::none();
    cx.simulate_mouse_down(edge.center(), MouseButton::Left, none);
    cx.simulate_mouse_move(gpui::point(px(x), edge.center().y), MouseButton::Left, none);
    cx.simulate_mouse_up(gpui::point(px(x), edge.center().y), MouseButton::Left, none);
    cx.run_until_parked();
}

/// The panel sliding out is drawn but deaf: a drag from its edge moves
/// nothing, where at rest it resizes the panel.
#[gpui::test]
fn a_panel_sliding_out_takes_no_input(cx: &mut TestAppContext) {
    let (view, cx) = window("column-panel-deaf", 1600., cx);
    assert!(cx.debug_bounds("column-veil").is_none());
    toggle_panel(&view, cx);
    at(MOTION_DUR_BASE_MS / 4, cx);
    assert!(cx.debug_bounds("column-veil").is_some(), "sliding out");
    drag_edge(1000., cx);
    let thread = thread(&view, cx);
    let width = view.read_with(cx, |view, cx| {
        view.panel.surfaces.get(view.cockpit.read(cx).thread_key(thread)).map(|s| s.width)
    });
    assert_eq!(width, Some(0.0), "no width was taken from a deaf panel");
}

/// The nav column rides the same slide: its edge and the board's left
/// move through widths between its own and none, and a flip turns around.
#[gpui::test]
fn the_nav_slides_open_and_shut(cx: &mut TestAppContext) {
    use crate::nav::COLUMN_WIDTH;
    let (view, cx) = window("column-nav-slide", 1600., cx);
    assert_eq!(board_left(cx), COLUMN_WIDTH);

    fold_nav(true, &view, cx);
    at(MOTION_DUR_BASE_MS / 4, cx);
    let folding = board_left(cx);
    assert!(
        folding > 20.0 && folding < COLUMN_WIDTH - 20.0,
        "mid-fold: {folding}"
    );
    let column = cx.debug_bounds("nav-column").expect("still drawn");
    assert_eq!(
        f32::from(column.size.width),
        COLUMN_WIDTH,
        "its face unsquashed"
    );
    assert!(cx.debug_bounds("column-veil").is_some(), "and deaf");

    fold_nav(false, &view, cx);
    at(1, cx);
    assert!(
        (board_left(cx) - folding).abs() < 10.0,
        "a flip continues from {folding}: {}",
        board_left(cx)
    );
    at(MOTION_DUR_BASE_MS, cx);
    assert_eq!(board_left(cx), COLUMN_WIDTH);
    assert_eq!(frame(cx), 0);

    fold_nav(true, &view, cx);
    at(MOTION_DUR_BASE_MS, cx);
    assert_eq!(board_left(cx), 0.0);
    assert_eq!(cx.debug_bounds("nav-column"), None, "let go once shut");
}

/// Reduced motion lands every flip on the frame it is made, for both
/// columns, with nothing asking for another.
#[gpui::test]
fn reduced_motion_lands_both_columns(cx: &mut TestAppContext) {
    use crate::nav::COLUMN_WIDTH;
    let (view, cx) = window("column-reduced", 1600., cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));

    toggle_panel(&view, cx);
    assert_eq!(frame(cx), 0);
    assert_eq!(panel_w(cx), 0.0);
    fold_nav(true, &view, cx);
    frame(cx);
    assert_eq!(board_left(cx), 0.0);
    assert_eq!(card_right(&view, cx), 1600.);
    assert_eq!(frame(cx), 0);

    toggle_panel(&view, cx);
    fold_nav(false, &view, cx);
    frame(cx);
    assert_eq!(panel_w(cx), least_w());
    assert_eq!(board_left(cx), COLUMN_WIDTH);
    assert_eq!(frame(cx), 0);
}

fn thread(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> ThreadId {
    view.read_with(cx, |view, cx| view.focused_thread(cx).expect("a Thread"))
}

fn show(view: &Entity<CockpitView>, id: SurfaceId, cx: &mut gpui::VisualTestContext) {
    let thread = thread(view, cx);
    view.update(cx, |view, cx| view.upsert_surface(view.cockpit.read(cx).thread_key(thread), id, cx));
    settle(cx);
}

fn drawn_w(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> f32 {
    view.read_with(cx, |view, cx| view.panel_target_w(cx))
}

/// Visiting Files, then Overview again, leaves the compact panel as it
/// was, through closing, reopening and a restart: a tool's floor is drawn,
/// never saved as the operator's width.
#[gpui::test]
fn a_tools_floor_is_not_the_operators_width(cx: &mut TestAppContext) {
    let (view, cx) = window("column-floor", 1600., cx);
    let thread = thread(&view, cx);
    assert_eq!(drawn_w(&view, cx), least_w());

    show(&view, SurfaceId::Files, cx);
    let files = drawn_w(&view, cx);
    assert!(files > 700.0, "Files takes most of the window: {files}");
    show(&view, SurfaceId::Overview, cx);
    assert_eq!(drawn_w(&view, cx), least_w(), "back to the compact width");

    toggle_panel(&view, cx);
    toggle_panel(&view, cx);
    settle(cx);
    assert_eq!(drawn_w(&view, cx), least_w(), "closed and reopened");

    view.update(cx, |view, cx| view.flush_panel_persistence(cx));
    let (saved, dir) = view.read_with(cx, |view, cx| {
        (
            view.panel.surfaces.get(view.cockpit.read(cx).thread_key(thread)).map(|state| state.width),
            view.prefs.dir.clone(),
        )
    });
    assert_eq!(saved, Some(0.0), "no width was ever chosen");
    let restarted = Surfaces::saved_in(&dir, thread);
    assert_eq!(restarted.map(|state| state.width), Some(0.0));
}

/// A width the operator dragged is theirs: a tool's floor draws over it
/// and gives it back, and it is what a restart reads.
#[gpui::test]
fn a_dragged_width_survives_a_visit_to_a_wider_tool(cx: &mut TestAppContext) {
    let (view, cx) = window("column-drag", 1600., cx);
    let thread = thread(&view, cx);
    drag_edge(1600. - 480., cx);
    settle(cx);
    assert_eq!(
        drawn_w(&view, cx),
        480.0,
        "the panel follows the pointer at once"
    );

    show(&view, SurfaceId::Files, cx);
    assert!(drawn_w(&view, cx) > 700.0);
    show(&view, SurfaceId::Overview, cx);
    assert_eq!(drawn_w(&view, cx), 480.0);

    // A restart follows a quit, and a quit flushes what the background
    // writer has not yet saved.
    view.update(cx, |view, cx| view.flush_panel_persistence(cx));
    let dir = view.read_with(cx, |view, _| view.prefs.dir.clone());
    let restarted = Surfaces::saved_in(&dir, thread);
    assert_eq!(restarted.map(|state| state.width), Some(480.0));
}

/// In a narrow window a wide tool leaves the board its least, and Overview
/// is compact again after it.
#[gpui::test]
fn a_wide_tool_in_a_narrow_window_leaves_the_board_its_least(cx: &mut TestAppContext) {
    let width = 900. + CAPTION_ALLOWANCE;
    let (view, cx) = window("column-narrow", width, cx);
    show(&view, SurfaceId::Files, cx);
    let files = drawn_w(&view, cx);
    assert!(files <= width - DOCK_THREAD_MIN_W, "{files}");
    assert!(card_right(&view, cx) >= DOCK_THREAD_MIN_W - 1.0);
    show(&view, SurfaceId::Overview, cx);
    assert_eq!(drawn_w(&view, cx), least_w());
}

/// The keyboard inside a Panel that slides shut is taken back at once: what
/// is typed while it goes lands in the Composer, and Enter sends it.
#[gpui::test]
fn keys_typed_while_the_panel_slides_shut_reach_the_composer(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    bind_production_keys(cx);
    let (core, fake) = cockpit("column-keys", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let dir = scratch("column-keys-settings");
    std::fs::create_dir_all(&dir).unwrap();
    view.update(cx, |view, _| {
        view.prefs.dir = dir;
        view.prefs.settings.panel_shown = true;
        view.nav_forced_open = true;
    });
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    settle(cx);
    show(&view, SurfaceId::Files, cx);
    cx.update(|window, cx| {
        let keys = view.read(cx).panel_keys.clone();
        window.focus(&keys, cx);
    });
    settle(cx);
    assert!(cx.update(|window, cx| view.read(cx).panel_keys.contains_focused(window, cx)));

    toggle_panel(&view, cx);
    at(MOTION_DUR_BASE_MS / 4, cx);
    assert!(cx.debug_bounds("column-veil").is_some(), "still sliding");
    cx.simulate_input("typed mid-slide");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        fake.sent.borrow().as_slice(),
        ["typed mid-slide"],
        "the Composer heard it and Enter sent it"
    );
}

fn pick(id: SurfaceId, view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    let thread = thread(view, cx);
    view.update(cx, |view, cx| view.upsert_surface(view.cockpit.read(cx).thread_key(thread), id, cx));
    cx.run_until_parked();
}

/// A picked tab's body fades in over the place the last one left, and the
/// outgoing body is gone on the frame the pick lands, so the strip and the
/// body never disagree and nothing under the fade answers for the old tab.
/// An update that is no pick plays nothing.
#[gpui::test]
fn a_picked_tabs_body_fades_in_and_the_outgoing_one_goes_at_once(cx: &mut TestAppContext) {
    let (view, cx) = window("column-tab-fade", 1600., cx);
    show(&view, SurfaceId::Overview, cx);
    assert!(cx.debug_bounds("panel-body").is_some(), "Overview shows");
    assert_eq!(frame(cx), 0);

    pick(SurfaceId::Files, &view, cx);
    assert!(frame(cx) > 0, "the new body is fading in");
    assert!(cx.debug_bounds("files").is_some());
    assert!(cx.debug_bounds("panel-body").is_none(), "Overview is gone");
    at(MOTION_DUR_BASE_MS / 4, cx);
    pick(SurfaceId::Overview, &view, cx);
    assert!(frame(cx) > 0, "a reversal starts its own fade");
    assert!(cx.debug_bounds("files").is_none());
    assert!(cx.debug_bounds("panel-body").is_some());
    settle(cx);

    view.update(cx, |_, cx| cx.notify());
    assert_eq!(frame(cx), 0, "an update that is no pick plays nothing");
}

/// The keyboard sat in a Panel body that reads its own keys (Files) goes
/// to the Composer when a tab with no keys of its own is shown: what is
/// typed afterwards lands in the line and Enter sends it, rather than
/// going to a body that is no longer drawn.
#[gpui::test]
fn keys_held_by_a_panel_body_return_to_the_composer_when_the_tab_changes(
    cx: &mut TestAppContext,
) {
    bind_production_keys(cx);
    let (core, fake) = cockpit("column-tab-keys", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let dir = scratch("column-tab-keys-settings");
    std::fs::create_dir_all(&dir).unwrap();
    view.update(cx, |view, _| {
        view.prefs.dir = dir;
        view.prefs.settings.panel_shown = true;
        view.nav_forced_open = true;
    });
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    settle(cx);
    show(&view, SurfaceId::Files, cx);
    cx.update(|window, cx| {
        let keys = view.read(cx).panel_keys.clone();
        window.focus(&keys, cx);
    });
    settle(cx);
    assert!(cx.update(|window, cx| view.read(cx).panel_keys_held(window, cx)));

    pick(SurfaceId::Overview, &view, cx);
    settle(cx);
    assert!(
        !cx.update(|window, cx| view.read(cx).panel_keys_held(window, cx)),
        "the keys left with the body that read them"
    );
    cx.simulate_input("after the switch");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(fake.sent.borrow().as_slice(), ["after the switch"]);
}

/// The system setting flipping while the panel and the nav are mid-slide
/// lands both where they were going, with no further frame asked for.
#[gpui::test]
fn reduced_motion_turned_on_mid_slide_lands_it(cx: &mut TestAppContext) {
    let (view, cx) = window("column-reduced-live", 1600., cx);
    toggle_panel(&view, cx);
    at(MOTION_DUR_BASE_MS / 4, cx);
    assert!(panel_w(cx) > 0.0, "mid-close");

    cx.update(|_, cx| cx.set_reduce_motion(true));
    frame(cx);
    assert_eq!(cx.debug_bounds("panel"), None, "landed shut");
    assert_eq!(card_right(&view, cx), 1600.);
    assert_eq!(frame(cx), 0);
}
