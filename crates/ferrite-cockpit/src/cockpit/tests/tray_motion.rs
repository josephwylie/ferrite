//! The Composer's tray as chips come and go (`ferrite_pane::composer`): a
//! citation or file taken off the draft closes its place over `CHIP_OUT`
//! with the chips beside it following, takes no input while it does, and
//! leaves the keyboard where the operator can type; the last one's going
//! closes the tray; and the Send button holds still throughout.

use super::*;
use ferrite_core::replies::Reply;
use ferrite_ui::theme::MOTION_CHIP_OUT_MS;

fn window<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    ferrite_ui::motion::testing::drive();
    let (core, _fake) = cockpit(name, 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    tick(cx);
    (view, cx)
}

fn cite(view: &Entity<CockpitView>, quotes: &[&str], cx: &mut gpui::VisualTestContext) {
    let composer = view.read_with(cx, |view, _| view.panes[0].composer.clone());
    for quote in quotes {
        composer.update(cx, |composer, cx| {
            composer.reply(
                Reply {
                    quote: (*quote).into(),
                    ..Reply::default()
                },
                cx,
            )
        });
    }
    tick(cx);
    // Past the entrance, so the chips stand.
    at(ferrite_ui::theme::MOTION_CHIP_IN_MS, cx);
}

fn at(ms: u64, cx: &mut gpui::VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
}

fn left(selector: &'static str, cx: &mut gpui::VisualTestContext) -> f32 {
    f32::from(cx.debug_bounds(selector).expect(selector).left())
}

fn press_remove(selector: &'static str, cx: &mut gpui::VisualTestContext) {
    let remove = cx.debug_bounds(selector).expect(selector);
    cx.simulate_click(remove.center(), gpui::Modifiers::none());
    cx.run_until_parked();
}

/// The chip after the one removed does not jump to the place the removed
/// one held: it is on its way there over `CHIP_OUT`.
#[gpui::test]
fn a_removed_citation_closes_its_place_and_the_next_follows(cx: &mut TestAppContext) {
    let (view, cx) = window("tray-reply-out", cx);
    cite(&view, &["first quote", "second quote"], cx);
    let before = left("cite-chip-1", cx);
    press_remove("cite-chip-0-remove", cx);
    at(MOTION_CHIP_OUT_MS / 2, cx);
    let middle = left("cite-chip-0", cx);
    assert!(
        middle < before && middle > left_of_row(cx),
        "mid-close: {middle} of {before}"
    );
    at(MOTION_CHIP_OUT_MS, cx);
    tick(cx);
    assert_eq!(left("cite-chip-0", cx), left_of_row(cx), "closed up");
}

fn left_of_row(cx: &mut gpui::VisualTestContext) -> f32 {
    left("tray-replies", cx)
}

/// Where the tray's open part starts: it drops toward the line as the
/// tray closes.
fn tray_top(cx: &mut gpui::VisualTestContext) -> f32 {
    f32::from(
        cx.debug_bounds("pending-attachment-tray")
            .expect("the tray")
            .top(),
    )
}

fn line_top(cx: &mut gpui::VisualTestContext) -> f32 {
    f32::from(
        cx.debug_bounds("focused-prompt-editor")
            .expect("the line")
            .top(),
    )
}

fn tray_height(cx: &mut gpui::VisualTestContext) -> f32 {
    cx.debug_bounds("pending-attachment-tray")
        .map_or(0.0, |tray| f32::from(tray.size.height))
}

