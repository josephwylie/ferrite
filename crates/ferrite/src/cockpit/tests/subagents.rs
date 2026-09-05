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
fn emit(fake: &Fake, event: ActivityEvent) {
    fake.streams.borrow()[0]
        .send(SessionEvent::Activity(event))
        .unwrap();
}
fn bounds(cx: &mut gpui::VisualTestContext, id: String) -> gpui::Bounds<gpui::Pixels> {
    cx.debug_bounds(Box::leak(id.clone().into_boxed_str()))
        .unwrap_or_else(|| {
            panic!(
                "missing {id}, strip {:?}",
                cx.debug_bounds("subject-strip-1")
            )
        })
}
fn tab(cx: &mut gpui::VisualTestContext, name: &str) -> gpui::Point<gpui::Pixels> {
    bounds(cx, format!("subject-agent-1-{}", key(name).as_str())).center()
}
fn click_child(cx: &mut gpui::VisualTestContext, name: &str) {
    let at = tab(cx, name);
    cx.simulate_click(at, gpui::Modifiers::none());
    cx.run_until_parked();
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
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("keep my draft".into(), cx))
    });
    click_child(cx, "Atlas");
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
        assert!(transcript_text(subject.transcript().blocks()).contains("Atlas transcript"));
    });
    assert!(cx.debug_bounds("return-main-1").is_some());
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
    assert!(tab(cx, "Cedar").x < tab(cx, "Atlas").x);
    assert_eq!(
        view.read_with(cx, |view, _| view.panes[0].selected.clone()),
        Subject::Subagent(key("Atlas"))
    );
    // The observational view cannot submit or consume the Main draft.
    cx.simulate_keystrokes("enter");
    assert!(fake.sent.borrow().is_empty());
    let main = cx.debug_bounds("subject-main-1").unwrap().center();
    cx.simulate_click(main, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert!(view.panes[0].is_main());
        assert_eq!(view.panes[0].composer.read(cx).text(), "keep my draft");
    });
}

#[gpui::test]
fn a_status_reorder_between_pointer_down_and_up_cannot_select_the_new_occupant(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("subagents-pointer", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    child(&fake, "Atlas", AgentStatus::Idle);
    child(&fake, "Cedar", AgentStatus::Idle);
    tick(cx);
    let pressed = tab(cx, "Atlas");
    cx.simulate_mouse_down(pressed, MouseButton::Left, gpui::Modifiers::none());
    emit(
        &fake,
        ActivityEvent::Status {
            key: key("Cedar"),
            state: AgentStatus::Working,
        },
    );
    tick(cx);
    cx.simulate_mouse_up(pressed, MouseButton::Left, gpui::Modifiers::none());
    assert!(view.read_with(cx, |view, _| view.panes[0].is_main()));
}

#[gpui::test]
fn child_scroll_disclosure_and_native_text_entity_survive_switching(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("subagents-view-state", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    child(&fake, "Atlas", AgentStatus::Idle);
    child(&fake, "Cedar", AgentStatus::Idle);
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
    click_child(cx, "Atlas");
    let toggle = view.read_with(cx, |view, _| {
        view.panes[0]
            .tool_bounds("same-call")
            .expect("tool control")
            .center()
    });
    cx.simulate_mouse_down(toggle, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(toggle, MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    let scroller = view.read_with(cx, |view, _| view.panes[0].scroll.bounds().center());
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: scroller,
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(240.))),
        ..Default::default()
    });
    cx.run_until_parked();
    let offset = view.read_with(cx, |view, _| {
        assert!(view.panes[0].tool_expanded("same-call"));
        assert!(!view.panes[0].follow_tail.get());
        view.panes[0].scroll.offset()
    });
    let namespace = view.read_with(cx, |view, _| view.panes[0].text_namespace());
    let entity = cx.update(|_, cx| {
        crate::rich::testing::first_entity(&format!("markdown-{namespace}-"), cx).unwrap()
    });
    click_child(cx, "Cedar");
    assert!(!view.read_with(cx, |view, _| view.panes[0].tool_expanded("same-call")));
    click_child(cx, "Atlas");
    view.read_with(cx, |view, _| {
        assert!(view.panes[0].tool_expanded("same-call"));
        assert_eq!(view.panes[0].scroll.offset(), offset);
        assert!(!view.panes[0].follow_tail.get());
    });
    assert_eq!(
        cx.update(|_, cx| crate::rich::testing::first_entity(
            &format!("markdown-{namespace}-"),
            cx
        )
        .unwrap()),
        entity
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
    let attention = cx.debug_bounds("agent-attention-1").unwrap().center();
    cx.simulate_click(attention, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.panes[0].selected.clone()),
        Subject::Subagent(key("Atlas"))
    );
    let serial = view.read_with(cx, |view, _| {
        view.cockpit
            .thread(view.panes[0].thread().unwrap())
            .unwrap()
            .activity()
            .pending_decisions()[0]
            .handle
            .serial
    });
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
            .activity()
            .pending_decisions();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].decision.id, "Cedar");
    });
}

