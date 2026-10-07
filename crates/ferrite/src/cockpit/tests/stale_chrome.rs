//! The stale-chrome matrix: the Cockpit draws its nav and each Pane as a
//! cached part, rebuilt only when something it shows changes — so for every
//! way chrome changes, a test that what is on screen (what the part last
//! drew, `cockpit::drawn`) follows. A four-Pane Group, the motion kit live,
//! an active window, the keyboard in the first Pane.

use super::*;
use crate::cockpit::drawn;
use ferrite_core::roster::PaneIdentity;

/// Let `ms` pass a display frame at a time, delivering each frame asked for.
fn frames(cx: &mut gpui::VisualTestContext, ms: u64) {
    for _ in 0..ms / 16 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
}

struct Board<'a> {
    view: Entity<CockpitView>,
    fake: Fake,
    threads: Vec<ThreadId>,
    cx: &'a mut gpui::VisualTestContext,
}

/// Four Threads in one Group on the board, `working` of them mid-turn.
fn board<'a>(name: &str, working: &[usize], cx: &'a mut TestAppContext) -> Board<'a> {
    crate::motion::testing::drive();
    let (mut core, fake) = cockpit(name, 4);
    let group = group_all(&mut core);
    let threads = core.threads().to_vec();
    for index in working {
        core.send(threads[*index], "Keep going".into());
    }
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(1000.)));
    cx.update(|window, _| window.activate_window());
    view.update(cx, |view, cx| {
        view.enter_group(group, cx);
        view.focus_pane(0);
        cx.notify();
    });
    for index in working {
        fake.streams.borrow()[*index]
            .send(SessionEvent::ReasoningSummaryDelta {
                text: "**Working**".into(),
                summary_index: 0,
            })
            .unwrap();
    }
    tick(cx);
    frames(cx, 600);
    Board {
        view,
        fake,
        threads,
        cx,
    }
}

impl Board<'_> {
    fn pane(&self, thread: usize) -> drawn::Pane {
        drawn::pane(PaneIdentity::Thread(self.threads[thread]))
    }

    fn send(&self, thread: usize, event: SessionEvent) {
        self.fake.streams.borrow()[thread].send(event).unwrap();
    }

    fn rect(&mut self, thread: usize) -> layout::Rect {
        let identity = PaneIdentity::Thread(self.threads[thread]);
        let view = self.view.clone();
        self.cx.update(|window, cx| {
            let view = view.read(cx);
            let index = view.index_of(identity).expect("on the board");
            view.pane_rects(window)
                .into_iter()
                .find(|(at, _)| *at == index)
                .map(|(_, rect)| rect)
                .expect("laid out")
        })
    }
}

fn centre(rect: &layout::Rect) -> gpui::Point<Pixels> {
    gpui::point(px(rect.x + rect.w / 2.), px(rect.y + rect.h / 2.))
}

/// Attention: a Thread out of sight finishes; its Pane and its nav row say
/// so at once.
#[gpui::test]
fn a_finished_thread_out_of_sight_wears_attention(cx: &mut TestAppContext) {
    let b = board("stale-attention", &[1], cx);
    assert!(!b.pane(1).attention);
    b.send(
        1,
        SessionEvent::TextDelta {
            text: "Done here.".into(),
        },
    );
    b.send(
        1,
        SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        },
    );
    tick(b.cx);
    frames(b.cx, 300);
    assert!(b.pane(1).attention, "the Pane draws its attention");
    assert!(
        drawn::row(b.threads[1]).contains("unread=true"),
        "and its nav row: {}",
        drawn::row(b.threads[1])
    );
    assert!(!b.pane(0).attention, "the focused Pane never does");
}

/// Focus: a click moves the keyboard to another Pane; both Panes redraw
/// their focus, and the new one's line takes the keyboard.
#[gpui::test]
fn a_click_moves_focus_and_both_panes_redraw_it(cx: &mut TestAppContext) {
    let mut b = board("stale-focus", &[], cx);
    assert!(b.pane(0).focused && !b.pane(2).focused);
    let rect = b.rect(2);
    b.cx.simulate_click(centre(&rect), gpui::Modifiers::none());
    frames(b.cx, 200);
    assert!(b.pane(2).focused, "the clicked Pane draws its focus");
    assert!(!b.pane(0).focused, "the one it left draws none");
    assert!(b.pane(2).editing, "its line holds the keyboard");
}

/// Drop target: native files dragged over a Pane edge its Composer, and
/// leaving takes the edge away.
#[gpui::test]
fn dragged_files_edge_the_pane_under_them(cx: &mut TestAppContext) {
    let mut b = board("stale-drop", &[], cx);
    let rect = b.rect(1);
    b.cx.simulate_event(gpui::FileDropEvent::Entered {
        position: centre(&rect),
        paths: gpui::ExternalPaths(vec![here().join("notes.txt")].into()),
    });
    tick(b.cx);
    assert!(
        b.pane(1).drop_target,
        "the Pane under the files draws the edge"
    );
    assert!(!b.pane(0).drop_target);
    b.cx.simulate_event(gpui::FileDropEvent::Exited);
    tick(b.cx);
    assert!(!b.pane(1).drop_target, "left: no edge");
}

