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

/// The Subject strip is Ferrite's own tab row: exactly one active pill,
/// which follows the pick, on a row no taller than a chip (no kit rule
/// hanging under it).
#[gpui::test]
fn the_subject_strip_marks_one_active_pill_that_follows_the_pick(cx: &mut TestAppContext) {
    use ferrite_core::activity::{
        ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject, TranscriptCoverage,
    };
    let (core, fake) = cockpit("subject-pill", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    let key = AgentKey::new(Provider::Claude, "ui-fixture", "Atlas");
    let mut info = AgentInfo::new(key.clone());
    info.name = Some("Atlas".into());
    info.parent = Some(Subject::Main);
    info.coverage = TranscriptCoverage::Live;
    for event in [
        ActivityEvent::Discovered(info),
        ActivityEvent::Status {
            key: key.clone(),
            state: AgentStatus::Idle,
        },
    ] {
        fake.streams.borrow()[0]
            .send(SessionEvent::Activity(event))
            .unwrap();
    }
    tick(cx);
    let strip = cx.debug_bounds("subject-strip-1").expect("the strip");
    let pill = cx.debug_bounds("subject-tab-selected").expect("one pill");
    let main = cx.debug_bounds("subject-main-1").unwrap();
    assert_eq!(pill, main, "Main is active first");
    assert_eq!(pill.size.height, px(crate::theme::CHIP_H));
    assert!(strip.size.height <= px(crate::theme::SUBJECT_STRIP_H));

    let atlas: &'static str =
        Box::leak(format!("subject-agent-1-{}", key.as_str()).into_boxed_str());
    let tab = cx.debug_bounds(atlas).expect("Atlas's tab");
    cx.simulate_click(tab.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    tick(cx);
    assert_eq!(
        cx.debug_bounds("subject-tab-selected").unwrap(),
        cx.debug_bounds(atlas).unwrap(),
        "the pill follows the pick"
    );
}

/// A long popover list scrolls with the keyboard: the draft's Project
/// chip over two dozen Projects keeps every row the arrows land on inside
/// its capped list, going down past the fold and back up.
#[gpui::test]
fn the_popover_keeps_its_keyboard_cursor_in_view(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("popover-cursor-in-view", 1);
    let base = scratch("popover-cursor-in-view-projects");
    for n in 0..24 {
        let dir = base.join(format!("project-{n:02}"));
        std::fs::create_dir_all(&dir).unwrap();
        core.register_project(&dir).unwrap();
    }
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(900.)));
    view.update(cx, |view, cx| {
        view.open_draft(DraftTarget::Main, cx);
        view.open_band_popover(pane::BandChip::Project, cx);
    });
    tick(cx);
    let rows = view.read_with(cx, |view, _| view.popover.as_ref().unwrap().rows.len());
    assert!(
        rows as f32 * crate::theme::MENU_ROW_H > crate::theme::MENU_MAX_H,
        "the premise: {rows} rows overflow the list"
    );
    let visible = |cx: &mut gpui::VisualTestContext| {
        let selected = view.read_with(cx, |view, _| view.popover.as_ref().unwrap().selected);
        let list = cx.debug_bounds("composer-menu-rows").expect("the list");
        let selector: &'static str =
            Box::leak(format!("composer-menu-row-{selected}").into_boxed_str());
        let row = cx.debug_bounds(selector).expect("the cursor row");
        assert!(
            row.top() >= list.top() - px(0.5) && row.bottom() <= list.bottom() + px(0.5),
            "row {selected} {row:?} is outside the list {list:?}"
        );
        selected
    };
    let first = visible(cx);
    for _ in 0..rows {
        view.update(cx, |view, cx| view.step_popover(1, cx));
        tick(cx);
        visible(cx);
    }
    assert!(visible(cx) + 2 >= rows, "the cursor reached the end");
    for _ in 0..rows {
        view.update(cx, |view, cx| view.step_popover(-1, cx));
        tick(cx);
        visible(cx);
    }
    assert!(visible(cx) <= first.max(1));
    let _ = std::fs::remove_dir_all(&base);
}

/// The Project card's name field wears the focus ink while the keyboard is
/// in it, like every other field, and its resting edge otherwise.
#[gpui::test]
fn the_project_name_field_shows_keyboard_focus(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("project-name-focus", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    view.update(cx, |view, cx| view.open_project_creator(cx));
    tick(cx);
    let name = view.read_with(cx, |view, _| {
        view.project_editor.as_ref().unwrap().name.clone()
    });
    cx.update(|window, cx| {
        let handle = name.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    });
    tick(cx);
    assert!(
        cx.debug_bounds("project-name-focused").is_some(),
        "focused: the edge is the focus ink"
    );
    let transcript = view.read_with(cx, |view, _| view.panes[0].transcript_focus.clone());
    cx.update(|window, cx| window.focus(&transcript, cx));
    tick(cx);
    assert!(
        cx.debug_bounds("project-name-focused").is_none(),
        "elsewhere: the resting edge"
    );
}
