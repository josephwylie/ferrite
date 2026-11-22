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
//! (`Fork::inherited` blocks in). The pane draws it with the
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
//! it draws of the window it reads through `side_chat_inputs`; what it
//! asks of the window (a question sent, Bring back, Trash) it says as a
//! `SideChatEvent`. The fork is a Thread of the engine, so asking for it,
//! binding it and retiring it stay the Cockpit's.

use std::collections::HashMap;

use gpui::WeakEntity;

use crate::cockpit::surfaces::SurfaceId;

use super::*;

use ferrite_core::activity::Subject;
use ferrite_core::cockpit::Admission;
use ferrite_core::replies::Reply;
use ferrite_core::workspace::WorkspaceBinding;
use ferrite_core::ThreadKey;
use ferrite_pane::composer::{Role, SentDraft};
use ferrite_pane::transcript::{TranscriptEvent, TranscriptView};
use ferrite_ui::icons::{self, icon};

gpui::actions!(side_chat, [Ask]);

/// The field's key context: its line's, where ⏎ sends the question
/// instead of the Pane's prompt.
pub(crate) const KEY_CONTEXT: &str = "SideChat";

/// A tab's fork, as the Cockpit reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fork {
    thread: ThreadKey,
    /// How many blocks the fork held when it was made: the Thread's
    /// history, which the tab leaves unshown.
    inherited: usize,
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
pub(crate) struct SideChatView {
    cockpit: WeakEntity<CockpitView>,
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
pub(crate) struct SideChatInputs {
    /// The Thread whose Panel shows a side chat, and the tab it shows.
    owner: ThreadKey,
    tab: u64,
    /// Whether the Panel is open on it, not sliding shut: only then does
    /// the field take the keyboard.
    shown: bool,
    /// Whether the fork holds words past the history it inherited.
    answered: bool,
    /// Why Bring back cannot run now, or None.
    blocked: Option<&'static str>,
    confirm_delete: bool,
}

/// What the side chat asks of the window.
pub(crate) enum SideChatEvent {
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

impl CockpitView {
    /// Tell `pane`'s transcript what `subject` of `thread` holds after its
    /// first `skip` blocks. The compact, read-only twin of
    /// `sync_transcript`, for the side chat's tabs: no subagents, rewinds,
    /// Reply or Decisions, as they have no Pane's Composer to reply into.
    pub(super) fn sync_compact_transcript(
        &self,
        thread: ThreadKey,
        subject: &Subject,
        pane: &PaneView,
        skip: usize,
        cx: &mut Context<Self>,
    ) {
        let reduce_motion = cx.reduce_motion();
        // The model is lent, not written: the transcript copies what it
        // borrows of it under its own update.
        self.cockpit.update(cx, |model, cx| {
            let Some(open) = model.thread(thread) else {
                return;
            };
            let Some(subject_view) = open.activity().subject(subject) else {
                return;
            };
            let namespace = pane.text_namespace();
            let revision = subject_view.presentation_revision();
            let status = pane::transcript_status(subject_view.status(), subject_view.fresh());
            let reading_size = self.prefs.settings.reading_size;
            let transcript = subject_view.transcript();
            let working = pane::working_tail(pane::WorkingFacts {
                transcript,
                thread: thread.thread(),
                starting: open.starting(),
                solo: false,
                focused: true,
                reduce_motion,
            });
            let workspace =
                ferrite_core::workspace::effective_cwd(open.session_project_root(), open.workspace())
                    .map(std::path::Path::to_path_buf);
            let presentation = ferrite_pane::transcript::TranscriptPresentation {
                thread: thread.thread(),
                selection_scope: pane.selection_scope,
                disclosure_focus: pane.tool_focus(),
                namespace,
                content_revision: revision,
                display_revision: pane.disclosure_revision(),
                focused: true,
                signal_status: Some(status),
                reading_size,
                working,
                tail: None,
                pending_call: None,
                workspace,
                solo: false,
                thinking: self.prefs.settings.show_thinking,
                agents: Default::default(),
                rewind: None,
                quotes: false,
                reply_to: open.provider().to_string().into(),
                skills: open.commands().into(),
                cited: Default::default(),
                note: None,
                earlier: None,
                parent: None,
                queued: 0,
            };
            let Some(entity) = pane.transcript() else {
                return;
            };
            let selection = self.selection.clone();
            entity.update(cx, |view, cx| {
                view.reconcile(
                    presentation,
                    || {
                        let mut blocks =
                            pane::rendered_window(transcript.blocks()).to_vec();
                        let first = transcript.blocks().len() - blocks.len();
                        blocks.drain(..skip.saturating_sub(first).min(blocks.len()));
                        let disclosure = pane.transcript_disclosure_snapshot();
                        ferrite_pane::transcript::TranscriptBody {
                            blocks,
                            preview: pane.preview.clone(),
                            expanded: disclosure.0,
                            target: disclosure.1,
                        }
                    },
                    selection,
                    cx,
                );
            });
        });
    }

