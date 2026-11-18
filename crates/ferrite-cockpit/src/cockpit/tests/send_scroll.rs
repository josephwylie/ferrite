//! The send scroll (T3 Code's): a sent prompt eases up to the top of the
//! transcript, its bubble popping once it lands; any scroll by the operator
//! stops it where it stands; its turn's first work lets it go to the tail.
//! And Scroll to end, the pill that takes the reader back to the tail.

use super::*;
use ferrite_ui::theme::{MOTION_SCROLL_MAX_MS, SCROLL_END_SHOW_DELAY_MS};

/// Deliver the frames the last draw asked for: how many it asked for.
fn frame(cx: &mut gpui::VisualTestContext) -> usize {
    let frames = cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    frames
}

/// Step the clock `ms` and draw the frame due then.
pub(super) fn at(ms: u64, cx: &mut gpui::VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    frame(cx);
}

pub(super) fn transcript(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> Entity<ferrite_pane::transcript::TranscriptView> {
    view.read_with(cx, |view, _| view.panes[0].transcript().unwrap().clone())
}

pub(super) fn scroll(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> ferrite_pane::transcript::TranscriptScroll {
    let transcript = transcript(view, cx);
    transcript.read_with(cx, |transcript, _| transcript.scroll().clone())
}

/// A finished first turn far longer than the view, the operator typing
/// the next prompt; `drive` runs the motion kit live.
pub(super) fn answered<'a>(
    name: &str,
    drive: bool,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, Fake, &'a mut gpui::VisualTestContext) {
    let answer = (0..80).map(|n| format!("Answer line {n}\n\n")).collect();
    answered_with(name, drive, answer, cx)
}

/// `answered`, the first turn's answer being `answer`.
pub(super) fn answered_with<'a>(
    name: &str,
    drive: bool,
    answer: String,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, Fake, &'a mut gpui::VisualTestContext) {
    let (mut core, fake) = cockpit(name, 1);
    let thread = core.threads()[0];
    let _ = core.send(thread, "first".into());
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("enter", Submit, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta { text: answer })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    if drive {
        ferrite_ui::motion::testing::drive();
    }
    cx.simulate_input("second");
    (view, fake, cx)
}

/// Send what was typed, and draw the frame that adds its prompt.
pub(super) fn send(cx: &mut gpui::VisualTestContext) {
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

#[gpui::test]
fn a_send_eases_its_prompt_to_the_top(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("send-eases", true, cx);
    let scroll = scroll(&view, cx);
    let start = scroll.offset().y;
    send(cx);
    assert!(scroll.easing(), "the send eases");
    let mut seen = Vec::new();
    let mut elapsed = 0;
    while elapsed <= MOTION_SCROLL_MAX_MS + 16 {
        at(16, cx);
        elapsed += 16;
        seen.push(scroll.offset().y);
    }
    assert!(!scroll.easing(), "landed by {MOTION_SCROLL_MAX_MS}ms and a frame");
    let target = scroll.offset().y;
    assert!(target < start - px(10.), "it moved down the content");
    assert!(
        seen.iter()
            .any(|y| *y < start - px(0.5) && *y > target + px(0.5)),
        "frames between show it on its way: {start:?} {seen:?} {target:?}"
    );
    at(16, cx);
    assert!((scroll.offset().y - target).abs() <= px(0.5), "it stays");
    let prompt = cx.debug_bounds("transcript-prompt").expect("the new prompt");
    let top = scroll.bounds().top();
    assert!(
        prompt.top() >= top && prompt.top() < top + scroll.bounds().size.height / 3.,
        "the prompt stands at the top of the view: {prompt:?} in {:?}",
        scroll.bounds()
    );
}

/// Mid-ease, the operator's scroll (`take`) stops it where it stands: not
/// at the target, the ease gone, the prompt no longer held.
fn stopped_mid_ease(
    name: &str,
    cx: &mut TestAppContext,
    take: impl FnOnce(&Entity<CockpitView>, &mut gpui::VisualTestContext),
) {
    let (view, fake, cx) = answered(name, true, cx);
    let scroll = scroll(&view, cx);
    send(cx);
    at(16, cx);
    at(16, cx);
    assert!(scroll.easing(), "the premise: on its way");
    take(&view, cx);
    at(16, cx);
    assert!(!scroll.easing(), "the ease is gone");
    let transcript = transcript(&view, cx);
    assert!(
        !transcript.read_with(cx, |transcript, _| transcript.send_anchored()),
        "the prompt is no longer held"
    );
    let stood = scroll.offset().y;
    for _ in 0..20 {
        at(16, cx);
    }
    assert!(
        (scroll.offset().y - stood).abs() <= px(0.5),
        "it stays where it stopped, not snapping on: {stood:?} → {:?}",
        scroll.offset().y
    );
    later_text_and_a_tool(&fake, cx);
    assert!(
        (scroll.offset().y - stood).abs() <= px(0.5),
        "the reader's passage stays put as the turn's text and tool arrive: {stood:?} → {:?}",
        scroll.offset().y
    );
}

/// The running turn's next text, then a tool call and its result.
fn later_text_and_a_tool(fake: &Fake, cx: &mut gpui::VisualTestContext) {
    let stream = fake.streams.borrow().len() - 1;
    let events = [
        SessionEvent::TextDelta {
            text: "Looking at it now.".into(),
        },
        SessionEvent::ToolStarted {
            id: "later-tool".into(),
            name: "Read".into(),
            input: serde_json::json!({"file_path": "/workspace/a.rs"}),
        },
        SessionEvent::ToolCompleted {
            id: "later-tool".into(),
            output: "12 lines".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        },
    ];
    for event in events {
        fake.streams.borrow()[stream].send(event).unwrap();
    }
    tick(cx);
    for _ in 0..4 {
        at(16, cx);
    }
}

#[gpui::test]
fn a_wheel_mid_ease_stops_it_where_it_stands(cx: &mut TestAppContext) {
    stopped_mid_ease("send-ease-wheel", cx, |view, cx| {
        let pointer = scroll(view, cx).bounds().center();
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: pointer,
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(40.))),
            modifiers: gpui::Modifiers::none(),
            touch_phase: gpui::TouchPhase::default(),
        });
        cx.run_until_parked();
    });
}

