//! WP-F terminal-native parity (decisions): the Decision as one transcript
//! row with three options, its keys, the amend note, Ferrite's thread
//! rules and the one answer path every surface shares (D-1 … D-8).
#[allow(unused_imports)]
use super::*;

fn answered(fake: &Fake) -> Vec<(String, DecisionAnswer)> {
    fake.answered.borrow().clone()
}

/// A Claude `Bash` approval of `command`.
fn bash(id: &str, command: &str) -> Decision {
    Decision {
        delivery: Default::default(),
        kind: Default::default(),
        policy: Default::default(),
        id: id.into(),
        tool_use_id: format!("toolu-{id}"),
        tool_name: "Bash".into(),
        description: String::new(),
        input: serde_json::json!({ "command": command }),
        suggestions: vec![],
    }
}

fn ask(decision: Decision) -> SessionEvent {
    SessionEvent::DecisionRequested { decision }
}

/// One Thread on the board with `decision` waiting on it, the production
/// keys bound.
fn waiting<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
    decision: Decision,
) -> (
    gpui::Entity<CockpitView>,
    Fake,
    &'a mut gpui::VisualTestContext,
) {
    let (core, fake) = cockpit(name, 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    fake.streams.borrow()[0].send(ask(decision)).unwrap();
    tick(cx);
    (view, fake, cx)
}

fn cursor(view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<usize> {
    view.read_with(cx, |view, _| {
        let request = view.shown_request(0).expect("a Decision waits");
        view.panes[0].request_forms.cursor(&request.handle)
    })
}

fn near(a: gpui::Pixels, b: gpui::Pixels) -> bool {
    (a - b).abs() <= px(0.5)
}

/// D-1, D-3: the row's grammar — the `◆` at the gutter's text origin on the
/// head's line, half a row to the command band, half a row to the options
/// (the first pre-selected: its `❯` in its own gutter), half a row to the
/// hint; every option one row.
#[gpui::test]
fn the_decision_row_reads_head_band_options_hint(cx: &mut TestAppContext) {
    let (view, _fake, cx) = waiting(
        "parity-row",
        cx,
        bash("row", "gh issue close 212 --reason \"not planned\""),
    );
    let (thread, serial) = view.read_with(cx, |view, _| {
        let request = view.shown_request(0).unwrap();
        (view.panes[0].thread().unwrap(), request.handle.serial)
    });
    let head = cx.debug_bounds("decision-head").expect("the head");
    let mark = cx.debug_bounds("decision-mark").expect("the ◆");
    let well = cx.debug_bounds("approval-well").expect("the command band");
    let allow = bounds(cx, format!("request-allow-{}-{serial}", thread.get()));
    let always = bounds(cx, format!("request-always-{}-{serial}", thread.get()));
    let deny = bounds(cx, format!("request-deny-{}-{serial}", thread.get()));
    let hints = cx.debug_bounds("decision-hints").expect("the hint");
    let line = px(crate::theme::LH_UI);
    let half = px(crate::theme::LH_UI / 2.);
    assert!(
        near(head.size.height, line),
        "the head is one row: {head:?}"
    );
    assert!(
        near(
            mark.left() + px(crate::theme::DECISION_MARK_LEAD),
            head.left()
        ),
        "◆ centred at the gutter's origin"
    );
    assert!(near(mark.size.width, px(crate::theme::DECISION_MARK)));
    assert!(
        near(well.top() - head.bottom(), half),
        "{head:?} / {well:?}"
    );
    assert!(
        near(well.size.height, line + half),
        "the band is a row and two quarter rows: {well:?}"
    );
    assert!(
        near(allow.top() - well.bottom(), half),
        "{well:?} / {allow:?}"
    );
    for row in [allow, always, deny] {
        assert!(near(row.size.height, line), "an option is one row: {row:?}");
    }
    assert!(near(always.top(), allow.bottom()) && near(deny.top(), always.bottom()));
    assert!(
        near(hints.top() - deny.bottom(), half),
        "{deny:?} / {hints:?}"
    );
    // The band and the options run on the content column, after the gutter.
    assert!(near(
        well.left(),
        head.left() + px(crate::theme::GLYPH_GUTTER)
    ));
    assert!(near(allow.left(), well.left()) && near(allow.right(), well.right()));
    let cursor = cx.debug_bounds("decision-cursor").expect("the ❯");
    assert!(
        allow.contains(&cursor.center()),
        "the first option is pre-selected: {cursor:?} in {allow:?}"
    );
}

/// D-2: the head's words — the tool as the transcript names it, the
/// provider, its context.
#[gpui::test]
fn the_head_names_its_provider_and_context(cx: &mut TestAppContext) {
    let (view, _fake, cx) = waiting("parity-head", cx, bash("head", "ls"));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let (provider, context) = view.read_with(cx, |view, _| view.decision_context(thread));
    assert_eq!(provider, Some(Provider::Claude));
    assert_eq!(
        context.as_deref(),
        Some("default"),
        "Claude's permission mode"
    );
}

/// D-4, D-5: `2` with a standing answer offered sends it.
#[gpui::test]
fn option_two_sends_the_providers_standing_answer(cx: &mut TestAppContext) {
    let mut decision = bash("standing", "gh issue close 212");
    decision.suggestions = vec![ferrite_core::DecisionChoice {
        label: "Always allow gh issue".into(),
        value: serde_json::json!({ "rule": "gh issue" }),
        standing: true,
    }];
    let (_view, fake, cx) = waiting("parity-two-standing", cx, decision);
    cx.dispatch_action(PickOption2);
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(),
            [(id, DecisionAnswer::AllowAlways { suggestion, .. })]
                if id == "standing" && *suggestion == serde_json::json!({ "rule": "gh issue" })),
        "{:?}",
        answered(&fake)
    );
}

