//! The cockpit's side of a transcript: what `sync_transcript` hands each
//! retained `TranscriptView` besides its blocks (its tail row,
//! the call a pending Decision gates, the checkout its paths resolve
//! against, the subagents its calls started), and the commands that act on the transcript the operator is
//! reading — copy and resend the prompt in view, scroll to a target, open a
//! path in a reader.
//!
//! The prompt band answers nothing under the pointer, so copy and resend
//! are commands here (the palette and the keymap call them), acting on the
//! prompt whose band is pinned at the top of the body — the turn being read
//! — else the latest.

use std::path::PathBuf;

use ferrite_core::activity::{ActivityView, AgentStatus};
use ferrite_core::transcript::BlockId;
use ferrite_core::ThreadKey;
use ferrite_pane::pane::{AgentClock, AgentFace, AgentFaces, AgentRun};
use ferrite_pane::transcript::{PromptAction, ScrollTarget, TranscriptTail};
use gpui::{App, ClipboardItem, Context, SharedString, Window};

use super::*;

/// Where a prompt sent again went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Handoff {
    /// On the wire: the agent's turn began.
    Sent,
    /// Behind a running turn, or held for a Session still starting.
    Queued,
}

/// Whether a sent prompt's Edit, Rewind and Fork can be asked of `open`
/// now (`TranscriptPresentation::rewind`): not drawn when its agent offers none,
/// disabled with the reason while a turn runs, prompts wait, a request
/// waits for an answer, or the agent is changing — the cut never races
/// them. Core refuses the same, in the same words.
pub(super) fn rewind_gate(
    open: &ferrite_client::cockpit::ThreadView<'_>,
) -> Option<Result<(), SharedString>> {
    if !open.supports_control(ferrite_core::ControlKind::Rewind) {
        return None;
    }
    let why = if open.busy() {
        Some("a turn is running; stop it first")
    } else if !open.queue().is_empty() {
        Some("prompts are queued; send or take them back first")
    } else if open.pending().is_some() {
        Some("a request waits for an answer")
    } else if open.tuning() {
        Some("its agent is changing; try again in a moment")
    } else {
        None
    };
    Some(why.map_or(Ok(()), |why| Err(why.into())))
}

/// How each subagent stands, by the call that started it, for that call's
/// row (`pane::render_agent`, T3's statuses): running while it starts or
/// works, waiting on the operator, failed, stopped, or done; what it does
/// (its latest progress summary) while it runs or waits; once it has ended,
/// its last words as its result. Every Subject's transcript reads the same
/// faces: a subagent's own agent calls open its children.
pub(super) fn agent_faces(
    activity: ActivityView<'_>,
    provider: ferrite_core::provider::Provider,
) -> AgentFaces {
    let mut faces = AgentFaces::default();
    for (call, agent) in activity.invocations() {
        let run = agent_run(agent);
        let live = matches!(run, AgentRun::Running | AgentRun::Waiting);
        let transcript = agent.transcript();
        let summary = live
            .then(|| {
                let detail = transcript.progress().detail.trim();
                (!detail.is_empty()).then(|| detail.to_owned())
            })
            .flatten();
        let result = (!live).then(|| last_words(transcript.blocks())).flatten();
        let clock = agent_clock(activity, agent, &call);
        faces.insert(
            call,
            AgentFace {
                provider,
                run,
                summary,
                result,
                clock,
                on: None,
            },
        );
    }
    faces
}

fn agent_run(agent: ferrite_core::activity::AgentView<'_>) -> AgentRun {
    match agent.status() {
        AgentStatus::Failed | AgentStatus::NotFound => AgentRun::Failed,
        AgentStatus::Waiting if agent.running() => AgentRun::Waiting,
        _ if agent.running() => AgentRun::Running,
        AgentStatus::Interrupted => AgentRun::Stopped,
        _ => AgentRun::Done,
    }
}

/// How long an agent has run, as its card row and its Lineage row both read
/// it: since its turn began while it runs, else its own stored run, else the
/// time `call` (the one that started it) took.
pub(super) fn agent_clock(
    activity: ActivityView<'_>,
    agent: ferrite_core::activity::AgentView<'_>,
    call: &str,
) -> Option<AgentClock> {
    let live = matches!(agent_run(agent), AgentRun::Running | AgentRun::Waiting);
    match agent.transcript().turn_started() {
        Some(at) if live => Some(AgentClock::Since(at)),
        _ => agent.ran().map(settled_clock),
    }
    .or_else(|| call_took(activity, agent, call))
}

/// A finished run's clock: placed on the wall clock when its end was
/// stamped, else (an older log's `7:31 pm`) only how long it took.
fn settled_clock(ran: ferrite_core::activity::AgentRan) -> AgentClock {
    ran.ended
        .and_then(|to| Some(AgentClock::Ran { from: to.checked_sub(ran.took)?, to }))
        .unwrap_or(AgentClock::Took(ran.took))
}

/// How long a settled agent ran where no run of its own was observed: the
/// time its parent's `call` took. A background launch's clock runs on to
/// its handback, which reports the agent's own duration.
fn call_took(
    activity: ActivityView<'_>,
    agent: ferrite_core::activity::AgentView<'_>,
    call: &str,
) -> Option<AgentClock> {
    let parent = activity.subject(agent.info().parent.as_ref()?)?;
    match parent.timings().get(call)? {
        ferrite_core::activity::ToolTiming::Done(took) => Some(AgentClock::Took(*took)),
        ferrite_core::activity::ToolTiming::Running(_) => None,
    }
}

