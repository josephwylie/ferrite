//! A pull request as a window shows it: the branch's PR as the forge
//! reports it, read from `gh pr view`'s JSON, and the PRs a Thread's
//! agent watches. The `gh` that reads a PR and the watch that follows
//! one are the Server's (`workspace`, `pr_watch`).

use serde::{Deserialize, Serialize};

use crate::{ProjectKey, ThreadKey};

/// The branch's pull request, as the forge reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub state: PrState,
    pub draft: bool,
    /// Where the PR lives on the forge; `None` when `gh` did not say.
    pub url: Option<String>,
    /// Its title, which the nav card's PR popover reads; `None` when the
    /// forge did not say.
    pub title: Option<String>,
    /// The rollup of the PR's checks. `None` when the PR has no checks at
    /// all — an honest silence, not a pass.
    pub checks: Option<CheckState>,
    /// Every check behind that rollup, in the order the forge lists them:
    /// what the checks card lists. Empty exactly when `checks`
    /// is `None` — the rollup *is* this list, folded.
    pub runs: Vec<Check>,
    /// Whether it conflicts with its base branch; `false` while the
    /// forge has not worked that out.
    pub conflicting: bool,
}

impl PullRequest {
    /// How the runs divide: the checks card's counts.
    pub fn tally(&self) -> Tally {
        let mut tally = Tally::default();
        for run in &self.runs {
            match run.state {
                CheckState::Failing => tally.failing += 1,
                CheckState::Pending => tally.pending += 1,
                CheckState::Passing => tally.passing += 1,
                CheckState::Skipped => tally.skipped += 1,
            }
        }
        tally
    }

    /// The label's reading of a forge read's snapshot: what `gh pr view`
    /// would have said of the same pull request. `None` without one.
    pub fn of_view(view: &ferrite_reviewr::forge::PrView) -> Option<Self> {
        let ferrite_reviewr::forge::PrView::Pr(pr) = view else {
            return None;
        };
        Some(Self::of_wire(&(**pr).clone().into()))
    }

    /// The label's reading of the Review's whole snapshot: its number,
    /// state and checks. The snapshot names no check's page, so no run has
    /// a url.
    fn of_wire(pr: &crate::protocol::WirePr) -> Self {
        use crate::protocol::{WireCheckStatus as Status, WirePrState as Wire};
        let runs: Vec<Check> = pr
            .checks
            .iter()
            .map(|check| {
                let (state, detail) = match check.status {
                    Status::Success => (CheckState::Passing, "success"),
                    Status::Failure => (CheckState::Failing, "failure"),
                    Status::Running => (CheckState::Pending, "in_progress"),
                    Status::Pending | Status::Unknown => (CheckState::Pending, "queued"),
                    Status::Skipped => (CheckState::Skipped, "skipped"),
                };
                Check {
                    name: check.name.clone(),
                    workflow: None,
                    state,
                    detail: detail.into(),
                    url: check.url.clone(),
                }
            })
            .collect();
        Self {
            number: pr.number,
            state: match pr.state {
                Wire::Merged => PrState::Merged,
                Wire::Closed => PrState::Closed,
                Wire::Open | Wire::Unknown => PrState::Open,
            },
            draft: pr.draft,
            url: (!pr.url.is_empty()).then(|| pr.url.clone()),
            title: (!pr.title.is_empty()).then(|| pr.title.clone()),
            checks: (!runs.is_empty()).then(|| rollup_state(&runs)),
            runs,
            conflicting: matches!(pr.merge, crate::protocol::WireMerge::Conflicting),
        }
    }
}

/// The run counts by state. A skipped run is neither a pass nor a failure
/// and is counted only as itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub passing: u32,
    pub failing: u32,
    pub pending: u32,
    pub skipped: u32,
}

impl Tally {
    pub fn total(&self) -> u32 {
        self.passing + self.failing + self.pending + self.skipped
    }

    /// The runs that have finished, however they finished.
    pub fn settled(&self) -> u32 {
        self.total() - self.pending
    }
}