/// The nav's tween: cmd-B folds the column over its ride, and the Panes
/// follow the board's width every frame of it and land on the board's own
/// width.
#[gpui::test]
fn the_panes_ride_the_nav_fold(cx: &mut TestAppContext) {
    let mut b = board("stale-nav-ride", &[], cx);
    let view = b.view.clone();
    view.update(b.cx, |view, cx| view.set_nav_collapsed(true, cx));
    let mut widths = vec![b.pane(0).cell_width];
    for _ in 0..6 {
        frames(b.cx, 32);
        widths.push(b.pane(0).cell_width);
    }
    widths.dedup();
    assert!(
        widths.len() > 3,
        "the Pane redraws at the ride's widths: {widths:?}"
    );
    frames(b.cx, 400);
    let landed = b.pane(0).cell_width;
    let rect = b.rect(0);
    assert!(
        (landed - rect.w).abs() < 0.5,
        "and lands on the board's own width: {landed} vs {}",
        rect.w
    );
}

/// Usage meters: an account window reported by one Thread is the account's
/// — every Pane of that provider draws it.
#[gpui::test]
fn an_account_window_reaches_every_panes_meter(cx: &mut TestAppContext) {
    let b = board("stale-usage", &[1], cx);
    let before = b.pane(2).usage.clone();
    b.send(
        1,
        SessionEvent::RateLimits {
            five_hour: Some(ferrite_core::RateLimitWindow {
                used_fraction: 0.61,
                resets_at: None,
            }),
            weekly: None,
        },
    );
    tick(b.cx);
    frames(b.cx, 200);
    let after = b.pane(2).usage.clone();
    assert_ne!(after, before, "another Pane's meter draws the new window");
    assert!(after.unwrap_or_default().contains("0.61"));
}

/// The rename editor: opening it on a Pane's head draws it there; the name
/// committed reaches the nav row, and the editor leaves the head.
#[gpui::test]
fn a_rename_opens_in_the_head_and_lands_in_the_nav(cx: &mut TestAppContext) {
    let b = board("stale-rename", &[], cx);
    let thread = b.threads[1];
    let view = b.view.clone();
    view.update(b.cx, |view, cx| {
        view.start_rename(RenameTarget::PaneTitle(thread), cx)
    });
    frames(b.cx, 100);
    assert!(b.pane(1).renaming, "the head draws the editor");
    view.update(b.cx, |view, cx| {
        let (_, editor) = view.rename.clone().expect("the editor is open");
        editor.update(cx, |editor, cx| editor.set("Renamed thread".into(), cx));
        view.finish_rename(true, cx);
    });
    frames(b.cx, 100);
    assert!(!b.pane(1).renaming, "the editor has left the head");
    assert!(
        drawn::row(thread).ends_with("Renamed thread"),
        "the nav draws the new name: {}",
        drawn::row(thread)
    );
}

/// Reduced motion, turned on with a Thread mid-turn, reaches every Pane.
#[gpui::test]
fn reduced_motion_reaches_every_pane(cx: &mut TestAppContext) {
    let b = board("stale-reduced", &[0, 2], cx);
    assert!(!b.pane(2).reduce_motion);
    b.cx.update(|_, cx| cx.set_reduce_motion(true));
    frames(b.cx, 100);
    for thread in 0..4 {
        assert!(b.pane(thread).reduce_motion, "Pane {thread} draws it");
    }
    assert!(b.cx.debug_bounds("progress-mark-still").is_some());
}

/// Window activation: leaving the window takes the keyboard from every
/// line; coming back gives it to the focused Pane's again.
#[gpui::test]
fn leaving_and_returning_to_the_window_reaches_the_panes(cx: &mut TestAppContext) {
    let b = board("stale-activation", &[], cx);
    assert!(b.pane(0).editing);
    b.cx.deactivate_window();
    frames(b.cx, 100);
    assert!(!b.pane(0).editing, "an inactive window edits nowhere");
    b.cx.update(|window, _| window.activate_window());
    frames(b.cx, 100);
    assert!(b.pane(0).editing, "back: the focused line edits again");
}

