use super::*;
use ferrite_core::activity::{
    ActivityEvent, AgentInfo, AgentKey, AgentStatus, ExecutionEvent, Subject, TranscriptCoverage,
};

fn key(name: &str) -> AgentKey {
    AgentKey::new(Provider::Claude, "ui-fixture", name)
}
fn child(fake: &Fake, name: &str, status: AgentStatus) {
    let mut info = AgentInfo::new(key(name));
    info.name = Some(name.into());
    info.parent = Some(Subject::Main);
    info.coverage = TranscriptCoverage::Live;
    emit(fake, ActivityEvent::Discovered(info));
    emit(
        fake,
        ActivityEvent::Content {
            key: key(name),
            id: Some(format!("{name}-text")),
            event: ExecutionEvent::Text {
                text: format!("{name} transcript\n\n"),
            },
        },
    );
    emit(
        fake,
        ActivityEvent::Status {
            key: key(name),
            state: status,
        },
    );
}

fn discover(fake: &Fake, name: &str) {
    let mut info = AgentInfo::new(key(name));
    info.name = Some(name.into());
    info.parent = Some(Subject::Main);
    info.coverage = TranscriptCoverage::Live;
    emit(fake, ActivityEvent::Discovered(info));
}
fn emit(fake: &Fake, event: ActivityEvent) {
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(event))
        .unwrap();
}
/// Give Pane 0 the subagent `name`'s own transcript (`select_subject`, as a
/// notice's jump does), by the call that started it: the fixture's keys are
/// made from the agents' names. A press on its `Ran agent` row opens it in
/// the right pane instead (`agent_pane`).
pub(super) fn click_child(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
    name: &str,
) {
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let subject = view.read_with(cx, |view, _| view.agent_of_call(thread, name).unwrap());
    cx.update(|window, app| {
        view.update(app, |view, cx| view.select_subject(thread, subject, window, cx))
    });
    cx.run_until_parked();
}
/// The Main transcript's `Agent` call that started `name`.
fn agent_call(fake: &Fake, name: &str) {
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: name.into(),
            name: "Agent".into(),
            input: serde_json::json!({ "description": format!("Audit {name}") }),
        })
        .unwrap();
}

#[gpui::test]
fn child_tabs_switch_transcripts_preserve_main_draft_and_survive_reorder(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("subagents-switch", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    child(&fake, "Atlas", AgentStatus::Working);
    child(&fake, "Cedar", AgentStatus::Idle);
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    assert_eq!(
        view.read_with(cx, |view, _| view
            .facts
            .get(thread)
            .map_or(0, |facts| facts.subagents)),
        2,
        "the Thread card counts the discovered children"
    );
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("keep my draft".into(), cx))
    });
    click_child(&view, cx, "Atlas");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[0].selected, Subject::Subagent(key("Atlas")));
        assert_eq!(view.panes[0].composer.read(cx).text(), "keep my draft");
        let subject = view
            .cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&view.panes[0].selected)
            .unwrap();
        assert!(plain_text(subject.transcript().blocks()).contains("Atlas transcript"));
    });
    assert!(cx.debug_bounds("subagent-head-1").is_some());
    assert!(
        cx.debug_bounds("composer-block").is_none(),
        "a subagent takes no prompts"
    );
    emit(
        &fake,
        ActivityEvent::Status {
            key: key("Atlas"),
            state: AgentStatus::Idle,
        },
    );
    emit(
        &fake,
        ActivityEvent::Status {
            key: key("Cedar"),
            state: AgentStatus::Working,
        },
    );
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.panes[0].selected.clone()),
        Subject::Subagent(key("Atlas"))
    );
    // The observational view cannot submit or consume the Main draft.
    cx.simulate_keystrokes("enter");
    assert!(fake.sent.borrow().is_empty());
    let main = cx.debug_bounds("subagent-main-1").unwrap().center();
    cx.simulate_click(main, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert!(view.panes[0].is_main());
        assert_eq!(view.panes[0].composer.read(cx).text(), "keep my draft");
    });
    assert!(
        cx.debug_bounds("subagent-head-1").is_none(),
        "Main has no head"
    );
}

