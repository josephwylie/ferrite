//! A Task's pull request and its merging (#223): the PR it carries, from
//! its input or from the Step that opened it, with every revision
//! reported; approval tied to the revision the operator saw; evidence
//! kept apart by kind, each with the revision it concerns; and the
//! worktree tidied once the PR is merged.
//!
//! Ferrite reads nothing from GitHub here and writes nothing to it: a
//! Worker reports what its own tools did. Ferrite never closes a PR and
//! never deletes a branch.

use super::{Answer, Effect, Refusal, RefusalCode};
use crate::factory_board::{
    short, Actor, Evidence, EvidenceKind, MergeOutcome, PullRequest, Revision, Status, Task,
};

/// How often a merged Task's kept worktree is tried again (G3).
pub const TIDY_AGAIN_MS: i64 = 10 * 60_000;

/// A merged Task whose worktree was kept, last tried long enough ago: it
/// is tried again, never forced. Whether it was.
pub(super) fn tidy_again(task: &mut Task, now_ms: i64, effects: &mut Vec<Effect>) -> bool {
    let merged = task.delivery.pr.as_ref().is_some_and(|pr| pr.merged);
    let (Some(path), Some(tried)) = (task.worktree.clone(), task.delivery.tidied_ms) else {
        return false;
    };
    if !merged || task.delivery.kept.is_none() || now_ms - tried < TIDY_AGAIN_MS {
        return false;
    }
    task.delivery.tidied_ms = Some(now_ms);
    effects.push(Effect::Tidy {
        task: task.id.clone(),
        path,
    });
    true
}

fn invalid(message: impl Into<String>) -> Refusal {
    Refusal::new(RefusalCode::Invalid, message)
}

/// The PR a Task is started with, validated.
pub(super) fn given(pr: Option<String>) -> Result<Option<PullRequest>, Refusal> {
    pr.filter(|text| !text.trim().is_empty())
        .map(|text| PullRequest::parse(&text).map_err(invalid))
        .transpose()
}

/// The current Worker reports the PR's head (and, the first time, which
/// PR): a new head is the next revision.
pub(super) fn reported_revision(
    task: &mut Task,
    head: String,
    pr: Option<String>,
    now_ms: i64,
) -> Result<Answer, Refusal> {
    let head = head.trim().to_string();
    if !(7..=64).contains(&head.len()) || !head.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid(format!(
            "`{head}` is no commit: report the PR's head commit hash"
        )));
    }
    let named = given(pr)?;
    match (&task.delivery.pr, named) {
        (None, None) => {
            return Err(invalid(
                "the task has no pull request yet: say which with --pr owner/repository#number",
            ))
        }
        (Some(have), Some(named))
            if (have.repository.as_str(), have.number)
                != (named.repository.as_str(), named.number) =>
        {
            return Err(invalid(format!(
                "this task's pull request is {}; {} is another",
                have.label(),
                named.label()
            )))
        }
        (None, Some(named)) => task.delivery.pr = Some(named),
        (Some(_), _) => {}
    }
    let by = task.asker();
    let pr = task.delivery.pr.as_mut().expect("named above");
    if pr.current().is_some_and(|each| each.head == head) {
        return Ok(Answer::Changed(false));
    }
    let revision = pr.current().map_or(1, |each| each.revision + 1);
    pr.revisions.push(Revision {
        revision,
        head: head.clone(),
        by: by.clone(),
        at_ms: now_ms,
    });
    let label = pr.label();
    task.note(
        now_ms,
        Actor::Agent,
        format!(
            "{by} reported {label} revision {revision} (head {})",
            short(&head)
        ),
    );
    Ok(Answer::Changed(true))
}

/// The current Worker reports what a command or tool showed: kept as a
/// tool result, about the current revision.
pub(super) fn reported_evidence(
    task: &mut Task,
    text: String,
    now_ms: i64,
) -> Result<Answer, Refusal> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(invalid("say what the command or tool showed"));
    }
    let by = task.asker();
    task.note(now_ms, Actor::Tool, format!("{by} reported: {text}"));
    task.evidence(EvidenceKind::Tool, by, text, now_ms);
    Ok(Answer::Changed(true))
}

