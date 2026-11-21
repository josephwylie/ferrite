use super::*;
use ferrite_core::progress::{BackgroundTask, ProgressEvent, TaskStatus};

fn running(fake: &Fake, tasks: &[(&str, &str)]) {
    fake.streams.borrow()[0]
        .send(SessionEvent::Progress {
            event: ProgressEvent::BackgroundSnapshot {
                tasks: tasks
                    .iter()
                    .map(|(id, label)| BackgroundTask {
                        id: (*id).into(),
                        label: (*label).into(),
                        detail: "local_bash".into(),
                        status: TaskStatus::Working,
                    })
                    .collect(),
            },
        })
        .unwrap();
}

/// ADR 0038: running background work is the transcript's last line, above
/// the Composer, and goes when the work does.
#[gpui::test]
fn background_work_is_the_transcripts_last_line(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-line", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    assert!(debug_bounds(cx, "background-work-line".into()).is_none());
    running(&fake, &[("task:1", "cargo test --workspace")]);
    tick(cx);
    assert!(debug_bounds(cx, "background-work-line".into()).is_some());
    assert!(debug_bounds(cx, "background-details".into()).is_some());
    assert!(
        debug_bounds(cx, "background-work-detail".into()).is_none(),
        "one task names itself in the title and has no description"
    );
    running(&fake, &[]);
    tick(cx);
    assert!(debug_bounds(cx, "background-work-line".into()).is_none());
}

/// `Details` opens the `Background work` dialog with a section per task,
/// and Escape closes it.
#[gpui::test]
fn details_opens_the_background_work_dialog(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-dialog", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "cargo test"), ("task:2", "npm run dev")]);
    tick(cx);
    assert!(debug_bounds(cx, "background-dialog".into()).is_none());
    assert!(
        debug_bounds(cx, "background-work-detail".into()).is_some(),
        "several tasks list their labels after the title"
    );
    let details = bounds(cx, "background-details".into());
    cx.simulate_click(details.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(debug_bounds(cx, "background-dialog".into()).is_some());
    cx.update(|window, app| {
        view.update(app, |view, cx| view.interrupt(&Interrupt, window, cx))
    });
    tick(cx);
    assert!(
        debug_bounds(cx, "background-dialog".into()).is_none(),
        "Escape closes the dialog"
    );
}

/// Once the last piece of work ends, the dialog stays up and says there is
/// none, rather than draw nothing under its title.
#[gpui::test]
fn the_background_work_dialog_says_when_no_work_is_left(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-dialog-empty", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "cargo test")]);
    tick(cx);
    let details = bounds(cx, "background-details".into());
    cx.simulate_click(details.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(debug_bounds(cx, "background-dialog-empty".into()).is_none(), "work is listed");
    running(&fake, &[]);
    tick(cx);
    assert!(debug_bounds(cx, "background-dialog".into()).is_some(), "the dialog stays up");
    assert!(debug_bounds(cx, "background-dialog-section".into()).is_none());
    assert!(
        debug_bounds(cx, "background-dialog-empty".into()).is_some(),
        "it says there is no background work"
    );
}

/// One Claude subagent goes by three names (its `Agent` call, its task and
/// its agent id, joined by aliases): the line counts the agent once.
#[gpui::test]
fn one_running_subagent_is_waited_on_once(cx: &mut TestAppContext) {
    use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject, TranscriptCoverage};
    let (core, fake) = cockpit("background-one-subagent", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let key = |native: &str| AgentKey::new(Provider::Claude, "ui-fixture", native);
    let emit = |event: ActivityEvent| {
        fake.streams.borrow()[0].send(SessionEvent::Activity(event)).unwrap();
    };
    let mut info = AgentInfo::new(key("toolu_count"));
    info.name = Some("Count .txt files in directory".into());
    info.parent = Some(Subject::Main);
    info.coverage = TranscriptCoverage::Live;
    emit(ActivityEvent::Discovered(info));
    emit(ActivityEvent::Status {
        key: key("toolu_count"),
        state: AgentStatus::Working,
    });
    for alias in ["task:t1", "agent:a1"] {
        emit(ActivityEvent::Alias {
            from: key(alias),
            to: key("toolu_count"),
        });
    }
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.background_work_title(0)).as_deref(),
        Some("Waiting on a subagent")
    );
    assert!(debug_bounds(cx, "background-work-line".into()).is_some());
}

