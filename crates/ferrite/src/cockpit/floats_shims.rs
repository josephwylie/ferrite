//! Phase 0 stand-ins for the other packages' methods the floats call.
//!
//! The shared interfaces name these on `CockpitView` (the nav's
//! `show_parked` and `set_thread_order`, the transcript's prompt verbs, the
//! decisions package's `answer_approval`); the integrator's Phase 0 commit
//! had not landed when this package forked, so each is stood in here with
//! the agreed signature and the plainest behaviour the cockpit already has.
//! When the owner's real method lands, the duplicate definition names the
//! stand-in to delete — delete this whole file once all have.

use super::*;

impl CockpitView {
    /// ⌘⇧P, the palette's `show parked`: the parked rows unfolded at the
    /// nav's foot, the nav opened if it is folded.
    pub(crate) fn show_parked(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.close_palette(cx);
        if self.nav_railed() {
            self.toggle_nav_now(cx);
        }
        self.nav_parked_open = true;
        cx.notify();
    }

    /// The palette's `sort: created` / `sort: recent`: the nav's Thread
    /// order, saved.
    pub(crate) fn set_thread_order(&mut self, order: ThreadListOrder, cx: &mut Context<Self>) {
        self.change_settings(|settings| settings.thread_list_order = order, cx);
    }

    /// The focused Thread's latest prompt, as the operator wrote it.
    fn latest_prompt(&self) -> Option<String> {
        let thread = self.focused_thread()?;
        let open = self.cockpit.thread(thread)?;
        open.transcript()
            .blocks()
            .iter()
            .rev()
            .find_map(|block| match &block.body {
                ferrite_core::transcript::Body::Prompt(text) => Some(text.clone()),
                _ => None,
            })
    }

    /// The palette's `copy prompt`: the latest prompt to the clipboard.
    pub(crate) fn copy_reading_prompt(&mut self, cx: &mut Context<Self>) {
        if let Some(prompt) = self.latest_prompt() {
            cx.write_to_clipboard(ClipboardItem::new_string(prompt));
        }
    }

    /// The palette's `resend prompt`: the latest prompt sent again.
    pub(crate) fn resend_reading_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.latest_prompt() else {
            return;
        };
        let Some(composer) = self
            .panes
            .get(self.focused())
            .map(|pane| pane.composer.clone())
        else {
            return;
        };
        composer.update(cx, |composer, cx| composer.set(prompt, cx));
        self.submit(&Submit, window, cx);
    }

    /// A toast's (and the wall's) answer to `thread`'s waiting approval:
    /// allow it, allow it for the Thread (the provider's standing answer
    /// when it offers one), or deny it with the operator's words.
    pub(crate) fn answer_approval(
        &mut self,
        thread: ThreadId,
        choice: crate::decision::ApprovalChoice,
        note: Option<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::decision::ApprovalChoice;
        let Some((handle, decision)) = self.cockpit.thread(thread).and_then(|open| {
            open.activity()
                .pending_decisions()
                .iter()
                .find(|request| pane::question_of(&request.decision).is_none())
                .map(|request| (request.handle.clone(), request.decision.clone()))
        }) else {
            return;
        };
        let response = match choice {
            ApprovalChoice::Allow if decision.policy.allow => DecisionAnswer::Allow {
                input: decision.input.clone(),
            },
            ApprovalChoice::AllowForThread if decision.policy.allow => {
                match decision.standing_answer() {
                    Some(standing) => DecisionAnswer::AllowAlways {
                        input: decision.input.clone(),
                        suggestion: standing.clone(),
                    },
                    None => DecisionAnswer::Allow {
                        input: decision.input.clone(),
                    },
                }
            }
            ApprovalChoice::DenyAndSteer if decision.policy.deny => DecisionAnswer::Deny {
                message: note.unwrap_or_else(|| "The operator denied this tool.".into()),
            },
            _ => return,
        };
        self.respond_exact(thread, &handle, response, cx);
        self.facts.acted(&self.cockpit, thread);
        cx.notify();
    }
}
