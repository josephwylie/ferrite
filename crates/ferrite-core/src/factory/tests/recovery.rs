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
    assert!(
        factory.step(7, &lost(0)).is_empty(),
        "stopped stays stopped"
    );

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

/// A scratch Store directory for a Factory, emptied.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("factory-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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
    let mut factory = Factory::open(&path);
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

    let mut factory = Factory::open(&path);
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
    std::fs::remove_dir_all(&path).unwrap();
}

fn start_in(
    factory: &mut Factory,
    project: ProjectId,
    instructions: &str,
) -> Result<Answer, Refusal> {
    factory.command(
        Command::Start {
            project,
            instructions: instructions.into(),
            agent: agent(),
            request: None,
            workflow: None,
            pr: None,
        },
        1,
    )
}

/// AC (#226): a Project's part of the store that cannot be read is said
/// plainly on its board and in every refusal, starts nothing and is never
/// written over; every other Project's Factory runs and is saved.
#[test]
fn an_unreadable_store_is_reported_and_never_started_afresh() {
    let path = scratch("unreadable");
    let parts = path.join(DIR_NAME);
    std::fs::create_dir_all(&parts).unwrap();
    let broken = parts.join("1.json");
    std::fs::write(&broken, b"{\"version\": 1, \"tasks\": [tru").unwrap();
    let mut factory = Factory::open(&path);
    let board = factory.board();
    let trouble = board.trouble_of(project()).expect("said on the board");
    assert!(trouble.contains("could not be read"), "{trouble}");
    assert!(trouble.contains(&broken.display().to_string()), "{trouble}");
    assert_eq!(board.trouble_of(project_id(2)), None, "only that Project's");

    let refused = start_in(&mut factory, project(), "Fix the flaky test").unwrap_err();
    assert_eq!(refused.code, RefusalCode::Failed);
    assert!(refused.message.contains("could not be read"), "{refused}");

    let Ok(Answer::Task(other)) = start_in(&mut factory, project_id(2), "Another project") else {
        panic!("another Project's Factory runs");
    };
    let effects = factory.step(2, &BTreeMap::new());
    assert!(
        matches!(effects.as_slice(), [Effect::StartWorker { .. }]),
        "{effects:?}"
    );
    assert_eq!(
        std::fs::read(&broken).unwrap(),
        b"{\"version\": 1, \"tasks\": [tru",
        "never written over"
    );
    let reread = Factory::open(&path);
    assert_eq!(status(&reread, &other), Status::Working, "and saved");

    // A missing store is a fresh Factory.
    let fresh = scratch("fresh");
    assert_eq!(Factory::open(&fresh).board().trouble_of(project()), None);
    std::fs::remove_dir_all(&path).unwrap();
    std::fs::remove_dir_all(&fresh).unwrap();
}

/// AC (#226): one Project's part that cannot be written holds back only
/// that Project: another's commands and steps go on, and the held change
/// is written, and acted on, once it can be.
#[cfg(unix)]
#[test]
fn a_project_that_cannot_be_saved_holds_back_only_itself() {
    use std::os::unix::fs::PermissionsExt;
    let path = scratch("unwritable");
    let mut factory = Factory::open(&path);
    let stuck = match start_in(&mut factory, project(), "Stuck") {
        Ok(Answer::Task(task)) => task,
        other => panic!("{other:?}"),
    };
    let Ok(Answer::Task(fine)) = start_in(&mut factory, project_id(2), "Fine") else {
        panic!();
    };
    factory.step(1, &BTreeMap::new());
    // Project 1's file is replaced by a directory: it cannot be written.
    let one = path.join(DIR_NAME).join("1.json");
    std::fs::remove_file(&one).unwrap();
    std::fs::create_dir(&one).unwrap();
    std::fs::set_permissions(&one, std::fs::Permissions::from_mode(0o500)).unwrap();

    let refused = factory
        .command(
            Command::Stop {
                task: stuck.clone(),
            },
            2,
        )
        .unwrap_err();
    assert_eq!(refused.code, RefusalCode::Failed);
    assert_eq!(
        factory.command(Command::Stop { task: fine.clone() }, 2),
        Ok(Answer::Changed(true)),
        "another Project is not held back"
    );
    assert!(factory.board().trouble_of(project()).is_some());
    assert_eq!(factory.board().trouble_of(project_id(2)), None);
    let effects = factory.step(3, &BTreeMap::new());
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Notify { task, .. } if *task == fine)),
        "{effects:?}"
    );
    std::fs::set_permissions(&one, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::remove_dir(&one).unwrap();
    std::fs::remove_dir_all(&path).unwrap();
}