/// A refused `Stop` clears its `Stopping...`, says why beside the line,
/// and the same button retries it.
#[gpui::test]
fn a_refused_background_stop_offers_retry(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-stop-refused", 1);
    *fake.native_controls.borrow_mut() = true;
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "cargo test")]);
    tick(cx);
    *fake.fail_control.borrow_mut() = true;
    let stop = bounds(cx, "background-stop-all".into());
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(debug_bounds(cx, "background-stopping".into()).is_none(), "nothing is pending");
    assert!(debug_bounds(cx, "background-stop-refused".into()).is_some(), "it says why");
    *fake.fail_control.borrow_mut() = false;
    let retry = bounds(cx, "background-stop-all".into());
    cx.simulate_click(retry.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        *fake.controls.borrow(),
        [ferrite_core::SessionControl::StopTask { id: "task:1".into() }]
    );
    assert!(debug_bounds(cx, "background-stopping".into()).is_some());
    assert!(debug_bounds(cx, "background-stop-refused".into()).is_none());
}

fn refuse_stop_later(fake: &Fake, id: &str) {
    fake.streams.borrow()[0]
        .send(SessionEvent::ControlRefused {
            control: ferrite_core::SessionControl::StopTask { id: id.into() },
            message: "no such task".into(),
        })
        .unwrap();
}

/// Claude answers a `stop_task` after it was accepted: the refusal comes as
/// an event naming the control, and the line gets its reason and `Retry`;
/// a retry waits again, and the next refusal reads as its own.
#[gpui::test]
fn a_stop_the_provider_refuses_later_offers_retry(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-stop-late", 1);
    *fake.native_controls.borrow_mut() = true;
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "cargo test")]);
    tick(cx);
    let stop = bounds(cx, "background-stop-all".into());
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(debug_bounds(cx, "background-stopping".into()).is_some());
    refuse_stop_later(&fake, "task:1");
    tick(cx);
    assert!(debug_bounds(cx, "background-stopping".into()).is_none(), "nothing is pending");
    assert!(debug_bounds(cx, "background-stop-refused".into()).is_some(), "it says why");
    let retry = bounds(cx, "background-stop-all".into());
    cx.simulate_click(retry.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(debug_bounds(cx, "background-stopping".into()).is_some(), "the retry waits again");
    assert!(debug_bounds(cx, "background-stop-refused".into()).is_none());
    refuse_stop_later(&fake, "task:1");
    tick(cx);
    assert!(debug_bounds(cx, "background-stop-refused".into()).is_some(), "its refusal is its own");
}

/// A provider that never answers leaves the task running: past the wait
/// the line stops saying `Stopping...` and offers `Retry`.
#[gpui::test]
fn a_stop_the_provider_never_settles_offers_retry(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-stop-silent", 1);
    *fake.native_controls.borrow_mut() = true;
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "cargo test")]);
    tick(cx);
    let stop = bounds(cx, "background-stop-all".into());
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(debug_bounds(cx, "background-stopping".into()).is_some());
    cx.executor().advance_clock(super::super::background_work::STOP_SETTLES_WITHIN);
    tick(cx);
    assert!(debug_bounds(cx, "background-stopping".into()).is_none());
    assert!(debug_bounds(cx, "background-stop-refused".into()).is_some(), "it says why");
    assert!(debug_bounds(cx, "background-stop-all".into()).is_some(), "Retry is offered");
}

fn press_stop(cx: &mut gpui::VisualTestContext) {
    let stop = bounds(cx, "background-stop-all".into());
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    tick(cx);
}

fn reason(view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, cx| view.background_stop_reason(0, "task:1", cx))
}

/// A provider's refusal is the reason the line gives, even when the wait
/// for an answer runs out after it.
#[gpui::test]
fn the_wait_running_out_keeps_the_providers_reason(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-stop-reason-kept", 1);
    *fake.native_controls.borrow_mut() = true;
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "cargo test")]);
    tick(cx);
    press_stop(cx);
    refuse_stop_later(&fake, "task:1");
    tick(cx);
    cx.executor().advance_clock(super::super::background_work::STOP_SETTLES_WITHIN);
    tick(cx);
    assert_eq!(reason(&view, cx).as_deref(), Some("no such task"));
}