#[gpui::test]
fn a_page_up_mid_ease_stops_it(cx: &mut TestAppContext) {
    stopped_mid_ease("send-ease-page", cx, |view, cx| {
        transcript(view, cx).update(cx, |transcript, cx| {
            transcript.page(ferrite_pane::transcript::Page::Up, cx)
        });
        cx.run_until_parked();
    });
}

#[gpui::test]
fn a_scrollbar_drag_mid_ease_stops_it(cx: &mut TestAppContext) {
    stopped_mid_ease("send-ease-drag", cx, |view, cx| {
        scroll(view, cx).list_state().scrollbar_drag_started();
    });
}

fn thumb(cx: &mut gpui::VisualTestContext) -> gpui::Bounds<gpui::Pixels> {
    let layer = cx
        .debug_bounds("transcript-scrollbar")
        .expect("the scrollbar layer");
    ferrite_ui::painted_thumb_in(layer).expect("the painted thumb")
}

fn drag_thumb_up(by: f32, cx: &mut gpui::VisualTestContext) {
    let grab = thumb(cx).center();
    cx.simulate_mouse_move(grab, None, gpui::Modifiers::none());
    cx.simulate_mouse_down(grab, gpui::MouseButton::Left, gpui::Modifiers::none());
    std::thread::sleep(Duration::from_millis(12));
    let to = grab - gpui::point(px(0.), px(by));
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
}

fn click_track_above_thumb(cx: &mut gpui::VisualTestContext) {
    let thumb = thumb(cx);
    let click = thumb.center() - gpui::point(px(0.), thumb.size.height + px(40.));
    cx.simulate_click(click, gpui::Modifiers::none());
    cx.run_until_parked();
}

/// A prompt held at the top with the room for its answer, its ease done.
fn held_prompt<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    Fake,
    ferrite_pane::transcript::TranscriptScroll,
    &'a mut gpui::VisualTestContext,
) {
    let (view, fake, cx) = answered(name, true, cx);
    let scroll = scroll(&view, cx);
    send(cx);
    for _ in 0..(MOTION_SCROLL_MAX_MS / 16 + 4) {
        at(16, cx);
    }
    assert!(!scroll.easing(), "the premise: the ease is done");
    let held = transcript(&view, cx).read_with(cx, |t, _| t.send_anchored() && t.runway_room() > 0.);
    assert!(held, "the premise: the prompt is held with room under it");
    (view, fake, scroll, cx)
}