/// What an agent said last: the prose that closes its transcript (its
/// answer), after whatever it last ran.
fn last_words(blocks: &[ferrite_core::transcript::Block]) -> Option<String> {
    use ferrite_core::transcript::Body;
    let mut words: Vec<String> = blocks
        .iter()
        .rev()
        .filter(|block| !matches!(block.body, Body::TurnEnd(_) | Body::Meta(_)))
        .map_while(|block| match &block.body {
            Body::Paragraph { spans } | Body::Heading { spans, .. } | Body::Bullet { spans } => {
                Some(spans.iter().map(|span| span.text.as_str()).collect::<String>())
            }
            _ => None,
        })
        .collect();
    words.reverse();
    let words = words.join("\n");
    (!words.trim().is_empty()).then_some(words)
}

/// What the failure strip says of a failed turn: the provider's reason,
/// its first line (the strip is one line), in the operator's words where
/// Ferrite wrote it — a Session that closed mid-turn says so plainly.
fn failure_words(message: &str) -> SharedString {
    use ferrite_core::transcript::TurnEnd;
    if message == TurnEnd::UNFINISHED {
        return "The session ended before this turn finished".into();
    }
    match message.lines().map(str::trim).find(|line| !line.is_empty()) {
        Some(line) => line.to_owned().into(),
        None => "This turn failed".into(),
    }
}

impl CockpitView {
    /// The row after a transcript's last (`tail_row`). What the operator
    /// answers in it (a Decision's cards, a Retry) keeps where it is drawn
    /// in the Pane's geometry for the kit's toasts to stand clear of
    /// (`toast_home`): the row clears it as it draws, before its cards
    /// draw; with no row, nothing is kept.
    pub(super) fn transcript_tail(&self, index: usize, cx: &Context<Self>) -> Option<TranscriptTail> {
        use ferrite_pane::pane::PaneGeometry;
        let geometry = self.panes.get(index)?.read(cx).geometry.clone();
        let Some(tail) = self.tail_row(index, cx) else {
            geometry.set(PaneGeometry { answers: Default::default(), ..geometry.get() });
            return None;
        };
        let row = tail.render;
        Some(TranscriptTail {
            key: tail.key,
            render: std::rc::Rc::new(move |window: &mut Window, cx: &mut App| {
                use gpui::prelude::*;
                let geometry = geometry.clone();
                let cleared = ferrite_ui::components::on_bounds(gpui::div(), move |_, _, _| {
                    geometry.set(PaneGeometry { answers: Default::default(), ..geometry.get() })
                });
                gpui::div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_w_0()
                    .child(cleared)
                    .child(row(window, cx))
                    .into_any_element()
            }),
        })
    }

    /// The row after a transcript's last: the pending Decision, as its
    /// card is built for the tail (`CockpitView::decision_tail`, the
    /// Decision card's owner), else the failure strip or the
    /// background-work line, under the line that says a Delegate waits on
    /// the operator (T3's `ThreadNow`: the line that needs you first).
    fn tail_row(&self, index: usize, cx: &Context<Self>) -> Option<TranscriptTail> {
        if let Some(decision) = self.decision_tail(index, cx) {
            return Some(decision);
        }
        let below = self
            .failure_tail(index, cx.weak_entity(), cx)
            .or_else(|| self.background_work_tail(index, cx));
        match (self.delegate_waiting_tail(index, cx), below) {
            (Some(above), Some(below)) => Some(TranscriptTail {
                key: above.key ^ below.key.rotate_left(1),
                render: std::rc::Rc::new(move |window: &mut Window, cx: &mut App| {
                    use gpui::prelude::*;
                    gpui::div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .child((above.render)(window, cx))
                        .child((below.render)(window, cx))
                        .into_any_element()
                }),
            }),
            (above, below) => above.or(below),
        }
    }