/// The last citation going closes the tray over the same time, the line
/// takes the keyboard, and Send, standing at the foot of the box, never
/// moves.
#[gpui::test]
fn the_last_citation_closes_the_tray_and_send_holds_still(cx: &mut TestAppContext) {
    let (view, cx) = window("tray-last-out", cx);
    let composer = view.read_with(cx, |view, _| view.panes[0].composer.clone());
    composer.update(cx, |composer, cx| composer.insert("hello", cx));
    cite(&view, &["only quote"], cx);
    let send = cx.debug_bounds("composer-send").expect("Send");
    assert!(tray_height(cx) > 0.0);
    let open = tray_top(cx);
    let line = line_top(cx);
    press_remove("cite-chip-0-remove", cx);
    let mut last = open;
    for _ in 0..2 {
        at(MOTION_CHIP_OUT_MS / 4, cx);
        let now = tray_top(cx);
        assert!(now >= last, "it only closes: the tray at {now} after {last}");
        assert_eq!(cx.debug_bounds("composer-send"), Some(send), "Send holds");
        assert_eq!(line_top(cx), line, "the line holds");
        last = now;
    }
    assert!(
        last > open && last < line,
        "mid-close the open part is between full and none: {open} < {last} < {line}"
    );
    at(MOTION_CHIP_OUT_MS, cx);
    tick(cx);
    assert_eq!(tray_height(cx), 0.0, "and is gone");
    assert_eq!(
        cx.debug_bounds("composer-send"),
        Some(send),
        "Send is where it was"
    );
    assert!(
        cx.debug_bounds("focused-prompt-editor").is_some(),
        "typing goes to the line"
    );
}

/// What is closing takes no press: a click where the leaving chip still is
/// opens nothing and removes nothing.
#[gpui::test]
fn a_citation_that_is_closing_takes_no_press(cx: &mut TestAppContext) {
    let (view, cx) = window("tray-ghost-inert", cx);
    cite(&view, &["first quote", "second quote"], cx);
    let chip = cx.debug_bounds("cite-chip-0").unwrap();
    press_remove("cite-chip-0-remove", cx);
    at(MOTION_CHIP_OUT_MS / 4, cx);
    let height = tray_height(cx);
    cx.simulate_click(
        gpui::point(chip.left() + px(8.), chip.center().y),
        gpui::Modifiers::none(),
    );
    cx.run_until_parked();
    let replies = view.read_with(cx, |view, cx| {
        view.panes[0].composer.read(cx).replies().len()
    });
    assert_eq!(replies, 1, "nothing more was taken");
    assert_eq!(
        tray_height(cx),
        height,
        "and no quote opened under the press"
    );
}

/// A citation added while another closes shows once: no ghost of it stays
/// behind, and nothing drawn takes a press it should not.
#[gpui::test]
fn a_citation_added_as_another_closes_leaves_one_chip_each(cx: &mut TestAppContext) {
    let (view, cx) = window("tray-reversal", cx);
    cite(&view, &["first quote"], cx);
    press_remove("cite-chip-0-remove", cx);
    at(MOTION_CHIP_OUT_MS / 4, cx);
    cite(&view, &["second quote"], cx);
    tick(cx);
    let chips = (0..3)
        .filter(|index| debug_bounds(cx, format!("cite-chip-{index}")).is_some())
        .count();
    assert_eq!(chips, 1, "one chip for the one reply");
    assert!(cx.debug_bounds("cite-chip-0-remove").is_some());
}

/// Under reduced motion a removal is at once: no ghost, no tray left
/// closing.
#[gpui::test]
fn reduced_motion_removes_a_citation_at_once(cx: &mut TestAppContext) {
    let (view, cx) = window("tray-reduced", cx);
    cx.update(|_, cx| cx.set_reduce_motion(true));
    cite(&view, &["first quote", "second quote"], cx);
    press_remove("cite-chip-0-remove", cx);
    assert_eq!(
        left("cite-chip-0", cx),
        left_of_row(cx),
        "closed on the first frame"
    );
    press_remove("cite-chip-0-remove", cx);
    assert_eq!(tray_height(cx), 0.0, "and the tray with the last");
}

