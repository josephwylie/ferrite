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

/// The watchdog sweeps on the executor's clock, not the wall clock: a slow
/// or loaded machine that takes longer than `SWEEP_INTERVAL` of real time
/// over a test must not drop a sweep (and its repaint and git refresh) into
/// the middle of it. Only the executor's time moving past the interval does.
#[gpui::test]
fn the_watchdog_sweeps_on_the_executor_clock(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("sweep-clock", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let before = view.read_with(cx, |view, _| view.swept);
    std::thread::sleep(SWEEP_INTERVAL + Duration::from_millis(50));
    view.update(cx, |view, cx| view.pump(cx));
    assert_eq!(
        view.read_with(cx, |view, _| view.swept),
        before,
        "real time alone never sweeps"
    );
    cx.executor().advance_clock(SWEEP_INTERVAL);
    view.update(cx, |view, cx| view.pump(cx));
    assert!(
        view.read_with(cx, |view, _| view.swept) > before,
        "the executor's time does"
    );
}

/// While toasts stand at the foot of the nav, the nav gives their ground
/// up: the Parked fold sits above the whole stack, never under it, and gets
/// its place back once the toasts are gone.
#[gpui::test]
fn the_parked_fold_stays_clear_of_the_toast_stack(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("toast-parked", 5);
    let threads = core.threads();
    core.park(threads[4]).unwrap();
    for (n, thread) in threads[..4].iter().enumerate() {
        core.send(*thread, format!("task {n}"));
    }
    core.pump();
    core.focus_thread(threads[0]);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let resting = cx.debug_bounds("nav-parked").expect("the Parked fold");
    assert!(cx.debug_bounds("nav-toast-reserve").is_none());

    for stream in &fake.streams.borrow()[1..4] {
        stream
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
    }
    tick(cx);
    // The kit's entrance slide runs on the wall clock and its stack springs
    // on the executor's: let both settle before reading where toasts are.
    std::thread::sleep(Duration::from_millis(600));
    for _ in 0..8 {
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    let layers = view.read_with(cx, |view, _| view.toasts);
    assert!(layers >= 2, "the premise: a stack of toasts ({layers})");
    let parked = cx.debug_bounds("nav-parked").expect("the Parked fold");
    let window_h = cx.update(|window, _| window.viewport_size().height);
    let stack_top = window_h - px(crate::theme::toast_reserve(layers) - crate::theme::SPACE_2);
    assert!(
        parked.bottom() <= stack_top,
        "the fold {parked:?} ends above the stack's top {stack_top:?}"
    );
    for thread in &threads[1..4] {
        if let Some(toast) = cx.debug_bounds(Box::leak(
            format!("toast-{}", thread.get()).into_boxed_str(),
        )) {
            assert!(
                parked.bottom() <= toast.top() - px(crate::theme::SPACE_3),
                "the fold {parked:?} clears the toast {toast:?}"
            );
        }
    }
    // Collapsed, only the front toast shows; the rest are a `+N` bubble on
    // its top-right corner.
    let more = cx.debug_bounds("toast-more").expect("the +N bubble");
    let corner = gpui::point(
        px(crate::theme::SPACE_2 + crate::theme::TOAST_W),
        window_h - px(crate::theme::GRID_PAD + crate::theme::TOAST_H),
    );
    assert!(
        (more.center().x - corner.x).abs() <= px(1.)
            && (more.center().y - corner.y).abs() <= px(1.),
        "the bubble straddles the front toast's corner: {more:?} / {corner:?}"
    );
    assert!(
        parked.top() < resting.top(),
        "the fold moved up to make room"
    );

    cx.update(|window, cx| {
        use gpui::component::WindowExt as _;
        window.clear_notifications(cx)
    });
    cx.executor().advance_clock(Duration::from_secs(1));
    tick(cx);
    assert_eq!(view.read_with(cx, |view, _| view.toasts), 0);
    assert_eq!(
        cx.debug_bounds("nav-parked"),
        Some(resting),
        "room given back"
    );
}

/// A Pane head's title keeps its floor beside a full agent strip: at a
/// width where everything cannot fit, the title still shows at least
/// `HEAD_TITLE_MIN_W` of itself while the checkout gives way and the tabs
/// fold into `+N`; with room, it shows whole (up to its cap).
#[gpui::test]
fn the_head_title_keeps_its_floor_beside_the_agent_tabs(cx: &mut TestAppContext) {
    use ferrite_core::activity::{
        ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject, TranscriptCoverage,
    };
    let (mut core, fake) = cockpit("head-title-floor", 1);
    let thread = core.threads()[0];
    core.rename_thread(thread, "Audit every surface of the overhaul")
        .unwrap();
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    for name in [
        // Short names: on a wide Pane the head spans the reading column
        // (720), and the title at its cap, the checkout and all four tabs
        // must fit there.
        "nav", "cmp", "set", "tok",
    ] {
        let key = AgentKey::new(Provider::Claude, "ui-fixture", name);
        let mut info = AgentInfo::new(key.clone());
        info.name = Some(name.into());
        info.parent = Some(Subject::Main);
        info.coverage = TranscriptCoverage::Live;
        for event in [
            ActivityEvent::Discovered(info),
            ActivityEvent::Status {
                key,
                state: AgentStatus::Idle,
            },
        ] {
            fake.streams.borrow()[0]
                .send(SessionEvent::Activity(event))
                .unwrap();
        }
    }
    let title = "pane-head-title-1";
    for (width, whole) in [(900., false), (1800., true)] {
        cx.simulate_resize(gpui::size(px(width), px(800.)));
        tick(cx);
        tick(cx);
        let bounds = cx.debug_bounds(title).expect("the head title");
        assert!(
            bounds.size.width >= px(crate::theme::HEAD_TITLE_MIN_W),
            "{width}: the title keeps its floor: {bounds:?}"
        );
        if whole {
            assert_eq!(
                bounds.size.width,
                px(crate::theme::HEAD_TITLE_MAX_W),
                "with room the long title takes its cap"
            );
        }
    }
    assert!(
        cx.debug_bounds("subject-overflow-1").is_none(),
        "the wide head fits every tab"
    );
}

/// An L2 cell's head says where the work is only when the instrument row
/// cannot: a Main checkout adds no right meta (the row already reads
/// `model · branch`).
#[gpui::test]
fn an_l2_head_does_not_repeat_the_branch(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("l2-no-branch-twice", 1);
    let thread = core.threads()[0];
    core.rename_thread(thread, "Board recipes and the long tail")
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(560.), px(700.)));
    tick(cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Instruments
    );
    assert!(cx.debug_bounds("l2-title-1").is_some());
    assert!(
        cx.debug_bounds("l2-binding-1").is_none(),
        "a Main binding leaves the head's right slot empty"
    );
    // A question too big for the cell puts its expander in that slot; the
    // one-word chip never pushes the title under its floor.
    fake.streams.borrow()[0]
        .send(question("l2-head-question"))
        .unwrap();
    tick(cx);
    let expand = cx.debug_bounds("question-expand").expect("the expander");
    let title = cx.debug_bounds("l2-title-1").unwrap();
    assert!(
        title.size.width >= px(crate::theme::HEAD_TITLE_MIN_W),
        "the title keeps its floor: {title:?}"
    );
    assert!(title.right() <= expand.left(), "{title:?} / {expand:?}");
}

