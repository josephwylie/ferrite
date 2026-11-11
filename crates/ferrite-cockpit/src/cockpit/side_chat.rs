//! The side chat (Claude's, `claude/sidechat-open.png`): quick questions
//! about a Thread that leave it as it is. Each conversation is a Surface of
//! the Thread's Panel (ADR 0039), one tab per fork: `Start a side chat` in
//! the selection's pill, or the Panel's `+` menu (Side chat, S), opens one
//! beside the Thread, which reflows narrower; the tab's × ends it.
//!
//! A tab's conversation is a fork of the Thread (`Cockpit::fork`, the
//! whole of it): the agent's own context, none of the Thread's state. The
//! fork is a Thread of the engine that no Pane holds — it is never in the
//! roster, so the nav, the board, focus cycling and the layout do not know
//! it — and it is retired with its tab (`retire`). The fork carries the
//! Thread's history, which the tab does not show: only what follows it
//! (`Conversation::inherited` blocks in). The pane draws it with the
//! transcript components the Pane's own use, compact, over a Field
//! Composer of its own: the selection's passage quoted in it as `> text`
//! when the chat was opened from a selection, `Ask a quick question`
//! when it is empty, ↵ sending.
//!
//! The fork answers asynchronously (`SideChats::asked`): the tab is there
//! at once, `No messages yet`, and takes the fork's Thread when the agent
//! has made it (`bind`). A fork the agent refuses leaves its reason in the
//! tab's place.

use crate::cockpit::surfaces::SurfaceId;
use std::collections::VecDeque;

use super::*;

use ferrite_core::activity::Subject;
use ferrite_core::workspace::WorkspaceBinding;
use ferrite_pane::composer::Role;
use ferrite_pane::quote::Reply;
use ferrite_ui::icons::{self, icon};

gpui::actions!(side_chat, [Ask]);

/// The field's key context: its line's, where ⏎ sends the question
/// instead of the Pane's prompt.
pub(crate) const KEY_CONTEXT: &str = "SideChat";

/// A tab's fork, once the agent has made it.
struct Conversation {
    thread: ThreadId,
    /// What the tab draws it with: the Pane's pieces (its transcript, its
    /// tool disclosures), never in the grid.
    pane: PaneView,
    /// How many blocks the fork held when it was made: the Thread's
    /// history, which the tab leaves unshown.
    inherited: usize,
}

struct Tab {
    id: u64,
    title: SharedString,
    field: Entity<Composer>,
    conversation: Option<Conversation>,
    /// Why the agent would not fork, in place of `No messages yet`.
    refused: Option<SharedString>,
}

/// One Thread's side chats, and the one its Panel last showed (kept in
/// step with the Panel's tabs by `tend_side_chat`).
struct Chat {
    tabs: Vec<Tab>,
    active: usize,
    /// Tabs ever made, which names the next: `Side chat 2`.
    made: usize,
}

/// Every Thread's side chat, and the forks still on their way.
#[derive(Default)]
pub(crate) struct SideChats {
    chats: std::collections::HashMap<ThreadId, Chat>,
    /// Forks asked and not yet landed — the owner's Thread and the tab
    /// waiting — oldest first, the order the agent answers in.
    asked: VecDeque<(ThreadId, u64)>,
    next_tab: u64,
    /// The field wants the keyboard (a chat opened, a tab changed): the
    /// next frame hands it over.
    focus_wanted: bool,
}

impl SideChats {
    /// Whether `thread` is a tab's fork: no Pane, nav row or palette entry
    /// of its own.
    pub(crate) fn owns(&self, thread: ThreadId) -> bool {
        self.chats.values().flat_map(|chat| &chat.tabs).any(|tab| {
            tab.conversation
                .as_ref()
                .is_some_and(|c| c.thread == thread)
        })
    }

    /// The shown tab's field.
    fn field(&self, owner: ThreadId) -> Option<&Entity<Composer>> {
        let chat = self.chats.get(&owner)?;
        Some(&chat.tabs.get(chat.active)?.field)
    }

    fn tab_mut(&mut self, id: u64) -> Option<(ThreadId, &mut Tab)> {
        self.chats.iter_mut().find_map(|(owner, chat)| {
            chat.tabs
                .iter_mut()
                .find(|tab| tab.id == id)
                .map(|tab| (*owner, tab))
        })
    }

