//! One poll of a watched PR, read through GitHub's GraphQL API as T3
//! Code's GitHub host reads it (`pullRequestReads.graphql`,
//! `pullRequestReviews.graphql`, `pullRequestBatches.graphql`,
//! `gitHubPullRequestJson.ts` and `GitHubPullRequestProvider.ts` at
//! 0fd48228): the cheap change check first ([`fingerprint`]), then the PR
//! whole ([`reading`]) — its checks with which are required, its comments,
//! reviews and inline review comments with their edits, its last merge
//! queue removal, and the login the read runs as.
//!
//! Nothing here runs a process. The caller hands in `ask`, which runs one
//! GraphQL query (the Server's `gh api graphql`, or a test's script), and
//! `rest`, which reads one REST path (the workflow runs GraphQL does not
//! list). Every page past the first is its own query, as T3's are.
//!
//! What differs from T3, and why:
//! - The detail and the remarks are one query, with the remarks left out
//!   (`$remarks: false`) when the plan needs only the detail: T3 runs them
//!   as two at once.
//! - The workflow runs are one REST read filtered here, where T3 asks
//!   twice (unfinished runs, and runs awaiting approval on a fork). T3's
//!   extra guard, which refuses approvals when another fork PR shares the
//!   head, is not ported: a run is matched by head commit and branch.
//! - Review threads' long tails are read again on every read that asks for
//!   remarks: T3 caches them between reads to save requests. What is news
//!   is the same.

use super::{Check, CheckStatus, Mergeable, QueueRemoval, Reading, Remark};
use ferrite_core::pull_request::PrState;
use serde_json::{json, Value};

/// Pages of one connection read before a read is called incomplete (T3's
/// `REVIEW_THREAD_PAGES`).
const PAGES: usize = 10;

/// Why a read failed, as the watcher treats it (T3's provider failure
/// reasons).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// GitHub's rate limit: watching pauses, and nothing counts.
    RateLimited,
    /// A transport or host failure, which can outlast any limit: tried
    /// again next sweep, never counted.
    Failed(String),
    /// GitHub refused the read (not found, forbidden): counted toward the
    /// give-up.
    Refused(String),
    /// The Server cannot use GitHub at all; counted, and `setup` is what
    /// to do about it, as the give-up wake says.
    Unusable { setup: String },
}

/// One GraphQL query and its variables.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub query: &'static str,
    pub variables: Value,
}

/// T3's watch fingerprint: the PR's state, head and check counts, and its
/// remark counts and newest edits. Unchanged, nothing on the PR moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub status: String,
    pub remarks: String,
}

/// `owner` and `name` of `owner/name`.
fn split(repository: &str) -> (&str, &str) {
    repository.split_once('/').unwrap_or((repository, ""))
}

