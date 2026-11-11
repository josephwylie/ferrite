//! The side chat (Claude's): quick questions about a Thread, each a Panel
//! tab (ADR 0039) with its own fork, the transcript reflowing narrower
//! beside it — and nothing in the nav, the board or the palette.
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

/// `owner`'s side chat Surfaces, in tab order.
fn side_tabs(view: &Entity<CockpitView>, owner: ThreadId, cx: &mut gpui::VisualTestContext) -> Vec<u64> {
    panel_tabs(view, owner, cx)
        .0
        .into_iter()
        .filter_map(|id| match id {
            crate::cockpit::surfaces::SurfaceId::SideChat(id) => Some(id),
            _ => None,
        })
        .collect()
}

/// Opened, a side chat is a Panel tab (ADR 0039) at least its token wide,
/// the thread reflowing narrower beside it; an empty tab says `No messages
/// yet`; the composer reads `Ask a quick question` — unless the chat
/// opened over a selection, which it quotes. The fork is no Pane: nothing
/// of the nav's or the board's.
#[gpui::test]
fn a_side_chat_opens_as_a_panel_tab_with_an_empty_word(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-open", cx);
    let before = cx.debug_bounds("board").expect("the board").size.width;
    open_chat(&view, cx, None);
    assert_eq!(fake.forks.borrow().len(), 1, "the agent is asked for a fork");

    let panel = cx.debug_bounds("panel").expect("the Panel shows it");
    let pane = cx.debug_bounds("side-chat").expect("the pane draws");
    assert!(panel.contains(&pane.center()));
    assert!(
        panel.size.width >= px(ferrite_ui::theme::SIDE_CHAT_W) - px(1.),
        "its floor: {panel:?}"
    );
    assert!((panel.right() - px(1600.)).abs() < px(1.), "at the window's right: {panel:?}");
    let board = cx.debug_bounds("board").expect("the board");
    assert!(
        board.size.width < before && board.right() <= panel.left() + px(1.),
        "the thread reflows beside it: {before:?} -> {board:?}"
    );
    let owner = owner(&view, cx);
    let ids = side_tabs(&view, owner, cx);
    assert_eq!(ids.len(), 1);
    let tab = debug_bounds(cx, format!("panel-tab-SideChat-{}", ids[0]))
        .expect("its tab in the head row");
    assert!(panel.contains(&tab.center()));
    assert_eq!(
        panel_tabs(&view, owner, cx).1,
        Some(crate::cockpit::surfaces::SurfaceId::SideChat(ids[0]))
    );
    assert!(cx.debug_bounds("side-chat-empty").is_some(), "No messages yet");
    assert!(cx.debug_bounds("side-chat-placeholder").is_some());

    view.read_with(cx, |view, cx| {
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

/// A second chat is a second tab, `Side chat 2`, its own fork; a tab
/// switches the conversation shown; closing a tab takes it and its fork
/// away, and the last leaves the Panel to its other Surfaces.
#[gpui::test]
fn side_chat_tabs_are_separate_conversations(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-tabs", cx);
    open_chat(&view, cx, None);
    open_chat(&view, cx, None);
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
    let ids = side_tabs(&view, owner, cx);
    assert_eq!(ids.len(), 2, "two tabs in the head row");

    // The first tab is its own conversation: its field is its own.
    view.update(cx, |view, cx| {
        let field = view.side_chats.field_of(owner, 1).expect("tab 2's field").clone();
        field.update(cx, |field, cx| field.insert("second", cx));
    });
    let first = debug_bounds(cx, format!("panel-tab-SideChat-{}", ids[0])).unwrap();
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.side_chats.titles(owner).1, 0);
        assert_eq!(view.side_chats.draft(owner, cx).as_deref(), Some(""));
    });

    // Closing the shown tab leaves the other.
    let forks = view.read_with(cx, |view, _| view.side_chats.threads(owner));
    let first = crate::cockpit::surfaces::SurfaceId::SideChat(ids[0]);
    view.update(cx, |view, cx| view.close_surface(owner, &first, cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.side_chats.titles(owner).0, ["Side chat 2"]);
        assert!(!view.side_chats.owns(forks[0]), "its fork is retired");
        assert!(view.side_chats.owns(forks[1]));
    });

    // The last closed, no side chat draws.
    let last = crate::cockpit::surfaces::SurfaceId::SideChat(ids[1]);
    view.update(cx, |view, cx| view.close_surface(owner, &last, cx));
    tick(cx);
    assert!(cx.debug_bounds("side-chat").is_none());
    assert!(view.read_with(cx, |view, _| view.side_chats.titles(owner).0.is_empty()));
    assert!(side_tabs(&view, owner, cx).is_empty());
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