    /// The titles of `owner`'s tabs and the shown one's index — what the
    /// tests read.
    #[cfg(test)]
    pub(crate) fn titles(&self, owner: ThreadId) -> (Vec<SharedString>, usize) {
        self.chats.get(&owner).map_or((Vec::new(), 0), |chat| {
            (
                chat.tabs.iter().map(|tab| tab.title.clone()).collect(),
                chat.active,
            )
        })
    }

    /// Every fork a tab holds.
    #[cfg(test)]
    pub(crate) fn threads(&self, owner: ThreadId) -> Vec<ThreadId> {
        self.chats.get(&owner).map_or_else(Vec::new, |chat| {
            chat.tabs
                .iter()
                .filter_map(|tab| tab.conversation.as_ref().map(|c| c.thread))
                .collect()
        })
    }

    /// Tab `index`'s field — what the tests type in.
    #[cfg(test)]
    pub(crate) fn field_of(&self, owner: ThreadId, index: usize) -> Option<&Entity<Composer>> {
        Some(&self.chats.get(&owner)?.tabs.get(index)?.field)
    }

    /// What the shown tab's field holds — what the tests read.
    #[cfg(test)]
    pub(crate) fn draft(&self, owner: ThreadId, cx: &gpui::App) -> Option<String> {
        Some(self.field(owner)?.read(cx).text().to_string())
    }
}

impl CockpitView {
    /// `Start a side chat` (the selection's pill, `passage` the selection):
    /// open `owner`'s side chat, or add a tab to it, asking the agent for
    /// the fork that holds the conversation.
    pub(super) fn open_side_chat(
        &mut self,
        owner: ThreadId,
        passage: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.new_side_tab(owner, passage, cx);
        cx.notify();
    }

    /// One more side chat, empty, for `owner`: asks the agent for its
    /// fork, and waits for it (`bind_side_thread`).
    fn new_side_tab(&mut self, owner: ThreadId, passage: Option<String>, cx: &mut Context<Self>) {
        let asked = self.ask_side_fork(owner);
        let id = self.push_side_tab(owner, passage, asked.clone().err(), cx);
        if asked.is_ok() {
            self.side_chats.asked.push_back((owner, id));
        }
    }

    /// Ask `owner`'s agent for a fork of the whole conversation, or say
    /// why it cannot make one now.
    fn ask_side_fork(&mut self, owner: ThreadId) -> Result<(), SharedString> {
        let open = self
            .cockpit
            .thread(owner)
            .ok_or_else(|| SharedString::from("That thread is not open"))?;
        if transcript_glue::rewind_gate(&open).is_none() {
            return Err("Its agent cannot start a side chat".into());
        }
        self.cockpit
            .fork(owner, None, None)
            .map_err(|why| SharedString::from(why))
    }

