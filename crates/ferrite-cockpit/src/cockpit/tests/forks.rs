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

/// The answer's `⋯`: Copy message, Reply and Fork from here; Copy copies
/// the answer's Markdown.
#[gpui::test]
fn an_answers_menu_copies_replies_and_forks(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("answer-menu", cx);
    let more = cx.debug_bounds("answer-more").expect("⋯");
    cx.simulate_click(more.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("answer-menu").is_some(), "the menu opens");
    for row in 0..2 {
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

    // Reply quotes the whole answer over the Composer.
    cx.simulate_click(more.center(), gpui::Modifiers::none());
    tick(cx);
    let reply = cx.debug_bounds("answer-menu-row-1").unwrap();
    cx.simulate_click(reply.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[0].composer.read(cx).quotes(), ["alpha"]);
    });

    // Fork from here, past the rule.
    cx.simulate_click(more.center(), gpui::Modifiers::none());
    tick(cx);
    let fork = cx
        .debug_bounds("answer-menu-row-3")
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