/// An L2 tail's tool row reads as L1 spells it, `● Name(args)`, and a long
/// unbroken argument is cut with an ellipsis inside the cell rather than
/// running out through its edge.
#[gpui::test]
fn l2_tail_tool_rows_stay_inside_the_cell(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("l2-tail-inside", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(560.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "long".into(),
            name: "Bash".into(),
            input: serde_json::json!({
                "command": "cargo check --workspace --all-targets --features visual-reference,test-support --message-format=short"
            }),
        })
        .unwrap();
    tick(cx);
    let (namespace, id, rect) = cx.update(|window, cx| {
        let view = view.read(cx);
        let pane = &view.panes[0];
        let thread = view.cockpit.thread(pane.thread().unwrap()).unwrap();
        let id = thread.transcript().blocks().last().unwrap().id;
        (pane.text_namespace(), id, view.pane_rects(window)[0].1)
    });
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Instruments
    );
    let row = cx
        .debug_bounds(Box::leak(
            format!("l2-tail-row-{namespace}-{id:?}").into_boxed_str(),
        ))
        .expect("the tool row");
    assert!(
        row.right() <= px(rect.x + rect.w - crate::theme::CELL_PAD) + px(0.5),
        "the row {row:?} ends inside the cell {rect:?}"
    );
    assert_eq!(row.size.height, px(crate::theme::LH_META), "one line");
}

