//! Answering a decision: keycaps, question forms and typed answers.
#[allow(unused_imports)]
use super::*;

/// The whole keystroke path in a real window: a blocked Pane, one key, and
/// the Decision gone because the answer went out.
#[gpui::test]
fn one_keystroke_answers_the_card(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("answer", 1);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    fake.streams.borrow()[0].send(decision("perm_01")).unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        assert!(
            view.cockpit
                .thread(thread)
                .and_then(|open| open.pending())
                .is_some(),
            "the card should be up before the key"
        );
    });

    cx.simulate_keystrokes("y");

    view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        assert!(
            view.cockpit
                .thread(thread)
                .and_then(|open| open.pending())
                .is_none(),
            "y must answer the Decision, not type a letter"
        );
    });
}

fn many_questions(id: &str) -> SessionEvent {
    let questions = (0..4)
        .map(|index| ferrite_core::questions::Question {
            id: None,
            question: format!("Question {} needs an answer", index + 1),
            header: format!("Question {}", index + 1),
            multi_select: false,
            secret: false,
            allow_other: false,
            options: vec![
                ferrite_core::questions::Choice {
                    label: "First choice".into(),
                    description: "The first available answer".into(),
                    preview: None,
                },
                ferrite_core::questions::Choice {
                    label: "Second choice".into(),
                    description: "The second available answer".into(),
                    preview: None,
                },
            ],
        })
        .collect::<Vec<_>>();
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind: ferrite_core::DecisionKind::Questions(questions),
            policy: Default::default(),
            id: id.into(),
            tool_use_id: "toolu_many_questions".into(),
            tool_name: "AskUserQuestion".into(),
            description: String::new(),
            input: serde_json::Value::Null,
            suggestions: vec![],
        },
    }
}

/// The Decision is the transcript's last row now (its tail): a long
/// question form scrolls with the transcript, so its last question is
/// reached by scrolling the body.
#[gpui::test]
fn a_long_question_form_scrolls_to_the_next_question(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("question-scroll", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(900.), px(600.)));
    fake.streams.borrow()[0]
        .send(many_questions("q_scroll"))
        .unwrap();
    tick(cx);
    let body = cx
        .debug_bounds("transcript-tail")
        .expect("the question is the transcript's tail");
    let last = cx.debug_bounds("question-choice-3-1").unwrap();
    assert!(
        last.bottom() <= body.bottom() + px(1.),
        "the followed tail shows the last question: {last:?} in {body:?}"
    );
}

