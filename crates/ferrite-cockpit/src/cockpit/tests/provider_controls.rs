use super::*;

#[gpui::test]
fn contract_context_card_refreshes_and_shows_native_details(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("native-context-ui", 1);
    *fake.native_controls.borrow_mut() = true;
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(800.)));
    for event in [
        SessionEvent::TokenUsage {
            total_tokens: 12000,
            input_tokens: 10000,
            cached_input_tokens: 1000,
            output_tokens: 2000,
            reasoning_output_tokens: 500,
            context_window: Some(180000),
        },
        SessionEvent::ContextDetails {
            details: ferrite_core::ContextDetails {
                is_auto_compact_enabled: Some(true),
                categories: vec![
                    ferrite_core::ContextCategory {
                        name: "Messages".into(),
                        tokens: 12000,
                    },
                    ferrite_core::ContextCategory {
                        name: "MCP tools (deferred)".into(),
                        tokens: 9000,
                    },
                ],
                mcp_tools: vec![ferrite_core::ContextItem {
                    name: "mcp__docs__search".into(),
                    tokens: 202,
                }],
                ..Default::default()
            },
        },
    ] {
        fake.streams.borrow()[0].send(event).unwrap();
    }
    tick(cx);
    // Opening the panel asks the Session for a fresh reading.
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    view.update(cx, |view, cx| view.open_panel(thread, cx));
    cx.run_until_parked();
    assert_eq!(
        fake.controls.borrow().as_slice(),
        [ferrite_core::SessionControl::RefreshContext]
    );
    // The legend is open, as Claude's: the window's categories, then the
    // deferred ones apart, then the lists, folded.
    let held = cx
        .debug_bounds("context-category-0-12000")
        .expect("the legend is always open");
    let deferred = cx.debug_bounds("context-category-1-9000").unwrap();
    let list = cx.debug_bounds("context-list-mcp-tools").unwrap();
    assert!(
        deferred.top() - held.bottom() >= px(ferrite_ui::theme::USAGE_LEGEND_GROUP_GAP),
        "deferred tools stand apart"
    );
    assert!(list.top() > deferred.bottom(), "the lists come last");
    assert!(cx.debug_bounds("context-list-mcp-tools-0").is_none());
    cx.simulate_click(list.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("context-list-mcp-tools-0").is_some(),
        "a list folds open to its items"
    );
    cx.run_until_parked();
    assert_eq!(
        fake.controls.borrow().len(),
        1,
        "drawing the open panel again must not request another refresh"
    );
}

