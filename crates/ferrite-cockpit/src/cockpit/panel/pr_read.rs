//! What each PR tab knows of its pull request, and which forge answer may
//! change it.
//!
//! A read belongs to one target: a PR number in a repository, which a
//! Thread's checkout names (`LinkTarget::PullRequest`) or a Project does (the
//! PRs destination, whatever is checked out). Each target keeps its
//! last good snapshot beside the state of its newest read, so a refresh or a
//! failure never takes the snapshot away, and an answer lands only if it
//! carries the request that is still out for its target (the pattern of
//! Changes' `generation`, held per target, not app-wide).

use std::collections::HashMap;

use gpui::SharedString;

use ferrite_core::protocol::{WirePr, WirePrUnread};
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::ThreadKey;

/// Whose repository a PR number is read in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PrSource {
    Thread(ThreadKey),
    Project(ProjectId),
}

/// A pull request as a PR tab or the PRs destination names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PrTarget {
    pub source: PrSource,
    pub number: u64,
}

impl PrTarget {
    pub(crate) fn thread(thread: ThreadKey, number: u64) -> Self {
        Self {
            source: PrSource::Thread(thread),
            number,
        }
    }

    pub(crate) fn project(project: ProjectId, number: u64) -> Self {
        Self {
            source: PrSource::Project(project),
            number,
        }
    }
}

/// Why the newest read brought no snapshot, and how to say it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PrFailure {
    pub kind: WirePrUnread,
    pub note: String,
}

/// How long ago a forge's ISO stamp was, in the nav's shorthand (`2h`, `3d`).
/// Empty under a minute and for a stamp that does not parse: no age is invented.
pub(crate) fn stamp_age(stamp: &str) -> SharedString {
    ferrite_core::clock::parse_stamp(stamp).map_or_else(SharedString::default, |at| {
        crate::facts::since_label(at, ferrite_core::clock::system_time())
    })
}

impl PrFailure {
    /// The headline for a failed read of the PR list, not of one PR.
    pub(crate) fn list_headline(&self) -> &'static str {
        match self.kind {
            WirePrUnread::Failed => "Could not list pull requests",
            _ => self.headline(),
        }
    }

    pub(crate) fn headline(&self) -> &'static str {
        match self.kind {
            WirePrUnread::NoCli => "Needs the forge CLI",
            WirePrUnread::TimedOut => "The forge did not answer",
            WirePrUnread::RateLimited => "Rate limit reached",
            WirePrUnread::Offline => "Cannot reach the forge",
            WirePrUnread::Failed => "Could not read this PR",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    /// A read is out; `request` is the one answer that may land.
    Reading {
        request: u64,
    },
    Settled,
    Failed(PrFailure),
}

/// What a forge answer says of the target.
pub(crate) enum Answer {
    Read(Box<WirePr>),
    /// The read failed; whatever was known still stands.
    Failed(PrFailure),
    /// The forge says there is no such PR (not a failure to ask): nothing
    /// known of it stands.
    Gone(PrFailure),
}

#[derive(Debug)]
pub(crate) struct PrRead {
    /// The last good snapshot, kept through a refresh and a failed one.
    pub snapshot: Option<WirePr>,
    pub status: Status,
}

impl PrRead {
    pub(crate) fn reading(&self) -> bool {
        matches!(self.status, Status::Reading { .. })
    }

