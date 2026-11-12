//! Adversarial repros (interrogate pass on PR #238). Each test asserts
//! what the spec requires; a failure is the finding.

use super::*;

const HEAD: &str = "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn scratch_store(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("factory-interrogate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn start_in(factory: &mut Factory, project: ProjectId, instructions: &str) -> TaskId {
    match factory.command(
        Command::Start {
            project,
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

fn at(thread: ThreadId, busy: bool, turns: u64, decision: Option<&str>) -> (ThreadId, Seen) {
    (
        thread,
        Seen {
            busy,
            turns,
            decision: decision.map(str::to_string),
            lost: false,
            closed: false,
        },
    )
}

fn working_count(factory: &Factory) -> usize {
    factory
        .board()
        .tasks
        .iter()
        .filter(|task| task.status == Status::Working)
        .count()
}

/// A one-off Task in project 1, Completed by WORKER, its completion seen
/// by the step at `t`.
fn completed_one_off(factory: &mut Factory, t: i64) -> TaskId {
    let task = start_in(factory, project(), "Fix the flaky test");
    factory.step(1, &BTreeMap::new());
    factory
        .command(
            Command::WorkerStarted {
                task: task.clone(),
                thread: WORKER,
                worktree: None,
            },
            2,
        )
        .unwrap();
    factory
        .command(
            Command::Report {
                worker: WORKER,
                outcome: Outcome::Finished {
                    summary: "Fixed".into(),
                },
            },
            3,
        )
        .unwrap();
    factory.step(t, &BTreeMap::from([at(WORKER, false, 1, None)]));
    assert_eq!(status(factory, &task), Status::Completed);
    task
}

/// Attack: a failed write of the Archive, then a command that succeeds.
/// The step moves the Task into the Archive in memory and fails to write
/// it; a later command writes only the board and clears the "archive
/// unwritten" flag, so the Archive is never retried. After a restart the
/// Task is in neither file.
#[test]
fn interrogate_archive_write_failure_then_a_command_loses_the_task() {
    let store = scratch_store("archive-lost");
    let mut factory = Factory::open(&store);
    let t0 = 10_000;
    let task = completed_one_off(&mut factory, t0);

    // The Archive's file cannot be written (a transient disk failure).
    let blocker = store.join(DIR_NAME).join("1.archive.json");
    std::fs::create_dir_all(&blocker).unwrap();
    std::fs::write(blocker.join("keep"), b"x").unwrap();
    factory.step(t0 + ARCHIVE_AFTER_MS, &BTreeMap::new());
    println!(
        "after the failed archive write: troubles = {:?}",
        factory.board().troubles
    );

    // The operator starts another Task in the same Project: it is written.
    let other = start_in(&mut factory, project(), "Another task");
    println!(
        "started {other}; troubles now = {:?}",
        factory.board().troubles
    );

    // The disk recovers; the Factory runs on for a while.
    std::fs::remove_dir_all(&blocker).unwrap();
    for n in 1..5 {
        factory.step(t0 + ARCHIVE_AFTER_MS + n * 60_000, &BTreeMap::new());
    }
    println!(
        "in memory before restart: {task} archived = {}",
        factory.task(&task).is_some_and(Task::archived)
    );
    println!(
        "on disk: {:?}",
        std::fs::read_dir(store.join(DIR_NAME))
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect::<Vec<_>>()
    );

    let reopened = Factory::open(&store);
    println!(
        "after restart: {task} = {:?}",
        reopened.task(&task).map(|t| (t.status, t.archived()))
    );
    assert!(
        reopened.task(&task).is_some(),
        "the archived Task {task} must survive a restart, in the Archive"
    );
    let _ = std::fs::remove_dir_all(&store);
}

/// Attack: id reuse across Projects. One Project's part cannot be read;
/// its ids are not counted, so another Project gives them again. Once the
/// file is readable again two Tasks share one id.
#[test]
fn interrogate_an_unreadable_part_lets_another_project_reuse_its_ids() {
    let store = scratch_store("id-reuse");
    let one = project_id(1);
    let two = project_id(2);
    let mut factory = Factory::open(&store);
    let a = start_in(&mut factory, one, "In project one");
    let b = start_in(&mut factory, two, "In project two");
    let c = start_in(&mut factory, two, "In project two again");
    println!("before: {a} (p1), {b} {c} (p2)");
    drop(factory);

    let part2 = store.join(DIR_NAME).join("2.json");
    let good = std::fs::read(&part2).unwrap();
    std::fs::write(&part2, b"{ not json").unwrap();
    let mut factory = Factory::open(&store);
    println!(
        "project 2 trouble: {:?}",
        factory.board().troubles.get(&two).is_some()
    );
    let d = start_in(&mut factory, one, "In project one while two is unreadable");
    println!("new Task in project 1 while project 2 is unreadable: {d}");
    drop(factory);

    std::fs::write(&part2, good).unwrap();
    let factory = Factory::open(&store);
    let ids: Vec<(String, u64)> = factory
        .board()
        .tasks
        .iter()
        .map(|task| {
            (
                task.id.clone(),
                serde_json::to_value(task.project)
                    .unwrap()
                    .as_u64()
                    .unwrap(),
            )
        })
        .collect();
    println!("after the file is fixed: {ids:?}");
    assert!(
        d != b && d != c,
        "a Task id is never given twice: {d} was already project 2's"
    );
    let _ = std::fs::remove_dir_all(&store);
}

/// Attack: the concurrency limit. A Task in Needs human for a Provider
/// permission used to free its slot, and went straight back to Working
/// past the limit when the permission was answered. Its Worker's run goes
/// on the moment it is answered, so it keeps its place: the next Task
/// waits for it.
#[test]
fn interrogate_an_answered_permission_goes_past_the_limit() {
    let mut factory = Factory::in_memory();
    factory.set_limit(1);
    let first = start_in(&mut factory, project(), "First");
    factory.step(1, &BTreeMap::new());
    worker(&mut factory, &first, WORKER);
    factory.step(
        2,
        &BTreeMap::from([at(WORKER, true, 0, Some("Run cargo test"))]),
    );
    assert_eq!(status(&factory, &first), Status::NeedsHuman);

    let second = start_in(&mut factory, project(), "Second");
    factory.step(
        3,
        &BTreeMap::from([at(WORKER, true, 0, Some("Run cargo test"))]),
    );
    assert_eq!(
        status(&factory, &second),
        Status::Queued,
        "a permission waiting keeps its Task's place"
    );

    // The operator answers the permission in the first Task's Worker.
    factory.step(4, &BTreeMap::from([at(WORKER, true, 0, None)]));
    println!(
        "limit {} · {first} {:?} · {second} {:?}",
        factory.limit(),
        status(&factory, &first),
        status(&factory, &second)
    );
    assert_eq!(status(&factory, &first), Status::Working);
    assert!(
        working_count(&factory) <= factory.limit(),
        "{} Tasks Working with a limit of {}",
        working_count(&factory),
        factory.limit()
    );

    report_finished(&mut factory, WORKER);
    factory.step(5, &BTreeMap::from([at(WORKER, false, 1, None)]));
    assert_eq!(status(&factory, &second), Status::Working, "its turn came");
}

fn report_finished(factory: &mut Factory, thread: ThreadId) {
    factory
        .command(
            Command::Report {
                worker: thread,
                outcome: Outcome::Finished {
                    summary: "Done".into(),
                },
            },
            4,
        )
        .unwrap();
}

fn worker(factory: &mut Factory, task: &str, thread: ThreadId) {
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

/// Attack: "only `merged` may route to Completed". A merge Step's "If
/// unable to finish" route set to Completed passes validation, saves, and
/// completes the Task with nothing merged.
#[test]
fn interrogate_a_merge_steps_unable_route_completes_unmerged() {
    let mut factory = Factory::in_memory();
    let (id, steps) = saved(
        &mut factory,
        &[("Build", "Build it"), ("Merge", "Merge it")],
    );
    edit(
        &mut factory,
        &id,
        &steps[1],
        StepChange {
            by_merge_result: Some(true),
            on_unable: Some(Destination::Complete),
            ..Default::default()
        },
    );
    let problems = factory
        .workflow(&id)
        .unwrap()
        .draft
        .problems(&Catalog::default());
    println!("problems: {problems:?}");
    println!("save: {:?}", save(&mut factory, &id));
    let task = match factory.command(
        Command::Start {
            project: project(),
            instructions: "Ship it".into(),
            agent: agent(),
            request: None,
            workflow: Some(id.clone()),
            pr: Some("acme/widgets#12".into()),
        },
        6,
    ) {
        Ok(Answer::Task(task)) => task,
        other => panic!("the workflow should not have run: {other:?}"),
    };
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);
    finished(&mut factory, FIRST, "Built");
    started(&mut factory, &task, SECOND);
    let answer = factory.command(
        Command::Report {
            worker: SECOND,
            outcome: Outcome::Unable {
                tried: "gh is not logged in".into(),
            },
        },
        8,
    );
    factory.step(9, &BTreeMap::new());
    let done = factory.task(&task).unwrap();
    println!(
        "report unable: {answer:?} -> {:?}, merged = {:?}",
        done.status,
        done.delivery.pr.as_ref().map(|pr| pr.merged)
    );
    assert!(
        !problems.is_empty() || done.status != Status::Completed,
        "an unable merge Step completed the Task with nothing merged"
    );
}

/// Attack: Delete. Returning to a "Start fresh every time" Step parks its
/// old Worker and forgets it: it is in no list, so deleting the Task does
/// not delete it (and the archived Worker history omits it).
#[test]
fn interrogate_a_fresh_steps_replaced_worker_outlives_its_deleted_task() {
    let mut factory = Factory::in_memory();
    let (id, steps) = saved(
        &mut factory,
        &[("Build", "Build it"), ("Review", "Review it")],
    );
    edit(
        &mut factory,
        &id,
        &steps[1],
        StepChange {
            on_unable: Some(Destination::Step(steps[0].clone())),
            fresh: Some(true),
            ..Default::default()
        },
    );
    edit(
        &mut factory,
        &id,
        &steps[0],
        StepChange {
            on_finished: Some(Destination::Step(steps[1].clone())),
            ..Default::default()
        },
    );
    save(&mut factory, &id).unwrap();
    let task = run(&mut factory, &id, "Add the flag");
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);
    finished(&mut factory, FIRST, "Added it");
    started(&mut factory, &task, SECOND);
    unable(&mut factory, SECOND, "No test");
    factory.step(10, &BTreeMap::from([at(FIRST, false, 1, None)]));
    finished(&mut factory, FIRST, "Added the test");
    started(&mut factory, &task, THIRD);
    println!(
        "Workers the Task knows: {:?}",
        factory.task(&task).unwrap().every_worker()
    );
    factory
        .command(Command::Stop { task: task.clone() }, 11)
        .unwrap();
    factory.step(12, &BTreeMap::new());
    factory
        .command(Command::Delete { task: task.clone() }, 13)
        .unwrap();
    let effects = factory.step(14, &BTreeMap::new());
    let deleted: Vec<ThreadId> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::DeleteThread { thread } => Some(*thread),
            _ => None,
        })
        .collect();
    println!("deleted on Delete: {deleted:?}");
    assert!(
        deleted.contains(&SECOND),
        "the first Review Worker {SECOND:?} goes with its Task: {deleted:?}"
    );
}

