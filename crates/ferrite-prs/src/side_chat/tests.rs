//! `SideChatView` in a window of its own: its tabs, what each says before
//! its fork answers, and what it asks of the window. The host is the
//! window's side: the tab the Panel shows, whether the fork has answered,
//! why Bring back cannot run, whether deletes are confirmed. Making,
//! binding and retiring a fork, and the Panel's tabs, are the Cockpit's to
//! test.

use ferrite_ui::testing::{click, mount, record, shows};
use gpui::{
    DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate, TestAppContext,
    VisualTestContext,
};

use super::*;

fn owner() -> ThreadKey {
    ThreadKey::fixture(1, 1)
}

fn fork_thread() -> ThreadKey {
    ThreadKey::fixture(1, 2)
}

/// A second tab's fork, of the same Thread.
fn other_fork() -> ThreadKey {
    ThreadKey::fixture(1, 4)
}

/// The window's side of the side chat.
struct Host {
    chat: Entity<SideChatView>,
    /// The tab the Panel shows.
    tab: Option<u64>,
    shown: bool,
    answered: bool,
    blocked: Option<&'static str>,
    confirm_delete: bool,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().flex_col().child(self.chat.clone())
    }
}

/// What the side chat asked, as the tests compare it.
#[derive(Debug, PartialEq)]
enum Asked {
    Ask { tab: u64, fork: ThreadKey, prompt: String },
    BringBack { owner: ThreadKey, fork: Fork },
    Trash { owner: ThreadKey, tab: u64 },
}

fn asked(event: &SideChatEvent) -> Asked {
    match event {
        SideChatEvent::Ask { tab, fork, draft } => Asked::Ask {
            tab: *tab,
            fork: *fork,
            prompt: draft.prompt().to_string(),
        },
        SideChatEvent::BringBack { owner, fork } => Asked::BringBack {
            owner: *owner,
            fork: *fork,
        },
        SideChatEvent::Trash { owner, tab } => Asked::Trash {
            owner: *owner,
            tab: *tab,
        },
    }
}

/// The production key table, as far as the actions linked here go.
fn bind_keys(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let table = ferrite_ui::keymap::bindings(ferrite_ui::keymap::Platform::Mac);
        let bindings = table
            .into_iter()
            .filter_map(|(keystroke, action, context)| {
                let action = cx.build_action(action, None).ok()?;
                let context = context.map(|context| KeyBindingContextPredicate::parse(context).unwrap().into());
                Some(KeyBinding::load(&keystroke, action, context, false, None, &DummyKeyboardMapper).unwrap())
            })
            .collect::<Vec<_>>();
        cx.bind_keys(bindings);
    });
}

/// The side chat with no tab yet, the Panel open on whichever it shows.
fn window(cx: &mut TestAppContext) -> (Entity<Host>, Entity<SideChatView>, &mut VisualTestContext) {
    cx.update(ferrite_pane::pane::init_components);
    bind_keys(cx);
    let (host, cx) = mount(cx, |_, cx| {
        let inputs = Source::of(&cx.entity(), |host: &Host, _: &Forks, _| {
            Some(SideChatInputs {
                owner: owner(),
                tab: host.tab?,
                shown: host.shown,
                answered: host.answered,
                blocked: host.blocked,
                confirm_delete: host.confirm_delete,
            })
        });
        Host {
            chat: cx.new(|_| SideChatView::new(inputs)),
            tab: None,
            shown: true,
            answered: false,
            blocked: None,
            confirm_delete: false,
        }
    });
    cx.simulate_resize(gpui::size(px(480.), px(700.)));
    let chat = host.read_with(cx, |host, _| host.chat.clone());
    (host, chat, cx)
}

/// One more tab, its fork asked for (or refused with `why`), shown.
fn open(host: &Entity<Host>, chat: &Entity<SideChatView>, why: Option<&str>, cx: &mut VisualTestContext) -> u64 {
    let asked = why.map_or(Ok(()), |why| Err(SharedString::from(why.to_string())));
    let id = chat.update(cx, |chat, cx| chat.open(owner(), asked, cx));
    host.update(cx, |host, cx| {
        host.tab = Some(id);
        cx.notify();
    });
    cx.run_until_parked();
    id
}

/// What the shown tab says in its conversation's place.
fn said(chat: &Entity<SideChatView>, id: u64, cx: &mut VisualTestContext) -> Option<String> {
    chat.read_with(cx, |chat, _| {
        let tab = chat.tab(owner(), id)?;
        SideChatView::said(tab, false).map(|said| said.to_string())
    })
}

fn holds_focus(chat: &Entity<SideChatView>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| chat.read(cx).holds_focus(owner(), window, cx))
}

