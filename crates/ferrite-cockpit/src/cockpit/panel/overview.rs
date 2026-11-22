//! The Overview Surface in a view of its own (ADR 0041): `OverviewView`
//! owns what the Overview keeps for every Thread, the Changes section's
//! unfolded files with the hunks each read (`hunks`) and the sections'
//! scroll. It has no render of its own: the root draws it as the
//! Overview's body, handing it what it shows of the window with
//! `OverviewInputs`, and the Overview's Background section drawn by the
//! background work's own view. What it asks of the window it says as an
//! `OverviewEvent`.

use super::hunks::Hunks;
use super::*;
use ferrite_core::docview::FileChange;
use ferrite_core::workspace::PullRequest;
use std::collections::HashMap;

/// What the Overview keeps, by Thread.
#[derive(Default)]
pub(crate) struct OverviewView {
    /// The Changes section's unfolded files and their hunks.
    pub(super) hunks: Hunks,
    /// Each Thread's sections' scroll, so a Thread's Overview comes back
    /// where it was left. The Background Surface scrolls with it.
    scrolls: HashMap<ThreadKey, ScrollHandle>,
}

/// What the Overview asks of the window.
pub(in crate::cockpit) enum OverviewEvent {
    /// An unfolded file's hunks are unread, or were read at another
    /// diffstat: read the checkout's uncommitted diff for it.
    ReadHunks {
        thread: ThreadKey,
        path: String,
        stat: (usize, usize),
    },
    /// `Create PR`.
    CreatePr(ThreadKey),
    /// A changed file's name: it opens in the preview.
    Preview(std::path::PathBuf),
    /// `Review changes`.
    Review,
    /// The pull request's row.
    OpenPr(ThreadKey),
}

impl EventEmitter<OverviewEvent> for OverviewView {}

/// What the Overview draws of the window for one Thread.
pub(in crate::cockpit) struct OverviewInputs<'a> {
    pub(super) thread: ThreadKey,
    /// The machine the Thread runs on, as the Environment names it.
    pub(super) machine: String,
    /// The Thread's checkout.
    pub(super) folder: Option<std::path::PathBuf>,
    /// The branch the checkout is on (`thread_branch`).
    pub(super) branch: Option<SharedString>,
    pub(super) facts: Option<&'a crate::facts::ThreadFacts>,
    pub(super) files: &'a [FileChange],
    pub(super) pr: Option<&'a PullRequest>,
    pub(super) usage: Option<ferrite_core::transcript::Usage>,
    pub(super) context_details: Option<&'a ferrite_core::ContextDetails>,
    pub(super) usage_details: Option<&'a ferrite_core::UsageDetails>,
    /// Why the Session refused the last context refresh, while it is this
    /// Session's.
    pub(super) session_error: Option<String>,
    pub(super) servers: &'a [ferrite_core::McpServer],
    /// The Background tasks section, as the background work draws it.
    pub(super) background: Option<Div>,
}

impl OverviewView {
    /// `thread`'s sections' scroll, made the first time it is asked for.
    pub(in crate::cockpit) fn scroll(&mut self, thread: ThreadKey) -> ScrollHandle {
        self.scrolls.entry(thread).or_default().clone()
    }

    /// Every Thread `keep` refuses takes its folds, reads and scroll with it.
    pub(in crate::cockpit) fn retain_threads(&mut self, keep: impl Fn(ThreadKey) -> bool) {
        self.hunks.retain_threads(&keep);
        self.scrolls.retain(|thread, _| keep(*thread));
    }

    #[cfg(test)]
    pub(in crate::cockpit) fn held_for_test(&self, thread: ThreadKey) -> usize {
        self.hunks.held_for_test(thread)
    }
}

impl CockpitView {
    /// The Overview's sections for `thread`: none while the Thread is not
    /// open.
    pub(in crate::cockpit) fn overview_sections(
        &self,
        thread: ThreadKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.cockpit.read(cx).thread(thread.thread()).is_none() {
            return div().into_any_element();
        }
        let folder = self.thread_path(thread.thread(), cx);
        let background = self
            .background_inputs(thread.thread(), false, cx)
            .and_then(|inputs| self.background.update(cx, |background, cx| background.section(&inputs, cx)));
        // The model is lent, not written: the Overview draws what it
        // borrows of it under its own update.
        self.cockpit.update(cx, |model, cx| {
            match self.overview_inputs(model, thread, folder, background) {
                Some(inputs) => self
                    .overview
                    .update(cx, |overview, cx| overview.sections(inputs, window, cx)),
                None => div().into_any_element(),
            }
        })
    }