/// The cheap change check (T3's `PullRequestWatchFingerprint`).
pub fn fingerprint(
    repository: &str,
    number: u64,
    ask: &mut dyn FnMut(Ask) -> Result<Value, ReadError>,
) -> Result<Fingerprint, ReadError> {
    let (owner, name) = split(repository);
    let answer = ask(Ask {
        query: FINGERPRINT,
        variables: json!({ "owner": owner, "name": name, "number": number, "mergeQueue": true }),
    })?;
    let pr = pull_request(&answer)?;
    let contexts = &pr["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["contexts"];
    let word = |value: &Value| value.as_str().unwrap_or_default().to_string();
    Ok(Fingerprint {
        status: [
            word(&pr["state"]),
            word(&pr["mergeable"]),
            word(&pr["headRefOid"]),
            state_counts(&contexts["checkRunCountsByState"]),
            state_counts(&contexts["statusContextCountsByState"]),
            pr["mergeQueueRemovals"]["filteredCount"]
                .as_u64()
                .unwrap_or(0)
                .to_string(),
        ]
        .join(" "),
        remarks: [
            pr["comments"]["totalCount"].as_u64().unwrap_or(0).to_string(),
            newest_edit(&pr["comments"]),
            pr["reviews"]["totalCount"].as_u64().unwrap_or(0).to_string(),
            newest_edit(&pr["reviews"]),
            pr["reviewThreads"]["totalCount"]
                .as_u64()
                .unwrap_or(0)
                .to_string(),
        ]
        .join(" "),
    })
}

fn state_counts(counts: &Value) -> String {
    let mut counts: Vec<String> = counts
        .as_array()
        .into_iter()
        .flatten()
        .filter(|count| count["count"].as_u64().unwrap_or(0) > 0)
        .map(|count| {
            format!(
                "{}:{}",
                count["state"].as_str().unwrap_or_default(),
                count["count"]
            )
        })
        .collect();
    counts.sort();
    counts.join(",")
}

fn newest_edit(connection: &Value) -> String {
    nodes(connection)
        .filter_map(|node| node["lastEditedAt"].as_str())
        .max()
        .unwrap_or_default()
        .to_string()
}

/// The PR whole: its detail, with its remarks when `remarks`, every page
/// read. A reading's `remarks` is `None` when they were not asked for or
/// not read whole, so the watermark never moves past what a partial read
/// missed.
pub fn reading(
    repository: &str,
    number: u64,
    remarks: bool,
    ask: &mut dyn FnMut(Ask) -> Result<Value, ReadError>,
    rest: &mut dyn FnMut(&str) -> Result<Value, ReadError>,
) -> Result<Reading, ReadError> {
    let (owner, name) = split(repository);
    let base = json!({ "owner": owner, "name": name, "number": number });
    let with = |extra: Value| {
        let mut variables = base.clone();
        if let (Some(variables), Some(extra)) = (variables.as_object_mut(), extra.as_object()) {
            variables.extend(extra.clone());
        }
        variables
    };
    let answer = ask(Ask {
        query: WATCH_READ,
        variables: with(json!({ "remarks": remarks, "mergeQueue": true })),
    })?;
    let pr = pull_request(&answer)?.clone();
    let viewer = text(&answer["data"]["viewer"], "login");

    let first = &pr["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["contexts"];
    let mut contexts: Vec<Value> = nodes(first).cloned().collect();
    let mut after = next_page(first);
    let mut pages = 1;
    while let Some(cursor) = after.filter(|_| pages < PAGES) {
        let page = ask(Ask {
            query: CHECK_PAGE,
            variables: with(json!({ "after": cursor })),
        })?;
        let connection = &pull_request(&page)?["commits"]["nodes"][0]["commit"]
            ["statusCheckRollup"]["contexts"];
        contexts.extend(nodes(connection).cloned());
        after = next_page(connection);
        pages += 1;
    }
    let state = match pr["state"].as_str() {
        Some("MERGED") => PrState::Merged,
        Some("CLOSED") => PrState::Closed,
        _ => PrState::Open,
    };
    let head = text(&pr, "headRefOid");
    let mut checks = dedupe_checks(contexts.iter().filter_map(check_entry).collect());
    if state == PrState::Open {
        if let Some(head) = &head {
            checks = with_workflow_runs(checks, &pr, head, repository, rest)?;
        }
    }
    // A failed remarks page leaves them unread, as T3's `readRemarks`
    // does: the checks still reach the evaluator.
    let remarks = match remarks {
        true => read_remarks(&pr, &with, ask).unwrap_or(None),
        false => None,
    };
    let removal = nodes(&pr["mergeQueueRemovals"]).next();
    Ok(Reading {
        state,
        head,
        base_branch: text(&pr, "baseRefName").unwrap_or_default(),
        checks,
        mergeable: match pr["mergeable"].as_str() {
            Some("MERGEABLE") => Mergeable::Mergeable,
            Some("CONFLICTING") => Mergeable::Conflicting,
            _ => Mergeable::Unknown,
        },
        viewer,
        author: text(&pr["author"], "login"),
        remarks,
        merge_queue_removal: removal.and_then(|removal| {
            Some(QueueRemoval {
                id: text(removal, "id")?,
                removed_ms: millis(&text(removal, "createdAt")?)?,
                reason: text(removal, "reason"),
            })
        }),
        merged_by: text(&pr["mergedBy"], "login"),
        merge_commit: text(&pr["mergeCommit"], "oid"),
    })
}

/// The answer's PR, or why there is none.
fn pull_request(answer: &Value) -> Result<&Value, ReadError> {
    let pr = &answer["data"]["repository"]["pullRequest"];
    if pr.is_object() {
        return Ok(pr);
    }
    Err(ReadError::Refused(
        answer["errors"][0]["message"]
            .as_str()
            .unwrap_or("GitHub knows no such pull request")
            .to_string(),
    ))
}

fn nodes(connection: &Value) -> impl Iterator<Item = &Value> {
    connection["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|node| node.is_object())
}

fn next_page(connection: &Value) -> Option<String> {
    let page = &connection["pageInfo"];
    (page["hasNextPage"] == true)
        .then(|| page["endCursor"].as_str().map(str::to_string))
        .flatten()
}

/// A string field, trimmed; `None` when absent or blank.
fn text(node: &Value, key: &str) -> Option<String> {
    let value = node.get(key)?.as_str()?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn millis(at: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.timestamp_millis())
}

/// A forge stamp in milliseconds; `None` for GitHub's not-yet sentinel.
fn stamp(node: &Value, key: &str) -> Option<i64> {
    let at = text(node, key).filter(|at| at != "0001-01-01T00:00:00Z")?;
    millis(&at)
}

/// A rollup context with what deduplication needs: its workflow, and when
/// it last moved (`i64::MAX` while it runs, so an unfinished run outranks a
/// finished one).
struct Entry {
    check: Check,
    workflow: Option<String>,
    at: Option<i64>,
}

/// One context: an Actions job (`name`) or a posted commit status
/// (`context`). One with no name at all is dropped.
fn check_entry(node: &Value) -> Option<Entry> {
    let name = text(node, "name").or_else(|| text(node, "context"))?;
    let status = check_status(node);
    let at = if status == CheckStatus::Pending {
        Some(i64::MAX)
    } else {
        stamp(node, "completedAt")
            .or_else(|| stamp(node, "startedAt"))
            .or_else(|| stamp(node, "createdAt"))
    };
    Some(Entry {
        check: Check {
            name,
            status,
            url: text(node, "detailsUrl").or_else(|| text(node, "targetUrl")),
            required: node["isRequired"] == true,
            placeholder: false,
        },
        workflow: text(&node["checkSuite"]["workflowRun"]["workflow"], "name"),
        at,
    })
}

/// GitHub's words for where a check stands, as T3 reads them.
fn check_status(node: &Value) -> CheckStatus {
    let word = |key: &str| {
        node.get(key)
            .and_then(Value::as_str)
            .map(|w| w.trim().to_ascii_uppercase())
    };
    if word("status").is_some_and(|status| !status.is_empty() && status != "COMPLETED") {
        return CheckStatus::Pending;
    }
    // A job's `conclusion`, else a status's `state`.
    match word("conclusion").or_else(|| word("state")).as_deref() {
        Some("SUCCESS") => CheckStatus::Success,
        Some("ACTION_REQUIRED") => CheckStatus::ActionRequired,
        Some("FAILURE" | "ERROR" | "TIMED_OUT" | "STARTUP_FAILURE") => CheckStatus::Failure,
        Some("CANCELLED") => CheckStatus::Cancelled,
        Some("SKIPPED") => CheckStatus::Skipped,
        Some("PENDING" | "EXPECTED") => CheckStatus::Pending,
        _ => CheckStatus::Neutral,
    }
}

/// Rollups repeat checks during reruns and overlapping runs, and checks
/// carry no ids: keep the newest of each workflow and name, name a job two
/// workflows share by its workflow too, and list failures first.
fn dedupe_checks(entries: Vec<Entry>) -> Vec<Check> {
    let mut newest: Vec<Entry> = Vec::new();
    for entry in entries {
        let same = newest.iter().position(|kept| {
            kept.workflow == entry.workflow && kept.check.name == entry.check.name
        });
        match same {
            None => newest.push(entry),
            Some(index) if at_least_as_new(entry.at, newest[index].at) => newest[index] = entry,
            Some(_) => {}
        }
    }
    let shared = |name: &str| {
        newest
            .iter()
            .filter(|entry| entry.check.name == name)
            .count()
            > 1
    };
    let mut checks: Vec<Check> = newest
        .iter()
        .map(|entry| {
            let mut check = entry.check.clone();
            if let Some(workflow) = &entry.workflow {
                if shared(&check.name) {
                    check.name = format!("{workflow} / {}", check.name);
                }
            }
            check
        })
        .collect();
    checks.sort_by_key(|check| !check.status.failed());
    checks
}

fn at_least_as_new(candidate: Option<i64>, kept: Option<i64>) -> bool {
    match (candidate, kept) {
        (None, kept) => kept.is_none(),
        (Some(_), None) => true,
        (Some(candidate), Some(kept)) => candidate >= kept,
    }
}

/// Workflow run statuses GitHub has not finished (T3's
/// `UNFINISHED_WORKFLOW_RUN_STATUSES`); an approval-gated run is read
/// apart, as an action-required check.
const UNFINISHED_RUNS: [&str; 5] = ["requested", "queued", "pending", "waiting", "in_progress"];

/// The workflow run a GitHub Actions URL is of.
fn run_id_of(url: Option<&str>) -> Option<u64> {
    let rest = url?.split("/actions/runs/").nth(1)?;
    rest.split('/').next()?.parse().ok()
}

/// T3's `withUnfinishedWorkflowRuns` and `withWorkflowApprovals`: a run
/// GitHub has not finished may have no check run on the head yet, so it
/// reads as a pending check; on a fork's PR, a run waiting for a
/// maintainer's approval reads as an action-required one, and a read that
/// could not tell leaves T3's placeholder, which holds the verdict open.
fn with_workflow_runs(
    mut checks: Vec<Check>,
    pr: &Value,
    head: &str,
    repository: &str,
    rest: &mut dyn FnMut(&str) -> Result<Value, ReadError>,
) -> Result<Vec<Check>, ReadError> {
    let fork = pr["isCrossRepository"] == true;
    let path = format!("repos/{repository}/actions/runs?head_sha={head}&event=pull_request&per_page=100");
    let runs = match rest(&path) {
        Ok(runs) => Some(runs),
        Err(ReadError::RateLimited) => return Err(ReadError::RateLimited),
        Err(_) => None,
    };
    let listed: Vec<&Value> = runs
        .as_ref()
        .map(|runs| runs["workflow_runs"].as_array().into_iter().flatten().collect())
        .unwrap_or_default();
    let run = |run: &Value, status: CheckStatus| {
        let id = run["id"].as_u64().unwrap_or_default();
        Check {
            name: text(run, "name").unwrap_or_else(|| format!("Workflow run {id}")),
            status,
            url: text(run, "html_url"),
            required: false,
            placeholder: false,
        }
    };
    let running: Vec<u64> = checks
        .iter()
        .filter(|check| matches!(check.status, CheckStatus::Pending | CheckStatus::ActionRequired))
        .filter_map(|check| run_id_of(check.url.as_deref()))
        .collect();
    let unfinished: Vec<Check> = listed
        .iter()
        .filter(|each| UNFINISHED_RUNS.contains(&each["status"].as_str().unwrap_or_default()))
        .filter(|each| !running.contains(&each["id"].as_u64().unwrap_or_default()))
        .map(|each| run(each, CheckStatus::Pending))
        .collect();
    if !fork {
        checks.extend(unfinished);
        return Ok(checks);
    }
    let branch = text(pr, "headRefName");
    let owner = text(&pr["headRepositoryOwner"], "login");
    let unavailable = runs.is_none() || branch.is_none() || owner.is_none();
    let represented: Vec<u64> = checks
        .iter()
        .filter(|check| check.status == CheckStatus::ActionRequired)
        .filter_map(|check| run_id_of(check.url.as_deref()))
        .collect();
    let approvals: Vec<Check> = listed
        .iter()
        .filter(|each| {
            // Maintainer-gated runs can report completed/action_required.
            (each["conclusion"] == "action_required" || each["status"] == "action_required")
                && text(each, "head_branch") == branch
                && !represented.contains(&each["id"].as_u64().unwrap_or_default())
        })
        .map(|each| run(each, CheckStatus::ActionRequired))
        .collect();
    checks.extend(unfinished);
    checks.extend(approvals);
    if unavailable {
        checks.push(Check {
            name: "Workflow approval status".into(),
            status: CheckStatus::ActionRequired,
            url: None,
            required: false,
            placeholder: true,
        });
    }
    Ok(checks)
}

/// The PR's comments, reviews and inline review comments, oldest first,
/// every page read; `None` when any part could not be read whole.
fn read_remarks(
    pr: &Value,
    with: &dyn Fn(Value) -> Value,
    ask: &mut dyn FnMut(Ask) -> Result<Value, ReadError>,
) -> Result<Option<Vec<Remark>>, ReadError> {
    let mut remarks = Vec::new();
    for (connection, query, review) in [
        ("comments", COMMENT_PAGE, false),
        ("reviews", REVIEW_PAGE, true),
    ] {
        let Some(read) = all_pages(&pr[connection], query, connection, with, ask)? else {
            return Ok(None);
        };
        remarks.extend(read.iter().filter_map(|node| remark(node, review, None)));
    }
    let Some(threads) = all_pages(&pr["reviewThreads"], THREAD_PAGE, "reviewThreads", with, ask)?
    else {
        return Ok(None);
    };
    for thread in &threads {
        let path = text(thread, "path");
        let first = &thread["comments"];
        let mut comments: Vec<Value> = nodes(first).cloned().collect();
        let mut after = next_page(first);
        let mut pages = 1;
        while let Some(cursor) = after {
            if pages >= PAGES {
                return Ok(None);
            }
            let page = ask(Ask {
                query: THREAD_COMMENTS,
                variables: json!({ "thread": thread["id"], "after": cursor }),
            })?;
            let connection = &page["data"]["node"]["comments"];
            comments.extend(nodes(connection).cloned());
            after = next_page(connection);
            pages += 1;
        }
        if (comments.len() as u64) < first["totalCount"].as_u64().unwrap_or(0) {
            return Ok(None);
        }
        remarks.extend(
            comments
                .iter()
                .filter_map(|node| remark(node, false, path.clone())),
        );
    }
    remarks.sort_by_key(|remark| remark.created_ms);
    Ok(Some(remarks))
}

/// Every node of a PR connection, from its first page in `first` and the
/// pages after it; `None` past `PAGES`.
fn all_pages(
    first: &Value,
    query: &'static str,
    connection: &str,
    with: &dyn Fn(Value) -> Value,
    ask: &mut dyn FnMut(Ask) -> Result<Value, ReadError>,
) -> Result<Option<Vec<Value>>, ReadError> {
    let mut all: Vec<Value> = nodes(first).cloned().collect();
    let mut after = next_page(first);
    let mut pages = 1;
    while let Some(cursor) = after {
        if pages >= PAGES {
            return Ok(None);
        }
        let page = ask(Ask {
            query,
            variables: with(json!({ "after": cursor })),
        })?;
        let next = &pull_request(&page)?[connection];
        all.extend(nodes(next).cloned());
        after = next_page(next);
        pages += 1;
    }
    Ok(Some(all))
}

/// A comment, review or inline review comment. A hidden one is left out,
/// as github.com collapses it; a bodiless review that is no verdict only
/// holds line comments.
fn remark(node: &Value, review: bool, path: Option<String>) -> Option<Remark> {
    if node["isMinimized"] == true {
        return None;
    }
    let body = node["body"].as_str().unwrap_or_default().to_string();
    let state = review.then(|| text(node, "state")).flatten();
    if review
        && body.trim().is_empty()
        && !matches!(
            state.as_deref(),
            Some("APPROVED" | "CHANGES_REQUESTED" | "DISMISSED")
        )
    {
        return None;
    }
    let created = text(node, if review { "submittedAt" } else { "createdAt" })?;
    Some(Remark {
        id: text(node, "id")?,
        author: text(&node["author"], "login"),
        body,
        created_ms: millis(&created)?,
        edited_ms: text(node, "lastEditedAt").and_then(|at| millis(&at)),
        url: text(node, "url"),
        path,
        review_state: state,
    })
}

/// T3's `PullRequestWatchFingerprint`.
pub const FINGERPRINT: &str = r#"query PullRequestWatchFingerprint($owner: String!, $name: String!, $number: Int!, $mergeQueue: Boolean!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      state
      mergeable
      headRefOid
      comments(first: 100, orderBy: { field: UPDATED_AT, direction: DESC }) { totalCount nodes { lastEditedAt } }
      reviews(last: 100) { totalCount nodes { lastEditedAt } }
      reviewThreads { totalCount }
      mergeQueueRemovals: timelineItems(itemTypes: [REMOVED_FROM_MERGE_QUEUE_EVENT]) @include(if: $mergeQueue) { filteredCount }
      commits(last: 1) { nodes { commit { statusCheckRollup { contexts {
        checkRunCountsByState { state count }
        statusContextCountsByState { state count }
      } } } } }
    }
  }
}"#;