/// What every Step is told of the PR: which it is, its current revision
/// and the approved one, and how to report a new head.
pub(super) fn context(task: &Task) -> String {
    let Some(pr) = &task.delivery.pr else {
        return "\n## Pull request\n\nThe task has no pull request yet. If you open one, report it \
                and its head commit with `report_revision` (`head`, and `pr` as \
                `owner/repository#number`).\n"
            .into();
    };
    let mut out = format!("\n## Pull request\n\n{} ({})\n", pr.label(), pr.url());
    match pr.current() {
        Some(now) => out.push_str(&format!(
            "Current revision {}: head {}, reported by {}.\n",
            now.revision, now.head, now.by
        )),
        None => out.push_str("No revision has been reported yet.\n"),
    }
    if let Some(approved) = pr.approved.and_then(|at| pr.revision(at)) {
        out.push_str(&format!(
            "Approved: revision {} (head {}).{}\n",
            approved.revision,
            approved.head,
            if pr.approval_current() {
                ""
            } else {
                " The current revision is not approved."
            }
        ));
    }
    out.push_str(&format!(
        "\nWhen you push commits to it, report its new head with `report_revision` (`head`).\n\n\
         Keep what a check or tool showed as evidence with `report_evidence`, one result at a \
         time: `<the command>: <what it showed>`.\n",
    ));
    out
}

/// A Task with a PR whose head nobody has read yet: GitHub is asked,
/// so it has a first revision before any Worker reports one (G2).
pub(super) fn read_when_started(task: &Task, effects: &mut Vec<Effect>) {
    if let Some(pr) = task
        .delivery
        .pr
        .as_ref()
        .filter(|pr| pr.revisions.is_empty())
    {
        effects.push(Effect::ReadHead {
            task: task.id.clone(),
            repository: pr.repository.clone(),
            number: pr.number,
        });
    }
}

/// What GitHub said the PR's head is: a new revision when it moved, by
/// GitHub; why it could not say, in the activity.
pub(super) fn read_head(task: &mut Task, head: Result<String, String>, now_ms: i64) -> Answer {
    let Some(pr) = task.delivery.pr.as_mut() else {
        return Answer::Changed(false);
    };
    let label = pr.label();
    let head = match head {
        Ok(head) => head.trim().to_string(),
        Err(why) => {
            task.note(
                now_ms,
                Actor::Ferrite,
                format!("Could not read {label}'s head from GitHub: {why}"),
            );
            return Answer::Changed(true);
        }
    };
    if !(7..=64).contains(&head.len()) || !head.chars().all(|c| c.is_ascii_hexdigit()) {
        return Answer::Changed(false);
    }
    if pr.current().is_some_and(|each| each.head == head) {
        return Answer::Changed(false);
    }
    let revision = pr.current().map_or(1, |each| each.revision + 1);
    pr.revisions.push(Revision {
        revision,
        head: head.clone(),
        by: "GitHub".into(),
        at_ms: now_ms,
    });
    task.note(
        now_ms,
        Actor::Tool,
        format!(
            "GitHub shows {label} at revision {revision} (head {})",
            short(&head)
        ),
    );
    Answer::Changed(true)
}

/// A completed Task with a merged PR has its worktree removed (never
/// forced, its branch kept); otherwise it stays, and the Task says why.
pub(super) fn tidy(task: &mut Task, effects: &mut Vec<Effect>) {
    if task.status != Status::Completed {
        return;
    }
    let Some(path) = task.worktree.clone() else {
        return;
    };
    match &task.delivery.pr {
        Some(pr) if pr.merged => effects.push(Effect::Tidy {
            task: task.id.clone(),
            path,
        }),
        Some(pr) => {
            task.delivery.kept = Some(format!(
                "The worktree stays: {} is not confirmed merged.",
                pr.label()
            ))
        }
        None => {
            task.delivery.kept =
                Some("The worktree stays: the task has no merged pull request.".into())
        }
    }
}

/// The Server tried to remove a merged Task's worktree: gone, or kept
/// and why.
pub(super) fn tidied(
    task: &mut Task,
    kept: Option<String>,
    now_ms: i64,
) -> Result<Answer, Refusal> {
    match kept {
        None => {
            if task.worktree.take().is_none() {
                return Ok(Answer::Changed(false));
            }
            task.delivery.kept = None;
            task.delivery.tidied_ms = None;
            let branch = task.branch.clone();
            task.note(
                now_ms,
                Actor::Ferrite,
                format!("The PR is merged, so its worktree was removed; the branch {branch} stays"),
            );
        }
        Some(why) => {
            task.delivery.tidied_ms.get_or_insert(now_ms);
            if task.delivery.kept.as_ref() == Some(&why) {
                return Ok(Answer::Changed(false));
            }
            task.note(now_ms, Actor::Ferrite, why.clone());
            task.delivery.kept = Some(why);
        }
    }
    Ok(Answer::Changed(true))
}