/// A draft in a narrow Pane keeps its whole controls row inside the
/// Composer: the Project and branch chips truncate first, and the model
/// and effort pair stays whole and in the block.
#[gpui::test]
fn a_narrow_draft_keeps_its_controls_inside_the_composer(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("narrow-draft-controls", 1);
    let dir = scratch("narrow-draft-controls-a-rather-long-project-directory-name");
    std::fs::create_dir_all(&dir).unwrap();
    core.register_project(&dir).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(720.), px(900.)));
    view.update(cx, |view, cx| view.open_draft(DraftTarget::Main, cx));
    tick(cx);
    let block = cx
        .debug_bounds("composer-block")
        .expect("the draft Composer");
    let effort = cx.debug_bounds("draft-effort-picker").expect("effort");
    let model = cx.debug_bounds("draft-model-picker").expect("model");
    assert!(
        effort.right()
            <= block.right() - px(crate::theme::COMPOSER_CONTROL_INSET + crate::theme::SEND_BUTTON)
                + px(0.5),
        "effort {effort:?} stays inside the block {block:?}"
    );
    assert!(model.right() <= effort.left());
    let band = cx.debug_bounds("draft-band").expect("the setup chips");
    assert!(
        band.top() >= block.bottom(),
        "the setup chips ride the meta row under the box: {band:?} / {block:?}"
    );
    // The meter gives way first, so the setup chips keep their names.
    assert!(cx.debug_bounds("usage-meter-draft-1").is_none());
    for chip in ["band-chip-0", "band-chip-1"] {
        let chip = cx.debug_bounds(chip).expect("a setup chip");
        assert!(
            chip.size.width >= px(48.),
            "{chip:?} keeps a readable label"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A Subagent's read-only footer is the Composer's own block: in the
/// reading column, inset from the Pane's foot, its `❯` on the Composer's
/// axis and its text at C1.
#[gpui::test]
fn the_subagent_footer_sits_in_the_composer_block(cx: &mut TestAppContext) {
    use ferrite_core::activity::{
        ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject, TranscriptCoverage,
    };
    let (core, fake) = cockpit("child-footer-block", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
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
    let main_block = cx.debug_bounds("composer-block").expect("Main's Composer");
    let tab: &'static str = Box::leak(format!("subject-agent-1-{}", key.as_str()).into_boxed_str());
    let at = cx.debug_bounds(tab).expect("Atlas's tab").center();
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
    tick(cx);
    let footer = cx
        .debug_bounds("child-footer-1")
        .expect("the read-only footer");
    let text = cx.debug_bounds("child-footer-text-1").unwrap();
    let pane = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1);
    assert_eq!(footer.left(), main_block.left(), "the Composer's column");
    assert_eq!(footer.size.width, main_block.size.width);
    assert!(
        (px(pane.y + pane.h) - footer.bottom() - px(crate::theme::COMPOSER_INSET_B)).abs()
            <= px(1.5),
        "inset from the Pane's foot: {footer:?} / {pane:?}"
    );
    assert_eq!(
        text.left() - footer.left(),
        px(crate::theme::BOX_INSET_X + crate::theme::GUTTER_W),
        "the text starts at C1"
    );
}

/// An L2 cell with an approval keeps its Composer under the card: `y`
/// still answers from the card, and a press in the Composer takes typing.
#[gpui::test]
fn an_l2_approval_cell_keeps_its_composer(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("l2-approval-composer", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(560.), px(700.)));
    tick(cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Instruments
    );
    fake.streams.borrow()[0].send(decision("l2-keep")).unwrap();
    tick(cx);
    let block = cx
        .debug_bounds("composer-block")
        .expect("the approval cell keeps its Composer");
    let pane = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1);
    assert!(
        block.bottom() <= px(pane.y + pane.h),
        "{block:?} / {pane:?}"
    );

    // The card answers from the keyboard.
    cx.simulate_keystrokes("y");
    tick(cx);
    assert!(
        matches!(
            fake.answered.borrow().last(),
            Some((id, DecisionAnswer::Allow { .. })) if id == "l2-keep"
        ),
        "y answers: {:?}",
        fake.answered.borrow()
    );

    // A second request, then a press in the Composer: typing lands there.
    fake.streams.borrow()[0].send(decision("l2-type")).unwrap();
    tick(cx);
    let block = cx.debug_bounds("composer-block").unwrap();
    cx.simulate_click(block.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("hold on");
    tick(cx);
    assert_eq!(composer_text(&view, cx), "hold on");
}

/// A question's own-answer field is a full control: `CONTROL_H` tall, its
/// edge and padding hanging left of the option labels' column so its text
/// starts where they do.
#[gpui::test]
fn the_own_answer_field_is_a_full_control_on_the_label_column(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("own-answer-field", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(900.)));
    fake.streams.borrow()[0].send(question("own")).unwrap();
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let serial = view.read_with(cx, |view, _| {
        view.cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .pending_decisions()[0]
            .handle
            .serial
    });
    let field = cx
        .debug_bounds(Box::leak(
            format!("request-other-{}-{serial}-0", thread.get()).into_boxed_str(),
        ))
        .expect("the own-answer field");
    let choice = cx.debug_bounds("question-choice-0-0").expect("an option");
    assert_eq!(field.size.height, px(crate::theme::CONTROL_H));
    let labels = choice.left()
        + px(crate::theme::DECISION_ROW_PAD_X
            + crate::theme::KBD_H
            + crate::theme::DECISION_ROW_INNER_GAP);
    assert_eq!(
        field.left() + px(crate::theme::QUESTION_FIELD_PAD_X + 1.),
        labels,
        "the field's text starts on the labels' column"
    );
}

