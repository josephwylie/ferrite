//! Forks from the window (Claude's): an answer's Fork and its `⋯` menu,
//! the Thread menu's Fork and the palette's Rewind and Fork. Each fork is a
//! new Thread that opens focused.
#[allow(unused_imports)]
use super::*;

/// One Thread whose agent can rewind and fork, holding a finished answer.
fn answered<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, Fake, &'a mut gpui::VisualTestContext) {
    let (core, fake) = cockpit(name, 1);
    *fake.native_controls.borrow_mut() = true;
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "alpha\n\n".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    (view, fake, cx)
}

/// The fork the agent was asked for opened, and holds the keyboard's Pane.
fn landed_on_fork(view: &Entity<CockpitView>, fake: &Fake, cx: &mut gpui::VisualTestContext) {
    tick(cx);
    assert_eq!(fake.forks.borrow().len(), 1, "one fork asked");
    view.read_with(cx, |view, _| {
        let threads = view.cockpit.threads();
        assert_eq!(threads.len(), 2, "the fork is open");
        assert_eq!(
            view.focused_thread(),
            Some(threads[1]),
            "the window lands on it"
        );
    });
}

/// Claude's answer hover: Copy, Fork and `⋯` under the answer. Fork takes
/// the whole conversation when the answer is the latest, and opens the
/// new Thread focused.
#[gpui::test]
fn an_answers_fork_opens_the_whole_conversation_focused(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("answer-fork", cx);
    let actions = cx.debug_bounds("answer-actions").expect("its actions");
    let copy = cx.debug_bounds("message-copy").expect("Copy");
    let fork = cx.debug_bounds("answer-fork").expect("Fork");
    let more = cx.debug_bounds("answer-more").expect("⋯");
    assert!(copy.right() <= fork.left() && fork.right() <= more.left());
    assert!(actions.contains(&fork.center()) && actions.contains(&more.center()));
    cx.simulate_click(fork.center(), gpui::Modifiers::none());
    landed_on_fork(&view, &fake, cx);
    assert_eq!(fake.forks.borrow()[0].before, None, "the whole of it");
}

/// The answer's `⋯`, and a right-click on it: Copy message, Copy message as
/// Markdown │ Attach message as context, Pin as chapter │ Fork from here;
/// Copy copies the answer, Attach makes a Reply card of it.
#[gpui::test]
fn an_answers_menu_copies_replies_and_forks(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("answer-menu", cx);
    let more = cx.debug_bounds("answer-more").expect("⋯");
    cx.simulate_click(more.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("answer-menu").is_some(), "the menu opens");
    for row in [0, 1, 3, 4, 6] {
        assert!(cx
            .debug_bounds(format!("answer-menu-row-{row}").leak())
            .is_some());
    }
    let copy = cx.debug_bounds("answer-menu-row-0").unwrap();
    cx.simulate_click(copy.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("answer-menu").is_none(), "a row closes it");
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .map(|text| text.trim_end().to_string())
            .as_deref(),
        Some("alpha")
    );

    // Reply attaches the whole answer as a Reply card in the tray.
    cx.simulate_click(more.center(), gpui::Modifiers::none());
    tick(cx);
    let reply = cx.debug_bounds("answer-menu-row-3").unwrap();
    cx.simulate_click(reply.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.panes[0].composer.read(cx).replies(),
            [ferrite_pane::quote::Reply::new("alpha", "")]
        );
    });

    // Fork from here, past the rule.
    cx.simulate_click(more.center(), gpui::Modifiers::none());
    tick(cx);
    let fork = cx
        .debug_bounds("answer-menu-row-6")
        .expect("Fork from here");
    cx.simulate_click(fork.center(), gpui::Modifiers::none());
    landed_on_fork(&view, &fake, cx);
}

/// An agent that cannot fork offers no Fork under an answer.
#[gpui::test]
fn an_agent_that_cannot_fork_offers_none(cx: &mut TestAppContext) {
    let (_view, fake, cx) = answered("answer-no-fork", cx);
    *fake.native_controls.borrow_mut() = false;
    // The gate is read as the transcript syncs: a frame after the change.
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "bravo\n\n".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("answer-more").is_some());
    assert!(cx.debug_bounds("answer-fork").is_none());
}

