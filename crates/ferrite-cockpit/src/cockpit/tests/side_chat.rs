//! The side chat (Claude's): a right-docked, tabbed pane of quick questions
//! about a Thread — the transcript reflowing narrower beside it, each tab
//! its own fork, expand and close instant — and nothing in the nav, the
//! board or the palette.
#[allow(unused_imports)]
use super::*;

/// One Thread whose agent can fork, holding a finished answer.
fn answered<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, Fake, &'a mut gpui::VisualTestContext) {
    let (core, fake) = cockpit(name, 1);
    *fake.native_controls.borrow_mut() = true;
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(700.)));
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

fn open_chat(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext, passage: Option<&str>) {
    view.update(cx, |view, cx| {
        let owner = view.focused_thread().unwrap();
        view.open_side_chat(owner, passage.map(str::to_string), cx);
    });
    tick(cx);
    tick(cx);
}

fn owner(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> ferrite_core::ThreadId {
    view.read_with(cx, |view, _| view.focused_thread().unwrap())
}

/// Opened, the pane docks at the right at its token width, the thread
/// reflowing narrower beside it; its head holds the tab, `+`, expand and
/// close; an empty tab says `No messages yet`; the composer reads `Ask a
/// quick question` — unless the chat opened over a selection, which it
/// quotes. The fork is no Pane: nothing of the nav's or the board's.
#[gpui::test]
fn a_side_chat_docks_right_with_tabs_and_an_empty_word(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-open", cx);
    let before = cx.debug_bounds("board").expect("the board").size.width;
    open_chat(&view, cx, None);
    assert_eq!(
        fake.forks.borrow().len(),
        1,
        "the agent is asked for a fork"
    );

    let pane = cx.debug_bounds("side-chat").expect("the pane draws");
    assert!(
        (pane.size.width - px(ferrite_ui::theme::SIDE_CHAT_W)).abs() < px(1.),
        "its token width: {pane:?}"
    );
    assert!(
        (pane.right() - px(1600.)).abs() < px(1.),
        "docked at the window's right: {pane:?}"
    );
    let board = cx.debug_bounds("board").expect("the board");
    assert!(
        board.size.width < before && board.right() <= pane.left() + px(1.),
        "the thread reflows beside it: {before:?} -> {board:?}"
    );
    for part in [
        "side-chat-tab-0",
        "side-chat-new",
        "side-chat-expand",
        "side-chat-close",
    ] {
        let part = cx.debug_bounds(part).unwrap_or_else(|| panic!("{part}"));
        assert!(pane.contains(&part.center()));
    }
    let tab = cx.debug_bounds("side-chat-tab-0").unwrap();
    let new = cx.debug_bounds("side-chat-new").unwrap();
    let expand = cx.debug_bounds("side-chat-expand").unwrap();
    let close = cx.debug_bounds("side-chat-close").unwrap();
    assert!(tab.right() <= new.left() && new.right() <= expand.left());
    assert!(
        expand.right() <= close.left(),
        "expand, then close, at the right"
    );
    assert!(
        cx.debug_bounds("side-chat-empty").is_some(),
        "No messages yet"
    );
    assert!(cx.debug_bounds("side-chat-placeholder").is_some());
    assert!(
        cx.debug_bounds("panel-toggle").is_some(),
        "the rail's door stands against the pane"
    );

    view.read_with(cx, |view, cx| {
        let owner = view.focused_thread().unwrap();
        assert_eq!(view.panes.len(), 1, "the fork is no Pane");
        assert_eq!(view.cockpit.threads().len(), 2);
        let fork = view.side_thread(owner).expect("the tab took the fork");
        assert!(view.side_chats.owns(fork));
        assert_eq!(view.focused_thread(), Some(owner), "the window stays");
        assert_eq!(view.side_chats.draft(owner, cx).as_deref(), Some(""));
    });
}

/// A selection's chat quotes it as `> text` ahead of the caret.
#[gpui::test]
fn a_side_chat_opened_over_a_selection_quotes_it(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("side-chat-quote", cx);
    open_chat(&view, cx, Some("two\nlines"));
    let owner = owner(&view, cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.side_chats.draft(owner, cx).as_deref(),
            Some("> two\n> lines\n\n")
        );
    });
    assert!(
        cx.debug_bounds("side-chat-placeholder").is_none(),
        "a quoted field has no placeholder"
    );
}

