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
        cx.debug_bounds(Box::leak(
            format!("request-other-1-{serial}-0").into_boxed_str()
        ))
        .is_none(),
        "provider disallows custom text"
    );
    let choice = cx
        .debug_bounds("question-choice-0-0")
        .expect("typed kind must drive UI even with unknown tool name and null input");
    cx.simulate_click(choice.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let submit = cx
        .debug_bounds(Box::leak(
            format!("request-submit-1-{serial}").into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_click(submit.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let answers = fake.answered.borrow();
    assert!(
        matches!(&answers[0].1, DecisionAnswer::Questions{answers} if answers[0].picks==[0] && answers[0].other.is_none()),
        "shared UI must send normalized answers, never provider response JSON"
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
    let submit = cx
        .debug_bounds(Box::leak(
            format!("request-submit-1-{serial}").into_boxed_str(),
        ))
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
    let submit = cx
        .debug_bounds(Box::leak(
            format!("request-submit-1-{serial}").into_boxed_str(),
        ))
        .unwrap();
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