#[gpui::test]
fn keyboard_focus_and_a_held_space_press_follow_the_agent_across_status_reorder(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("subagents-keyboard", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    child(&fake, "Atlas", AgentStatus::Idle);
    child(&fake, "Cedar", AgentStatus::Idle);
    tick(cx);
    cx.simulate_keystrokes("tab tab");
    let keystroke = gpui::Keystroke::parse("space").unwrap();
    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    emit(
        &fake,
        ActivityEvent::Status {
            key: key("Cedar"),
            state: AgentStatus::Working,
        },
    );
    tick(cx);
    cx.simulate_event(gpui::KeyUpEvent { keystroke });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.panes[0].selected.clone()),
        Subject::Subagent(key("Atlas"))
    );
    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |view, _| view.panes[0].selected.clone()),
        Subject::Subagent(key("Cedar"))
    );
    cx.simulate_keystrokes("home");
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.panes[0].is_main()));
}

#[gpui::test]
fn native_selection_clears_on_subject_navigation_without_leaking_into_another_agent(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("subagents-selection", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    child(&fake, "Atlas", AgentStatus::Idle);
    child(&fake, "Cedar", AgentStatus::Idle);
    tick(cx);
    click_child(cx, "Atlas");
    let from = caret(&view, cx, 0, 0);
    let to = caret(&view, cx, 0, 5);
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(clipboard(cx).as_deref(), Some("Atlas"));
    click_child(cx, "Cedar");
    assert!(!cx.update(|window, cx| gpui::base::TextSelection::has_selection(window, cx)));
    click_child(cx, "Atlas");
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
fn native_tab_overflow_follows_measured_header_width_and_keeps_discovery_order(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("subagents-responsive-tabs", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(800.)));
    for name in ["Atlas", "Cedar", "Finch", "Juniper", "Rowan"] {
        child(&fake, name, AgentStatus::Idle);
    }
    tick(cx);
    for name in ["Atlas", "Cedar", "Finch", "Juniper", "Rowan"] {
        let _ = tab(cx, name);
    }
    assert!(
        cx.debug_bounds("subject-overflow-1").is_none(),
        "all five native tabs fit a wide pane"
    );
    click_child(cx, "Rowan");
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    cx.simulate_resize(gpui::size(px(640.), px(800.)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("subject-main-1").is_some());
    assert!(
        cx.debug_bounds("subject-overflow-1").is_some(),
        "narrow header exposes overflow"
    );
    assert!(
        cx.debug_bounds(Box::leak(
            format!("subject-agent-1-{}", key("Rowan").as_str()).into_boxed_str()
        ))
        .is_none(),
        "selected hidden agent is not promoted ahead of discovery order"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.panes[0].selected.clone()),
        Subject::Subagent(key("Rowan"))
    );
    emit(
        &fake,
        ActivityEvent::Status {
            key: key("Rowan"),
            state: AgentStatus::Working,
        },
    );
    tick(cx);
    assert!(tab(cx, "Rowan").x > cx.debug_bounds("subject-main-1").unwrap().center().x);
    cx.simulate_resize(gpui::size(px(1600.), px(800.)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("subject-overflow-1").is_none());
    assert!(tab(cx, "Rowan").x < tab(cx, "Atlas").x);
}

#[gpui::test]
fn native_tab_press_cannot_jump_to_an_overflow_slot_after_resize(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("subagents-resize-press", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(800.)));
    for name in ["Atlas", "Cedar", "Finch", "Juniper", "Rowan"] {
        child(&fake, name, AgentStatus::Idle);
    }
    tick(cx);
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    let pressed = tab(cx, "Rowan");
    cx.simulate_mouse_down(pressed, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_resize(gpui::size(px(640.), px(800.)));
    cx.run_until_parked();
    let main = cx.debug_bounds("subject-main-1").unwrap().center();
    cx.simulate_mouse_up(main, MouseButton::Left, gpui::Modifiers::none());
    assert!(view.read_with(cx, |view, _| view.panes[0].is_main()));
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
    let DecisionAnswer::Allow { input } = &answered[0].1 else {
        panic!("freeform answer");
    };
    assert_eq!(input["answers"]["Which approach?"], "neither, wait 2 days");
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
        let attention = cx.debug_bounds("agent-attention-1").unwrap().center();
        cx.simulate_click(attention, gpui::Modifiers::none());
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

#[gpui::test]
fn an_open_overflow_menu_resolves_a_subject_alias_before_selecting_its_request(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("subagents-stale-menu-alias", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(640.), px(800.)));
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    for name in ["Atlas", "Cedar", "Finch", "Juniper", "Rowan"] {
        child(&fake, name, AgentStatus::Idle);
    }
    let SessionEvent::DecisionRequested { decision } = decision("aliased-request") else {
        unreachable!()
    };
    emit(
        &fake,
        ActivityEvent::Decision {
            subject: Some(Subject::Subagent(key("Rowan"))),
            decision,
        },
    );
    tick(cx);
    let overflow = cx.debug_bounds("subject-overflow-1").unwrap().center();
    cx.simulate_click(overflow, gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(view.read_with(cx, |view, _| view.panes[0].agent_menu_open));
    emit(
        &fake,
        ActivityEvent::Alias {
            from: key("Rowan"),
            to: key("RowanCanonical"),
        },
    );
    tick(cx);
    // Remove hover before using the menu's native wrapping navigation.
    cx.simulate_mouse_move(
        gpui::point(px(10.), px(780.)),
        None,
        gpui::Modifiers::none(),
    );
    cx.run_until_parked();
    cx.simulate_keystrokes("up enter");
    cx.run_until_parked();
    let serial = view.read_with(cx, |view, _| {
        assert_eq!(
            view.panes[0].selected,
            Subject::Subagent(key("RowanCanonical"))
        );
        view.cockpit
            .thread(view.panes[0].thread().unwrap())
            .unwrap()
            .activity()
            .pending_decisions()[0]
            .handle
            .serial
    });
    assert!(cx
        .debug_bounds(Box::leak(
            format!("request-allow-1-{serial}").into_boxed_str()
        ))
        .is_some());
}

#[gpui::test]
fn native_child_progress_uses_the_shared_pinned_row_and_selected_wall_cache(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("subagents-native-progress", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
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
    click_child(cx, "Atlas");
    assert!(cx
        .debug_bounds("progress-caption-Checking child paths")
        .is_some());
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
        .debug_bounds("progress-caption-Retrying · Server busy")
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
        .debug_bounds("progress-caption-Retrying · Server busy")
        .is_none());
}