/// One check on the PR: an Actions job, or a commit status some other
/// service posted. Both arrive in the same rollup and both are shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// The job or context name — `test (windows-latest)`,
    /// `codecov/patch`.
    pub name: String,
    /// The Actions workflow the job belongs to. `None` for a plain commit
    /// status, which has no workflow above it.
    pub workflow: Option<String>,
    pub state: CheckState,
    /// GitHub's own word for where the run stands, lowercased —
    /// `success`, `in_progress`, `timed_out`, `skipped`. The state above
    /// is this folded to four; this is what the forge actually said.
    pub detail: String,
    /// Where the run's log lives, for the row that opens it.
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckState {
    Passing,
    Failing,
    Pending,
    /// Ran nothing and claims nothing — a skipped job or a neutral
    /// status. Never the rollup's own reading: a PR whose every check
    /// skipped is not a passing PR.
    Skipped,
}

/// Read `gh pr view`'s JSON. The rollup is a heterogeneous array —
/// Actions jobs (`CheckRun`) and posted commit statuses (`StatusContext`)
/// side by side, with different keys for the same three facts — so
/// each element is read for whichever pair it carries. An element with no
/// name at all is dropped rather than listed as a blank row.
pub fn parse_pr_json(body: &str) -> Option<PullRequest> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let number = value.get("number")?.as_u64()?;
    let state = match value.get("state").and_then(serde_json::Value::as_str)? {
        "MERGED" => PrState::Merged,
        "CLOSED" => PrState::Closed,
        _ => PrState::Open,
    };
    let draft = value
        .get("isDraft")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let url = value
        .get("url")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let title = value
        .get("title")
        .and_then(serde_json::Value::as_str)
        .filter(|title| !title.is_empty())
        .map(str::to_string);
    let runs: Vec<Check> = value
        .get("statusCheckRollup")
        .and_then(serde_json::Value::as_array)
        .map(|nodes| nodes.iter().filter_map(parse_check).collect())
        .unwrap_or_default();
    // No runs is no rollup: the silence stays a silence rather than
    // becoming a pass.
    let checks = (!runs.is_empty()).then(|| rollup_state(&runs));
    Some(PullRequest {
        number,
        state,
        draft,
        url,
        title,
        checks,
        runs,
        conflicting: value.get("mergeable").and_then(serde_json::Value::as_str)
            == Some("CONFLICTING"),
    })
}

/// One rollup element, whichever shape it came in.
fn parse_check(node: &serde_json::Value) -> Option<Check> {
    let string = |key: &str| {
        node.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.is_empty())
    };
    // A job answers to `name`, a posted status to `context`.
    let name = string("name").or_else(|| string("context"))?;
    // A finished job reports its `conclusion` and an unfinished one only
    // its `status`; a commit status puts both in one `state`.
    let word = string("conclusion")
        .or_else(|| string("status"))
        .or_else(|| string("state"))?
        .to_ascii_lowercase();
    Some(Check {
        state: check_state(&word),
        detail: word,
        workflow: string("workflowName"),
        url: string("detailsUrl").or_else(|| string("targetUrl")),
        name,
    })
}

/// GitHub's vocabulary folded to the four states a dot can show. Anything
/// unrecognised is pending, not passing: a word we do not know is not a
/// word that clears CI.
fn check_state(word: &str) -> CheckState {
    match word {
        "success" => CheckState::Passing,
        // A cancelled run is a run that did not pass, and reads as one.
        "failure" | "timed_out" | "startup_failure" | "action_required" | "error" | "cancelled" => {
            CheckState::Failing
        }
        "skipped" | "neutral" | "stale" => CheckState::Skipped,
        _ => CheckState::Pending,
    }
}

/// A check rollup reads worst-first: any failure fails the PR, any
/// unfinished run leaves it pending, and only runs that actually passed
/// pass it. An all-skipped rollup is pending — nothing has vouched
/// for it.
fn rollup_state(runs: &[Check]) -> CheckState {
    if runs.iter().any(|run| run.state == CheckState::Failing) {
        return CheckState::Failing;
    }
    if runs.iter().any(|run| run.state == CheckState::Pending) {
        return CheckState::Pending;
    }
    if runs.iter().any(|run| run.state == CheckState::Passing) {
        return CheckState::Passing;
    }
    CheckState::Pending
}