/// B2: a step whose change cannot be written keeps it and its effects, and
/// writes and hands them out on a later pass: what the restart caught is
/// not forgotten because one save failed.
#[cfg(unix)]
#[test]
fn a_step_that_cannot_be_saved_is_kept_and_tried_again() {
    let path = scratch("retry");
    let mut factory = Factory::open(&path);
    let task = start(&mut factory, "Fix the flaky test");
    let effects = factory.step(1, &BTreeMap::new());
    assert!(matches!(effects.as_slice(), [Effect::StartWorker { .. }]));
    // The Server stops before the Worker starts; the store will not write.
    let parts = path.join(DIR_NAME);
    let one = parts.join("1.json");
    let saved = std::fs::read(&one).unwrap();
    let mut factory = Factory::open(&path);
    std::fs::remove_file(&one).unwrap();
    std::fs::create_dir(&one).unwrap();
    let effects = factory.step(2, &BTreeMap::new());
    assert!(
        effects.is_empty(),
        "nothing acts before it is written: {effects:?}"
    );
    assert!(factory.board().trouble_of(project()).is_some());

    std::fs::remove_dir(&one).unwrap();
    std::fs::write(&one, saved).unwrap();
    let effects = factory.step(3, &BTreeMap::new());
    assert_eq!(notified(&effects), [Status::Stopped], "{effects:?}");
    assert_eq!(status(&factory, &task), Status::Stopped);
    assert_eq!(factory.board().trouble_of(project()), None);
    assert_eq!(status(&Factory::open(&path), &task), Status::Stopped);
    std::fs::remove_dir_all(&path).unwrap();
}

const DAY: i64 = 24 * 3_600_000;

fn complete(factory: &mut Factory, task: &str, at_ms: i64) {
    let worker = factory.task(task).unwrap().worker.unwrap();
    factory
        .command(
            Command::Report {
                worker,
                outcome: Outcome::Finished {
                    summary: "Done".into(),
                },
            },
            at_ms,
        )
        .unwrap();
    factory.step(at_ms, &BTreeMap::new());
}

fn archived(factory: &Factory, query: &str) -> Vec<TaskId> {
    factory
        .board()
        .archive(project(), query)
        .map(|task| task.id.clone())
        .collect()
}

fn on_board(factory: &Factory) -> Vec<TaskId> {
    factory
        .board()
        .on_board(project())
        .map(|task| task.id.clone())
        .collect()
}

/// AC: a Completed Task leaves the board for the Archive 7 days after it
/// completed, on the passed-in clock, and not before; the Archive is
/// searched by title and instructions.
#[test]
fn completed_tasks_move_to_the_archive_after_seven_days() {
    let mut factory = Factory::in_memory();
    let task = working(&mut factory);
    complete(&mut factory, &task, 100);
    assert_eq!(on_board(&factory), vec![task.clone()]);
    factory.step(100 + 7 * DAY - 1, &BTreeMap::new());
    assert_eq!(on_board(&factory), vec![task.clone()], "not yet");
    assert!(archived(&factory, "").is_empty());

    factory.step(100 + 7 * DAY, &BTreeMap::new());
    assert!(on_board(&factory).is_empty(), "gone from the board");
    assert_eq!(archived(&factory, ""), vec![task.clone()]);
    assert_eq!(factory.task(&task).unwrap().status, Status::Completed);
    assert_eq!(archived(&factory, "FLAKY"), vec![task.clone()], "title");
    assert!(archived(&factory, "deploy").is_empty());
}

/// AC: Clear moves the Project's Completed Tasks to the Archive at once,
/// and nothing else; a second Clear changes nothing. The Archive searches
/// instructions too.
#[test]
fn clear_archives_a_projects_completed_tasks_at_once() {
    let mut factory = Factory::in_memory();
    factory.set_limit(5);
    let done = working(&mut factory);
    complete(&mut factory, &done, 10);
    let open = start(
        &mut factory,
        "Rename the flag\nKeep the old one as an alias",
    );
    let elsewhere = factory
        .command(
            Command::Start {
                project: project_id(2),
                instructions: "Another project's".into(),
                agent: agent(),
                request: None,
                workflow: None,
                pr: None,
            },
            11,
        )
        .unwrap();
    let cleared = factory.command(Command::Clear { project: project() }, 12);
    assert_eq!(cleared, Ok(Answer::Changed(true)));
    assert_eq!(archived(&factory, ""), vec![done.clone()]);
    assert_eq!(on_board(&factory), vec![open.clone()]);
    assert_eq!(
        factory.command(Command::Clear { project: project() }, 13),
        Ok(Answer::Changed(false))
    );
    let Answer::Task(elsewhere) = elsewhere else {
        panic!()
    };
    assert!(factory
        .task(&elsewhere)
        .unwrap()
        .keeping
        .archived_ms
        .is_none());

    let mut factory = Factory::in_memory();
    let task = start(
        &mut factory,
        "Tidy\nThe README mentions the old deploy script",
    );
    factory.step(1, &BTreeMap::new());
    worker_up(&mut factory, &task, WORKER);
    complete(&mut factory, &task, 20);
    factory
        .command(Command::Clear { project: project() }, 21)
        .unwrap();
    assert_eq!(archived(&factory, "deploy script"), vec![task.clone()]);
}

