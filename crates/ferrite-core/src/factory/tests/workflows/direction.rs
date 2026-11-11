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

fn ask(factory: &mut Factory, worker: ThreadId, text: &str) -> Result<Answer, Refusal> {
    factory.command(
        Command::Ask {
            worker,
            text: text.into(),
        },
        12,
    )
}

fn answer(factory: &mut Factory, task: &str, text: &str) -> Result<Answer, Refusal> {
    factory.command(
        Command::AnswerQuestion {
            task: task.into(),
            text: text.into(),
            question: None,
        },
        14,
    )
}

/// AC: a Worker's question moves its Task to Needs human (told) and
/// starts no dependent Step; the answer resumes the asker with the
/// question and the answer, and a later Step is told both; a second
/// answer starts nothing.
#[test]
fn a_question_holds_the_task_until_answered_and_resumes_the_asker() {
    let mut factory = Factory::in_memory();
    let (id, _) = saved(
        &mut factory,
        &[("Build", "Build it"), ("Review", "Review it")],
    );
    let task = running(&mut factory, &id);
    assert_eq!(
        ask(&mut factory, FIRST, "Which flag name?"),
        Ok(Answer::Changed(true))
    );
    let effects = factory.step(13, &at(FIRST, false, 1));
    assert_eq!(notified(&effects), [Status::NeedsHuman]);
    assert!(
        starts(&effects).is_empty() && prompts(&effects).is_empty(),
        "{effects:?}"
    );
    let held = the(&factory, &task);
    assert_eq!(held.status, Status::NeedsHuman);
    assert_eq!(
        held.oversight.question.as_ref().map(|q| q.text.as_str()),
        Some("Which flag name?")
    );
    assert!(
        factory.step(14, &at(FIRST, false, 5)).is_empty(),
        "not nudged while it waits"
    );

    assert_eq!(
        answer(&mut factory, &task, "  "),
        Err(Refusal::new(RefusalCode::Invalid, "write the answer"))
    );
    assert_eq!(
        answer(&mut factory, &task, "Call it --fast"),
        Ok(Answer::Changed(true))
    );
    let effects = factory.step(15, &at(FIRST, false, 5));
    let told = prompts(&effects);
    assert_eq!(told.len(), 1, "{effects:?}");
    assert!(told[0].contains("Which flag name?"), "{}", told[0]);
    assert!(told[0].contains("Call it --fast"), "{}", told[0]);
    assert_eq!(status(&factory, &task), Status::Working);
    assert!(starts(&effects).is_empty());

    assert_eq!(
        answer(&mut factory, &task, "Call it --fast"),
        Ok(Answer::Changed(false))
    );
    assert!(
        factory.step(16, &at(FIRST, true, 5)).is_empty(),
        "a duplicate starts nothing"
    );

    let effects = finished(&mut factory, FIRST, "Added --fast");
    let [(_, _, prompt)] = starts(&effects).try_into().unwrap();
    assert!(prompt.contains("Which flag name?"), "{prompt}");
    assert!(prompt.contains("Call it --fast"), "{prompt}");
}

/// A question from a Worker no longer at the Task's Step is ignored, and
/// an answer naming an earlier question changes nothing.
#[test]
fn a_stale_question_or_answer_changes_nothing() {
    let mut factory = Factory::in_memory();
    let (id, _) = saved(
        &mut factory,
        &[("Build", "Build it"), ("Review", "Review it")],
    );
    let task = running(&mut factory, &id);
    finished(&mut factory, FIRST, "Built");
    started(&mut factory, &task, SECOND);
    assert_eq!(
        ask(&mut factory, FIRST, "Late?"),
        Ok(Answer::Changed(false))
    );
    ask(&mut factory, SECOND, "Which branch?").unwrap();
    let named = factory.command(
        Command::AnswerQuestion {
            task: task.clone(),
            text: "main".into(),
            question: Some(7),
        },
        14,
    );
    assert_eq!(named, Ok(Answer::Changed(false)));
    assert_eq!(status(&factory, &task), Status::NeedsHuman);
}