/// Hover fades in the nav: a row under the pointer draws its wash in, and
/// out once the pointer leaves.
#[gpui::test]
fn a_nav_rows_hover_fades_in_and_out(cx: &mut TestAppContext) {
    let b = board("stale-hover", &[], cx);
    let row = debug_bounds(b.cx, format!("nav-thread-{}", b.threads[2].get())).expect("a row");
    b.cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
    frames(b.cx, 200);
    let key =
        b.cx.update(|_, _| crate::motion::testing::hover_values())
            .into_iter()
            .find(|(key, value)| key.starts_with("nav-thread-") && *value == 1.0)
            .map(|(key, _)| key)
            .expect("a row is hovered");
    assert_eq!(
        crate::nav::testing::drawn_hover(&key),
        Some(1.0),
        "drawn in"
    );
    b.cx.simulate_mouse_move(
        gpui::point(px(1200.), px(500.)),
        None,
        gpui::Modifiers::none(),
    );
    frames(b.cx, 300);
    assert!(
        crate::nav::testing::drawn_hover(&key).is_none_or(|t| t == 0.0),
        "drawn out: {:?}",
        crate::nav::testing::drawn_hover(&key)
    );
}

/// A popover: the `/` menu opens over the focused Pane's line as it is
/// typed, and closes on escape.
#[gpui::test]
fn the_slash_menu_opens_and_closes_over_its_pane(cx: &mut TestAppContext) {
    let b = board("stale-popover", &[], cx);
    bind_production_keys(b.cx);
    b.cx.simulate_input("/");
    frames(b.cx, 100);
    assert!(
        b.cx.debug_bounds("composer-menu-rows").is_some(),
        "the menu is drawn"
    );
    b.cx.simulate_keystrokes("escape");
    frames(b.cx, 100);
    assert!(
        b.cx.debug_bounds("composer-menu-rows").is_none(),
        "and gone on escape"
    );
}

/// Preferences: the reading size reaches every Pane's transcript, the
/// focused one's and the others'.
#[gpui::test]
fn the_reading_size_reaches_every_pane(cx: &mut TestAppContext) {
    let b = board("stale-prefs", &[], cx);
    for thread in 0..4 {
        b.send(
            thread,
            SessionEvent::TextDelta {
                text: format!("Pane {thread} answers."),
            },
        );
    }
    tick(b.cx);
    frames(b.cx, 100);
    let view = b.view.clone();
    let prefixes: Vec<String> = view.read_with(b.cx, |view, _| {
        view.panes
            .iter()
            .map(|pane| format!("markdown-{}-", pane.text_namespace()))
            .collect()
    });
    view.update(b.cx, |view, cx| {
        view.prefs.settings.reading_size = ferrite_core::settings::ReadingSize::nearest(18);
        cx.notify();
    });
    frames(b.cx, 100);
    for prefix in &prefixes {
        assert_eq!(
            b.cx.update(|_, cx| crate::rich::testing::font_size(prefix, cx)),
            Some(px(18.)),
            "{prefix} reads at the new size"
        );
    }
}

/// A working clock turns over on its Pane while the board sits still.
#[gpui::test]
fn a_working_clock_turns_over_on_its_pane(cx: &mut TestAppContext) {
    let b = board("stale-clock", &[1], cx);
    let before = b
        .pane(1)
        .clock
        .clone()
        .expect("the premise: a working clock");
    // The turn's clock is the wall clock: let a real second pass, and the
    // pulse clock's ride for it.
    std::thread::sleep(Duration::from_millis(1_100));
    frames(b.cx, 1_100);
    let after = b.pane(1).clock.clone().unwrap_or_default();
    assert_ne!(after, before, "the Pane redrew its clock");
}

/// A choice menu: the model picker opens placed on its Composer's edge
/// with no frame asked for beyond its own, and its drawn cursor follows the
/// arrows.
#[gpui::test]
fn the_model_picker_opens_placed_and_its_cursor_follows_the_arrows(cx: &mut TestAppContext) {
    let b = board("stale-picker", &[], cx);
    bind_production_keys(b.cx);
    let thread = b.threads[0];
    b.view
        .update(b.cx, |view, cx| view.open_provider_picker(thread, cx));
    frames(b.cx, 300);
    let drawn = crate::menu::testing::drawn();
    assert!(drawn.placed, "drawn placed, not left at zero opacity");
    let menu =
        b.cx.debug_bounds("choice-menu")
            .expect("the picker is open");
    b.cx.update(|window, _| window.refresh());
    frames(b.cx, 50);
    assert_eq!(
        b.cx.debug_bounds("choice-menu"),
        Some(menu),
        "already where a full redraw puts it"
    );
    b.cx.simulate_keystrokes("down");
    frames(b.cx, 50);
    let moved = crate::menu::testing::drawn().cursor;
    assert!(
        moved.is_some() && moved != drawn.cursor,
        "↓ moves the drawn cursor: {:?} → {moved:?}",
        drawn.cursor
    );
    b.cx.simulate_keystrokes("up");
    frames(b.cx, 50);
    assert_eq!(
        crate::menu::testing::drawn().cursor,
        drawn.cursor,
        "and ↑ back"
    );
}