/// Whose repository a PR number is read in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrSource {
    Thread(ThreadKey),
    Project(ProjectKey),
}

impl PrSource {
    /// The machine the repository is on, whose forge CLI reads it.
    pub fn machine(self) -> crate::MachineId {
        match self {
            Self::Thread(thread) => thread.machine(),
            Self::Project(project) => project.machine(),
        }
    }
}

/// A pull request as a PR tab or the PRs destination names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PrTarget {
    pub source: PrSource,
    pub number: u64,
}

impl PrTarget {
    pub fn thread(thread: ThreadKey, number: u64) -> Self {
        Self {
            source: PrSource::Thread(thread),
            number,
        }
    }

    pub fn project(project: ProjectKey, number: u64) -> Self {
        Self {
            source: PrSource::Project(project),
            number,
        }
    }
}

/// What the Server's watching holds of a Thread, as every window reads it
/// (the details card's eye, the Working shelf, the Notice): each PR its
/// agent watches, and whether a server wake waits to resume it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watching {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prs: Vec<WatchedPr>,
    /// A server wake (`crate::wake`) waits for the Thread: Main will be
    /// resumed by it, as by a queued turn start in T3.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub wake_waiting: bool,
}

/// One watched PR, as a window draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchedPr {
    pub url: String,
    pub number: u64,
    /// The watch holds its Thread working (`Watch::holds`).
    pub holds: bool,
}

impl Watching {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether it holds the Thread working: a watch holds, or a wake waits
    /// to resume Main.
    pub fn holds(&self) -> bool {
        self.wake_waiting || self.prs.iter().any(|pr| pr.holds)
    }

    /// The watch on PR `number`, at `url` when the reading named one.
    /// GitHub's URLs ignore case.
    pub fn of_pr(&self, number: u64, url: Option<&str>) -> Option<&WatchedPr> {
        self.prs.iter().find(|pr| {
            pr.number == number && url.is_none_or(|url| pr.url.eq_ignore_ascii_case(url.trim()))
        })
    }
}

/// A Thread's PR watches: the ones on, and the ones that ended, kept so
/// watching a PR again does not repeat what its last watch told.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchFact {
    pub on: Vec<Watch>,
    pub ended: Vec<Watch>,
}

impl WatchFact {
    /// No watch on, and none ended: what a Thread that never watched has.
    pub fn is_empty(&self) -> bool {
        self.on.is_empty() && self.ended.is_empty()
    }

    /// What a window reads of these watches (`Watching`).
    pub fn watching(&self, wake_waiting: bool) -> Watching {
        Watching {
            prs: self
                .on
                .iter()
                .map(|watch| WatchedPr {
                    url: watch.url.clone(),
                    number: watch.number,
                    holds: watch.holds(),
                })
                .collect(),
            wake_waiting,
        }
    }

    /// The watch on `repository#number`, while it is on.
    pub fn on(&self, repository: &str, number: u64) -> Option<&Watch> {
        self.on.iter().find(|watch| watch.is(repository, number))
    }

    /// Watch `target`, as T3's `thread.pull-request.watch` does. A watch
    /// already on keeps its cursor and only takes `until`; a new one
    /// starts at `Checks` unless told, from what the PR's ended watch had
    /// told, so watching again repeats no check or conflict news.
    /// `checks_pending` is a fresh read's word on whether checks were
    /// still running, which lets the first read tell a failure CI made
    /// after the call. Answers whether it was already watched.
    pub fn watch(
        &mut self,
        target: Target,
        until: Option<Until>,
        checks_pending: Option<bool>,
        now_ms: i64,
    ) -> bool {
        let (repository, number) = (target.repository.as_str(), target.number);
        if let Some(watch) = self.on.iter_mut().find(|watch| watch.is(repository, number)) {
            watch.until = until.unwrap_or(watch.until);
            return true;
        }
        let ended = self
            .ended
            .iter()
            .position(|watch| watch.is(repository, number))
            .map(|at| self.ended.remove(at).cursor);
        let cursor = match ended {
            None => Cursor {
                awaiting_baseline: Some(Baseline::All),
                checks_pending: checks_pending.unwrap_or(false),
                ..Cursor::default()
            },
            Some(ended) => Cursor {
                head: ended.head,
                failed_checks: ended.failed_checks,
                passed: ended.passed,
                passed_checks: ended.passed_checks,
                owed_checks: ended.owed_checks,
                awaiting_approval: ended.awaiting_approval,
                approval_announced: ended.approval_announced,
                conflicting: ended.conflicting,
                awaiting_baseline: ended.awaiting_baseline,
                checks_awaited_since_ms: ended.checks_awaited_since_ms,
                // A finished-checks answer kept from the last watch may be
                // an older head's.
                checks_pending: checks_pending.unwrap_or(ended.checks_pending),
                ..Cursor::default()
            },
        };
        self.on.push(Watch {
            url: target.url,
            number,
            repository: target.repository,
            until: until.unwrap_or_default(),
            cursor: Cursor {
                started_ms: now_ms,
                remarks_through_ms: now_ms,
                ..cursor
            },
        });
        false
    }