/// AC: a Provider Decision in a Worker's Session is the Task's: it needs
/// the operator (told) until it is answered, then works again without
/// being nudged for the turn it was in.
#[test]
fn a_provider_decision_in_a_worker_needs_the_operator_in_the_task() {
    let mut factory = Factory::in_memory();
    let (id, _) = saved(&mut factory, &[("Build", "Build it")]);
    let task = running(&mut factory, &id);
    let asking = BTreeMap::from([(
        FIRST,
        Seen {
            busy: true,
            turns: 0,
            decision: Some("Run cargo test".into()),
        },
    )]);
    let effects = factory.step(10, &asking);
    assert_eq!(notified(&effects), [Status::NeedsHuman]);
    let held = the(&factory, &task);
    assert_eq!(held.oversight.permission.as_deref(), Some("Run cargo test"));
    assert_eq!(
        held.reason.as_deref(),
        Some("Build asks permission: Run cargo test")
    );
    assert!(factory.step(11, &asking).is_empty(), "told once");

    let effects = factory.step(12, &at(FIRST, true, 0));
    assert!(notified(&effects).is_empty());
    assert_eq!(status(&factory, &task), Status::Working);
    assert!(factory.step(13, &at(FIRST, true, 0)).is_empty());
    // Its report during the wait still counts.
    factory.step(14, &asking);
    let effects = finished(&mut factory, FIRST, "Tested");
    assert_eq!(notified(&effects), [Status::Completed]);
}

fn decided(factory: &mut Factory, worker: ThreadId, outcome: Outcome) -> Vec<Effect> {
    factory
        .command(Command::Report { worker, outcome }, 8)
        .unwrap();
    factory.step(9, &BTreeMap::new())
}

