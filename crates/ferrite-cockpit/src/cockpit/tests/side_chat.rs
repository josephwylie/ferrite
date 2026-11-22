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

fn open_chat(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| {
        let owner = view.focused_thread(cx).unwrap();
        view.open_side_chat(owner, cx);
    });
    tick(cx);
    tick(cx);
}

fn owner(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> ferrite_core::ThreadId {
    view.read_with(cx, |view, cx| view.focused_thread(cx).unwrap())
}

/// `owner`'s side chat Surfaces, in tab order.
pub(super) fn side_tabs(
    view: &Entity<CockpitView>,
    owner: ThreadId,
    cx: &mut gpui::VisualTestContext,
) -> Vec<u64> {
    panel_tabs(view, owner, cx)
        .0
        .into_iter()
        .filter_map(|id| match id {
            crate::cockpit::surfaces::SurfaceId::SideChat(id) => Some(id),
            _ => None,
        })
        .collect()
}

/// What the cases read of the side chat, held here so a move of its
/// state changes these and not the cases.
pub(super) fn draft(view: &CockpitView, owner: ThreadId, cx: &gpui::App) -> Option<String> {
    view.side_chat.read(cx).draft(owner, cx)
}

fn titles(view: &CockpitView, owner: ThreadId, cx: &gpui::App) -> (Vec<SharedString>, usize) {
    view.side_chat.read(cx).titles(owner)
}

fn forks(view: &CockpitView, owner: ThreadId, cx: &gpui::App) -> Vec<ThreadId> {
    view.side_chat.read(cx).threads(owner)
}

fn owns(view: &CockpitView, thread: ThreadId, cx: &gpui::App) -> bool {
    view.side_chat.read(cx).owns(thread)
}

/// The shown tab's fork, once it has one.
pub(super) fn shown_fork(view: &CockpitView, owner: ThreadId, cx: &gpui::App) -> Option<ThreadId> {
    view.side_chat.read(cx).shown_fork(owner)
}

fn field_of(
    view: &CockpitView,
    owner: ThreadId,
    index: usize,
    cx: &gpui::App,
) -> Option<Entity<ferrite_pane::composer::Composer>> {
    view.side_chat.read(cx).field_of(owner, index).cloned()
}

fn bring_back_blocked(view: &CockpitView, owner: ThreadId, id: u64, cx: &gpui::App) -> Option<&'static str> {
    view.bring_back_blocked(owner, view.side_chat.read(cx).fork(owner, id), cx)
}

fn shown_pane<R>(
    view: &CockpitView,
    owner: ThreadId,
    cx: &gpui::App,
    read: impl FnOnce(&PaneView, &gpui::App) -> R,
) -> Option<R> {
    Some(read(view.side_chat.read(cx).pane(owner)?, cx))
}

/// Opened, a side chat is a Panel tab (ADR 0039) at least its token wide,
/// the thread reflowing narrower beside it; an empty tab says `No messages
/// yet`; the composer reads `Ask a quick question`. The fork is no Pane:
/// nothing of the nav's or the board's.
#[gpui::test]
fn a_side_chat_opens_as_a_panel_tab_with_an_empty_word(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-open", cx);
    let before = cx.debug_bounds("board").expect("the board").size.width;
    open_chat(&view, cx);
    assert_eq!(fake.forks.borrow().len(), 1, "the agent is asked for a fork");

    let panel = cx.debug_bounds("panel").expect("the Panel shows it");
    let pane = cx.debug_bounds("side-chat").expect("the pane draws");
    assert!(panel.contains(&pane.center()));
    assert!(
        panel.size.width >= px(ferrite_ui::theme::SIDE_CHAT_W + ferrite_ui::theme::CAPTION_ALLOWANCE) - px(1.),
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
        assert_eq!(view.cockpit.read(cx).threads().len(), 2);
        let fork = shown_fork(view, owner, cx).expect("the tab took the fork");
        assert!(owns(view, fork, cx));
        assert_eq!(view.focused_thread(cx), Some(owner), "the window stays");
        assert_eq!(draft(view, owner, cx).as_deref(), Some(""));
    });
}