/// D-4: `2` with no standing answer allows and keeps a Ferrite thread
/// rule, which allows later approvals it covers as they arrive — and only
/// those.
#[gpui::test]
fn option_two_without_a_standing_answer_keeps_a_thread_rule(cx: &mut TestAppContext) {
    let (view, fake, cx) = waiting("parity-two-rule", cx, bash("first", "gh issue close 212"));
    cx.dispatch_action(PickOption2);
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(), [(id, DecisionAnswer::Allow { .. })] if id == "first"),
        "{:?}",
        answered(&fake)
    );
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.decisions.rules(),
            [crate::decision::ThreadRule {
                thread,
                tool: "Bash".into(),
                pattern: "gh issue *".into(),
            }]
        );
    });
    fake.streams.borrow()[0]
        .send(ask(bash("second", "gh issue list --state open")))
        .unwrap();
    tick(cx);
    assert!(
        matches!(answered(&fake).last(), Some((id, DecisionAnswer::Allow { .. })) if id == "second"),
        "the rule allows what it covers: {:?}",
        answered(&fake)
    );
    fake.streams.borrow()[0]
        .send(ask(bash("third", "gh pr merge 3")))
        .unwrap();
    tick(cx);
    assert_eq!(answered(&fake).len(), 2, "what it does not cover waits");
}

/// D-4: `3` denies with the steer message, and the keyboard lands in the
/// Pane's Composer.
#[gpui::test]
fn option_three_denies_and_lands_in_the_composer(cx: &mut TestAppContext) {
    let (view, fake, cx) = waiting("parity-three", cx, bash("steer", "rm -rf build"));
    cx.dispatch_action(PickOption3);
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(),
            [(id, DecisionAnswer::Deny { message })]
                if id == "steer" && message == crate::decision::DENY_AND_STEER),
        "{:?}",
        answered(&fake)
    );
    let focused = cx.update(|window, cx| {
        view.read(cx).panes[0]
            .composer
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    });
    assert!(focused, "the keyboard lands in the Composer to steer");
}