/// A thumb drag after the send's ease has finished, with the runway still
/// under the prompt, takes the scroll: later text and a tool call leave the
/// reader's passage where it is.
#[gpui::test]
fn a_thumb_drag_after_the_ease_keeps_the_reader_put(cx: &mut TestAppContext) {
    let (view, fake, scroll, cx) = held_prompt("send-thumb", cx);
    drag_thumb_up(60., cx);
    let stood = scroll.offset().y;
    let held = |cx: &mut gpui::VisualTestContext| {
        transcript(&view, cx).read_with(cx, |t, _| t.send_anchored())
    };
    assert!(!held(cx), "the drag took the prompt's hold");
    later_text_and_a_tool(&fake, cx);
    assert!(!scroll.is_following_tail(), "not pulled to the tail");
    assert!(
        (scroll.offset().y - stood).abs() <= px(0.5),
        "the passage stays: {stood:?} → {:?}",
        scroll.offset().y
    );
}

/// A click on the scrollbar's track is as much the reader's as a wheel is.
#[gpui::test]
fn a_track_click_after_the_ease_keeps_the_reader_put(cx: &mut TestAppContext) {
    let (view, fake, scroll, cx) = held_prompt("send-track", cx);
    click_track_above_thumb(cx);
    let stood = scroll.offset().y;
    assert!(
        !transcript(&view, cx).read_with(cx, |t, _| t.send_anchored()),
        "the click took the prompt's hold"
    );
    later_text_and_a_tool(&fake, cx);
    assert!(
        (scroll.offset().y - stood).abs() <= px(0.5),
        "the passage stays: {stood:?} → {:?}",
        scroll.offset().y
    );
}

/// Find on a hit in a row already in view is the reader's too: the ease
/// and the prompt's hold go with it.
#[gpui::test]
fn find_in_a_visible_row_takes_the_prompts_hold(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (view, _fake, cx) = answered("send-find", true, cx);
    let scroll = scroll(&view, cx);
    send(cx);
    at(16, cx);
    cx.simulate_keystrokes("cmd-f");
    tick(cx);
    cx.simulate_input("second");
    tick(cx);
    for _ in 0..8 {
        transcript(&view, cx).update(cx, |_, cx| cx.notify());
        at(16, cx);
    }
    assert!(
        !transcript(&view, cx).read_with(cx, |t, _| t.send_anchored()),
        "Find took the prompt's hold"
    );
    assert!(!scroll.easing(), "Find stopped the ease");
}

/// End and the Scroll to end pill are one move at two speeds: both land on
/// the tail, with no runway left under the last row.
fn lands_on_the_tail(name: &str, pill: bool, cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (view, _fake, cx) = answered(name, true, cx);
    send(cx);
    for _ in 0..(MOTION_SCROLL_MAX_MS / 16 + 4) {
        at(16, cx);
    }
    scrolled_away(&view, cx);
    at(SCROLL_END_SHOW_DELAY_MS + 10, cx);
    if pill {
        let pill = cx.debug_bounds("scroll-to-end").expect("the pill");
        cx.simulate_click(pill.center(), gpui::Modifiers::none());
    } else {
        cx.simulate_keystrokes("end");
    }
    for _ in 0..(MOTION_SCROLL_MAX_MS / 16 + 4) {
        at(16, cx);
    }
    let scroll = scroll(&view, cx);
    assert!(scroll.is_following_tail(), "following the tail");
    let room = transcript(&view, cx).read_with(cx, |t, _| t.runway_room());
    assert_eq!(room, 0., "no runway under the last row");
    assert!(
        (scroll.offset().y + scroll.max_offset().y).abs() <= px(0.5),
        "at the tail: {:?} of {:?}",
        scroll.offset().y,
        scroll.max_offset().y
    );
}

#[gpui::test]
fn end_lands_on_the_tail(cx: &mut TestAppContext) {
    lands_on_the_tail("end-key", false, cx);
}

#[gpui::test]
fn the_pill_lands_on_the_tail(cx: &mut TestAppContext) {
    lands_on_the_tail("end-pill", true, cx);
}