#[gpui::test]
fn child_scroll_disclosure_and_native_text_entity_survive_switching(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("subagents-view-state", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    child(&fake, "Atlas", AgentStatus::Working);
    child(&fake, "Cedar", AgentStatus::Working);
    emit(
        &fake,
        ActivityEvent::Content {
            key: key("Atlas"),
            id: Some("long".into()),
            event: ExecutionEvent::Text {
                text: "A separate paragraph of transcript history.\n\n".repeat(90),
            },
        },
    );
    for name in ["Atlas", "Cedar"] {
        emit(
            &fake,
            ActivityEvent::Content {
                key: key(name),
                id: Some(format!("{name}-tool")),
                event: ExecutionEvent::ToolStarted {
                    id: "same-call".into(),
                    name: "Read".into(),
                    input: serde_json::json!({"file_path":"src/lib.rs"}),
                },
            },
        );
        emit(
            &fake,
            ActivityEvent::Content {
                key: key(name),
                id: Some(format!("{name}-result")),
                event: ExecutionEvent::ToolCompleted {
                    id: "same-call".into(),
                    output: "pub fn checked() {}".into(),
                    is_error: false,
                    result: ferrite_core::ToolResult::Opaque,
                },
            },
        );
    }
    tick(cx);
    click_child(&view, cx, "Atlas");
    tick(cx);
    open_live(cx);
    let viewport = view.read_with(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .bounds()
    });
    let toggle = view.read_with(cx, |view, cx| {
        view.panes[0]
            .tool_bounds("same-call", cx)
            .expect("tool control")
            .center()
    });
    assert!(
        viewport.contains(&toggle),
        "tool control is visibly clickable"
    );
    cx.simulate_mouse_down(toggle, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(toggle, MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    let scroller = view.read_with(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .scroll()
            .bounds()
            .center()
    });
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: scroller,
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(240.))),
        ..Default::default()
    });
    cx.run_until_parked();
    let offset = view.read_with(cx, |view, cx| {
        let transcript = view.panes[0].transcript().unwrap();
        assert!(view.panes[0].tool_expanded("same-call"));
        assert!(!transcript.read(cx).is_following_tail());
        transcript.read(cx).scroll().offset()
    });
    let namespace = view.read_with(cx, |view, _| view.panes[0].text_namespace());
    let entity = cx.update(|_, cx| {
        ferrite_pane::rich::testing::first_entity(&format!("markdown-{namespace}-"), cx).unwrap()
    });
    click_child(&view, cx, "Cedar");
    assert!(!view.read_with(cx, |view, _| view.panes[0].tool_expanded("same-call")));
    click_child(&view, cx, "Atlas");
    view.read_with(cx, |view, cx| {
        let transcript = view.panes[0].transcript().unwrap();
        assert!(view.panes[0].tool_expanded("same-call"));
        assert_eq!(transcript.read(cx).scroll().offset(), offset);
        assert!(!transcript.read(cx).is_following_tail());
    });
    assert_eq!(
        cx.update(|_, cx| ferrite_pane::rich::testing::first_entity(
            &format!("markdown-{namespace}-"),
            cx
        )
        .unwrap()),
        entity
    );
}

#[gpui::test]
fn core_eviction_releases_a_hidden_retained_transcript_entity(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("subagents-evicted-view", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));

    child(&fake, "Atlas", AgentStatus::Idle);
    for index in 0..128 {
        discover(&fake, &format!("filler-{index}"));
    }
    child(&fake, "Tail", AgentStatus::Idle);
    tick(cx);
    tick(cx);

    let atlas = Subject::Subagent(key("Atlas"));
    let tail = Subject::Subagent(key("Tail"));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let select =
        |subject: Subject, view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
            view.update(cx, |view, cx| {
                let generation = view
                    .cockpit
                    .thread(thread)
                    .unwrap()
                    .activity()
                    .subject(&subject)
                    .unwrap()
                    .revision();
                view.panes[0].select_subject(subject, generation, cx);
                cx.notify();
            });
            cx.run_until_parked();
        };
    select(atlas.clone(), &view, cx);
    let (atlas_entity, atlas_weak) = view.read_with(cx, |view, _| {
        let transcript = &view.panes[0].transcripts[&atlas];
        (transcript.entity_id(), transcript.downgrade())
    });

    // Switching alone preserves hidden Subject state while Tail's history is
    // still waiting to restore.
    select(tail.clone(), &view, cx);
    view.update(cx, |view, cx| view.retry_subject_history(0, cx));
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes[0].transcripts[&atlas].entity_id(), atlas_entity);
    });

    // Retaining Tail exceeds core's child budget and evicts the hidden Atlas.
    for _ in 0..8 {
        tick(cx);
    }
    view.read_with(cx, |view, _| {
        let atlas_core = view
            .cockpit
            .thread(thread)
            .unwrap()
            .activity()
            .subject(&atlas)
            .unwrap();
        assert!(!atlas_core.retained());
        assert!(!view.panes[0].transcripts.contains_key(&atlas));
        assert!(view.panes[0].transcripts.contains_key(&tail));
        assert!(!view.transcript_entities.contains(&atlas_entity));
    });
    assert!(
        atlas_weak.upgrade().is_none(),
        "the evicted snapshot is released"
    );
}