/// The image preview's scrim covers the whole window, nav and titlebar
/// included, like every modal's; its sheet stays centred on its Pane.
#[gpui::test]
fn the_image_preview_dims_the_whole_window(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("preview-scrim", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let image = scratch("preview-scrim-image").join("shot.png");
    std::fs::create_dir_all(image.parent().unwrap()).unwrap();
    std::fs::write(&image, include_bytes!("../../../assets/app-icon.png")).unwrap();
    view.update_in(cx, |view, window, cx| {
        view.panes[0]
            .preview
            .open(image.clone(), "Screenshot".into(), window, cx);
    });
    tick(cx);
    tick(cx);
    let scrim = cx
        .debug_bounds("attachment-preview-scrim")
        .expect("a scrim");
    let window = cx.update(|window, _| window.viewport_size());
    assert_eq!(scrim.origin, gpui::point(px(0.), px(0.)));
    assert_eq!(scrim.size, window, "the scrim covers the window");
    let sheet = cx.debug_bounds("attachment-preview-content").unwrap();
    let pane = cx.update(|window, cx| view.read(cx).pane_rects(window)[0].1);
    assert!((sheet.center().x - px(pane.x + pane.w / 2.)).abs() <= px(2.));
    let _ = std::fs::remove_dir_all(image.parent().unwrap());
}

/// A draft's body says what to do, as an empty Thread's does.
#[gpui::test]
fn a_draft_body_says_how_to_start(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("draft-empty", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    view.update(cx, |view, cx| view.open_draft(DraftTarget::Main, cx));
    tick(cx);
    let empty = cx
        .debug_bounds("draft-empty")
        .expect("the draft's guidance");
    let block = cx.debug_bounds("composer-block").unwrap();
    assert!(empty.bottom() <= block.top(), "{empty:?} / {block:?}");
}

/// On the empty board the titlebar has no location, and the `dev` tag
/// takes the location's own inset rather than trailing an empty slot.
#[gpui::test]
fn the_empty_board_titlebar_keeps_the_dev_tag_on_the_inset(cx: &mut TestAppContext) {
    if !crate::titlebar::DEV {
        return;
    }
    let (core, _fake) = cockpit("empty-titlebar", 1);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-w", CloseThread, None)]));
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    assert!(cx.debug_bounds("empty-board").is_some());
    let tag = cx.debug_bounds("titlebar-dev-badge").expect("the dev tag");
    assert_eq!(
        tag.left(),
        px(crate::theme::NAV_WIDTH + crate::theme::GRID_PAD),
        "{tag:?}"
    );
}

/// The checks card grows to its own tally: the counts line is never cut
/// while the card is under its cap.
#[gpui::test]
fn the_checks_card_grows_to_its_tally(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("checks-card-width", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    let status = branch_status_with_checks();
    let tally = status.pr.as_ref().unwrap().tally();
    let text = [
        (tally.failing, "failed"),
        (tally.pending, "running"),
        (tally.passing, "passed"),
        (tally.skipped, "skipped"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, word)| format!("{count} {word}"))
    .collect::<Vec<_>>()
    .join(" · ");
    view.update(cx, |view, cx| {
        view.facts.set_branches(vec![(thread, Some(status))]);
        cx.notify();
    });
    tick(cx);
    let mark = cx.debug_bounds("ci-mark-1").expect("the ci mark");
    cx.simulate_mouse_down(mark.center(), MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    let card = cx.debug_bounds("context-checks-card").expect("the card");
    assert!(card.size.width >= px(crate::theme::CHECKS_CARD_W));
    assert!(card.size.width <= px(crate::theme::CHECKS_CARD_MAX_W));
    let tally = cx.debug_bounds("checks-tally").unwrap();
    let natural = cx.update(|window, _| {
        window
            .text_system()
            .shape_line(
                text.clone().into(),
                px(crate::theme::FS_SM),
                &[gpui::TextRun {
                    len: text.len(),
                    font: gpui::font(crate::theme::FONT_UI),
                    color: gpui::rgb(crate::theme::TEXT).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            )
            .width
    });
    assert!(
        tally.size.width + px(0.5) >= natural,
        "the tally `{text}` is whole: {tally:?} for {natural:?}"
    );
}

/// A Project with Threads still offers "Remove Project", disabled, with the
/// reason in its tooltip; pressing it removes nothing.
#[gpui::test]
fn a_project_in_use_keeps_its_remove_verb_disabled(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("remove-project-in-use", 1);
    let thread = core.threads()[0];
    let project = core.project_id(thread).expect("the Thread's Project");
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    view.update(cx, |view, cx| view.open_project_editor(project, cx));
    tick(cx);
    let remove = cx.debug_bounds("remove-project").expect("the remove verb");
    cx.simulate_click(remove.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.cockpit.registry().project(project).is_some());
        assert!(view.project_editor.is_some(), "the sheet stays up");
    });
}

/// With an approval pending in an L2 cell, `y` allows whether the keyboard
/// is in the Composer or on the card — the L1 rule. The cockpit's focus
/// rule puts the keyboard in the Composer on every frame, so the card case
/// focuses the card and presses `y` in the same turn, before any frame can
/// move it.
#[gpui::test]
fn an_l2_approval_allows_on_y_with_the_card_or_the_composer_focused(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("l2-approval-focus", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(560.), px(700.)));
    tick(cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Instruments
    );
    for (n, holder) in ["composer", "card"].into_iter().enumerate() {
        let id = format!("l2-focus-{n}");
        fake.streams.borrow()[0].send(decision(&id)).unwrap();
        tick(cx);
        assert!(
            cx.debug_bounds("composer-block").is_some(),
            "the cell keeps its Composer"
        );
        let on_card = cx.update(|window, cx| {
            let card = view.read(cx).panes[0].decision_focus.clone();
            if holder == "card" {
                window.focus(&card, cx);
            }
            let on_card = card.is_focused(window);
            window.dispatch_keystroke(gpui::Keystroke::parse("y").unwrap(), cx);
            on_card
        });
        assert_eq!(on_card, holder == "card", "the premise: {holder} holds it");
        tick(cx);
        assert!(
            matches!(
                fake.answered.borrow().last(),
                Some((answered, DecisionAnswer::Allow { .. })) if *answered == id
            ),
            "y allows with the {holder} holding the keyboard: {:?}",
            fake.answered.borrow()
        );
    }
}

/// Theme rule 6, sampled where each face is set: the app's own copy is in
/// the UI face (Geist), and only code and machine text is in the code face
/// (Geist Mono).
#[gpui::test]
fn the_ui_and_code_faces_follow_what_the_text_is(cx: &mut TestAppContext) {
    use crate::theme::{FONT_CODE, FONT_UI};
    use gpui::Styled as _;
    let (core, fake) = cockpit("font-roles", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(1000.)));
    tick(cx);
    cx.simulate_input("Check the fixture");
    cx.simulate_keystrokes("enter");
    tick(cx);
    let stream = fake.streams.borrow()[0].clone();
    for (id, name, input) in [
        (
            "run",
            "Bash",
            serde_json::json!({ "command": "cargo test -p ferrite" }),
        ),
        (
            "read",
            "Read",
            serde_json::json!({ "file_path": "src/lib.rs" }),
        ),
    ] {
        stream
            .send(SessionEvent::ToolStarted {
                id: id.into(),
                name: name.into(),
                input,
            })
            .unwrap();
        stream
            .send(SessionEvent::ToolCompleted {
                id: id.into(),
                output: "ok".into(),
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            })
            .unwrap();
    }
    stream
        .send(SessionEvent::TextDelta {
            text: "Use `cargo test` here.\n\n```rust\nlet x = 1;\n```\n\n".into(),
        })
        .unwrap();
    stream
        .send(SessionEvent::ToolStarted {
            id: "edit".into(),
            name: "Edit".into(),
            input: serde_json::json!({ "file_path": "/workspace/x.txt" }),
        })
        .unwrap();
    stream
        .send(SessionEvent::ToolCompleted {
            id: "edit".into(),
            output: "applied".into(),
            is_error: false,
            result: ferrite_core::ToolResult::FileEdit {
                path: "/workspace/x.txt".into(),
                hunks: vec![ferrite_core::Hunk {
                    old_start: 1,
                    old_lines: 2,
                    new_start: 1,
                    new_lines: 2,
                    lines: vec![" alpha".into(), "-bravo".into(), "+delta".into()],
                }],
            },
        })
        .unwrap();
    stream
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    drop(stream);
    tick(cx);
    tick(cx);

    let (namespace, blocks, composer) = view.read_with(cx, |view, _| {
        let pane = &view.panes[0];
        let thread = view.cockpit.thread(pane.thread().unwrap()).unwrap();
        let blocks: Vec<_> = thread
            .transcript()
            .blocks()
            .iter()
            .map(|block| (block.id, block.body.clone()))
            .collect();
        (pane.text_namespace(), blocks, pane.composer.entity_id())
    });
    let id_of = |pick: &dyn Fn(&Body) -> bool| {
        blocks
            .iter()
            .find(|(_, body)| pick(body))
            .map(|(id, _)| *id)
            .expect("the fixture's block")
    };
    let face = |cx: &mut gpui::VisualTestContext, prefix: String| {
        cx.update(|_, cx| crate::rich::testing::font_family(&prefix, cx))
            .unwrap_or_else(|| panic!("{prefix} was drawn"))
    };
    let prompt = id_of(&|body| matches!(body, Body::Prompt(_)));
    let group = id_of(&|body| matches!(body, Body::Tool(tool) if tool.name == "Bash"));
    let edit = id_of(&|body| matches!(body, Body::Tool(tool) if tool.name == "Edit"));
    let stamp = id_of(&|body| matches!(body, Body::TurnEnd(_)));

    // UI: the prompt echo, a group summary, a tool call's line (its name;
    // the arguments carry the code mark, `call_highlights`), the stamp.
    for (what, prefix) in [
        ("prompt", format!("literal-{namespace}-{prompt:?}-0")),
        ("group summary", format!("literal-{namespace}-{group:?}-0")),
        ("call line", format!("literal-{namespace}-{edit:?}-0")),
        ("stamp", format!("literal-{namespace}-{stamp:?}-0")),
    ] {
        assert_eq!(face(cx, prefix).as_ref(), FONT_UI, "{what} is UI text");
    }
    // Code: the diff's lines and the Composer's line.
    // A settled edit's diff shows once it is disclosed.
    for disclosure in [
        pane::DisclosureId::Group("edit".into()),
        pane::DisclosureId::Tool("edit".into()),
    ] {
        if let Some(control) =
            view.read_with(cx, |view, _| view.panes[0].tool_bounds(disclosure.clone()))
        {
            cx.simulate_click(control.center(), gpui::Modifiers::none());
            tick(cx);
        }
    }
    let diff_faces: Vec<_> = (0..10)
        .filter_map(|ordinal| {
            let prefix = format!("literal-{namespace}-{edit:?}-{ordinal}");
            cx.update(|_, cx| {
                let text = crate::rich::testing::full_text(&prefix, cx)?;
                ["alpha", "bravo", "delta"]
                    .iter()
                    .any(|line| text.trim() == *line)
                    .then(|| crate::rich::testing::font_family(&prefix, cx))
                    .flatten()
            })
        })
        .collect();
    assert!(!diff_faces.is_empty(), "the diff is drawn");
    assert!(
        diff_faces.iter().all(|face| face.as_ref() == FONT_CODE),
        "every diff line is code: {diff_faces:?}"
    );
    assert_eq!(
        cx.update(|_, cx| crate::composer::testing::face(composer, cx))
            .as_deref(),
        Some(FONT_CODE),
        "the Composer's line is code"
    );
    // Code in prose: fenced blocks and inline spans.
    let style = crate::rich::style(px(16.));
    assert_eq!(
        style.code_block().text.font_family.as_deref(),
        Some(FONT_CODE)
    );
    assert_eq!(style.inline_code_font().as_deref(), Some(FONT_CODE));

    // The surfaces chrome inherits its face from.
    let ui = |mut div: gpui::Div| div.style().text.font_family.clone();
    for (what, family) in [
        ("nav", ui(crate::nav::shell(false))),
        (
            "menu row",
            ui(crate::components::menu_row_content(
                &crate::components::MenuItem::new("Rename"),
                false,
                false,
            )),
        ),
        (
            "floating surface",
            ui(crate::components::floating_surface()),
        ),
        ("settings sheet", ui(crate::prefs::sheet(400., 300.))),
        ("settings label", ui(crate::components::text_ui())),
        ("metadata", ui(crate::components::text_meta())),
    ] {
        assert_eq!(family.as_deref(), Some(FONT_UI), "{what} is UI text");
    }
    for (what, family) in [
        ("keycap", ui(crate::components::kbd("y"))),
        (
            "keys",
            ui(crate::components::key_combo("cmd-F", crate::theme::TEXT)),
        ),
        ("command well", ui(crate::decision::well(gpui::div()))),
    ] {
        assert_eq!(family.as_deref(), Some(FONT_CODE), "{what} is code");
    }
}

/// Focus is drawn only when it tells the operator something: a lone Pane
/// rests on its hairline with no head rule, and the focus ink appears once a
/// second Pane shares the board.
#[gpui::test]
fn focus_is_drawn_only_beside_another_pane(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("focus-only-when-shared", 2);
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
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.visible_indices().len()),
        1
    );
    for thread in &threads {
        assert!(
            cx.debug_bounds(Box::leak(
                format!("pane-focus-edge-{}", thread.get()).into_boxed_str(),
            ))
            .is_none(),
            "a lone Pane draws no focus"
        );
    }

    view.update(cx, |view, cx| view.enter_group(group, cx));
    tick(cx);
    assert!(view.read_with(cx, |view, _| view.visible_indices().len()) > 1);
    let shown = threads
        .iter()
        .filter(|thread| {
            cx.debug_bounds(Box::leak(
                format!("pane-focus-edge-{}", thread.get()).into_boxed_str(),
            ))
            .is_some()
        })
        .count();
    assert_eq!(
        shown, 1,
        "exactly the focused Pane of the two wears the ring"
    );
}