/// Under reduced motion (the kit at rest) the send lands in one frame,
/// `TX_SEND_ANCHOR_OFFSET` under the top, the room for its answer below.
#[gpui::test]
fn a_send_lands_at_once_without_motion(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("send-snaps", false, cx);
    let scroll = scroll(&view, cx);
    send(cx);
    assert!(!scroll.easing(), "no ease");
    tick(cx);
    let prompt = cx.debug_bounds("transcript-prompt").expect("the new prompt");
    let top = scroll.bounds().top();
    assert!(
        (prompt.top() - top - px(ferrite_ui::theme::TX_SEND_ANCHOR_OFFSET)).abs() <= px(1.),
        "the bubble lands TX_SEND_ANCHOR_OFFSET under the top: {prompt:?} in {:?}",
        scroll.bounds()
    );
    let transcript = transcript(&view, cx);
    assert!(transcript.read_with(cx, |transcript, _| transcript.runway_room()) > 0.);
}

/// The bubble's pop waits for the ease: held at its first frame on the way
/// up, it plays its whole `BUBBLE_IN` from the landing.
#[gpui::test]
fn the_bubble_pops_when_the_send_lands(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("send-pop", true, cx);
    let scroll = scroll(&view, cx);
    let transcript = transcript(&view, cx);
    let arriving = |cx: &mut gpui::VisualTestContext| {
        transcript.read_with(cx, |transcript, cx| transcript.bubbles_arriving(cx))
    };
    send(cx);
    let mut elapsed = 0;
    while scroll.easing() {
        assert!(
            arriving(cx).iter().all(|t| *t == 0.),
            "held at t = 0 on its way"
        );
        at(16, cx);
        elapsed += 16;
        assert!(elapsed < 1000, "the ease lands");
    }
    // The frame after the landing draws it first; the next is under way.
    at(16, cx);
    at(16, cx);
    let moving = arriving(cx);
    assert!(
        moving.iter().any(|t| *t > 0. && *t < 1.),
        "it pops from the landing: {moving:?}"
    );
    let bubble = ferrite_ui::theme::MOTION_BUBBLE_FADE_MS;
    at(bubble - 64, cx);
    assert!(
        !arriving(cx).is_empty(),
        "it plays its whole {bubble}ms from the landing"
    );
}