    /// What the Overview draws of the window for `thread`.
    fn overview_inputs<'a>(
        &'a self,
        model: &'a Cockpit,
        key: ThreadKey,
        folder: Option<std::path::PathBuf>,
        background: Option<Div>,
    ) -> Option<OverviewInputs<'a>> {
        let thread = key.thread();
        let open = model.thread(thread)?;
        let transcript = open.transcript();
        let facts = self.facts.get(model.thread_key(thread));
        Some(OverviewInputs {
            thread: key,
            machine: model.machine().map_or_else(
                || "This computer".to_string(),
                |bridge| bridge.name().to_string(),
            ),
            folder,
            branch: self.thread_branch(model.thread_key(thread)),
            facts,
            files: facts.map(|facts| facts.changed_files.as_slice()).unwrap_or_default(),
            pr: self.facts.pr(model.thread_key(thread)),
            usage: transcript.usage(),
            context_details: transcript.context_details(),
            usage_details: transcript.usage_details(),
            session_error: self
                .session_control_error
                .of(thread, open.generation())
                .map(str::to_string),
            servers: transcript.mcp_servers(),
            background,
        })
    }

    pub(in crate::cockpit) fn on_overview_event(
        &mut self,
        _: &Entity<OverviewView>,
        event: &OverviewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            OverviewEvent::ReadHunks { thread, path, stat } => {
                self.read_overview_hunks(*thread, path.clone(), *stat, cx)
            }
            OverviewEvent::CreatePr(thread) => self.create_pr(thread.thread(), cx),
            OverviewEvent::Preview(path) => self.open_preview(path.clone(), None, window, cx),
            OverviewEvent::Review => self.open_beside(
                beside::Beside::Review(ferrite_reviewr::model::Scope::Uncommitted),
                window,
                cx,
            ),
            OverviewEvent::OpenPr(thread) => self.open_pr_surface(thread.thread(), cx),
        }
    }

    /// Read the checkout's uncommitted diff for one file, where the
    /// checkout is (the Server's, on a machine), and hand the answer to the
    /// Overview.
    fn read_overview_hunks(
        &mut self,
        thread: ThreadKey,
        path: String,
        stat: (usize, usize),
        cx: &mut Context<Self>,
    ) {
        if !self.overview.read(cx).wants_read(thread, &path, stat) {
            return;
        }
        let Some(workspace) = self.thread_path(thread.thread(), cx) else {
            return;
        };
        let answer = self
            .cockpit
            .update(cx, |cockpit, _| cockpit.review_diff(&workspace, ferrite_reviewr::ferrite::Read::Uncommitted));
        self.overview.update(cx, |overview, cx| {
            overview.wait_hunks(thread, path, stat, &workspace, answer, cx)
        });
    }

    /// `Create PR`: ask the Thread's agent to commit what is left, push its
    /// branch and open the pull request (`create_pr_prompt`), queued behind
    /// a running turn, as Claude's button does. The PR's row takes the
    /// button's place once the forge reports it.
    pub(in crate::cockpit) fn create_pr(&mut self, thread: ThreadId, cx: &mut Context<Self>) {
        let Some(index) = self.pane_for(thread) else {
            return;
        };
        let branch = self.thread_branch(self.cockpit.read(cx).thread_key(thread));
        let base = self
            .facts
            .get(self.cockpit.read(cx).thread_key(thread))
            .and_then(|facts| facts.default_branch.clone());
        let prompt = super::changes::create_pr_prompt(branch.as_deref(), base.as_deref());
        self.send_again(index, thread, prompt, cx).ok();
    }

    /// The branch `thread`'s checkout is on: as git last said, else as the
    /// Thread was bound.
    pub(in crate::cockpit) fn thread_branch(&self, thread: ferrite_core::ThreadKey) -> Option<SharedString> {
        let facts = self.facts.get(thread)?;
        facts
            .status
            .as_ref()
            .and_then(|status| status.branch.clone())
            .map(SharedString::from)
            .or_else(|| facts.branch.clone())
    }

    /// What `path`'s block shows under its head, once read.
    #[cfg(test)]
    pub(crate) fn panel_hunks(
        &self,
        thread: ThreadId,
        path: &str,
        cx: &gpui::App,
    ) -> Option<super::hunks::Shown> {
        self.overview.read(cx).hunks_for_test(self.cockpit.read(cx).thread_key(thread), path).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Thread 3 here and Thread 3 on box keep a scroll each, and one
    /// going takes only its own.
    #[test]
    fn an_overview_keeps_two_machines_threads_apart() {
        let mut overview = OverviewView::default();
        let (mine, boxes) = (ThreadKey::fixture(0, 3), ThreadKey::fixture(1, 3));
        overview.scroll(mine);
        overview.scroll(boxes);
        assert_eq!(overview.scrolls.len(), 2);
        overview.retain_threads(|key| key != boxes);
        assert!(overview.scrolls.contains_key(&mine));
        assert!(!overview.scrolls.contains_key(&boxes));
    }
}
