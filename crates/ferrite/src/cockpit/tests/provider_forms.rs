use super::*;
use ferrite_core::{DecisionKind, DecisionPolicy, FormField, FormFieldKind};

fn typed_decision(kind: DecisionKind) -> SessionEvent {
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind,
            policy: DecisionPolicy::default(),
            id: "native-form".into(),
            tool_use_id: "call".into(),
            tool_name: "Provider request".into(),
            description: "Choose an approach".into(),
            input: serde_json::Value::Null,
            suggestions: vec![],
        },
    }
}

#[gpui::test]
fn contract_typed_question_renders_and_returns_normalized_picks(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("typed-question-contract", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(800.)));
    let q = ferrite_core::questions::Question {
        id: Some("q".into()),
        question: "Choose an approach".into(),
        header: "Approach".into(),
        multi_select: false,
        secret: false,
        allow_other: false,
        options: vec![
            ferrite_core::questions::Choice {
                label: "Patch".into(),
                description: "Small change".into(),
                preview: None,
            },
            ferrite_core::questions::Choice {
                label: "Rewrite".into(),
                description: "Start over".into(),
                preview: None,
            },
        ],
    };
    fake.streams.borrow()[0]
        .send(typed_decision(DecisionKind::Questions(vec![q])))
        .unwrap();
    tick(cx);
    let serial = view.read_with(cx, |v, _| {
        v.cockpit
            .thread(v.panes[0].thread().unwrap())
            .unwrap()
            .activity()
            .pending_decisions()[0]
            .handle
            .serial
    });
    assert!(
        debug_bounds(cx, format!("request-other-1-{serial}-0")).is_none(),
        "provider disallows custom text"
    );
    let choice = cx
        .debug_bounds("question-choice-0-0")
        .expect("typed kind must drive UI even with unknown tool name and null input");
    cx.simulate_click(choice.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let submit = debug_bounds(cx, format!("request-submit-1-{serial}")).unwrap();
    cx.simulate_click(submit.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let answers = fake.answered.borrow();
    assert!(
        matches!(&answers[0].1, DecisionAnswer::Questions{answers} if answers[0].picks==[0] && answers[0].other.is_none()),
        "shared UI must send normalized answers, never provider response JSON"
    );
}

/// Regression: the live Claude question that crashed Ferrite twice on Windows.
/// Keep the real labels and descriptions here: the contract is that a provider
/// can present this ordinary two-choice form without overflowing the UI thread.
#[gpui::test]
fn contract_live_claude_base_branch_question_renders(cx: &mut TestAppContext) {
    // Rust's test workers normally have twice the stack reserved for Ferrite's
    // Windows GUI main thread, which hid this crash from the existing form
    // contracts. Re-enter this one test in a production-sized worker so the
    // regression command exercises the same constraint as ferrite.exe.
    #[cfg(windows)]
    if std::env::var_os("FERRITE_QUESTION_STACK_CHILD").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg(
                "cockpit::tests::provider_forms::contract_live_claude_base_branch_question_renders",
            )
            .args(["--exact", "--nocapture"])
            .env("RUST_MIN_STACK", "1048576")
            .env("FERRITE_QUESTION_STACK_CHILD", "1")
            .status()
            .expect("start the production-stack question probe");
        assert!(
            status.success(),
            "the native question overflowed the production-sized UI stack"
        );
        return;
    }

    let (core, fake) = cockpit("claude-base-branch-question", 1);
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(800.)));

    let question = ferrite_core::questions::Question {
        id: None,
        question: "What should the new threads-menu branch be based on?".into(),
        header: "Base branch".into(),
        multi_select: false,
        secret: false,
        allow_other: true,
        options: vec![
            ferrite_core::questions::Choice {
                label: "feature/filtering-menu-rework (Recommended)".into(),
                description: "Branch off the filtering rework so the threads menu can reuse the shared `panel-styles.ts` (SIDEBAR_PANEL/PANEL_SUBHEAD) it introduced. Downside: this PR depends on that one merging first.".into(),
                preview: None,
            },
            ferrite_core::questions::Choice {
                label: "master".into(),
                description: "Branch off master and copy the panel style constants locally (as metrics-menu currently does on master). Independent PR, but duplicates the shared constants until the filtering branch lands.".into(),
                preview: None,
            },
        ],
    };
    fake.streams.borrow()[0]
        .send(typed_decision(DecisionKind::Questions(vec![question])))
        .unwrap();
    tick(cx);

    assert!(
        cx.debug_bounds("question-choice-0-0").is_some(),
        "the first provider choice is visible"
    );
    assert!(
        cx.debug_bounds("question-choice-0-1").is_some(),
        "the second provider choice is visible"
    );
}