/// A Retry owns its own wait: the first press's timer cannot cut it short.
#[gpui::test]
fn an_earlier_stops_timer_does_not_cut_a_retry_short(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-stop-stale-timer", 1);
    *fake.native_controls.borrow_mut() = true;
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "cargo test")]);
    tick(cx);
    *fake.fail_control.borrow_mut() = true;
    press_stop(cx);
    cx.executor().advance_clock(super::super::background_work::STOP_SETTLES_WITHIN / 2 + std::time::Duration::from_secs(1));
    *fake.fail_control.borrow_mut() = false;
    press_stop(cx);
    assert!(debug_bounds(cx, "background-stopping".into()).is_some());
    cx.executor().advance_clock(std::time::Duration::from_secs(5));
    tick(cx);
    assert!(
        debug_bounds(cx, "background-stopping".into()).is_some(),
        "the retry has had 5 of its 10 seconds"
    );
}

/// A refusal that comes after the wait ran out answers the stop that
/// timed out, not the Retry sent since.
#[gpui::test]
fn a_late_refusal_is_not_blamed_on_the_retry(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-stop-late-blame", 1);
    *fake.native_controls.borrow_mut() = true;
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "cargo test")]);
    tick(cx);
    press_stop(cx);
    cx.executor().advance_clock(super::super::background_work::STOP_SETTLES_WITHIN);
    tick(cx);
    press_stop(cx);
    refuse_stop_later(&fake, "task:1");
    tick(cx);
    assert!(debug_bounds(cx, "background-stopping".into()).is_some(), "the retry still waits");
    refuse_stop_later(&fake, "task:1");
    tick(cx);
    assert_eq!(reason(&view, cx).as_deref(), Some("no such task"));
}

/// A stop is remembered only while its task runs.
#[gpui::test]
fn a_finished_tasks_stop_is_forgotten(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-stop-pruned", 1);
    *fake.native_controls.borrow_mut() = true;
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "cargo test")]);
    tick(cx);
    press_stop(cx);
    assert_eq!(view.read_with(cx, |view, cx| view.background.read(cx).stops_held()), 1);
    running(&fake, &[]);
    tick(cx);
    assert_eq!(view.read_with(cx, |view, cx| view.background.read(cx).stops_held()), 0);
    running(&fake, &[("task:1", "cargo test")]);
    tick(cx);
    assert!(debug_bounds(cx, "background-stop-says-Stop".into()).is_some(), "the line is plain Stop again");
}

fn working_agent(fake: &Fake, native: &str, name: &str) -> ferrite_core::activity::AgentKey {
    use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject, TranscriptCoverage};
    let key = AgentKey::new(Provider::Claude, "ui-fixture", native);
    let mut info = AgentInfo::new(key.clone());
    info.name = Some(name.into());
    info.parent = Some(Subject::Main);
    info.coverage = TranscriptCoverage::Live;
    let stream = &fake.streams.borrow()[0];
    stream.send(SessionEvent::Activity(ActivityEvent::Discovered(info))).unwrap();
    stream
        .send(SessionEvent::Activity(ActivityEvent::Status {
            key: key.clone(),
            state: AgentStatus::Working,
        }))
        .unwrap();
    key
}

/// A line of agents alone offers T3's plain `Stop`, which ends each of them.
#[gpui::test]
fn stop_on_a_line_of_agents_stops_them(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-stop-agents", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let first = working_agent(&fake, "toolu_a", "Read the docs");
    let second = working_agent(&fake, "toolu_b", "Run the audit");
    tick(cx);
    assert!(debug_bounds(cx, "background-stop-says-Stop".into()).is_some());
    press_stop(cx);
    let mut stopped = fake.stopped.borrow().clone();
    stopped.sort();
    assert_eq!(stopped, [first, second]);
}

