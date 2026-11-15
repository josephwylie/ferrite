//! Where the reader stands while the layout changes around them: the passage
//! held through a reflow, a long jump that stays continuous, and paging that
//! leaves no line unread. Each test records the frames between, because a
//! final offset alone cannot tell a continuous move from a leap.
use super::send_scroll::{answered, answered_with, at, scroll, scrolled_away, send, transcript};
use super::*;
use ferrite_pane::transcript::Page;
use ferrite_ui::theme::{MOTION_SCROLL_MAX_MS, SCROLL_END_SHOW_DELAY_MS};

/// How far a held passage may drift: a fraction of a row cannot know a
/// paragraph's fixed gap, so the passage lands within a few lines.
const HELD_TOLERANCE: f32 = 60.;

fn long_answer_around_a_needle() -> String {
    let filler = |n: usize| {
        (0..n)
            .map(|i| format!("Paragraph {i} of the long answer runs on about its subject at length, with enough words to wrap onto several lines in a narrow column.\n\n"))
            .collect::<String>()
    };
    format!(
        "{}The distinctive needle sentence sits in the middle of this answer.\n\n{}",
        filler(40),
        filler(40)
    )
}

/// Page keys reach the transcript when the Composer has nothing to move in.
fn empty_composer(cx: &mut gpui::VisualTestContext) {
    cx.simulate_keystrokes("cmd-a backspace");
}

fn hit_inset(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> f32 {
    let hit = transcript(view, cx)
        .read_with(cx, |transcript, _| transcript.find_state())
        .and_then(|state| state.painted)
        .expect("the hit painted");
    f32::from(hit.top() - scroll(view, cx).bounds().top())
}

/// The sentence Find stands on stays at its inset through every frame of a
/// panel closing and opening and of the window narrowing, widening and
/// narrowing again, the reflow changing the rows' height each time.
#[gpui::test]
fn a_held_sentence_stays_put_through_reflow(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (view, _fake, cx) = answered_with("held-sentence", true, long_answer_around_a_needle(), cx);
    cx.simulate_keystrokes("cmd-f");
    tick(cx);
    cx.simulate_input("needle");
    tick(cx);
    for _ in 0..8 {
        transcript(&view, cx).update(cx, |_, cx| cx.notify());
        at(16, cx);
    }
    let scroll = scroll(&view, cx);
    assert!(
        !scroll.is_following_tail(),
        "the premise: reading mid-answer"
    );
    let inset = hit_inset(&view, cx);
    let mut frames = vec![(scroll.bounds().size.width, inset)];
    let mut settle = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..16 {
            at(16, cx);
            frames.push((scroll.bounds().size.width, hit_inset(&view, cx)));
        }
    };
    cx.simulate_keystrokes("cmd-alt-b");
    settle(cx);
    cx.simulate_keystrokes("cmd-alt-b");
    settle(cx);
    for width in [760., 1300., 900., 1000.] {
        cx.simulate_resize(gpui::size(px(width), px(700.)));
        settle(cx);
    }
    let widths: std::collections::BTreeSet<_> = frames
        .iter()
        .map(|(width, _)| f32::from(*width).round() as i32)
        .collect();
    assert!(
        widths.len() >= 6,
        "the premise: the width really changed: {widths:?}"
    );
    for (width, at) in &frames {
        assert!(
            (at - inset).abs() <= HELD_TOLERANCE,
            "the sentence left its inset at width {width:?}: {at} vs {inset}\n{frames:?}"
        );
    }
}

/// A fully visible row boundary keeps its exact inset through a reflow.
#[gpui::test]
fn the_top_of_the_transcript_stays_the_top_through_reflow(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (view, _fake, cx) = answered("top-reflow", true, cx);
    empty_composer(cx);
    cx.simulate_keystrokes("home");
    at(16, cx);
    let scroll = scroll(&view, cx);
    assert_eq!(scroll.offset().y, px(0.), "the premise: the top");
    for width in [700., 1300., 1000.] {
        cx.simulate_resize(gpui::size(px(width), px(700.)));
        for _ in 0..4 {
            at(16, cx);
            assert_eq!(scroll.offset().y, px(0.), "still the top at {width}");
        }
    }
}

/// One jump to the end from the top of a long answer, every frame recorded
/// as the distance still to go.
fn jump_to_the_end(name: &str, pill: bool, cx: &mut TestAppContext) -> Vec<f32> {
    bind_production_keys(cx);
    let (view, _fake, cx) = answered(name, true, cx);
    send(cx);
    for _ in 0..(MOTION_SCROLL_MAX_MS / 16 + 4) {
        at(16, cx);
    }
    transcript(&view, cx).update(cx, |transcript, cx| transcript.page(Page::Top, cx));
    at(16, cx);
    let scroll = scroll(&view, cx);
    scrolled_away(&view, cx);
    at(SCROLL_END_SHOW_DELAY_MS + 10, cx);
    let to_go = |scroll: &ferrite_pane::transcript::TranscriptScroll| {
        f32::from(scroll.max_offset().y + scroll.offset().y)
    };
    let far = to_go(&scroll);
    assert!(
        far > 4. * f32::from(scroll.bounds().size.height),
        "the premise: several viewports away: {far}"
    );
    if pill {
        let pill = cx.debug_bounds("scroll-to-end").expect("the pill");
        cx.simulate_click(pill.center(), gpui::Modifiers::none());
    } else {
        cx.simulate_keystrokes("end");
    }
    let mut frames = vec![to_go(&scroll)];
    for _ in 0..(MOTION_SCROLL_MAX_MS / 16 + 4) {
        at(16, cx);
        frames.push(to_go(&scroll));
    }
    assert!(scroll.is_following_tail(), "following the tail");
    assert!(
        frames.last().unwrap().abs() <= 0.5,
        "the same final row: {frames:?}"
    );
    frames
}

