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

use crate::cockpit::surfaces::PrTab;
use ferrite_core::ThreadId;
use ferrite_core::protocol::{WirePr, WirePrComment, WirePrUnread, WireReviewState};
use ferrite_core::workspace::registry::ProjectId;

/// Measured identity chrome and the scroll state each tab left behind.
#[derive(Debug, Default)]
pub(crate) struct PrHeaderState {
    condensed: [bool; 4],
    pub expanded_height: f32,
    pub compact_height: f32,
    pub queued: bool,
}

impl PrHeaderState {
    pub fn condensed(&self, tab: PrTab) -> bool {
        tab != PrTab::Guide && self.condensed[tab as usize]
    }

    pub fn set_condensed(&mut self, tab: PrTab, condensed: bool) {
        self.condensed[tab as usize] = tab != PrTab::Guide && condensed;
    }

    /// T3's thresholds and absolute compensation, before viewport clamping.
    pub fn transition(&self, tab: PrTab, top: f32) -> Option<(bool, Option<f32>)> {
        if tab == PrTab::Guide || self.expanded_height <= 0. {
            return None;
        }
        if self.condensed(tab) {
            (top < 4.).then_some((false, None))
        } else {
            (top > self.expanded_height + 32.).then(|| {
                (
                    true,
                    Some((top - (self.expanded_height - self.compact_height)).max(0.)),
                )
            })
        }
    }
}

/// Whose repository a PR number is read in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PrSource {
    Thread(ThreadId),
    Project(ProjectId),
}

/// A pull request as a PR tab or the PRs destination names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PrTarget {
    pub source: PrSource,
    pub number: u64,
}