/// Attack: a stale report from a superseded Worker. A redirect supersedes
/// Check's Worker; its late `report revision --pr` is answered "changed
/// nothing" but sets the Task's PR.
#[test]
fn interrogate_a_superseded_workers_revision_report_sets_the_pr() {
    let mut factory = Factory::in_memory();
    let (id, steps) = saved(
        &mut factory,
        &[("Build", "Build it"), ("Check", "Check it")],
    );
    let task = run(&mut factory, &id, "Do it");
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);
    finished(&mut factory, FIRST, "Built");
    started(&mut factory, &task, SECOND);
    factory
        .command(
            Command::Redirect {
                task: task.clone(),
                direction: Override {
                    to: steps[0].clone(),
                    skip: Vec::new(),
                    then: ThenReturn::Routes,
                    note: String::new(),
                },
            },
            9,
        )
        .unwrap();
    assert!(factory
        .task(&task)
        .unwrap()
        .oversight
        .superseded
        .contains(&SECOND));
    let revision = factory.revision();
    let answer = factory.command(
        Command::ReportRevision {
            worker: SECOND,
            head: HEAD.into(),
            pr: Some("acme/widgets#99".into()),
        },
        10,
    );
    let pr = factory.task(&task).unwrap().delivery.pr.clone();
    println!(
        "superseded report: {answer:?}; Task's PR now {:?}; board revision {revision} -> {}",
        pr.as_ref().map(PullRequest::label),
        factory.revision()
    );
    assert_eq!(pr, None, "a superseded Worker's report is dropped");
}

