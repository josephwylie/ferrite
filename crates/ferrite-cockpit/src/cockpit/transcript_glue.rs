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
use ferrite_core::cockpit::Admission;
use ferrite_core::transcript::BlockId;
use ferrite_core::ThreadKey;
use ferrite_pane::pane::{AgentClock, AgentFace, AgentFaces, AgentRun};
use ferrite_pane::transcript::{PromptAction, ScrollTarget, TranscriptTail};
use gpui::{App, ClipboardItem, Context, IntoElement, SharedString, Window};

use super::CockpitView;

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
    open: &ferrite_core::cockpit::ThreadView<'_>,
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
pub(super) fn agent_faces(activity: ActivityView<'_>) -> AgentFaces {
    let mut faces = AgentFaces::default();
    for (call, agent) in activity.invocations() {
        let run = match agent.status() {
            AgentStatus::Failed | AgentStatus::NotFound => AgentRun::Failed,
            AgentStatus::Waiting if agent.running() => AgentRun::Waiting,
            _ if agent.running() => AgentRun::Running,
            AgentStatus::Interrupted => AgentRun::Stopped,
            _ => AgentRun::Done,
        };
        let live = matches!(run, AgentRun::Running | AgentRun::Waiting);
        let transcript = agent.transcript();
        let summary = live
            .then(|| {
                let detail = transcript.progress().detail.trim();
                (!detail.is_empty()).then(|| detail.to_owned())
            })
            .flatten();
        let result = (!live).then(|| last_words(transcript.blocks())).flatten();
        let clock = match transcript.turn_started() {
            Some(at) if live => Some(AgentClock::Since(at)),
            _ => transcript.blocks().iter().rev().find_map(|block| match &block.body {
                ferrite_core::transcript::Body::TurnEnd(end) => end
                    .elapsed_ms
                    .map(|ms| AgentClock::Took(std::time::Duration::from_millis(ms))),
                _ => None,
            }),
        }
        .or_else(|| call_took(activity, agent, &call));
        faces.insert(
            call,
            AgentFace {
                run,
                summary,
                result,
                clock,
            },
        );
    }
    faces
}

/// How long a settled agent ran where its own transcript does not say: the
/// time its parent's `call` took, when the call waited on it (not one that
/// launched it into the background and returned at once).
fn call_took(
    activity: ActivityView<'_>,
    agent: ferrite_core::activity::AgentView<'_>,
    call: &str,
) -> Option<AgentClock> {
    let parent = activity.subject(agent.info().parent.as_ref()?)?;
    if parent.transcript().tool(call)?.background.is_some() {
        return None;
    }
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
    /// The row after a transcript's last: the pending Decision, as its
    /// card is built for the tail (`CockpitView::decision_tail`, the
    /// Decision card's owner), else the failure strip, else the
    /// background-work line.
    pub(super) fn transcript_tail(
        &self,
        index: usize,
        cx: &Context<Self>,
    ) -> Option<TranscriptTail> {
        self.decision_tail(index, cx)
            .or_else(|| self.failure_tail(index, cx.weak_entity(), cx))
            .or_else(|| self.background_work_tail(index, cx))
    }

    /// The failure strip (SPEC §5.9 item 4) under Main's transcript when the
    /// Thread's last turn failed: why (`failure_words`), and `Retry`, which
    /// re-sends the failed turn's prompt (`retry_failed_turn`) — none for a
    /// closed Session, which has nothing to send to.
    fn failure_tail(&self, index: usize, owner: gpui::WeakEntity<Self>, cx: &App) -> Option<TranscriptTail> {
        use ferrite_core::transcript::Status;
        let pane = self.panes.get(index)?;
        if !pane.is_main() {
            return None;
        }
        let thread = pane.thread()?;
        let turn = self.cockpit.read(cx).thread(thread)?.turn();
        let Some(ferrite_core::TurnOutcome::Error(message)) = &turn.outcome else {
            return None;
        };
        if turn.status == Status::Streaming {
            return None;
        }
        let closed = turn.status == Status::Closed;
        let words = failure_words(message);
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            ("failure", thread, words.as_ref(), closed).hash(&mut hasher);
            hasher.finish()
        };
        Some(TranscriptTail {
            key,
            render: std::rc::Rc::new(move |_: &mut Window, cx: &mut App| {
                let owner = owner.clone();
                let retry = (!closed).then(|| {
                    ferrite_pane::pane::retry_button(thread.thread().get(), cx)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |view, cx| view.retry_failed_turn(thread, cx));
                        })
                        .into_any_element()
                });
                ferrite_pane::pane::failure_strip(thread.thread().get(), words.clone(), retry)
                    .into_any_element()
            }),
        })
    }

    /// `Retry` on the failure strip: send the failed turn's prompt again,
    /// attachments and all, through the one resend path. Whether it went.
    pub(crate) fn retry_failed_turn(&mut self, thread: ThreadKey, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.pane_for(thread) else {
            return false;
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
        match prompt {
            Some(prompt) => self.send_again(index, thread, prompt, cx).is_ok(),
            None => false,
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
            .transcript()?
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
    /// takes prompts. Whether it went.
    pub(crate) fn resend_reading_prompt(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        if !self.panes.get(index).is_some_and(|pane| pane.is_main()) {
            return false;
        }
        let Some(thread) = self.panes[index].thread() else {
            return false;
        };
        let Some(prompt) = self.reading_prompt(index, cx) else {
            return false;
        };
        self.send_again(index, thread, prompt, cx).is_ok()
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
        if let Err(why) = asked {
            self.cockpit.update(cx, |cockpit, _| cockpit.say(thread, format!("rewind refused: {why}")));
        }
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
        match self.cockpit.update(cx, |cockpit, _| cockpit.fork(thread, before.as_deref(), draft)) {
            Ok(()) => {
                let owner = thread;
                self.side_chat.update(cx, |chat, _| chat.forget_fork(owner))
            }
            Err(why) => self.cockpit.update(cx, |cockpit, _| cockpit.say(thread, format!("fork refused: {why}"))),
        }
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

    /// Send `prompt` to Pane `index`'s Thread again, queued behind a
    /// running turn: where it went, or why it did not.
    pub(super) fn send_again(
        &mut self,
        index: usize,
        thread: ThreadKey,
        prompt: String,
        cx: &mut Context<Self>,
    ) -> Result<Handoff, String> {
        self.focus_pane(index, cx);
        let handoff = if self
            .cockpit
            .read(cx)
            .thread(thread)
            .is_some_and(|open| open.needs_queue())
        {
            if !self.cockpit.update(cx, |cockpit, _| cockpit.queue(thread, prompt)) {
                return Err("The agent's queue did not take it.".into());
            }
            Ok(Handoff::Queued)
        } else {
            match self.cockpit.update(cx, |cockpit, _| cockpit.send(thread, prompt)) {
                Admission::Delivered => {
                    self.scroll_transcript_to_new_prompt(index, cx);
                    Ok(Handoff::Sent)
                }
                Admission::Held => Ok(Handoff::Queued),
                Admission::Refused(why) => Err(why),
                Admission::Unconfirmed(why) => Err(format!(
                    "It may or may not have been sent ({why}). Check the transcript before retrying."
                )),
            }
        };
        self.facts.acted(self.cockpit.read(cx), thread);
        cx.notify();
        handoff
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
        if let Some(transcript) = self.panes.get(index).and_then(|pane| pane.transcript()) {
            transcript.update(cx, |transcript, cx| transcript.scroll_to(target, cx));
        }
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
