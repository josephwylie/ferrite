//! The side chat (Claude's, `claude/sidechat-open.png`): quick questions
//! about a Thread that leave it as it is. Each conversation is a Surface of
//! the Thread's Panel (ADR 0039), one tab per fork: the Panel's `+` menu
//! (Side chat, S) opens one beside the Thread, which reflows narrower; the
//! tab's × ends it.
//!
//! Its head (T3's `SideChatPane`, 36 tall) holds the title, Bring back and
//! Trash. Bring back puts the side chat's latest run into the main
//! Thread's next message, quoted in its composer, once the main Thread is
//! idle and the side chat holds a finished reply (each reason its
//! tooltip); Trash ends the chat as the tab's × does, after a second press
//! when deletes are confirmed.
//!
//! A tab's conversation is a fork of the Thread (`Cockpit::fork`, the
//! whole of it): the agent's own context, none of the Thread's state. The
//! fork is a Thread of the engine that no Pane holds — it is never in the
//! roster, so the nav, the board, focus cycling and the layout do not know
//! it — and it is retired with its tab (`retire`). The fork carries the
//! Thread's history, which the tab does not show: only what follows it
//! (`Fork::inherited` blocks in, counted once the window holds the fork's
//! history; a fork on another machine streams only while its tab is
//! drawn). The pane draws it with the
//! transcript components the Pane's own use, compact, over a Field
//! Composer of its own: `Ask a quick question` when it is empty, ↵
//! sending.
//!
//! The fork answers asynchronously (`SideChatView::asked`): the tab is
//! there at once, `No messages yet`, and takes the fork's Thread when the
//! agent has made it (`bind`). A fork the agent refuses leaves its reason
//! in the tab's place.
//!
//! Every Thread's side chats live in one view of their own (ADR 0041),
//! which the Panel draws as the body of whichever side chat it shows. What
//! it draws of the window it reads through a `Source` the root makes over
//! itself (`side_chat_inputs`), asking with the forks its tabs hold; what
//! it asks of the window (a question sent, Bring back, Trash) it says as a
//! `SideChatEvent`. The fork is a Thread of the engine, so asking for it,
//! binding it and retiring it stay the Cockpit's, and so does telling the
//! shown fork's transcript what the fork holds: the root does it where it
//! tells each Pane's (`sync_visible_transcripts`), before the frame draws.

use std::collections::HashMap;

use ferrite_core::{Rehold, ThreadKey};
use ferrite_pane::composer::{Composer, Role, SentDraft};
use ferrite_pane::pane::PaneView;
use ferrite_pane::transcript::{TranscriptEvent, TranscriptView};
use ferrite_ui::icons::{self, icon};
use ferrite_ui::pointer::Pointer as _;
use ferrite_ui::source::Source;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, AnyElement, ClickEvent, Context, Div, Entity, EventEmitter, Focusable as _,
    SharedString, Window,
};

gpui::actions!(side_chat, [Ask]);

/// The field's key context: its line's, where ⏎ sends the question
/// instead of the Pane's prompt.
pub const KEY_CONTEXT: &str = "SideChat";

/// A tab's fork, as the Cockpit reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fork {
    pub thread: ThreadKey,
    /// How many blocks the fork held when the window first held its
    /// history: the Thread's, which the tab leaves unshown. None until
    /// then, and the tab shows nothing and asks nothing: another machine
    /// streams the fork only once its tab draws it (F-A2).
    pub inherited: Option<usize>,
}

/// A tab's fork, once the agent has made it.
struct Conversation {
    fork: Fork,
    /// What the tab draws it with: the Pane's pieces (its transcript, its
    /// tool disclosures), never in the grid.
    pane: PaneView,
}

struct Tab {
    id: u64,
    title: SharedString,
    field: Entity<Composer>,
    conversation: Option<Conversation>,
    /// Why the agent would not fork, in place of `No messages yet`.
    refused: Option<SharedString>,
}

/// One Thread's side chats, and the one its Panel last showed.
struct Chat {
    tabs: Vec<Tab>,
    active: usize,
    /// Tabs ever made, which names the next: `Side chat 2`.
    made: usize,
}

/// Every Thread's side chat, and the forks still on their way.
pub struct SideChatView {
    inputs: Source<SideChatInputs, Forks>,
    chats: HashMap<ThreadKey, Chat>,
    /// The tab waiting on each Thread's fork, kept when the tab closes so
    /// the fork is retired when it lands. One per Thread: the engine makes
    /// one fork of a Thread at a time, so a newer ask replaces one that
    /// failed and never landed.
    asked: HashMap<ThreadKey, u64>,
    next_tab: u64,
    /// The field wants the keyboard (a chat opened, a tab changed): the
    /// next frame that shows it hands it over.
    focus_wanted: bool,
    /// The side chat whose Trash waits for its second press.
    trash_armed: Option<u64>,
}