/// Attack: dedup of script submissions. A repeated identity is the Task
/// it already started, unless that Task was archived (Clear): then the
/// same identity starts a second Task.
#[test]
fn interrogate_an_archived_submission_is_started_again() {
    let mut factory = Factory::in_memory();
    let (id, _) = saved(&mut factory, &[("Agent", "Do it")]);
    factory
        .command(
            Command::EditStart {
                workflow: id.clone(),
                start: Start {
                    kind: StartKind::External,
                    ..Start::default()
                },
            },
            1,
        )
        .unwrap();
    save(&mut factory, &id).unwrap();
    factory
        .command(
            Command::Automation {
                workflow: id.clone(),
                turn: Turn::On,
            },
            2,
        )
        .unwrap();
    let submit = |factory: &mut Factory| {
        factory.command(
            Command::Submit {
                workflow: id.clone(),
                identity: "JIRA-1".into(),
                instructions: "Fix JIRA-1".into(),
            },
            3,
        )
    };
    let Ok(Answer::Task(first)) = submit(&mut factory) else {
        panic!("a Task");
    };
    factory.step(4, &BTreeMap::new());
    started(&mut factory, &first, FIRST);
    finished(&mut factory, FIRST, "Fixed");
    assert_eq!(status(&factory, &first), Status::Completed);
    factory
        .command(Command::Clear { project: project() }, 10)
        .unwrap();
    assert!(factory.task(&first).unwrap().archived());
    let again = submit(&mut factory);
    println!("first {first}; the same identity again after Clear: {again:?}");
    assert_eq!(again, Ok(Answer::Duplicate(first)));
}