/// The detail T3's `PullRequestCore` reads for a watch, with the first
/// page of `PullRequestActivity`'s remarks and `ReviewThreads`' threads,
/// and the viewer.
pub const WATCH_READ: &str = r#"query PullRequestWatch($owner: String!, $name: String!, $number: Int!, $remarks: Boolean!, $mergeQueue: Boolean!) {
  viewer { login }
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      state
      mergeable
      headRefOid
      headRefName
      baseRefName
      isCrossRepository
      headRepositoryOwner { login }
      author { login }
      mergedBy { login }
      mergeCommit { oid }
      mergeQueueRemovals: timelineItems(last: 1, itemTypes: [REMOVED_FROM_MERGE_QUEUE_EVENT]) @include(if: $mergeQueue) {
        nodes { ... on RemovedFromMergeQueueEvent { id createdAt reason } }
      }
      commits(last: 1) { nodes { commit { statusCheckRollup { contexts(first: 100) {
        nodes { ...CheckContext }
        pageInfo { hasNextPage endCursor }
      } } } } }
      comments(first: 100) @include(if: $remarks) { ...Comments }
      reviews(first: 100) @include(if: $remarks) { ...Reviews }
      reviewThreads(first: 100) @include(if: $remarks) { ...Threads }
    }
  }
}
fragment CheckContext on StatusCheckRollupContext {
  ... on StatusContext { context state targetUrl createdAt isRequired(pullRequestNumber: $number) }
  ... on CheckRun {
    name status conclusion startedAt completedAt detailsUrl isRequired(pullRequestNumber: $number)
    checkSuite { workflowRun { workflow { name } } }
  }
}
fragment Comments on IssueCommentConnection {
  pageInfo { hasNextPage endCursor }
  nodes { id body createdAt lastEditedAt url isMinimized author { login } }
}
fragment Reviews on PullRequestReviewConnection {
  pageInfo { hasNextPage endCursor }
  nodes { id body state submittedAt lastEditedAt url isMinimized author { login } }
}
fragment Threads on PullRequestReviewThreadConnection {
  pageInfo { hasNextPage endCursor }
  nodes {
    id
    path
    comments(first: 10) {
      totalCount
      pageInfo { hasNextPage endCursor }
      nodes { id body createdAt lastEditedAt url isMinimized author { login } }
    }
  }
}"#;