/// AC: agent approval is refused (cannot be saved) without a deciding
/// Step and written instructions, each said; given both, it saves.
#[test]
fn agent_approval_needs_a_deciding_step_and_instructions() {
    let mut factory = Factory::in_memory();
    let (id, steps) = saved(
        &mut factory,
        &[("Build", "Build it"), ("Review", "Review it")],
    );
    approval(
        &mut factory,
        &id,
        ApprovalChange {
            by: Some(Approver::Agent),
            ..Default::default()
        },
    )
    .unwrap();
    let problems: Vec<String> = workflow(&factory, &id)
        .draft
        .problems(&Catalog::default())
        .into_iter()
        .filter(|problem| problem.approval)
        .map(|problem| problem.message)
        .collect();
    assert_eq!(
        problems,
        [
            "Agent approval: choose the agent that decides.",
            "Agent approval: write the decision instructions (what to verify, what evidence \
             to return, when it may approve without you).",
        ]
    );
    let refused = save(&mut factory, &id).unwrap_err();
    assert_eq!(refused.code, RefusalCode::Invalid);
    assert!(
        refused.message.contains("choose the agent that decides"),
        "{refused}"
    );

    approval(
        &mut factory,
        &id,
        ApprovalChange {
            step: Some(steps[1].clone()),
            instructions: Some("Approve when the tests pass".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(save(&mut factory, &id), Ok(Answer::Version(2)));
}

/// AC: the deciding Step whose own route leads to approval decides in the
/// same run, told the instructions; approving runs what follows approval.
#[test]
fn a_deciding_step_whose_route_leads_to_approval_decides_in_the_same_run() {
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
            on_finished: Some(Destination::Approval),
            ..Default::default()
        },
    );
    approval(
        &mut factory,
        &id,
        ApprovalChange {
            by: Some(Approver::Agent),
            step: Some(steps[1].clone()),
            instructions: Some("Approve when the tests pass".into()),
            ..Default::default()
        },
    )
    .unwrap();
    save(&mut factory, &id).unwrap();
    let task = running(&mut factory, &id);
    let effects = finished(&mut factory, FIRST, "Built");
    let [(_, _, prompt)] = starts(&effects).try_into().unwrap();
    assert!(prompt.contains("Approve when the tests pass"), "{prompt}");
    assert!(prompt.contains("report outcome approved"), "{prompt}");
    started(&mut factory, &task, SECOND);

    let effects = decided(
        &mut factory,
        SECOND,
        Outcome::Approved {
            summary: "All 40 tests pass".into(),
        },
    );
    assert!(starts(&effects).is_empty(), "no second run: {effects:?}");
    assert_eq!(notified(&effects), [Status::Completed]);
}

/// AC: a deciding Step not on the approval route runs once more to
/// decide, with the instructions and what is approved; it rejects with
/// reasons, which reach the rework recipient; a plain "finished" from a
/// deciding run is refused.
#[test]
fn another_deciding_step_runs_once_more_and_a_rejection_reaches_rework() {
    let mut factory = Factory::in_memory();
    let (id, steps) = reviewed(&mut factory, ApprovalChange::default());
    approval(
        &mut factory,
        &id,
        ApprovalChange {
            by: Some(Approver::Agent),
            step: Some(steps[0].clone()),
            instructions: Some("Check the changelog entry".into()),
            rework_to: Some(Destination::Step(steps[0].clone())),
            ..Default::default()
        },
    )
    .unwrap();
    save(&mut factory, &id).unwrap();
    let task = running(&mut factory, &id);
    let effects = finished(&mut factory, FIRST, "Built");
    let [(_, _, prompt)] = starts(&effects).try_into().unwrap();
    assert!(
        !prompt.contains("Check the changelog entry"),
        "Review does not decide"
    );
    started(&mut factory, &task, SECOND);

    finished(&mut factory, SECOND, "Reviewed: fine");
    assert_eq!(
        status(&factory, &task),
        Status::Working,
        "not Ready to approve yet"
    );
    let deciding = the(&factory, &task);
    assert_eq!(deciding.worker, Some(FIRST), "Build's agent decides");
    assert!(deciding.oversight.deciding);
    let effects = factory.step(10, &at(FIRST, false, 1));
    let told = prompts(&effects);
    assert_eq!(told.len(), 1, "{effects:?}");
    assert!(told[0].contains("Check the changelog entry"), "{}", told[0]);
    assert!(
        told[0].contains("Reviewed: fine"),
        "what is approved: {}",
        told[0]
    );

    let refused = factory.command(
        Command::Report {
            worker: FIRST,
            outcome: Outcome::Finished {
                summary: "done".into(),
            },
        },
        11,
    );
    assert_eq!(refused.unwrap_err().code, RefusalCode::Invalid);

    decided(
        &mut factory,
        FIRST,
        Outcome::Rejected {
            reasons: "No changelog entry".into(),
        },
    );
    let back = the(&factory, &task);
    assert_eq!((back.status, back.worker), (Status::Working, Some(FIRST)));
    assert!(!back.oversight.deciding, "it reworks now");
    let effects = factory.step(12, &at(FIRST, false, 2));
    let told = prompts(&effects);
    assert!(told[0].contains("No changelog entry"), "{}", told[0]);
    assert!(told[0].contains("report outcome finished"), "{}", told[0]);
}

/// AC: approving runs the configured continuation: a Step, told what was
/// approved.
#[test]
fn approving_runs_the_step_that_follows_approval() {
    let mut factory = Factory::in_memory();
    let (id, _) = reviewed(&mut factory, ApprovalChange::default());
    let merge = add(&mut factory, &id, "Merge", "Merge it");
    edit(
        &mut factory,
        &id,
        &merge,
        StepChange {
            on_finished: Some(Destination::Complete),
            ..Default::default()
        },
    );
    approval(
        &mut factory,
        &id,
        ApprovalChange {
            after: Some(Destination::Step(merge.clone())),
            ..Default::default()
        },
    )
    .unwrap();
    save(&mut factory, &id).unwrap();
    let task = running(&mut factory, &id);
    finished(&mut factory, FIRST, "Built");
    started(&mut factory, &task, SECOND);
    finished(&mut factory, SECOND, "Approved by review");
    assert_eq!(status(&factory, &task), Status::Approval);
    command(&mut factory, Command::Approve { task: task.clone() }).unwrap();
    let effects = factory.step(21, &BTreeMap::new());
    let [(_, _, prompt)] = starts(&effects).try_into().unwrap();
    assert!(prompt.contains("Merge it"), "{prompt}");
    assert!(prompt.contains("Approved by review"), "{prompt}");
    assert_eq!(the(&factory, &task).step.as_deref(), Some(merge.as_str()));
}

/// AC: the read-only list of what ends in Completed names approval when
/// approving completes the Task.
#[test]
fn the_completion_list_names_approval() {
    let mut factory = Factory::in_memory();
    let (id, _) = reviewed(&mut factory, ApprovalChange::default());
    assert_eq!(
        workflow(&factory, &id).draft.completion_paths(),
        ["Your approval marks the task complete"]
    );
}

/// Build → Review → Check, Check routing to approval; rework goes to
/// Build by default.
fn three(factory: &mut Factory) -> (WorkflowId, Vec<StepId>) {
    let (id, steps) = saved(
        factory,
        &[
            ("Build", "Build it"),
            ("Review", "Review it"),
            ("Check", "Check it"),
        ],
    );
    edit(
        factory,
        &id,
        &steps[2],
        StepChange {
            on_finished: Some(Destination::Approval),
            ..Default::default()
        },
    );
    approval(
        factory,
        &id,
        ApprovalChange {
            rework_to: Some(Destination::Step(steps[0].clone())),
            ..Default::default()
        },
    )
    .unwrap();
    save(factory, &id).unwrap();
    (id, steps)
}

/// A Task on `three` at Ready to approve: FIRST, SECOND, THIRD ran.
fn ready(factory: &mut Factory, id: &str) -> TaskId {
    let task = running(factory, id);
    finished(factory, FIRST, "Built");
    started(factory, &task, SECOND);
    finished(factory, SECOND, "Review found a typo");
    started(factory, &task, THIRD);
    finished(factory, THIRD, "Checks pass");
    assert_eq!(status(factory, &task), Status::Approval);
    task
}

fn rework(
    factory: &mut Factory,
    task: &str,
    text: &str,
    to: Option<Destination>,
) -> Result<Answer, Refusal> {
    factory.command(
        Command::Rework {
            task: task.into(),
            feedback: text.into(),
            to,
        },
        20,
    )
}

/// AC: rework needs feedback; it goes to the Workflow's recipient unless
/// the operator chooses another; the recipient and every later Step are
/// told the feedback and the earlier findings.
#[test]
fn rework_reaches_the_recipient_and_every_later_step() {
    let mut factory = Factory::in_memory();
    let (id, steps) = three(&mut factory);
    let task = ready(&mut factory, &id);
    assert_eq!(
        rework(&mut factory, &task, " ", None).unwrap_err().code,
        RefusalCode::Invalid
    );
    assert_eq!(status(&factory, &task), Status::Approval);

    // Chosen for this Task: Review, not the Workflow's Build.
    rework(
        &mut factory,
        &task,
        "Rename the flag",
        Some(Destination::Step(steps[1].clone())),
    )
    .unwrap();
    let held = the(&factory, &task);
    assert_eq!((held.status, held.worker), (Status::Working, Some(SECOND)));
    let effects = factory.step(21, &at(SECOND, false, 1));
    let told = prompts(&effects);
    assert!(told[0].contains("Rename the flag"), "{}", told[0]);
    assert!(
        told[0].contains("Checks pass"),
        "earlier findings: {}",
        told[0]
    );

    // Check continues, told the feedback too.
    finished(&mut factory, SECOND, "Renamed");
    let effects = factory.step(22, &at(THIRD, false, 1));
    let told = prompts(&effects);
    assert!(told[0].contains("Rename the flag"), "{}", told[0]);
    assert!(told[0].contains("Renamed"), "{}", told[0]);

    // Default recipient: the Workflow's, Build.
    finished(&mut factory, THIRD, "Checks pass again");
    rework(&mut factory, &task, "Add a test", None).unwrap();
    assert_eq!(the(&factory, &task).worker, Some(FIRST));
    assert_eq!(
        workflow(&factory, &id)
            .latest()
            .unwrap()
            .definition
            .approval
            .rework_to,
        Destination::Step(steps[0].clone()),
        "the saved Workflow is unchanged"
    );
}

fn redirect(
    factory: &mut Factory,
    task: &str,
    to: &str,
    skip: &[&str],
    then: ThenReturn,
) -> Result<Answer, Refusal> {
    factory.command(
        Command::Redirect {
            task: task.into(),
            direction: Override {
                to: to.into(),
                skip: skip.iter().map(|id| id.to_string()).collect(),
                then,
                note: "Only the docs changed".into(),
            },
        },
        30,
    )
}

/// AC: a redirect sends one Task to a chosen Step and skips Steps for it;
/// the run it interrupts is superseded (parked, its late report ignored);
/// the saved Workflow is unchanged.
#[test]
fn a_redirect_skips_steps_and_supersedes_the_current_run() {
    let mut factory = Factory::in_memory();
    let (id, steps) = three(&mut factory);
    let before = workflow(&factory, &id).clone();
    let task = running(&mut factory, &id);
    finished(&mut factory, FIRST, "Built");
    started(&mut factory, &task, SECOND);

    // Mid-Review: back to Build, skipping Review.
    redirect(
        &mut factory,
        &task,
        &steps[0],
        &[&steps[1]],
        ThenReturn::Routes,
    )
    .unwrap();
    let effects = factory.step(31, &at(FIRST, false, 1));
    assert!(
        effects.contains(&Effect::Park { thread: SECOND }),
        "{effects:?}"
    );
    let told = prompts(&effects);
    assert!(told[0].contains("Only the docs changed"), "{}", told[0]);
    assert_eq!(the(&factory, &task).worker, Some(FIRST));

    let late = factory.command(
        Command::Report {
            worker: SECOND,
            outcome: Outcome::Finished {
                summary: "Late review".into(),
            },
        },
        32,
    );
    assert_eq!(late, Ok(Answer::Changed(false)), "a late result is ignored");
    assert_eq!(the(&factory, &task).worker, Some(FIRST));

    // Build finishes: Review is skipped, Check starts.
    let effects = finished(&mut factory, FIRST, "Docs rebuilt");
    let [(_, _, prompt)] = starts(&effects).try_into().unwrap();
    assert!(prompt.contains("Check it"), "{prompt}");
    assert_eq!(
        the(&factory, &task).step.as_deref(),
        Some(steps[2].as_str())
    );
    assert_eq!(
        *workflow(&factory, &id),
        before,
        "the saved Workflow is unchanged"
    );
}

/// AC: "return it to me" brings the Task back to the operator once the
/// chosen Step finishes, whatever its route, and overrides agent
/// approval.
#[test]
fn return_it_to_me_overrides_the_routes_and_agent_approval() {
    let mut factory = Factory::in_memory();
    let (id, steps) = three(&mut factory);
    approval(
        &mut factory,
        &id,
        ApprovalChange {
            by: Some(Approver::Agent),
            step: Some(steps[2].clone()),
            instructions: Some("Approve if green".into()),
            ..Default::default()
        },
    )
    .unwrap();
    save(&mut factory, &id).unwrap();
    let task = running(&mut factory, &id);
    redirect(&mut factory, &task, &steps[1], &[], ThenReturn::Me).unwrap();
    let effects = factory.step(31, &BTreeMap::new());
    assert!(
        effects.contains(&Effect::Park { thread: FIRST }),
        "{effects:?}"
    );
    started(&mut factory, &task, SECOND);
    let effects = finished(&mut factory, SECOND, "Reviewed the docs");
    assert_eq!(notified(&effects), [Status::Approval]);
    assert!(starts(&effects).is_empty(), "Check is not run: {effects:?}");
    let back = the(&factory, &task);
    assert_eq!(back.status, Status::Approval);
    assert!(back.oversight.to_me);

    // Again through the routes, to the deciding Check: still the operator's.
    rework(
        &mut factory,
        &task,
        "Once more",
        Some(Destination::Step(steps[2].clone())),
    )
    .unwrap();
    assert!(!the(&factory, &task).oversight.to_me, "rework clears it");
    let fresh = run(&mut factory, &id, "Another");
    factory.step(40, &BTreeMap::new());
    started(&mut factory, &fresh, ThreadId::new(50));
    redirect(&mut factory, &fresh, &steps[2], &[], ThenReturn::Me).unwrap();
    factory.step(41, &BTreeMap::new());
    started(&mut factory, &fresh, ThreadId::new(51));
    let effects = finished(&mut factory, ThreadId::new(51), "Green");
    assert_eq!(
        notified(&effects),
        [Status::Approval],
        "the operator approves"
    );
}

/// AC: a redirect and what it superseded survive a reload: the late
/// result still cannot restore the skipped path.
#[test]
fn a_redirect_survives_a_reload() {
    let dir = std::env::temp_dir().join(format!("factory-redirect-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("factory.json");
    let mut factory = Factory::load(&path).unwrap();
    let (id, steps) = three(&mut factory);
    let task = running(&mut factory, &id);
    redirect(&mut factory, &task, &steps[2], &[&steps[1]], ThenReturn::Me).unwrap();
    factory.step(31, &BTreeMap::new());
    started(&mut factory, &task, THIRD);

    let mut reread = Factory::load(&path).unwrap();
    let held = the(&reread, &task).clone();
    assert_eq!(held.oversight.superseded, [FIRST]);
    assert_eq!(
        held.oversight.direction.as_ref().map(|d| d.skip.clone()),
        Some(vec![steps[1].clone()])
    );
    let late = reread.command(
        Command::Report {
            worker: FIRST,
            outcome: Outcome::Finished {
                summary: "Late build".into(),
            },
        },
        32,
    );
    assert_eq!(late, Ok(Answer::Changed(false)));
    assert_eq!(the(&reread, &task).step.as_deref(), Some(steps[2].as_str()));
    std::fs::remove_dir_all(&dir).unwrap();
}
