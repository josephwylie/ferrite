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
    /// runs a watcher for this window.
    pub(super) fn pr_watch_target(&self, thread: ThreadId, cx: &gpui::App) -> Option<(String, bool)> {
        let key = self.cockpit.read(cx).thread_key(thread);
        let pr = self.facts.pr(key)?;
        if pr.state != ferrite_core::workspace::PrState::Open {
            return None;
        }
        match self.facts.watched_pr(key) {
            Some(watch) => Some((watch.url.clone(), true)),
            None if self.cockpit.read(cx).starts_pr_watches() => Some((pr.url.clone()?, false)),
            None => None,
        }
    }

    /// Watch the PR at `url` for the Thread's agent, or stop watching it.
    /// The eye goes when the digest says so; a refusal (the PR merged
    /// meanwhile, no Server) is said in the nav's notice.
    pub(super) fn watch_pr(
        &mut self,
        thread: ThreadId,
        url: String,
        watching: bool,
        cx: &mut Context<Self>,
    ) {
        let answer = self
            .cockpit
            .update(cx, |cockpit, _| cockpit.watch_pr(thread, &url, watching));
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