/// A group's gutter holds its worst-state dot and the disclosure's named
/// hit box, the summary starts at C1 after it, and the chevron trails the
/// summary: nothing at the column's right.
#[gpui::test]
fn a_group_chevron_trails_its_summary_and_its_target_leads_in_the_gutter(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("group-chevron-leads", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    for (id, name) in [("one", "Bash"), ("two", "Read")] {
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolStarted {
                id: id.into(),
                name: name.into(),
                input: serde_json::json!({ "command": "true" }),
            })
            .unwrap();
        fake.streams.borrow()[0]
            .send(SessionEvent::ToolCompleted {
                id: id.into(),
                output: "ok".into(),
                is_error: false,
                result: ferrite_core::ToolResult::Opaque,
            })
            .unwrap();
    }
    tick(cx);
    let row = cx.debug_bounds("tool-group-one").expect("the group");
    let control = view
        .read_with(cx, |view, _| {
            view.panes[0].tool_bounds(pane::DisclosureId::Group("one".into()))
        })
        .expect("the group's chevron");
    assert!(
        (control.left() - row.left()).abs() <= px(0.5),
        "the target sits in the gutter: {control:?} / {row:?}"
    );
    assert!(control.right() <= row.left() + px(crate::theme::GUTTER_W) + px(0.5));
    let chevron = cx
        .debug_bounds("disclosure-chevron")
        .expect("the chevron's box is always laid out");
    assert!(
        chevron.left() > control.right() && chevron.right() < row.left() + row.size.width / 2.,
        "the chevron trails the summary, not the column's right: {chevron:?} / {row:?}"
    );
    assert_eq!(chevron.size.width, px(crate::theme::ICON_CHEVRON));
}

