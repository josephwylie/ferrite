//! A watched pull request: what its watch has already told, one reading of
//! the PR, and the news between the two. T3 Code's PR watch, ported from
//! its source (`apps/server/src/orchestration-v2/pullRequestWatch.ts` at
//! 0fd48228), so a Ferrite watch wakes its agent on the same news in the
//! same words.
//!
//! Nothing here does I/O. `evaluate` compares a `Cursor` (everything the
//! watch has told) with a `Reading` (one poll of the PR) and answers a
//! `Report`: the news, the cursor to save, and whether the watch ends.
//! `wake_text` words a report as the prompt that wakes the agent. The
//! `read` half asks GitHub's GraphQL API for a `Reading` through a query
//! runner the caller hands in (the Server's `gh api graphql`).
//!
//! Where it differs from T3, and why:
//! - No `needs-rebase`: T3's GitHub host never reports a required rebase
//!   (only GitLab does), and GitHub is the only forge Ferrite reads.
//! - No `checksAlwaysReport` capability: GitHub always reports checks, so
//!   "no checks after 15 minutes" is always news.
//! - No fallbacks for watches T3 saved before a field existed: Ferrite has
//!   no such saves.

mod closes;
mod read;
mod wake;

pub use closes::{closed_by, Closes};
pub use read::{fingerprint, reading, Ask, Fingerprint, ReadError};
pub use crate::wake::{Outcome, Wake};
pub use wake::{combine, wake_text};

use crate::pull_request::{Baseline, Cursor, PrState, QueueSeen};
use std::collections::HashSet;

/// Consecutive comment-only wakes before the watch ends, which stops two
/// bots answering each other forever. Check, conflict or push news resets
/// the count.
pub const WAKE_LIMIT: u32 = 10;
/// How long a head may go without any check before the agent is told none
/// reported.
pub const NO_CHECKS_MS: i64 = 15 * 60_000;
/// Failed reads in a row before the Server gives up on a watch.
pub const READ_FAILURE_LIMIT: u32 = 8;

/// One poll of the PR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub state: PrState,
    /// The head commit; `None` when the read did not say.
    pub head: Option<String>,
    pub base_branch: String,
    pub checks: Vec<Check>,
    pub mergeable: Mergeable,
    /// The forge login the watch reads as; the agent's own remarks are not
    /// news. Falls back to the PR's author when unknown.
    pub viewer: Option<String>,
    pub author: Option<String>,
    /// Comments and reviews; `None` when they were not read whole, so the
    /// watermark never moves past remarks a partial read missed.
    pub remarks: Option<Vec<Remark>>,
    pub merge_queue_removal: Option<QueueRemoval>,
    pub merged_by: Option<String>,
    pub merge_commit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mergeable {
    Mergeable,
    Conflicting,
    /// The forge is still working it out, as it does after a push.
    Unknown,
}

/// One check, as the watch compares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub url: Option<String>,
    /// Branch protection requires it. With no check required, every check
    /// is the gate.
    pub required: bool,
    /// Stands for a read the forge could not finish (T3's "Workflow
    /// approval status"), not a real check: it holds the verdict open.
    pub placeholder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Success,
    Failure,
    Cancelled,
    Pending,
    ActionRequired,
    Skipped,
    Neutral,
}

impl CheckStatus {
    /// T3's word for the status, which a failed check's line names.
    pub fn word(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Cancelled => "cancelled",
            Self::Pending => "pending",
            Self::ActionRequired => "action-required",
            Self::Skipped => "skipped",
            Self::Neutral => "neutral",
        }
    }

    fn failed(self) -> bool {
        matches!(self, Self::Failure | Self::Cancelled)
    }
}

/// A comment or review on the PR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remark {
    pub id: String,
    pub author: Option<String>,
    pub body: String,
    pub created_ms: i64,
    pub edited_ms: Option<i64>,
    pub url: Option<String>,
    /// The file an inline review comment is on.
    pub path: Option<String>,
    /// A review's verdict, as the forge words it (`APPROVED`).
    pub review_state: Option<String>,
}

