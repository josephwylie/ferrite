//! Recovery and the Archive (#226): a Worker whose Session was lost
//! mid-Step is revived once and told it was interrupted; a second loss
//! stops the Task; Completed Tasks leave the board after 7 days or with
//! Clear, on a passed-in clock.

use super::*;

/// The Worker as the Server sees it after its Session was lost: no
/// Session, at rest, its turn count starting again.
fn lost(turns: u64) -> BTreeMap<ThreadId, Seen> {
    BTreeMap::from([(
        WORKER,
        Seen {
            busy: false,
            turns,
            decision: None,
            lost: true,
        },
    )])
}

fn activity(factory: &Factory, task: &str) -> Vec<String> {
    factory
        .task(task)
        .unwrap()
        .activity
        .iter()
        .map(|entry| entry.text.clone())
        .collect()
}

/// AC: a Worker whose Session ends mid-Step, with no restart, is revived
/// once: the same Thread is told it was interrupted, to continue and to
/// report, and the activity says the Session ended (not that the agent
/// could not finish). The Task is still Working.
#[test]
fn a_session_lost_mid_step_revives_the_same_worker_once() {
    let mut factory = Factory::in_memory();
    let task = working(&mut factory);
    let effects = factory.step(4, &lost(0));
    let [Effect::Prompt { thread, text }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(*thread, WORKER, "the same Thread");
    assert!(text.contains("interrupted"), "{text}");
    assert!(text.contains("report outcome finished"), "{text}");
    assert_eq!(status(&factory, &task), Status::Working);
    let said = activity(&factory, &task);
    assert!(
        said.last().unwrap().contains("session ended unexpectedly"),
        "{said:?}"
    );
    assert!(!said.iter().any(|line| line.contains("Unable")), "{said:?}");

    // Revived: its new Session works, and nothing more is sent.
    assert!(factory.step(5, &seen(true, 0)).is_empty());
}

/// AC: lost a second time in the same run, the Task stops with the
/// reason and the operator is told; nothing is started or re-run. The
/// operator's Resume prompts the same Worker on, and a loss after it is
/// revived once more.
#[test]
fn a_second_loss_stops_the_task_and_resume_continues_it() {
    let mut factory = Factory::in_memory();
    let task = working(&mut factory);
    factory.step(4, &lost(0));
    factory.step(5, &seen(true, 0));
    let effects = factory.step(6, &lost(0));
    assert_eq!(notified(&effects), vec![Status::Stopped], "{effects:?}");
    assert!(prompts(&effects).is_empty(), "{effects:?}");
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::StartWorker { .. })),
        "never re-run from its start: {effects:?}"
    );
    let stopped = factory.task(&task).unwrap();
    assert_eq!(stopped.status, Status::Stopped);
    let reason = stopped.reason.clone().unwrap();
    assert!(reason.contains("ended unexpectedly"), "{reason}");
    assert!(reason.contains("revived once"), "{reason}");
    assert!(factory.step(7, &lost(0)).is_empty(), "stopped stays stopped");

    factory
        .command(Command::Resume { task: task.clone() }, 8)
        .unwrap();
    let effects = factory.step(9, &lost(0));
    let [Effect::Prompt { thread, text }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(*thread, WORKER);
    assert!(text.contains("resumed"), "{text}");
    factory.step(10, &seen(true, 0));
    let effects = factory.step(11, &lost(0));
    assert!(prompts(&effects)[0].contains("interrupted"), "{effects:?}");
    assert_eq!(status(&factory, &task), Status::Working);
}

const OTHER: ThreadId = ThreadId::new(41);

/// A scratch directory's Factory file, emptied.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("factory-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("factory.json")
}

fn worker_up(factory: &mut Factory, task: &str, thread: ThreadId) {
    factory
        .command(
            Command::WorkerStarted {
                task: task.into(),
                thread,
                worktree: None,
            },
            2,
        )
        .unwrap();
}

fn each_lost(threads: &[ThreadId]) -> BTreeMap<ThreadId, Seen> {
    threads
        .iter()
        .map(|thread| {
            (
                *thread,
                Seen {
                    busy: false,
                    turns: 0,
                    decision: None,
                    lost: true,
                },
            )
        })
        .collect()
}

