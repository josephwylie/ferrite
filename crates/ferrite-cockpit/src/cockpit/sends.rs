//! A send keeps the whole draft until its admission is known. The Composer
//! hands over a `SentDraft`; the Engine says `Delivered`, `Held` for a
//! Session still starting, `Refused`, or (over a Server link that went
//! quiet) `Unconfirmed`. Only `Delivered` lets the draft go: a `Refused` or
//! `Unconfirmed` one goes back to the Composer at once, and a `Held` one
//! waits in its Pane for the prompt to come back if the Session never starts.

use super::*;
use ferrite_core::cockpit::Admission;
use ferrite_pane::composer::SentDraft;

/// How much of a prompt names it in a notice.
const NAMED_CHARS: usize = 48;

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
            Admission::Delivered => {
                // A Session that takes a prompt now has taken every earlier one.
                self.panes[index].held_sends.clear();
                true
            }
            Admission::Held => {
                self.panes[index].held_sends.push(draft);
                true
            }
            Admission::Refused(_) => {
                self.put_back_send(index, draft, cx);
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
                self.put_back_send(index, draft, cx);
                false
            }
        }
    }

    pub(super) fn put_back_send(&mut self, index: usize, draft: SentDraft, cx: &mut Context<Self>) {
        self.panes[index]
            .composer
            .update(cx, |composer, cx| composer.restore(draft, cx));
        cx.notify();
    }

    /// The draft a held prompt came from, when `prompt` is that prompt
    /// handed back by a Session that never started.
    pub(super) fn held_draft(&mut self, index: usize, prompt: &str) -> Option<SentDraft> {
        let held = &mut self.panes[index].held_sends;
        let at = held
            .iter()
            .position(|draft| draft.prompt().trim() == prompt.trim())?;
        Some(held.remove(at))
    }
}

fn named(prompt: &str) -> String {
    let prompt = prompt.trim();
    match prompt.char_indices().nth(NAMED_CHARS) {
        Some((end, _)) => format!("{}…", &prompt[..end]),
        None => prompt.to_string(),
    }
}