    /// Save what a read told the watch on `repository#number` started at
    /// `started_ms`: only while that same watch is on, so a stop or a new
    /// watch that landed during the read wins. Answers whether it saved.
    pub fn advance(&mut self, repository: &str, number: u64, started_ms: i64, next: Cursor) -> bool {
        match self
            .on
            .iter_mut()
            .find(|watch| watch.is(repository, number) && watch.cursor.started_ms == started_ms)
        {
            Some(watch) => {
                watch.cursor = next;
                true
            }
            None => false,
        }
    }

    /// End the watch on `repository#number`, keeping what it told as the
    /// PR's ended watch: with `last`, what its final read saw (a watch the
    /// Server ends), else its cursor as it stands (one the agent or the
    /// operator stops). `started_ms`, when given, must be the watch's.
    /// Answers whether one ended.
    pub fn end(
        &mut self,
        repository: &str,
        number: u64,
        started_ms: Option<i64>,
        last: Option<Cursor>,
    ) -> bool {
        let Some(at) = self.on.iter().position(|watch| {
            watch.is(repository, number)
                && started_ms.is_none_or(|started| watch.cursor.started_ms == started)
        }) else {
            return false;
        };
        let mut watch = self.on.remove(at);
        if let Some(last) = last {
            watch.cursor = last;
        }
        self.ended.retain(|ended| !ended.is(repository, number));
        self.ended.push(watch);
        true
    }

    /// End every watch, as parking or interrupting the Thread does.
    /// Answers whether any was on.
    pub fn end_all(&mut self) -> bool {
        let on = std::mem::take(&mut self.on);
        let any = !on.is_empty();
        for watch in on {
            self.ended
                .retain(|ended| !ended.is(&watch.repository, watch.number));
            self.ended.push(watch);
        }
        any
    }
}

/// Which watch a waiting wake's news is about, so the wake is dropped once
/// that news is stale (T3's `isStalePullRequestWatchStamp`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    pub repository: String,
    pub number: u64,
    pub started_ms: i64,
    /// The head the news is about; `None` for news with remarks, which a
    /// push leaves standing.
    pub head: Option<String>,
    /// The news ended the watch.
    pub ended: bool,
}

impl WatchFact {
    /// Whether the agent is still owed a wake about `stamp`: its watch is
    /// still on, on the same head, or, for the notice that ended it, no
    /// newer watch replaced it.
    pub fn owes(&self, stamp: &Stamp) -> bool {
        let (repository, number) = (stamp.repository.as_str(), stamp.number);
        if stamp.ended {
            let latest = self
                .on(repository, number)
                .or_else(|| self.ended.iter().find(|watch| watch.is(repository, number)));
            return latest.is_some_and(|watch| watch.cursor.started_ms == stamp.started_ms);
        }
        self.on(repository, number).is_some_and(|watch| {
            watch.cursor.started_ms == stamp.started_ms
                && stamp
                    .head
                    .as_ref()
                    .is_none_or(|head| watch.cursor.head.as_ref() == Some(head))
        })
    }
}

/// A PR to watch: where it is, and its number in its repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub url: String,
    pub number: u64,
    /// `owner/name`.
    pub repository: String,
}