/// AC (case 16): after a restart the working Task's Worker is revived and
/// told Ferrite restarted; a Task waiting on the operator's answer still
/// waits with nothing started, and its answer prompts its Worker as
/// ever; a Task whose Worker had not started yet stops rather than
/// starting its Step again.
#[test]
fn a_restart_revives_working_workers_and_leaves_waiting_tasks_waiting() {
    let path = scratch("restart");
    let mut factory = Factory::load(&path).unwrap();
    let working = start(&mut factory, "Fix the flaky test");
    let asking = start(&mut factory, "Rename the flag");
    factory.step(1, &BTreeMap::new());
    worker_up(&mut factory, &working, WORKER);
    worker_up(&mut factory, &asking, OTHER);
    factory
        .command(
            Command::Ask {
                worker: OTHER,
                text: "Keep the old name as an alias?".into(),
            },
            3,
        )
        .unwrap();
    let unstarted = start(&mut factory, "Tidy the README");
    let effects = factory.step(4, &BTreeMap::new());
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::StartWorker { .. })),
        "{effects:?}"
    );
    // The Server stops before it starts that Worker.

    let mut factory = Factory::load(&path).unwrap();
    let effects = factory.step(10, &each_lost(&[WORKER, OTHER]));
    let told: Vec<(ThreadId, &str)> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Prompt { thread, text } => Some((*thread, text.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(told.len(), 1, "{effects:?}");
    assert_eq!(told[0].0, WORKER);
    assert!(told[0].1.contains("Ferrite restarted"), "{}", told[0].1);
    assert!(
        activity(&factory, &working)
            .last()
            .unwrap()
            .contains("Ferrite restarted"),
        "{:?}",
        activity(&factory, &working)
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::StartWorker { .. })),
        "{effects:?}"
    );

    let waiting = factory.task(&asking).unwrap();
    assert_eq!(waiting.status, Status::NeedsHuman);
    assert_eq!(
        waiting.oversight.question.as_ref().unwrap().text,
        "Keep the old name as an alias?"
    );
    let stopped = factory.task(&unstarted).unwrap();
    assert_eq!(stopped.status, Status::Stopped, "{stopped:?}");
    assert!(
        stopped.reason.as_deref().unwrap().contains("restarted"),
        "{stopped:?}"
    );

    factory
        .command(
            Command::AnswerQuestion {
                task: asking.clone(),
                text: "Yes, for one release".into(),
                question: None,
            },
            11,
        )
        .unwrap();
    let effects = factory.step(12, &each_lost(&[OTHER]));
    let [Effect::Prompt { thread, text }] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(*thread, OTHER);
    assert!(text.contains("Yes, for one release"), "{text}");
    assert!(!text.contains("interrupted"), "{text}");
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

/// AC: a Factory store that cannot be read is said plainly on the board
/// and in every refusal, starts nothing, and is never written over.
#[test]
fn an_unreadable_store_is_reported_and_never_started_afresh() {
    let path = scratch("unreadable");
    std::fs::write(&path, b"{\"version\": 1, \"tasks\": [tru").unwrap();
    let mut factory = Factory::open(&path);
    let trouble = factory.board().trouble.expect("said on the board");
    assert!(trouble.contains("could not be read"), "{trouble}");
    assert!(trouble.contains(&path.display().to_string()), "{trouble}");

    let refused = factory
        .command(
            Command::Start {
                project: project(),
                instructions: "Fix the flaky test".into(),
                agent: agent(),
                request: None,
                workflow: None,
                pr: None,
            },
            1,
        )
        .unwrap_err();
    assert_eq!(refused.code, RefusalCode::Failed);
    assert!(refused.message.contains("could not be read"), "{refused}");
    assert!(factory.step(2, &BTreeMap::new()).is_empty());
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"{\"version\": 1, \"tasks\": [tru",
        "never written over"
    );

    // A missing file is a fresh Factory, and a good one is itself.
    let fresh = scratch("fresh");
    assert_eq!(Factory::open(&fresh).board().trouble, None);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    std::fs::remove_dir_all(fresh.parent().unwrap()).unwrap();
}