    /// The Panel's `+` Side chat: open `owner`'s side chat, or add a tab to
    /// it, asking the agent for the fork that holds the conversation.
    pub(super) fn open_side_chat(&mut self, owner: ThreadKey, cx: &mut Context<Self>) {
        let asked = self.ask_side_fork(owner, cx);
        let id = self
            .side_chat
            .update(cx, |chat, cx| chat.open(owner, asked, cx));
        self.upsert_surface(owner, SurfaceId::SideChat(id), cx);
        cx.notify();
    }

    /// Ask `owner`'s agent for a fork of the whole conversation, or say
    /// why it cannot make one now.
    fn ask_side_fork(&mut self, owner: ThreadKey, cx: &mut gpui::App) -> Result<(), SharedString> {
        let open = self
            .cockpit
            .read(cx)
            .thread(owner)
            .ok_or_else(|| SharedString::from("That thread is not open"))?;
        if transcript_glue::rewind_gate(&open).is_none() {
            return Err("Its agent cannot start a side chat".into());
        }
        self.cockpit
            .update(cx, |cockpit, _| cockpit.fork(owner, None, None))
            .map_err(|why| SharedString::from(why))
    }

    /// A scene's side chat: the tab as the agent would leave it before the
    /// fork answers, with no agent asked.
    #[cfg(feature = "visual-reference")]
    pub(crate) fn fixture_side_chat(&mut self, owner: ThreadKey, cx: &mut Context<Self>) {
        let id = self
            .side_chat
            .update(cx, |chat, cx| chat.push_tab(owner, None, cx));
        self.upsert_surface(owner, SurfaceId::SideChat(id), cx);
    }

    /// Whose side chat a focus that names no `fork_of` is for. Only a
    /// Server from before `Capability::ForkOf` sends a fork's focus that
    /// way, and then the one Thread waiting on a fork takes it: with two
    /// waiting it could be either's. Delete once no such Server runs.
    pub(super) fn unnamed_fork_owner(&self, thread: ThreadKey, cx: &gpui::App) -> Option<ThreadKey> {
        if self.cockpit.read(cx).names_fork_owners() {
            return None;
        }
        self.side_chat
            .read(cx)
            .only_waiting()
            .filter(|owner| *owner != thread)
    }

    /// A Thread the agent made from `owner`, for a fork its side chat
    /// asked: its tab takes it, and the window stays where it was. False
    /// when no tab of `owner` was waiting — an ordinary fork, for the
    /// window to land on.
    pub(super) fn bind_side_thread(
        &mut self,
        owner: ThreadKey,
        thread: ThreadKey,
        cx: &mut Context<Self>,
    ) -> bool {
        let inherited = self
            .cockpit
            .read(cx)
            .thread(thread)
            .map_or(0, |open| open.transcript().blocks().len());
        let fork = Fork { thread, inherited };
        let Some(taken) = self.side_chat.update(cx, |chat, cx| chat.bind(owner, fork, cx)) else {
            return false;
        };
        // The fork joined the roster as any open Thread does: it leaves it.
        self.cockpit.update(cx, |cockpit, _| cockpit.set_aside(thread));
        self.sync_panes(cx);
        if !taken {
            // Its tab closed while the agent worked: nothing wants it.
            self.retire(thread, cx);
        }
        cx.notify();
        true
    }

    /// Put a fork away with its tab. Deleted, so it leaves no Thread
    /// behind — but a fork that works in a legacy worktree would take the
    /// operator's checkout with it, so that one is only parked.
    fn retire(&mut self, fork: ThreadKey, cx: &mut gpui::App) {
        let thread = fork;
        let worktree = self.cockpit.read(cx).thread(thread).is_some_and(|open| {
            matches!(open.workspace(), Some(WorkspaceBinding::Worktree { .. }))
        });
        if worktree {
            let _ = self.cockpit.update(cx, |cockpit, _| cockpit.park(thread));
        } else if let Err(error) = self.delete_thread(thread, cx) {
            ferrite_core::log!(
                "side chat thread {} could not be deleted: {error:?}",
                thread.thread()
            );
        }
    }