impl Target {
    /// The PR a github.com URL names (`https://github.com/o/r/pull/7`).
    pub fn of_url(url: &str) -> Option<Self> {
        let rest = url
            .trim()
            .trim_end_matches('/')
            .strip_prefix("https://github.com/")?;
        let mut parts = rest.split('/');
        let (owner, name, pull, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        let number = number.parse().ok().filter(|_| pull == "pull" && parts.next().is_none())?;
        Self::of(&format!("{owner}/{name}"), number)
    }

    /// The PR `number` in `repository` (`owner/name`) on github.com.
    pub fn of(repository: &str, number: u64) -> Option<Self> {
        let mut parts = repository.split('/');
        let fine = |part: Option<&str>| {
            part.is_some_and(|part| {
                !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            })
        };
        (fine(parts.next()) && fine(parts.next()) && parts.next().is_none() && number > 0).then(|| {
            Self {
                url: format!("https://github.com/{repository}/pull/{number}"),
                number,
                repository: repository.to_string(),
            }
        })
    }
}

/// One watched PR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watch {
    pub url: String,
    pub number: u64,
    /// `owner/name`.
    pub repository: String,
    pub until: Until,
    pub cursor: Cursor,
}

impl Watch {
    /// Whether this is the watch of `repository#number`. GitHub's names
    /// ignore case.
    pub fn is(&self, repository: &str, number: u64) -> bool {
        self.number == number && self.repository.eq_ignore_ascii_case(repository)
    }

    /// Whether it holds its Thread working now, as T3's watch task is
    /// `awaited`: while the head's checks run, and with `until: merged`
    /// until the watch ends. Once every check finished, a person moves the
    /// PR next.
    pub fn holds(&self) -> bool {
        self.until == Until::Merged || self.cursor.checks_pending
    }
}

/// How long the watch holds its Thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Until {
    /// Until every check has finished.
    #[default]
    Checks,
    /// Until the PR merges, closes or is unwatched.
    Merged,
}

/// Everything the watch has already told, so the next reading tells only
/// what is new.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    pub started_ms: i64,
    /// The head commit last read; `None` before any read named one.
    pub head: Option<String>,
    /// The checks failing at the last read.
    pub failed_checks: Vec<String>,
    /// Whether the gate (the required checks, or every check when none is
    /// required) had passed, and which checks it was.
    pub passed: bool,
    pub passed_checks: Vec<String>,
    /// Checks owed one wake once every check has finished: the ones still
    /// pending when the required ones passed early, or the failed ones
    /// while a failure keeps the required ones from passing.
    pub owed_checks: Vec<String>,
    /// Approval holds, keyed by workflow run URL, or by check name when no
    /// run URL is known.
    pub awaiting_approval: Vec<String>,
    /// The agent was told checks await approval on this head.
    pub approval_announced: bool,
    /// The remark watermark: remarks active after it are new, and those at
    /// it are new unless their id is listed.
    pub remarks_through_ms: i64,
    pub remark_ids: Vec<String>,
    pub conflicting: bool,
    pub merge_queue: QueueSeen,
    /// Consecutive comment-only wakes.
    pub wakes: u32,
    /// What is already on the PR is recorded, not told.
    pub awaiting_baseline: Option<Baseline>,
    /// When the head was first read without any check, until a check shows
    /// up or the agent is told none did.
    pub checks_awaited_since_ms: Option<i64>,
    /// Whether checks were still running on the head at the last read.
    pub checks_pending: bool,
}

/// Which baseline a new watch still owes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Baseline {
    /// The first read records checks and conflicts without telling them.
    All,
    /// A read that could not be trusted to show the checks left their
    /// baseline to the next one.
    Checks,
}