#[gpui::test]
fn hidden_child_requests_jump_and_reply_only_to_their_captured_handle(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("subagents-decisions", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    for name in ["Atlas", "Cedar"] {
        child(&fake, name, AgentStatus::Waiting);
        let SessionEvent::DecisionRequested { mut decision } = decision(name) else {
            unreachable!()
        };
        decision.tool_use_id = "same-call".into();
        emit(
            &fake,
            ActivityEvent::Decision {
                subject: Some(Subject::Subagent(key(name))),
                decision,
            },
        );
    }
    tick(cx);
    // ⌘D (the titlebar has no needs-you door any more: the nav's strip
    // names its key).
    cx.dispatch_action(crate::cockpit::NextDecision);
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.panes[0].selected.clone()),
        Subject::Subagent(key("Atlas"))
    );
    let serial = view.read_with(cx, |view, _| {
        view.cockpit
            .thread(view.panes[0].thread().unwrap())
            .unwrap()
            .pending_decisions()[0]
            .handle
            .serial
    });
    // Allow once is the split button's face.
    let allow = bounds(cx, format!("request-allow-1-{serial}")).center();
    cx.simulate_click(allow, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(fake.answered.borrow().len(), 1);
    assert_eq!(fake.answered.borrow()[0].0, "Atlas");
    view.read_with(cx, |view, _| {
        let pending = view
            .cockpit
            .thread(view.panes[0].thread().unwrap())
            .unwrap()
            .pending_decisions();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].decision.id, "Cedar");
    });
}

#[gpui::test]
fn native_selection_clears_on_subject_navigation_without_leaking_into_another_agent(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("subagents-selection", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    child(&fake, "Atlas", AgentStatus::Working);
    child(&fake, "Cedar", AgentStatus::Working);
    tick(cx);
    click_child(&view, cx, "Atlas");
    let from = caret(&view, cx, 0, 0);
    let to = caret(&view, cx, 0, 5);
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("Atlas"));
    click_child(&view, cx, "Cedar");
    assert!(!cx.update(|window, cx| gpui::base::TextSelection::has_selection(window, cx)));
    click_child(&view, cx, "Atlas");
    assert!(!cx.update(|window, cx| gpui::base::TextSelection::has_selection(window, cx)));
}

#[gpui::test]
fn next_child_request_leaves_another_group_and_opens_its_root(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("subagents-request-view", 3);
    let threads = core.threads();
    let group = core
        .apply_group(GroupChange::Create {
            first: threads[1],
            second: threads[2],
        })
        .unwrap()
        .group
        .unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| view.enter_group(group, cx));
    child(&fake, "Atlas", AgentStatus::Waiting);
    let SessionEvent::DecisionRequested { decision } = decision("child-away") else {
        unreachable!()
    };
    emit(
        &fake,
        ActivityEvent::Decision {
            subject: Some(Subject::Subagent(key("Atlas"))),
            decision,
        },
    );
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.cockpit.roster().view()),
        View::Group(group)
    );
    cx.simulate_keystrokes("cmd-d");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.roster().view(), View::Solo);
        assert_eq!(view.cockpit.roster().focused_thread(), Some(threads[0]));
        assert_eq!(
            view.panes[view.focused()].selected,
            Subject::Subagent(key("Atlas"))
        );
    });
}