impl PrTarget {
    pub(crate) fn thread(thread: ThreadId, number: u64) -> Self {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SummarySection {
    Description,
    Checks,
    Comments,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PrSummary {
    open: [bool; 3],
    pub oldest_first: bool,
    pub shown: usize,
}

impl Default for PrSummary {
    fn default() -> Self {
        Self {
            open: [true, false, true],
            oldest_first: false,
            shown: COMMENT_PAGE,
        }
    }
}

impl PrSummary {
    pub(crate) fn is_open(self, section: SummarySection) -> bool {
        self.open[section as usize]
    }

    pub(crate) fn toggle(&mut self, section: SummarySection) {
        self.open[section as usize] = !self.is_open(section);
    }
}

pub(crate) const COMMENT_PAGE: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommentFilter {
    Bots,
    Resolved,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CommentFilters {
    pub bots: bool,
    pub resolved: bool,
}

impl CommentFilters {
    pub(crate) fn toggle(&mut self, filter: CommentFilter) {
        match filter {
            CommentFilter::Bots => self.bots = !self.bots,
            CommentFilter::Resolved => self.resolved = !self.resolved,
        }
    }

    fn includes(self, comment: &WirePrComment) -> bool {
        let finished = comment.resolved || comment.verdict == Some(WireReviewState::Dismissed);
        let verdict = matches!(
            comment.verdict,
            Some(WireReviewState::Approved | WireReviewState::ChangesRequested)
        );
        // A human reply keeps a bot finding in context; bot verdicts always count.
        (self.resolved || !finished)
            && (self.bots
                || !comment.author_is_bot
                || verdict
                || comment.replies.iter().any(|reply| !reply.author_is_bot))
    }
}

pub(crate) struct CommentPage<'a> {
    pub entries: Vec<(usize, &'a WirePrComment)>,
    pub total_comments: usize,
    pub hidden_comments: usize,
    pub older_conversations: usize,
}

pub(crate) fn comment_page(
    pr: &WirePr,
    filters: CommentFilters,
    summary: PrSummary,
) -> CommentPage<'_> {
    let total_comments = pr
        .comments
        .iter()
        .map(|comment| 1 + comment.replies.len())
        .sum();
    let mut entries: Vec<_> = pr
        .comments
        .iter()
        .enumerate()
        .filter(|(_, comment)| filters.includes(comment))
        .collect();
    let visible_comments: usize = entries
        .iter()
        .map(|(_, comment)| 1 + comment.replies.len())
        .sum();
    // Provider reads are newest first. Ordering reverses this recent page, not
    // which conversations belong to the page.
    entries.sort_by_key(|(_, comment)| {
        std::cmp::Reverse(ferrite_core::clock::parse_stamp(&comment.created_at))
    });
    let older_conversations = entries.len().saturating_sub(summary.shown);
    entries.truncate(summary.shown);
    if summary.oldest_first {
        entries.reverse();
    }
    CommentPage {
        entries,
        total_comments,
        hidden_comments: total_comments - visible_comments,
        older_conversations,
    }
}

#[derive(Debug)]
pub(crate) struct PrRead {
    /// The last good snapshot, kept through a refresh and a failed one.
    pub snapshot: Option<WirePr>,
    pub status: Status,
    pub summary: PrSummary,
    /// Summary keeps its position when another tab or PR is shown.
    pub summary_scroll: gpui::ScrollHandle,
    pub timeline_scroll: gpui::ScrollHandle,
    pub header: std::rc::Rc<std::cell::RefCell<PrHeaderState>>,
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
    pub comment_filters: CommentFilters,
}

impl PrReads {
    pub(crate) fn get(&self, target: PrTarget) -> Option<&PrRead> {
        self.reads.get(&target)
    }

    pub(crate) fn summary_mut(&mut self, target: PrTarget) -> Option<&mut PrSummary> {
        Some(&mut self.reads.get_mut(&target)?.summary)
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
                summary: PrSummary::default(),
                summary_scroll: gpui::ScrollHandle::new(),
                timeline_scroll: gpui::ScrollHandle::new(),
                header: Default::default(),
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
                summary: PrSummary::default(),
                summary_scroll: gpui::ScrollHandle::new(),
                timeline_scroll: gpui::ScrollHandle::new(),
                header: Default::default(),
                status: Status::Settled,
            },
        );
    }

    pub(crate) fn forget(&mut self, target: PrTarget) {
        self.reads.remove(&target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_thresholds_use_measured_chrome_and_keep_tabs_independent() {
        let mut state = PrHeaderState {
            expanded_height: 108.,
            compact_height: 28.,
            ..Default::default()
        };
        assert_eq!(state.transition(PrTab::Summary, 140.), None);
        assert_eq!(
            state.transition(PrTab::Summary, 141.),
            Some((true, Some(61.)))
        );
        state.set_condensed(PrTab::Summary, true);
        assert_eq!(state.transition(PrTab::Summary, 4.), None);
        assert_eq!(state.transition(PrTab::Summary, 3.), Some((false, None)));
        assert!(!state.condensed(PrTab::Timeline));
        state.set_condensed(PrTab::Guide, true);
        assert!(!state.condensed(PrTab::Guide));
        assert_eq!(state.transition(PrTab::Guide, 1_000.), None);
    }

    fn target(number: u64) -> PrTarget {
        PrTarget::thread(ThreadId::new(1), number)
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

    fn comment(body: &str) -> WirePrComment {
        serde_json::from_value(serde_json::json!({
            "kind":"comment", "author":"ada", "anchor":"comment", "body":body,
            "created_at":"2026-11-16T10:00:00Z"
        }))
        .unwrap()
    }

    #[test]
    fn summary_filters_hide_bot_chatter_and_finished_work_but_keep_human_replies_and_verdicts() {
        let filters = CommentFilters::default();
        let mut bot = comment("bot finding");
        bot.author_is_bot = true;
        assert!(!filters.includes(&bot));
        bot.verdict = Some(WireReviewState::Approved);
        assert!(filters.includes(&bot), "a bot's verdict is review evidence");
        bot.verdict = None;
        bot.replies.push(
            serde_json::from_value(
                serde_json::json!({"author":"ada", "body":"response", "created_at":""}),
            )
            .unwrap(),
        );
        assert!(
            filters.includes(&bot),
            "the root remains beside a human reply"
        );
        bot.resolved = true;
        assert!(!filters.includes(&bot));
        assert!(
            CommentFilters {
                resolved: true,
                ..filters
            }
            .includes(&bot)
        );
        let mut dismissed = comment("");
        dismissed.verdict = Some(WireReviewState::Dismissed);
        assert!(!filters.includes(&dismissed));
        assert!(
            CommentFilters {
                resolved: true,
                ..filters
            }
            .includes(&dismissed)
        );
    }

    #[test]
    fn summary_paging_reverses_the_recent_page_and_counts_replies_hidden_by_filters() {
        let mut pr: WirePr = serde_json::from_value(serde_json::json!({
            "number":7, "title":"PR", "url":"https://example.test/7", "state":"open",
            "head":"feature", "base":"main", "merge":"clean", "sync":{"kind":"unknown"}
        }))
        .unwrap();
        assert!(
            pr.reviewers.is_none() && pr.labels.is_none(),
            "older servers do not fabricate metadata"
        );
        pr.comments = (1..=12)
            .map(|day| {
                let mut comment = comment(&day.to_string());
                comment.created_at = format!("2026-11-{day:02}T10:00:00Z");
                comment
            })
            .collect();
        let newest = comment_page(&pr, CommentFilters::default(), PrSummary::default());
        assert_eq!(newest.entries.first().unwrap().1.body, "12");
        assert_eq!(newest.entries.last().unwrap().1.body, "3");
        assert_eq!(newest.older_conversations, 2);
        let oldest = comment_page(
            &pr,
            CommentFilters::default(),
            PrSummary {
                oldest_first: true,
                ..PrSummary::default()
            },
        );
        assert_eq!(oldest.entries.first().unwrap().1.body, "3");
        assert_eq!(oldest.entries.last().unwrap().1.body, "12");
        pr.comments[0].resolved = true;
        pr.comments[0].replies.push(
            serde_json::from_value(
                serde_json::json!({"author":"ada", "body":"reply", "created_at":""}),
            )
            .unwrap(),
        );
        let filtered = comment_page(&pr, CommentFilters::default(), PrSummary::default());
        assert_eq!(filtered.total_comments, 13);
        assert_eq!(filtered.hidden_comments, 2);
    }

    #[test]
    fn summary_section_state_survives_refresh_and_is_separate_for_each_requested_pr() {
        let mut reads = PrReads::default();
        reads.begin(target(1), false);
        reads.begin(target(2), false);
        assert!(
            !reads
                .get(target(1))
                .unwrap()
                .summary
                .is_open(SummarySection::Checks)
        );
        reads
            .summary_mut(target(1))
            .unwrap()
            .toggle(SummarySection::Checks);
        reads.begin(target(1), true);
        assert!(
            reads
                .get(target(1))
                .unwrap()
                .summary
                .is_open(SummarySection::Checks)
        );
        assert!(
            !reads
                .get(target(2))
                .unwrap()
                .summary
                .is_open(SummarySection::Checks)
        );
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
        let failure = |kind| PrFailure {
            kind,
            note: String::new(),
        };
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
        assert_eq!(
            stamp_age(&ferrite_core::clock::now_stamp()),
            "",
            "just now says nothing"
        );
        assert_eq!(stamp_age("2026-11-11"), "", "no age is invented");
    }
}