/// The Composer is one input row in its box — the line, then the model pair
/// and the round send control — with a quiet meta row under it (mode at
/// left, usage at right). Enter still sends and the control turns to Stop
/// while the turn runs.
#[gpui::test]
fn the_composer_is_one_row_over_a_quiet_meta_row(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("composer-one-row", 1);
    let thread = core.threads()[0];
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let block = cx.debug_bounds("composer-block").unwrap();
    let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
    let meta = cx.debug_bounds("composer-meta").expect("the meta row");
    let send: &'static str =
        Box::leak(format!("composer-send-{:?}", PaneIdentity::Thread(thread)).into_boxed_str());
    let control = cx.debug_bounds(send).expect("the send control");
    assert_eq!(
        block.size.height,
        px(2. * crate::theme::COMPOSER_EDGE_W
            + crate::theme::COMPOSER_PAD_T
            + crate::theme::COMPOSER_PAD_B
            + crate::theme::COMPOSER_ROW_H),
        "one row in the box"
    );
    assert!(
        meta.top() >= block.bottom(),
        "the meta row is under the box"
    );
    assert!(editor.right() <= control.left() && control.right() <= block.right());
    assert!(control.top() >= editor.top() && control.bottom() <= editor.bottom() + px(0.5));
    assert!(
        cx.debug_bounds("prompt-placeholder").is_some(),
        "the resting line carries its one hint in the placeholder"
    );

    cx.simulate_input("go");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(fake.sent.borrow().as_slice(), ["go"], "Enter sends");
    let stop: &'static str =
        Box::leak(format!("composer-stop-{:?}", PaneIdentity::Thread(thread)).into_boxed_str());
    let stop = cx.debug_bounds(stop).expect("Stop while the turn runs");
    assert_eq!(stop.center(), control.center());
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 1, "Esc still interrupts");
}