/// A second chat is a second tab, `Side chat 2`, its own fork; a tab
/// switches the conversation shown; closing a tab takes it and its fork
/// away, and the last leaves the Panel to its other Surfaces.
#[gpui::test]
fn side_chat_tabs_are_separate_conversations(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-tabs", cx);
    open_chat(&view, cx);
    open_chat(&view, cx);
    let owner = owner(&view, cx);
    view.read_with(cx, |view, cx| {
        let (titles, active) = titles(view, owner, cx);
        assert_eq!(titles, ["Side chat", "Side chat 2"]);
        assert_eq!(active, 1, "the new tab is shown");
        let forks = forks(view, owner, cx);
        assert_eq!(forks.len(), 2, "each tab its own fork");
        assert_ne!(forks[0], forks[1]);
    });
    assert_eq!(fake.forks.borrow().len(), 2);
    let ids = side_tabs(&view, owner, cx);
    assert_eq!(ids.len(), 2, "two tabs in the head row");

    // The first tab is its own conversation: its field is its own.
    view.update(cx, |view, cx| {
        let field = field_of(view, owner, 1, cx).expect("tab 2's field");
        field.update(cx, |field, cx| field.insert("second", cx));
    });
    let first = debug_bounds(cx, format!("panel-tab-SideChat-{}", ids[0])).unwrap();
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(titles(view, owner, cx).1, 0);
        assert_eq!(draft(view, owner, cx).as_deref(), Some(""));
    });

    // Closing the shown tab leaves the other.
    let forks = view.read_with(cx, |view, cx| forks(view, owner, cx));
    let first = crate::cockpit::surfaces::SurfaceId::SideChat(ids[0]);
    view.update(cx, |view, cx| view.close_surface(view.cockpit.read(cx).thread_key(owner), &first, cx));
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(titles(view, owner, cx).0, ["Side chat 2"]);
        assert!(!owns(view, forks[0], cx), "its fork is retired");
        assert!(owns(view, forks[1], cx));
    });

    // The last closed, no side chat draws.
    let last = crate::cockpit::surfaces::SurfaceId::SideChat(ids[1]);
    view.update(cx, |view, cx| view.close_surface(view.cockpit.read(cx).thread_key(owner), &last, cx));
    tick(cx);
    assert!(cx.debug_bounds("side-chat").is_none());
    assert!(view.read_with(cx, |view, cx| titles(view, owner, cx).0.is_empty()));
    assert!(side_tabs(&view, owner, cx).is_empty());
}