impl Remark {
    fn active_ms(&self) -> i64 {
        self.edited_ms.unwrap_or(self.created_ms)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueRemoval {
    pub id: String,
    pub removed_ms: i64,
    pub reason: Option<String>,
}

/// One piece of news.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Checks that newly failed or were cancelled.
    ChecksFailed(Vec<Check>),
    /// The gate passed: `count` required checks, or every check.
    ChecksPassed {
        count: usize,
        required: bool,
    },
    /// Every check finished after an earlier failure.
    ChecksFinished {
        passed: usize,
        failing: Vec<Check>,
    },
    /// A run waits on a maintainer.
    AwaitingApproval(Vec<Check>),
    /// New comments and reviews that are not the agent's own.
    Remarks(Vec<Remark>),
    /// Nothing reported on this head after `minutes`.
    NoChecks {
        minutes: i64,
    },
    /// The branch now conflicts with `base`.
    Conflicting {
        base: String,
    },
    MergeQueueRemoved {
        reason: Option<String>,
    },
}

/// Why a watch ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum End {
    Merged {
        base: String,
        by: Option<String>,
        commit: Option<String>,
    },
    Closed,
    /// `WAKE_LIMIT` comment-only wakes in a row.
    CommentLimit,
    /// The Server gave up after `READ_FAILURE_LIMIT` failed reads.
    /// `problem` is the forge's own setup instruction when the Server
    /// cannot use it at all.
    Unreadable {
        problem: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub changes: Vec<Change>,
    /// The cursor to save.
    pub next: Cursor,
    pub end: Option<End>,
}

/// Compare what the watch has told with a fresh reading: T3's
/// `evaluatePullRequestWatch`, plus its reactor's rule that a merged or
/// closed PR ends the watch.
pub fn evaluate(watch: &Cursor, reading: &Reading, now_ms: i64) -> Report {
    let end = match reading.state {
        PrState::Open => None,
        PrState::Merged => Some(End::Merged {
            base: reading.base_branch.clone(),
            by: reading.merged_by.clone(),
            commit: reading.merge_commit.clone(),
        }),
        PrState::Closed => Some(End::Closed),
    };
    if end.is_some() {
        return Report {
            changes: Vec::new(),
            next: watch.clone(),
            end,
        };
    }

    let mut changes = Vec::new();
    // A read without a head says nothing about pushes, so it keeps the
    // last known one.
    let head = reading.head.clone().or_else(|| watch.head.clone());
    let head_moved = watch.head.is_some() && head != watch.head;
    let checks_baseline = watch.awaiting_baseline.is_some() && !head_moved;
    let kept = |list: &Vec<String>| if head_moved { Vec::new() } else { list.clone() };

    // Hosts can return no checks on read failure; keep the prior state.
    let mut failed_checks = kept(&watch.failed_checks);
    let mut passed = !head_moved && watch.passed;
    let mut passed_checks = kept(&watch.passed_checks);
    let mut owed_checks = kept(&watch.owed_checks);
    let owed_before = owed_checks.clone();
    let mut awaiting_approval = kept(&watch.awaiting_approval);
    let mut approval_announced = !head_moved && watch.approval_announced;
    let mut checks_pending = !head_moved && watch.checks_pending;
    // An unavailable approval read cannot establish success or that no
    // checks exist.
    let unread = reading.checks.iter().any(|check| check.placeholder);
    let checks: Vec<&Check> = reading.checks.iter().filter(|c| !c.placeholder).collect();
    let status_is = |check: &&Check, status: CheckStatus| check.status == status;
    let failed: Vec<&Check> = checks
        .iter()
        .copied()
        .filter(|c| c.status.failed())
        .collect();
    let awaiting: Vec<&Check> = checks
        .iter()
        .copied()
        .filter(|c| status_is(c, CheckStatus::ActionRequired))
        .collect();
    let failed_before = failed_checks.clone();
    let newly_failed: Vec<&Check> = failed
        .iter()
        .copied()
        .filter(|c| !failed_before.contains(&c.name))
        .collect();
    if !checks.is_empty() {
        // A run awaiting approval waits on a person, like a review, so only
        // running checks count. An unfinished baseline read keeps the seed
        // for the next one.
        checks_pending = checks.iter().any(|c| status_is(c, CheckStatus::Pending))
            || (checks_baseline && unread && watch.checks_pending);
        // The baseline hides failures from before the watch, but CI running
        // at the start can fail before the first read.
        if !newly_failed.is_empty() && (!checks_baseline || watch.checks_pending) {
            changes.push(Change::ChecksFailed(owned(&newly_failed)));
        }
        if !approval_announced && !awaiting.is_empty() {
            if !checks_baseline {
                changes.push(Change::AwaitingApproval(owned(&awaiting)));
            }
            approval_announced = true;
        }
        failed_checks = names(&failed);

        // Approval reads can omit gated runs. Keep each hold until its jobs
        // finish.
        let mut awaiting_now: Vec<String> = Vec::new();
        for key in awaiting.iter().map(|c| approval_key(c)) {
            if !awaiting_now.contains(&key) {
                awaiting_now.push(key);
            }
        }
        let name_unfinished = |name: &str| {
            let mut matching = checks.iter().filter(|c| c.name == name).peekable();
            matching.peek().is_none()
                || matching
                    .any(|c| matches!(c.status, CheckStatus::Pending | CheckStatus::ActionRequired))
        };
        // GitHub deduplicates jobs across runs, so a visible pending job may
        // hide one in an awaited run. A pending check without a URL could be
        // such a job; one with another host's URL cannot.
        let jobs_pending = checks.iter().any(|c| {
            c.status == CheckStatus::Pending
                && c.url
                    .as_deref()
                    .is_none_or(|url| workflow_run_of(url).is_some())
        });
        let finished: HashSet<Option<&str>> = if jobs_pending {
            HashSet::new()
        } else {
            checks
                .iter()
                .filter(|c| !matches!(c.status, CheckStatus::ActionRequired | CheckStatus::Pending))
                .map(|c| c.url.as_deref().and_then(workflow_run_of))
                .collect()
        };
        let held: Vec<String> = awaiting_approval
            .iter()
            .filter(|key| {
                !awaiting_now.contains(*key)
                    && if workflow_run_of(key) == Some(key.as_str()) {
                        !finished.contains(&Some(key.as_str()))
                    } else {
                        name_unfinished(key)
                    }
            })
            .cloned()
            .collect();
        awaiting_approval = awaiting_now.into_iter().chain(held).collect();
    }
    let required: Vec<&Check> = checks.iter().copied().filter(|c| c.required).collect();
    let gate = if required.is_empty() {
        &checks
    } else {
        &required
    };
    let passed_now = awaiting_approval.is_empty()
        && gate
            .iter()
            .all(|c| c.status != CheckStatus::Pending && !c.status.failed());
    if !checks.is_empty() && !(unread && passed_now) {
        let gate_names = names(gate);
        // Required checks can finish between sweeps; detect new names. With
        // no required checks, ignore advisory gate growth.
        let gate_grew =
            !required.is_empty() && gate_names.iter().any(|name| !passed_checks.contains(name));
        // Other checks still pending when required ones passed are owed one
        // wake when all settle. Only those are owed, so later advisory churn
        // never wakes. One turned required is gate news.
        let owed: Vec<String> = if passed_now && passed {
            checks
                .iter()
                .filter(|c| !c.required && owed_checks.contains(&c.name))
                .map(|c| c.name.clone())
                .collect()
        } else {
            Vec::new()
        };
        let settled = failed.is_empty()
            && awaiting_approval.is_empty()
            && checks.iter().all(|c| c.status != CheckStatus::Pending);
        owed_checks = owed.clone();
        if passed_now && (!passed || gate_grew) {
            if !checks_baseline {
                changes.push(Change::ChecksPassed {
                    count: gate.len(),
                    required: !required.is_empty(),
                });
            }
            owed_checks = if required.is_empty() {
                Vec::new()
            } else {
                checks
                    .iter()
                    .filter(|c| !c.required && c.status == CheckStatus::Pending)
                    .map(|c| c.name.clone())
                    .collect()
            };
        } else if settled && !owed.is_empty() {
            changes.push(Change::ChecksPassed {
                count: checks.len(),
                required: false,
            });
            owed_checks = Vec::new();
        } else if gate.iter().any(|c| c.status.failed()) {
            // A failed gate never sends the passed wake. A failure seen while
            // other checks still run, or a failed check that ran again, owes
            // one wake once every check has finished.
            let rerun = failed_before
                .iter()
                .any(|name| !failed_checks.contains(name));
            let running = unread
                || !awaiting_approval.is_empty()
                || checks.iter().any(|c| {
                    matches!(c.status, CheckStatus::Pending | CheckStatus::ActionRequired)
                });
            if running {
                if !owed_before.is_empty() || rerun || !newly_failed.is_empty() {
                    owed_checks = failed_checks.clone();
                }
            } else if !owed_before.is_empty() || rerun {
                changes.push(Change::ChecksFinished {
                    passed: checks
                        .iter()
                        .filter(|c| c.status == CheckStatus::Success)
                        .count(),
                    failing: owned(&failed),
                });
            }
        }
        passed = passed_now;
        passed_checks = if passed_now { gate_names } else { Vec::new() };
    }

    // Only a head never read with checks waits for them. CI can register
    // late, so the watch waits before telling that none reported.
    let awaited_before = if head_moved || watch.awaiting_baseline == Some(Baseline::All) {
        Some(now_ms)
    } else {
        watch.checks_awaited_since_ms
    };
    let mut checks_awaited_since_ms = if checks.is_empty() {
        awaited_before
    } else {
        None
    };
    let awaited_ms = checks_awaited_since_ms.map_or(0, |since| now_ms - since);
    if !unread && awaited_ms >= NO_CHECKS_MS {
        changes.push(Change::NoChecks {
            minutes: (awaited_ms as f64 / 60_000.0).round() as i64,
        });
        checks_awaited_since_ms = None;
        checks_pending = false;
    } else if checks_awaited_since_ms.is_some() {
        checks_pending = true;
    }

    let own = reading
        .viewer
        .as_ref()
        .or(reading.author.as_ref())
        .map(|login| login.to_lowercase());
    let through = watch.remarks_through_ms;
    // Hosts can stamp remarks to the second, so remarks at the watermark
    // are told apart by id.
    let fresh: Vec<Remark> = reading
        .remarks
        .iter()
        .flatten()
        .filter(|remark| {
            let at = remark.active_ms();
            (at > through || (at == through && !watch.remark_ids.contains(&remark.id)))
                && remark.author.as_ref().map(|login| login.to_lowercase()) != own
        })
        .cloned()
        .collect();
    let latest = fresh.iter().map(Remark::active_ms).fold(through, i64::max);
    let at_latest = fresh.iter().filter(|remark| remark.active_ms() == latest);
    let mut remark_ids = if latest == through {
        watch.remark_ids.clone()
    } else {
        Vec::new()
    };
    remark_ids.extend(at_latest.map(|remark| remark.id.clone()));
    if !fresh.is_empty() {
        changes.push(Change::Remarks(fresh));
    }

    // Like a failed check, a conflict a push did not resolve is news for
    // the new head.
    let told_conflict = !head_moved && watch.conflicting;
    if watch.awaiting_baseline != Some(Baseline::All)
        && reading.mergeable == Mergeable::Conflicting
        && !told_conflict
    {
        changes.push(Change::Conflicting {
            base: reading.base_branch.clone(),
        });
    }
    // Unknown is a host still computing after a push; only a clean answer
    // clears a conflict.
    let conflicting = match reading.mergeable {
        Mergeable::Unknown => told_conflict,
        Mergeable::Conflicting => true,
        Mergeable::Mergeable => false,
    };

    // The queue's checks run on a commit of its own, so its head can stay
    // green after the queue drops it. After the first read, ids avoid
    // comparing the server clock with the host's; the first read uses whole
    // seconds to exclude historical removals.
    let merge_queue = match &reading.merge_queue_removal {
        Some(removal) => {
            let seen = match &watch.merge_queue {
                QueueSeen::Removal(id) => Some(id),
                QueueSeen::Unread | QueueSeen::Nothing => None,
            };
            // Merging also records a removal, up to a second before the PR
            // reads as merged.
            if removal.reason.as_deref() != Some("merged")
                && seen != Some(&removal.id)
                && (watch.merge_queue != QueueSeen::Unread
                    || removal.removed_ms >= watch.started_ms.div_euclid(1000) * 1000)
            {
                changes.push(Change::MergeQueueRemoved {
                    reason: removal.reason.clone(),
                });
            }
            QueueSeen::Removal(removal.id.clone())
        }
        None => match &watch.merge_queue {
            QueueSeen::Unread => QueueSeen::Nothing,
            seen => seen.clone(),
        },
    };

    let comments_only =
        !changes.is_empty() && changes.iter().all(|c| matches!(c, Change::Remarks(_)));
    let progress = head_moved || (!changes.is_empty() && !comments_only);
    let wakes = (if progress { 0 } else { watch.wakes }) + u32::from(comments_only);
    Report {
        changes,
        next: Cursor {
            started_ms: watch.started_ms,
            head,
            failed_checks,
            passed,
            passed_checks,
            owed_checks,
            awaiting_approval,
            approval_announced,
            remarks_through_ms: latest,
            remark_ids,
            conflicting,
            merge_queue,
            wakes,
            // Only a read that cannot be trusted to show the checks leaves
            // the baseline to the next one.
            awaiting_baseline: (checks_baseline
                && (unread || (reading.head.is_none() && checks.is_empty())))
            .then_some(Baseline::Checks),
            checks_awaited_since_ms,
            checks_pending,
        },
        end: (comments_only && wakes >= WAKE_LIMIT).then_some(End::CommentLimit),
    }
}

fn owned(checks: &[&Check]) -> Vec<Check> {
    checks.iter().map(|&check| check.clone()).collect()
}

fn names(checks: &[&Check]) -> Vec<String> {
    checks.iter().map(|check| check.name.clone()).collect()
}

/// A GitHub Actions job's URL extends its workflow run's URL; their names
/// need not match. The run's URL, when `url` is one of them.
fn workflow_run_of(url: &str) -> Option<&str> {
    const RUNS: &str = "/actions/runs/";
    // The last `/actions/runs/<digits>`, as T3's greedy `^.*` finds it.
    url.rmatch_indices(RUNS).find_map(|(at, _)| {
        let id = at + RUNS.len();
        let digits = url[id..].bytes().take_while(u8::is_ascii_digit).count();
        (digits > 0).then(|| &url[..id + digits])
    })
}

fn approval_key(check: &Check) -> String {
    check
        .url
        .as_deref()
        .and_then(workflow_run_of)
        .unwrap_or(&check.name)
        .to_string()
}

#[cfg(test)]
mod tests;
