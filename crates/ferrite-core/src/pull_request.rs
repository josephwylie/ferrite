//! A pull request as a window shows it: the branch's PR as the forge
//! reports it, read from `gh pr view`'s JSON, and the PRs a Thread's
//! agent watches. The `gh` that reads a PR and the watch that follows
//! one are the Server's (`workspace`, `pr_watch`).

use serde::{Deserialize, Serialize};

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