#[gpui::test]
fn a_press_on_the_side_chat_field_takes_the_keys_back_from_the_composer(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-pressed", cx);
    open_chat(&view, cx);
    let composer = cx.debug_bounds("composer-block").expect("the Composer");
    cx.simulate_click(composer.center(), gpui::Modifiers::none());
    tick(cx);
    let field = cx.debug_bounds("side-chat-placeholder").expect("the side chat field");
    cx.simulate_click(field.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("what is alpha?");
    assert_eq!(composer_text(&view, cx), "");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(fake.sent.borrow().last().map(String::as_str), Some("what is alpha?"));
}

/// The field's card is all one press target, as the Composer's box is: a
/// press off its text line, by the send glyph, still takes the keys.
#[gpui::test]
fn a_press_anywhere_on_the_side_chat_card_takes_the_keys(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-card-pressed", cx);
    open_chat(&view, cx);
    let composer = cx.debug_bounds("composer-block").expect("the Composer");
    cx.simulate_click(composer.center(), gpui::Modifiers::none());
    tick(cx);
    let card = cx.debug_bounds("side-chat-composer").expect("the side chat card");
    let line = cx.debug_bounds("side-chat-placeholder").expect("the text line");
    let below_line = gpui::point(card.left() + px(8.), card.bottom() - px(6.));
    assert!(below_line.y > line.bottom(), "the press is off the text line");
    cx.simulate_click(below_line, gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("what is alpha?");
    assert_eq!(composer_text(&view, cx), "", "nothing typed into the Composer");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(fake.sent.borrow().last().map(String::as_str), Some("what is alpha?"));
}

/// A question typed in a tab goes to its fork alone — never the Thread —
/// and the tab then shows the fork's own words, not the history it
/// inherited.
#[gpui::test]
fn a_side_chat_question_goes_to_its_fork_and_shows_only_the_new_words(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-ask", cx);
    open_chat(&view, cx);
    let owner = owner(&view, cx);
    let fork = view
        .read_with(cx, |view, cx| shown_fork(view, owner, cx))
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
        assert_eq!(draft(view, owner, cx).as_deref(), Some(""));
        let open = view.cockpit.read(cx).thread(fork).expect("the fork is open");
        assert!(
            open.transcript().blocks().iter().any(|block| matches!(
                &block.body,
                ferrite_core::transcript::Body::Paragraph { .. }
            )),
            "the fork holds the Thread's history too"
        );
        let main = view.cockpit.read(cx).thread(owner).unwrap();
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

/// The side chat's head (T3's): 36 tall under the tab strip, its title,
/// Bring back and Trash. Bring back waits for a finished reply, then puts
/// the side chat's run into the main Thread's next message; Trash takes
/// the chat away, a second press first when deletes are confirmed.
#[gpui::test]
fn the_side_chat_head_brings_back_and_trashes(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-head", cx);
    open_chat(&view, cx);
    let owner = owner(&view, cx);
    let id = side_tabs(&view, owner, cx)[0];
    let head = cx.debug_bounds("side-chat-head").expect("its head");
    assert_eq!(head.size.height, px(ferrite_ui::theme::SURFACE_BAR_H));
    let blocked = view.read_with(cx, |view, cx| bring_back_blocked(view, owner, id, cx));
    assert_eq!(blocked, Some("Available after a finished reply."));

    cx.simulate_input("what is alpha?");
    cx.simulate_keystrokes("enter");
    tick(cx);
    let send = |event: SessionEvent| fake.streams.borrow()[1].send(event).unwrap();
    send(SessionEvent::TextDelta {
        text: "Alpha is the first letter.\n\n".into(),
    });
    tick(cx);
    let blocked = view.read_with(cx, |view, cx| bring_back_blocked(view, owner, id, cx));
    assert_eq!(blocked, Some("Available after a finished reply."), "still answering");
    let dot = format!("panel-tab-SideChat-{id}-pending");
    assert!(debug_bounds(cx, dot.clone()).is_some(), "its tab's pending dot");
    send(SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    });
    tick(cx);
    assert!(debug_bounds(cx, dot).is_none(), "answered: no dot");
    assert_eq!(view.read_with(cx, |view, cx| bring_back_blocked(view, owner, id, cx)), None);
    let back = cx.debug_bounds("side-chat-bring-back").unwrap();
    cx.simulate_click(back.center(), gpui::Modifiers::none());
    tick(cx);
    let composer = composer_text(&view, cx);
    assert!(composer.starts_with("> From a side chat:"), "{composer:?}");
    assert!(composer.contains("Alpha is the first letter."), "{composer:?}");

    view.update(cx, |view, _| view.prefs.settings.confirm_delete = true);
    let trash = cx.debug_bounds("side-chat-trash").unwrap();
    cx.simulate_click(trash.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(side_tabs(&view, owner, cx), [id], "armed: still there");
    cx.simulate_click(trash.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(side_tabs(&view, owner, cx).is_empty(), "the second press deletes it");
}

/// A tab pressed in the head row hands its field the keyboard, and
/// closing the tab shown hands it to the tab left.
#[gpui::test]
fn the_side_chat_tab_shown_takes_the_keys(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("side-chat-tab-keys", cx);
    open_chat(&view, cx);
    open_chat(&view, cx);
    let owner = owner(&view, cx);
    let ids = side_tabs(&view, owner, cx);
    let first = debug_bounds(cx, format!("panel-tab-SideChat-{}", ids[0])).unwrap();
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("first");
    view.read_with(cx, |view, cx| {
        assert_eq!(draft(view, owner, cx).as_deref(), Some("first"), "the tab pressed");
        let second = field_of(view, owner, 1, cx).unwrap();
        assert_eq!(second.read(cx).text(), "", "not the other");
    });
    assert_eq!(composer_text(&view, cx), "", "nor the Composer");

    let shown = crate::cockpit::surfaces::SurfaceId::SideChat(ids[0]);
    view.update(cx, |view, cx| view.close_surface(view.cockpit.read(cx).thread_key(owner), &shown, cx));
    tick(cx);
    cx.simulate_input("left");
    view.read_with(cx, |view, cx| {
        assert_eq!(draft(view, owner, cx).as_deref(), Some("left"), "the tab left");
    });
}

/// A side chat opened from the `+` menu has the keyboard before the next
/// key: what is typed after the menu's `s` goes to its field, none of it to
/// the Composer.
#[gpui::test]
fn a_side_chat_opened_from_the_plus_menu_takes_the_next_key(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("side-chat-plus-keys", cx);
    let owner = owner(&view, cx);
    view.update(cx, |view, cx| view.open_surface_menu(cx));
    tick(cx);
    cx.simulate_keystrokes("s");
    assert_eq!(side_tabs(&view, owner, cx).len(), 1, "s opens a side chat");
    cx.simulate_keystrokes("x");
    assert_eq!(composer_text(&view, cx), "", "the Composer did not take the letter");
    view.read_with(cx, |view, cx| {
        assert_eq!(draft(view, owner, cx).as_deref(), Some("x"), "the side chat's field did");
    });

    // A second one, while the first holds the keyboard.
    view.update(cx, |view, cx| view.open_surface_menu(cx));
    tick(cx);
    cx.simulate_keystrokes("s");
    assert_eq!(side_tabs(&view, owner, cx).len(), 2, "s opens another");
    cx.simulate_keystrokes("y");
    assert_eq!(composer_text(&view, cx), "", "the Composer did not take the second letter");
    view.read_with(cx, |view, cx| {
        let first = field_of(view, owner, 0, cx).unwrap();
        let second = field_of(view, owner, 1, cx).unwrap();
        assert_eq!(first.read(cx).text(), "x", "the first field kept its own");
        assert_eq!(second.read(cx).text(), "y", "the new field took the letter");
    });
}

/// Deleting a Thread ends its side chats as closing their tabs does: each
/// fork is retired, none left running where nothing can reach it.
#[gpui::test]
fn deleting_a_thread_retires_its_side_chats_forks(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("side-chat-owner-deleted", cx);
    open_chat(&view, cx);
    let owner = owner(&view, cx);
    let fork = view
        .read_with(cx, |view, cx| shown_fork(view, owner, cx))
        .expect("the side chat has its fork");
    press_nav_verb(&view, owner, MenuVerb::Delete, cx);
    view.read_with(cx, |view, cx| {
        assert!(view.cockpit.read(cx).thread(owner).is_none(), "the Thread is gone");
        assert!(view.cockpit.read(cx).thread(fork).is_none(), "its side chat's fork went with it");
        assert!(!owns(view, fork, cx), "and the side chat with it");
    });
}

/// ⌘W in the field ends its side chat, not the Thread, and the fork goes.
#[gpui::test]
fn cmd_w_in_the_side_chat_field_ends_its_tab(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("side-chat-cmd-w", cx);
    open_chat(&view, cx);
    let owner = owner(&view, cx);
    assert_eq!(side_tabs(&view, owner, cx).len(), 1);
    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    assert!(side_tabs(&view, owner, cx).is_empty(), "the tab is gone");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), Some(owner), "the Thread stays");
        assert_eq!(view.cockpit.read(cx).threads(), [owner], "the fork is retired");
    });
}

