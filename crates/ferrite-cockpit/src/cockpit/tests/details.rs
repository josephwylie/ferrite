//! The Thread details card (`cockpit/details.rs`): the Thread's lightweight
//! context, reachable beside the tool the Panel holds, with its own
//! visibility, focus and dismissal.
use super::*;
use crate::cockpit::surfaces::SurfaceId;
use ferrite_ui::theme::*;

fn solo<'a>(
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
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(width), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    (view, thread, cx)
}

fn composer_focused(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> bool {
    cx.update(|window, cx| {
        view.read(cx).panes[0]
            .composer
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    })
}

fn open(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> bool {
    view.read_with(cx, |view, _| view.panes[0].details.is_open())
}

#[gpui::test]
fn the_card_opens_beside_the_panels_files(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("details-beside-files", 1900., cx);
    view.update(cx, |view, cx| {
        view.upsert_surface(thread, SurfaceId::Files, cx)
    });
    tick(cx);
    assert!(
        cx.debug_bounds("details-card").is_none(),
        "closed until asked"
    );

    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    let card = cx.debug_bounds("details-card").expect("⌘⇧B opens the card");
    let files = cx
        .debug_bounds("files")
        .expect("the Panel still shows Files");
    assert!(
        card.right() <= files.left(),
        "the card floats over the transcript, not the tool"
    );
    assert!(!composer_focused(&view, cx), "the card has the keyboard");
    assert!(
        cx.debug_bounds("details-head").is_some(),
        "its Thread's name heads it"
    );
    assert_eq!(
        panel_tabs(&view, thread, cx).1,
        Some(SurfaceId::Files),
        "opening it leaves the Panel's tool alone"
    );
}

#[gpui::test]
fn the_title_bar_toggle_opens_and_shuts_it(cx: &mut TestAppContext) {
    let (view, _, cx) = solo("details-toggle", 1900., cx);
    let toggle = cx
        .debug_bounds("details-toggle")
        .expect("the title bar's toggle");
    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(open(&view, cx));
    assert!(
        cx.debug_bounds("panel-toggle").is_some(),
        "beside the Panel's own toggle"
    );
    assert!(cx.debug_bounds("details-card").is_some());

    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        !open(&view, cx),
        "a second press on the toggle shuts it, not reopens it"
    );
    assert!(cx.debug_bounds("details-card").is_none());
}

#[gpui::test]
fn escape_shuts_the_card_and_returns_the_keyboard_to_the_composer(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("details-escape", 1900., cx);
    view.update(cx, |view, cx| {
        view.upsert_surface(thread, SurfaceId::Files, cx)
    });
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!open(&view, cx));
    assert!(cx.debug_bounds("details-card").is_none());
    assert!(
        cx.debug_bounds("files").is_some(),
        "Escape leaves the tool open"
    );
    assert!(composer_focused(&view, cx));
}

#[gpui::test]
fn a_press_outside_shuts_the_card(cx: &mut TestAppContext) {
    let (view, thread, cx) = solo("details-click-out", 1900., cx);
    view.update(cx, |view, cx| {
        view.upsert_surface(thread, SurfaceId::Files, cx)
    });
    tick(cx);
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    let card = cx.debug_bounds("details-card").unwrap();
    cx.simulate_click(card.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(open(&view, cx), "a press inside it keeps it");

    let files = cx.debug_bounds("files").unwrap();
    cx.simulate_click(files.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(!open(&view, cx), "a press outside shuts it");
    assert!(cx.debug_bounds("files").is_some(), "the tool stays");
}

#[gpui::test]
fn a_narrow_pane_keeps_the_card_inside_it(cx: &mut TestAppContext) {
    let (view, _, cx) = solo("details-narrow", 760., cx);
    cx.simulate_keystrokes("cmd-alt-b");
    tick(cx);
    assert!(cx.debug_bounds("panel").is_none(), "the Panel is put away");
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    assert!(open(&view, cx));
    let card = cx.debug_bounds("details-card").expect("the card");
    assert!(
        card.left() >= px(0.) && card.right() <= px(760.),
        "inside the window: {card:?}"
    );
    assert!(
        card.size.width >= px(160.),
        "still wide enough to read: {card:?}"
    );
    assert!(card.size.width <= px(DETAILS_W));
}

#[gpui::test]
fn the_card_hangs_below_the_find_bar(cx: &mut TestAppContext) {
    let (_, _, cx) = solo("details-find", 1900., cx);
    cx.simulate_keystrokes("cmd-f");
    tick(cx);
    let find = cx.debug_bounds("find-bar").expect("the find bar");
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    let card = cx.debug_bounds("details-card").expect("the card");
    assert!(
        card.top() >= find.bottom(),
        "clear of the find bar: {card:?} vs {find:?}"
    );
}

#[gpui::test]
fn the_card_leaves_with_its_exit_and_snaps_when_motion_is_reduced(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let (view, _, cx) = solo("details-exit", 1900., cx);
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!open(&view, cx));
    let leaving = cx.debug_bounds("details-card").is_some();
    for _ in 0..40 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    assert!(
        cx.debug_bounds("details-card").is_none(),
        "gone once its exit ran (was leaving: {leaving})"
    );
}