/// D-5: ↓↓ moves the cursor to the third option, wrapping on; ⏎ answers
/// the option under it.
#[gpui::test]
fn the_cursor_moves_and_confirm_answers_its_option(cx: &mut TestAppContext) {
    let (view, fake, cx) = waiting("parity-cursor", cx, bash("cursor", "cargo test"));
    assert_eq!(cursor(&view, cx), None, "the first option is pre-selected");
    cx.dispatch_action(crate::decision::SelectNext);
    cx.dispatch_action(crate::decision::SelectNext);
    tick(cx);
    assert_eq!(cursor(&view, cx), Some(2));
    let (thread, serial) = view.read_with(cx, |view, _| {
        let request = view.shown_request(0).unwrap();
        (view.panes[0].thread().unwrap(), request.handle.serial)
    });
    let deny = bounds(cx, format!("request-deny-{}-{serial}", thread.get()));
    let mark = cx.debug_bounds("decision-cursor").expect("the ❯");
    assert!(deny.contains(&mark.center()), "the ❯ follows the cursor");
    cx.dispatch_action(crate::decision::SelectNext);
    tick(cx);
    assert_eq!(cursor(&view, cx), Some(0), "it wraps");
    cx.dispatch_action(crate::decision::SelectPrevious);
    cx.dispatch_action(crate::decision::Confirm);
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(), [(id, DecisionAnswer::Deny { .. })] if id == "cursor"),
        "{:?}",
        answered(&fake)
    );
}

/// D-5: esc denies like option 3.
#[gpui::test]
fn dismiss_denies(cx: &mut TestAppContext) {
    let (_view, fake, cx) = waiting("parity-dismiss", cx, bash("dismiss", "make deploy"));
    cx.dispatch_action(crate::decision::Dismiss);
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(),
            [(id, DecisionAnswer::Deny { message })]
                if id == "dismiss" && message == crate::decision::DENY_AND_STEER),
        "{:?}",
        answered(&fake)
    );
}

/// D-6: ⇥ opens the note, which takes the keyboard; ⏎ sends the cursor's
/// option with it — a denial carries it as its message.
#[gpui::test]
fn the_amend_note_rides_a_denial(cx: &mut TestAppContext) {
    let (view, fake, cx) = waiting("parity-amend-deny", cx, bash("amend", "rm -rf build"));
    cx.dispatch_action(crate::decision::SelectPrevious);
    cx.dispatch_action(crate::decision::Amend);
    tick(cx);
    assert!(cx.debug_bounds("decision-note").is_some(), "the note opens");
    let focused = view.update_in(cx, |view, window, cx| {
        let request = view.shown_request(0).unwrap();
        view.panes[0]
            .request_forms
            .note(&request.handle)
            .expect("a note")
            .focus_handle(cx)
            .is_focused(window)
    });
    assert!(focused, "the note takes the keyboard");
    cx.simulate_input("use cargo clean instead");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(),
            [(id, DecisionAnswer::Deny { message })]
                if id == "amend" && message == "use cargo clean instead"),
        "{:?}",
        answered(&fake)
    );
}

/// D-6: after an allow the note is the Thread's next prompt, queued on the
/// Session.
#[gpui::test]
fn the_amend_note_follows_an_allow_as_the_next_prompt(cx: &mut TestAppContext) {
    let (view, fake, cx) = waiting("parity-amend-allow", cx, bash("amend-allow", "cargo test"));
    cx.dispatch_action(crate::decision::Amend);
    tick(cx);
    cx.simulate_input("then run clippy");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(), [(id, DecisionAnswer::Allow { .. })] if id == "amend-allow"),
        "{:?}",
        answered(&fake)
    );
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let queued: Vec<String> = view.read_with(cx, |view, _| {
        view.cockpit
            .thread(thread)
            .unwrap()
            .queued_all()
            .into_iter()
            .map(str::to_string)
            .collect()
    });
    assert_eq!(queued, ["then run clippy"], "the note reached the Session");
}