/// The sent prompt's turn starts work: the room goes and the transcript
/// follows the tail. A prose-only answer keeps the prompt at the top.
#[gpui::test]
fn the_turns_first_call_lets_the_prompt_go_to_the_tail(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("send-release", false, cx);
    send(cx);
    tick(cx);
    let transcript = transcript(&view, cx);
    let scroll = scroll(&view, cx);
    let stream = fake.streams.borrow().len() - 1;
    fake.streams.borrow()[stream]
        .send(SessionEvent::TextDelta {
            text: "Looking.".into(),
        })
        .unwrap();
    tick(cx);
    assert!(
        transcript.read_with(cx, |transcript, _| transcript.send_anchored()),
        "prose keeps the prompt held"
    );
    let prompt = cx.debug_bounds("transcript-prompt").expect("the new prompt");
    assert!(
        prompt.top() < scroll.bounds().top() + px(2. * ferrite_ui::theme::TX_SEND_ANCHOR_OFFSET),
        "the prompt stays at the top: {prompt:?}"
    );
    fake.streams.borrow()[stream]
        .send(SessionEvent::ToolStarted {
            id: "release-tool".into(),
            name: "Read".into(),
            input: serde_json::json!({"file_path": "/workspace/a.rs"}),
        })
        .unwrap();
    fake.streams.borrow()[stream]
        .send(SessionEvent::ToolCompleted {
            id: "release-tool".into(),
            output: "12 lines".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    tick(cx);
    transcript.read_with(cx, |transcript, _| {
        assert!(!transcript.send_anchored(), "the call lets it go");
        assert_eq!(transcript.runway_room(), 0., "the room goes");
    });
    assert!(scroll.is_following_tail(), "at the tail, following");
}

/// The turn's first call landing while the send still eases up lets the
/// prompt go as one landing after the ease does: the view ends at the
/// tail, following.
#[gpui::test]
fn a_call_mid_ease_lets_the_prompt_go_to_the_tail(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("mid-ease", true, cx);
    let scroll = scroll(&view, cx);
    send(cx);
    at(16, cx);
    at(16, cx);
    assert!(scroll.easing(), "premise: on its way up");
    let stream = fake.streams.borrow().len() - 1;
    fake.streams.borrow()[stream]
        .send(SessionEvent::ToolStarted {
            id: "mid".into(),
            name: "Read".into(),
            input: serde_json::json!({"file_path": "/workspace/a.rs"}),
        })
        .unwrap();
    fake.streams.borrow()[stream]
        .send(SessionEvent::ToolCompleted {
            id: "mid".into(),
            output: "1 line".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        })
        .unwrap();
    cx.run_until_parked();
    for _ in 0..60 {
        at(16, cx);
    }
    let transcript = transcript(&view, cx);
    let held = transcript.read_with(cx, |t, _| t.send_anchored());
    assert!(!held, "the call lets the prompt go");
    assert!(scroll.is_following_tail(), "at the tail, following");
}

/// Scrolled away to the top of a long transcript.
pub(super) fn scrolled_away(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    let scroll = scroll(view, cx);
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: scroll.bounds().center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(600.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::default(),
    });
    cx.run_until_parked();
    assert!(!scroll.is_following_tail(), "the premise: scrolled away");
}

#[gpui::test]
fn scroll_to_end_shows_after_its_delay_and_hides_at_once(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("scroll-end-delay", false, cx);
    scrolled_away(&view, cx);
    assert!(cx.debug_bounds("scroll-to-end").is_none(), "not at once");
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    assert!(cx.debug_bounds("scroll-to-end").is_none(), "absent at 100ms");
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.run_until_parked();
    let pill = cx.debug_bounds("scroll-to-end").expect("present at 160ms");
    assert_eq!(pill.size.height, px(ferrite_ui::theme::SCROLL_END_H));
    let list = scroll(&view, cx).bounds();
    assert_eq!(
        list.bottom() - pill.bottom(),
        px(ferrite_ui::theme::SCROLL_END_INSET),
        "its foot is SCROLL_END_INSET above the Composer's top"
    );
    // Back at the tail: gone on the very next frame.
    transcript(&view, cx).update(cx, |transcript, cx| {
        transcript.page(ferrite_pane::transcript::Page::Bottom, cx)
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("scroll-to-end").is_none(), "gone at once");
}

#[gpui::test]
fn scroll_to_end_no_longer_wanted_asks_for_no_frame(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("scroll-end-unwanted", false, cx);
    scrolled_away(&view, cx);
    let transcript = transcript(&view, cx);
    transcript.update(cx, |transcript, cx| {
        transcript.page(ferrite_pane::transcript::Page::Bottom, cx)
    });
    cx.run_until_parked();
    let notified = std::rc::Rc::new(std::cell::Cell::new(0));
    let _count = cx.update({
        let notified = notified.clone();
        |_, cx| cx.observe(&transcript, move |_, _| notified.set(notified.get() + 1))
    });
    at(SCROLL_END_SHOW_DELAY_MS + 10, cx);
    assert!(
        cx.debug_bounds("scroll-to-end").is_none(),
        "back at the tail"
    );
    assert_eq!(notified.get(), 0, "the delay's timer draws nothing");
}

#[gpui::test]
fn scroll_to_end_eases_to_the_tail_keeping_the_composers_focus(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("scroll-end-click", true, cx);
    scrolled_away(&view, cx);
    at(SCROLL_END_SHOW_DELAY_MS + 10, cx);
    let pill = cx.debug_bounds("scroll-to-end").expect("the pill");
    let composer_focused = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            view.read_with(cx, |view, cx| {
                view.panes[0].composer.focus_handle(cx).is_focused(window)
            })
        })
    };
    assert!(composer_focused(cx), "premise: typing in the Composer");
    cx.simulate_click(pill.center(), gpui::Modifiers::none());
    let scroll = scroll(&view, cx);
    assert!(cx.debug_bounds("scroll-to-end").is_none(), "it goes at once");
    assert!(scroll.easing(), "it eases");
    for _ in 0..20 {
        at(16, cx);
    }
    assert!(!scroll.easing(), "landed");
    assert!(scroll.is_following_tail(), "following the tail again");
    assert!(composer_focused(cx), "the Composer keeps focus");
}

