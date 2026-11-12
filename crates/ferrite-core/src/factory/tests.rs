//! The Factory's state machine through its interface: commands in, step
//! → effects, the board out, with the clock passed in.

use std::collections::BTreeMap;

use super::*;
use crate::store::Provider;

fn project_id(id: u64) -> ProjectId {
    serde_json::from_value(id.into()).unwrap()
}

fn project() -> ProjectId {
    project_id(1)
}

fn agent() -> Agent {
    Agent {
        provider: Provider::Claude,
        model: Some("claude-model".into()),
        effort: None,
    }
}

fn start(factory: &mut Factory, instructions: &str) -> TaskId {
    match factory.command(
        Command::Start {
            project: project(),
            instructions: instructions.into(),
            agent: agent(),
            request: None,
            workflow: None,
            pr: None,
        },
        0,
    ) {
        Ok(Answer::Task(task)) => task,
        other => panic!("{other:?}"),
    }
}

const WORKER: ThreadId = ThreadId::new(40);

/// What the Server tells the step of each Worker it sees.
fn seen(busy: bool, turns: u64) -> BTreeMap<ThreadId, Seen> {
    BTreeMap::from([(
        WORKER,
        Seen {
            busy,
            turns,
            decision: None,
            lost: false,
            closed: false,
        },
    )])
}

fn status(factory: &Factory, task: &TaskId) -> Status {
    factory.task(task).expect("the task").status
}

/// A Task started, its Worker up as `WORKER`, working on its first turn.
fn working(factory: &mut Factory) -> TaskId {
    let task = start(factory, "Fix the flaky test");
    factory.step(1, &BTreeMap::new());
    factory
        .command(
            Command::WorkerStarted {
                task: task.clone(),
                thread: WORKER,
                worktree: Some("/repo/.worktrees/factory/t-1".into()),
            },
            2,
        )
        .unwrap();
    assert!(factory.step(3, &seen(true, 0)).is_empty());
    task
}

fn prompts(effects: &[Effect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Prompt { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn notified(effects: &[Effect]) -> Vec<Status> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Notify { status, .. } => Some(*status),
            _ => None,
        })
        .collect()
}

fn report(factory: &mut Factory, outcome: Outcome) -> Result<Answer, Refusal> {
    factory.command(
        Command::Report {
            worker: WORKER,
            outcome,
        },
        10,
    )
}

#[test]
fn a_fresh_factory_has_no_tasks_and_starts_nothing() {
    let mut factory = Factory::in_memory();
    assert!(factory.board().tasks.is_empty());
    assert!(factory.step(0, &BTreeMap::new()).is_empty());
}

/// Nothing starts until the start command; then the step starts exactly
/// one Worker for it, with the Task's instructions, on its own branch.
#[test]
fn a_task_starts_one_worker_with_its_instructions() {
    let mut factory = Factory::in_memory();
    let task = start(&mut factory, "Fix the flaky test\nIt fails one run in ten.");
    assert_eq!(status(&factory, &task), Status::Queued);
    assert_eq!(factory.task(&task).unwrap().title, "Fix the flaky test");

    let effects = factory.step(1, &BTreeMap::new());
    let [Effect::StartWorker {
        task: started,
        project: in_project,
        agent: with,
        branch,
        worktree: None,
        prompt,
    }] = effects.as_slice()
    else {
        panic!("{effects:?}");
    };
    assert_eq!((started, *in_project, with), (&task, project(), &agent()));
    assert_eq!(branch, &format!("factory/{task}"));
    assert!(prompt.contains("It fails one run in ten."), "{prompt}");
    assert!(prompt.contains("report outcome finished"), "{prompt}");
    assert_eq!(status(&factory, &task), Status::Working);
    assert!(factory.step(2, &BTreeMap::new()).is_empty(), "only once");
}