/// How a merge Step reports: one of the six results, never `merged`
/// unless it confirmed the merge, and `head_changed` without merging
/// when the PR is not at the approved revision.
pub(super) fn merge_reporting(task: &Task) -> String {
    let results: Vec<String> = MergeOutcome::ALL
        .into_iter()
        .map(|outcome| format!("- outcome `{}`, when {}", outcome.word(), outcome.help()))
        .collect();
    let approved = match task.delivery.pr.as_ref() {
        Some(pr) => match pr.approved.and_then(|at| pr.revision(at)) {
            Some(approved) => format!(
                "Merge only revision {} (head {}), the one approved. If the PR's head is \
                 anything else, do not merge: report head_changed.\n\n",
                approved.revision, approved.head
            ),
            None => String::new(),
        },
        None => String::new(),
    };
    format!(
        "## Reporting the merge\n\n{approved}Confirm the PR actually merged before reporting \
         merged. Report the result with `report_outcome`, the summary being what showed it:\n\n\
         {}\n\nIf you cannot do the step at all, call it with outcome `unable` and what you \
         tried.\n",
        results.join("\n")
    )
}

/// This Step's last merge result was unconfirmed: before anything else
/// it finds out whether the PR merged, and never merges again blindly.
pub(super) fn reconcile(task: &Task, step: &crate::factory_board::Step) -> String {
    let last = task
        .results
        .iter()
        .rev()
        .find_map(|result| match &result.outcome {
            crate::factory_board::Outcome::Merge { result: merge, .. } if result.step == step.id => Some(*merge),
            _ => None,
        });
    if last != Some(MergeOutcome::Unknown) {
        return String::new();
    }
    let which = task
        .delivery
        .pr
        .as_ref()
        .map_or_else(|| "the task's pull request".to_string(), PullRequest::label);
    format!(
        "\n## Reconcile first\n\nThe last merge attempt's result was unconfirmed. Before \
         anything else, find out whether {which} is already merged (for example `gh pr view \
         --json state,mergedAt,mergeCommit`). If it merged, report merged without merging \
         again. Do not run the merge again until you have confirmed it did not merge.\n"
    )
}

/// What a merge Step waiting on pending checks is told.
pub(super) fn waiting_prompt(task: &Task) -> String {
    let which = task
        .delivery
        .pr
        .as_ref()
        .map_or_else(|| "the pull request".to_string(), PullRequest::label);
    format!(
        "The required checks on {which} are still pending, so the task keeps waiting with \
         you. Wait for them to finish (for example `gh pr checks --watch`), then check the PR \
         again and report the merge result.\n\n{}",
        merge_reporting(task)
    )
}

/// Approval is given for the revision current now; a later one is not
/// covered by it.
pub(super) fn approved(task: &mut Task, now_ms: i64) {
    let Some(pr) = task.delivery.pr.as_mut() else {
        return;
    };
    let Some(now) = pr.current().cloned() else {
        return;
    };
    pr.approved = Some(now.revision);
    let label = pr.label();
    task.note(
        now_ms,
        Actor::Ferrite,
        format!(
            "Approval recorded for {label} revision {} (head {})",
            now.revision,
            short(&now.head)
        ),
    );
}

/// How a piece of evidence reads, said as being about an older revision
/// when it is: never proof of the current one.
pub fn evidence_line(task: &Task, evidence: &Evidence) -> String {
    let now = task.revision_now();
    let about = match (evidence.revision, now) {
        (Some(at), Some(now)) if at < now => format!(
            " (about revision {at}, older than the current revision {now}: not evidence for it)"
        ),
        (Some(at), _) => format!(" (revision {at})"),
        (None, Some(_)) => " (before any revision was reported)".into(),
        (None, None) => String::new(),
    };
    format!(
        "{} \u{b7} {}: {}{about}",
        evidence.kind.label(),
        evidence.by,
        evidence.text
    )
}

/// The evidence a deciding agent weighs, each about its revision.
pub(super) fn evidence_section(task: &Task) -> String {
    if task.delivery.evidence.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = task
        .delivery
        .evidence
        .iter()
        .map(|each| format!("- {}", evidence_line(task, each)))
        .collect();
    format!("\n## Evidence\n\n{}\n", lines.join("\n"))
}