/// Commands and agents together: `Stop` ends both.
#[gpui::test]
fn stop_on_commands_and_agents_stops_both(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-stop-all-work", 1);
    *fake.native_controls.borrow_mut() = true;
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "cargo test")]);
    let agent = working_agent(&fake, "toolu_a", "Read the docs");
    tick(cx);
    assert!(debug_bounds(cx, "background-stop-says-Stop".into()).is_some());
    press_stop(cx);
    assert_eq!(
        *fake.controls.borrow(),
        [ferrite_core::SessionControl::StopTask { id: "task:1".into() }]
    );
    assert_eq!(*fake.stopped.borrow(), [agent]);
}

/// Where the Session cannot stop commands, the line does not pretend to:
/// its `Stop` ends the agents and says what it leaves running.
#[gpui::test]
fn stop_that_cannot_reach_commands_says_what_it_leaves(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-stop-agents-only-of-both", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "cargo test")]);
    working_agent(&fake, "toolu_a", "Read the docs");
    tick(cx);
    assert!(debug_bounds(cx, "background-stop-says-Stop".into()).is_some());
    assert!(debug_bounds(cx, "background-stop-remains-1 command left running".into()).is_some());
}

/// An agent whose status is not known has nothing to stop: the line stops
/// the command and names the agent it leaves.
#[gpui::test]
fn stop_that_cannot_reach_an_unknown_agent_names_it(cx: &mut TestAppContext) {
    use ferrite_core::activity::{
        ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject, TranscriptCoverage,
    };
    let (core, fake) = cockpit("background-stop-unknown-agent", 1);
    *fake.native_controls.borrow_mut() = true;
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "cargo test")]);
    let key = AgentKey::new(Provider::Claude, "ui-fixture", "toolu_a");
    let mut info = AgentInfo::new(key.clone());
    info.name = Some("Read the docs".into());
    info.parent = Some(Subject::Main);
    info.coverage = TranscriptCoverage::Live;
    let stream = &fake.streams.borrow()[0];
    stream.send(SessionEvent::Activity(ActivityEvent::Discovered(info))).unwrap();
    stream
        .send(SessionEvent::Activity(ActivityEvent::Status { key, state: AgentStatus::Unknown }))
        .unwrap();
    tick(cx);
    assert!(debug_bounds(cx, "background-stop-says-Stop".into()).is_some());
    assert!(debug_bounds(cx, "background-stop-remains-1 agent left running".into()).is_some());
}

fn open_background_tab(view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    view.update(cx, |view, cx| {
        let thread = view.focused_thread().unwrap();
        view.open_background(thread, cx);
    });
    tick(cx);
}