/// `+` adds `Side chat 2`, its own fork; a tab switches the conversation
/// shown; a tab's × takes it and its fork away, the last closing the pane.
#[gpui::test]
fn side_chat_tabs_are_separate_conversations(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-tabs", cx);
    open_chat(&view, cx, None);
    let plus = cx.debug_bounds("side-chat-new").unwrap();
    cx.simulate_click(plus.center(), gpui::Modifiers::none());
    tick(cx);
    tick(cx);
    let owner = owner(&view, cx);
    view.read_with(cx, |view, _| {
        let (titles, active) = view.side_chats.titles(owner);
        assert_eq!(titles, ["Side chat", "Side chat 2"]);
        assert_eq!(active, 1, "the new tab is shown");
        let forks = view.side_chats.threads(owner);
        assert_eq!(forks.len(), 2, "each tab its own fork");
        assert_ne!(forks[0], forks[1]);
    });
    assert_eq!(fake.forks.borrow().len(), 2);
    assert!(cx.debug_bounds("side-chat-tab-1").is_some());

    // The first tab is its own conversation: its field is its own.
    view.update(cx, |view, cx| {
        let field = view
            .side_chats
            .field_of(owner, 1)
            .expect("tab 2's field")
            .clone();
        field.update(cx, |field, cx| field.insert("second", cx));
    });
    let first = cx.debug_bounds("side-chat-tab-0").unwrap();
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.side_chats.titles(owner).1, 0);
        assert_eq!(view.side_chats.draft(owner, cx).as_deref(), Some(""));
    });

    // Closing the shown tab leaves the other.
    let forks = view.read_with(cx, |view, _| view.side_chats.threads(owner));
    let close = cx.debug_bounds("side-chat-tab-0-close").unwrap();
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.side_chats.titles(owner).0, ["Side chat 2"]);
        assert!(!view.side_chats.owns(forks[0]), "its fork is retired");
        assert!(view.side_chats.owns(forks[1]));
    });

    // The last tab's × closes the pane.
    let close = cx.debug_bounds("side-chat-tab-0-close").unwrap();
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("side-chat").is_none());
    view.read_with(cx, |view, _| assert!(!view.side_chats.is_open(owner)));
}

/// Expand covers the content area (the thread under it), the same control
/// collapses it; close takes the pane away and the thread has its width
/// back. Each is one frame: no ride.
#[gpui::test]
fn a_side_chat_expands_over_the_thread_and_closes(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("side-chat-expand", cx);
    open_chat(&view, cx, None);
    let docked = cx.debug_bounds("side-chat").unwrap();
    let board = cx.debug_bounds("board").unwrap();

    let expand = cx.debug_bounds("side-chat-expand").unwrap();
    cx.simulate_click(expand.center(), gpui::Modifiers::none());
    tick(cx);
    let wide = cx.debug_bounds("side-chat").unwrap();
    assert!(
        wide.left() <= board.left() + px(1.) && wide.right() >= docked.right() - px(1.),
        "it covers the content area: {wide:?} over {board:?}"
    );
    assert!(
        cx.debug_bounds("title-tools").is_none(),
        "the thread's tools go under it"
    );

    let collapse = cx.debug_bounds("side-chat-expand").unwrap();
    cx.simulate_click(collapse.center(), gpui::Modifiers::none());
    tick(cx);
    let back = cx.debug_bounds("side-chat").unwrap();
    assert!(
        (back.size.width - docked.size.width).abs() < px(1.),
        "collapsed: {back:?}"
    );

    let close = cx.debug_bounds("side-chat-close").unwrap();
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("side-chat").is_none());
    let after = cx.debug_bounds("board").unwrap();
    assert!(
        after.size.width > board.size.width,
        "the thread has its width back"
    );
}

/// A question typed in a tab goes to its fork alone — never the Thread —
/// and the tab then shows the fork's own words, not the history it
/// inherited.
#[gpui::test]
fn a_side_chat_question_goes_to_its_fork_and_shows_only_the_new_words(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-ask", cx);
    open_chat(&view, cx, None);
    let owner = owner(&view, cx);
    let fork = view
        .read_with(cx, |view, _| view.side_thread(owner))
        .unwrap();
    let sent_before = fake.sent.borrow().len();

    cx.simulate_input("what is alpha?");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(fake.sent.borrow().len(), sent_before + 1);
    assert_eq!(
        fake.sent.borrow().last().map(String::as_str),
        Some("what is alpha?")
    );
    view.read_with(cx, |view, cx| {
        assert_eq!(view.side_chats.draft(owner, cx).as_deref(), Some(""));
        let open = view.cockpit.thread(fork).expect("the fork is open");
        assert!(
            open.transcript().blocks().iter().any(|block| matches!(
                &block.body,
                ferrite_core::transcript::Body::Paragraph { .. }
            )),
            "the fork holds the Thread's history too"
        );
        let main = view.cockpit.thread(owner).unwrap();
        assert!(
            !main
                .transcript()
                .blocks()
                .iter()
                .any(|block| block.prompt_id.is_some() && block_text(block) == "what is alpha?"),
            "the Thread is unchanged"
        );
    });

    // The fork's stream answers; its words show, the history does not.
    fake.streams.borrow()[1]
        .send(SessionEvent::TextDelta {
            text: "Alpha is the first letter.\n\n".into(),
        })
        .unwrap();
    tick(cx);
    tick(cx);
    assert!(
        cx.debug_bounds("side-chat-empty").is_none(),
        "no longer empty"
    );
}

fn block_text(block: &ferrite_core::transcript::Block) -> String {
    match &block.body {
        ferrite_core::transcript::Body::Prompt(text) => text.clone(),
        _ => String::new(),
    }
}
