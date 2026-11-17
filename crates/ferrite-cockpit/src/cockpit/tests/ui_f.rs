//! WP-F's cockpit-level tests (append-only; see plans/20 §2).
#[allow(unused_imports)]
use super::*;

fn answered(fake: &Fake) -> Vec<(String, DecisionAnswer)> {
    fake.answered.borrow().clone()
}

/// Every approval offers exactly three options and a digit picks one of
/// them: a native choice outside the three (`Block example.com`) has no
/// row, so `2` is option 2 — allow for this thread (Ferrite keeps the rule
/// when the provider offered no standing answer).
#[gpui::test]
fn an_approval_digit_picks_one_of_its_three_options(cx: &mut TestAppContext) {
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
    // Rows: 1. Allow, 2. Allow `Write` for this thread, 3. Deny, and tell
    // Claude what to do instead.
    cx.simulate_keystrokes("2");
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(), [(id, DecisionAnswer::Allow { .. })] if id == "digit"),
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
