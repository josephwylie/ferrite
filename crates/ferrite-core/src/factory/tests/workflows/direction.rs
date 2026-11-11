//! Where a Task needs the operator, or an agent stands in for them
//! (#222): questions, Provider Decisions, approval by the operator or an
//! agent, rework, and one Task's redirect, through the module's
//! interface.

use super::*;

/// What a busy or resting Worker shows the step.
fn at(thread: ThreadId, busy: bool, turns: u64) -> BTreeMap<ThreadId, Seen> {
    BTreeMap::from([(
        thread,
        Seen {
            busy,
            turns,
            decision: None,
        },
    )])
}

fn approval(factory: &mut Factory, id: &str, change: ApprovalChange) -> Result<Answer, Refusal> {
    factory.command(
        Command::EditApproval {
            workflow: id.into(),
            change,
        },
        2,
    )
}

/// Build → Review, Review routing to Ready to approve, saved.
fn reviewed(factory: &mut Factory, change: ApprovalChange) -> (WorkflowId, Vec<StepId>) {
    let (id, steps) = saved(factory, &[("Build", "Build it"), ("Review", "Review it")]);
    edit(
        factory,
        &id,
        &steps[1],
        StepChange {
            on_finished: Some(Destination::Approval),
            ..Default::default()
        },
    );
    approval(factory, &id, change).unwrap();
    save(factory, &id).unwrap();
    (id, steps)
}

/// A Task on `id`, its first Step's Worker up as FIRST.
fn running(factory: &mut Factory, id: &str) -> TaskId {
    let task = run(factory, id, "Add the flag");
    factory.step(7, &BTreeMap::new());
    started(factory, &task, FIRST);
    task
}

fn command(factory: &mut Factory, command: Command) -> Result<Answer, Refusal> {
    factory.command(command, 20)
}

fn the<'a>(factory: &'a Factory, id: &str) -> &'a Task {
    factory.task(id).expect("the task")
}

/// AC: a result routed to Ready to approve waits there for the operator,
/// who is told; approving runs what follows approval (here Completed),
/// and approving again changes nothing.
#[test]
fn a_result_waits_in_ready_to_approve_and_approving_completes_it() {
    let mut factory = Factory::in_memory();
    let (id, _) = reviewed(&mut factory, ApprovalChange::default());
    let task = running(&mut factory, &id);
    finished(&mut factory, FIRST, "Built");
    started(&mut factory, &task, SECOND);
    let effects = finished(&mut factory, SECOND, "Looks right");
    assert_eq!(notified(&effects), [Status::Approval]);
    assert!(starts(&effects).is_empty(), "{effects:?}");
    assert_eq!(status(&factory, &task), Status::Approval);
    assert_eq!(factory.board().needs_you(project()), 1);

    let approve = || Command::Approve { task: task.clone() };
    assert_eq!(command(&mut factory, approve()), Ok(Answer::Changed(true)));
    let effects = factory.step(21, &BTreeMap::new());
    assert_eq!(notified(&effects), [Status::Completed]);
    let done = the(&factory, &task);
    assert_eq!(done.status, Status::Completed);
    assert_eq!(done.result.as_deref(), Some("Looks right"));
    assert!(done
        .activity
        .iter()
        .any(|entry| entry.by == Actor::Operator && entry.text == "Approved"));
    assert_eq!(command(&mut factory, approve()), Ok(Answer::Changed(false)));
}