/// A new tab says `No messages yet` over `Ask a quick question`, and its
/// field takes the keyboard once the Panel is open on it, not before.
#[gpui::test]
fn a_new_tab_says_no_messages_yet_and_takes_the_keys_once_shown(cx: &mut TestAppContext) {
    let (host, chat, cx) = window(cx);
    host.update(cx, |host, _| host.shown = false);
    let id = open(&host, &chat, None, cx);
    assert!(shows(cx, "side-chat-empty") && shows(cx, "side-chat-placeholder"));
    assert_eq!(said(&chat, id, cx).as_deref(), Some("No messages yet"));
    assert!(!holds_focus(&chat, cx), "the Panel is not open on it");
    host.update(cx, |host, cx| {
        host.shown = true;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(holds_focus(&chat, cx), "now it is");
    assert_eq!(chat.read_with(cx, |chat, _| chat.only_waiting()), Some(owner()), "its fork is awaited");
}

/// A tab whose fork the agent refused says why in the empty word's place,
/// and waits for no fork.
#[gpui::test]
fn a_refused_tab_says_why_and_waits_for_no_fork(cx: &mut TestAppContext) {
    let (host, chat, cx) = window(cx);
    let id = open(&host, &chat, Some("Its agent cannot start a side chat"), cx);
    assert!(shows(cx, "side-chat-empty"));
    assert_eq!(said(&chat, id, cx).as_deref(), Some("Its agent cannot start a side chat"));
    assert_eq!(chat.read_with(cx, |chat, _| chat.only_waiting()), None);
}

/// A second tab is `Side chat 2`, shown, its field its own; the tab the
/// Panel shows is the one drawn.
#[gpui::test]
fn each_tab_is_its_own_conversation(cx: &mut TestAppContext) {
    let (host, chat, cx) = window(cx);
    let first = open(&host, &chat, None, cx);
    let second = open(&host, &chat, None, cx);
    let titles = chat.read_with(cx, |chat, _| chat.titles(owner()));
    assert_eq!(titles, (vec!["Side chat".into(), "Side chat 2".into()], 1));
    cx.simulate_input("second");
    assert_eq!(chat.read_with(cx, |chat, cx| chat.draft(owner(), cx)).as_deref(), Some("second"));
    host.update(cx, |host, cx| {
        host.tab = Some(first);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(chat.read_with(cx, |chat, _| chat.titles(owner()).1), 0, "it follows the Panel");
    assert_eq!(chat.read_with(cx, |chat, cx| chat.draft(owner(), cx)).as_deref(), Some(""));
    assert!(holds_focus(&chat, cx), "the tab shown takes the keys");
    let gone = chat.update(cx, |chat, cx| chat.close(owner(), second, cx));
    assert_eq!(gone, None, "no fork of its own to retire yet");
    assert_eq!(chat.read_with(cx, |chat, _| chat.titles(owner())), (vec!["Side chat".into()], 0));
}

/// ⏎ sends nothing until the window holds the fork's history; then it asks
/// the window to send the question to the tab's fork, the field empties,
/// and a question the fork did not take comes back.
#[gpui::test]
fn enter_asks_the_window_once_the_forks_history_is_held(cx: &mut TestAppContext) {
    let (host, chat, cx) = window(cx);
    let id = open(&host, &chat, None, cx);
    let fork = Fork {
        thread: fork_thread(),
        inherited: None,
    };
    assert_eq!(chat.update(cx, |chat, cx| chat.bind(owner(), fork, cx)), Some(true), "the tab took it");
    let heard = record(cx, &chat, asked);
    let drafts = record(cx, &chat, |event: &SideChatEvent| match event {
        SideChatEvent::Ask { draft, .. } => Some(draft.clone()),
        _ => None,
    });
    cx.simulate_input("what is alpha?");
    cx.simulate_keystrokes("enter");
    assert_eq!(heard.take(), [], "its history is not held yet");
    assert_eq!(chat.read_with(cx, |chat, _| chat.unsettled()), [fork_thread()]);
    chat.update(cx, |chat, cx| chat.settle(fork_thread(), 3, cx));
    cx.simulate_keystrokes("enter");
    let said = heard.take();
    assert_eq!(
        said,
        [Asked::Ask {
            tab: id,
            fork: fork_thread(),
            prompt: "what is alpha?".into()
        }]
    );
    assert_eq!(chat.read_with(cx, |chat, cx| chat.draft(owner(), cx)).as_deref(), Some(""));
    let draft = drafts.take().into_iter().flatten().last().expect("the draft sent");
    chat.update(cx, |chat, cx| chat.unsent(id, draft, cx));
    assert_eq!(chat.read_with(cx, |chat, cx| chat.draft(owner(), cx)).as_deref(), Some("what is alpha?"));
}

/// Bring back asks the window for the shown tab's fork, with where its
/// own run starts; a tab with no fork yet asks nothing.
#[gpui::test]
fn bring_back_asks_for_the_tabs_fork(cx: &mut TestAppContext) {
    let (host, chat, cx) = window(cx);
    open(&host, &chat, None, cx);
    let first = Fork {
        thread: fork_thread(),
        inherited: Some(3),
    };
    chat.update(cx, |chat, cx| chat.bind(owner(), first, cx));
    open(&host, &chat, None, cx);
    let heard = record(cx, &chat, asked);
    click(cx, "side-chat-bring-back");
    assert_eq!(heard.take(), [], "no fork yet");
    let second = Fork {
        thread: other_fork(),
        inherited: Some(5),
    };
    chat.update(cx, |chat, cx| chat.bind(owner(), second, cx));
    click(cx, "side-chat-bring-back");
    assert_eq!(
        heard.take(),
        [Asked::BringBack {
            owner: owner(),
            fork: second
        }],
        "the second tab's fork, from its own boundary"
    );
}

/// Trash asks the window to end the tab: at once, or on a second press
/// when deletes are confirmed.
#[gpui::test]
fn trash_asks_on_the_second_press_when_deletes_are_confirmed(cx: &mut TestAppContext) {
    let (host, chat, cx) = window(cx);
    let id = open(&host, &chat, None, cx);
    let heard = record(cx, &chat, asked);
    host.update(cx, |host, cx| {
        host.confirm_delete = true;
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "side-chat-trash");
    assert_eq!(heard.take(), [], "armed");
    click(cx, "side-chat-trash");
    assert_eq!(heard.take(), [Asked::Trash { owner: owner(), tab: id }]);
    host.update(cx, |host, cx| {
        host.confirm_delete = false;
        cx.notify();
    });
    cx.run_until_parked();
    click(cx, "side-chat-trash");
    assert_eq!(heard.take(), [Asked::Trash { owner: owner(), tab: id }], "unconfirmed: at once");
}