/// Attack: a failed write then retry. `WorkerStarted` cannot be written:
/// the Worker's Thread already runs, but the Task never learns it. It
/// stays Working with no Worker for good, holding a slot; the Worker's
/// reports are refused and it is never nudged.
#[test]
fn interrogate_a_failed_worker_started_write_wedges_the_task() {
    let store = scratch_store("wedge");
    let mut factory = Factory::open(&store);
    factory.set_limit(1);
    let task = start_in(&mut factory, project(), "Fix it");
    factory.step(1, &BTreeMap::new());

    let part = store.join(DIR_NAME).join("1.json");
    let good = std::fs::read(&part).unwrap();
    std::fs::remove_file(&part).unwrap();
    std::fs::create_dir_all(&part).unwrap();
    std::fs::write(part.join("keep"), b"x").unwrap();
    let answer = factory.command(
        Command::WorkerStarted {
            task: task.clone(),
            thread: WORKER,
            worktree: None,
        },
        2,
    );
    println!("WorkerStarted while the disk fails: {answer:?}");
    std::fs::remove_dir_all(&part).unwrap();
    std::fs::write(&part, good).unwrap();

    let queued = start_in(&mut factory, project(), "Waiting for a slot");
    let mut effects = Vec::new();
    for n in 0..10u64 {
        effects.extend(factory.step(
            3 + n as i64 * 120_000,
            &BTreeMap::from([at(WORKER, false, n + 1, None)]),
        ));
    }
    let report = factory.command(
        Command::Report {
            worker: WORKER,
            outcome: Outcome::Finished {
                summary: "Fixed".into(),
            },
        },
        2_000_000,
    );
    let wedged = factory.task(&task).unwrap();
    println!(
        "20 minutes on: {task} {:?} worker {:?}; {queued} {:?}; effects {effects:?}; its Worker's report: {report:?}",
        wedged.status,
        wedged.worker,
        status(&factory, &queued)
    );
    assert!(
        wedged.worker == Some(WORKER) || wedged.status != Status::Working,
        "the Task is left Working with no Worker, forever"
    );
    let _ = std::fs::remove_dir_all(&store);
}

/// Attack: nudge-once. A Worker nudged to report, which then asks a
/// Provider permission that the operator answers, is nudged a second time
/// in the same run: answering the permission forgets the nudge.
#[test]
fn interrogate_a_permission_between_makes_a_second_nudge() {
    let mut factory = Factory::in_memory();
    let task = start_in(&mut factory, project(), "Fix it");
    factory.step(1, &BTreeMap::new());
    worker(&mut factory, &task, WORKER);
    let mut nudges = Vec::new();
    let mut pass = |factory: &mut Factory, now: i64, seen: (ThreadId, Seen)| {
        let effects = factory.step(now, &BTreeMap::from([seen]));
        for text in prompts(&effects) {
            if text.contains("without reporting") {
                nudges.push(now);
            }
        }
    };
    pass(&mut factory, 3, at(WORKER, true, 0, None));
    pass(&mut factory, 4, at(WORKER, false, 1, None)); // at rest, no report: nudged
    pass(&mut factory, 5, at(WORKER, true, 1, Some("Run cargo test")));
    pass(&mut factory, 6, at(WORKER, true, 1, None)); // the operator allows it
    pass(&mut factory, 7, at(WORKER, false, 2, None)); // at rest again, no report
    println!(
        "nudged at {nudges:?}; status {:?}; activity {:?}",
        status(&factory, &task),
        factory
            .task(&task)
            .unwrap()
            .activity
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(nudges.len(), 1, "a Worker at rest is nudged exactly once");
}