/// The prompt heads its turn: the operator's line is at prose size, the
/// same size as the answer under it, and turns sit `GAP_TURN` apart while
/// the blocks inside one (the stamp included) sit a block step apart.
#[gpui::test]
fn the_prompt_heads_its_turn_at_prose_size(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("prompt-heads-turn", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    tick(cx);
    for (prompt, answer) in [
        ("first ask", "First answer."),
        ("second ask", "Second answer."),
    ] {
        cx.simulate_input(prompt);
        cx.simulate_keystrokes("enter");
        tick(cx);
        let stream = fake.streams.borrow()[0].clone();
        stream
            .send(SessionEvent::TextDelta {
                text: format!("{answer}\n\n"),
            })
            .unwrap();
        stream
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
        tick(cx);
    }
    let (namespace, prompts, stamps) = view.read_with(cx, |view, _| {
        let pane = &view.panes[0];
        let thread = view.cockpit.thread(pane.thread().unwrap()).unwrap();
        let ids = |kind: fn(&Body) -> bool| -> Vec<_> {
            thread
                .transcript()
                .blocks()
                .iter()
                .filter(|block| kind(&block.body))
                .map(|block| block.id)
                .collect()
        };
        (
            pane.text_namespace(),
            ids(|body| matches!(body, Body::Prompt(_))),
            ids(|body| matches!(body, Body::TurnEnd(_))),
        )
    });
    let size = cx.update(|_, cx| {
        crate::rich::testing::font_size(&format!("literal-{namespace}-{:?}-0", prompts[1]), cx)
    });
    assert_eq!(size, Some(px(crate::theme::FS_PROSE)), "prose size");
    assert_eq!(crate::theme::GAP_TURN, 32.);
    let mut line = |id| {
        cx.update(|_, cx| {
            crate::rich::testing::bounds(&format!("literal-{namespace}-{id:?}-0"), 0, cx).unwrap()
        })
    };
    // Measured, not just declared: the first turn's stamp to the second
    // prompt is the turn step.
    assert_eq!(
        line(prompts[1]).top() - line(stamps[0]).bottom(),
        px(crate::theme::GAP_TURN),
        "turns sit a turn step apart"
    );
}

/// A Decision card's head names its kind and says nothing more while it
/// simply waits: no `waiting` beside the card that is plainly waiting.
#[gpui::test]
fn a_waiting_decision_head_names_only_its_kind(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("decision-head-quiet", 1);
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(900.)));
    fake.streams.borrow()[0].send(question("quiet")).unwrap();
    tick(cx);
    assert!(
        cx.debug_bounds("question-island").is_some(),
        "the card is up"
    );
    assert!(
        cx.debug_bounds("decision-status").is_none(),
        "a waiting card carries no status word"
    );
}

