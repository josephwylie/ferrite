//! A Thread's pull request: the PR of the branch its checkout is on, read
//! by `gh` where the checkout is (`Cockpit::pull_request`), which the
//! palette's PR actions open. The behaviour matches herdr-plugin-gh-pr's
//! README; none of its code is used (it carries no license).
//!
//! A reading is asked for when the operator focuses a Thread and when a
//! Thread's worktree opens, at most once per `AUTO_EVERY`; the `refresh PR
//! status` action asks at once. With no `gh`, no login or no PR there is
//! no reading, and never an error.

use std::time::{Duration, Instant};

use ferrite_core::workspace::PullRequest;

/// The least time between two automatic readings of one Thread's PR.
pub const AUTO_EVERY: Duration = Duration::from_secs(30);

/// One Thread's PR: the last reading, and whether another is on its way.
#[derive(Debug, Default)]
pub struct PrWatch {
    /// What `gh` last said; `None` when it said nothing (or was never asked).
    pub reading: Option<PullRequest>,
    checking: bool,
    asked: Option<Instant>,
}

impl PrWatch {
    /// Whether to ask `gh` now, marking the ask when so. A manual refresh
    /// always asks; an automatic one waits out `AUTO_EVERY` since the last
    /// ask. Never while an answer is still on its way.
    pub fn ask(&mut self, now: Instant, manual: bool) -> bool {
        let recent = self
            .asked
            .is_some_and(|asked| now.saturating_duration_since(asked) < AUTO_EVERY);
        if self.checking || (recent && !manual) {
            return false;
        }
        self.checking = true;
        self.asked = Some(now);
        true
    }

    /// Adopt an answer. Whether the reading moved.
    pub fn answered(&mut self, reading: Option<PullRequest>) -> bool {
        let moved = self.checking || self.reading != reading;
        self.checking = false;
        self.reading = reading;
        moved
    }

    /// Adopt the panel's PR tab's fuller read of the same branch
    /// (`PullRequest::of_wire`), so its checks and state move the reading
    /// too. The snapshot names no check's page: a run keeps the one `gh`
    /// last gave it. An answer still on its way stays so. Whether the
    /// reading moved.
    pub fn adopt(&mut self, mut reading: PullRequest) -> bool {
        if let Some(old) = &self.reading {
            for run in &mut reading.runs {
                run.url = old
                    .runs
                    .iter()
                    .find(|known| known.name == run.name)
                    .and_then(|known| known.url.clone());
            }
            if reading.url.is_none() {
                reading.url = old.url.clone();
            }
        }
        let moved = self.reading.as_ref() != Some(&reading);
        self.reading = Some(reading);
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::workspace::{CheckState, PrState};

    fn pr(state: PrState, checks: Option<CheckState>) -> PullRequest {
        PullRequest {
            number: 123,
            state,
            draft: false,
            url: None,
            checks,
            runs: Vec::new(),
        }
    }

    /// A reading lands and moves the watch; one on its way still counts
    /// as moved when `gh` then says nothing.
    #[test]
    fn an_answer_moves_the_watch() {
        let now = Instant::now();
        let mut watch = PrWatch::default();
        assert!(watch.ask(now, false));
        assert!(watch.answered(Some(pr(PrState::Open, Some(CheckState::Passing)))));
        assert_eq!(watch.reading.as_ref().map(|pr| pr.number), Some(123));
        assert!(watch.ask(now + AUTO_EVERY, false));
        assert!(watch.answered(None), "gh had nothing to say");
        assert!(watch.reading.is_none());
    }

    /// Automatic readings wait out thirty seconds per Thread; a manual one
    /// never waits, and nothing asks twice at once.
    #[test]
    fn automatic_readings_are_paced_and_manual_ones_are_not() {
        let now = Instant::now();
        let mut watch = PrWatch::default();
        assert!(watch.ask(now, false));
        assert!(!watch.ask(now, true), "one answer out at a time");
        watch.answered(None);
        assert!(!watch.ask(now + AUTO_EVERY / 2, false));
        assert!(watch.ask(now + AUTO_EVERY / 2, true));
        watch.answered(None);
        assert!(!watch.ask(now + AUTO_EVERY, false), "the manual ask counts");
        assert!(watch.ask(now + AUTO_EVERY / 2 + AUTO_EVERY, false));
    }
}