    /// A tab in `owner`'s side chat, shown, its field holding the passage
    /// as a quote when there is one. `refused` is why it has no fork.
    fn push_side_tab(
        &mut self,
        owner: ThreadId,
        passage: Option<String>,
        refused: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> u64 {
        let field = cx.new(|cx| {
            let mut line = Composer::new(cx);
            line.set_role(Role::Field(Some(KEY_CONTEXT)), cx);
            line.set_visible_row_limit(ferrite_pane::composer::MAX_ROWS, cx);
            if let Some(passage) = passage {
                // The caret lands under the quote.
                let quote = ferrite_pane::quote::compose(&[Reply::new(passage, "")], "");
                line.insert(&format!("{quote}\n\n"), cx);
            }
            line
        });
        let id = self.side_chats.next_tab;
        self.side_chats.next_tab += 1;
        let chat = self.side_chats.chats.entry(owner).or_insert_with(|| Chat {
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
        self.side_chats.focus_wanted = true;
        self.upsert_surface(owner, SurfaceId::SideChat(id), cx);
        id
    }

    /// A scene's side chat: the tab as the agent would leave it before the
    /// fork answers, with no agent asked.
    #[cfg(feature = "visual-reference")]
    pub(crate) fn fixture_side_chat(
        &mut self,
        owner: ThreadId,
        passage: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.push_side_tab(owner, passage, None, cx);
    }

    /// A Thread the agent made for a fork the side chat asked: its tab
    /// takes it, and the window stays where it was. False when no tab was
    /// waiting — an ordinary fork, for the window to land on.
    pub(super) fn bind_side_thread(&mut self, thread: ThreadId, cx: &mut Context<Self>) -> bool {
        let Some((_, id)) = self.side_chats.asked.pop_front() else {
            return false;
        };
        // The fork joined the roster as any open Thread does: it leaves it.
        self.cockpit.set_aside(thread);
        self.sync_panes(cx);
        let inherited = self
            .cockpit
            .thread(thread)
            .map_or(0, |open| open.transcript().blocks().len());
        let pane = PaneView::new(thread, cx);
        if let Some(entity) = pane.transcript() {
            cx.subscribe(&entity, Self::side_transcript_event).detach();
        }
        if let Some((_, tab)) = self.side_chats.tab_mut(id) {
            tab.conversation = Some(Conversation {
                thread,
                pane,
                inherited,
            });
        } else {
            // Its tab closed while the agent worked: nothing wants it.
            self.retire(thread);
        }
        cx.notify();
        true
    }

    /// Put a fork away with its tab. Deleted, so it leaves no Thread
    /// behind — but a fork that works in a legacy worktree would take the
    /// operator's checkout with it, so that one is only parked.
    fn retire(&mut self, thread: ThreadId) {
        let worktree = self.cockpit.thread(thread).is_some_and(|open| {
            matches!(open.workspace(), Some(WorkspaceBinding::Worktree { .. }))
        });
        if worktree {
            let _ = self.cockpit.park(thread);
        } else if let Err(error) = self.cockpit.delete(thread) {
            ferrite_core::log!("side chat thread {thread} could not be deleted: {error:?}");
        }
    }

    /// Its Surface's tab closed: the side chat and its fork go.
    pub(super) fn close_side_tab(&mut self, owner: ThreadId, id: u64, cx: &mut Context<Self>) {
        let Some(chat) = self.side_chats.chats.get_mut(&owner) else {
            return;
        };
        let Some(index) = chat.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let tab = chat.tabs.remove(index);
        chat.active = chat.active.min(chat.tabs.len().saturating_sub(1));
        self.side_chats.asked.retain(|(_, id)| *id != tab.id);
        if chat.tabs.is_empty() {
            self.side_chats.chats.remove(&owner);
        }
        if let Some(conversation) = tab.conversation {
            self.retire(conversation.thread);
        }
        self.side_chats.focus_wanted = true;
        cx.notify();
    }

    /// Every side chat of `owner` goes, and their tabs (what the tests do
    /// between cases).
    #[cfg(test)]
    pub(super) fn close_side_chat(&mut self, owner: ThreadId, cx: &mut Context<Self>) {
        let Some(chat) = self.side_chats.chats.remove(&owner) else {
            return;
        };
        for tab in chat.tabs {
            self.forget_surface(owner, &SurfaceId::SideChat(tab.id));
            self.side_chats.asked.retain(|(_, id)| *id != tab.id);
            if let Some(conversation) = tab.conversation {
                self.retire(conversation.thread);
            }
        }
        cx.notify();
    }

    /// ⏎ in the shown tab's field: the question goes to its fork (queued
    /// behind an answer still running). Nothing before the fork exists, or
    /// for an empty line.
    pub(super) fn send_side_chat(&mut self, _: &Ask, _: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.focused_thread() else {
            return;
        };
        let Some(chat) = self.side_chats.chats.get(&owner) else {
            return;
        };
        let Some(tab) = chat.tabs.get(chat.active) else {
            return;
        };
        let Some(thread) = tab.conversation.as_ref().map(|c| c.thread) else {
            return;
        };
        let field = tab.field.clone();
        if field.read(cx).text().trim().is_empty() {
            return;
        }
        let text = field
            .update(cx, |field, cx| field.take(cx))
            .trim()
            .to_string();
        if self
            .cockpit
            .thread(thread)
            .is_some_and(|open| open.needs_queue())
        {
            self.cockpit.queue(thread, text);
        } else {
            self.cockpit.send(thread, text);
        }
        if let Some(entity) = self.side_transcript(owner) {
            entity.update(cx, |transcript, cx| transcript.scroll_to_bottom(cx));
        }
        cx.notify();
    }

    /// The shown tab's transcript entity, once it has a fork.
    fn side_transcript(
        &self,
        owner: ThreadId,
    ) -> Option<Entity<ferrite_pane::transcript::TranscriptView>> {
        let chat = self.side_chats.chats.get(&owner)?;
        chat.tabs
            .get(chat.active)?
            .conversation
            .as_ref()?
            .pane
            .transcript()
    }

    /// The shown tab's fork, once it has one — what the tests drive.
    #[cfg(test)]
    pub(crate) fn side_thread(&self, owner: ThreadId) -> Option<ThreadId> {
        let chat = self.side_chats.chats.get(&owner)?;
        Some(chat.tabs.get(chat.active)?.conversation.as_ref()?.thread)
    }

    /// Every frame: follow the side chat each Panel shows, hand the field
    /// the keyboard it asked for, and keep each tab's transcript told what
    /// its fork holds.
    pub(super) fn tend_side_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let owners: Vec<ThreadId> = self.side_chats.chats.keys().copied().collect();
        for owner in &owners {
            let Some(SurfaceId::SideChat(id)) = self.active_surface(*owner) else {
                continue;
            };
            let Some(chat) = self.side_chats.chats.get_mut(owner) else {
                continue;
            };
            if let Some(at) = chat.tabs.iter().position(|tab| tab.id == id) {
                if chat.active != at {
                    chat.active = at;
                    self.side_chats.focus_wanted = true;
                }
            }
        }
        if let Some(owner) = self.focused_thread().filter(|owner| {
            matches!(
                self.pane_for(*owner).and_then(|index| self.shown_surface(index)),
                Some(SurfaceId::SideChat(_))
            )
        }) {
            if std::mem::take(&mut self.side_chats.focus_wanted) {
                if let Some(field) = self.side_chats.field(owner) {
                    let focus = field.read(cx).focus_target(window, cx);
                    window.focus(&focus, cx);
                }
            }
        }
        for owner in owners {
            self.sync_side_transcript(owner, cx);
        }
    }

    /// Whether the shown side chat's field holds the keyboard: the pump
    /// leaves it there rather than handing it to the Pane's Composer.
    pub(super) fn side_chat_holds_focus(&self, window: &Window, cx: &gpui::App) -> bool {
        self.focused_thread()
            .and_then(|owner| self.side_chats.field(owner))
            .is_some_and(|field| {
                field
                    .read(cx)
                    .focus_target(window, cx)
                    .contains_focused(window, cx)
            })
    }

    /// Tell the shown tab's transcript what its fork holds after the
    /// history it inherited.
    fn sync_side_transcript(&mut self, owner: ThreadId, cx: &mut Context<Self>) {
        let Some(conversation) = self
            .side_chats
            .chats
            .get(&owner)
            .and_then(|chat| chat.tabs.get(chat.active))
            .and_then(|tab| tab.conversation.as_ref())
        else {
            return;
        };
        self.sync_compact_transcript(
            conversation.thread,
            &Subject::Main,
            &conversation.pane,
            conversation.inherited,
            None,
            cx,
        );
    }

    /// Tell `pane`'s transcript what `subject` of `thread` holds after its
    /// first `skip` blocks, `note` standing under the first prompt — the
    /// compact, read-only twin of `sync_transcript`, for the right pane's
    /// bodies (the side chat's tabs, a subagent's drill-in): no Decision
    /// strip, subagents or rewinds, and no Reply (they have no Composer of
    /// a Pane's to reply into).
    pub(super) fn sync_compact_transcript(
        &self,
        thread: ThreadId,
        subject: &Subject,
        pane: &PaneView,
        skip: usize,
        note: Option<SharedString>,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self.cockpit.thread(thread) else {
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
            thread,
            starting: open.starting(),
            solo: false,
            focused: true,
            reduce_motion: cx.reduce_motion(),
        });
        let workspace =
            ferrite_core::workspace::effective_cwd(open.session_project_root(), open.workspace())
                .map(std::path::Path::to_path_buf);
        let key = ferrite_pane::transcript::TranscriptKey {
            namespace: namespace.clone(),
            content_revision: revision,
            display_revision: pane.disclosure_revision(),
            focused: true,
            signal_status: Some(status),
            reading_size,
            working: working.as_ref().map(|working| working.key),
            tail: None,
            pending_call: None,
            workspace: workspace.clone(),
            solo: false,
            thinking: self.prefs.settings.show_thinking,
            agents: Default::default(),
            rewind: None,
            quotes: false,
            skills: open.commands().len(),
            note: note.clone(),
            queued: 0,
        };
        let Some(entity) = pane.transcript() else {
            return;
        };
        if entity.read(cx).matches(&key) {
            return;
        }
        let mut blocks = pane::rendered_window(transcript.blocks(), Level::Transcript).to_vec();
        let first = transcript.blocks().len() - blocks.len();
        blocks.drain(..skip.saturating_sub(first).min(blocks.len()));
        let disclosure = pane.transcript_disclosure_snapshot();
        let input = ferrite_pane::transcript::TranscriptInput {
            thread,
            namespace,
            content_revision: revision,
            display_revision: pane.disclosure_revision(),
            blocks,
            signal_status: Some(status),
            focused: true,
            reading_size,
            selection_scope: pane.selection_scope,
            preview: pane.preview.clone(),
            expanded: disclosure.0,
            target: disclosure.1,
            disclosure_focus: disclosure.2,
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
            note,
            queued: 0,
        };
        let selection = self.selection.clone();
        entity.update(cx, |transcript, cx| transcript.sync(input, selection, cx));
    }

    /// A tab's transcript asks for something: a tool row folds or unfolds
    /// there. The rest (a path's reader, a subagent) is the Pane's.
    fn side_transcript_event(
        &mut self,
        entity: Entity<ferrite_pane::transcript::TranscriptView>,
        event: &ferrite_pane::transcript::TranscriptEvent,
        cx: &mut Context<Self>,
    ) {
        let ferrite_pane::transcript::TranscriptEvent::ToggleDisclosure(call) = event else {
            return;
        };
        for tab in self
            .side_chats
            .chats
            .values_mut()
            .flat_map(|chat| chat.tabs.iter_mut())
        {
            if let Some(conversation) = &mut tab.conversation {
                if conversation.pane.transcript().as_ref() == Some(&entity) {
                    conversation.pane.clear_tool_target();
                    conversation.pane.toggle_tool(call);
                }
            }
        }
        cx.notify();
    }

    // ------------------------------------------------------------ drawing

    /// Side chat `id`'s Surface: the conversation or its empty word, the
    /// composer.
    pub(super) fn side_chat_body(&self, owner: ThreadId, id: u64, cx: &mut Context<Self>) -> AnyElement {
        use ferrite_ui::theme::*;
        let Some(tab) = self
            .side_chats
            .chats
            .get(&owner)
            .and_then(|chat| chat.tabs.iter().find(|tab| tab.id == id))
        else {
            return div().into_any_element();
        };
        let (shown, said) = match &tab.conversation {
            Some(conversation)
                if self
                    .cockpit
                    .thread(conversation.thread)
                    .is_some_and(|open| {
                        open.transcript().blocks().len() > conversation.inherited
                    }) =>
            {
                (conversation.pane.transcript(), None)
            }
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
            .child(body)
            .child(self.side_chat_composer(tab, cx))
            .into_any_element()
    }

    /// Side chat `id`'s title, for its tab.
    pub(super) fn side_chat_title(&self, owner: ThreadId, id: u64) -> SharedString {
        self.side_chats
            .chats
            .get(&owner)
            .and_then(|chat| chat.tabs.iter().find(|tab| tab.id == id))
            .map_or_else(|| "Side chat".into(), |tab| tab.title.clone())
    }

    /// The composer at the foot: a `FIELD` card with the line, its
    /// placeholder while empty, and `↵` at its own foot.
    fn side_chat_composer(&self, tab: &Tab, cx: &mut Context<Self>) -> Div {
        use ferrite_ui::theme::*;
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
            .on_action(cx.listener(Self::send_side_chat))
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
}