/// What the side chat draws from, as the Cockpit has it this frame.
pub struct SideChatInputs {
    /// The Thread whose Panel shows a side chat, and the tab it shows.
    pub owner: ThreadKey,
    pub tab: u64,
    /// Whether the Panel is open on it, not sliding shut: only then does
    /// the field take the keyboard.
    pub shown: bool,
    /// Whether the fork holds words past the history it inherited.
    pub answered: bool,
    /// Why Bring back cannot run now, or None.
    pub blocked: Option<&'static str>,
    pub confirm_delete: bool,
}

/// The fork each tab holds, by the tab's id: what the side chat asks the
/// window about, since whether a fork has answered and whether Bring back
/// can run are the window's to read.
#[derive(Default)]
pub struct Forks(HashMap<u64, Fork>);

impl Forks {
    pub fn of(&self, tab: u64) -> Option<Fork> {
        self.0.get(&tab).copied()
    }
}

/// What the side chat asks of the window.
pub enum SideChatEvent {
    /// ⏎ in a tab's field: the question for its fork. A draft the fork
    /// does not take comes back to the field (`unsent`).
    Ask {
        tab: u64,
        fork: ThreadKey,
        draft: SentDraft,
    },
    /// Bring back: the fork's latest run into the main Thread's next
    /// message.
    BringBack { owner: ThreadKey, fork: Fork },
    /// Trash, pressed for the last time: the tab's Surface closes.
    Trash { owner: ThreadKey, tab: u64 },
}

impl EventEmitter<SideChatEvent> for SideChatView {}

impl SideChatView {
    pub fn new(inputs: Source<SideChatInputs, Forks>) -> Self {
        Self {
            inputs,
            chats: HashMap::new(),
            asked: HashMap::new(),
            next_tab: 0,
            focus_wanted: false,
            trash_armed: None,
        }
    }

    /// Whether `thread` is a tab's fork: no Pane, nav row or palette entry
    /// of its own.
    pub fn owns(&self, thread: ThreadKey) -> bool {
        self.chats.values().flat_map(|chat| &chat.tabs).any(|tab| {
            tab.conversation
                .as_ref()
                .is_some_and(|c| c.fork.thread == thread)
        })
    }

    /// The Thread whose side chat `thread` is a fork of: the head's
    /// parent.
    pub fn owner_of(&self, thread: ThreadKey) -> Option<ThreadKey> {
        self.chats.iter().find_map(|(owner, chat)| {
            chat.tabs
                .iter()
                .any(|tab| tab.conversation.as_ref().is_some_and(|c| c.fork.thread == thread))
                .then_some(*owner)
        })
    }

    fn tab(&self, owner: ThreadKey, id: u64) -> Option<&Tab> {
        self.chats.get(&owner)?.tabs.iter().find(|tab| tab.id == id)
    }

    /// Side chat `id`'s fork and what draws it, once the agent has made it.
    pub fn shown_conversation(&self, owner: ThreadKey, id: u64) -> Option<(Fork, &PaneView)> {
        let conversation = self.tab(owner, id)?.conversation.as_ref()?;
        Some((conversation.fork, &conversation.pane))
    }

    /// The fork each tab holds: what the view asks the window about.
    fn forks(&self) -> Forks {
        Forks(
            self.chats
                .values()
                .flat_map(|chat| &chat.tabs)
                .filter_map(|tab| Some((tab.id, tab.conversation.as_ref()?.fork)))
                .collect(),
        )
    }

    fn tab_mut(&mut self, id: u64) -> Option<&mut Tab> {
        self.chats
            .values_mut()
            .find_map(|chat| chat.tabs.iter_mut().find(|tab| tab.id == id))
    }

    /// Side chat `id`'s fork, once the agent has made it.
    pub fn fork(&self, owner: ThreadKey, id: u64) -> Option<Fork> {
        Some(self.tab(owner, id)?.conversation.as_ref()?.fork)
    }

    /// Side chat `id`'s title, for its tab.
    pub fn title(&self, owner: ThreadKey, id: u64) -> SharedString {
        self.tab(owner, id)
            .map_or_else(|| "Side chat".into(), |tab| tab.title.clone())
    }

