//! Stabilization tests: behaviour and state gaps closed after the work
//! packages merged.
#[allow(unused_imports)]
use super::*;

/// Reduced motion holds the working line's Ferrite mark still, at L1 and in
/// an L2 cell alike; otherwise its shards snap on their timeline.
#[gpui::test]
fn the_working_mark_rests_under_reduced_motion_at_l1_and_l2(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("working-mark-motion", 1);
    let thread = core.threads()[0];
    core.send(thread, "Inspect progress".into());
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::ReasoningSummaryDelta {
            text: "**Checking marks**".into(),
            summary_index: 0,
        })
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("progress-mark-live").is_some());
    assert!(cx.debug_bounds("progress-mark-still").is_none());

    cx.update(|_, cx| cx.set_reduce_motion(true));
    tick(cx);
    assert!(cx.debug_bounds("progress-mark-still").is_some(), "L1");
    assert!(cx.debug_bounds("progress-mark-live").is_none(), "L1");

    cx.simulate_resize(gpui::size(px(560.), px(700.)));
    tick(cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Instruments,
        "the premise: the cell is at L2"
    );
    assert!(cx.debug_bounds("progress-mark-still").is_some(), "L2");
    assert!(cx.debug_bounds("progress-mark-live").is_none(), "L2");
}

/// While a Pane is dragged by its title its own cell dims in its slot, and
/// the release — on a Pane or anywhere else — brings it back.
#[gpui::test]
fn a_dragged_pane_dims_its_source_until_the_release(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("drag-source-dims", 2);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    let (a, b) = (threads[0], threads[1]);
    let source = format!("pane-drag-source-{}", a.get());
    let source: &'static str = Box::leak(source.into_boxed_str());
    let title: &'static str = Box::leak(format!("pane-title-{}", a.get()).into_boxed_str());
    let target = cx.update(|window, cx| {
        let view = view.read(cx);
        let index = view.pane_for(b).unwrap();
        view.pane_rects(window)
            .into_iter()
            .find(|(i, _)| *i == index)
            .map(|(_, r)| gpui::point(px(r.x + r.w / 2.0), px(r.y + r.h / 2.0)))
            .unwrap()
    });
    let outside = gpui::point(px(5.), px(890.));

    for release in [target, outside] {
        assert!(
            cx.debug_bounds(source).is_none(),
            "at rest nothing is dimmed"
        );
        let grab = cx.debug_bounds(title).expect("the title is drawn").center();
        cx.simulate_mouse_down(grab, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        cx.simulate_mouse_move(
            grab + gpui::point(px(12.), px(12.)),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        cx.run_until_parked();
        cx.simulate_mouse_move(release, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        view.read_with(cx, |view, _| assert_eq!(view.pane_drag_source, Some(a)));
        assert!(
            cx.debug_bounds(source).is_some(),
            "the source cell dims while the drag is live"
        );
        cx.simulate_mouse_up(release, gpui::MouseButton::Left, gpui::Modifiers::none());
        cx.run_until_parked();
        tick(cx);
        view.read_with(cx, |view, _| assert_eq!(view.pane_drag_source, None));
        assert!(cx.debug_bounds(source).is_none(), "the release restores it");
    }
}

/// Native files dragged over a Pane lay the drop sheet over that Pane and
/// edge its Composer in the accent; the sheet follows the pointer to the
/// next Pane, and leaving the window or dropping clears both.
#[gpui::test]
fn files_over_a_pane_edge_its_composer_in_the_accent(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("drop-composer-edge", 2);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    let rects: Vec<_> = cx.update(|window, cx| {
        view.read(cx)
            .pane_rects(window)
            .into_iter()
            .map(|(_, r)| r)
            .collect()
    });
    let inside = |r: &layout::Rect, at: gpui::Point<Pixels>| {
        at.x >= px(r.x) && at.x <= px(r.x + r.w) && at.y >= px(r.y) && at.y <= px(r.y + r.h)
    };
    let centre = |r: &layout::Rect| gpui::point(px(r.x + r.w / 2.), px(r.y + r.h / 2.));
    let paths = gpui::ExternalPaths(vec![here().join("notes.txt")].into());
    assert!(cx.debug_bounds("prompt-drop-sheet").is_none());
    assert!(cx.debug_bounds("composer-drop-target").is_none());

    cx.simulate_event(gpui::FileDropEvent::Entered {
        position: centre(&rects[0]),
        paths: paths.clone(),
    });
    tick(cx);
    for rect in [&rects[0], &rects[1]] {
        if rect != &rects[0] {
            cx.simulate_event(gpui::FileDropEvent::Pending {
                position: centre(rect),
            });
            tick(cx);
        }
        let sheet = cx.debug_bounds("prompt-drop-sheet").expect("the sheet");
        let edged = cx
            .debug_bounds("composer-drop-target")
            .expect("the Composer wears the accent edge");
        assert!(inside(rect, sheet.center()), "{sheet:?} over {rect:?}");
        assert!(inside(rect, edged.center()), "{edged:?} in {rect:?}");
    }

    cx.simulate_event(gpui::FileDropEvent::Exited);
    tick(cx);
    assert!(cx.debug_bounds("prompt-drop-sheet").is_none(), "left");
    assert!(cx.debug_bounds("composer-drop-target").is_none(), "left");

    cx.simulate_event(gpui::FileDropEvent::Entered {
        position: centre(&rects[1]),
        paths,
    });
    tick(cx);
    assert!(cx.debug_bounds("composer-drop-target").is_some());
    cx.simulate_event(gpui::FileDropEvent::Submit {
        position: centre(&rects[1]),
    });
    tick(cx);
    assert!(cx.debug_bounds("prompt-drop-sheet").is_none(), "dropped");
    assert!(cx.debug_bounds("composer-drop-target").is_none(), "dropped");
}

/// The command key is drawn, never typed: the empty board's keycaps and the
/// context menu's shortcuts both place the one `⌘` glyph on macOS.
#[gpui::test]
fn keycaps_and_menu_shortcuts_draw_the_command_glyph(cx: &mut TestAppContext) {
    if crate::keymap::PLATFORM != crate::keymap::Platform::Mac {
        return;
    }
    let (core, _fake) = cockpit("command-glyph", 1);
    let thread = core.threads()[0];
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-w", CloseThread, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    assert!(
        cx.debug_bounds("command-key").is_none(),
        "nothing shows keys"
    );

    view.update(cx, |view, cx| {
        view.open_context_menu(
            MenuTarget::Pane(thread),
            gpui::point(px(400.), px(300.)),
            cx,
        )
    });
    tick(cx);
    assert!(
        cx.debug_bounds("command-key").is_some(),
        "the Pane menu's shortcuts draw the glyph"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);

    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    assert!(cx.debug_bounds("empty-board").is_some());
    assert!(
        cx.debug_bounds("command-key").is_some(),
        "the empty board's keycaps draw the glyph"
    );
}