/// The same start asked twice (a double click, a retried request) is one
/// Task.
#[test]
fn a_repeated_start_is_one_task() {
    let mut factory = Factory::in_memory();
    let ask = || Command::Start {
        project: project(),
        instructions: "Fix it".into(),
        agent: agent(),
        request: Some("click-1".into()),
        workflow: None,
        pr: None,
    };
    let first = factory.command(ask(), 0).unwrap();
    let again = factory.command(ask(), 1).unwrap();
    assert_eq!(first, again);
    assert_eq!(factory.board().tasks.len(), 1);
}

#[test]
fn empty_instructions_start_nothing() {
    let mut factory = Factory::in_memory();
    let refused = factory.command(
        Command::Start {
            project: project(),
            instructions: "  \n".into(),
            agent: agent(),
            request: None,
            workflow: None,
            pr: None,
        },
        0,
    );
    assert_eq!(refused.unwrap_err().code, RefusalCode::Invalid);
    assert!(factory.board().tasks.is_empty());
}

/// Finished completes the Task with its result and tells the operator;
/// the same report again changes nothing and tells no one.
#[test]
fn a_finished_report_completes_the_task_once() {
    let mut factory = Factory::in_memory();
    let task = working(&mut factory);
    report(
        &mut factory,
        Outcome::Finished {
            summary: "Fixed the race in setup".into(),
        },
    )
    .unwrap();
    let done = factory.task(&task).unwrap();
    assert_eq!(done.status, Status::Completed);
    assert_eq!(done.result.as_deref(), Some("Fixed the race in setup"));
    assert_eq!(
        notified(&factory.step(11, &seen(false, 1))),
        [Status::Completed]
    );

    let revision = factory.revision();
    let again = report(
        &mut factory,
        Outcome::Finished {
            summary: "Fixed the race in setup".into(),
        },
    );
    assert_eq!(again, Ok(Answer::Changed(false)));
    assert_eq!(factory.revision(), revision);
    assert!(factory.step(12, &seen(false, 1)).is_empty());
}

#[test]
fn an_unable_report_needs_the_operator_with_what_was_tried() {
    let mut factory = Factory::in_memory();
    let task = working(&mut factory);
    report(
        &mut factory,
        Outcome::Unable {
            tried: "Bisected to the CI image; cannot reproduce locally".into(),
        },
    )
    .unwrap();
    let stuck = factory.task(&task).unwrap();
    assert_eq!(stuck.status, Status::NeedsHuman);
    assert!(stuck
        .reason
        .as_deref()
        .is_some_and(|why| why.contains("cannot reproduce")));
    assert_eq!(
        notified(&factory.step(11, &seen(false, 1))),
        [Status::NeedsHuman]
    );
}

/// A report from a Thread that is no Task's Worker is refused.
#[test]
fn a_report_from_no_worker_is_refused() {
    let mut factory = Factory::in_memory();
    let refused = report(
        &mut factory,
        Outcome::Finished {
            summary: "done".into(),
        },
    );
    assert_eq!(refused.unwrap_err().code, RefusalCode::NotAWorker);
}

/// The at-rest rule: a Worker whose turn ended without a report is
/// prompted once; at rest again with still no report, its Task needs the
/// operator.
#[test]
fn a_worker_at_rest_without_a_report_is_prompted_once_then_needs_the_operator() {
    let mut factory = Factory::in_memory();
    let task = working(&mut factory);
    let effects = factory.step(4, &seen(false, 1));
    assert_eq!(prompts(&effects).len(), 1, "{effects:?}");
    assert!(prompts(&effects)[0].contains("report outcome"));
    assert!(factory.step(5, &seen(false, 1)).is_empty(), "prompted once");
    assert_eq!(status(&factory, &task), Status::Working);

    assert!(factory.step(6, &seen(true, 1)).is_empty());
    let effects = factory.step(7, &seen(false, 2));
    assert!(prompts(&effects).is_empty(), "never twice: {effects:?}");
    assert_eq!(notified(&effects), [Status::NeedsHuman]);
    assert_eq!(status(&factory, &task), Status::NeedsHuman);
}

