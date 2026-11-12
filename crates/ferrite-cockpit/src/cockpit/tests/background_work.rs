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