    /// The shown tab's field.
    fn field(&self, owner: ThreadKey) -> Option<&Entity<Composer>> {
        let chat = self.chats.get(&owner)?;
        Some(&chat.tabs.get(chat.active)?.field)
    }

    pub fn holds_focus(&self, owner: ThreadKey, window: &Window, cx: &gpui::App) -> bool {
        self.field(owner).is_some_and(|field| {
            field
                .read(cx)
                .focus_target(window, cx)
                .contains_focused(window, cx)
        })
    }

    /// One more side chat, empty, for `owner`. `asked` is whether the
    /// agent was asked for its fork, or why it could not be.
    pub fn open(
        &mut self,
        owner: ThreadKey,
        asked: Result<(), SharedString>,
        cx: &mut Context<Self>,
    ) -> u64 {
        let id = self.push_tab(owner, asked.clone().err(), cx);
        if asked.is_ok() {
            self.asked.insert(owner, id);
        }
        id
    }

    /// The agent refused the fork tab `id` of `owner`'s side chat was
    /// opened for: the tab says why, and waits for no fork.
    pub fn refused(&mut self, owner: ThreadKey, id: u64, why: SharedString, cx: &mut Context<Self>) {
        if self.asked.get(&owner) == Some(&id) {
            self.asked.remove(&owner);
        }
        if let Some(tab) = self.tab_mut(id) {
            tab.refused = Some(why);
        }
        cx.notify();
    }

    /// A scene's side chat: a tab as the agent would leave it before the
    /// fork answers, with no agent asked.
    #[cfg(any(test, feature = "test-support"))]
    pub fn fixture_tab(&mut self, owner: ThreadKey, cx: &mut Context<Self>) -> u64 {
        self.push_tab(owner, None, cx)
    }