/// A prompt the Worker never takes up still ends in the operator's hands.
#[test]
fn a_prompted_worker_that_never_works_again_needs_the_operator_in_time() {
    let mut factory = Factory::in_memory();
    let task = working(&mut factory);
    factory.step(4, &seen(false, 1));
    assert!(factory
        .step(4 + NUDGE_GRACE_MS - 1, &seen(false, 1))
        .is_empty());
    assert_eq!(
        notified(&factory.step(4 + NUDGE_GRACE_MS, &seen(false, 1))),
        [Status::NeedsHuman]
    );
    assert_eq!(status(&factory, &task), Status::NeedsHuman);
}

/// A Worker whose turn ended with a background task or Subagent still
/// running is busy, not at rest: never prompted.
#[test]
fn a_busy_worker_is_never_prompted() {
    let mut factory = Factory::in_memory();
    working(&mut factory);
    for at in 4..20 {
        assert!(factory.step(at, &seen(true, at as u64)).is_empty());
    }
}

/// A Worker that reported is never prompted, at rest or not.
#[test]
fn a_worker_that_reported_is_never_prompted() {
    let mut factory = Factory::in_memory();
    working(&mut factory);
    report(
        &mut factory,
        Outcome::Finished {
            summary: "done".into(),
        },
    )
    .unwrap();
    factory.step(11, &seen(false, 1));
    for at in 12..20 {
        assert!(prompts(&factory.step(at, &seen(false, at as u64))).is_empty());
    }
}

/// A Worker whose first turn has not ended is not at rest: its first
/// prompt may still be on its way.
#[test]
fn a_worker_whose_first_turn_has_not_ended_is_left_alone() {
    let mut factory = Factory::in_memory();
    let task = start(&mut factory, "Fix it");
    factory.step(1, &BTreeMap::new());
    factory
        .command(
            Command::WorkerStarted {
                task,
                thread: WORKER,
                worktree: None,
            },
            2,
        )
        .unwrap();
    assert!(factory.step(3, &seen(false, 0)).is_empty());
}

/// Stop interrupts the Worker and stops the Task; Resume prompts it to
/// carry on. Each again changes nothing.
#[test]
fn stop_interrupts_and_resume_continues() {
    let mut factory = Factory::in_memory();
    let task = working(&mut factory);
    let stop = || Command::Stop { task: task.clone() };
    factory.command(stop(), 4).unwrap();
    assert_eq!(status(&factory, &task), Status::Stopped);
    let effects = factory.step(5, &seen(true, 0));
    assert!(
        effects
            .iter()
            .any(|effect| *effect == Effect::Interrupt { thread: WORKER }),
        "{effects:?}"
    );
    assert_eq!(notified(&effects), [Status::Stopped]);
    assert_eq!(factory.command(stop(), 6), Ok(Answer::Changed(false)));
    assert!(
        factory.step(7, &seen(false, 1)).is_empty(),
        "a stopped Worker at rest is left alone"
    );

    let resume = || Command::Resume { task: task.clone() };
    factory.command(resume(), 8).unwrap();
    assert_eq!(prompts(&factory.step(9, &seen(false, 1))).len(), 1);
    assert_eq!(status(&factory, &task), Status::Working);
    assert_eq!(factory.command(resume(), 10), Ok(Answer::Changed(false)));
    assert!(
        factory.step(11, &seen(false, 1)).is_empty(),
        "the resumed turn has not ended"
    );
    assert_eq!(
        prompts(&factory.step(12, &seen(false, 2))).len(),
        1,
        "asked to report"
    );
}