/// Main and child questions share native controls; composer input stays chat.
#[gpui::test]
fn a_question_decision_is_answered_by_its_form(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("question-form", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    cx.simulate_input("Set up the question form");
    cx.simulate_keystrokes("enter");
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "I need one choice before I continue.".into(),
        })
        .unwrap();
    tick(cx);
    assert!(
        cx.debug_bounds("transcript-prompt").is_some(),
        "the fixture must show transcript text before the question arrives"
    );
    fake.streams.borrow()[0].send(question("q_01")).unwrap();
    tick(cx);
    cx.simulate_keystrokes("y");
    cx.run_until_parked();
    assert_eq!(composer_text(&view, cx), "y");
    assert!(fake.answered.borrow().is_empty());
    let choice = cx.debug_bounds("question-choice-0-1").unwrap();
    let island = cx.debug_bounds("question-island").unwrap();
    let (transcript_bounds, first_row_visible) = view.read_with(cx, |view, cx| {
        let scroll = view.panes[0].transcript().unwrap().read(cx).scroll();
        (scroll.bounds(), scroll.item_is_visible(0))
    });
    let composer = cx.debug_bounds("composer-block").unwrap();
    assert!(choice.right() <= island.right());
    assert!(
        first_row_visible && island.top() - transcript_bounds.top() > px(100.),
        "the pending question must leave the transcript mounted and readable above it: \
         {first_row_visible} {island:?} / {transcript_bounds:?}"
    );
    assert!(
        island.bottom() <= composer.top(),
        "the pending question is the transcript's last row, above the Composer: \
         {island:?} / {composer:?}"
    );
    cx.simulate_click(choice.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let serial = view.read_with(cx, |view, _| {
        view.cockpit.thread(thread).unwrap().pending_decisions()[0]
            .handle
            .serial
    });
    let submit = cx
        .debug_bounds(Box::leak(
            format!("request-submit-1-{serial}").into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_click(submit.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(
        composer_text(&view, cx),
        "y",
        "form submission preserves the chat draft"
    );
    let answered = fake.answered.borrow();
    let (id, DecisionAnswer::Questions { answers }) = answered.last().unwrap() else {
        panic!("answered question");
    };
    assert_eq!(id, "q_01");
    assert_eq!(answers[0].picks, vec![1]);
    assert_eq!(answers[0].other, None);
    assert!(cx.debug_bounds("question-island").is_none());
}

#[gpui::test]
fn a_typed_line_answers_a_question_in_the_operators_words(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("question-typed", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0].send(question("q_02")).unwrap();
    tick(cx);
    let serial = view.read_with(cx, |view, _| {
        view.cockpit.thread(thread).unwrap().pending_decisions()[0]
            .handle
            .serial
    });
    let input = cx
        .debug_bounds(Box::leak(
            format!("request-other-1-{serial}-0").into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_click(input.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    cx.simulate_input("neither, wait 2 dayss");
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    let submit = cx
        .debug_bounds(Box::leak(
            format!("request-submit-1-{serial}").into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_click(submit.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let answered = fake.answered.borrow();
    let DecisionAnswer::Questions { answers } = &answered.last().unwrap().1 else {
        panic!("answer");
    };
    assert_eq!(answers[0].other.as_deref(), Some("neither, wait 2 days"));
    assert!(fake.sent.borrow().is_empty());
}

/// A model-written title lands only on a Thread nobody has named: the
/// operator's own rename in the meantime wins, and None changes
/// nothing.
#[gpui::test]
fn a_titlers_answer_names_only_an_untitled_thread(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("titler-adopt", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let (first, second) = view.read_with(cx, |view, _| {
        let threads = view.cockpit.threads();
        (threads[0], threads[1])
    });
    view.update(cx, |view, cx| {
        view.adopt_title(first, None, cx);
        assert!(view.cockpit.thread(first).unwrap().title().is_none());
        view.adopt_title(first, Some("Wire the titler".into()), cx);
        assert_eq!(
            view.cockpit.thread(first).unwrap().title(),
            Some("Wire the titler")
        );
        assert_eq!(view.facts.name(first).as_ref(), "Wire the titler");

        view.cockpit.rename_thread(second, "Mine").unwrap();
        view.adopt_title(second, Some("Model's idea".into()), cx);
        assert_eq!(view.cockpit.thread(second).unwrap().title(), Some("Mine"));
    });
}

/// The mouse presses the option it points at — a real click on the
/// Decision row's option 3 denies (and steers), even with text on the
/// Composer line, where the digit KEY would type instead.
#[gpui::test]
fn clicking_a_keycap_runs_its_own_decide_verb(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("keycap-click", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    fake.streams.borrow()[0].send(decision("perm_01")).unwrap();
    tick(cx);
    // The row lives in the cached transcript: read its bounds while that
    // view has just painted, before typing re-renders only the Composer.
    let deny = cx.debug_bounds("decision-deny").expect("option 3, Deny");
    cx.simulate_input("not yet");
    tick(cx);
    let serial = view.read_with(cx, |view, cx| {
        assert!(
            !view.panes[0].composer.read(cx).is_empty(),
            "the premise: half-typed text on the line"
        );
        view.cockpit.thread(thread).unwrap().pending_decisions()[0]
            .handle
            .serial
    });
    cx.run_until_parked();
    let _ = serial;
    cx.simulate_click(deny.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.cockpit.thread(thread).unwrap().pending().is_none());
    });
    assert!(
        matches!(
            fake.answered.borrow().as_slice(),
            [(id, DecisionAnswer::Deny { .. })] if id == "perm_01"
        ),
        "option 3 denies: {:?}",
        fake.answered.borrow()
    );
}

/// With nothing blocked, the answer keys are letters again.
#[gpui::test]
fn the_answer_keys_are_letters_when_nothing_is_blocked(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("letters", 1);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    cx.simulate_keystrokes("y");

    let typed = view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.take(cx))
    });
    assert_eq!(typed, "y");
}
