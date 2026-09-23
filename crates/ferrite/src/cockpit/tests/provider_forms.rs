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

/// A full directory list must not take the committing action off screen.
/// Exercise both create and edit through their real footer buttons.
#[gpui::test]
fn project_completion_stays_visible_while_a_compact_form_scrolls(cx: &mut TestAppContext) {
    let (core, _) = cockpit("project-fixed-footer", 1);
    let before = core.registry().projects().len();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(640.), px(420.)));
    let base = scratch("project-fixed-footer-folders");
    let directories: Vec<_> = (0..8)
        .map(|index| {
            let path = base.join(format!("directory-{index}"));
            std::fs::create_dir_all(&path).unwrap();
            path.canonicalize().unwrap()
        })
        .collect();
    let mut created = None;

    for editing in [false, true] {
        let title = if editing {
            "Renamed project"
        } else {
            "New project"
        };
        view.update(cx, |view, cx| {
            if editing {
                view.open_project_editor(created.unwrap(), cx);
            } else {
                view.open_project_creator(cx);
                for directory in &directories {
                    view.adopt_editor_directory(directory.clone(), cx);
                }
            }
            let name = view.project_editor.as_ref().unwrap().name.clone();
            name.update(cx, |name, cx| name.set(title.into(), cx));
        });
        tick(cx);

        let card = cx.debug_bounds("project-editor-card").unwrap();
        let confirm = cx.debug_bounds("project-confirm").unwrap();
        assert!(
            confirm.size.height > px(0.)
                && confirm.left() >= card.left()
                && confirm.top() >= card.top()
                && confirm.right() <= card.right()
                && confirm.bottom() <= card.bottom(),
            "the full footer action must be inside the compact card: {confirm:?}, {card:?}"
        );
        let first_id = format!("project-directory:{}", directories[0].display());
        let first = debug_bounds(cx, first_id.clone()).unwrap();
        assert!(first.top() >= card.top() && first.bottom() < confirm.top());

        cx.simulate_event(gpui::ScrollWheelEvent {
            position: card.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-2000.))),
            ..Default::default()
        });
        tick(cx);
        assert_eq!(cx.debug_bounds("project-confirm").unwrap(), confirm);
        // `+ Add directory` rides the body under the list: scrolled to the
        // end, it sits under the last directory and above the pinned footer.
        let add = cx
            .debug_bounds("project-add-directory")
            .expect("the add control follows the list");
        assert!(
            add.size.height > px(0.)
                && add.left() >= card.left()
                && add.right() <= card.right()
                && add.bottom() < confirm.top(),
            "{add:?} inside {card:?}, above {confirm:?}"
        );
        let last = debug_bounds(
            cx,
            format!(
                "project-directory:{}",
                directories.last().unwrap().display()
            ),
        )
        .expect("scrolling reveals the last directory");
        assert!(last.top() >= card.top() && last.bottom() < confirm.top());
        assert!(
            last.bottom() <= add.top(),
            "the add control is under the list"
        );
        if let Some(scrolled_first) = debug_bounds(cx, first_id) {
            assert!(
                scrolled_first.top() < first.top(),
                "the body actually scrolled"
            );
        }

        cx.simulate_click(confirm.center(), gpui::Modifiers::none());
        tick(cx);
        created = Some(view.read_with(cx, |view, _| {
            assert!(
                view.project_editor.is_none(),
                "the visible footer completes the form"
            );
            assert_eq!(view.cockpit.registry().projects().len(), before + 1);
            let project = view.nav_filter.expect("the completed Project is selected");
            if let Some(created) = created {
                assert_eq!(project, created, "Done must edit the same Project");
            }
            let record = view.cockpit.registry().project(project).unwrap();
            assert_eq!(record.title, title);
            assert_eq!(record.root, directories[0]);
            assert_eq!(record.additional_roots, directories[1..]);
            project
        }));
    }
}

/// A question without a header is named by its whole text, and the notice
/// row cuts it by width at the column's edge: one line, reaching the edge,
/// not a character count short of it.
#[gpui::test]
fn a_long_question_notice_is_cut_by_width_not_by_characters(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("question-notice-width", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    let text = "Which details should remain visible when the transcript is narrowed to a single \
                column, and should the tool rows keep their durations or give them up first?";
    let q = ferrite_core::questions::Question {
        id: Some("q".into()),
        question: text.into(),
        header: String::new(),
        multi_select: false,
        secret: false,
        allow_other: false,
        options: vec![ferrite_core::questions::Choice {
            label: "Keep".into(),
            description: String::new(),
            preview: None,
        }],
    };
    fake.streams.borrow()[0]
        .send(typed_decision(DecisionKind::Questions(vec![q])))
        .unwrap();
    tick(cx);
    let (id, notice) = view.read_with(cx, |v, _| {
        let thread = v.cockpit.thread(v.panes[0].thread().unwrap()).unwrap();
        let block = thread.transcript().blocks().last().unwrap();
        let ferrite_core::transcript::Body::Notice(line) = &block.body else {
            panic!("the question leaves a notice: {:?}", block.body)
        };
        (block.id, line.clone())
    });
    assert_eq!(notice, format!("asks 1 question · {text}"));
    let row = debug_bounds(cx, format!("notice-{id:?}")).expect("the notice row");
    let block = cx.debug_bounds("composer-block").unwrap();
    assert_eq!(row.size.height, px(crate::theme::LH_UI), "one line");
    assert!(
        row.right() <= block.right() - px(crate::theme::BOX_INSET_X) + px(0.5),
        "the row {row:?} ends at the column's edge {block:?}"
    );
    assert!(
        row.right() >= block.right() - px(crate::theme::BOX_INSET_X) - px(0.5),
        "the row {row:?} runs to the column's edge, not a character count short"
    );
}