    pub(crate) fn failure(&self) -> Option<&PrFailure> {
        match &self.status {
            Status::Failed(failure) => Some(failure),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(crate) struct PrReads {
    reads: HashMap<PrTarget, PrRead>,
    issued: u64,
}

impl PrReads {
    pub(crate) fn get(&self, target: PrTarget) -> Option<&PrRead> {
        self.reads.get(&target)
    }

    /// The snapshot of `target`, once read.
    pub(crate) fn snapshot(&self, target: PrTarget) -> Option<&WirePr> {
        self.get(target)?.snapshot.as_ref()
    }

    /// The request to make for `target`, or `None` when (unless `again`) one
    /// is out already or it has been read before. `again` asks even with a
    /// read out, which that read's answer then no longer matches. A new
    /// request clears the failure the last one left: the retry shows at once.
    pub(crate) fn begin(&mut self, target: PrTarget, again: bool) -> Option<u64> {
        let read = self.reads.get(&target);
        if !again && read.is_some() {
            return None;
        }
        self.issued += 1;
        let request = self.issued;
        self.reads
            .entry(target)
            .and_modify(|read| read.status = Status::Reading { request })
            .or_insert(PrRead {
                snapshot: None,
                status: Status::Reading { request },
            });
        Some(request)
    }

    /// Adopt an answer, if `request` is still the one `target` waits on: a
    /// target forgotten meanwhile, or asked again, has none to adopt.
    pub(crate) fn finish(&mut self, target: PrTarget, request: u64, answer: Answer) -> bool {
        let Some(read) = self.reads.get_mut(&target) else {
            return false;
        };
        if read.status != (Status::Reading { request }) {
            return false;
        }
        match answer {
            Answer::Read(pr) => {
                read.snapshot = Some(*pr);
                read.status = Status::Settled;
            }
            Answer::Failed(failure) => read.status = Status::Failed(failure),
            Answer::Gone(failure) => {
                read.snapshot = None;
                read.status = Status::Failed(failure);
            }
        }
        true
    }

    /// `target` read as `pr`, as a fixture world has it.
    #[cfg(any(test, feature = "visual-reference"))]
    pub(crate) fn install(&mut self, target: PrTarget, pr: WirePr) {
        self.reads.insert(
            target,
            PrRead {
                snapshot: Some(pr),
                status: Status::Settled,
            },
        );
    }

    pub(crate) fn forget(&mut self, target: PrTarget) {
        self.reads.remove(&target);
    }

    /// Keep only the targets `keep` holds to.
    pub(crate) fn retain(&mut self, keep: impl Fn(&PrTarget) -> bool) {
        self.reads.retain(|target, _| keep(target));
    }

    #[cfg(test)]
    pub(crate) fn held_for_test(&self, thread: ThreadKey) -> usize {
        self.reads.keys().filter(|target| target.source == PrSource::Thread(thread)).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(number: u64) -> PrTarget {
        PrTarget::thread(ThreadKey::fixture(0, 1), number)
    }

    fn failure(kind: WirePrUnread) -> PrFailure {
        PrFailure {
            kind,
            note: "why".into(),
        }
    }

    #[test]
    fn a_passive_read_asks_once_and_an_explicit_one_supersedes() {
        let mut reads = PrReads::default();
        let first = reads.begin(target(1), false).unwrap();
        assert_eq!(reads.begin(target(1), false), None);
        let second = reads.begin(target(1), true).unwrap();
        assert!(!reads.finish(
            target(1),
            first,
            Answer::Failed(failure(WirePrUnread::Failed))
        ));
        assert!(reads.finish(
            target(1),
            second,
            Answer::Failed(failure(WirePrUnread::Failed))
        ));
    }

    #[test]
    fn a_forgotten_target_takes_no_answer() {
        let mut reads = PrReads::default();
        let request = reads.begin(target(1), false).unwrap();
        reads.forget(target(1));
        assert!(!reads.finish(
            target(1),
            request,
            Answer::Failed(failure(WirePrUnread::Failed))
        ));
        assert!(reads.get(target(1)).is_none());
    }

    #[test]
    fn every_failure_kind_has_its_own_headline() {
        use WirePrUnread::*;
        let kinds = [NoCli, TimedOut, RateLimited, Offline, Failed];
        let heads: std::collections::HashSet<_> = kinds
            .map(|kind| failure(kind).headline())
            .into_iter()
            .collect();
        assert_eq!(heads.len(), kinds.len());
    }

    #[test]
    fn a_failed_list_does_not_say_it_could_not_read_a_pr() {
        let failure = |kind| PrFailure { kind, note: String::new() };
        assert_eq!(
            failure(WirePrUnread::Failed).list_headline(),
            "Could not list pull requests"
        );
        assert_eq!(
            failure(WirePrUnread::Offline).list_headline(),
            failure(WirePrUnread::Offline).headline()
        );
    }

    #[test]
    fn a_forge_stamp_reads_as_the_navs_age() {
        assert!(stamp_age("2020-01-01T00:00:00Z").ends_with('y'));
        assert_eq!(stamp_age(&ferrite_core::clock::now_stamp()), "", "just now says nothing");
        assert_eq!(stamp_age("2026-11-11"), "", "no age is invented");
    }
}