/// A file chip closes the same way: the one after it follows over
/// `CHIP_OUT`, and the ghost takes no press.
#[gpui::test]
fn a_removed_file_closes_its_place_and_the_next_follows(cx: &mut TestAppContext) {
    let (view, cx) = window("tray-file-out", cx);
    let files = [
        std::path::PathBuf::from("/nonexistent/first.md"),
        std::path::PathBuf::from("/nonexistent/second.md"),
    ];
    let composer = view.read_with(cx, |view, _| view.panes[0].composer.clone());
    composer.update(cx, |composer, cx| composer.add_files(&files, cx));
    tick(cx);
    at(ferrite_ui::theme::MOTION_CHIP_IN_MS, cx);
    let before = left("tray-file-1", cx);
    let first = left("tray-file-0", cx);
    press_remove("tray-file-0-remove", cx);
    at(MOTION_CHIP_OUT_MS / 2, cx);
    let middle = left("tray-file-1", cx);
    assert!(
        middle < before && middle > first,
        "mid-close: {middle} of {before}"
    );
    assert_eq!(
        composer.read_with(cx, |composer, _| composer.files().to_vec()),
        [files[1].clone()],
        "the draft already holds one"
    );
    at(MOTION_CHIP_OUT_MS, cx);
    tick(cx);
    assert_eq!(left("tray-file-0", cx), first, "closed up");
}

/// The chip after the one removed stands where it did on the first frame
/// after the press: the closing chip is the chip it was, neither wider nor
/// narrower, and the row does not jump.
#[gpui::test]
fn a_removed_citation_leaves_its_neighbour_where_it_stood(cx: &mut TestAppContext) {
    let (view, cx) = window("tray-reply-still", cx);
    cite(&view, &["first quote", "second quote"], cx);
    let before = left("cite-chip-1", cx);
    press_remove("cite-chip-0-remove", cx);
    let after = left("cite-chip-0", cx);
    assert!(
        (after - before).abs() < 0.5,
        "the neighbour moved {before} to {after}"
    );
}

/// A frame that comes a moment after the press, as a real one does, does
/// not snap the neighbours to where the row starts.
#[gpui::test]
fn a_frame_after_the_press_does_not_snap_the_row_shut(cx: &mut TestAppContext) {
    let (view, cx) = window("tray-reply-gap", cx);
    cite(&view, &["first quote", "second quote"], cx);
    let before = left("cite-chip-1", cx);
    let composer = view.read_with(cx, |view, _| view.panes[0].composer.clone());
    cx.update(|window, cx| {
        composer.update(cx, |composer, cx| composer.remove_reply(0, window, cx));
        // The first frame to draw the ghost comes a moment after it went.
        cx.background_executor().advance_clock(Duration::from_millis(1));
    });
    cx.run_until_parked();
    let after = left("cite-chip-0", cx);
    assert!(
        before - after < 4.0,
        "the neighbour jumped {before} to {after} on the first frame"
    );
}

/// Citations [A, B, C], B taken off and then A before B has gone: the
/// ghosts close in the places they held, A, B, then C where it was.
#[gpui::test]
fn citations_removed_out_of_order_close_in_their_own_places(cx: &mut TestAppContext) {
    let (view, cx) = window("tray-reply-order", cx);
    cite(&view, &["a", "bbbbbbbbbbbbbbbbbbbb", "cccccccccc"], cx);
    let before = left("cite-chip-2", cx);
    press_remove("cite-chip-1-remove", cx);
    press_remove("cite-chip-0-remove", cx);
    let after = left("cite-chip-0", cx);
    assert!(
        (after - before).abs() < 4.0,
        "the last one moved {before} to {after}: the ghosts are out of order"
    );
}

/// The same for files: [A, B, C], A then C taken off.
#[gpui::test]
fn files_removed_out_of_order_close_in_their_own_places(cx: &mut TestAppContext) {
    let (view, cx) = window("tray-file-order", cx);
    let files = ["a.md", "bbbbbbbbbbbbbbbbbbbb.md", "cccccccccc.md"]
        .map(|name| std::path::PathBuf::from(format!("/nonexistent/{name}")));
    let composer = view.read_with(cx, |view, _| view.panes[0].composer.clone());
    composer.update(cx, |composer, cx| composer.add_files(&files, cx));
    tick(cx);
    at(ferrite_ui::theme::MOTION_CHIP_IN_MS, cx);
    let before = left("tray-file-1", cx);
    press_remove("tray-file-0-remove", cx);
    press_remove("tray-file-2-remove", cx);
    let after = left("tray-file-1", cx);
    assert!(
        (after - before).abs() < 4.0,
        "the middle one moved {before} to {after}: the ghosts are out of order"
    );
}