/// A compact (L2) Composer's placeholder carries no hint: a narrow cell has
/// no room for one beside the ghost, and a clipped hint reads as noise.
#[gpui::test]
fn a_compact_placeholder_carries_no_hint(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("compact-placeholder", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(560.), px(700.)));
    tick(cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Instruments
    );
    assert!(cx.debug_bounds("prompt-placeholder").is_some(), "the ghost");
    assert!(
        cx.debug_bounds("prompt-placeholder-hint").is_none(),
        "no hint at L2"
    );
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    assert!(
        cx.debug_bounds("prompt-placeholder-hint").is_some(),
        "L1 carries its one hint"
    );
}

/// A Pane wider than the reading column lays its head out on the column's
/// grid: the title starts at the transcript's C1 (where the Composer's line
/// and every row's text start), so the Pane keeps one left edge. A narrow
/// Pane keeps the head at its own padding.
#[gpui::test]
fn the_head_title_starts_at_c1_on_a_wide_pane(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("head-on-column", 1);
    let thread = core.threads()[0];
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let title: &'static str =
        Box::leak(format!("pane-head-title-{}", thread.get()).into_boxed_str());
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let block = cx.debug_bounds("composer-block").unwrap();
    let c1 = block.left() + px(crate::theme::BOX_INSET_X + crate::theme::GUTTER_W);
    let head = cx.debug_bounds(title).expect("the head title");
    assert!(
        (head.left() - c1).abs() <= px(0.5),
        "the title {head:?} starts at C1 {c1:?}"
    );

    cx.simulate_resize(gpui::size(px(760.), px(900.)));
    tick(cx);
    let head = cx.debug_bounds(title).unwrap();
    let block = cx.debug_bounds("composer-block").unwrap();
    assert!(
        head.left() < block.left() + px(crate::theme::BOX_INSET_X + crate::theme::GUTTER_W),
        "a narrow Pane keeps its head at its own padding"
    );
}

/// An L2 cell with an approval reads like every other cell around its
/// card: the `model · branch` facts line under the head and the Composer's
/// meta row with the Session's mode. L1 still drops the mode while a
/// Decision owns the keyboard.
#[gpui::test]
fn an_l2_approval_cell_keeps_its_facts_and_mode(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("l2-approval-facts", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(560.), px(700.)));
    for event in [
        SessionEvent::Init {
            session_id: "facts".into(),
            model: "claude-opus-5-5[1m]".into(),
        },
        SessionEvent::PermissionMode {
            mode: "acceptEdits".into(),
        },
    ] {
        fake.streams.borrow()[0].send(event).unwrap();
    }
    tick(cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Instruments
    );
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap().get());
    let facts: &'static str = Box::leak(format!("l2-facts-{thread}").into_boxed_str());
    let mode: &'static str = Box::leak(format!("composer-mode-{thread}").into_boxed_str());
    let quiet_facts = cx.debug_bounds(facts).expect("a quiet cell's facts line");
    assert!(cx.debug_bounds(mode).is_some(), "a quiet cell's mode");

    fake.streams.borrow()[0].send(decision("l2-facts")).unwrap();
    tick(cx);
    let asked = cx
        .debug_bounds(facts)
        .expect("the approval cell's facts line");
    assert_eq!(asked, quiet_facts, "the facts line holds its place");
    assert!(cx.debug_bounds(mode).is_some(), "the approval cell's mode");

    // At L1 the Decision owns the keyboard, and the mode steps aside.
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    assert!(
        cx.debug_bounds(mode).is_none(),
        "L1 drops the mode under a Decision"
    );
}