    /// A tab in `owner`'s side chat, shown, its field empty. `refused` is
    /// why it has no fork.
    fn push_tab(
        &mut self,
        owner: ThreadKey,
        refused: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> u64 {
        let field = cx.new(|cx| {
            let mut line = Composer::new(cx);
            line.set_role(Role::Field(Some(KEY_CONTEXT)), cx);
            line.set_visible_row_limit(ferrite_pane::composer::MAX_ROWS, cx);
            line
        });
        let id = self.next_tab;
        self.next_tab += 1;
        let chat = self.chats.entry(owner).or_insert_with(|| Chat {
            tabs: Vec::new(),
            active: 0,
            made: 0,
        });
        chat.made += 1;
        let title = match chat.made {
            1 => "Side chat".to_string(),
            n => format!("Side chat {n}"),
        };
        chat.tabs.push(Tab {
            id,
            title: title.into(),
            field,
            conversation: None,
            refused,
        });
        chat.active = chat.tabs.len() - 1;
        self.focus_wanted = true;
        cx.notify();
        id
    }

    /// `owner`'s fork has landed: the tab that asked takes it. None when no
    /// tab asked for one; false when its tab closed while the agent worked.
    pub fn bind(&mut self, owner: ThreadKey, fork: Fork, cx: &mut Context<Self>) -> Option<bool> {
        let id = self.asked.remove(&owner)?;
        if self.tab_mut(id).is_none() {
            return Some(false);
        }
        let conversation = Self::conversation(fork, cx);
        if let Some(tab) = self.tab_mut(id) {
            tab.conversation = Some(conversation);
        }
        cx.notify();
        Some(true)
    }

    /// What a tab draws `fork` with, its disclosures pressed in place.
    fn conversation(fork: Fork, cx: &mut Context<Self>) -> Conversation {
        let pane = PaneView::new(fork.thread, cx);
        if let Some(entity) = pane.transcript() {
            cx.subscribe(&entity, Self::on_transcript_event).detach();
        }
        Conversation { fork, pane }
    }

    /// A machine let go is held again: its Threads' side chats are theirs,
    /// each on its fork's key now. A fork is its owner's machine's, and
    /// the same Thread still, so what it inherited stands.
    pub fn held_again(&mut self, rehold: Rehold, cx: &mut Context<Self>) {
        let moved: Vec<_> = self.chats.extract_if(|owner, _| rehold.key(*owner).is_some()).collect();
        for (owner, mut chat) in moved {
            for tab in &mut chat.tabs {
                tab.conversation = tab.conversation.take().map(|was| {
                    let thread = rehold.key(was.fork.thread).unwrap_or(was.fork.thread);
                    Self::conversation(Fork { thread, ..was.fork }, cx)
                });
            }
            self.chats.extend(rehold.key(owner).map(|owner| (owner, chat)));
        }
        let asked: Vec<_> = self.asked.extract_if(|owner, _| rehold.key(*owner).is_some()).collect();
        self.asked.extend(asked.into_iter().filter_map(|(owner, id)| Some((rehold.key(owner)?, id))));
        cx.notify();
    }

    /// The forks whose history the window has not held yet.
    pub fn unsettled(&self) -> Vec<ThreadKey> {
        self.chats
            .values()
            .flat_map(|chat| &chat.tabs)
            .filter_map(|tab| tab.conversation.as_ref())
            .filter(|conversation| conversation.fork.inherited.is_none())
            .map(|conversation| conversation.fork.thread)
            .collect()
    }

    /// `fork`'s history has landed, `inherited` blocks of it.
    pub fn settle(&mut self, fork: ThreadKey, inherited: usize, cx: &mut Context<Self>) {
        let conversation = self
            .chats
            .values_mut()
            .flat_map(|chat| &mut chat.tabs)
            .filter_map(|tab| tab.conversation.as_mut())
            .find(|conversation| conversation.fork.thread == fork);
        if let Some(conversation) = conversation {
            conversation.fork.inherited = Some(inherited);
            cx.notify();
        }
    }

    /// The Thread whose fork a side chat waits on, when it is the only one.
    pub fn only_waiting(&self) -> Option<ThreadKey> {
        let mut waiting = self.asked.keys();
        match (waiting.next(), waiting.next()) {
            (Some(owner), None) => Some(*owner),
            _ => None,
        }
    }

    /// An ordinary fork of `owner` was asked, so one its side chat asked
    /// before has settled (the engine makes one at a time): what lands
    /// next is the window's, not that tab's.
    pub fn forget_fork(&mut self, owner: ThreadKey) {
        self.asked.remove(&owner);
    }

    /// Tab `id` of `owner` goes. Its fork, if it had one, is the Cockpit's
    /// to retire.
    pub fn close(&mut self, owner: ThreadKey, id: u64, cx: &mut Context<Self>) -> Option<ThreadKey> {
        let chat = self.chats.get_mut(&owner)?;
        let index = chat.tabs.iter().position(|tab| tab.id == id)?;
        let tab = chat.tabs.remove(index);
        chat.active = chat.active.min(chat.tabs.len().saturating_sub(1));
        if chat.tabs.is_empty() {
            self.chats.remove(&owner);
        }
        self.focus_wanted = true;
        cx.notify();
        tab.conversation.map(|conversation| conversation.fork.thread)
    }

    /// ⏎ in tab `id`'s field: its question, for the fork. Nothing before
    /// the window holds the fork's history, or for an empty line.
    fn ask(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(tab) = self.tab_mut(id) else {
            return;
        };
        let Some(conversation) = tab
            .conversation
            .as_ref()
            .filter(|conversation| conversation.fork.inherited.is_some())
        else {
            return;
        };
        let fork = conversation.fork.thread;
        let transcript = conversation.pane.transcript();
        let field = tab.field.clone();
        if field.read(cx).text().trim().is_empty() {
            return;
        }
        let draft = field.update(cx, |field, cx| field.take_draft(cx));
        if let Some(entity) = transcript {
            entity.update(cx, |transcript, cx| transcript.scroll_to_bottom(cx));
        }
        cx.emit(SideChatEvent::Ask { tab: id, fork, draft });
        cx.notify();
    }

    /// A question its fork did not take: back in the field, ahead of
    /// anything typed since.
    pub fn unsent(&mut self, id: u64, draft: SentDraft, cx: &mut Context<Self>) {
        if let Some(tab) = self.tab_mut(id) {
            tab.field
                .update(cx, |field, cx| field.restore(draft, true, cx));
        }
    }

    /// Trash: the side chat and its fork go, after a second press when the
    /// settings ask to confirm a delete.
    fn trash(&mut self, owner: ThreadKey, id: u64, confirm: bool, cx: &mut Context<Self>) {
        if confirm && self.trash_armed != Some(id) {
            self.trash_armed = Some(id);
            cx.notify();
            return;
        }
        self.trash_armed = None;
        cx.emit(SideChatEvent::Trash { owner, tab: id });
    }

    /// A tab's transcript asks for something: a tool row folds or unfolds
    /// there. The rest (a path's reader, a subagent) is the Pane's.
    fn on_transcript_event(
        &mut self,
        entity: Entity<TranscriptView>,
        event: &TranscriptEvent,
        cx: &mut Context<Self>,
    ) {
        let TranscriptEvent::ToggleDisclosure(call) = event else {
            return;
        };
        for tab in self.chats.values_mut().flat_map(|chat| chat.tabs.iter_mut()) {
            if let Some(conversation) = &mut tab.conversation {
                if conversation.pane.transcript().as_ref() == Some(&entity) {
                    conversation.pane.clear_tool_target();
                    conversation.pane.toggle_tool(call);
                }
            }
        }
        cx.notify();
    }

    /// Follow the tab the Panel shows, and hand its field the keyboard it
    /// asked for once the Panel is open on it.
    fn follow(&mut self, inputs: &SideChatInputs, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(chat) = self.chats.get_mut(&inputs.owner) {
            if let Some(at) = chat.tabs.iter().position(|tab| tab.id == inputs.tab) {
                if chat.active != at {
                    chat.active = at;
                    self.focus_wanted = true;
                }
            }
        }
        if inputs.shown && std::mem::take(&mut self.focus_wanted) {
            if let Some(field) = self.field(inputs.owner) {
                let focus = field.read(cx).focus_target(window, cx);
                window.focus(&focus, cx);
            }
        }
    }

    // ------------------------------------------------------------ drawing

    /// The shown side chat's Surface: the conversation or its empty word,
    /// the composer.
    fn body(&self, inputs: &SideChatInputs, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let Some(tab) = self.tab(inputs.owner, inputs.tab) else {
            return div().into_any_element();
        };
        let said = Self::said(tab, inputs.answered);
        let shown = match &tab.conversation {
            Some(conversation) if said.is_none() => conversation.pane.transcript(),
            _ => None,
        };
        let body = div()
            .relative()
            .flex_1()
            .min_h_0()
            .when_some(shown, |body, transcript| body.child(transcript))
            .when_some(said, |body, said| {
                body.child(
                    div()
                        .debug_selector(|| "side-chat-empty".into())
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .px(px(SPACE_4))
                        .text_color(rgb(INK_SUBTLE))
                        .child(said),
                )
            });
        div()
            .debug_selector(|| "side-chat".into())
            .size_full()
            .flex()
            .flex_col()
            .font_family(FONT_UI)
            .text_size(px(FS_UI))
            .line_height(px(LH_UI))
            .text_color(rgb(INK_STRONG))
            .child(self.head(inputs, tab, cx))
            .child(body)
            .child(self.composer(tab, cx))
            .into_any_element()
    }

    /// What `tab` says in its conversation's place: why the agent would
    /// not fork, else `No messages yet` until its fork has `answered`.
    fn said(tab: &Tab, answered: bool) -> Option<SharedString> {
        match &tab.conversation {
            Some(_) if answered => None,
            _ => Some(tab.refused.clone().unwrap_or("No messages yet".into())),
        }
    }

    /// T3's side chat head, under the tab strip: the title, Bring back and
    /// Trash. The tab's × closes it, so the head has none.
    fn head(&self, inputs: &SideChatInputs, tab: &Tab, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::components::{icon_button_as, IconSize, IconState, Tip as _};
        use ferrite_ui::theme::*;
        let owner = inputs.owner;
        let id = tab.id;
        let blocked = inputs.blocked;
        let confirm = inputs.confirm_delete;
        let armed = self.trash_armed == Some(id);
        let back_key = ferrite_ui::pointer::hover_key(&gpui::ElementId::from("side-chat-bring-back"));
        let back = div()
            .id("side-chat-bring-back")
            .debug_selector(|| "side-chat-bring-back".into())
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(SURFACE_TAB_H))
            .px(px(SPACE_2))
            .rounded(px(R_CONTROL))
            .font_weight(W_LABEL)
            .text_color(rgb(INK_SUBTLE))
            .when(blocked.is_some(), |back| back.opacity(0.64))
            .when(blocked.is_none(), |back| back.hover_control(back_key).text_color(rgb(INK_STRONG)))
            .tooltip(ferrite_ui::menu::tooltip(
                blocked.unwrap_or("Adds this side chat to the main thread's next message."),
            ))
            .child("Bring back")
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                if let Some(fork) = view.fork(owner, id) {
                    cx.emit(SideChatEvent::BringBack { owner, fork });
                }
            }));
        let trash = icon_button_as(
            "side-chat-trash",
            icons::TRASH,
            IconSize::Compact,
            "Delete side chat",
            if armed { IconState::Armed } else { IconState::Plain },
            cx,
        )
        .tip(if armed {
            "Delete side chat? Press again"
        } else {
            "Delete side chat"
        })
        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
            cx.stop_propagation();
            view.trash(owner, id, confirm, cx);
        }));
        ferrite_ui::components::surface_bar()
            .debug_selector(|| "side-chat-head".into())
            .pl(px(SPACE_3))
            .pr(px(SPACE_1))
            .text_size(px(FS_BADGE))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(W_LABEL)
                    .child(tab.title.clone()),
            )
            .child(back)
            .child(trash)
    }

    /// The composer at the foot: a `FIELD` card with the line, its
    /// placeholder while empty, and `↵` at its own foot.
    fn composer(&self, tab: &Tab, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
        let id = tab.id;
        let empty = tab.field.read(cx).text().is_empty();
        div()
            .debug_selector(|| "side-chat-composer".into())
            .flex_shrink_0()
            .m(px(SIDE_CHAT_INSET))
            .p(px(SPACE_3))
            .rounded(px(R_FLOAT))
            .bg(paint::FIELD)
            .border_1()
            .border_color(paint::FIELD_RING)
            .flex()
            .flex_col()
            // A press anywhere on the card focuses the field, as a press on
            // a Pane's Composer box does: the window focuses the innermost
            // handle a press lands in.
            .track_focus(&tab.field.focus_handle(cx))
            .on_action(cx.listener(move |view, _: &Ask, _, cx| view.ask(id, cx)))
            .child(
                div()
                    .relative()
                    .child(tab.field.clone())
                    .when(empty, |line| {
                        line.child(
                            div()
                                .debug_selector(|| "side-chat-placeholder".into())
                                .absolute()
                                .top_0()
                                .left_0()
                                .text_color(rgb(INK_SUBTLE))
                                .child("Ask a quick question"),
                        )
                    }),
            )
            .child(div().flex().justify_end().h(px(LH_UI)).child(icon(
                icons::CORNER_DOWN_LEFT,
                ICON_BUTTON_GLYPH,
                INK_SUBTLE,
            )))
    }

    /// The titles of `owner`'s tabs and the shown one's index — what the
    /// tests read.
    #[cfg(any(test, feature = "test-support"))]
    pub fn titles(&self, owner: ThreadKey) -> (Vec<SharedString>, usize) {
        self.chats.get(&owner).map_or((Vec::new(), 0), |chat| {
            (
                chat.tabs.iter().map(|tab| tab.title.clone()).collect(),
                chat.active,
            )
        })
    }

    /// Every fork a tab holds.
    #[cfg(any(test, feature = "test-support"))]
    pub fn threads(&self, owner: ThreadKey) -> Vec<ThreadKey> {
        self.chats.get(&owner).map_or_else(Vec::new, |chat| {
            chat.tabs
                .iter()
                .filter_map(|tab| tab.conversation.as_ref().map(|c| c.fork.thread))
                .collect()
        })
    }

    /// The shown tab's fork, once it has one — what the tests drive.
    #[cfg(any(test, feature = "test-support"))]
    pub fn shown_fork(&self, owner: ThreadKey) -> Option<ThreadKey> {
        let chat = self.chats.get(&owner)?;
        Some(chat.tabs.get(chat.active)?.conversation.as_ref()?.fork.thread)
    }

    /// Tab `index`'s field — what the tests type in.
    #[cfg(any(test, feature = "test-support"))]
    pub fn field_of(&self, owner: ThreadKey, index: usize) -> Option<&Entity<Composer>> {
        Some(&self.chats.get(&owner)?.tabs.get(index)?.field)
    }

    /// What the shown tab's field holds — what the tests read.
    #[cfg(any(test, feature = "test-support"))]
    pub fn draft(&self, owner: ThreadKey, cx: &gpui::App) -> Option<String> {
        Some(self.field(owner)?.read(cx).text().to_string())
    }

    /// The shown tab's pieces, once it has a fork — what the tests press.
    #[cfg(any(test, feature = "test-support"))]
    pub fn pane(&self, owner: ThreadKey) -> Option<&PaneView> {
        let chat = self.chats.get(&owner)?;
        Some(&chat.tabs.get(chat.active)?.conversation.as_ref()?.pane)
    }
}

impl Render for SideChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(inputs) = self.inputs.read(&self.forks(), cx) else {
            return div().into_any_element();
        };
        self.follow(&inputs, window, cx);
        self.body(&inputs, cx)
    }
}

#[cfg(test)]
mod tests;
