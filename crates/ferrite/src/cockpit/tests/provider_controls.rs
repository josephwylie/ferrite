use super::*;

#[gpui::test]
fn contract_context_card_refreshes_and_shows_native_details(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("native-context-ui", 1);
    *fake.native_controls.borrow_mut() = true;
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(800.)));
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
                usable_window: Some(150000),
                auto_compact_threshold: Some(140000),
                is_auto_compact_enabled: Some(true),
                categories: vec![ferrite_core::ContextCategory {
                    name: "Messages".into(),
                    tokens: 12000,
                }],
            },
        },
    ] {
        fake.streams.borrow()[0].send(event).unwrap();
    }
    tick(cx);
    let meter = cx.debug_bounds("usage-meter-1").unwrap();
    cx.simulate_mouse_down(meter.center(), MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        fake.controls.borrow().as_slice(),
        [ferrite_core::SessionControl::RefreshContext]
    );
    assert!(cx.debug_bounds("context-usable-150000").is_some());
    assert!(cx.debug_bounds("context-compaction-140000").is_some());
    assert!(cx.debug_bounds("context-category-0-12000").is_some());
    cx.simulate_mouse_down(meter.center(), MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        fake.controls.borrow().len(),
        1,
        "closing the card must not request another refresh"
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
    let controls = cx
        .debug_bounds("session-controls-1")
        .expect("capable Sessions expose a shared controls menu");
    cx.simulate_click(controls.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(fake
        .controls
        .borrow()
        .contains(&ferrite_core::SessionControl::RefreshMcp));
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
    let controls = cx.debug_bounds("session-controls-1").unwrap();
    cx.simulate_click(controls.center(), gpui::Modifiers::none());
    cx.run_until_parked();
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
fn contract_native_accounting_and_cost_are_visible(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("native-usage-details-ui", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(1000.)));
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
    let meter = cx.debug_bounds("usage-meter-1").unwrap();
    cx.simulate_mouse_down(meter.center(), MouseButton::Left, gpui::Modifiers::none());
    cx.run_until_parked();
    for selector in [
        "usage-scope-turn",
        "usage-input-100",
        "usage-cached-input-20",
        "usage-output-30",
        "usage-reasoning-output-10",
        "usage-cost-0.04",
    ] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "missing native accounting field: {selector}"
        );
    }
}

#[gpui::test]
fn contract_native_turn_diff_is_disclosed_without_a_fake_tool(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("native-turn-diff-ui", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(900.)));
    fake.streams.borrow()[0].send(SessionEvent::TurnDiff{turn_id:"native-turn".into(),diff:"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-old-native\n+new-native\n".into()}).unwrap();
    tick(cx);
    let show = cx
        .debug_bounds("turn-diff-disclosure")
        .expect("aggregate changes have their own shared disclosure");
    cx.simulate_click(show.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |v, _| {
        let thread = v.panes[0].thread().unwrap();
        let runs = v.selection.registered(thread);
        assert!(
            runs.iter()
                .any(|(_, _, _, text)| text.contains("+new-native")),
            "native diff must be visible and copyable"
        );
        assert!(
            !v.cockpit
                .thread(thread)
                .unwrap()
                .transcript()
                .blocks()
                .iter()
                .any(|b| matches!(b.body, ferrite_core::transcript::Body::Tool(_))),
            "turn diff is not a fabricated tool call"
        );
    });
}
