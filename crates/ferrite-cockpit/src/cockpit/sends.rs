//! A send keeps the whole draft until its admission is known. The Composer
//! hands over a `SentDraft`; the Engine says `Delivered`, `Held` for a
//! Session still starting, `Refused`, or (over a Server link that went
//! quiet) `Unconfirmed`. Only `Delivered` lets the draft go: a `Refused` or
//! `Unconfirmed` one goes back to the Composer at once, and a `Held` one
//! waits under its Thread for the prompt to come back if the Session never
//! starts. It belongs to the Thread, not to the Pane that sent it, so
//! switching or closing the Pane does not lose it.
//!
//! The window never waits for the admission (ADR 0041). A sent draft is
//! on its way in the `Outbox` until the Server answers; a Thread's drafts
//! go one at a time, in the order sent, so each is sent or queued by what
//! the Thread is doing once the one before it is settled. A prompt sent
//! again (`send_again`) goes the same way, with no draft to settle: what
//! follows it hears where it went.

use super::*;
use ferrite_core::prompt_queue::Admission;
use ferrite_core::draft_content::DraftContent;
use ferrite_core::ThreadKey;
use ferrite_pane::composer::SentDraft;
use ferrite_core::replies::Reply;
use std::collections::{HashMap, VecDeque};

/// How much of a prompt names it in a notice.
const NAMED_CHARS: usize = 48;

/// The drafts each Thread's Engine is holding for its Session, oldest first.
#[derive(Default)]
pub(crate) struct HeldSends(HashMap<ThreadKey, Vec<Held>>);

struct Held {
    draft: SentDraft,
    /// The Thread has been seen holding the prompt, so its absence from the
    /// queue means the Session took it.
    listed: bool,
    /// The Thread closed with the prompt still held: the Engine let it go
    /// with the Session, so the draft is all that is left of it.
    lost: bool,
}

/// The prompts on their way to each Thread, oldest first: the first is
/// asked of the Server, the rest wait for its answer.
#[derive(Default)]
pub(crate) struct Outbox(HashMap<ThreadKey, VecDeque<Outgoing>>);

struct Outgoing {
    /// The prompt as sent: a draft's, trimmed.
    text: String,
    from: Origin,
}

/// Where a prompt on its way came from, and so what its answer settles.
enum Origin {
    /// The Composer's ↵: the draft is kept until the Thread takes it.
    Composer { draft: SentDraft, cited: Vec<Reply> },
    /// A prompt sent again: what follows hears where it went.
    Replay(Replayed),
}

/// What follows a prompt sent again: where it went, or why it did not.
type Replayed = Box<dyn FnOnce(&mut CockpitView, Result<Handoff, String>, &mut Context<CockpitView>)>;

impl Outbox {
    #[cfg(test)]
    pub(crate) fn count(&self, thread: ThreadKey) -> usize {
        self.0.get(&thread).map_or(0, VecDeque::len)
    }
}

/// What the Server said of a draft on its way.
enum Answered {
    Sent(Admission),
    /// Whether the Thread's queue took it.
    Queued(bool),
}

/// A send's answer as an admission: a link that went before it answered
/// leaves the prompt unconfirmed.
pub(super) fn admission(answer: std::io::Result<Admission>) -> Admission {
    answer.unwrap_or_else(|error| Admission::Unconfirmed(error.to_string()))
}

impl HeldSends {
    #[cfg(test)]
    pub(crate) fn count(&self, thread: ThreadKey) -> usize {
        self.0.get(&thread).map_or(0, Vec::len)
    }
}

impl CockpitView {
    /// The Composer's draft goes to `thread`: now, or once the drafts sent
    /// before it are settled.
    pub(super) fn send_draft(
        &mut self,
        thread: ThreadKey,
        draft: SentDraft,
        text: String,
        cited: Vec<Reply>,
        cx: &mut Context<Self>,
    ) {
        self.send_in_turn(thread, text, Origin::Composer { draft, cited }, cx);
    }

    /// Send `prompt` to Pane `index`'s Thread again, queued behind a
    /// running turn. Nothing follows it: a prompt the Thread refuses is
    /// not said.
    pub(super) fn send_again(
        &mut self,
        index: usize,
        thread: ThreadKey,
        prompt: String,
        cx: &mut Context<Self>,
    ) {
        self.send_again_then(index, thread, prompt, |_, _, _| {}, cx);
    }