#[gpui::test]
fn contract_session_controls_use_shared_native_handles(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("native-session-ui", 1);
    bind_production_keys(cx);
    *fake.native_controls.borrow_mut() = true;
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(800.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::McpServers {
            servers: vec![ferrite_core::McpServer {
                name: "search".into(),
                status: ferrite_core::McpStatus::NeedsAuth,
                error: Some("Sign in to search".into()),
            }],
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::Progress {
            event: ferrite_core::progress::ProgressEvent::BackgroundSnapshot {
                tasks: vec![ferrite_core::progress::BackgroundTask {
                    id: "task:1".into(),
                    label: "Background check".into(),
                    detail: "shell".into(),
                    status: ferrite_core::progress::TaskStatus::Working,
                }],
            },
        })
        .unwrap();
    tick(cx);
    // The session controls are the palette's (FL-4): `refresh MCP` runs the
    // native refresh, `background tasks` opens the card.
    assert!(
        cx.debug_bounds("session-controls-1").is_none(),
        "the status line has no `•••`"
    );
    cx.simulate_keystrokes("cmd-k");
    cx.run_until_parked();
    cx.simulate_input("refresh");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(fake
        .controls
        .borrow()
        .contains(&ferrite_core::SessionControl::RefreshMcp));
    cx.simulate_keystrokes("cmd-k");
    cx.run_until_parked();
    // `background` alone also finds the Thread whose word it is now.
    cx.simulate_input("background tasks");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    tick(cx);
    let reconnect = cx.debug_bounds("mcp-reconnect-0").unwrap();
    cx.simulate_click(reconnect.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(fake
        .controls
        .borrow()
        .contains(&ferrite_core::SessionControl::ReconnectMcp {
            server: "search".into()
        }));
    let stop = cx.debug_bounds("background-stop-0").unwrap();
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(fake
        .controls
        .borrow()
        .contains(&ferrite_core::SessionControl::StopTask {
            id: "task:1".into()
        }));
    assert!(
        fake.sent.borrow().is_empty(),
        "controls cannot become chat prompts"
    );
    assert!(
        cx.debug_bounds("mcp-status-0-needs-auth").is_some(),
        "native status must remain visible beside the server identity"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("mcp-reconnect-0").is_none(),
        "Escape closes controls rather than interrupting Main"
    );
    assert_eq!(*fake.interrupts.borrow(), 0);
}

#[gpui::test]
fn contract_permission_and_mcp_auth_controls_are_native(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("native-auth-ui", 1);
    bind_production_keys(cx);
    *fake.native_controls.borrow_mut() = true;
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(1000.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::McpServers {
            servers: vec![ferrite_core::McpServer {
                name: "search".into(),
                status: ferrite_core::McpStatus::NeedsAuth,
                error: None,
            }],
        })
        .unwrap();
    tick(cx);
    // The card opens from the palette's `background tasks` (FL-4).
    cx.simulate_keystrokes("cmd-k");
    cx.run_until_parked();
    cx.simulate_input("background");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    tick(cx);
    let mode = cx
        .debug_bounds("permission-mode-0")
        .expect("provider-supplied permission mode exposed");
    cx.simulate_click(mode.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(fake
        .controls
        .borrow()
        .contains(&ferrite_core::SessionControl::SetPermissionMode {
            mode: "native-mode".into()
        }));
    let login = cx
        .debug_bounds("mcp-login-0")
        .expect("native sign-in action");
    cx.simulate_click(login.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(fake
        .controls
        .borrow()
        .contains(&ferrite_core::SessionControl::LoginMcp {
            server: "search".into()
        }));
    fake.streams.borrow()[0]
        .send(SessionEvent::McpAuthorization {
            server: "search".into(),
            url: Some("https://example.com/authorize".into()),
        })
        .unwrap();
    tick(cx);
    assert!(
        cx.debug_bounds("mcp-authorize-0").is_some(),
        "authorization link requires an explicit user click"
    );
    let reload = cx.debug_bounds("mcp-reload").unwrap();
    cx.simulate_click(reload.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(fake
        .controls
        .borrow()
        .contains(&ferrite_core::SessionControl::ReloadMcp));
    assert!(fake.sent.borrow().is_empty());
    fake.streams.borrow()[0]
        .send(SessionEvent::McpAuthorization {
            server: "search".into(),
            url: None,
        })
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("mcp-authorize-0").is_none());
}

#[gpui::test]
fn contract_native_accounting_is_visible(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("native-usage-details-ui", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(1000.)));
    for event in [
        SessionEvent::ContextUsage {
            total_tokens: 12000,
            context_window: Some(180000),
        },
        SessionEvent::UsageDetails {
            details: ferrite_core::UsageDetails {
                scope: ferrite_core::UsageScope::Turn,
                input_tokens: 100,
                cached_input_tokens: 20,
                output_tokens: 30,
                reasoning_output_tokens: 10,
            },
        },
        SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: Some(0.04),
        },
    ] {
        fake.streams.borrow()[0].send(event).unwrap();
    }
    tick(cx);
    cx.run_until_parked();
    for selector in [
        "usage-scope-turn",
        "usage-input-100",
        "usage-cached-input-20",
        "usage-output-30",
        "usage-reasoning-output-10",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "missing native accounting field: {selector}"
        );
    }
    assert!(
        cx.debug_bounds("usage-cost-0.04").is_none(),
        "the cost is kept, not shown"
    );
}

#[gpui::test]
fn contract_native_turn_diff_is_kept_without_a_fake_tool(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("native-turn-diff-ui", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(900.)));
    fake.streams.borrow()[0].send(SessionEvent::TurnDiff{turn_id:"native-turn".into(),diff:"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old-native\n+new-native\n".into()}).unwrap();
    tick(cx);
    view.read_with(cx, |v, _| {
        let thread = v.panes[0].thread().unwrap();
        let open = v.cockpit.thread(thread).unwrap();
        // The transcript keeps the turn's change set and draws no summary
        // row for it.
        assert_eq!(
            open.transcript()
                .turn_diff()
                .map(|diff| diff.turn_id.as_str()),
            Some("native-turn")
        );
        assert!(
            !open
                .transcript()
                .blocks()
                .iter()
                .any(|b| matches!(b.body, ferrite_core::transcript::Body::Tool(_))),
            "turn diff is not a fabricated tool call"
        );
    });
}

