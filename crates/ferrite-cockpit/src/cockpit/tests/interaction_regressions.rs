use super::*;

fn wheel(cx: &mut gpui::VisualTestContext, position: gpui::Point<Pixels>, dy: f32) {
    cx.simulate_event(gpui::ScrollWheelEvent {
        position,
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::default(),
    });
    cx.run_until_parked();
}

#[gpui::test]
fn wheel_over_a_large_pinned_prompt_scrolls_the_transcript(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("large-pinned-wheel", 1);
    let thread = core.threads()[0];
    core.send(
        thread,
        (0..60).map(|n| format!("Long prompt line {n}\n")).collect(),
    );
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: (0..80).map(|n| format!("Answer line {n}\n\n")).collect(),
        })
        .unwrap();
    tick(cx);
    let scroll = view.read_with(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .clone()
    });
    let before = scroll.offset().y;
    assert!(before < px(-100.), "fixture must overflow");
    assert!(scroll.top_mask() > px(0.), "the prompt is pinned");
    let pointer = gpui::point(scroll.bounds().center().x, scroll.bounds().top() + px(10.));
    wheel(cx, pointer, 120.);
    assert!(
        scroll.offset().y > before,
        "wheel over the pinned prompt must reach the transcript: {:?} -> {:?}",
        before,
        scroll.offset().y
    );
    assert!(
        scroll.top_mask() < scroll.bounds().size.height / 2.,
        "a pinned prompt must leave room to read and scroll the answer"
    );
    wheel(cx, pointer, -100000.);
    assert!(
        scroll.is_following_tail(),
        "scrolling down over the prompt resumes tail following"
    );
}

#[gpui::test]
fn selecting_in_a_long_answer_keeps_the_scroll_position(cx: &mut TestAppContext) {
    selection_keeps_position(cx, "selection-short-prompt", "Explain the implementation");
}

#[gpui::test]
fn selecting_below_a_large_pinned_prompt_keeps_the_scroll_position(cx: &mut TestAppContext) {
    selection_keeps_position(
        cx,
        "selection-long-prompt",
        &"A long prompt with several requirements.\n".repeat(50),
    );
}

fn selection_keeps_position(cx: &mut TestAppContext, name: &str, prompt: &str) {
    let (mut core, fake) = cockpit(name, 1);
    let thread = core.threads()[0];
    core.send(thread, prompt.into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: (0..100)
                .map(|n| format!("Answer paragraph {n:03} has several words.\n\n"))
                .collect(),
        })
        .unwrap();
    tick(cx);
    let scroll = view.read_with(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .clone()
    });
    wheel(cx, scroll.bounds().center(), 220.);
    let before = scroll.offset().y;
    // Aim at a visible paragraph rather than the prompt's pinned copy.
    let row = (1..98)
        .find(|row| {
            let point = caret(&view, cx, *row, 1);
            point.y > scroll.bounds().top() + scroll.top_mask() + px(30.)
                && point.y < scroll.bounds().bottom() - px(100.)
        })
        .expect("a visible answer paragraph");
    let position = caret(&view, cx, row, 1);
    cx.simulate_mouse_down(position, MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(scroll.offset().y, before, "press must preserve scroll");
    let to = position + gpui::point(px(90.), px(35.));
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(scroll.offset().y, before, "drag must preserve scroll");
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(scroll.offset().y, before, "release must preserve scroll");
    cx.update(|window, cx| {
        assert!(
            gpui::base::TextSelection::has_selection(window, cx),
            "the drag selected answer text"
        )
    });
    press(cx, position, 2);
    cx.simulate_mouse_up(position, MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        scroll.offset().y,
        before,
        "double-click must preserve scroll"
    );
}