/// The Scroll to end pill from far up comes within a viewport at once (the
/// rows between are never laid out) and eases the rest: each frame closer,
/// none overshooting, several frames on the way.
#[gpui::test]
fn the_pill_from_far_up_eases_its_last_stretch(cx: &mut TestAppContext) {
    let frames = jump_to_the_end("jump-pill", true, cx);
    let viewport = 571.;
    assert!(
        frames[1] <= viewport + 1.,
        "within a viewport after one frame: {frames:?}"
    );
    let on_the_way = frames
        .iter()
        .filter(|left| **left > 0.5 && **left < viewport)
        .count();
    assert!(on_the_way >= 3, "frames between: {frames:?}");
    assert!(
        frames.windows(2).all(|pair| pair[1] <= pair[0] + 0.5),
        "never backs up or overshoots: {frames:?}"
    );
}

/// End is the same move without the ease: one frame to the same final row.
#[gpui::test]
fn end_from_far_up_lands_at_once(cx: &mut TestAppContext) {
    let frames = jump_to_the_end("jump-end", false, cx);
    assert!(frames[1].abs() <= 0.5, "at once: {frames:?}");
}

/// Numbered lines under a pinned prompt: each Page Down and Page Up leaves
/// the line the page before ended on in the readable view, so no line is
/// hidden by the pinned band on both sides of a page.
#[gpui::test]
fn paging_leaves_no_line_unread(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (view, _fake, cx) = answered("page-lines", true, cx);
    empty_composer(cx);
    cx.simulate_keystrokes("home");
    at(16, cx);
    let scroll = scroll(&view, cx);
    let view_h = scroll.bounds().size.height;
    let line = transcript(&view, cx).read_with(cx, |t, _| t.line_height());
    // The readable window in content coordinates, below the pinned band.
    let readable = |scroll: &ferrite_pane::transcript::TranscriptScroll| {
        let top = f32::from(-scroll.offset().y + scroll.top_mask());
        (top, f32::from(-scroll.offset().y + view_h))
    };
    let mut pages = vec![readable(&scroll)];
    let mut guard = 0;
    while !scroll.is_following_tail() && guard < 40 {
        cx.simulate_keystrokes("pagedown");
        at(16, cx);
        at(16, cx);
        pages.push(readable(&scroll));
        guard += 1;
    }
    assert!(pages.len() > 4, "the premise: several pages: {pages:?}");
    assert!(
        pages.iter().any(|(top, _)| *top > pages[0].0 + 1.),
        "a band pinned: {pages:?}"
    );
    assert!(
        pages.iter().skip(1).any(|_| scroll.top_mask() > px(line)),
        "the pinned band covers more than a line"
    );
    for pair in pages.windows(2) {
        let overlap = pair[0].1 - pair[1].0;
        assert!(
            overlap >= line - 1.,
            "a gap between pages going down: {pair:?}"
        );
    }
    let mut up = vec![readable(&scroll)];
    while f32::from(-scroll.offset().y) > 1. && up.len() < 40 {
        cx.simulate_keystrokes("pageup");
        at(16, cx);
        at(16, cx);
        up.push(readable(&scroll));
    }
    for pair in up.windows(2) {
        let overlap = pair[1].1 - pair[0].0;
        assert!(
            overlap >= line - 1.,
            "a gap between pages going up: {pair:?}"
        );
    }
}

/// A short answer leaves send runway under it. Wheeled up until part of it
/// lies below the view, the real rows below count: the cue shows.
#[gpui::test]
fn a_short_answer_with_runway_still_shows_the_cue(cx: &mut TestAppContext) {
    bind_production_keys(cx);
    let (view, fake, cx) = answered("short-runway", true, cx);
    send(cx);
    for _ in 0..(MOTION_SCROLL_MAX_MS / 16 + 4) {
        at(16, cx);
    }
    let reply = fake.streams.borrow().last().unwrap().clone();
    reply
        .send(SessionEvent::TextDelta {
            text: (0..3).map(|n| format!("Reply line {n}\n\n")).collect(),
        })
        .unwrap();
    reply
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    let room = transcript(&view, cx).read_with(cx, |t, _| t.runway_room());
    assert!(
        room > 20.,
        "the premise: runway still under the reply: {room}"
    );
    let scroll = scroll(&view, cx);
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: scroll.bounds().center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(500.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::default(),
    });
    cx.run_until_parked();
    at(SCROLL_END_SHOW_DELAY_MS + 10, cx);
    assert!(cx.debug_bounds("scroll-to-end").is_some(), "the cue shows");
}
