//! The loops overlay over a cached Cockpit (`loops_overlay`): the loops
//! keep their look while the Cockpit is not rebuilt for them, and a cached
//! Cockpit never shows stale UI — for each way it reads what changes.

use super::*;
use crate::composer::{caret_alpha, Caret};

fn renders() -> usize {
    crate::cockpit::RENDERS.with(std::cell::Cell::get)
}

fn overlay_renders() -> usize {
    crate::loops_overlay::testing::overlay_renders()
}

fn hosted(cx: &mut gpui::VisualTestContext) -> usize {
    cx.update(|window, _| crate::loops_overlay::hosted_marks(window))
}

/// Let `ms` pass a display frame at a time, delivering each frame asked for.
fn frames(cx: &mut gpui::VisualTestContext, ms: u64) {
    for _ in 0..ms / 16 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
}

/// An active window, the keyboard in the first Pane's Composer, the motion
/// kit live, first paint settled.
fn focused<'a>(
    name: &str,
    threads: usize,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    crate::motion::testing::drive();
    let (core, _fake) = cockpit(name, threads);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    cx.update(|window, _| window.activate_window());
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    frames(cx, 1_000);
    (view, cx)
}

fn composer_of(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Entity<Composer> {
    view.read_with(cx, |view, _| view.panes[0].composer.clone())
}

/// The block's alpha on screen at `at`: the last draw at or before it.
fn shown_at(draws: &[(std::time::Instant, Option<Caret>)], at: std::time::Instant) -> Option<f32> {
    draws
        .iter()
        .rev()
        .find(|(drawn, _)| *drawn <= at)
        .and_then(|(_, caret)| match caret {
            Some(Caret::Block(alpha)) => Some(*alpha),
            _ => None,
        })
}

/// The focused Composer's caret blinks in the overlay: three turns of the
/// soft blink draw the overlay and not the Cockpit, and at every instant of
/// the pulse clock's grid the caret on screen is the blink's value there.
#[gpui::test]
fn the_caret_blinks_in_the_overlay_and_keeps_its_look(cx: &mut TestAppContext) {
    let (view, cx) = focused("overlay-caret", 2, cx);
    let composer = composer_of(&view, cx);
    composer.update(cx, |composer, _| composer.clear_caret_log());
    assert_eq!(hosted(cx), 1, "the premise: the overlay draws the caret");
    let (cockpit, overlay) = (renders(), overlay_renders());
    let turn = crate::theme::MOTION_CARET_BLINK_MS;
    for _ in 0..3 * turn {
        cx.executor().advance_clock(Duration::from_millis(1));
        cx.run_until_parked();
    }
    let end = cx.update(|_, cx| cx.background_executor().now());
    assert!(
        renders() - cockpit <= 1,
        "the blink rebuilt the Cockpit {} times in three turns",
        renders() - cockpit
    );
    assert!(overlay_renders() > overlay, "the overlay drew the blink");
    let (from, draws) = composer.read_with(cx, |composer, _| composer.caret_log());
    let from = from.expect("focus restarted the blink");
    assert!(
        draws.len() >= 3 * 5 && draws.len() <= 3 * 8 + 2,
        "{} caret draws in three turns: the fades' instants, not the plateaus'",
        draws.len()
    );
    let (epoch, tick) = cx
        .update(|_, cx| crate::motion::testing::grid(cx))
        .expect("the overlay declared the blink");
    let first = draws.first().map(|(at, _)| *at).unwrap_or(end);
    let mut at = epoch;
    while at <= end {
        if at >= first {
            assert_eq!(
                shown_at(&draws, at),
                Some(caret_alpha(at.saturating_duration_since(from))),
                "at {:?} into the blink",
                at.saturating_duration_since(from)
            );
        }
        at += tick;
    }
}

/// Typing still reaches a line whose caret the overlay draws: the input
/// handler the cached Cockpit registered is replayed with it, and the edit
/// restarts the blink solid.
#[gpui::test]
fn typing_reaches_a_line_the_overlay_draws_the_caret_of(cx: &mut TestAppContext) {
    let (view, cx) = focused("overlay-typing", 1, cx);
    frames(cx, 700);
    assert_eq!(hosted(cx), 1, "the premise: the overlay draws the caret");
    cx.simulate_input("héllo");
    cx.run_until_parked();
    let composer = composer_of(&view, cx);
    composer.read_with(cx, |composer, _| {
        assert_eq!(composer.text(), "héllo");
        assert_eq!(
            composer.caret_log().1.last().map(|(_, caret)| *caret),
            Some(Some(Caret::Block(1.0))),
            "an edit lands on a solid block"
        );
    });
}

/// A float's loops are drawn over the float: with the palette open, the
/// board's lit line blinks in the overlay's main layer (under the palette's
/// veil, which is deferred and paints after it), and the palette's own line
/// in the overlay's layer at the float's priority, right over the float —
/// neither rebuilding the Cockpit.
#[gpui::test]
fn an_open_palettes_caret_blinks_in_the_overlay_over_its_float(cx: &mut TestAppContext) {
    let (view, cx) = focused("overlay-float", 2, cx);
    view.update_in(cx, |view, window, cx| {
        view.open_palette(crate::palette::PaletteScope::All, "", window, cx)
    });
    frames(cx, 300);
    let palette_line = view.read_with(cx, |view, cx| {
        view.floats
            .palette
            .as_ref()
            .expect("the palette is open")
            .read(cx)
            .input
            .clone()
    });
    palette_line.update(cx, |line, _| line.clear_caret_log());
    let cockpit = renders();
    frames(cx, 1_200);
    assert_eq!(hosted(cx), 2, "the board's lit line and the palette's");
    assert!(
        renders() - cockpit <= 1,
        "the blinks rebuilt the Cockpit {} times",
        renders() - cockpit
    );
    let (_, draws) = palette_line.read_with(cx, |line, _| line.caret_log());
    let alphas: std::collections::BTreeSet<u32> = draws
        .iter()
        .filter_map(|(_, caret)| match caret {
            Some(Caret::Block(alpha)) => Some(alpha.to_bits()),
            _ => None,
        })
        .collect();
    assert!(
        alphas.len() > 2,
        "the palette's caret blinks over its float: {draws:?}"
    );
}

/// Turned off (`FERRITE_LOOPS_OVERLAY=0`), every loop is drawn in the
/// Cockpit again, uncached: the caret's fades redraw it as before the
/// overlay (the A-1 cadence, at most 37 in 5s).
#[gpui::test]
fn turned_off_the_overlay_leaves_every_loop_in_the_cockpit(cx: &mut TestAppContext) {
    crate::loops_overlay::testing::disable();
    let (_view, cx) = focused("overlay-off", 2, cx);
    assert_eq!(hosted(cx), 0, "no overlay");
    let before = renders();
    frames(cx, 5_000);
    let rendered = renders() - before;
    eprintln!("OVERLAY_OFF cockpit_renders_5s={rendered}");
    assert!(
        (20..=37).contains(&rendered),
        "the caret's fades redraw the Cockpit: {rendered} in 5s"
    );
}

// ------------------------------------------------- a cached Cockpit is fresh

/// What the Cockpit reads but does not mount as a view is the kit Root's
/// layers (`Root::render_notification_layer`): a toast the kit pushes
/// notifies the Root, which the Cockpit read, and rebuilds it.
#[gpui::test]
fn a_kit_notification_rebuilds_the_cached_cockpit(cx: &mut TestAppContext) {
    use gpui::component::{notification::Notification, WindowExt as _};
    let (_view, cx) = focused("overlay-kit-toast", 1, cx);
    let before = renders();
    cx.update(|window, cx| window.push_notification(Notification::new().title("A kit toast"), cx));
    cx.run_until_parked();
    assert!(
        renders() > before,
        "the Cockpit draws the kit's notification layer: it must be rebuilt"
    );
}

/// The window's own state (active, focused) reaches the cached Cockpit:
/// leaving the window stops the blink at once — the Pane's lit line holds a
/// solid, still block, drawn in the Cockpit again — and the overlay draws
/// nothing more.
#[gpui::test]
fn deactivating_the_window_stills_the_overlays_caret(cx: &mut TestAppContext) {
    let (view, cx) = focused("overlay-deactivate", 1, cx);
    assert_eq!(hosted(cx), 1);
    cx.deactivate_window();
    frames(cx, 100);
    assert_eq!(hosted(cx), 0, "nothing blinks, nothing is hosted");
    let composer = composer_of(&view, cx);
    composer.update(cx, |composer, _| composer.clear_caret_log());
    let overlay = overlay_renders();
    frames(cx, 1_200);
    let (_, draws) = composer.read_with(cx, |composer, _| composer.caret_log());
    assert!(
        draws
            .iter()
            .all(|(_, caret)| *caret == Some(Caret::Block(1.0))),
        "solid and still: {draws:?}"
    );
    // The blink's last declared wake may still come once; then the clock
    // parks.
    assert!(
        overlay_renders() - overlay <= 2,
        "the overlay is not woken for a still caret: {} draws",
        overlay_renders() - overlay
    );
    assert!(cx.update(|_, cx| crate::motion::pulse_parked(cx)));
}

/// A frame the Cockpit did not ask for (its parent's, the overlay's)
/// replays it: nothing in its render leaves it dirty for the next frame.
#[gpui::test]
fn a_frame_the_cockpit_did_not_ask_for_replays_it(cx: &mut TestAppContext) {
    let (view, cx) = focused("overlay-replay", 2, cx);
    let content = cx.update(|window, cx| {
        window
            .root::<gpui::component::Root>()
            .flatten()
            .expect("the kit Root")
            .read(cx)
            .view()
            .clone()
    });
    let before = renders();
    for _ in 0..5 {
        cx.update(|_, cx| cx.notify(content.entity_id()));
        cx.run_until_parked();
    }
    assert_eq!(renders(), before, "the cached Cockpit was replayed");
    // And it is still the Cockpit that answers: a change it makes shows.
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(renders(), before + 1);
}
