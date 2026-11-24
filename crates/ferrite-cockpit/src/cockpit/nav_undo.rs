//! A nav act on several Threads, and the way back. Each Thread's try is its
//! own `Outcome`, so a failure is named where it happened and never hides
//! the others. ⌘Z on a Park revives the Threads it took.

use super::*;

/// One Thread's try at a nav act: what the operator reads when it failed.
pub(super) struct Outcome {
    pub thread: ThreadKey,
    pub result: Result<(), String>,
}

impl CockpitView {
    /// Park each of `threads` (reversible: the Parked shelf holds them,
    /// and ⌘Z takes it back while the foot's notice stands). A Thread that
    /// would not park is named and stays picked; Undo holds only the rest.
    pub(super) fn park_listed(&mut self, keys: &[ThreadKey], cx: &mut Context<Self>) {
        let keys = keys.to_vec();
        let asked = self.cockpit.update(cx, |cockpit, _| {
            keys.iter().map(|key| cockpit.park_thread(*key)).collect()
        });
        self.when_all_answered(
            asked,
            move |view, answers, cx| {
                let outcomes: Vec<Outcome> = keys
                    .into_iter()
                    .zip(answers)
                    .map(|(thread, parked)| Outcome {
                        thread,
                        result: parked.map_err(|error| error.to_string()),
                    })
                    .collect();
                let parked: Vec<ThreadKey> = outcomes
                    .iter()
                    .filter(|outcome| outcome.result.is_ok())
                    .map(|outcome| outcome.thread)
                    .collect();
                view.report_failures("park", &outcomes);
                view.sync_panes(cx);
                view.nav_view.update(cx, |nav, cx| {
                    nav.picked.retain(|picked| !parked.contains(picked));
                    if !parked.is_empty() {
                        nav.offer_undo(nav_chrome::UndoAct::Parked(parked), cx);
                    }
                });
                cx.notify();
            },
            cx,
        );
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
        if failed.is_empty() {
            self.nav_notice.clear();
        } else {
            self.nav_notice
                .problem(format!("Couldn't {verb} {}", failed.join("; ")));
        }
    }

    /// ⌘Z on a Park: the Threads revive, last parked first.
    pub(super) fn undo_parked(&mut self, parked: Vec<ThreadKey>, cx: &mut Context<Self>) {
        self.reopen_threads(parked.into_iter().rev().collect(), "undo", cx);
    }
}
