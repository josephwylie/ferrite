//! The window's half of a PR action: the copy, the composer and the
//! hand-off to the Thread's agent are the window's, and the reader
//! (`ferrite_prs::pull_request::action`) follows what each became.

use ferrite_core::pull_request::{PrSource, PrTarget};
use ferrite_prs::pull_request::action::Stage;
#[cfg(feature = "visual-reference")]
use ferrite_prs::pull_request::action::{ActionState, Retry};
use ferrite_prs::pull_request::PrAction;
use gpui::ClipboardItem;

use crate::cockpit::{CockpitView, Handoff};
use gpui::Context;

impl CockpitView {
    /// A row of the `⋯`, the primary action or a view's button, on
    /// `target`'s PR: copy, add it to the current composer, or ask the Thread's
    /// agent, queued behind a running turn, and follow what comes of it.
    pub(in crate::cockpit) fn run_pr_action(
        &mut self,
        target: PrTarget,
        action: PrAction,
        cx: &mut Context<Self>,
    ) {
        let PrSource::Thread(thread) = target.source else {
            return;
        };
        let Some(pr) = self.pull_request.read(cx).snapshot(target).cloned() else {
            return;
        };
        match action {
            PrAction::CopyLink => cx.write_to_clipboard(ClipboardItem::new_string(pr.url)),
            PrAction::CopyBranch => cx.write_to_clipboard(ClipboardItem::new_string(pr.head)),
            PrAction::Ask => {
                let Some(index) = self.pane_for(thread) else {
                    return;
                };
                self.panes[index].composer.update(cx, |composer, cx| {
                    let context = format!(
                        "About pull request #{}: {}\n{}\n\n",
                        pr.number, pr.title, pr.url
                    );
                    let draft = composer.text();
                    let text = if draft.is_empty() {
                        context
                    } else {
                        format!("{draft}\n\n{context}")
                    };
                    composer.set(text, cx);
                });
                cx.notify();
            }
            _ => {
                if !self
                    .pull_request
                    .update(cx, |reader, cx| reader.may_hand_off(target, action, &pr, cx))
                {
                    return;
                }
                let (Some(prompt), Some(index)) = (action.prompt(&pr), self.pane_for(thread)) else {
                    let closed = Err("Its Thread is not open.".into());
                    self.pull_request
                        .update(cx, |reader, cx| reader.handed(target, action, closed, cx));
                    return;
                };
                self.pull_request
                    .update(cx, |reader, cx| reader.handing(target, action, prompt.clone(), cx));
                self.send_again_then(
                    index,
                    thread,
                    prompt.clone(),
                    move |view, handoff, cx| {
                        let handed = handoff.map(|handoff| match handoff {
                            Handoff::Sent => Stage::Running,
                            Handoff::Queued => {
                                let open = view.cockpit.read(cx).thread(thread);
                                Stage::queued_in(open.as_ref(), &prompt)
                            }
                        });
                        view.pull_request
                            .update(cx, |reader, cx| reader.handed(target, action, handed, cx));
                    },
                    cx,
                );
            }
        }
        cx.notify();
    }

    /// `thread` streamed: its pending actions move on, and a turn that
    /// `settled` has its PR read back.
    pub(in crate::cockpit) fn advance_pr_actions(
        &mut self,
        thread: ferrite_core::ThreadKey,
        settled: bool,
        cx: &mut Context<Self>,
    ) {
        // The model is lent, not written: the reader reads the Thread under
        // its own update.
        let checking = self.cockpit.update(cx, |model, cx| {
            let open = model.thread(thread);
            self.pull_request
                .update(cx, |reader, cx| reader.advance(thread, open.as_ref(), settled, cx))
        });
        if checking.is_empty() {
            return;
        }
        for (target, _) in &checking {
            self.read_pr_snapshot(*target, true, cx);
        }
        self.pull_request
            .update(cx, |reader, cx| reader.unread_are_done(&checking, cx));
    }

    /// A fixture world's merge of `thread`'s PR #`number`, caught mid-flight.
    #[cfg(feature = "visual-reference")]
    pub(crate) fn fixture_pr_merge_running(
        &mut self,
        thread: ferrite_core::ThreadKey,
        number: u64,
        cx: &mut Context<Self>,
    ) {
        let target = PrTarget::thread(thread, number);
        let running = ActionState::Pending {
            action: PrAction::MergeNow,
            stage: Stage::Running,
            prompt: "merge".into(),
        };
        self.pull_request
            .update(cx, |reader, _| reader.hold_action(target, running));
    }

    /// As [`Self::fixture_pr_merge_running`], after the PR stayed open.
    #[cfg(feature = "visual-reference")]
    pub(crate) fn fixture_pr_merge_failed(
        &mut self,
        thread: ferrite_core::ThreadKey,
        number: u64,
        cx: &mut Context<Self>,
    ) {
        let target = PrTarget::thread(thread, number);
        let error = format!(
            "#{number} is not merged after the agent finished. Its reply in the Thread says why."
        );
        let failed = ActionState::Failed {
            action: PrAction::MergeNow,
            error,
            retry: Retry::Action,
        };
        self.pull_request
            .update(cx, |reader, _| reader.hold_action(target, failed));
    }

    /// As [`Self::fixture_pr_merge_running`], after the PR showed merged.
    #[cfg(feature = "visual-reference")]
    pub(crate) fn fixture_pr_merge_done(
        &mut self,
        thread: ferrite_core::ThreadKey,
        number: u64,
        cx: &mut Context<Self>,
    ) {
        let target = PrTarget::thread(thread, number);
        let done = ActionState::Done {
            action: PrAction::MergeNow,
        };
        self.pull_request
            .update(cx, |reader, _| reader.hold_action(target, done));
    }
}