    /// Its Surface's tab closed: the side chat and its fork go.
    pub(super) fn close_side_tab(&mut self, owner: ThreadKey, id: u64, cx: &mut Context<Self>) {
        if let Some(fork) = self.side_chat.update(cx, |chat, cx| chat.close(owner, id, cx)) {
            self.retire(fork, cx);
        }
    }

    /// Whether the shown side chat's field holds the keyboard, so ⌘W
    /// closes its tab (`close_panel_in_focus`).
    pub(super) fn side_chat_holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.focused_thread(cx).is_some_and(|owner| {
            self.side_chat.read(cx).holds_focus(owner, window, cx)
        })
    }

    pub(super) fn on_side_chat_event(
        &mut self,
        _: &Entity<SideChatView>,
        event: &SideChatEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SideChatEvent::Ask { tab, fork, draft } => self.ask_side_chat(*tab, *fork, draft, cx),
            SideChatEvent::BringBack { owner, fork } => self.bring_back(*owner, *fork, window, cx),
            SideChatEvent::Trash { owner, tab } => {
                self.close_surface(*owner, &SurfaceId::SideChat(*tab), cx)
            }
        }
    }

    /// A tab's question goes to its fork (queued behind an answer still
    /// running); one the fork does not take goes back to the field.
    fn ask_side_chat(&mut self, tab: u64, fork: ThreadKey, draft: &SentDraft, cx: &mut Context<Self>) {
        let text = draft.prompt().trim().to_string();
        let admitted = if self
            .cockpit
            .read(cx)
            .thread(fork)
            .is_some_and(|open| open.needs_queue())
        {
            self.cockpit.update(cx, |cockpit, _| cockpit.queue(fork, text))
        } else {
            matches!(
                self.cockpit.update(cx, |cockpit, _| cockpit.send(fork, text)),
                Admission::Delivered | Admission::Held
            )
        };
        if !admitted {
            self.side_chat
                .update(cx, |chat, cx| chat.unsent(tab, draft.clone(), cx));
        }
        cx.notify();
    }

    /// Everything the side chat draws from: the tab the Panel shows, and
    /// what `conversation` (the view's own, by Thread and tab) holds. Its
    /// transcript is told here what its fork holds after its history.
    pub(super) fn side_chat_inputs<'a>(
        &self,
        conversation: impl FnOnce(ThreadKey, u64) -> Option<(Fork, &'a PaneView)>,
        cx: &mut Context<Self>,
    ) -> Option<SideChatInputs> {
        let thread = self.panes[self.panel_drawn_pane(cx)?].thread()?;
        let owner = thread;
        let Some(SurfaceId::SideChat(tab)) = self.active_surface(owner) else {
            return None;
        };
        let conversation = conversation(owner, tab);
        if let Some((fork, pane)) = conversation {
            self.sync_compact_transcript(fork.thread, &Subject::Main, pane, fork.inherited, cx);
        }
        let fork = conversation.map(|(fork, _)| fork);
        let answered = fork.is_some_and(|fork| {
            self.cockpit
                .read(cx)
                .thread(fork.thread)
                .is_some_and(|open| open.transcript().blocks().len() > fork.inherited)
        });
        Some(SideChatInputs {
            owner,
            tab,
            shown: self.panel_thread(cx) == Some(owner),
            answered,
            blocked: self.bring_back_blocked(owner, fork, cx),
            confirm_delete: self.prefs.settings.confirm_delete,
        })
    }

