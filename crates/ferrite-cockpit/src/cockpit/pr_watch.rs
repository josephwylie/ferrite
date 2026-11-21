//! When the window asks `gh` for a Thread's PR on its own: as the Thread
//! is focused, and each time its work may have moved its git or forge
//! state (`Transcript::published`, HD27).
//!
//! Only the root's pump writes what it remembers, and only the root asks,
//! so it is a plain struct on the root (ADR 0041). The reading itself is
//! the Facts' (`ask_pr`, `pr_answered`).

use super::*;

#[derive(Default)]
pub(super) struct PrWatch {
    /// The Thread the pump last saw focused: focusing another asks for its
    /// PR.
    focus: Option<ThreadId>,
    /// Each Thread's `Transcript::published` as last seen: when it moves,
    /// its PR is read again at once.
    published: std::collections::HashMap<ThreadId, u64>,
    /// The PR reads `check_pr` would have sent `gh`: (Thread, manual).
    #[cfg(test)]
    pub(crate) asked: Vec<(ThreadId, bool)>,
}

impl PrWatch {
    /// `focused` is the Thread focused now: it, when it is not the one last
    /// seen.
    fn focus_moved(&mut self, focused: Option<ThreadId>) -> Option<ThreadId> {
        if focused == self.focus {
            return None;
        }
        self.focus = focused;
        focused
    }

    /// `thread`'s work has published `count` times: whether that moved
    /// since it was last seen. First sight only notes the count.
    fn published_moved(&mut self, thread: ThreadId, count: u64) -> bool {
        self.published
            .insert(thread, count)
            .is_some_and(|seen| seen != count)
    }

    /// Forget every Thread `keep` lets go.
    pub(super) fn retain_threads(&mut self, keep: impl Fn(ThreadId) -> bool) {
        self.published.retain(|thread, _| keep(*thread));
    }

    /// Whether it holds anything of `thread`'s.
    #[cfg(test)]
    pub(super) fn holds(&self, thread: ThreadId) -> bool {
        self.published.contains_key(&thread)
    }
}

impl CockpitView {
    /// The pump's watch: the Thread focused since the last frame, and each
    /// whose work published since, have their PR read.
    pub(super) fn watch_prs(&mut self, cx: &mut Context<Self>) {
        self.refresh_published(cx);
        if let Some(thread) = self.pr_watch.focus_moved(self.focused_thread()) {
            self.check_pr(thread, false, cx);
        }
    }

    /// Ask `gh` for the Thread's PR where its checkout is
    /// (`Cockpit::pull_request`, on a machine its Server): at once when
    /// `manual`, else at most once per `pr::AUTO_EVERY`. The row's symbol
    /// turns until the answer lands. A manual ask reads the open PR tab
    /// again too: its checks and conversation move without the reading.
    pub(crate) fn check_pr(&mut self, thread: ThreadId, manual: bool, cx: &mut Context<Self>) {
        if manual {
            self.refresh_pr_tab(thread, cx);
        }
        let Some(path) = self.thread_path(thread) else {
            return;
        };
        if !self
            .facts
            .ask_pr(thread, cx.background_executor().now(), manual)
        {
            return;
        }
        self.notify_part(parts::Part::Nav, cx);
        #[cfg(test)]
        self.pr_watch.asked.push((thread, manual));
        // Tests and captures never run `gh`: only a fixture world's forge
        // answers them (`Cockpit::fixture_forge`); a test reads what was
        // asked.
        if cfg!(any(test, feature = "visual-reference")) && !self.cockpit.has_forge_fixture(&path) {
            return;
        }
        let answer = self.cockpit.pull_request(&path);
        cx.spawn(async move |this, cx| {
            // A read that failed is `gh` with nothing to say: no label.
            let reading = beside::answered(answer, cx).await.ok().flatten();
            this.update(cx, |view, cx| view.pr_landed(thread, reading, cx))
                .ok();
        })
        .detach();
    }

    /// A Thread whose work may have moved its git or forge state since the
    /// last frame — a commit, a push, a PR, background work handed back
    /// (`Transcript::published`): its PR is read again at once, so Changes
    /// follows (HD27). First sight only notes the count: opening a Thread
    /// asks already.
    fn refresh_published(&mut self, cx: &mut Context<Self>) {
        let moved: Vec<ThreadId> = self
            .cockpit
            .threads()
            .into_iter()
            .filter(|thread| {
                let Some(count) = self
                    .cockpit
                    .thread(*thread)
                    .map(|open| open.transcript().published())
                else {
                    return false;
                };
                self.pr_watch.published_moved(*thread, count)
            })
            .collect();
        for thread in moved {
            self.check_pr(thread, true, cx);
        }
    }

    /// Adopt what `gh` said about the Thread's PR.
    pub(super) fn pr_landed(
        &mut self,
        thread: ThreadId,
        reading: Option<ferrite_core::workspace::PullRequest>,
        cx: &mut Context<Self>,
    ) {
        if self.facts.pr_answered(thread, reading) {
            self.notify_part(parts::Part::Nav, cx);
            self.pr_reading_changed(thread, cx);
        }
    }
}
