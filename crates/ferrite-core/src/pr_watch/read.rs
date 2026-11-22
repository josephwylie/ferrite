//! One poll of a watched PR through `gh`: the commands a Server runs, and
//! what they print read into a `Reading`. The checks are read as T3 Code's
//! GitHub host reads them (`gitHubPullRequestJson.ts` and
//! `pullRequestChecks.ts` at 0fd48228): the same status words, the newest
//! run of each check, and a workflow's name before a job name two
//! workflows share.
//!
//! What `gh pr view` cannot say, and so a reading leaves out:
//! - Which checks are required. `statusCheckRollup` has no `isRequired`;
//!   `gh pr checks --required` lists them, so a second command does.
//! - Who the viewer is: the Server knows its own `gh` login.
//! - Edits to remarks (no `lastEditedAt`), inline review comments, merge
//!   queue removals and workflow runs awaiting approval: these need
//!   GitHub's API directly.

use super::{Check, CheckStatus, Mergeable, Reading, Remark};
use crate::workspace::PrState;
use serde_json::Value;

/// What `pr_watch_view` asks `gh pr view` for.
const VIEW_FIELDS: &str =
    "state,headRefOid,baseRefName,mergeable,statusCheckRollup,comments,reviews,author,mergedBy,mergeCommit";

/// `gh`'s arguments for the PR at `url`.
pub fn pr_watch_view(url: &str) -> [&str; 5] {
    ["pr", "view", url, "--json", VIEW_FIELDS]
}

/// `gh`'s arguments for the PR's required checks. It fails when none is
/// required, which reads the same as not knowing: every check is the gate.
pub fn pr_watch_required(url: &str) -> [&str; 6] {
    ["pr", "checks", url, "--required", "--json", "name,workflow"]
}

/// Read `pr_watch_view`'s JSON, with `pr_watch_required`'s when that
/// command succeeded. `None` when `view` is not a PR.
pub fn parse_watch_json(view: &str, required: Option<&str>) -> Option<Reading> {
    let value: Value = serde_json::from_str(view).ok()?;
    let required: Vec<(String, String)> = required
        .and_then(|body| serde_json::from_str::<Vec<Value>>(body).ok())
        .unwrap_or_default()
        .iter()
        .filter_map(|row| {
            Some((
                text(row, "workflow").unwrap_or_default(),
                text(row, "name")?,
            ))
        })
        .collect();
    let state = match value.get("state")?.as_str()? {
        "MERGED" => PrState::Merged,
        "CLOSED" => PrState::Closed,
        _ => PrState::Open,
    };
    let login = |key: &str| value.get(key).and_then(|actor| text(actor, "login"));
    let entries = value
        .get("statusCheckRollup")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|node| check_entry(node, &required));
    Some(Reading {
        state,
        head: text(&value, "headRefOid"),
        base_branch: text(&value, "baseRefName").unwrap_or_default(),
        checks: dedupe_checks(entries.collect()),
        mergeable: match value.get("mergeable").and_then(Value::as_str) {
            Some("MERGEABLE") => Mergeable::Mergeable,
            Some("CONFLICTING") => Mergeable::Conflicting,
            _ => Mergeable::Unknown,
        },
        viewer: None,
        author: login("author"),
        remarks: remarks(&value),
        merge_queue_removal: None,
        merged_by: login("mergedBy"),
        merge_commit: value
            .get("mergeCommit")
            .and_then(|commit| text(commit, "oid")),
    })
}

/// A string field, trimmed; `None` when absent or blank.
fn text(node: &Value, key: &str) -> Option<String> {
    let value = node.get(key)?.as_str()?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// A rollup element with what deduplication needs: its workflow, and when
/// it last moved (`i64::MAX` while it runs, so an unfinished run outranks a
/// finished one).
struct Entry {
    check: Check,
    workflow: Option<String>,
    at: Option<i64>,
}

/// One rollup element, an Actions job (`name`) or a posted commit status
/// (`context`). One with no name at all is dropped.
fn check_entry(node: &Value, required: &[(String, String)]) -> Option<Entry> {
    let name = text(node, "name").or_else(|| text(node, "context"))?;
    let workflow = text(node, "workflowName");
    let status = check_status(node);
    let at = if status == CheckStatus::Pending {
        Some(i64::MAX)
    } else {
        stamp(node, "completedAt").or_else(|| stamp(node, "startedAt"))
    };
    let workflow_key = workflow.clone().unwrap_or_default();
    Some(Entry {
        check: Check {
            required: required
                .iter()
                .any(|(flow, job)| *flow == workflow_key && *job == name),
            name,
            status,
            url: text(node, "detailsUrl").or_else(|| text(node, "targetUrl")),
            placeholder: false,
        },
        workflow,
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

/// A forge stamp in milliseconds; `None` for GitHub's not-yet sentinel.
fn stamp(node: &Value, key: &str) -> Option<i64> {
    let at = text(node, key).filter(|at| at != "0001-01-01T00:00:00Z")?;
    millis(&at)
}

fn millis(at: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.timestamp_millis())
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

/// Comments and reviews, oldest first. A hidden comment is left out, as
/// github.com collapses it; a bodiless review that is no verdict only
/// holds line comments. `None` when `gh` did not list the comments.
fn remarks(value: &Value) -> Option<Vec<Remark>> {
    let author = |node: &Value| node.get("author").and_then(|actor| text(actor, "login"));
    let body = |node: &Value| {
        node.get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let comments = value
        .get("comments")?
        .as_array()?
        .iter()
        .filter_map(|comment| {
            if comment.get("isMinimized").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            Some(Remark {
                id: text(comment, "id")?,
                author: author(comment),
                body: body(comment),
                created_ms: millis(&text(comment, "createdAt")?)?,
                edited_ms: None,
                url: text(comment, "url"),
                path: None,
                review_state: None,
            })
        });
    let reviews = value
        .get("reviews")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|review| {
            let state = text(review, "state");
            let verdict = matches!(
                state.as_deref(),
                Some("APPROVED" | "CHANGES_REQUESTED" | "DISMISSED")
            );
            if body(review).trim().is_empty() && !verdict {
                return None;
            }
            Some(Remark {
                id: text(review, "id")?,
                author: author(review),
                body: body(review),
                created_ms: millis(&text(review, "submittedAt")?)?,
                edited_ms: None,
                url: None,
                path: None,
                review_state: state,
            })
        });
    let mut remarks: Vec<Remark> = comments.chain(reviews).collect();
    remarks.sort_by_key(|remark| remark.created_ms);
    Some(remarks)
}