/// D-6: esc closes the note and the keyboard goes back; a second esc
/// denies.
#[gpui::test]
fn esc_closes_the_note_then_denies(cx: &mut TestAppContext) {
    let (view, fake, cx) = waiting("parity-amend-esc", cx, bash("amend-esc", "cargo test"));
    cx.dispatch_action(crate::decision::Amend);
    tick(cx);
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(cx.debug_bounds("decision-note").is_none(), "esc closed it");
    assert!(
        answered(&fake).is_empty(),
        "closing the note answers nothing"
    );
    let focused = cx.update(|window, cx| {
        view.read(cx).panes[0]
            .composer
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    });
    assert!(focused, "the keyboard is back in the Composer");
    cx.dispatch_action(crate::decision::Dismiss);
    tick(cx);
    assert!(
        matches!(
            answered(&fake).as_slice(),
            [(_, DecisionAnswer::Deny { .. })]
        ),
        "{:?}",
        answered(&fake)
    );
}

/// D-8: a lone single-select question answers on one option, and false
/// sends the caller to the Pane for anything more.
#[gpui::test]
fn answer_question_option_answers_a_lone_single_select(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("parity-question-option", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    fake.streams.borrow()[0].send(question("lone")).unwrap();
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    assert!(!view.update_in(cx, |view, window, cx| view
        .answer_question_option(thread, 9, window, cx)));
    assert!(view.update_in(cx, |view, window, cx| view
        .answer_question_option(thread, 1, window, cx)));
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(),
            [(id, DecisionAnswer::Questions { answers })] if id == "lone" && answers[0].picks == [1]),
        "{:?}",
        answered(&fake)
    );
}

/// D-8: a multi-select question needs its form: false, and nothing sent.
#[gpui::test]
fn answer_question_option_leaves_a_multi_select_to_its_form(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("parity-question-multi", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    let SessionEvent::DecisionRequested { mut decision } = question("multi") else {
        unreachable!()
    };
    let ferrite_core::DecisionKind::Questions(questions) = &mut decision.kind else {
        unreachable!()
    };
    questions[0].multi_select = true;
    fake.streams.borrow()[0].send(ask(decision)).unwrap();
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    assert!(!view.update_in(cx, |view, window, cx| view
        .answer_question_option(thread, 0, window, cx)));
    assert!(answered(&fake).is_empty(), "a multi-select needs its form");
}

/// D-8: a question's cursor walks its options and ⏎ answers the one under
/// it.
#[gpui::test]
fn a_question_answers_from_its_cursor(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("parity-question-keys", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    fake.streams.borrow()[0].send(question("keys")).unwrap();
    tick(cx);
    assert!(cx.debug_bounds("decision-hints").is_some());
    cx.dispatch_action(crate::decision::SelectNext);
    tick(cx);
    assert_eq!(cursor(&view, cx), Some(1));
    cx.dispatch_action(crate::decision::Confirm);
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(),
            [(id, DecisionAnswer::Questions { answers })] if id == "keys" && answers[0].picks == [1]),
        "{:?}",
        answered(&fake)
    );
}

/// D-8: esc dismisses a question.
#[gpui::test]
fn dismiss_skips_a_question(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("parity-question-dismiss", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    fake.streams.borrow()[0].send(question("skip")).unwrap();
    tick(cx);
    cx.dispatch_action(crate::decision::Dismiss);
    tick(cx);
    assert!(
        matches!(answered(&fake).as_slice(),
            [(id, DecisionAnswer::Deny { message })]
                if id == "skip" && message == crate::decision::QUESTION_SKIPPED),
        "{:?}",
        answered(&fake)
    );
}

/// The transcript's tail redraws when the Decision does: the cursor moving
/// changes its key, and an answered Decision leaves no tail.
#[gpui::test]
fn the_tail_key_follows_the_cursor(cx: &mut TestAppContext) {
    let (view, _fake, cx) = waiting("parity-tail", cx, bash("tail", "cargo test"));
    let key = |view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            view.decision_tail(0, cx).map(|tail| tail.key)
        })
    };
    let before = key(&view, cx).expect("a waiting Decision is a tail");
    cx.dispatch_action(crate::decision::SelectNext);
    tick(cx);
    let after = key(&view, cx).expect("still waiting");
    assert_ne!(before, after, "the cursor is part of what the tail draws");
    cx.dispatch_action(PickOption1);
    tick(cx);
    assert_eq!(
        key(&view, cx),
        None,
        "an answered Decision leaves the transcript"
    );
}
