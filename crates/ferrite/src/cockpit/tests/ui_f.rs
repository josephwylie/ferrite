//! WP-F's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

/// An L2 cell swaps its body for the Decision card and keeps its Composer
/// under it, which holds the keyboard with the `Decision` context: `y` on
/// its empty line answers the cell, as the card's keycap says.
#[gpui::test]
fn an_l2_decision_answers_from_the_keyboard(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("l2-decision-keys", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(560.), px(700.)));
    tick(cx);
    assert_eq!(
        cx.update(|window, cx| view.read(cx).level_now(window)),
        Level::Instruments,
        "the premise: the cell is at L2"
    );
    fake.streams.borrow()[0].send(decision("l2-perm")).unwrap();
    tick(cx);
    cx.simulate_keystrokes("y");
    tick(cx);
    assert!(
        matches!(
            fake.answered.borrow().last(),
            Some((id, DecisionAnswer::Allow { .. })) if id == "l2-perm"
        ),
        "y answers the L2 Decision: {:?}",
        fake.answered.borrow()
    );
}

fn answered(fake: &Fake) -> Vec<(String, DecisionAnswer)> {
    fake.answered.borrow().clone()
}

/// An approval card on an empty Composer line answers in one key: `y`
/// allows, `n` denies, and `a` sends the provider's standing answer.
#[gpui::test]
fn an_approval_answers_y_a_and_n_in_one_key(cx: &mut TestAppContext) {
    for (key, id) in [("y", "one-key-y"), ("n", "one-key-n"), ("a", "one-key-a")] {
        let (core, fake) = cockpit(id, 1);
        bind_production_keys(cx);
        let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(800.)));
        let SessionEvent::DecisionRequested { mut decision } = decision(id) else {
            unreachable!()
        };
        decision.suggestions = vec![ferrite_core::DecisionChoice {
            label: "Always allow Write".into(),
            value: serde_json::json!({ "rule": "Write" }),
            standing: true,
        }];
        fake.streams.borrow()[0]
            .send(SessionEvent::DecisionRequested { decision })
            .unwrap();
        tick(cx);
        assert!(
            cx.debug_bounds("question-island").is_some(),
            "the card is up"
        );
        cx.simulate_keystrokes(key);
        tick(cx);
        let answered = answered(&fake);
        let [(answered_id, answer)] = answered.as_slice() else {
            panic!("`{key}` answers exactly once: {answered:?}");
        };
        assert_eq!(answered_id, id);
        match key {
            "y" => assert!(matches!(answer, DecisionAnswer::Allow { .. })),
            "n" => assert!(matches!(answer, DecisionAnswer::Deny { .. })),
            _ => assert!(
                matches!(answer, DecisionAnswer::AllowAlways { suggestion, .. }
                    if *suggestion == serde_json::json!({ "rule": "Write" })),
                "{answer:?}"
            ),
        }
    }
}

/// A native choice's row shows its digit, and the digit sends that choice.
#[gpui::test]
fn an_approval_digit_picks_its_native_choice(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("one-key-digit", 1);
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    let SessionEvent::DecisionRequested { mut decision } = decision("digit") else {
        unreachable!()
    };
    decision.suggestions = vec![ferrite_core::DecisionChoice {
        label: "Block example.com".into(),
        value: serde_json::json!({ "opaque": "block" }),
        standing: false,
    }];
    fake.streams.borrow()[0]
        .send(SessionEvent::DecisionRequested { decision })
        .unwrap();
    tick(cx);
    // Rows: [y] Allow, [2] Block example.com, [n] Deny.
    cx.simulate_keystrokes("2");
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(), [(_, DecisionAnswer::Choose { value })]
            if *value == serde_json::json!({ "opaque": "block" })),
        "{:?}",
        answered(&fake)
    );
}

/// A single single-select question answers on its digit: one key.
#[gpui::test]
fn a_single_select_question_answers_on_its_digit(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("one-key-question", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    fake.streams.borrow()[0].send(question("digit-q")).unwrap();
    tick(cx);
    cx.simulate_keystrokes("2");
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(), [(id, DecisionAnswer::Questions { answers })]
            if id == "digit-q" && answers[0].picks == [1] && answers[0].other.is_none()),
        "{:?}",
        answered(&fake)
    );
    assert_eq!(
        composer_text(&view, cx),
        "",
        "the digit answered; it never typed"
    );
}

/// With words on the line a digit is a digit: the question keeps waiting.
#[gpui::test]
fn a_digit_types_when_the_line_has_words(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("digit-types", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    fake.streams.borrow()[0].send(question("typed-q")).unwrap();
    tick(cx);
    cx.simulate_input("wait ");
    cx.simulate_keystrokes("2");
    tick(cx);
    assert!(answered(&fake).is_empty());
    assert_eq!(composer_text(&view, cx), "wait 2");
}

/// A multi-select question: digits toggle its options, ↵ sends the picks.
#[gpui::test]
fn a_multi_select_question_toggles_on_digits_and_sends_on_enter(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("multi-toggle", 1);
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    let SessionEvent::DecisionRequested { mut decision } = question("multi-q") else {
        unreachable!()
    };
    let ferrite_core::DecisionKind::Questions(questions) = &mut decision.kind else {
        unreachable!()
    };
    questions[0].multi_select = true;
    fake.streams.borrow()[0]
        .send(SessionEvent::DecisionRequested { decision })
        .unwrap();
    tick(cx);
    cx.simulate_keystrokes("1 2 1 2 1");
    tick(cx);
    assert!(answered(&fake).is_empty(), "toggling never sends");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(), [(_, DecisionAnswer::Questions { answers })]
            if answers[0].picks == [0]),
        "{:?}",
        answered(&fake)
    );
}

/// A Subagent's approval card shows its keys and they answer it: `y` on
/// the child's tab allows exactly that child's request.
#[gpui::test]
fn a_subagent_approval_answers_its_keycaps(cx: &mut TestAppContext) {
    use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, Subject};
    for (key, id) in [("y", "child-y"), ("n", "child-n"), ("1", "child-1")] {
        let (core, fake) = cockpit(id, 1);
        bind_production_keys(cx);
        let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
        cx.simulate_resize(gpui::size(px(1000.), px(800.)));
        let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
        let agent = AgentKey::new(Provider::Claude, "root", "keys-child");
        let mut info = AgentInfo::new(agent.clone());
        info.name = Some("keys-child".into());
        info.parent = Some(Subject::Main);
        fake.streams.borrow()[0]
            .send(SessionEvent::Activity(ActivityEvent::Discovered(info)))
            .unwrap();
        let SessionEvent::DecisionRequested { decision } = decision(id) else {
            unreachable!()
        };
        fake.streams.borrow()[0]
            .send(SessionEvent::Activity(ActivityEvent::Decision {
                subject: Some(Subject::Subagent(agent.clone())),
                decision,
            }))
            .unwrap();
        tick(cx);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.select_subject(thread, Subject::Subagent(agent.clone()), window, cx)
            })
        });
        tick(cx);
        assert!(
            cx.debug_bounds("question-island").is_some(),
            "the child's card is up"
        );
        cx.simulate_keystrokes(key);
        tick(cx);
        let answered = answered(&fake);
        match key {
            "n" => assert!(
                matches!(answered.as_slice(), [(answered_id, DecisionAnswer::Deny { .. })] if answered_id == id),
                "{answered:?}"
            ),
            _ => assert!(
                matches!(answered.as_slice(), [(answered_id, DecisionAnswer::Allow { .. })] if answered_id == id),
                "`{key}` allows: {answered:?}"
            ),
        }
    }
}