/// The palette lists the Thread and never its side chat's fork.
#[gpui::test]
fn the_palette_lists_no_side_chat_fork(cx: &mut TestAppContext) {
    let (view, _fake, cx) = answered("side-chat-palette", cx);
    open_chat(&view, cx);
    let owner = owner(&view, cx);
    let fork = view.read_with(cx, |view, cx| shown_fork(view, owner, cx)).unwrap();
    let rows = cx.update(|_, cx| view.read(cx).palette_thread_rows(cx));
    let act = |thread| crate::palette::Act::Thread(thread);
    assert!(rows.iter().any(|row| row.act == act(owner)));
    assert!(!rows.iter().any(|row| row.act == act(fork)), "no row for the fork");
}

/// A tool row in a side chat's answer folds and unfolds there.
#[gpui::test]
fn a_tool_row_in_a_side_chat_unfolds_in_its_tab(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-tool", cx);
    open_chat(&view, cx);
    let owner = owner(&view, cx);
    cx.simulate_input("what is alpha?");
    cx.simulate_keystrokes("enter");
    tick(cx);
    fake.streams.borrow()[1]
        .send(SessionEvent::ToolStarted {
            id: "side-tool".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "cat <<'EOF'\nlet answer = 42;\nEOF" }),
        })
        .unwrap();
    fake.streams.borrow()[1]
        .send(SessionEvent::TextDelta {
            text: "Next file.\n\n".into(),
        })
        .unwrap();
    fake.streams.borrow()[1]
        .send(SessionEvent::ToolStarted {
            id: "next".into(),
            name: "Read".into(),
            input: serde_json::json!({ "file_path": "next.rs" }),
        })
        .unwrap();
    tick(cx);
    tick(cx);
    let chevron = view
        .read_with(cx, |view, cx| {
            shown_pane(view, owner, cx, |pane, cx| pane.tool_bounds("side-tool", cx)).flatten()
        })
        .expect("the tool row draws in the tab")
        .center();
    let expanded = |view: &CockpitView, cx: &gpui::App| {
        shown_pane(view, owner, cx, |pane, _| pane.tool_expanded("side-tool"))
    };
    assert_eq!(view.read_with(cx, |view, cx| expanded(view, cx)), Some(false));
    cx.simulate_click(chevron, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |view, cx| expanded(view, cx)), Some(true));
}