/// The line, its dialog and the Background tab count the same work: an
/// agent the Thread waits on is in all three.
#[gpui::test]
fn an_agent_waited_on_is_in_the_line_the_dialog_and_the_tab(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-inventory-agent", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    working_agent(&fake, "toolu_a", "Read the docs");
    tick(cx);
    open_background_tab(&view, cx);
    assert!(debug_bounds(cx, "background-work-line".into()).is_some());
    assert!(debug_bounds(cx, "background-running-0".into()).is_some(), "the tab lists it");
    assert!(debug_bounds(cx, "background-tab-open-toolu_a".into()).is_some(), "and opens it");
    let details = bounds(cx, "background-details".into());
    cx.simulate_click(details.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(debug_bounds(cx, "background-dialog-open-toolu_a".into()).is_some(), "the dialog lists it");
}

/// `Open` on an agent in the Background tab, and `Open thread` in the
/// dialog, give the Pane that agent's full chat.
#[gpui::test]
fn opening_a_background_agent_shows_its_full_chat(cx: &mut TestAppContext) {
    use ferrite_core::activity::Subject;
    let (core, fake) = cockpit("background-open-agent", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let key = working_agent(&fake, "toolu_a", "Read the docs");
    tick(cx);
    open_background_tab(&view, cx);
    let open = bounds(cx, "background-tab-open-toolu_a".into());
    cx.simulate_click(open.center(), gpui::Modifiers::none());
    tick(cx);
    let selected = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.panes[0].selected.clone())
    };
    assert_eq!(selected(&view, cx), Subject::Subagent(key.clone()), "the tab's Open");

    cx.update(|window, app| {
        view.update(app, |view, cx| view.select_subject(view.panes[0].thread().unwrap(), Subject::Main, window, cx))
    });
    tick(cx);
    let details = bounds(cx, "background-details".into());
    cx.simulate_click(details.center(), gpui::Modifiers::none());
    tick(cx);
    let open = bounds(cx, "background-dialog-open-toolu_a".into());
    cx.simulate_click(open.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(selected(&view, cx), Subject::Subagent(key), "the dialog's Open thread");
}

/// While Main's turn runs its agents are rows of the transcript, so the line
/// stays away, but the Background tab keeps listing them.
#[gpui::test]
fn an_agent_is_in_the_tab_while_main_is_busy(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-inventory-busy", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let send = |event: SessionEvent| fake.streams.borrow()[0].send(event).unwrap();
    for id in ["toolu_bg", "toolu_fg"] {
        send(SessionEvent::ToolStarted {
            id: id.into(),
            name: "Agent".into(),
            input: serde_json::json!({"description": id, "prompt": "Read the docs"}),
        });
    }
    send(SessionEvent::ToolCompleted {
        id: "toolu_bg".into(),
        output: "Async agent launched.".into(),
        is_error: false,
        result: ferrite_core::ToolResult::Background { task: "agent:bg".into() },
    });
    working_agent(&fake, "toolu_bg", "In the background");
    working_agent(&fake, "toolu_fg", "Waited on");
    tick(cx);
    assert!(view.read_with(cx, |view, _| {
        view.cockpit.thread(view.panes[0].thread().unwrap()).unwrap().busy()
    }));
    open_background_tab(&view, cx);
    assert!(debug_bounds(cx, "background-work-line".into()).is_none(), "the rows show it");
    assert!(debug_bounds(cx, "background-running-0".into()).is_some(), "the tab lists it");
    assert!(
        debug_bounds(cx, "background-running-1".into()).is_none(),
        "the agent Main waits on is not background work"
    );
}

/// A command only a progress snapshot knows of is in the tab as well.
#[gpui::test]
fn a_command_known_only_from_a_snapshot_is_in_the_tab(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-inventory-command", 1);
    *fake.native_controls.borrow_mut() = true;
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    running(&fake, &[("task:1", "npm run dev")]);
    tick(cx);
    open_background_tab(&view, cx);
    assert!(debug_bounds(cx, "background-running-0".into()).is_some());
    let stop = bounds(cx, "background-stop-task:1".into());
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        *fake.controls.borrow(),
        [ferrite_core::SessionControl::StopTask { id: "task:1".into() }]
    );
    assert!(debug_bounds(cx, "background-stopping".into()).is_some(), "the line says so too");
}

/// The line's clock lays out again as its reading widens, `9s` to `10s`,
/// so it never runs into `Details`.
#[gpui::test]
fn the_lines_clock_widens_clear_of_details(cx: &mut TestAppContext) {
    ferrite_ui::motion::testing::drive();
    let held = hold_wall_clock();
    let (core, fake) = cockpit("background-clock-width", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let send = |event: SessionEvent| fake.streams.borrow()[0].send(event).unwrap();
    send(SessionEvent::ToolStarted {
        id: "toolu_bg".into(),
        name: "Agent".into(),
        input: serde_json::json!({"description": "Read the docs", "prompt": "Read the docs"}),
    });
    send(SessionEvent::ToolCompleted {
        id: "toolu_bg".into(),
        output: "Async agent launched.".into(),
        is_error: false,
        result: ferrite_core::ToolResult::Background { task: "agent:bg".into() },
    });
    send(SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    });
    tick(cx);
    let step = |cx: &mut gpui::VisualTestContext, by: u64| {
        held.advance(Duration::from_secs(by));
        cx.executor().advance_clock(Duration::from_secs(by));
        tick(cx);
    };
    step(cx, 9);
    let narrow = bounds(cx, "background-line-clock".into());
    step(cx, 1);
    let wide = bounds(cx, "background-line-clock".into());
    let details = bounds(cx, "background-details".into());
    assert!(wide.size.width > narrow.size.width, "`10s` is laid out wider than `9s`");
    assert!(wide.right() <= details.left(), "the clock stays clear of Details");
}
