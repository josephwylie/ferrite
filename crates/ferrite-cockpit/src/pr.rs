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

use ferrite_core::pull_request::{PrState, PullRequest};

/// The least time between two automatic readings of one Thread's PR.
pub const AUTO_EVERY: Duration = Duration::from_secs(30);

/// Where a pull request stands, in one word: the Overview's PR row and
/// the details card's tooltip.
pub(crate) fn state_word(pr: &PullRequest) -> &'static str {
    match (pr.state, pr.draft) {
        (PrState::Merged, _) => "Merged",
        (PrState::Closed, _) => "Closed",
        (PrState::Open, true) => "Draft",
        (PrState::Open, false) => "Open",
    }
}

/// One Thread's PR: the last reading, and whether another is on its way.
#[derive(Debug, Default)]
pub struct PrReading {
    /// What `gh` last said; `None` when it said nothing (or was never asked).
    pub reading: Option<PullRequest>,
    checking: bool,
    asked: Option<Instant>,
}

impl PrReading {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_core::pull_request::{CheckState, PrState};

    fn pr(state: PrState, checks: Option<CheckState>) -> PullRequest {
        PullRequest {
            number: 123,
            state,
            draft: false,
            url: None,
            title: None,
            checks,
            runs: Vec::new(),
            conflicting: false,
        }
    }

    /// A reading lands and moves it; one on its way still counts
    /// as moved when `gh` then says nothing.
    #[test]
    fn an_answer_moves_the_reading() {
        let now = Instant::now();
        let mut paced = PrReading::default();
        assert!(paced.ask(now, false));
        assert!(paced.answered(Some(pr(PrState::Open, Some(CheckState::Passing)))));
        assert_eq!(paced.reading.as_ref().map(|pr| pr.number), Some(123));
        assert!(paced.ask(now + AUTO_EVERY, false));
        assert!(paced.answered(None), "gh had nothing to say");
        assert!(paced.reading.is_none());
    }

    /// Automatic readings wait out thirty seconds per Thread; a manual one
    /// never waits, and nothing asks twice at once.
    #[test]
    fn automatic_readings_are_paced_and_manual_ones_are_not() {
        let now = Instant::now();
        let mut paced = PrReading::default();
        assert!(paced.ask(now, false));
        assert!(!paced.ask(now, true), "one answer out at a time");
        paced.answered(None);
        assert!(!paced.ask(now + AUTO_EVERY / 2, false));
        assert!(paced.ask(now + AUTO_EVERY / 2, true));
        paced.answered(None);
        assert!(!paced.ask(now + AUTO_EVERY, false), "the manual ask counts");
        assert!(paced.ask(now + AUTO_EVERY / 2 + AUTO_EVERY, false));
    }
}