/// Whether `thread`'s transcript holds `text` anywhere.
fn holds(view: &CockpitView, thread: ThreadId, text: &str, cx: &gpui::App) -> bool {
    view.cockpit.read(cx).thread(thread).is_some_and(|open| {
        ferrite_core::transcript::plain_text(open.transcript().blocks()).contains(text)
    })
}

/// A tab closed before its fork lands takes that fork with it: the window
/// stays on the Thread, one Pane, and the fork is retired.
#[gpui::test]
fn a_fork_that_lands_after_its_tab_closed_is_retired(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-closed-early", cx);
    let owner = owner(&view, cx);
    view.update(cx, |view, cx| view.open_side_chat(owner, cx));
    let id = side_tabs(&view, owner, cx)[0];
    assert!(view.read_with(cx, |view, cx| shown_fork(view, owner, cx)).is_none(), "not landed yet");
    let tab = crate::cockpit::surfaces::SurfaceId::SideChat(id);
    view.update(cx, |view, cx| view.close_surface(view.cockpit.read(cx).thread_key(owner), &tab, cx));
    tick(cx);
    tick(cx);
    assert_eq!(fake.forks.borrow().len(), 1, "the fork was asked");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes.len(), 1, "the fork is no Pane");
        assert_eq!(view.focused_thread(cx), Some(owner), "the window stays");
        assert_eq!(view.cockpit.read(cx).threads(), [owner], "the fork is retired");
    });
}