/// A send behind a running turn waits in the queue: it neither holds a
/// prompt at the top nor scrolls, and Scroll to end names it.
#[gpui::test]
fn a_queued_send_stays_put_and_scroll_to_end_names_it(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("send-queued", false, cx);
    send(cx);
    tick(cx);
    // Past the room the running turn's prompt holds below it.
    scrolled_away(&view, cx);
    scrolled_away(&view, cx);
    let scroll = scroll(&view, cx);
    let transcript = transcript(&view, cx);
    let stood = scroll.offset().y;
    cx.simulate_input("third");
    send(cx);
    tick(cx);
    let queued = view.read_with(cx, |view, _| {
        let thread = view.cockpit.threads()[0];
        view.cockpit.thread(thread).map(|open| open.queue().len())
    });
    assert_eq!(queued, Some(1), "the premise: it was queued");
    assert!(!scroll.easing(), "no ease");
    assert!(
        !transcript.read_with(cx, |transcript, _| transcript.send_anchored()),
        "no prompt held"
    );
    assert!((scroll.offset().y - stood).abs() <= px(0.5), "it stays put");
    cx.executor()
        .advance_clock(Duration::from_millis(SCROLL_END_SHOW_DELAY_MS + 10));
    cx.run_until_parked();
    assert!(cx.debug_bounds("scroll-to-end").is_some(), "the pill");
    assert!(
        cx.debug_bounds("scroll-to-end-queued").is_some(),
        "it reads `Scroll to end · 1 queued`"
    );
}

/// While the find bar is open the pill stays away.
#[gpui::test]
fn scroll_to_end_waits_out_the_find_bar(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (view, _fake, cx) = answered("scroll-end-find", false, cx);
    scrolled_away(&view, cx);
    cx.simulate_keystrokes("cmd-f");
    tick(cx);
    assert!(cx.debug_bounds("find-bar").is_some(), "the premise: find is open");
    cx.executor()
        .advance_clock(Duration::from_millis(SCROLL_END_SHOW_DELAY_MS + 10));
    cx.run_until_parked();
    assert!(cx.debug_bounds("scroll-to-end").is_none(), "no pill over find");
}

/// Mid-ease on its way to the tail, the Scroll to end pill's reader takes
/// the scroll (`take`): the ease is gone, the passage stays where it
/// stopped as the turn's text and a tool call arrive, and the tail does
/// not take it back.
fn pill_stopped_mid_ease(
    name: &str,
    cx: &mut TestAppContext,
    take: impl FnOnce(&Entity<CockpitView>, &mut gpui::VisualTestContext),
) {
    let (view, fake, cx) = answered(name, true, cx);
    send(cx);
    for _ in 0..(MOTION_SCROLL_MAX_MS / 16 + 4) {
        at(16, cx);
    }
    scrolled_away(&view, cx);
    at(SCROLL_END_SHOW_DELAY_MS + 10, cx);
    let pill = cx.debug_bounds("scroll-to-end").expect("the pill");
    cx.simulate_click(pill.center(), gpui::Modifiers::none());
    let scroll = scroll(&view, cx);
    at(16, cx);
    at(16, cx);
    assert!(scroll.easing(), "the premise: the pill's ease is on its way");
    take(&view, cx);
    at(16, cx);
    assert!(!scroll.easing(), "the ease is gone");
    let stood = scroll.offset().y;
    for _ in 0..20 {
        at(16, cx);
    }
    later_text_and_a_tool(&fake, cx);
    assert!(
        (scroll.offset().y - stood).abs() <= px(0.5),
        "the passage stays put: {stood:?} → {:?}",
        scroll.offset().y
    );
    assert!(!scroll.is_following_tail(), "not pulled to the tail");
}

#[gpui::test]
fn a_wheel_mid_pill_ease_stops_it(cx: &mut TestAppContext) {
    pill_stopped_mid_ease("pill-wheel", cx, |view, cx| {
        let pointer = scroll(view, cx).bounds().center();
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: pointer,
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(40.))),
            modifiers: gpui::Modifiers::none(),
            touch_phase: gpui::TouchPhase::default(),
        });
        cx.run_until_parked();
    });
}

#[gpui::test]
fn a_page_up_mid_pill_ease_stops_it(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    pill_stopped_mid_ease("pill-page", cx, |_, cx| {
        cx.simulate_keystrokes("pageup");
        cx.run_until_parked();
    });
}

#[gpui::test]
fn a_thumb_drag_mid_pill_ease_stops_it(cx: &mut TestAppContext) {
    pill_stopped_mid_ease("pill-drag", cx, |_, cx| drag_thumb_up(60., cx));
}

#[gpui::test]
fn a_track_click_mid_pill_ease_stops_it(cx: &mut TestAppContext) {
    pill_stopped_mid_ease("pill-track", cx, |_, cx| click_track_above_thumb(cx));
}

