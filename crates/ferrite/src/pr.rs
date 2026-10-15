//! A Thread's pull request, as its nav row labels it: `#123 ✓` — the PR of
//! the branch its checkout is on, read by `gh` where the checkout is
//! (`Cockpit::pull_request`). The behaviour matches herdr-plugin-gh-pr's
//! README; none of its code is used (it carries no license).
//!
//! A reading is asked for when the operator focuses a Thread and when a
//! Thread's worktree opens, at most once per `AUTO_EVERY`; the `refresh PR
//! status` action asks at once. While an answer is out the label's symbol
//! is `⟳`. With no `gh`, no login or no PR there is no label: a row never
//! shows an error.

use std::time::{Duration, Instant};

use ferrite_core::workspace::{CheckState, PrState, PullRequest};

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

    /// Adopt an answer. Whether the label moved.
    pub fn answered(&mut self, reading: Option<PullRequest>) -> bool {
        let moved = self.checking || self.reading != reading;
        self.checking = false;
        self.reading = reading;
        moved
    }

    /// The row's label (`#123 ✓`, `#123 ⟳`), or nothing without a PR.
    pub fn label(&self) -> Option<String> {
        let pr = self.reading.as_ref()?;
        let symbol = if self.checking {
            Some('\u{27f3}')
        } else {
            symbol(pr)
        };
        Some(match symbol {
            Some(symbol) => format!("#{} {symbol}", pr.number),
            None => format!("#{}", pr.number),
        })
    }
}

/// The PR's one symbol: merged `◆` and closed `⊘` first, else its CI —
/// passing `✓`, failing `✗`, pending `●` — and nothing for an open PR
/// with no checks.
fn symbol(pr: &PullRequest) -> Option<char> {
    match (pr.state, pr.checks) {
        (PrState::Merged, _) => Some('\u{25c6}'),
        (PrState::Closed, _) => Some('\u{2298}'),
        (PrState::Open, Some(CheckState::Passing)) => Some('\u{2713}'),
        (PrState::Open, Some(CheckState::Failing)) => Some('\u{2717}'),
        (PrState::Open, Some(CheckState::Pending | CheckState::Skipped)) => Some('\u{25cf}'),
        (PrState::Open, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn label(reading: PullRequest) -> Option<String> {
        PrWatch {
            reading: Some(reading),
            ..PrWatch::default()
        }
        .label()
    }

    /// The README's table: the number, then one symbol for where the PR
    /// stands.
    #[test]
    fn the_label_is_the_number_and_one_symbol() {
        let cases = [
            (PrState::Open, Some(CheckState::Passing), "#123 \u{2713}"),
            (PrState::Open, Some(CheckState::Failing), "#123 \u{2717}"),
            (PrState::Open, Some(CheckState::Pending), "#123 \u{25cf}"),
            (PrState::Merged, Some(CheckState::Failing), "#123 \u{25c6}"),
            (PrState::Closed, None, "#123 \u{2298}"),
            (PrState::Open, None, "#123"),
        ];
        for (state, checks, want) in cases {
            assert_eq!(label(pr(state, checks)).as_deref(), Some(want));
        }
        assert_eq!(PrWatch::default().label(), None, "no PR, no label");
    }

    /// While an answer is out the symbol turns; a Thread with no PR yet
    /// still says nothing.
    #[test]
    fn a_reading_on_its_way_turns_the_symbol() {
        let now = Instant::now();
        let mut watch = PrWatch::default();
        assert!(watch.ask(now, false));
        assert_eq!(watch.label(), None);
        assert!(watch.answered(Some(pr(PrState::Open, Some(CheckState::Passing)))));
        assert!(watch.ask(now + AUTO_EVERY, false));
        assert_eq!(watch.label().as_deref(), Some("#123 \u{27f3}"));
        assert!(watch.answered(None), "gh had nothing to say");
        assert_eq!(watch.label(), None);
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