/// Two Threads' side chats wait on forks; the first tab closes before
/// either lands. Each fork goes to the tab of the Thread it was made from:
/// the closed tab's is retired, the other tab takes its own.
#[gpui::test]
fn each_fork_lands_in_the_tab_of_the_thread_it_forks(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("side-chat-two-owners", 2);
    *fake.native_controls.borrow_mut() = true;
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(700.)));
    let threads = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads());
    let (first, second) = (threads[0], threads[1]);
    // A Thread's agent is heard once the window has landed on it.
    for (stream, thread, text) in [(1, second, "beta\n\n"), (0, first, "alpha\n\n")] {
        view.update(cx, |view, cx| view.focus_thread(thread, cx));
        let send = |event| fake.streams.borrow()[stream].send(event).unwrap();
        send(SessionEvent::TextDelta { text: text.into() });
        send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        });
        tick(cx);
    }
    view.read_with(cx, |view, cx| {
        assert!(holds(view, first, "alpha", cx) && holds(view, second, "beta", cx));
    });
    view.update(cx, |view, cx| {
        view.open_side_chat(first, cx);
        view.open_side_chat(second, cx);
    });
    let tab = crate::cockpit::surfaces::SurfaceId::SideChat(side_tabs(&view, first, cx)[0]);
    view.update(cx, |view, cx| view.close_surface(view.cockpit.read(cx).thread_key(first), &tab, cx));
    tick(cx);
    tick(cx);
    assert_eq!(fake.forks.borrow().len(), 2, "both forks were asked");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes.len(), 2, "no fork is a Pane");
        assert_eq!(view.focused_thread(cx), Some(first), "the window stays");
        let fork = shown_fork(view, second, cx).expect("the second tab took a fork");
        assert!(holds(view, fork, "beta", cx), "the second Thread's own fork");
        assert_eq!(view.cockpit.read(cx).threads().len(), 3, "the first fork is retired");
    });
}

/// A script asks the windows to land on another Thread (`thread.focus`)
/// while a side chat waits on its fork: the window lands there, and the
/// fork still goes to its tab.
#[gpui::test]
fn a_focus_asked_while_a_fork_is_on_its_way_lands_the_window(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-focus-meanwhile", cx);
    let owner = owner(&view, cx);
    assert!(
        view.read_with(cx, |view, cx| view.cockpit.read(cx).names_fork_owners()),
        "a Server that names a fork's owner"
    );
    let other = view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.open(Provider::Claude, WorkspaceChoice::Main { checkout: here() }))
            .unwrap()
    });
    tick(cx);
    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_task_alert(ferrite_core::protocol::ServerMessage::Focus {
                thread: other,
                fork_of: None,
            }));
        view.open_side_chat(owner, cx);
    });
    tick(cx);
    tick(cx);
    assert_eq!(fake.forks.borrow().len(), 1, "the fork was asked");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), Some(other), "the window lands where it was asked");
        assert!(view.pane_for(other).is_some(), "the other Thread keeps its Pane");
        let fork = shown_fork(view, owner, cx).expect("the tab took its fork");
        assert!(holds(view, fork, "alpha", cx), "the owner's own fork");
    });
}

/// A Server from before `fork_of` (one an update leaves running until
/// its agents are idle) says a fork opened without naming its owner: the
/// one side chat waiting still takes it, and the window stays.
#[gpui::test]
fn a_server_from_before_fork_of_still_gives_the_waiting_tab_its_fork(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-old-server", cx);
    let owner = owner(&view, cx);
    view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| cockpit.fixture_server_before_fork_of()));
    open_chat(&view, cx);
    assert_eq!(fake.forks.borrow().len(), 1, "the fork was asked");
    view.read_with(cx, |view, cx| {
        assert_eq!(view.focused_thread(cx), Some(owner), "the window stays");
        assert_eq!(view.panes.len(), 1, "the fork is no Pane");
        let fork = shown_fork(view, owner, cx).expect("the tab took its fork");
        assert!(holds(view, fork, "alpha", cx), "the owner's own fork");
    });
}