/// Delete is for a Task that is Queued, Stopped or Completed; it hands
/// back the Worker to delete and the worktree to remove.
#[test]
fn only_a_queued_stopped_or_completed_task_is_deleted() {
    let mut factory = Factory::in_memory();
    let task = working(&mut factory);
    let delete = || Command::Delete { task: task.clone() };
    assert_eq!(
        factory.command(delete(), 4).unwrap_err().code,
        RefusalCode::NotNow
    );
    factory
        .command(Command::Stop { task: task.clone() }, 5)
        .unwrap();
    factory.step(6, &seen(false, 1));
    factory.command(delete(), 7).unwrap();
    assert!(factory.task(&task).is_none());
    let effects = factory.step(8, &BTreeMap::new());
    assert_eq!(
        effects,
        vec![
            Effect::DeleteThread { thread: WORKER },
            Effect::RemoveWorktree {
                path: "/repo/.worktrees/factory/t-1".into()
            }
        ]
    );
    assert_eq!(
        factory.command(delete(), 9).unwrap_err().code,
        RefusalCode::UnknownTask
    );

    let queued = start(&mut factory, "Another");
    factory
        .command(Command::Delete { task: queued }, 10)
        .unwrap();
    assert!(
        factory.step(11, &BTreeMap::new()).is_empty(),
        "nothing started for it"
    );
}

/// What the operator says to a Worker is recorded on its Task, nothing
/// more: a Task that needs them still does, and a waiting question still
/// waits for its answer. Resume and Answer are the commands for that.
#[test]
fn what_the_operator_says_is_recorded_and_changes_nothing_else() {
    let mut factory = Factory::in_memory();
    let task = working(&mut factory);
    factory
        .command(
            Command::Ask {
                worker: WORKER,
                text: "Which token?".into(),
            },
            10,
        )
        .unwrap();
    factory
        .command(
            Command::OperatorSaid {
                worker: WORKER,
                text: "Use the staging token".into(),
            },
            11,
        )
        .unwrap();
    let effects = factory.step(12, &seen(false, 1));
    assert!(prompts(&effects).is_empty(), "{effects:?}");
    let task = factory.task(&task).unwrap();
    assert_eq!(task.status, Status::NeedsHuman);
    assert_eq!(
        task.oversight.question.as_ref().map(|q| q.text.as_str()),
        Some("Which token?"),
        "still waiting for its answer"
    );
    let last = task.activity.last().unwrap();
    assert_eq!(
        (last.by, last.text.as_str()),
        (Actor::Operator, "Use the staging token")
    );
}

/// A Worker that could not be started stops its Task with why.
#[test]
fn a_worker_that_cannot_start_stops_the_task() {
    let mut factory = Factory::in_memory();
    let task = start(&mut factory, "Fix it");
    factory.step(1, &BTreeMap::new());
    factory
        .command(
            Command::WorkerFailed {
                task: task.clone(),
                why: "branch factory/t-1 exists".into(),
            },
            2,
        )
        .unwrap();
    assert_eq!(status(&factory, &task), Status::Stopped);
    assert_eq!(
        notified(&factory.step(3, &BTreeMap::new())),
        [Status::Stopped]
    );
}