/// The Thread menu's Fork (`F`) and the palette's `Fork thread` fork the
/// whole conversation, focused; the palette's `Rewind to last message`
/// opens the latest prompt's own Rewind menu.
#[gpui::test]
fn the_menu_and_the_palette_fork_and_rewind(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("menu-fork", cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let fork = view.read_with(cx, |view, _| {
        view.context_rows(MenuTarget::Thread(thread))
            .into_iter()
            .flatten()
            .find(|(_, verb)| *verb == MenuVerb::Fork)
            .map(|(item, _)| (item.label.to_string(), item.shortcut, item.disabled))
    });
    assert_eq!(fork, Some(("Fork".into(), Some("F".into()), false)));
    let names = |cx: &mut gpui::VisualTestContext| -> Vec<String> {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.palette_command_rows(window, cx)
                    .into_iter()
                    .map(|row| row.name.to_string())
                    .collect()
            })
        })
    };
    let offered = names(cx);
    assert!(offered.contains(&"Fork thread".to_string()), "{offered:?}");
    view.update_in(cx, |view, window, cx| {
        view.run_menu_verb(MenuTarget::Thread(thread), MenuVerb::Fork, window, cx)
    });
    landed_on_fork(&view, &fake, cx);

    // A sent prompt goes out under its id: the palette's Rewind opens its
    // menu.
    view.update(cx, |view, _| view.cockpit.send(thread, "again".into()));
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    view.update(cx, |view, cx| view.focus_thread(thread, cx));
    tick(cx);
    let offered = names(cx);
    assert!(
        offered.contains(&"Rewind to last message".to_string()),
        "{offered:?}"
    );
    view.update_in(cx, |view, window, cx| {
        view.run_command(crate::palette::Command::Rewind, window, cx)
    });
    view.read_with(cx, |view, _| {
        let menu = view.context_menu.as_ref().expect("the Rewind menu");
        assert!(matches!(menu.target, MenuTarget::Prompt { thread: on, .. } if on == thread));
    });
}

/// A prompt's Rewind menu offers its code back only where the agent keeps
/// file checkpoints (`ControlKind::RewindFiles`): Claude does, Codex does
/// not, and the menu asks the agent, never the provider's name.
#[gpui::test]
fn the_rewind_menu_offers_the_code_only_where_the_agent_restores_files(cx: &mut TestAppContext) {
    // The rows never depend on which prompt: any block names one.
    let labels = |view: &CockpitView, thread| {
        let block = view
            .cockpit
            .thread(thread)
            .and_then(|open| open.transcript().blocks().first().map(|shown| shown.id));
        let Some(block) = block else {
            return Vec::new();
        };
        view.context_rows(MenuTarget::Prompt { thread, block })
            .into_iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect::<Vec<_>>()
    };
    let (view, _fake, cx) = answered("rewind-code", cx);
    view.read_with(cx, |view, _| {
        let thread = view.cockpit.threads()[0];
        assert_eq!(
            labels(view, thread),
            ["Rewind conversation", "Rewind conversation and code"]
        );
    });

    let (core, fake) = cockpit("rewind-no-code", 1);
    *fake.native_controls.borrow_mut() = true;
    fake.withheld
        .borrow_mut()
        .push(ferrite_core::ControlKind::RewindFiles);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "alpha\n\n".into(),
        })
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        let thread = view.cockpit.threads()[0];
        assert_eq!(labels(view, thread), ["Rewind conversation"]);
    });
}

fn right_click(cx: &mut gpui::VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_down(at, MouseButton::Right, gpui::Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Right, gpui::Modifiers::none());
    tick(cx);
}

/// Right-clicking an answer opens Claude's message menu, not the Pane's:
/// Copy message, Copy as Markdown │ Attach, Pin │ Fork; Pin marks the
/// answer a chapter and the row then reads `Unpin chapter`. On a prompt it
/// is Copy, Copy as Markdown │ Attach │ Rewind, Fork; Rewind opens the
/// cockpit's own Rewind menu.
#[gpui::test]
fn right_clicking_an_answer_or_a_prompt_opens_the_message_menu(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("rc-menus", cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let answer = cx.debug_bounds("transcript-answer").expect("the answer");
    right_click(cx, answer.center());
    assert!(cx.debug_bounds("answer-menu").is_some(), "the message menu");
    for row in [0, 1, 3, 4, 6] {
        assert!(
            cx.debug_bounds(format!("answer-menu-row-{row}").leak())
                .is_some(),
            "answer row {row}"
        );
    }
    assert!(cx.debug_bounds("answer-menu-row-7").is_none());
    let pin = cx.debug_bounds("answer-menu-row-4").unwrap();
    cx.simulate_click(pin.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("answer-chapter").is_some(), "pinned");

    // A prompt: the same menu with its own rows.
    view.update(cx, |view, _| view.cockpit.send(thread, "again".into()));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "bravo\n\n".into(),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    let prompt = cx.debug_bounds("transcript-prompt").expect("the prompt");
    right_click(cx, prompt.center());
    for row in [0, 1, 3, 5, 6] {
        assert!(
            cx.debug_bounds(format!("answer-menu-row-{row}").leak())
                .is_some(),
            "prompt row {row}"
        );
    }
    assert!(cx.debug_bounds("answer-menu-row-4").is_none(), "no Pin");
    let copy = cx.debug_bounds("answer-menu-row-0").unwrap();
    cx.simulate_click(copy.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()).as_deref(),
        Some("again")
    );
}
