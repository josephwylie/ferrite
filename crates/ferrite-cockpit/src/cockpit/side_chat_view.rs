//! The window's half of the side chat (`ferrite_prs::side_chat`): a tab's
//! conversation is a fork of the Thread, a Thread of the engine that no
//! Pane holds, so asking for it, binding it to its tab, telling its
//! transcript what it holds and retiring it stay the Cockpit's. What the
//! side chat draws of the window it reads through the `Source` made here
//! (`side_chat_inputs`, asked with the forks its tabs hold); what it asks
//! (a question sent, Bring back, Trash) it says as a `SideChatEvent`. The
//! shown fork's transcript is told what the fork holds where each Pane's
//! is (`sync_visible_transcripts`), before the frame draws.

use ferrite_core::activity::Subject;
use ferrite_core::checkout::WorkspaceBinding;
use ferrite_core::prompt_queue::Admission;
use ferrite_core::replies::Reply;
use ferrite_core::ThreadKey;
use ferrite_pane::composer::SentDraft;
use ferrite_prs::side_chat::{Fork, Forks, SideChatEvent, SideChatInputs, SideChatView};
use ferrite_ui::source::Source;

use super::*;
use crate::cockpit::surfaces::SurfaceId;

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
        cx: &mut gpui::App,
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
                ferrite_core::checkout::effective_cwd(open.session_project_root(), open.workspace())
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
        let can = self.can_side_fork(owner, cx);
        let asking = can.is_ok();
        let id = self
            .side_chat
            .update(cx, |chat, cx| chat.open(owner, can, cx));
        self.upsert_surface(owner, SurfaceId::SideChat(id), cx);
        if asking {
            let asked = self.cockpit.update(cx, |cockpit, _| cockpit.fork(owner, None, None));
            self.when_answered(
                asked,
                move |view, asked, cx| {
                    if let Err(why) = asked {
                        view.side_chat
                            .update(cx, |chat, cx| chat.refused(owner, id, why.to_string().into(), cx));
                    }
                },
                cx,
            );
        }
        cx.notify();
    }

    /// Whether `owner`'s agent can be asked for a fork of the whole
    /// conversation now, or why not.
    fn can_side_fork(&self, owner: ThreadKey, cx: &gpui::App) -> Result<(), SharedString> {
        let open = self
            .cockpit
            .read(cx)
            .thread(owner)
            .ok_or_else(|| SharedString::from("That thread is not open"))?;
        if transcript_glue::rewind_gate(&open).is_none() {
            return Err("Its agent cannot start a side chat".into());
        }
        Ok(())
    }

    /// A scene's side chat: the tab as the agent would leave it before the
    /// fork answers, with no agent asked.
    #[cfg(feature = "visual-reference")]
    pub(crate) fn fixture_side_chat(&mut self, owner: ThreadKey, cx: &mut Context<Self>) {
        let id = self
            .side_chat
            .update(cx, |chat, cx| chat.fixture_tab(owner, cx));
        self.upsert_surface(owner, SurfaceId::SideChat(id), cx);
    }

    /// Whose side chat a focus that names no `fork_of` is for. Only a
    /// Server from before `Capability::ForkOf` sends a fork's focus that
    /// way, and then the one Thread waiting on a fork takes it: with two
    /// waiting it could be either's. Delete once no such Server runs.
    pub(super) fn unnamed_fork_owner(&self, thread: ThreadKey, cx: &gpui::App) -> Option<ThreadKey> {
        if self.cockpit.read(cx).names_fork_owners(thread.machine()) {
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
        let fork = Fork {
            thread,
            inherited: self.held_history(thread, cx),
        };
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

    /// How many blocks `fork` holds, once the window holds its history.
    fn held_history(&self, fork: ThreadKey, cx: &gpui::App) -> Option<usize> {
        let open = self.cockpit.read(cx).thread(fork)?;
        open.history_loaded().then(|| open.transcript().blocks().len())
    }

    /// Each fork whose history has landed since its tab took it counts
    /// what it inherited, before its tab shows or asks anything.
    pub(super) fn settle_side_forks(&mut self, cx: &mut Context<Self>) {
        let unsettled = self.side_chat.read(cx).unsettled();
        for fork in unsettled {
            if let Some(inherited) = self.held_history(fork, cx) {
                self.side_chat.update(cx, |chat, cx| chat.settle(fork, inherited, cx));
            }
        }
    }

    /// Put a fork away with its tab. Deleted, so it leaves no Thread
    /// behind — but a fork that works in a legacy worktree would take the
    /// operator's checkout with it, so that one is only parked.
    fn retire(&mut self, fork: ThreadKey, cx: &mut Context<Self>) {
        let thread = fork;
        let worktree = self.cockpit.read(cx).thread(thread).is_some_and(|open| {
            matches!(open.workspace(), Some(WorkspaceBinding::Worktree { .. }))
        });
        if worktree {
            // Nothing follows it: the fork has no Pane to close.
            let _ = self.cockpit.update(cx, |cockpit, _| cockpit.park(thread));
            return;
        }
        self.delete_threads(
            vec![thread],
            |_, deleted, _| {
                for (thread, deleted) in deleted {
                    if let Err(error) = deleted {
                        ferrite_core::log!(
                            "side chat thread {} could not be deleted: {error:?}",
                            thread.thread()
                        );
                    }
                }
            },
            cx,
        );
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
        let draft = draft.clone();
        let settle = move |view: &mut Self, admitted: bool, cx: &mut Context<Self>| {
            if !admitted {
                view.side_chat
                    .update(cx, |chat, cx| chat.unsent(tab, draft, cx));
            }
            cx.notify();
        };
        if self
            .cockpit
            .read(cx)
            .thread(fork)
            .is_some_and(|open| open.needs_queue())
        {
            let queued = self.cockpit.update(cx, |cockpit, _| cockpit.queue(fork, text));
            self.when_answered(
                queued,
                move |view, queued, cx| settle(view, queued.unwrap_or(false), cx),
                cx,
            );
        } else {
            let sent = self.cockpit.update(cx, |cockpit, _| cockpit.send(fork, text));
            self.when_answered(
                sent,
                move |view, sent, cx| {
                    let admitted = matches!(sent, Ok(Admission::Delivered | Admission::Held));
                    settle(view, admitted, cx)
                },
                cx,
            );
        }
        cx.notify();
    }

    /// The fork of the side chat the Panel shows, once it has one: a
    /// Thread whose records the window draws.
    pub(super) fn side_chat_drawn(&self, cx: &gpui::App) -> Option<ThreadKey> {
        let (owner, tab) = self.side_chat_shown(cx)?;
        Some(self.side_chat.read(cx).fork(owner, tab)?.thread)
    }

    /// The window's side chat, reading it through a `Source` over the root.
    pub(super) fn new_side_chat(cx: &mut Context<Self>) -> Entity<SideChatView> {
        let inputs = Source::of(&cx.entity(), |root: &Self, forks: &Forks, cx| {
            root.side_chat_inputs(forks, cx)
        });
        cx.new(|_| SideChatView::new(inputs))
    }

    /// The Thread whose Panel shows a side chat this frame, and its tab.
    fn side_chat_shown(&self, cx: &gpui::App) -> Option<(ThreadKey, u64)> {
        let owner = self.panes[self.panel_drawn_pane(cx)?].thread()?;
        let Some(SurfaceId::SideChat(tab)) = self.active_surface(owner) else {
            return None;
        };
        Some((owner, tab))
    }

    /// The shown side chat's transcript is told what its fork holds after
    /// its history, as each Pane's is (`sync_visible_transcripts`): before
    /// the frame draws it.
    pub(super) fn sync_side_chat_transcript(&self, cx: &mut Context<Self>) {
        let Some((owner, tab)) = self.side_chat_shown(cx) else {
            return;
        };
        self.side_chat.update(cx, |chat, cx| {
            if let Some((Fork { thread, inherited: Some(inherited) }, pane)) =
                chat.shown_conversation(owner, tab)
            {
                self.sync_compact_transcript(thread, &Subject::Main, pane, inherited, cx);
            }
        });
    }

    /// Everything the side chat draws from: the tab the Panel shows, and
    /// what its fork (`forks`, the view's own, by tab) holds.
    fn side_chat_inputs(&self, forks: &Forks, cx: &gpui::App) -> Option<SideChatInputs> {
        let (owner, tab) = self.side_chat_shown(cx)?;
        let fork = forks.of(tab);
        let answered = fork.is_some_and(|fork| {
            fork.inherited.is_some_and(|inherited| {
                self.cockpit
                    .read(cx)
                    .thread(fork.thread)
                    .is_some_and(|open| open.transcript().blocks().len() > inherited)
            })
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
        let blocks = transcript.blocks().get(fork.inherited?..)?;
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
            .composer(cx)
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