/// Two Threads' side chats wait on a Server from before `fork_of`: a
/// fork that names no owner could be either's, so neither tab takes it
/// and it lands as any fork does.
#[gpui::test]
fn a_server_from_before_fork_of_guesses_no_owner_between_two(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("side-chat-old-server-two", 2);
    *fake.native_controls.borrow_mut() = true;
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(700.)));
    view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| cockpit.fixture_server_before_fork_of()));
    let threads = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads());
    let (first, second) = (threads[0], threads[1]);
    for (stream, thread) in [(1, second), (0, first)] {
        view.update(cx, |view, cx| view.focus_thread(thread, cx));
        let send = |event| fake.streams.borrow()[stream].send(event).unwrap();
        send(SessionEvent::TextDelta { text: "said\n\n".into() });
        send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        });
        tick(cx);
    }
    view.update(cx, |view, cx| {
        view.open_side_chat(first, cx);
        view.open_side_chat(second, cx);
    });
    tick(cx);
    tick(cx);
    assert_eq!(fake.forks.borrow().len(), 2, "both forks were asked");
    view.read_with(cx, |view, cx| {
        assert_eq!(shown_fork(view, first, cx), None, "the first tab took no fork");
        assert_eq!(shown_fork(view, second, cx), None, "the second tab took no fork");
        let landed = view.focused_thread(cx).unwrap();
        assert!(![first, second].contains(&landed), "the window lands on a fork");
    });
}

/// A side chat whose fork failed (Claude's before it has named its
/// session) leaves no claim behind: the next side chat takes its own
/// fork, and an ordinary fork of the Thread after it still lands the
/// window.
#[gpui::test]
fn a_failed_fork_leaves_the_next_forks_where_they_belong(cx: &mut TestAppContext) {
    let (view, fake, cx) = answered("side-chat-fork-failed", cx);
    let owner = owner(&view, cx);
    *fake.fail_fork.borrow_mut() = true;
    open_chat(&view, cx);
    *fake.fail_fork.borrow_mut() = false;
    open_chat(&view, cx);
    let tabs = side_tabs(&view, owner, cx);
    view.read_with(cx, |view, cx| {
        assert_eq!(view.side_chat.read(cx).fork(owner, tabs[0]), None, "the failed fork's tab waits");
        assert!(view.side_chat.read(cx).fork(owner, tabs[1]).is_some(), "the next tab takes its fork");
    });

    *fake.fail_fork.borrow_mut() = true;
    open_chat(&view, cx);
    *fake.fail_fork.borrow_mut() = false;
    view.update(cx, |view, cx| view.fork_thread(owner, None, None, cx));
    tick(cx);
    tick(cx);
    view.read_with(cx, |view, cx| {
        let landed = view.focused_thread(cx).unwrap();
        assert_ne!(landed, owner, "the window lands on the ordinary fork");
        assert!(!owns(view, landed, cx), "no side chat took it");
    });
}

/// A Thread whose agent cannot fork still gets its tab: no fork is
/// asked, and the tab says why in the empty word's place.
#[gpui::test]
fn a_side_chat_the_agent_cannot_fork_still_opens_its_tab(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("side-chat-refused", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(700.)));
    tick(cx);
    open_chat(&view, cx);
    let owner = owner(&view, cx);
    assert!(fake.forks.borrow().is_empty(), "no fork is asked");
    assert_eq!(side_tabs(&view, owner, cx).len(), 1, "the tab is there");
    assert!(cx.debug_bounds("side-chat-empty").is_some(), "the reason shows");
    assert!(view.read_with(cx, |view, cx| shown_fork(view, owner, cx)).is_none());
}