    /// Why Bring back cannot run for `owner`'s side chat now (T3's
    /// reasons), or None: the main Thread must be idle, and the side chat's
    /// `fork` must hold a finished reply.
    pub(super) fn bring_back_blocked(&self, owner: ThreadKey, fork: Option<Fork>, cx: &gpui::App) -> Option<&'static str> {
        use ferrite_core::transcript::Status;
        let busy = |thread: ThreadKey| {
            self.cockpit
                .read(cx)
                .thread(thread)
                .is_none_or(|open| matches!(open.transcript().status(), Status::Streaming | Status::Blocked))
        };
        if busy(owner) {
            return Some("Available when the main thread is idle.");
        }
        let replied = fork
            .and_then(|fork| self.side_run(fork, cx))
            .is_some_and(|run| !run.is_empty());
        (!replied).then_some("Available after a finished reply.")
    }

    /// `fork`'s latest run, its question and the reply, as plain text: None
    /// while the fork is answering or has not answered.
    fn side_run(&self, fork: Fork, cx: &gpui::App) -> Option<String> {
        use ferrite_core::transcript::{Body, Status};
        let open = self.cockpit.read(cx).thread(fork.thread)?;
        let transcript = open.transcript();
        if transcript.status() != Status::Idle {
            return None;
        }
        let blocks = transcript.blocks().get(fork.inherited..)?;
        let asked = blocks.iter().rposition(|block| matches!(block.body, Body::Prompt(_)))?;
        let run = &blocks[asked..];
        if run.len() < 2 {
            return None;
        }
        Some(ferrite_core::transcript::plain_text(run).trim().to_string())
    }

    /// Bring back: `fork`'s latest run goes into the main Thread's next
    /// message, quoted in its composer (T3's merge-back, which adds the
    /// side chat to that message). Nothing while it cannot run.
    fn bring_back(&mut self, owner: ThreadKey, fork: Fork, window: &mut Window, cx: &mut Context<Self>) {
        use gpui::component::{notification::Notification, WindowExt};
        if self.bring_back_blocked(owner, Some(fork), cx).is_some() {
            return;
        }
        let (Some(run), Some(index)) = (self.side_run(fork, cx), self.pane_for(owner)) else {
            return;
        };
        let quote = ferrite_core::replies::compose(&[Reply::new(format!("From a side chat:\n{run}"), "")], "");
        self.panes[index]
            .composer
            .update(cx, |composer, cx| composer.insert(&format!("{quote}\n\n"), cx));
        window.push_notification(
            Notification::new()
                .title("Brought back")
                .message("The main thread gets this side chat with your next message there."),
            cx,
        );
        cx.notify();
    }

    /// Whether side chat `id`'s fork is answering.
    pub(super) fn side_chat_running(&self, owner: ThreadKey, id: u64, cx: &gpui::App) -> bool {
        self.side_chat
            .read(cx)
            .fork(owner, id)
            .and_then(|fork| self.cockpit.read(cx).thread(fork.thread))
            .is_some_and(|open| open.transcript().status() == ferrite_core::transcript::Status::Streaming)
    }
}

impl SideChatView {
    pub(super) fn new(cockpit: WeakEntity<CockpitView>) -> Self {
        Self {
            cockpit,
            chats: HashMap::new(),
            asked: HashMap::new(),
            next_tab: 0,
            focus_wanted: false,
            trash_armed: None,
        }
    }

    /// Whether `thread` is a tab's fork: no Pane, nav row or palette entry
    /// of its own.
    pub(crate) fn owns(&self, thread: ThreadKey) -> bool {
        self.chats.values().flat_map(|chat| &chat.tabs).any(|tab| {
            tab.conversation
                .as_ref()
                .is_some_and(|c| c.fork.thread == thread)
        })
    }