/// AC: a completed Task's Workers are parked once at rest (never while
/// the reporting turn still runs) and kept; only deleting the Task
/// deletes them.
#[test]
fn workers_are_parked_on_completion_and_deleted_only_with_the_task() {
    let mut factory = Factory::in_memory();
    let task = working(&mut factory);
    report(
        &mut factory,
        Outcome::Finished {
            summary: "Done".into(),
        },
    )
    .unwrap();
    let parks = |effects: &[Effect]| {
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::Park { .. }))
            .count()
    };
    let effects = factory.step(20, &seen(true, 0));
    assert_eq!(parks(&effects), 0, "its turn still runs: {effects:?}");
    let effects = factory.step(21, &seen(false, 1));
    assert_eq!(
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::Park { thread } if *thread == WORKER))
            .count(),
        1,
        "{effects:?}"
    );
    assert_eq!(parks(&factory.step(22, &seen(false, 1))), 0, "once");
    assert_eq!(factory.task(&task).unwrap().worker, Some(WORKER), "kept");

    let effects = {
        factory
            .command(Command::Delete { task: task.clone() }, 23)
            .unwrap();
        factory.step(24, &BTreeMap::new())
    };
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::DeleteThread { thread } if *thread == WORKER)),
        "{effects:?}"
    );
    assert_eq!(parks(&effects), 0, "{effects:?}");
}

/// ADR 0025 (B1): a pass that changes nothing writes nothing and moves no
/// revision, however much the Factory holds; the Archive is kept off the
/// board a window is told of each change, and is sent only when it
/// changes.
#[test]
fn an_idle_pass_writes_nothing_and_the_archive_is_sent_only_when_it_changes() {
    let path = scratch("idle");
    let mut factory = Factory::open(&path);
    let task = working(&mut factory);
    complete(&mut factory, &task, 100);
    factory
        .command(Command::Clear { project: project() }, 200)
        .unwrap();
    let busy = start(&mut factory, "Still going");
    factory.step(201, &BTreeMap::new());
    factory
        .command(
            Command::WorkerStarted {
                task: busy.clone(),
                thread: OTHER,
                worktree: None,
            },
            202,
        )
        .unwrap();
    factory.step(203, &BTreeMap::new());

    let board = factory.changes();
    assert!(
        board.archive.is_none(),
        "the Archive is not sent with a change"
    );
    assert!(
        board.tasks.iter().all(|each| each.id != task),
        "{:?}",
        board.tasks
    );
    assert_eq!(
        factory
            .board()
            .archive
            .unwrap()
            .iter()
            .map(|each| each.id.clone())
            .collect::<Vec<_>>(),
        [task.clone()]
    );

    // Idle: the Worker works and nothing is due.
    let part = path.join(DIR_NAME).join("1.json");
    std::fs::remove_file(&part).unwrap();
    let (revision, archive) = (factory.revision(), factory.archive_revision());
    let busy_seen = BTreeMap::from([(
        OTHER,
        Seen {
            busy: true,
            turns: 0,
            decision: None,
            lost: false,
        },
    )]);
    for at in 300..310 {
        assert!(factory.step(at, &busy_seen).is_empty());
    }
    assert_eq!(factory.revision(), revision, "no change, no broadcast");
    assert!(!part.exists(), "no change, no write");

    // A change to the board moves the revision, not the Archive's.
    factory
        .command(Command::Stop { task: busy.clone() }, 311)
        .unwrap();
    assert!(factory.revision() > revision);
    assert_eq!(factory.archive_revision(), archive);
    std::fs::remove_dir_all(&path).unwrap();
}

/// A Factory kept in one file by an earlier Ferrite is moved into each
/// Project's part, its archived Tasks into the Archive, and runs as it was.
#[test]
fn an_older_single_file_factory_is_moved_into_project_parts() {
    let path = scratch("migrate");
    let mut factory = Factory::open(&path);
    let kept = working(&mut factory);
    let Ok(Answer::Task(other)) = start_in(&mut factory, project_id(2), "Elsewhere") else {
        panic!();
    };
    let mut board = factory.board();
    board.tasks[0].keeping.archived_ms = Some(5);
    let legacy = serde_json::json!({
        "version": 1,
        "next": 9,
        "tasks": board.tasks,
        "next_workflow": 1,
        "workflows": [],
    });
    std::fs::remove_dir_all(path.join(DIR_NAME)).unwrap();
    std::fs::write(path.join(FILE_NAME), serde_json::to_vec(&legacy).unwrap()).unwrap();

    let mut moved = Factory::open(&path);
    assert_eq!(moved.board().trouble, None);
    assert!(!path.join(FILE_NAME).exists(), "set aside");
    assert!(path.join(DIR_NAME).join("2.json").exists());
    let archive = moved.board().archive.unwrap();
    assert_eq!(archive.len(), 1);
    assert_eq!(archive[0].id, kept);
    assert_eq!(status(&moved, &other), Status::Queued);
    let next = start(&mut moved, "New");
    assert_eq!(next, "t-9", "ids go on from the old file's");
    std::fs::remove_dir_all(&path).unwrap();
}