#[gpui::test]
fn unattributed_question_keeps_native_input_focus_across_repaint_and_sends_exact_answer(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("subagents-unattributed-question", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("Main draft".into(), cx))
    });
    let SessionEvent::DecisionRequested { decision } = question("unattributed-question") else {
        unreachable!()
    };
    emit(
        &fake,
        ActivityEvent::Decision {
            subject: None,
            decision,
        },
    );
    tick(cx);
    let serial = view.read_with(cx, |view, _| {
        let activity = view
            .cockpit
            .thread(view.panes[0].thread().unwrap())
            .unwrap()
            .activity();
        assert!(activity.children().is_empty());
        assert_eq!(activity.pending_decisions().len(), 1);
        activity.pending_decisions()[0].handle.serial
    });
    let input = bounds(cx, format!("request-other-1-{serial}-0")).center();
    cx.simulate_click(input, gpui::Modifiers::none());
    cx.run_until_parked();
    cx.simulate_input("neither, ");
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    cx.simulate_input("wait 2 days");
    cx.run_until_parked();
    assert_eq!(composer_text(&view, cx), "Main draft");
    let submit = bounds(cx, format!("request-submit-1-{serial}")).center();
    cx.simulate_click(submit, gpui::Modifiers::none());
    cx.run_until_parked();
    let answered = fake.answered.borrow();
    assert_eq!(answered.len(), 1);
    assert_eq!(answered[0].0, "unattributed-question");
    let DecisionAnswer::Questions { answers } = &answered[0].1 else {
        panic!("freeform answer");
    };
    assert_eq!(answers[0].other.as_deref(), Some("neither, wait 2 days"));
}

#[gpui::test]
fn attention_and_global_navigation_visit_every_distinct_pending_subject_then_wrap(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("subagents-request-cycle", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    for name in ["Atlas", "Cedar", "Finch"] {
        child(&fake, name, AgentStatus::Waiting);
        let SessionEvent::DecisionRequested { decision } = decision(name) else {
            unreachable!()
        };
        emit(
            &fake,
            ActivityEvent::Decision {
                subject: Some(Subject::Subagent(key(name))),
                decision,
            },
        );
    }
    let SessionEvent::DecisionRequested { decision } = decision("Atlas-second") else {
        unreachable!()
    };
    emit(
        &fake,
        ActivityEvent::Decision {
            subject: Some(Subject::Subagent(key("Atlas"))),
            decision,
        },
    );
    tick(cx);
    for name in ["Atlas", "Cedar", "Finch", "Atlas"] {
        cx.dispatch_action(crate::cockpit::NextDecision);
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |view, _| view.panes[0].selected.clone()),
            Subject::Subagent(key(name))
        );
    }
    for name in ["Cedar", "Finch", "Atlas", "Cedar"] {
        cx.simulate_keystrokes("cmd-d");
        cx.run_until_parked();
        assert_eq!(
            view.read_with(cx, |view, _| view.panes[0].selected.clone()),
            Subject::Subagent(key(name))
        );
    }
}