    /// The Thread whose side chat `thread` is a fork of: the head's
    /// parent.
    pub(crate) fn owner_of(&self, thread: ThreadKey) -> Option<ThreadKey> {
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

    fn tab_mut(&mut self, id: u64) -> Option<&mut Tab> {
        self.chats
            .values_mut()
            .find_map(|chat| chat.tabs.iter_mut().find(|tab| tab.id == id))
    }

    /// Side chat `id`'s fork, once the agent has made it.
    pub(super) fn fork(&self, owner: ThreadKey, id: u64) -> Option<Fork> {
        Some(self.tab(owner, id)?.conversation.as_ref()?.fork)
    }

    /// Side chat `id`'s title, for its tab.
    pub(super) fn title(&self, owner: ThreadKey, id: u64) -> SharedString {
        self.tab(owner, id)
            .map_or_else(|| "Side chat".into(), |tab| tab.title.clone())
    }

    /// The shown tab's field.
    fn field(&self, owner: ThreadKey) -> Option<&Entity<Composer>> {
        let chat = self.chats.get(&owner)?;
        Some(&chat.tabs.get(chat.active)?.field)
    }

    fn holds_focus(&self, owner: ThreadKey, window: &Window, cx: &gpui::App) -> bool {
        self.field(owner).is_some_and(|field| {
            field
                .read(cx)
                .focus_target(window, cx)
                .contains_focused(window, cx)
        })
    }

    /// One more side chat, empty, for `owner`. `asked` is whether the
    /// agent was asked for its fork, or why it could not be.
    fn open(
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
    fn bind(&mut self, owner: ThreadKey, fork: Fork, cx: &mut Context<Self>) -> Option<bool> {
        let id = self.asked.remove(&owner)?;
        if self.tab_mut(id).is_none() {
            return Some(false);
        }
        let pane = PaneView::new(fork.thread, cx);
        if let Some(entity) = pane.transcript() {
            cx.subscribe(&entity, Self::on_transcript_event).detach();
        }
        if let Some(tab) = self.tab_mut(id) {
            tab.conversation = Some(Conversation { fork, pane });
        }
        cx.notify();
        Some(true)
    }

    /// The Thread whose fork a side chat waits on, when it is the only one.
    fn only_waiting(&self) -> Option<ThreadKey> {
        let mut waiting = self.asked.keys();
        match (waiting.next(), waiting.next()) {
            (Some(owner), None) => Some(*owner),
            _ => None,
        }
    }

    /// An ordinary fork of `owner` was asked, so one its side chat asked
    /// before has settled (the engine makes one at a time): what lands
    /// next is the window's, not that tab's.
    pub(super) fn forget_fork(&mut self, owner: ThreadKey) {
        self.asked.remove(&owner);
    }

    /// Tab `id` of `owner` goes. Its fork, if it had one, is the Cockpit's
    /// to retire.
    fn close(&mut self, owner: ThreadKey, id: u64, cx: &mut Context<Self>) -> Option<ThreadKey> {
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
    /// the fork exists, or for an empty line.
    fn ask(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(tab) = self.tab_mut(id) else {
            return;
        };
        let Some(conversation) = &tab.conversation else {
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
    fn unsent(&mut self, id: u64, draft: SentDraft, cx: &mut Context<Self>) {
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
        let (shown, said) = match &tab.conversation {
            Some(conversation) if inputs.answered => (conversation.pane.transcript(), None),
            _ => (
                None,
                Some(tab.refused.clone().unwrap_or("No messages yet".into())),
            ),
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
        tools::surface_bar()
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
    #[cfg(test)]
    pub(crate) fn titles(&self, owner: ThreadKey) -> (Vec<SharedString>, usize) {
        self.chats.get(&owner).map_or((Vec::new(), 0), |chat| {
            (
                chat.tabs.iter().map(|tab| tab.title.clone()).collect(),
                chat.active,
            )
        })
    }

    /// Every fork a tab holds.
    #[cfg(test)]
    pub(crate) fn threads(&self, owner: ThreadKey) -> Vec<ThreadKey> {
        self.chats.get(&owner).map_or_else(Vec::new, |chat| {
            chat.tabs
                .iter()
                .filter_map(|tab| tab.conversation.as_ref().map(|c| c.fork.thread))
                .collect()
        })
    }

    /// The shown tab's fork, once it has one — what the tests drive.
    #[cfg(test)]
    pub(crate) fn shown_fork(&self, owner: ThreadKey) -> Option<ThreadKey> {
        let chat = self.chats.get(&owner)?;
        Some(chat.tabs.get(chat.active)?.conversation.as_ref()?.fork.thread)
    }

    /// Tab `index`'s field — what the tests type in.
    #[cfg(test)]
    pub(crate) fn field_of(&self, owner: ThreadKey, index: usize) -> Option<&Entity<Composer>> {
        Some(&self.chats.get(&owner)?.tabs.get(index)?.field)
    }

    /// What the shown tab's field holds — what the tests read.
    #[cfg(test)]
    pub(crate) fn draft(&self, owner: ThreadKey, cx: &gpui::App) -> Option<String> {
        Some(self.field(owner)?.read(cx).text().to_string())
    }

    /// The shown tab's pieces, once it has a fork — what the tests press.
    #[cfg(test)]
    pub(crate) fn pane(&self, owner: ThreadKey) -> Option<&PaneView> {
        let chat = self.chats.get(&owner)?;
        Some(&chat.tabs.get(chat.active)?.conversation.as_ref()?.pane)
    }
}

impl Render for SideChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(cockpit) = self.cockpit.upgrade() else {
            return div().into_any_element();
        };
        let chats = &self.chats;
        let conversation = |owner: ThreadKey, id: u64| {
            let tab = chats.get(&owner)?.tabs.iter().find(|tab| tab.id == id)?;
            let conversation = tab.conversation.as_ref()?;
            Some((conversation.fork, &conversation.pane))
        };
        let Some(inputs) =
            cockpit.update(cx, |cockpit, cx| cockpit.side_chat_inputs(conversation, cx))
        else {
            return div().into_any_element();
        };
        self.follow(&inputs, window, cx);
        self.body(&inputs, cx)
    }
}