#[gpui::test]
fn the_composer_mode_chip_opens_a_native_mode_menu_while_idle(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("native-mode-chip", 1);
    bind_production_keys(cx);
    *fake.native_controls.borrow_mut() = true;
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(800.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0]
        .send(SessionEvent::PermissionMode {
            mode: "native-mode".into(),
        })
        .unwrap();
    tick(cx);
    let chip = debug_bounds(cx, format!("mode-picker-{}", thread.get()))
        .expect("the mode chip rides an idle Pane, not only a streaming one");
    cx.simulate_click(chip.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.mode_picker.is_some(), "a click opens the mode menu");
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.mode_picker.is_some(), "the menu survives a redraw");
    });
    // Anchored BottomLeft, the menu rests above its chip (SPEC §5.5),
    // clear of it; its one item is the pick.
    // Row 0 is the menu's `Mode` heading.
    let item = cx
        .debug_bounds("choice-row-1")
        .expect("the mode menu's item is drawn");
    assert!(item.bottom() <= chip.top(), "{item:?} over {chip:?}");
    cx.simulate_click(item.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(
        fake.controls
            .borrow()
            .contains(&ferrite_core::SessionControl::SetPermissionMode {
                mode: "native-mode".into()
            }),
        "a pick is the native control: {:?}",
        fake.controls.borrow()
    );
}

/// Background work (HD64, Claude's `bg-tasks-panel.png`): no shelf over
/// the Composer. A call launched into the background runs on past its
/// turn, its row live (the running mark and its clock); the rail lists it
/// with a stop square, and opens the Background tasks pane. Handed back,
/// the row settles and the pane counts it finished.
#[gpui::test]
fn background_work_lives_in_its_row_the_rail_and_the_pane(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("background-work", 1);
    *fake.native_controls.borrow_mut() = true;
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    let send = |event: SessionEvent| fake.streams.borrow()[0].send(event).unwrap();
    send(SessionEvent::ToolStarted {
        id: "call".into(),
        name: "Bash".into(),
        input: serde_json::json!({"command": "sleep 20 && echo done", "description": "Sleep 20s then print done"}),
    });
    send(SessionEvent::ToolCompleted {
        id: "call".into(),
        output: "Command running in background with ID: b1.".into(),
        is_error: false,
        result: ferrite_core::ToolResult::Background { task: "b1".into() },
    });
    send(SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    });
    tick(cx);
    view.update(cx, |view, cx| {
        let thread = view.focused_thread().unwrap();
        view.open_panel(thread, cx);
    });
    tick(cx);
    assert!(
        cx.debug_bounds("composer-shelf").is_none(),
        "no pill over the Composer"
    );
    assert!(
        cx.debug_bounds("tool-background-live").is_some(),
        "the launching row is live after its turn ended"
    );
    assert!(cx.debug_bounds("panel-background-head").is_some());
    let stop = cx
        .debug_bounds("background-stop-call")
        .expect("the rail's row has a stop square");
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(fake
        .controls
        .borrow()
        .contains(&ferrite_core::SessionControl::StopTask { id: "b1".into() }));

    // The row's `View live output ▸` opens the pane, its card open on the
    // command's output.
    let live = cx.debug_bounds("tool-background-live").unwrap();
    cx.simulate_mouse_down(live.center(), gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(live.center(), gpui::MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("background-pane").is_some());
    assert!(cx.debug_bounds("background-running-0").is_some());
    assert!(cx.debug_bounds("background-output").is_some());
    // Its head closes it.
    let head = cx.debug_bounds("background-running-0-head").unwrap();
    cx.simulate_click(head.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("background-output").is_none());

    send(SessionEvent::ToolCompleted {
        id: "call".into(),
        output: "done\n".into(),
        is_error: false,
        result: ferrite_core::ToolResult::Command {
            stdout: "done\n".into(),
            stderr: String::new(),
            exit_code: Some(0),
            duration_ms: None,
        },
    });
    tick(cx);
    assert!(cx.debug_bounds("background-running-0").is_none());
    assert!(cx.debug_bounds("background-finished-0").is_some());
    assert!(
        cx.debug_bounds("tool-background-live").is_none(),
        "handed back, the row settles to plain text"
    );
    let close = cx.debug_bounds("background-pane-close").unwrap();
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("background-pane").is_none());
    assert!(cx.debug_bounds("panel-background-finished").is_some());
}

/// A turn's wrap-up (HD58) draws as its box, and a next step's press sends
/// its words as the next prompt.
#[gpui::test]
fn a_wrap_up_step_sends_its_words(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("wrap-up-step", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    let send = |event: SessionEvent| fake.streams.borrow()[0].send(event).unwrap();
    send(SessionEvent::ToolStarted {
        id: "w".into(),
        name: ferrite_core::wrap_up::CLAUDE_NAME.into(),
        input: serde_json::json!({
            "recap": "Fixed the parser.",
            "next": ["Commit these changes", "Run the full suite"],
        }),
    });
    send(SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    });
    tick(cx);
    assert!(cx.debug_bounds("wrap-up").is_some());
    let step = cx
        .debug_bounds("wrap-up-step-w-0")
        .expect("the recommended step is a button");
    cx.simulate_click(step.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(fake.sent.borrow().as_slice(), ["Commit these changes"]);
}
