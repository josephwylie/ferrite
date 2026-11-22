//! The operator's own hand on a Thread's PR watch (ADR 0046): T3 Code's
//! eye in the details card stops it, and its PR menu's `Watch for changes`
//! and `Stop watching` start and stop it, here also in the palette. The
//! watch itself is the Server's: the window asks, and the Thread's digest
//! says what became of it (`Facts::watched_pr`).

use super::*;

impl CockpitView {
    /// The URL a watch of the Thread's open PR names, and whether the
    /// Server watches it now. `None` for a PR that is not open, and for one
    /// not watched that cannot start a watch: it has no URL, or no Server
    /// runs a watcher on its machine.
    pub(super) fn pr_watch_target(&self, thread: ThreadKey, cx: &gpui::App) -> Option<(String, bool)> {
        let pr = self.facts.pr(thread)?;
        if pr.state != ferrite_core::workspace::PrState::Open {
            return None;
        }
        match self.facts.watched_pr(thread) {
            Some(watch) => Some((watch.url.clone(), true)),
            None if self.cockpit.read(cx).starts_pr_watches(thread.machine()) => Some((pr.url.clone()?, false)),
            None => None,
        }
    }

    /// Watch the PR at `url` for the Thread's agent, or stop watching it.
    /// The eye goes when the digest says so; a refusal (the PR merged
    /// meanwhile, no Server) is said in the nav's notice.
    pub(super) fn watch_pr(
        &mut self,
        thread: ThreadKey,
        url: String,
        watching: bool,
        cx: &mut Context<Self>,
    ) {
        let answer = self
            .cockpit
            .update(cx, |cockpit, _| cockpit.watch_pr(thread, &url, watching));
        // An Engine in this process has already answered: the card reads it
        // now, as it does after any act of the window's own.
        self.facts.acted(self.cockpit.read(cx), thread);
        cx.notify();
        cx.spawn(async move |this, cx| {
            if let Err(error) = beside::answered(answer, cx).await {
                this.update(cx, |view, cx| {
                    view.nav_notice.problem(error.to_string());
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}