    /// The failure strip (SPEC §5.9 item 4) under Main's transcript when the
    /// Thread's last turn failed: why (`failure_words`), and `Retry`, which
    /// re-sends the failed turn's prompt (`retry_failed_turn`) — none for a
    /// closed Session, which has nothing to send to. A turn that failed for
    /// its CLI not being signed in on another Machine offers that sign-in
    /// beside it (`sign_in_cli`).
    fn failure_tail(&self, index: usize, owner: gpui::WeakEntity<Self>, cx: &App) -> Option<TranscriptTail> {
        use ferrite_core::transcript::Status;
        let pane = self.panes.get(index)?;
        if !pane.read(cx).is_main() {
            return None;
        }
        let thread = pane.thread()?;
        let open = self.cockpit.read(cx).thread(thread)?;
        let turn = open.turn();
        let Some(ferrite_core::TurnOutcome::Error(message)) = &turn.outcome else {
            return None;
        };
        if turn.status == Status::Streaming {
            return None;
        }
        let closed = turn.status == Status::Closed;
        let words = failure_words(message);
        let machine = thread.machine();
        let provider = open.provider();
        let sign_in = (self.elsewhere(machine, cx) && ferrite_core::sign_in::signed_out(provider, message))
            .then(|| SharedString::from(format!("Sign in to {provider} on {}", self.place_of(machine, cx))));
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            ("failure", thread, words.as_ref(), closed, sign_in.as_deref()).hash(&mut hasher);
            hasher.finish()
        };
        let geometry = pane.read(cx).geometry.clone();
        Some(TranscriptTail {
            key,
            render: std::rc::Rc::new(move |_: &mut Window, cx: &mut App| {
                use gpui::prelude::*;
                let strip = thread.thread().get();
                let sign_in = sign_in.clone().map(|label| {
                    let owner = owner.clone();
                    ferrite_pane::pane::strip_button("failure-sign-in", strip, label, cx).on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |view, cx| view.sign_in_cli(machine, provider, cx));
                    })
                });
                let retry = (!closed).then(|| {
                    let owner = owner.clone();
                    ferrite_pane::pane::strip_button("failure-retry", strip, "Retry", cx).on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |view, cx| view.retry_failed_turn(thread, cx));
                    })
                });
                let presses = (sign_in.is_some() || retry.is_some()).then(|| {
                    let geometry = geometry.clone();
                    let presses = gpui::div()
                        .flex()
                        .flex_shrink_0()
                        .gap(gpui::px(ferrite_ui::theme::SPACE_2))
                        .children(sign_in)
                        .children(retry);
                    // The presses are what the operator answers here.
                    ferrite_ui::components::on_bounds(presses, move |at, _, _| {
                        let kept = geometry.get();
                        let answers = kept.answers.with_cards(at).with_buttons(at);
                        geometry.set(ferrite_pane::pane::PaneGeometry { answers, ..kept })
                    })
                    .into_any_element()
                });
                ferrite_pane::pane::failure_strip(strip, words.clone(), presses).into_any_element()
            }),
        })
    }

    /// `Retry` on the failure strip: send the failed turn's prompt again,
    /// attachments and all, through the one resend path.
    pub(crate) fn retry_failed_turn(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let prompt = self.cockpit.read(cx).thread(thread).and_then(|open| {
            let main = open
                .activity()
                .subject(&ferrite_core::activity::Subject::Main)?;
            main.transcript()
                .blocks()
                .iter()
                .rev()
                .find_map(|block| match &block.body {
                    ferrite_core::transcript::Body::Prompt(line) => Some((line.clone(), block.wake.is_some())),
                    _ => None,
                })
                // A wake's turn is the Server's to send again, not the
                // operator's.
                .and_then(|(line, wake)| (!wake).then_some(line))
        });
        if let Some(prompt) = prompt {
            self.send_again(index, thread, prompt, cx);
        }
    }

    /// ⌘-click on a path target: the file in the Thread's preview, in the
    /// Panel (`open_preview`), at the link's line. The transcript's event
    /// carries no window, so the preview opens on the next turn of the
    /// loop, in the window that holds the cockpit.
    pub(super) fn open_preview_from(
        &mut self,
        index: usize,
        path: PathBuf,
        line: Option<u32>,
        cx: &mut Context<Self>,
    ) {
        let Some(identity) = self.panes.get(index).map(|pane| pane.identity) else {
            return;
        };
        let Some(window) = cx.active_window() else {
            return;
        };
        let cockpit = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = cockpit.update(cx, |view, cx| {
                    let Some(index) = view.index_of(identity) else {
                        return;
                    };
                    view.focus_pane(index, cx);
                    view.open_preview(path, line, window, cx);
                });
            });
        });
    }

    /// The prompt the operator is reading in Pane `index`: the pinned
    /// band's turn, else the latest prompt in view.
    fn reading_prompt(&self, index: usize, cx: &App) -> Option<String> {
        self.panes
            .get(index)?
            .read(cx).transcript()?
            .read(cx)
            .reading_prompt()
    }

    /// Copy the prompt being read in Pane `index` (its words, without its
    /// attachments). Whether there was one.
    pub(crate) fn copy_reading_prompt(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let Some(prompt) = self.reading_prompt(index, cx) else {
            return false;
        };
        let (words, _) = ferrite_core::prompt_files::split(prompt);
        cx.write_to_clipboard(ClipboardItem::new_string(words));
        true
    }

    /// Send the prompt being read in Pane `index` again, attachments and
    /// all — queued behind a running turn. Only a Thread's Main Subject
    /// takes prompts. Whether there was one to send.
    pub(crate) fn resend_reading_prompt(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        if !self.panes.get(index).is_some_and(|pane| pane.read(cx).is_main()) {
            return false;
        }
        let Some(thread) = self.panes[index].thread() else {
            return false;
        };
        let Some(prompt) = self.reading_prompt(index, cx) else {
            return false;
        };
        self.send_again(index, thread, prompt, cx);
        true
    }

    /// A press on a sent prompt's Edit, Rewind or Fork in Pane `index`:
    /// Edit and Fork ask at once; Rewind opens its menu at `at`, whose rows
    /// say what the cut does not bring back.
    pub(super) fn prompt_action(
        &mut self,
        index: usize,
        block: BlockId,
        action: PromptAction,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.focus_pane(index, cx);
        let Some(thread) = self.panes[index].thread() else {
            return;
        };
        match action {
            PromptAction::Rewind => {
                self.open_context_menu(super::MenuTarget::Prompt { thread, block }, at, cx)
            }
            PromptAction::Edit | PromptAction::Fork => {
                self.cut_at_prompt(thread, block, action, false, cx)
            }
        }
    }

    /// Ask `thread` for `action` at the prompt Block `block` (its files
    /// too when `files`); a refusal is said in its Pane.
    pub(super) fn cut_at_prompt(
        &mut self,
        thread: ThreadKey,
        block: BlockId,
        action: PromptAction,
        files: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(prompt) = self.cockpit.read(cx).thread(thread).and_then(|open| {
            open.transcript()
                .blocks()
                .iter()
                .find(|shown| shown.id == block)?
                .prompt_id
                .clone()
        }) else {
            return;
        };
        let asked = match action {
            PromptAction::Edit => self.cockpit.update(cx, |cockpit, _| cockpit.rewind(thread, &prompt, false, true)),
            // Claude's "Takes this and what follows out of the
            // conversation": its words come back to the Composer, to edit
            // and send again.
            PromptAction::Rewind => self.cockpit.update(cx, |cockpit, _| cockpit.rewind(thread, &prompt, files, true)),
            PromptAction::Fork => return self.fork_thread(thread, Some(prompt), None, cx),
        };
        self.when_answered(
            asked,
            move |view, asked, cx| {
                if let Err(why) = asked {
                    view.cockpit
                        .update(cx, |cockpit, _| cockpit.say(thread, format!("rewind refused: {why}")));
                }
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    /// A new Thread from `thread`'s conversation before the prompt `before`
    /// names, or all of it, `draft` in its Composer; it opens focused once
    /// the agent answers (`Engine::fork`). A refusal is said in `thread`.
    pub(super) fn fork_thread(
        &mut self,
        thread: ThreadKey,
        before: Option<String>,
        draft: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let asked = self.cockpit.update(cx, |cockpit, _| cockpit.fork(thread, before.as_deref(), draft));
        self.when_answered(
            asked,
            move |view, asked, cx| {
                match asked {
                    Ok(()) => view.side_chat.update(cx, |chat, _| chat.forget_fork(thread)),
                    Err(why) => view
                        .cockpit
                        .update(cx, |cockpit, _| cockpit.say(thread, format!("fork refused: {why}"))),
                }
                cx.notify();
            },
            cx,
        );
        cx.notify();
    }

    /// An answer's Fork: the conversation through the answer ending at
    /// `after` — before the next prompt, or all of it when the answer is
    /// the latest.
    pub(super) fn fork_after(&mut self, thread: ThreadKey, after: BlockId, cx: &mut Context<Self>) {
        let next = self.cockpit.read(cx).thread(thread).and_then(|open| {
            open.transcript()
                .blocks()
                .iter()
                .skip_while(|shown| shown.id != after)
                .skip(1)
                .find_map(|shown| shown.prompt_id.clone())
        });
        self.fork_thread(thread, next, None, cx);
    }

    /// `thread`'s latest prompt a rewind can name.
    pub(super) fn last_prompt(&self, thread: ThreadKey, cx: &App) -> Option<BlockId> {
        self.cockpit
            .read(cx)
            .thread(thread)?
            .transcript()
            .blocks()
            .iter()
            .rev()
            .find(|shown| shown.prompt_id.is_some())
            .map(|shown| shown.id)
    }

    /// Scroll Pane `index`'s transcript to `target`: the top, the live
    /// tail, or a turn's band with its output under it (a scene's views).
    #[cfg_attr(not(feature = "visual-reference"), allow(dead_code))]
    pub(crate) fn scroll_transcript(
        &mut self,
        index: usize,
        target: ScrollTarget,
        cx: &mut Context<Self>,
    ) {
        if let Some(transcript) = self.panes.get(index).and_then(|pane| pane.read(cx).transcript()) {
            transcript.update(cx, |transcript, cx| transcript.scroll_to(target, cx));
        }
    }
}

/// A read-only body the paging keys move: a scrolled document, or the Files
/// editor's lines.
pub(super) enum Reader {
    Document(ScrollHandle),
    Code(Entity<tools::Editor>),
}

impl CockpitView {
    /// Compare a visible Subject's key every root render, and only then copy
    /// its bounded blocks and timings into its retained entity — with its
    /// tail row, the call a pending Decision gates, and the
    /// checkout its path targets resolve against (`transcript_glue`).
    pub(super) fn sync_transcript(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((thread, subject, namespace, focused, selection_scope, disclosure_revision)) =
            self.panes.get(index).and_then(|pane| {
                pane.thread().map(|thread| {
                    (
                        thread,
                        pane.read(cx).selected.clone(),
                        pane.read(cx).text_namespace(),
                        index == self.focused(cx),
                        pane.read(cx).selection_scope,
                        pane.read(cx).disclosure_revision(),
                    )
                })
            })
        else {
            return;
        };
        let Some(subject_view) = self
            .cockpit
            .read(cx)
            .thread(thread)
            .and_then(|open| open.activity().subject(&subject))
        else {
            return;
        };
        // What the root's own reads give, before the model is lent to the
        // transcript's update below.
        let earlier = Self::earlier_history(subject_view.transcript(), self.panes[index].read(cx).is_main());
        let delegates = self.delegate_faces(thread, subject_view.transcript(), cx);
        let parent = self.delegator_link(index, cx);
        let tail = self.transcript_tail(index, cx);
        let solo = self.on_plane(thread, cx);
        self.wake_for_background_clock(index, cx);
        let entity = self.panes[index]
            .update(cx, |pane, cx| pane.ensure_transcript(cx))
            .expect("thread Pane has a transcript entity");
        if self.transcript_entities.insert(entity.entity_id()) {
            cx.subscribe(&entity, Self::transcript_event).detach();
        }
        // One reading size for every transcript, on every board.
        let reading_size = self.prefs.settings.reading_size;
        let reduce_motion = cx.reduce_motion();
        let pane = &self.panes[index];
        // What the transcript's body takes of the Pane, should it build one:
        // read now, as the transcript's update below holds `cx`.
        let (disclosure, preview) = {
            let pane = pane.read(cx);
            (pane.transcript_disclosure_snapshot(), pane.preview.clone())
        };
        let selection = self.selection.clone();
        self.cockpit.update(cx, |model, cx| {
            let Some(open) = model.thread(thread) else {
                return;
            };
            let Some(subject_view) = open.activity().subject(&subject) else {
                return;
            };
            let revision = subject_view.presentation_revision();
            let status = pane::transcript_status(subject_view.status(), subject_view.fresh());
            // The gated call hides while its Decision waits as the tail row.
            let pending_call = tail
                .as_ref()
                .and_then(|_| open.pending())
                .map(|decision| decision.tool_use_id.clone())
                .filter(|call| !call.is_empty());
            let workspace =
                ferrite_core::checkout::effective_cwd(open.session_project_root(), open.workspace())
                    .map(std::path::Path::to_path_buf);
            // While the turn runs, its header under its prompt and its live
            // row at the tail (`pane::working_tail`), over a Decision that
            // waits.
            let working = pane::working_tail(pane::WorkingFacts {
                transcript: subject_view.transcript(),
                thread: thread.thread(),
                starting: open.starting(),
                solo,
                focused,
                reduce_motion,
            });
            let mut agents = transcript_glue::agent_faces(open.activity(), open.provider());
            for (call, face) in delegates {
                agents.insert(call, face);
            }
            let rewind = transcript_glue::rewind_gate(&open);
            // A subagent's tab draws no Composer to quote into.
            let quotes = pane.read(cx).is_main();
            let queued = open.queue().len();
            let presentation = ferrite_pane::transcript::TranscriptPresentation {
                thread: thread.thread(),
                selection_scope,
                disclosure_focus: pane.read(cx).tool_focus(),
                namespace,
                content_revision: revision,
                display_revision: disclosure_revision,
                focused,
                signal_status: Some(status),
                reading_size,
                working,
                tail,
                pending_call,
                workspace,
                solo,
                thinking: self.prefs.settings.show_thinking,
                agents,
                rewind,
                quotes,
                reply_to: open.provider().to_string().into(),
                skills: open.commands().into(),
                cited: model
                    .saved(thread)
                    .and_then(|saved| self.cited_sources.of(&saved))
                    .map(std::borrow::Cow::Borrowed)
                    .unwrap_or_default(),
                note: None,
                earlier,
                parent,
                queued,
            };
            let transcript = subject_view.transcript();
            entity.update(cx, |view, cx| {
                view.reconcile(
                    presentation,
                    move || {
                        ferrite_pane::transcript::TranscriptBody {
                            blocks: pane::rendered_window(transcript.blocks()).to_vec(),
                            preview,
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

    /// Whether the Thread began before the first row `transcript` draws: its
    /// Blocks were dropped to the limit, or the pane's window is cut from
    /// them. Main's whole history can be copied out: its Server replays the
    /// log (`copy_transcript`).
    pub(super) fn earlier_history(
        transcript: &ferrite_core::transcript::Transcript,
        main: bool,
    ) -> Option<ferrite_pane::transcript::Earlier> {
        let dropped = transcript.earlier_dropped();
        let cut = pane::rendered_window(transcript.blocks()).len()
            < transcript.blocks().len();
        (dropped || cut).then_some(ferrite_pane::transcript::Earlier { copyable: main })
    }

    /// Whether `thread`'s Pane is the window's plane, on its black ground,
    /// rather than drawn by the Factory, which draws a Worker or a
    /// Coordinator on its own ground.
    pub(super) fn on_plane(&self, thread: ThreadKey, cx: &gpui::App) -> bool {
        self.factory_worker_drawn(cx) != Some(thread)
    }

    pub(super) fn sync_visible_transcripts(&mut self, cx: &mut Context<Self>) {
        let mut shown = self.visible_indices(cx);
        // The Factory draws a Worker or its Coordinator the roster does not
        // show (its Task's Worker tab, the Coordinator surface).
        shown.extend(
            self.factory_worker_drawn(cx)
                .and_then(|thread| self.pane_for(thread))
                .filter(|index| !shown.contains(index)),
        );
        shown.into_iter().for_each(|index| self.sync_transcript(index, cx));
        self.sync_side_chat_transcript(cx);
    }

    /// Core's child-retention budget evicted these Subjects. Release only
    /// their retained row trees; normal tab switches retain scroll state.
    pub(super) fn release_evicted_transcripts(
        &mut self,
        index: usize,
        changed: &[ferrite_client::cockpit::SubjectUpdate],
        cx: &mut gpui::App,
    ) {
        let Some(thread) = self.panes[index].thread() else {
            return;
        };
        let Some(activity) = self.cockpit.read(cx).thread(thread).map(|thread| thread.activity()) else {
            return;
        };
        let evicted: Vec<ferrite_core::activity::Subject> = changed
            .iter()
            .map(|change| activity.canonical_subject(&change.subject))
            .filter(|subject| activity.subject(subject).is_some_and(|subject| !subject.retained()))
            .collect();
        for subject in evicted {
            if let Some(transcript) = self.panes[index].update(cx, |pane, _| pane.release_transcript(&subject)) {
                self.transcript_entities.remove(&transcript.entity_id());
            }
        }
    }

    /// PageUp, PageDown, Home or End on the focused Pane's transcript; the
    /// details card, holding the keyboard, takes them and leaves it be.
    pub(super) fn page_transcript(
        &mut self,
        page: Page,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.details_hold_focus(window, cx) {
            return;
        }
        if let Some(reader) = self.reader(window, cx) {
            Self::scroll_reader(&reader, page, cx);
            cx.notify();
            return;
        }
        if let Some(transcript) = self
            .panes
            .get(self.focused(cx))
            .and_then(|pane| pane.read(cx).transcript())
        {
            transcript.update(cx, |transcript, cx| transcript.page(page, cx));
        }
    }

    /// Move a read-only body by `page`, the way the transcript's own paging does.
    pub(super) fn scroll_reader(reader: &Reader, page: Page, cx: &mut Context<Self>) {
        let scroll = match reader {
            Reader::Code(editor) => return editor.read(cx).scroll_page(page),
            Reader::Document(scroll) => scroll,
        };
        let max = scroll.max_offset().y;
        let step = scroll.bounds().size.height;
        let at = scroll.offset().y;
        let to = match page {
            Page::Top => px(0.),
            Page::Bottom => -max,
            Page::Up => (at + step).min(px(0.)),
            Page::Down => (at - step).max(-max),
        };
        scroll.set_offset(gpui::point(scroll.offset().x, to));
    }

    /// The read-only body the paging keys belong to: the open preview's
    /// document when it holds focus, else the Files tab the panel's keys
    /// are on. Everything else leaves them to the transcript.
    pub(super) fn reader(&self, window: &Window, cx: &gpui::App) -> Option<Reader> {
        let index = self.focused(cx);
        let pane = self.panes.get(index)?;
        if pane.read(cx).preview.focused(window, cx) && self.preview_shown(index, cx) {
            if let Some(scroll) = self.panel.file_surface.read(cx).reader_scroll(pane.identity) {
                return Some(Reader::Document(scroll));
            }
        }
        let thread = pane.thread()?;
        let on_panel = self.panel_control_focused(window, cx) || self.panel_keys_held(window, cx);
        if !on_panel || self.active_surface(thread) != Some(crate::cockpit::surfaces::SurfaceId::Files) {
            return None;
        }
        let files = self.tools.files.read(cx);
        files
            .prose_scroll(thread)
            .map(Reader::Document)
            .or_else(|| files.shown_editor(thread).map(Reader::Code))
    }

    /// A send in Pane `index`: its prompt, once it arrives, scrolls to the
    /// top of the transcript (`TranscriptView::scroll_to_new_prompt`).
    pub(super) fn scroll_transcript_to_new_prompt(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(transcript) = self.panes[index].read(cx).transcript() {
            transcript.update(cx, |transcript, cx| transcript.scroll_to_new_prompt(cx));
        }
    }

    pub(super) fn scroll_transcript_to_bottom(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(transcript) = self.panes[index].read(cx).transcript() {
            transcript.update(cx, |transcript, cx| transcript.scroll_to_bottom(cx));
        }
    }

    pub(super) fn transcript_event(
        &mut self,
        entity: Entity<ferrite_pane::transcript::TranscriptView>,
        event: &ferrite_pane::transcript::TranscriptEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.panes.iter().position(|pane| {
            pane.read(cx).transcripts
                .values()
                .any(|transcript| transcript == &entity)
        }) else {
            return;
        };
        match event {
            ferrite_pane::transcript::TranscriptEvent::ToggleDisclosure(call) => {
                // A click just opens or closes the row; the keyboard ring is
                // Tab's alone, so a click never leaves one behind.
                self.focus_pane(index, cx);
                self.panes[index].update(cx, |pane, _| pane.clear_tool_target());
                self.panes[index].update(cx, |pane, _| pane.toggle_tool(call));
                cx.notify();
            }
            ferrite_pane::transcript::TranscriptEvent::OpenReader { path, line } => {
                // ⌘-click on a path: the Thread's preview, in the Panel.
                self.focus_pane(index, cx);
                self.open_preview_from(index, path.clone(), *line, cx);
            }
            ferrite_pane::transcript::TranscriptEvent::OpenAgent(call) => {
                self.open_agent_from(index, call.clone(), cx);
            }
            ferrite_pane::transcript::TranscriptEvent::Prompt { block, action, at } => {
                self.prompt_action(index, *block, *action, *at, cx);
            }
            // The reply waits in the Pane's tray as a quote chip, naming its
            // source; the focus pump hands the Composer the keyboard.
            ferrite_pane::transcript::TranscriptEvent::Reply(reply, block, anchor) => {
                let mut reply = reply.clone();
                if let Some((thread, block)) = self.panes[index].thread().zip(*block) {
                    reply = reply.cited_from(ferrite_core::replies::Source { thread: thread.thread(), block });
                }
                self.focus_pane(index, cx);
                self.panes[index]
                    .composer(cx)
                    .update(cx, |composer, cx| composer.cite_with_comment(reply, anchor.clone(), cx));
                cx.notify();
            }
            ferrite_pane::transcript::TranscriptEvent::Fork { after } => {
                if let Some(thread) = self.panes[index].thread() {
                    self.fork_after(thread, *after, cx);
                }
            }
            ferrite_pane::transcript::TranscriptEvent::CopyFullHistory => {
                if let Some(thread) = self.panes[index].thread() {
                    self.copy_transcript(thread, cx);
                }
            }
            ferrite_pane::transcript::TranscriptEvent::OpenParent => {
                if let Some(from) = self.panes[index]
                    .thread()
                    .and_then(|thread| self.facts.get(thread)?.delegated_from.clone())
                {
                    self.open_elsewhere(&from.server, from.thread, cx);
                }
            }
            // A wrap-up's next step: its words go as the next prompt.
            ferrite_pane::transcript::TranscriptEvent::Send(words) => {
                self.focus_pane(index, cx);
                if let Some(thread) = self.panes[index].thread() {
                    let _ = self.cockpit.update(cx, |cockpit, _| cockpit.send(thread, words.clone()));
                }
                cx.notify();
            }
            ferrite_pane::transcript::TranscriptEvent::OpenThread(thread) => {
                if let Some(here) = self.panes[index].thread() {
                    self.focus_thread(here.sibling(*thread), cx);
                }
            }
            ferrite_pane::transcript::TranscriptEvent::OpenPullRequest { number, url } => {
                self.focus_pane(index, cx);
                if let Some(thread) = self.panes[index].thread() {
                    self.open_named_pr(thread, *number, url.clone(), cx);
                }
            }
            ferrite_pane::transcript::TranscriptEvent::OpenBackground => {
                self.focus_pane(index, cx);
                if let Some(thread) = self.panes[index].thread() {
                    self.open_background(thread, cx);
                }
            }
            ferrite_pane::transcript::TranscriptEvent::OpenChanges(path) => {
                self.focus_pane(index, cx);
                let Some(thread) = self.panes[index].thread() else {
                    return;
                };
                match path {
                    Some(path) => {
                        let root = self.thread_path(thread, cx);
                        let relative = root
                            .as_deref()
                            .and_then(|root| std::path::Path::new(path).strip_prefix(root).ok())
                            .map_or_else(|| path.clone(), |path| path.display().to_string());
                        self.show_changes_of(thread, &relative, cx);
                    }
                    None => {
                        self.upsert_surface(thread, surfaces::SurfaceId::Changes, cx);
                        cx.notify();
                    }
                }
            }
        }
    }

    /// Transcript membership changes only through the pump. Prune stale
    /// expanded call ids here once per changed Thread, never per frame.
    pub(super) fn prune_tool_disclosures(&mut self, thread: ThreadKey, cx: &mut gpui::App) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let mut valid = std::collections::HashSet::new();
        if let Some(transcript) = self
            .cockpit
            .read(cx)
            .thread(thread)
            .and_then(|open| open.activity().subject(&self.panes[index].read(cx).selected))
            .map(|subject| subject.transcript())
        {
            let blocks = transcript.blocks();
            let mut turn_start = 0;
            for (at, block) in blocks.iter().enumerate() {
                match &block.body {
                    ferrite_core::transcript::Body::Tool(tool) => {
                        valid.insert(pane::DisclosureId::Tool(tool.call.clone()));
                        valid.insert(pane::DisclosureId::Group(tool.call.clone()));
                    }
                    ferrite_core::transcript::Body::Thinking(_) => {
                        valid.insert(pane::DisclosureId::Reasoning(block.id));
                        valid.insert(pane::DisclosureId::Live(block.id));
                    }
                    // A turn's fold, or a steer's, by its prompt. A steer
                    // stays inside its turn, whose card spans all of it.
                    ferrite_core::transcript::Body::Prompt(_) => {
                        valid.insert(pane::DisclosureId::Turn(block.id));
                        valid.insert(pane::DisclosureId::Prompt(block.id));
                        if !block.steer {
                            turn_start = at;
                        }
                    }
                    // A changed-files card's Expand all and its folders.
                    ferrite_core::transcript::Body::TurnEnd(_) => {
                        valid.insert(pane::DisclosureId::Changes(block.id));
                        for output in ferrite_core::transcript::outputs::of_turn(&blocks[turn_start..at]) {
                            if let ferrite_core::transcript::outputs::Output::Changes(changes) = output {
                                valid.extend(
                                    ferrite_pane::transcript::changes_folders(&changes, self.thread_path(thread, cx).as_deref())
                                        .into_iter()
                                        .map(|folder| pane::DisclosureId::Folder(block.id, folder)),
                                );
                            }
                        }
                        turn_start = at + 1;
                    }
                    _ => {}
                }
            }
        }
        self.panes[index].update(cx, |pane, _| pane.prune_tools(&valid));
    }

    /// The Panes on screen, as indices into the mirror — the roster's
    /// `visible`, in its order.
    /// Every Thread whose records this frame draws, which the window
    /// streams whole while it does; every other reads its digest (ADR
    /// 0032, F-A2): the shown Pane's Thread, or the Factory's Worker or
    /// Coordinator in its place, and the fork of the side chat the Panel
    /// shows. A subagent's transcript, the Panel's other Surfaces and a
    /// review beside the Pane read the shown Thread's own records.
    pub(super) fn transcripts_drawn(&self, cx: &gpui::App) -> Vec<ThreadKey> {
        let board = match self.factory_worker_drawn(cx) {
            Some(worker) => vec![worker],
            None => self
                .visible_indices(cx)
                .into_iter()
                .filter_map(|index| self.panes[index].thread())
                .collect(),
        };
        board.into_iter().chain(self.side_chat_drawn(cx)).collect()
    }

    pub(super) fn visible_indices(&self, cx: &gpui::App) -> Vec<usize> {
        self.cockpit
            .read(cx)
            .visible()
            .into_iter()
            .filter_map(|identity| self.index_of(identity))
            .collect()
    }

    pub(super) fn tool_cycle_previous(
        &mut self,
        _: &ToolCyclePrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sheet_up(cx)
            || self
                .focused_thread(cx)
                .and_then(|thread| self.cockpit.read(cx).thread(thread))
                .is_some_and(|thread| {
                    !thread.activity().children().is_empty()
                        || thread
                            .pending_decisions()
                            .iter()
                            .any(|request| asks_in_fields(&request.decision))
                })
        {
            window.focus_prev(cx);
        } else {
            let composer = self
                .panes
                .get(self.focused(cx))
                .map(|pane| pane.read(cx).composer.clone());
            if !composer.is_some_and(|composer| {
                composer.update(cx, |composer, cx| composer.cycle_files(true, window, cx))
            }) {
                self.cycle_tools(true, window, cx);
            }
        }
    }

    pub(super) fn cycle_tools(&mut self, reverse: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.get(self.focused(cx)).is_none_or(|pane| pane.read(cx).draft().is_some()) {
            return;
        }
        let focused = self.focused(cx);
        let calls = self.expandable_tools(focused, cx);
        let transcript = self.panes[focused].read(cx).transcript();
        let tool_focus = self.panes[focused].read(cx).tool_focus();
        let in_controls = ferrite_pane::rich::controls_focused(window);
        let cycle_controls = |from_edge, window: &mut Window, cx: &mut gpui::App| {
            transcript.as_ref().is_some_and(|transcript| {
                transcript.update(cx, |transcript, cx| {
                    transcript.cycle_controls(reverse, from_edge, window, cx)
                })
            })
        };
        // Preserve the existing disclosure walk, then include native actions
        // such as fenced-code Copy/Preview before returning to the composer.
        if in_controls && cycle_controls(false, window, cx) {
            return;
        }
        if reverse
            && !in_controls
            && !tool_focus.is_focused(window)
            && cycle_controls(true, window, cx)
        {
            return;
        }
        let targeted = if in_controls && !reverse {
            false
        } else {
            self.panes[focused].update(cx, |pane, _| pane.cycle_tools(&calls, reverse).is_some())
        };
        if !targeted && !reverse && !in_controls && cycle_controls(true, window, cx) {
            cx.notify();
            return;
        }
        let focus = if targeted {
            tool_focus
        } else {
            self.panes[focused].read(cx).composer.focus_handle(cx)
        };
        window.focus(&focus, cx);
        cx.notify();
    }

    /// ↵ on the keyboard's row: a tool call opens or folds; an agent's row,
    /// which has no details, opens its agent.
    pub(super) fn toggle_tool_action(&mut self, _: &ToggleTool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.focused_thread(cx) else {
            return;
        };
        let Some(call) = self.panes[self.focused(cx)].read(cx).targeted_tool().cloned() else {
            return;
        };
        if let pane::DisclosureId::Tool(agent) = &call {
            if self.targeted_agent(thread, &call, cx).is_some() {
                let index = self.focused(cx);
                self.open_agent_from(index, agent.clone(), cx);
                return;
            }
        }
        self.toggle_tool(thread, &call, window, cx);
    }

    /// The disclosures the Pane's transcript draws, in order: what the
    /// keyboard walks. The rows decide it — a call folded in a shut group
    /// is not a stop — so it is read from the transcript as last drawn.
    pub(super) fn expandable_tools(&self, index: usize, cx: &gpui::App) -> Vec<pane::DisclosureId> {
        if self.panes[index].thread().is_none() {
            return Vec::new();
        }
        self.panes[index]
            .read(cx).transcript()
            .map(|transcript| transcript.read(cx).disclosures())
            .unwrap_or_default()
    }

    /// ⌘F: find in the focused Pane's transcript — or in its preview, when
    /// the preview took the last press or holds the keyboard
    /// (`ferrite_pane::find`).
    pub(super) fn open_find(
        &mut self,
        _: &ferrite_pane::find::Open,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(thread) = self.panel_thread(cx) {
            let on_controls = self.panel_control_focused(window, cx);
            if self.open_files_find(thread, on_controls, window, cx) {
                return;
            }
        }
        let index = self.focused(cx);
        let Some(pane) = self.panes.get(index) else {
            return;
        };
        let (owner, preview) = (pane.identity, pane.read(cx).preview.clone());
        let reading =
            self.panel.file_surface.read(cx).pressed() == Some(owner) || preview.focused(window, cx);
        if reading
            && self.preview_shown(index, cx)
            && self
                .panel.file_surface
                .update(cx, |files, cx| files.open_find(owner, &preview, window, cx))
        {
            return;
        }
        if let Some(transcript) = self.panes.get(index).and_then(|pane| pane.read(cx).transcript()) {
            transcript.update(cx, |view, cx| view.open_find(window, cx));
        }
    }

    /// Copy the active native selection. Its retained document keeps logical
    /// transcript fragments available even while their rows are offscreen.
    pub(super) fn copy_selection(&mut self, _: &CopySelection, window: &mut Window, cx: &mut Context<Self>) {
        let text = gpui::base::TextSelection::selected_text(window, cx);
        let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Cite the selection from the keyboard, as the button beside it does.
    pub(super) fn cite_selection(&mut self, _: &CiteSelection, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(transcript) = self.panes.get(self.focused(cx)).and_then(|pane| pane.read(cx).transcript()) {
            transcript.update(cx, |transcript, cx| transcript.cite_selection(window, cx));
        }
    }

    pub(super) fn toggle_tool(
        &mut self,
        thread: ThreadKey,
        call: &pane::DisclosureId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        self.focus_pane(index, cx);
        gpui::base::TextSelection::clear(window, cx);
        let (rich, namespace) = {
            let pane = self.panes[index].read(cx);
            (pane.rich.clone(), pane.text_namespace())
        };
        rich.clear_output_selection(&namespace, cx);
        self.panes[index].update(cx, |pane, _| pane.toggle_tool(call));
        window.focus(&self.panes[index].read(cx).tool_focus(), cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::failure_words;
    use ferrite_core::transcript::TurnEnd;

    #[test]
    fn the_strip_says_why_the_turn_failed() {
        assert_eq!(
            failure_words("API Error: 529 overloaded").as_ref(),
            "API Error: 529 overloaded"
        );
        assert_eq!(
            failure_words("\nAPI Error: 500\n{\"type\":\"error\"}").as_ref(),
            "API Error: 500"
        );
        assert_eq!(failure_words("  ").as_ref(), "This turn failed");
        assert_eq!(
            failure_words(TurnEnd::UNFINISHED).as_ref(),
            "The session ended before this turn finished"
        );
    }
}