/// A long answer whose first words are far above the view's end.
fn tall_answer() -> String {
    format!("Needle opens it. {}\n\n", "alpha beta gamma delta epsilon ".repeat(300))
}

/// Find on a hit at the head of a tall row, the row in view, scrolls within
/// the row (`settle_found`). The send's ease it stopped does not expire
/// and take the hit back, nor do later text and a tool call.
#[gpui::test]
fn find_in_a_tall_visible_row_owns_the_result(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (view, fake, cx) = answered_with("send-find-tall", true, tall_answer(), cx);
    let scroll = scroll(&view, cx);
    send(cx);
    at(16, cx);
    assert!(scroll.easing(), "the premise: the send's ease is on its way");
    cx.simulate_keystrokes("cmd-f");
    tick(cx);
    cx.simulate_input("needle");
    tick(cx);
    for _ in 0..8 {
        transcript(&view, cx).update(cx, |_, cx| cx.notify());
        at(16, cx);
    }
    let found = |cx: &mut gpui::VisualTestContext| {
        transcript(&view, cx)
            .read_with(cx, |transcript, _| transcript.find_state())
            .and_then(|state| state.painted)
            .expect("the hit painted")
    };
    let hit = found(cx);
    let shown = scroll.bounds();
    assert!(
        hit.top() >= shown.top() && hit.bottom() <= shown.bottom(),
        "the hit is in view: {hit:?} in {shown:?}"
    );
    let stood = scroll.offset().y;
    for _ in 0..(2 * MOTION_SCROLL_MAX_MS / 16) {
        at(16, cx);
    }
    later_text_and_a_tool(&fake, cx);
    assert!(
        (scroll.offset().y - stood).abs() <= px(0.5),
        "the hit stays in place: {stood:?} → {:?}",
        scroll.offset().y
    );
    assert!((found(cx).top() - hit.top()).abs() <= px(0.5), "and so does the hit");
    assert!(!scroll.easing() && !scroll.is_following_tail());
}

/// The reader's wheel while a Find reveal still waits for its hit to paint
/// takes the scroll: the reveal does not settle over it.
#[gpui::test]
fn a_wheel_before_find_settles_keeps_the_reader_put(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (view, _fake, cx) = answered_with("find-wheel", true, tall_answer(), cx);
    let scroll = scroll(&view, cx);
    cx.simulate_keystrokes("cmd-f");
    tick(cx);
    cx.simulate_input("needle");
    tick(cx);
    cx.update(|window, cx| window.dispatch_action(Box::new(ferrite_pane::find::Next), cx));
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: scroll.bounds().center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-600.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::default(),
    });
    for _ in 0..8 {
        transcript(&view, cx).update(cx, |_, cx| cx.notify());
        at(16, cx);
    }
    let shown = scroll.bounds();
    let hit = transcript(&view, cx)
        .read_with(cx, |transcript, _| transcript.find_state())
        .and_then(|state| state.painted)
        .expect("the hit painted");
    assert!(
        hit.bottom() < shown.top() || hit.top() > shown.bottom(),
        "the reveal did not settle over the wheel: {hit:?} in {shown:?}"
    );
}