    /// `send_again`, and `then` hears where the prompt went, or why it
    /// did not, once the Server answers.
    pub(super) fn send_again_then(
        &mut self,
        index: usize,
        thread: ThreadKey,
        prompt: String,
        then: impl FnOnce(&mut Self, Result<Handoff, String>, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        self.focus_pane(index, cx);
        self.send_in_turn(thread, prompt, Origin::Replay(Box::new(then)), cx);
    }

    /// `text` goes to `thread`: now, or once the prompts sent before it
    /// are settled.
    fn send_in_turn(&mut self, thread: ThreadKey, text: String, from: Origin, cx: &mut Context<Self>) {
        let waiting = self.outbox.0.entry(thread).or_default();
        waiting.push_back(Outgoing { text, from });
        if waiting.len() == 1 {
            self.ask_next_send(thread, cx);
        }
    }

    /// Ask the Server to take `thread`'s oldest draft on its way: queued
    /// behind a running turn, else sent.
    fn ask_next_send(&mut self, thread: ThreadKey, cx: &mut Context<Self>) {
        let Some(text) = self
            .outbox
            .0
            .get(&thread)
            .and_then(|waiting| waiting.front())
            .map(|next| next.text.clone())
        else {
            self.outbox.0.remove(&thread);
            return;
        };
        if self
            .cockpit
            .read(cx)
            .thread(thread)
            .is_some_and(|open| open.needs_queue())
        {
            let queued = self.cockpit.update(cx, |cockpit, _| cockpit.queue(thread, text));
            self.when_answered(
                queued,
                move |view, queued, cx| {
                    view.sent(thread, Answered::Queued(queued.unwrap_or(false)), cx)
                },
                cx,
            );
        } else {
            let sent = self.cockpit.update(cx, |cockpit, _| cockpit.send(thread, text));
            self.when_answered(
                sent,
                move |view, sent, cx| view.sent(thread, Answered::Sent(admission(sent)), cx),
                cx,
            );
        }
    }

    /// The Server answered for `thread`'s oldest prompt on its way: settle
    /// it, and ask for the next.
    fn sent(&mut self, thread: ThreadKey, answered: Answered, cx: &mut Context<Self>) {
        let Some(Outgoing { from, .. }) = self
            .outbox
            .0
            .get_mut(&thread)
            .and_then(VecDeque::pop_front)
        else {
            return;
        };
        match from {
            Origin::Composer { draft, cited } => self.sent_draft(thread, draft, cited, answered, cx),
            Origin::Replay(then) => {
                let handoff = self.replayed(thread, answered, cx);
                then(self, handoff, cx);
            }
        }
        self.ask_next_send(thread, cx);
    }

    /// Settle the Composer's `draft` against its answer, and follow an
    /// admitted prompt.
    fn sent_draft(
        &mut self,
        thread: ThreadKey,
        draft: SentDraft,
        cited: Vec<Reply>,
        answered: Answered,
        cx: &mut Context<Self>,
    ) {
        let admitted = match answered {
            Answered::Queued(true) => {
                self.hold_queued(thread, draft);
                true
            }
            Answered::Queued(false) => {
                self.return_send(thread, draft, cx);
                false
            }
            Answered::Sent(admission) => {
                let admitted = self.settle_send(thread, draft, admission, cx);
                if let Some(index) = self.pane_sending(thread, cx).filter(|_| admitted) {
                    self.scroll_transcript_to_new_prompt(index, cx);
                }
                admitted
            }
        };
        if admitted {
            self.keep_cited_sources(thread, &cited, cx);
        }
        self.facts.acted(self.cockpit.read(cx), thread);
        // A first prompt names an untitled Thread.
        self.facts.renamed(self.cockpit.read(cx), thread);
        self.refresh_names(cx);
        cx.notify();
    }

    /// Where a prompt sent again went, by its answer, or why it did not.
    fn replayed(
        &mut self,
        thread: ThreadKey,
        answered: Answered,
        cx: &mut Context<Self>,
    ) -> Result<Handoff, String> {
        let handoff = match answered {
            Answered::Queued(true) | Answered::Sent(Admission::Held) => Ok(Handoff::Queued),
            Answered::Queued(false) => Err("The agent's queue did not take it.".into()),
            Answered::Sent(Admission::Delivered) => {
                if let Some(index) = self.pane_sending(thread, cx) {
                    self.scroll_transcript_to_new_prompt(index, cx);
                }
                Ok(Handoff::Sent)
            }
            Answered::Sent(Admission::Refused(why)) => Err(why),
            Answered::Sent(Admission::Unconfirmed(why)) => Err(format!(
                "It may or may not have been sent ({why}). Check the transcript before retrying."
            )),
        };
        self.facts.acted(self.cockpit.read(cx), thread);
        cx.notify();
        handoff
    }

    /// The Pane whose Composer sends to `thread`, while one shows it.
    fn pane_sending(&self, thread: ThreadKey, cx: &gpui::App) -> Option<usize> {
        self.panes
            .iter()
            .position(|pane| pane.read(cx).is_main() && pane.thread() == Some(thread))
    }

    /// A draft `thread` did not take goes back to its Composer, ahead of
    /// newer typing: now while a Pane shows the Thread, else when one does.
    fn return_send(&mut self, thread: ThreadKey, draft: SentDraft, cx: &mut Context<Self>) {
        match self.pane_sending(thread, cx) {
            Some(index) => self.put_back_send(index, draft, true, cx),
            None => self.held_sends.0.entry(thread).or_default().push(Held {
                draft,
                listed: false,
                lost: true,
            }),
        }
    }

    /// Settle `draft` against what became of its send. Whether the prompt
    /// is on its way (the caller then follows it: scroll, title).
    pub(super) fn settle_send(
        &mut self,
        thread: ThreadKey,
        draft: SentDraft,
        admission: Admission,
        cx: &mut Context<Self>,
    ) -> bool {
        match admission {
            Admission::Delivered => true,
            Admission::Held => {
                let listed = self.cockpit.read(cx).thread(thread).is_some_and(|view| {
                    view.queue()
                        .iter()
                        .any(|queued| queued.text.trim() == draft.prompt().trim())
                });
                self.held_sends.0.entry(thread).or_default().push(Held {
                    draft,
                    listed,
                    lost: false,
                });
                true
            }
            Admission::Refused(_) => {
                self.return_send(thread, draft, cx);
                false
            }
            Admission::Unconfirmed(reason) => {
                let named = named(draft.line());
                self.cockpit.update(cx, |cockpit, _| cockpit.say(
                    thread,
                    format!(
                        "\"{named}\" may or may not have been sent ({reason}). It is back in the composer: check the transcript before sending it again."
                    ),
                ));
                self.return_send(thread, draft, cx);
                false
            }
        }
    }

    /// `draft` back into Pane `index`'s Composer, ahead of newer typing when
    /// `prepend`.
    pub(super) fn put_back_send(
        &mut self,
        index: usize,
        draft: SentDraft,
        prepend: bool,
        cx: &mut Context<Self>,
    ) {
        self.panes[index]
            .composer(cx)
            .update(cx, |composer, cx| composer.restore(draft, prepend, cx));
        cx.notify();
    }

    /// Keep `draft` under `thread` while its prompt waits in the Thread's
    /// queue, so taking the prompt back to edit gives the draft back whole.
    pub(super) fn hold_queued(&mut self, thread: ThreadKey, draft: SentDraft) {
        self.held_sends.0.entry(thread).or_default().push(Held {
            draft,
            listed: true,
            lost: false,
        });
    }

    /// `prompt`, handed back to edit, as typed parts: the draft it was sent
    /// from while this window still holds it, else its text split into
    /// parts.
    pub(super) fn recovered_content(&mut self, thread: ThreadKey, prompt: String) -> DraftContent {
        match self.held_draft(thread, &prompt) {
            Some(draft) => draft.content().clone(),
            None => DraftContent::from_prompt(prompt),
        }
    }

    /// The draft `thread` held, when `prompt` is that prompt handed back by
    /// a Session that never started.
    fn held_draft(&mut self, thread: ThreadKey, prompt: &str) -> Option<SentDraft> {
        let held = self.held_sends.0.get_mut(&thread)?;
        let at = held
            .iter()
            .position(|held| held.draft.prompt().trim() == prompt.trim())?;
        let held = held.remove(at).draft;
        if self.held_sends.0.get(&thread).is_some_and(Vec::is_empty) {
            self.held_sends.0.remove(&thread);
        }
        Some(held)
    }

    /// Follow each held draft with its prompt: one the Session took is let
    /// go, one still waiting or handed back and not yet collected is kept,
    /// and one whose Thread closed under it is marked lost. A deleted
    /// Thread's go.
    pub(super) fn settle_held_sends(&mut self, cx: &gpui::App) {
        let cockpit = self.cockpit.read(cx);
        self.held_sends.0.retain(|&thread, held| {
            let Some(view) = cockpit.thread(thread) else {
                if cockpit.peek(thread).is_err() {
                    return false;
                }
                held.iter_mut().for_each(|held| held.lost = true);
                return true;
            };
            let waiting = view.queue();
            let handed_back = view.recovered().is_some();
            held.retain_mut(|held| {
                let here = waiting
                    .iter()
                    .any(|queued| queued.text.trim() == held.draft.prompt().trim());
                held.listed |= here;
                here || handed_back || !held.listed || held.lost
            });
            !held.is_empty()
        });
    }

    /// The drafts `thread` lost when it closed with a prompt held.
    pub(super) fn lost_sends(&mut self, thread: ThreadKey) -> Vec<SentDraft> {
        let Some(held) = self.held_sends.0.get_mut(&thread) else {
            return Vec::new();
        };
        let (lost, kept) = std::mem::take(held).into_iter().partition(|held| held.lost);
        *held = kept;
        if held.is_empty() {
            self.held_sends.0.remove(&thread);
        }
        lost.into_iter().map(|held: Held| held.draft).collect()
    }
}

fn named(prompt: &str) -> String {
    let prompt = prompt.trim();
    match prompt.char_indices().nth(NAMED_CHARS) {
        Some((end, _)) => format!("{}…", &prompt[..end]),
        None => prompt.to_string(),
    }
}
