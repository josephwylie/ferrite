//! A send keeps the whole draft until its admission is known. The Composer
//! hands over a `SentDraft`; the Engine says `Delivered`, `Held` for a
//! Session still starting, `Refused`, or (over a Server link that went
//! quiet) `Unconfirmed`. Only `Delivered` lets the draft go: a `Refused` or
//! `Unconfirmed` one goes back to the Composer at once, and a `Held` one
//! waits under its Thread for the prompt to come back if the Session never
//! starts. It belongs to the Thread, not to the Pane that sent it, so
//! switching or closing the Pane does not lose it.

use super::*;
use ferrite_core::cockpit::Admission;
use ferrite_pane::composer::SentDraft;
use std::collections::HashMap;

/// How much of a prompt names it in a notice.
const NAMED_CHARS: usize = 48;

/// The drafts each Thread's Engine is holding for its Session, oldest first.
#[derive(Default)]
pub(crate) struct HeldSends(HashMap<ThreadId, Vec<Held>>);

struct Held {
    draft: SentDraft,
    /// The Thread has been seen holding the prompt, so its absence from the
    /// queue means the Session took it.
    listed: bool,
    /// The Thread closed with the prompt still held: the Engine let it go
    /// with the Session, so the draft is all that is left of it.
    lost: bool,
}

impl HeldSends {
    #[cfg(test)]
    pub(crate) fn count(&self, thread: ThreadId) -> usize {
        self.0.get(&thread).map_or(0, Vec::len)
    }
}

impl CockpitView {
    /// Settle `draft` against what became of its send. Whether the prompt
    /// is on its way (the caller then follows it: scroll, title).
    pub(super) fn settle_send(
        &mut self,
        index: usize,
        thread: ThreadId,
        draft: SentDraft,
        admission: Admission,
        cx: &mut Context<Self>,
    ) -> bool {
        match admission {
            Admission::Delivered => true,
            Admission::Held => {
                let listed = self.cockpit.thread(thread).is_some_and(|view| {
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
                self.put_back_send(index, draft, true, cx);
                false
            }
            Admission::Unconfirmed(reason) => {
                let named = named(draft.line());
                self.cockpit.say(
                    thread,
                    format!(
                        "\"{named}\" may or may not have been sent ({reason}). It is back in the composer: check the transcript before sending it again."
                    ),
                );
                self.put_back_send(index, draft, true, cx);
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
            .composer
            .update(cx, |composer, cx| composer.restore(draft, prepend, cx));
        cx.notify();
    }

    /// The draft `thread` held, when `prompt` is that prompt handed back by
    /// a Session that never started.
    pub(super) fn held_draft(&mut self, thread: ThreadId, prompt: &str) -> Option<SentDraft> {
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
    pub(super) fn settle_held_sends(&mut self) {
        let cockpit = &self.cockpit;
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
    pub(super) fn lost_sends(&mut self, thread: ThreadId) -> Vec<SentDraft> {
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