/// Esc on a live Subagent, opened from its row, stops that Subagent and
/// leaves Main's turn running.
#[gpui::test]
fn escape_on_an_opened_subagent_stops_it(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("subagents-escape-tab", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    child(&fake, "Atlas", AgentStatus::Working);
    child(&fake, "Cedar", AgentStatus::Working);
    tick(cx);
    click_child(&view, cx, "Cedar");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(*fake.stopped.borrow(), [key("Cedar")]);
    assert_eq!(*fake.interrupts.borrow(), 0, "Main's turn runs on");
}

#[gpui::test]
fn native_child_progress_uses_the_shared_pinned_row_and_selected_wall_cache(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("subagents-native-progress", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new_thinking(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    child(&fake, "Atlas", AgentStatus::Working);
    emit(
        &fake,
        ActivityEvent::Content {
            key: key("Atlas"),
            id: Some("heading".into()),
            event: ExecutionEvent::ReasoningSummaryPart {
                item_id: "heading".into(),
                summary_index: 0,
                text: "**Checking child paths**".into(),
                snapshot: false,
            },
        },
    );
    tick(cx);
    click_child(&view, cx, "Atlas");
    assert!(cx.debug_bounds("live-thinking").is_some());
    emit(
        &fake,
        ActivityEvent::Content {
            key: key("Atlas"),
            id: None,
            event: ExecutionEvent::Progress {
                event: ferrite_core::progress::ProgressEvent::Phase {
                    phase: ferrite_core::progress::Phase::Retrying,
                    detail: "Server busy".into(),
                },
            },
        },
    );
    tick(cx);
    assert!(cx
        .debug_bounds("live-phase")
        .is_some());
    view.read_with(cx, |view, _| {
        let pane = &view.panes[0];
        let facts = view.facts.get(pane.thread().unwrap()).unwrap();
        assert!(facts
            .wall_for(&pane.selected)
            .unwrap()
            .working
            .contains("Retrying · Server busy"));
        assert!(!facts.wall.working.contains("Server busy"));
    });
    emit(
        &fake,
        ActivityEvent::Content {
            key: key("Atlas"),
            id: Some("end".into()),
            event: ExecutionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Interrupted,
                cost_usd: None,
            },
        },
    );
    tick(cx);
    assert!(cx
        .debug_bounds("live-phase")
        .is_none());
}

/// Escape stops what the Pane shows: a running Subagent alone, Main's turn
/// alone; on a Subagent with nothing to stop, it goes back to Main.
#[gpui::test]
fn escape_on_a_subagent_stops_only_that_subagent_or_goes_back(cx: &mut TestAppContext) {
    use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject};
    let (mut core, fake) = cockpit("escape-subagent", 1);
    let thread = core.threads()[0];
    core.send(thread, "keep working".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let running = AgentKey::new(Provider::Claude, "native", "running");
    let finished = AgentKey::new(Provider::Claude, "native", "finished");
    for (key, state) in [
        (&running, AgentStatus::Working),
        (&finished, AgentStatus::Idle),
    ] {
        let mut info = AgentInfo::new(key.clone());
        info.parent = Some(Subject::Main);
        let stream = &fake.streams.borrow()[0];
        stream
            .send(SessionEvent::Activity(ActivityEvent::Discovered(info)))
            .unwrap();
        stream
            .send(SessionEvent::Activity(ActivityEvent::Status {
                key: key.clone(),
                state,
            }))
            .unwrap();
    }
    tick(cx);
    let escape_on = |subject: Subject, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.select_subject(thread, subject, window, cx)
            })
        });
        tick(cx);
        cx.simulate_keystrokes("escape");
        tick(cx);
    };

    escape_on(Subject::Subagent(running.clone()), cx);
    assert_eq!(*fake.stopped.borrow(), std::slice::from_ref(&running));
    assert_eq!(*fake.interrupts.borrow(), 0, "Main's turn runs on");

    escape_on(Subject::Subagent(finished), cx);
    assert_eq!(
        *fake.stopped.borrow(),
        std::slice::from_ref(&running),
        "nothing to stop"
    );
    assert_eq!(
        *fake.interrupts.borrow(),
        0,
        "nor does it reach Main's turn"
    );
    assert!(
        view.read_with(cx, |view, _| view.panes[0].is_main()),
        "it goes back to Main"
    );

    escape_on(Subject::Main, cx);
    assert_eq!(*fake.interrupts.borrow(), 1);
    assert_eq!(*fake.stopped.borrow(), [running], "no Subagent is stopped");
}

/// A Main `Agent` call reads as Claude's row (`Ran agent <task>`), runs
/// with its agent — the line under the turn never names it a second time —
/// and a press on it opens that agent in the right pane (`agent_pane`).
#[gpui::test]
fn an_agent_row_runs_with_its_agent_and_opens_it(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("subagents-row", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    agent_call(&fake, "Atlas");
    child(&fake, "Atlas", AgentStatus::Working);
    emit(
        &fake,
        ActivityEvent::Content {
            key: key("Atlas"),
            id: None,
            event: ExecutionEvent::Progress {
                event: ferrite_core::progress::ProgressEvent::Phase {
                    phase: ferrite_core::progress::Phase::Working,
                    detail: "Reading nav.rs".into(),
                },
            },
        },
    );
    tick(cx);
    assert!(cx.debug_bounds("agent-running").is_some(), "its spinner");
    assert!(cx.debug_bounds("agent-row-Atlas").is_some());
    view.read_with(cx, |view, _| {
        let activity = view
            .cockpit
            .thread(view.panes[0].thread().unwrap())
            .unwrap()
            .activity();
        let faces = crate::cockpit::transcript_glue::agent_faces(activity);
        assert_eq!(
            faces.get("Atlas"),
            Some(&ferrite_pane::pane::AgentFace {
                run: ferrite_pane::pane::AgentRun::Running,
                summary: Some("Reading nav.rs".into()),
            })
        );
    });
    assert!(
        cx.debug_bounds("live-call").is_none(),
        "one running row"
    );
    let row = cx.debug_bounds("agent-open-Atlas").expect("the row opens");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.agent_drills.subject(thread),
            Some(&Subject::Subagent(key("Atlas")))
        );
        assert!(view.panes[0].is_main(), "the Pane keeps Main");
    });
    assert!(cx.debug_bounds("agent-drill").is_some());
    assert!(cx.debug_bounds("subagent-head-1").is_none());
    emit(
        &fake,
        ActivityEvent::Status {
            key: key("Atlas"),
            state: AgentStatus::Idle,
        },
    );
    tick(cx);
    let close = cx.debug_bounds("panel-tab-Agent-close").unwrap().center();
    cx.simulate_click(close, gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| !view.agent_drills.is_open(thread)));
    assert!(cx.debug_bounds("agent-running").is_none(), "settled");
}