/// Opening a settled turn's work (a fold pressed) leaves the heading the
/// reader pressed where it stood, whichever way the rows under it grow.
#[gpui::test]
fn expanding_completed_work_keeps_the_clicked_heading(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("fold-anchor", 1);
    let thread = core.threads()[0];
    let (view, cx) = {
        assert_eq!(core.send(thread, "turn 0".into()), ferrite_core::cockpit::Admission::Delivered);
        add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx))
    };
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    for turn in 0..6 {
        if turn > 0 {
            let admission = view.update(cx, |view, _| view.cockpit.send(thread, format!("turn {turn}")));
            assert_eq!(admission, ferrite_core::cockpit::Admission::Delivered);
        }
        let stream = fake.streams.borrow().len() - 1;
        let send = |event| fake.streams.borrow()[stream].send(event).unwrap();
        send(SessionEvent::ToolStarted {
            id: format!("fold-tool-{turn}"),
            name: "Read".into(),
            input: serde_json::json!({"file_path": "/workspace/a.rs"}),
        });
        send(SessionEvent::ToolCompleted {
            id: format!("fold-tool-{turn}"),
            output: "12 lines".into(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        });
        send(SessionEvent::TextDelta {
            text: (0..14).map(|n| format!("Turn {turn} line {n}\n\n")).collect(),
        });
        send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        });
        tick(cx);
    }
    let scroll = scroll(&view, cx);
    transcript(&view, cx).update(cx, |transcript, cx| {
        transcript.page(ferrite_pane::transcript::Page::Top, cx)
    });
    tick(cx);
    for _ in 0..80 {
        let view_bounds = scroll.bounds();
        let mid = cx.debug_bounds("turn-fold").filter(|bounds| {
            bounds.top() > view_bounds.top() + view_bounds.size.height / 4.
                && bounds.bottom() < view_bounds.bottom() - view_bounds.size.height / 4.
        });
        let Some(bounds) = mid else {
            cx.simulate_event(gpui::ScrollWheelEvent {
                position: view_bounds.center(),
                delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-80.))),
                modifiers: gpui::Modifiers::none(),
                touch_phase: gpui::TouchPhase::default(),
            });
            tick(cx);
            continue;
        };
        let before = bounds.top();
        assert!(!scroll.is_following_tail(), "the premise: the reader is mid-history");
        let words = cx.debug_bounds("turn-fold-words").expect("the fold's words");
        cx.simulate_click(words.center(), gpui::Modifiers::none());
        for _ in 0..4 {
            at(16, cx);
        }
        assert!(
            cx.debug_bounds("turn-fold-open").is_some(),
            "the press opened the work"
        );
        let after = cx.debug_bounds("turn-fold").expect("the heading is still drawn").top();
        assert!(
            (after - before).abs() <= px(0.5),
            "the heading stays put: {before:?} → {after:?}"
        );
        return;
    }
    panic!("no fold came to mid-view");
}

/// At the followed tail too: opening the last turn's work stops following,
/// and the heading the reader pressed stays where it stood.
#[gpui::test]
fn expanding_work_at_the_followed_tail_keeps_the_clicked_heading(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("fold-tail", 1);
    let thread = core.threads()[0];
    let (view, cx) = {
        assert_eq!(core.send(thread, "turn 0".into()), ferrite_core::cockpit::Admission::Delivered);
        add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx))
    };
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    for turn in 0..3 {
        if turn > 0 {
            let admission = view.update(cx, |view, _| view.cockpit.send(thread, format!("turn {turn}")));
            assert_eq!(admission, ferrite_core::cockpit::Admission::Delivered);
        }
        let stream = fake.streams.borrow().len() - 1;
        let send = |event| fake.streams.borrow()[stream].send(event).unwrap();
        for tool in 0..8 {
            send(SessionEvent::ToolStarted {
                id: format!("tail-tool-{turn}-{tool}"),
                name: "Read".into(),
                input: serde_json::json!({"file_path": format!("/workspace/{tool}.rs")}),
            });
            send(SessionEvent::ToolCompleted {
                id: format!("tail-tool-{turn}-{tool}"),
                output: "12 lines".into(),
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            });
        }
        let lines = if turn == 2 { 2 } else { 14 };
        send(SessionEvent::TextDelta {
            text: (0..lines).map(|n| format!("Turn {turn} line {n}\n\n")).collect(),
        });
        send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        });
        tick(cx);
    }
    for _ in 0..8 {
        at(16, cx);
    }
    let scroll = scroll(&view, cx);
    assert!(scroll.is_following_tail(), "the premise: the reader follows the tail");
    let shown = scroll.bounds();
    let fold = cx.debug_bounds("turn-fold").expect("the last turn's fold");
    assert!(
        fold.top() > shown.top() + shown.size.height / 4. && fold.bottom() < shown.bottom(),
        "the premise: the fold is in view, clear of the pinned band: {fold:?} in {shown:?}"
    );
    let words = cx.debug_bounds("turn-fold-words").expect("the fold's words");
    cx.simulate_click(words.center(), gpui::Modifiers::none());
    for _ in 0..4 {
        at(16, cx);
    }
    assert!(cx.debug_bounds("turn-fold-open").is_some(), "the press opened the work");
    let after = cx.debug_bounds("turn-fold").expect("the heading is still drawn").top();
    assert!(
        (after - fold.top()).abs() <= px(0.5),
        "the heading stays put: {:?} → {after:?}",
        fold.top()
    );
    assert!(!scroll.is_following_tail(), "the reader's press stopped following");
}
