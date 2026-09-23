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
    let layers = view.read_with(cx, |view, _| view.toast_layers);
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
    assert_eq!(view.read_with(cx, |view, _| view.toast_layers), 0);
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
        "nav-audit",
        "composer-audit",
        "settings-audit",
        "tokens-audit",
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
        effort.right() <= block.right() - px(crate::theme::COMPOSER_PAD_X) + px(0.5),
        "effort {effort:?} stays inside the block {block:?}"
    );
    assert!(model.right() <= effort.left());
    let band = cx.debug_bounds("draft-band").expect("the setup chips");
    assert!(band.right() <= model.left(), "{band:?} / {model:?}");
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