/// The last merge queue removal the watch saw.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueueSeen {
    /// No read yet: the first one tells only removals since the watch
    /// started.
    #[default]
    Unread,
    /// Read, and the PR had never left a merge queue.
    Nothing,
    Removal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixture forge's snapshot reads as `gh pr view` would label it:
    /// running checks are pending, and the worst check is the rollup.
    #[test]
    fn a_snapshot_reads_as_the_label_would() {
        use ferrite_reviewr::forge::{self, PrView};
        let check = |name: &str, status| forge::Check {
            name: name.into(),
            status,
            url: None,
        };
        let snapshot = forge::PrSnapshot {
            number: 146,
            title: "t".into(),
            url: String::new(),
            body: String::new(),
            author: None,
            author_avatar_url: None,
            updated_at: None,
            changed_files: None,
            additions: None,
            deletions: None,
            state: forge::PrState::Open,
            is_draft: false,
            head_ref: "h".into(),
            head_is_fork: false,
            head_oid: String::new(),
            base_ref: "main".into(),
            merge: forge::Merge::Clean,
            sync: forge::Sync::InSync,
            checks: vec![
                check("a", forge::CheckStatus::Running),
                check("b", forge::CheckStatus::Success),
            ],
            comments: Vec::new(),
            comments_truncated: false,
            checks_truncated: false,
        };
        let pr = super::PullRequest::of_view(&PrView::Pr(Box::new(snapshot))).unwrap();
        assert_eq!(
            (pr.number, pr.state, pr.url),
            (146, super::PrState::Open, None)
        );
        assert_eq!(pr.checks, Some(super::CheckState::Pending));
        assert_eq!(pr.runs[0].detail, "in_progress");
        assert!(!pr.conflicting, "the snapshot says it merges cleanly");
        assert_eq!(super::PullRequest::of_view(&PrView::NoPr), None);
    }

    /// `gh`'s JSON is read for the PR's own facts, and a failing check
    /// anywhere in the rollup is what the label must say.
    #[test]
    fn a_pr_reading_takes_the_worst_check_in_the_rollup() {
        let body = r#"{"isDraft":false,"number":47,"state":"OPEN","url":"https://github.com/o/r/pull/47","statusCheckRollup":[
            {"name":"build","status":"COMPLETED","conclusion":"SUCCESS"},
            {"name":"test","status":"COMPLETED","conclusion":"FAILURE"}
        ]}"#;
        let pr = parse_pr_json(body).expect("a PR");
        assert_eq!(pr.number, 47);
        assert_eq!(pr.state, PrState::Open);
        assert!(!pr.draft);
        assert_eq!(pr.url.as_deref(), Some("https://github.com/o/r/pull/47"));
        assert_eq!(pr.checks, Some(CheckState::Failing));
        assert_eq!(pr.title, None, "gh said no title");
    }

    /// The nav card's PR popover names the PR by its title, which `gh`
    /// is asked for.
    #[test]
    fn a_pr_reading_keeps_its_title() {
        let body = r#"{"isDraft":false,"number":415,"state":"OPEN","title":"Thread cards in the nav","statusCheckRollup":[]}"#;
        let pr = parse_pr_json(body).expect("a PR");
        assert_eq!(pr.title.as_deref(), Some("Thread cards in the nav"));
    }

    /// A PR with no checks at all claims nothing — silence, never a pass.
    #[test]
    fn a_pr_without_checks_claims_no_ci() {
        let body = r#"{"isDraft":true,"number":3,"state":"OPEN","statusCheckRollup":[]}"#;
        let pr = parse_pr_json(body).expect("a PR");
        assert!(pr.draft);
        assert_eq!(pr.checks, None);
        assert!(pr.runs.is_empty());
    }

    /// The card lists what the mark folds: every run, with the workflow
    /// above it, the forge's own word for where it stands, and the link
    /// to its log.
    #[test]
    fn every_run_behind_the_rollup_is_kept_in_order() {
        let body = r#"{"isDraft":false,"number":9,"state":"OPEN","statusCheckRollup":[
            {"name":"build","workflowName":"CI","status":"COMPLETED","conclusion":"SUCCESS",
             "detailsUrl":"https://github.com/o/r/actions/runs/1"},
            {"name":"test (windows)","workflowName":"CI","status":"IN_PROGRESS",
             "detailsUrl":"https://github.com/o/r/actions/runs/2"}
        ]}"#;
        let pr = parse_pr_json(body).expect("a PR");
        assert_eq!(pr.checks, Some(CheckState::Pending));
        let names: Vec<&str> = pr.runs.iter().map(|run| run.name.as_str()).collect();
        assert_eq!(names, ["build", "test (windows)"]);
        assert_eq!(pr.runs[0].workflow.as_deref(), Some("CI"));
        assert_eq!(pr.runs[0].detail, "success");
        assert_eq!(pr.runs[1].detail, "in_progress");
        assert_eq!(pr.runs[1].state, CheckState::Pending);
        assert_eq!(
            pr.runs[0].url.as_deref(),
            Some("https://github.com/o/r/actions/runs/1")
        );
    }

    /// A posted commit status names itself `context` and links through
    /// `targetUrl`, and belongs to no workflow — the same row, read from
    /// different keys.
    #[test]
    fn a_posted_commit_status_is_listed_beside_the_actions_jobs() {
        let body = r#"{"isDraft":false,"number":9,"state":"OPEN","statusCheckRollup":[
            {"name":"build","workflowName":"CI","status":"COMPLETED","conclusion":"SUCCESS"},
            {"context":"codecov/patch","state":"SUCCESS","targetUrl":"https://codecov.io/x"}
        ]}"#;
        let pr = parse_pr_json(body).expect("a PR");
        assert_eq!(pr.checks, Some(CheckState::Passing));
        assert_eq!(pr.runs[1].name, "codecov/patch");
        assert_eq!(pr.runs[1].workflow, None);
        assert_eq!(pr.runs[1].url.as_deref(), Some("https://codecov.io/x"));
    }

    /// A skipped job is not a pass: it keeps its own state, and the mark
    /// counts it as neither.
    #[test]
    fn skipped_runs_count_as_themselves() {
        let body = r#"{"isDraft":false,"number":9,"state":"OPEN","statusCheckRollup":[
            {"name":"build","status":"COMPLETED","conclusion":"SUCCESS"},
            {"name":"deploy","status":"COMPLETED","conclusion":"SKIPPED"}
        ]}"#;
        let pr = parse_pr_json(body).expect("a PR");
        assert_eq!(pr.checks, Some(CheckState::Passing));
        let tally = pr.tally();
        assert_eq!(tally.passing, 1);
        assert_eq!(tally.skipped, 1);
        assert_eq!(tally.failing, 0);
        assert_eq!(tally.total(), 2);
        assert_eq!(tally.settled(), 2);
    }

    /// A rollup where nothing ran and nothing passed has vouched for
    /// nothing, and reads as pending rather than green.
    #[test]
    fn an_all_skipped_rollup_is_not_a_pass() {
        let body = r#"{"isDraft":false,"number":9,"state":"OPEN","statusCheckRollup":[
            {"name":"deploy","status":"COMPLETED","conclusion":"SKIPPED"}
        ]}"#;
        let pr = parse_pr_json(body).expect("a PR");
        assert_eq!(pr.checks, Some(CheckState::Pending));
    }

    /// A conclusion we have never seen is not a word that clears CI.
    #[test]
    fn an_unknown_conclusion_stays_pending() {
        assert_eq!(check_state("brand_new_word"), CheckState::Pending);
        assert_eq!(check_state("cancelled"), CheckState::Failing);
        assert_eq!(check_state("queued"), CheckState::Pending);
    }

    /// The card says when a pull request conflicts with its base: `gh`'s
    /// `mergeable` reads `CONFLICTING`. Any other word, or none, is no
    /// conflict.
    #[test]
    fn a_pr_reading_knows_when_it_conflicts() {
        let read = |mergeable: &str| {
            let body = format!(r#"{{"number":9,"state":"OPEN","mergeable":"{mergeable}"}}"#);
            parse_pr_json(&body).expect("a PR").conflicting
        };
        assert!(read("CONFLICTING"));
        assert!(!read("MERGEABLE"));
        assert!(!read("UNKNOWN"));
        let bare = parse_pr_json(r#"{"number":9,"state":"OPEN"}"#).expect("a PR");
        assert!(!bare.conflicting);
    }

    /// Not JSON at all, or JSON without a number: no PR, not a panic.
    #[test]
    fn junk_from_gh_reads_as_no_pr() {
        assert!(parse_pr_json("no such pr").is_none());
        assert!(parse_pr_json("{}").is_none());
    }
}