/// The keyboard reaches an agent's row: Tab lands on it, ↵ opens its
/// agent, and ⌫ stops it while it runs — the stop-agent op Escape uses on
/// an opened agent. The shortcuts sheet lists it.
#[gpui::test]
fn the_keyboard_opens_and_stops_an_agents_row(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("subagents-row-keys", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    agent_call(&fake, "Atlas");
    child(&fake, "Atlas", AgentStatus::Working);
    tick(cx);
    cx.simulate_keystrokes("tab");
    tick(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.panes[0].targeted_tool().cloned()),
        Some(ferrite_pane::pane::DisclosureId::Tool("Atlas".into())),
        "the agent's row is a stop"
    );
    assert!(cx.debug_bounds("tool-disclosure-keyboard-target").is_some());
    cx.simulate_keystrokes("backspace");
    tick(cx);
    assert_eq!(*fake.stopped.borrow(), [key("Atlas")]);
    assert!(
        view.read_with(cx, |view, _| view.panes[0].is_main()),
        "stopping leaves the Pane on Main"
    );
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| {
            let thread = view.panes[0].thread().unwrap();
            view.agent_drills.subject(thread).cloned()
        }),
        Some(Subject::Subagent(key("Atlas"))),
        "↵ opens it"
    );
    assert_eq!(
        crate::shortcuts::label("cockpit::StopAgent", Some("ToolDisclosure")),
        Some(crate::shortcuts::Entry::Row(
            crate::shortcuts::Place::Transcript,
            "Stop the agent"
        ))
    );
}

/// An agent opened from its row is an `Agent` tab of the Panel at its
/// floor, the thread reflowing beside it; the Panel's maximize covers the
/// content area; the tab's × closes it.
#[gpui::test]
fn an_opened_agent_is_a_panel_tab(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("subagents-drill", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(800.)));
    agent_call(&fake, "Atlas");
    child(&fake, "Atlas", AgentStatus::Idle);
    tick(cx);
    let before = cx.debug_bounds("board").expect("the board").size.width;
    view.update(cx, |view, cx| view.open_agent_from(0, "Atlas".into(), cx));
    tick(cx);
    tick(cx);

    let pane = cx.debug_bounds("panel").expect("the pane docks");
    assert!(
        (pane.size.width - px(ferrite_ui::theme::SIDE_CHAT_W)).abs() < px(1.),
        "its token width: {pane:?}"
    );
    assert!((pane.right() - px(1600.)).abs() < px(1.), "at the right");
    let board = cx.debug_bounds("board").unwrap();
    assert!(
        board.size.width < before && board.right() <= pane.left() + px(1.),
        "the thread reflows beside it"
    );
    let tab = cx.debug_bounds("panel-tab-Agent").expect("its tab");
    assert!(pane.contains(&tab.center()));
    assert!(
        cx.debug_bounds("panel-toggle").is_some(),
        "the rail's door stands against the pane"
    );

    // The Panel's one maximize covers the content area, and restores.
    let maximize = cx.debug_bounds("panel-maximize").unwrap();
    cx.simulate_click(maximize.center(), gpui::Modifiers::none());
    tick(cx);
    let wide = cx.debug_bounds("panel").unwrap();
    assert!(wide.size.width > pane.size.width * 2., "maximized: {wide:?}");
    let maximize = cx.debug_bounds("panel-maximize").unwrap();
    cx.simulate_click(maximize.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        (cx.debug_bounds("panel").unwrap().size.width - pane.size.width).abs() < px(1.),
        "restored"
    );

    // Its tab's × closes it.
    let close = cx.debug_bounds("panel-tab-Agent-close").unwrap();
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    assert!(view.read_with(cx, |view, _| !view.agent_drills.is_open(thread)));
    assert!(cx.debug_bounds("agent-drill").is_none());
}