/// The head's check contexts past the first page (T3's
/// `PullRequestCheckContexts`).
pub const CHECK_PAGE: &str = r#"query PullRequestCheckContexts($owner: String!, $name: String!, $number: Int!, $after: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      commits(last: 1) { nodes { commit { statusCheckRollup { contexts(first: 100, after: $after) {
        nodes {
          ... on StatusContext { context state targetUrl createdAt isRequired(pullRequestNumber: $number) }
          ... on CheckRun {
            name status conclusion startedAt completedAt detailsUrl isRequired(pullRequestNumber: $number)
            checkSuite { workflowRun { workflow { name } } }
          }
        }
        pageInfo { hasNextPage endCursor }
      } } } } }
    }
  }
}"#;

pub const COMMENT_PAGE: &str = r#"query PullRequestComments($owner: String!, $name: String!, $number: Int!, $after: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      comments(first: 100, after: $after) {
        pageInfo { hasNextPage endCursor }
        nodes { id body createdAt lastEditedAt url isMinimized author { login } }
      }
    }
  }
}"#;

pub const REVIEW_PAGE: &str = r#"query PullRequestReviews($owner: String!, $name: String!, $number: Int!, $after: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviews(first: 100, after: $after) {
        pageInfo { hasNextPage endCursor }
        nodes { id body state submittedAt lastEditedAt url isMinimized author { login } }
      }
    }
  }
}"#;

/// T3's `ReviewThreadPages`.
pub const THREAD_PAGE: &str = r#"query ReviewThreadPages($owner: String!, $name: String!, $number: Int!, $after: String) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100, after: $after) {
        pageInfo { hasNextPage endCursor }
        nodes {
          id
          path
          comments(first: 10) {
            totalCount
            pageInfo { hasNextPage endCursor }
            nodes { id body createdAt lastEditedAt url isMinimized author { login } }
          }
        }
      }
    }
  }
}"#;

/// The rest of one review thread's comments (T3's `ReviewThreadComments`).
pub const THREAD_COMMENTS: &str = r#"query ReviewThreadComments($thread: ID!, $after: String) {
  node(id: $thread) {
    ... on PullRequestReviewThread {
      comments(first: 100, after: $after) {
        pageInfo { hasNextPage endCursor }
        nodes { id body createdAt lastEditedAt url isMinimized author { login } }
      }
    }
  }
}"#;