#[gpui::test]
fn contract_mcp_form_defaults_submit_typed_values(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("mcp-form-contract", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(800.)));
    fake.streams.borrow()[0]
        .send(typed_decision(DecisionKind::Form {
            fields: vec![
                FormField {
                    id: "count".into(),
                    label: "Count".into(),
                    description: String::new(),
                    required: true,
                    kind: FormFieldKind::Integer {
                        minimum: Some(1),
                        maximum: Some(3),
                        default: Some(2),
                    },
                },
                FormField {
                    id: "enabled".into(),
                    label: "Enabled".into(),
                    description: String::new(),
                    required: false,
                    kind: FormFieldKind::Boolean {
                        default: Some(true),
                    },
                },
            ],
        }))
        .unwrap();
    tick(cx);
    let serial = view.read_with(cx, |v, _| {
        v.cockpit
            .thread(v.panes[0].thread().unwrap())
            .unwrap()
            .activity()
            .pending_decisions()[0]
            .handle
            .serial
    });
    let submit = debug_bounds(cx, format!("request-submit-1-{serial}"))
        .expect("MCP uses shared form submit");
    cx.simulate_click(submit.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(
        matches!(&fake.answered.borrow()[0].1,DecisionAnswer::Form{values} if *values==serde_json::json!({"count":2,"enabled":true}))
    );
}

#[gpui::test]
fn contract_denied_allow_policy_disables_mouse_and_keyboard_approval(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("deny-only-contract", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(800.)));
    let SessionEvent::DecisionRequested { mut decision } = typed_decision(DecisionKind::Approval)
    else {
        unreachable!()
    };
    decision.policy.allow = false;
    fake.streams.borrow()[0]
        .send(SessionEvent::DecisionRequested { decision })
        .unwrap();
    tick(cx);
    view.update_in(cx, |v, window, cx| {
        let focus = v.panes[0].decision_focus.clone();
        window.focus(&focus, cx);
    });
    cx.simulate_keystrokes("y");
    cx.run_until_parked();
    assert!(
        fake.answered.borrow().is_empty(),
        "keyboard must respect the native restriction"
    );
}

#[gpui::test]
fn contract_mcp_required_input_without_default_is_editable_and_validated(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("mcp-edit-contract", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(800.)));
    fake.streams.borrow()[0]
        .send(typed_decision(DecisionKind::Form {
            fields: vec![FormField {
                id: "count".into(),
                label: "Count".into(),
                description: "Enter 1 to 3".into(),
                required: true,
                kind: FormFieldKind::Integer {
                    minimum: Some(1),
                    maximum: Some(3),
                    default: None,
                },
            }],
        }))
        .unwrap();
    tick(cx);
    let serial = view.read_with(cx, |v, _| {
        v.cockpit
            .thread(v.panes[0].thread().unwrap())
            .unwrap()
            .activity()
            .pending_decisions()[0]
            .handle
            .serial
    });
    let field = cx
        .debug_bounds("form-field-count")
        .expect("fields without defaults still need an editable input");
    cx.simulate_click(field.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    cx.simulate_input("9");
    cx.run_until_parked();
    let submit = debug_bounds(cx, format!("request-submit-1-{serial}")).unwrap();
    cx.simulate_click(submit.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(fake.answered.borrow().is_empty());
    assert!(
        cx.debug_bounds("form-validation-error").is_some(),
        "validation failure must be visible, not merely retained in hidden state"
    );
    let cancel = cx
        .debug_bounds("form-cancel")
        .expect("operator can cancel any MCP form");
    cx.simulate_click(cancel.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(matches!(
        &fake.answered.borrow()[0].1,
        DecisionAnswer::Cancel
    ));
}

#[gpui::test]
fn contract_every_native_approval_choice_is_selectable(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("approval-choice-ui", 1);
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1100.), px(800.)));
    let SessionEvent::DecisionRequested { mut decision } = typed_decision(DecisionKind::Approval)
    else {
        unreachable!()
    };
    decision.policy.allow = false;
    decision.suggestions = vec![
        ferrite_core::DecisionChoice {
            label: "Allow this session".into(),
            value: serde_json::json!({"opaque":"first"}),
            standing: true,
        },
        ferrite_core::DecisionChoice {
            label: "Block example.com".into(),
            value: serde_json::json!({"opaque":"second"}),
            standing: false,
        },
    ];
    fake.streams.borrow()[0]
        .send(SessionEvent::DecisionRequested { decision })
        .unwrap();
    tick(cx);
    let choice = cx
        .debug_bounds("approval-choice-1")
        .expect("all native choices need a shared labeled button");
    cx.simulate_click(choice.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert!(
        matches!(&fake.answered.borrow()[0].1,DecisionAnswer::Choose{value} if *value==serde_json::json!({"opaque":"second"})),
        "opaque native choices survive UI unchanged even when plain Allow is forbidden"
    );
}
