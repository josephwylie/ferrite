//! A Thread's pull request, as its nav row labels it: `#123` and a mark
//! (`PrLabel`, drawn as an icon by the nav) — the PR of
//! the branch its checkout is on, read by `gh` where the checkout is
//! (`Cockpit::pull_request`). The behaviour matches herdr-plugin-gh-pr's
//! README; none of its code is used (it carries no license).
//!
//! A reading is asked for when the operator focuses a Thread and when a
//! Thread's worktree opens, at most once per `AUTO_EVERY`; the `refresh PR
//! status` action asks at once. While an answer is out the label's mark
//! is `Checking`. With no `gh`, no login or no PR there is no label: a row never
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

    /// Adopt the panel's PR tab's fuller read of the same branch
    /// (`PullRequest::of_wire`), so its checks and state move the label
    /// too. The snapshot names no check's page: a run keeps the one `gh`
    /// last gave it. An answer still on its way stays so. Whether the label
    /// moved.
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

    /// The row's label (`#123` and its mark), or nothing without a PR.
    pub fn label(&self) -> Option<PrLabel> {
        let pr = self.reading.as_ref()?;
        Some(PrLabel {
            number: pr.number,
            mark: if self.checking {
                Some(PrMark::Checking)
            } else {
                PrMark::of(pr)
            },
        })
    }
}

/// A row's PR label: its number, and one mark for where it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrLabel {
    pub number: u64,
    pub mark: Option<PrMark>,
}

impl PrLabel {
    /// The number as the row types it: `#123`.
    pub fn number_text(&self) -> String {
        format!("#{}", self.number)
    }
}

/// Where a PR stands, one mark: the nav draws each as an icon of one size
/// (`nav::pr_mark`), never a font's glyph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrMark {
    Passing,
    Failing,
    Pending,
    /// A reading is on its way.
    Checking,
    Merged,
    Closed,
}

impl PrMark {
    /// Merged and closed first, else its CI — skipped checks pass — and
    /// nothing for an open PR with no checks.
    fn of(pr: &PullRequest) -> Option<Self> {
        match (pr.state, pr.checks) {
            (PrState::Merged, _) => Some(PrMark::Merged),
            (PrState::Closed, _) => Some(PrMark::Closed),
            (PrState::Open, Some(CheckState::Passing | CheckState::Skipped)) => {
                Some(PrMark::Passing)
            }
            (PrState::Open, Some(CheckState::Failing)) => Some(PrMark::Failing),
            (PrState::Open, Some(CheckState::Pending)) => Some(PrMark::Pending),
            (PrState::Open, None) => None,
        }
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

    fn label(reading: PullRequest) -> Option<PrLabel> {
        PrWatch {
            reading: Some(reading),
            ..PrWatch::default()
        }
        .label()
    }

    /// The README's table: the number, then one mark for where the PR
    /// stands.
    #[test]
    fn the_label_is_the_number_and_one_mark() {
        let cases = [
            (PrState::Open, Some(CheckState::Passing), Some(PrMark::Passing)),
            (PrState::Open, Some(CheckState::Skipped), Some(PrMark::Passing)),
            (PrState::Open, Some(CheckState::Failing), Some(PrMark::Failing)),
            (PrState::Open, Some(CheckState::Pending), Some(PrMark::Pending)),
            (PrState::Merged, Some(CheckState::Failing), Some(PrMark::Merged)),
            (PrState::Closed, None, Some(PrMark::Closed)),
            (PrState::Open, None, None),
        ];
        for (state, checks, mark) in cases {
            assert_eq!(
                label(pr(state, checks)),
                Some(PrLabel { number: 123, mark })
            );
        }
        assert_eq!(PrWatch::default().label(), None, "no PR, no label");
    }

    /// While an answer is out the mark says so; a Thread with no PR yet
    /// still says nothing.
    #[test]
    fn a_reading_on_its_way_marks_the_label() {
        let now = Instant::now();
        let mut watch = PrWatch::default();
        assert!(watch.ask(now, false));
        assert_eq!(watch.label(), None);
        assert!(watch.answered(Some(pr(PrState::Open, Some(CheckState::Passing)))));
        assert!(watch.ask(now + AUTO_EVERY, false));
        assert_eq!(
            watch.label(),
            Some(PrLabel {
                number: 123,
                mark: Some(PrMark::Checking)
            })
        );
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