/// Persist, then act: a change is on disk before the effect that depends
/// on it is handed out, and a Factory that cannot write changes nothing.
#[test]
fn a_change_is_written_before_its_effects() {
    let dir = std::env::temp_dir().join(format!("factory-persist-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.clone();
    let mut factory = Factory::open(&path);
    let task = start(&mut factory, "Fix it");
    let effects = factory.step(1, &BTreeMap::new());
    assert!(matches!(effects.as_slice(), [Effect::StartWorker { .. }]));
    let reread = Factory::open(&path);
    assert_eq!(status(&reread, &task), Status::Working);
    let next = start(&mut Factory::open(&path), "Another");
    assert_ne!(next, task, "ids are never given twice");

    // Where its part is written cannot be.
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(DIR_NAME), b"not a directory").unwrap();
    let refused = factory.command(Command::Stop { task: task.clone() }, 2);
    assert_eq!(refused.unwrap_err().code, RefusalCode::Failed);
    assert_eq!(status(&factory, &task), Status::Working);
    assert!(factory.step(3, &seen(true, 0)).is_empty());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Only the Tasks that need the operator are counted for the sidebar.
#[test]
fn the_board_counts_what_needs_the_operator() {
    let mut factory = Factory::in_memory();
    working(&mut factory);
    start(&mut factory, "Queued one");
    assert_eq!(factory.board().needs_you(project()), 0);
    report(
        &mut factory,
        Outcome::Unable {
            tried: "no access".into(),
        },
    )
    .unwrap();
    assert_eq!(factory.board().needs_you(project()), 1);
    assert_eq!(factory.board().needs_you(project_id(2)), 0);
}

mod recovery;
mod workflows;

const OTHER: ThreadId = ThreadId::new(41);

fn both_at_rest(turns: u64) -> BTreeMap<ThreadId, Seen> {
    [WORKER, OTHER]
        .into_iter()
        .map(|thread| {
            (
                thread,
                Seen {
                    busy: false,
                    turns,
                    decision: None,
                    lost: false,
                    closed: false,
                },
            )
        })
        .collect()
}

fn prompted(effects: &[Effect], to: ThreadId) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Prompt { thread, text } if *thread == to => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Spec: the limit bounds Tasks in Working, however they get there. A
/// resumed or answered Task waits its turn in Queued, keeping its Worker,
/// and continues once a place is free.
#[test]
fn a_task_back_to_work_waits_its_turn_under_the_limit() {
    let mut factory = Factory::in_memory();
    factory.set_limit(1);
    let first = working(&mut factory);
    factory
        .command(
            Command::Stop {
                task: first.clone(),
            },
            4,
        )
        .unwrap();
    factory.step(5, &both_at_rest(1));
    let second = start(&mut factory, "Another");
    factory.step(6, &both_at_rest(1));
    factory
        .command(
            Command::WorkerStarted {
                task: second.clone(),
                thread: OTHER,
                worktree: None,
            },
            7,
        )
        .unwrap();
    factory.step(7, &both_at_rest(1));

    factory
        .command(
            Command::Resume {
                task: first.clone(),
            },
            8,
        )
        .unwrap();
    let effects = factory.step(9, &both_at_rest(1));
    assert_eq!(status(&factory, &first), Status::Queued, "no place yet");
    assert!(prompted(&effects, WORKER).is_empty(), "{effects:?}");
    assert_eq!(factory.task(&first).unwrap().worker, Some(WORKER));

    factory
        .command(
            Command::Ask {
                worker: OTHER,
                text: "Which branch?".into(),
            },
            10,
        )
        .unwrap();
    let effects = factory.step(11, &both_at_rest(1));
    assert_eq!(status(&factory, &first), Status::Working, "released");
    assert_eq!(prompted(&effects, WORKER).len(), 1, "{effects:?}");
    assert!(prompted(&effects, WORKER)[0].contains("resumed"));

    factory
        .command(
            Command::AnswerQuestion {
                task: second.clone(),
                text: "main".into(),
                question: None,
            },
            12,
        )
        .unwrap();
    let effects = factory.step(13, &both_at_rest(1));
    assert_eq!(
        status(&factory, &second),
        Status::Queued,
        "answered, it waits"
    );
    assert!(prompted(&effects, OTHER).is_empty(), "{effects:?}");

    factory
        .command(
            Command::Report {
                worker: WORKER,
                outcome: Outcome::Finished {
                    summary: "Done".into(),
                },
            },
            14,
        )
        .unwrap();
    let effects = factory.step(15, &both_at_rest(1));
    assert_eq!(status(&factory, &second), Status::Working);
    let told = prompted(&effects, OTHER);
    assert_eq!(told.len(), 1, "{effects:?}");
    assert!(told[0].contains("Their answer: main"), "{told:?}");
}

/// B9: one rule for how many Tasks may work at once, the Factory's.
#[test]
fn the_limit_is_kept_within_its_range() {
    let mut factory = Factory::in_memory();
    factory.set_limit(0);
    assert_eq!(factory.limit(), 1);
    factory.set_limit(50);
    assert_eq!(factory.limit(), *LIMIT_RANGE.end());
    assert_eq!(clamp_limit(5), 5);
}
