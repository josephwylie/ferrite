//! Recovery (#226, ADR 0037): a Worker whose Session was lost while its
//! Task worked (its Session closed on its own, or the Server restarted)
//! is revived once, on the same Thread and conversation, and told it was
//! interrupted. A second loss in the same run of a Step stops the Task
//! with the reason. A Step is never run again from its start unasked.

use serde::{Deserialize, Serialize};

use std::collections::BTreeSet;

use super::{reporting, Actor, Effect, Seen, State, Status, Task, TaskId, Watch, ASKING, REPORTING};

/// What a Task remembers of interruptions and time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keeping {
    /// When the current run's Worker was revived; a second loss in the
    /// same run stops the Task. A new run (another Step, a return, the
    /// operator's Resume) starts without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revived_ms: Option<i64>,
}

/// Why a Worker has no Session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Cause {
    /// The Server restarted while it worked.
    Restart,
    /// Its Session ended on its own mid-Step.
    Ended,
}

/// The Task's Worker was lost while it worked: revive it once, or stop
/// the Task the second time. Whether it was.
pub(super) fn lost(
    task: &mut Task,
    seen: &Seen,
    cause: Cause,
    now_ms: i64,
    effects: &mut Vec<Effect>,
) -> bool {
    if !seen.lost || seen.busy {
        return false;
    }
    // A prompt waiting to be sent starts its Session itself.
    if task.watch.prompt.is_some() {
        return false;
    }
    let permission = task.status == Status::NeedsHuman && task.oversight.permission.is_some();
    if task.status != Status::Working && !permission {
        return false;
    }
    let Some(thread) = task.worker else {
        return false;
    };
    // A Provider Decision dies with its Session: nobody can answer it now.
    task.oversight.permission = None;
    let session = session_name(task);
    let happened = match cause {
        Cause::Restart => format!("Ferrite restarted while {session} was working"),
        Cause::Ended => format!("{session} ended unexpectedly"),
    };
    if task.keeping.revived_ms.is_some() {
        task.note(
            now_ms,
            Actor::Ferrite,
            format!("Session interrupted again: {happened}. Stopped the task"),
        );
        task.reason = Some(format!(
            "{happened}, after it had been revived once. Nothing was retried: resume it or \
             redirect the task."
        ));
        task.enter(Status::Stopped, effects);
        return true;
    }
    task.keeping.revived_ms = Some(now_ms);
    task.reason = None;
    if task.status != Status::Working {
        task.enter(Status::Working, effects);
    }
    task.note(
        now_ms,
        Actor::Ferrite,
        format!("Session interrupted: {happened}. Revived it once and told it to continue"),
    );
    effects.push(Effect::Prompt {
        thread,
        text: revival_prompt(task, cause),
    });
    task.watch = Watch {
        baseline: seen.turns,
        ..Watch::default()
    };
    true
}

/// "The Claude session for Builder", as the activity and a stop say it.
fn session_name(task: &Task) -> String {
    let provider = match task.agent.provider {
        crate::store::Provider::Claude => "Claude",
        crate::store::Provider::Codex => "Codex",
    };
    match task.current_step() {
        Some(step) => format!("the {provider} session for {}", step.name),
        None => format!("the agent's {provider} session"),
    }
}

/// What a revived Worker is told: it was interrupted, to continue, to
/// check before repeating anything outside its worktree, and to report.
fn revival_prompt(task: &Task, cause: Cause) -> String {
    let why = match cause {
        Cause::Restart => "Ferrite restarted while you were working on this task",
        Cause::Ended => "Your session ended unexpectedly while you were working on this task",
    };
    let step = task.current_step();
    let merge = if step.is_some_and(|step| step.outcome_routes.is_some()) {
        "\n\nA merge may already have happened: check the pull request's state before you merge \
         again, and report what you find."
    } else {
        ""
    };
    let reporting = match step {
        Some(step) => reporting(task, step),
        None => format!("{REPORTING}\n\n{ASKING}"),
    };
    format!(
        "You were interrupted: {why}. This is the same conversation, started again. Continue \
         where you left off. Before you repeat anything with an effect outside this worktree (a \
         push, a merge, a comment), check whether it already happened.{merge}\n\n{reporting}"
    )
}

/// Each Task the restart caught between deciding to start its Worker and
/// starting it stops: whether that Worker began is not known, so nothing
/// is started again unasked. Resume starts it.
pub(super) fn unstarted(
    state: &mut State,
    unstarted: &BTreeSet<TaskId>,
    now_ms: i64,
    effects: &mut Vec<Effect>,
) {
    for task in &mut state.tasks {
        if !unstarted.contains(&task.id)
            || task.status != Status::Working
            || task.worker.is_some()
        {
            continue;
        }
        task.note(
            now_ms,
            Actor::Ferrite,
            "Ferrite restarted before the agent started; stopped the task",
        );
        task.reason =
            Some("Ferrite restarted before this task's agent started. Resume to start it.".into());
        task.enter(Status::Stopped, effects);
    }
}
