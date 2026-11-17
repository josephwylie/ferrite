//! A nav act on several Threads, and the way back. Each Thread's try is its
//! own `Outcome`, so a failure is named where it happened and never hides
//! the others. ⌘Z on a Park revives the Threads it took.

use super::*;

/// One Thread's try at a nav act: what the operator reads when it failed.
pub(super) struct Outcome {
    pub thread: ThreadId,
    pub result: Result<(), String>,
}

impl CockpitView {
    /// Park each of `threads` (reversible: the Parked shelf holds them,
    /// and ⌘Z takes it back while the foot's notice stands). A Thread that
    /// would not park is named and stays picked; Undo holds only the rest.
    pub(super) fn park_listed(&mut self, threads: &[ThreadId], cx: &mut Context<Self>) {
        let mut parked = Vec::new();
        let mut outcomes = Vec::new();
        for &thread in threads {
            let result = self.cockpit.park_thread(thread).map_err(|e| e.to_string());
            if result.is_ok() {
                parked.push(thread);
            }
            outcomes.push(Outcome { thread, result });
        }
        self.nav_picked
            .retain(|picked| !parked.contains(picked));
        self.report_failures("park", &outcomes);
        self.sync_panes(cx);
        if !parked.is_empty() {
            self.offer_undo(nav_chrome::UndoAct::Parked(parked), cx);
        }
        cx.notify();
    }

    /// The notice for every failed outcome, each by its Thread's name; none
    /// when all went through.
    pub(super) fn report_failures(&mut self, verb: &str, outcomes: &[Outcome]) {
        let failed: Vec<String> = outcomes
            .iter()
            .filter_map(|outcome| {
                let why = outcome.result.as_ref().err()?;
                Some(format!("{}: {why}", self.facts.name(outcome.thread)))
            })
            .collect();
        self.nav_notice = (!failed.is_empty())
            .then(|| format!("Couldn't {verb} {}", failed.join("; ")).into());
    }

    /// ⌘Z on a Park: the Threads revive, last parked first.
    pub(super) fn undo_parked(&mut self, parked: Vec<ThreadId>, cx: &mut Context<Self>) {
        let outcomes: Vec<Outcome> = parked
            .into_iter()
            .rev()
            .map(|thread| Outcome {
                thread,
                result: self.try_revive(thread, cx).map_err(|e| e.to_string()),
            })
            .collect();
        self.report_failures("undo", &outcomes);
        self.sync_panes(cx);
    }
}
