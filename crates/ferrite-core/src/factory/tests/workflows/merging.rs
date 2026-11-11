//! Pull requests and merging (#223) through the module's interface: a
//! Task's PR and its revisions, approval tied to a revision, evidence,
//! routes by merge result, and the worktree tidied once merged.

use super::*;

fn start_on(
    factory: &mut Factory,
    workflow: Option<&str>,
    pr: Option<&str>,
) -> Result<Answer, Refusal> {
    factory.command(
        Command::Start {
            project: project(),
            instructions: "Ship the flag".into(),
            agent: agent(),
            request: None,
            workflow: workflow.map(str::to_string),
            pr: pr.map(str::to_string),
        },
        6,
    )
}

fn task_id(answer: Result<Answer, Refusal>) -> TaskId {
    match answer {
        Ok(Answer::Task(task)) => task,
        other => panic!("{other:?}"),
    }
}

fn the<'a>(factory: &'a Factory, id: &str) -> &'a Task {
    factory.task(id).expect("the task")
}

fn revision(
    factory: &mut Factory,
    worker: ThreadId,
    head: &str,
    pr: Option<&str>,
) -> Result<Answer, Refusal> {
    factory.command(
        Command::ReportRevision {
            worker,
            head: head.into(),
            pr: pr.map(str::to_string),
        },
        8,
    )
}

const HEAD_1: &str = "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD_2: &str = "2222222bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// AC: a Task accepts a PR reference at start, as owner/repository#number
/// or a URL, validated; the first Step is told it.
#[test]
fn a_task_takes_its_pr_at_start_as_a_reference_or_a_url() {
    let mut factory = Factory::in_memory();
    factory.set_limit(10);
    let (id, _) = saved(&mut factory, &[("Build", "Build it")]);
    for given in [
        "acme/widgets#12",
        " https://github.com/acme/widgets/pull/12 ",
        "https://github.com/acme/widgets/pull/12/files",
        "github.com/acme/widgets/pull/12",
    ] {
        let task = task_id(start_on(&mut factory, Some(&id), Some(given)));
        let pr = the(&factory, &task).delivery.pr.clone().expect("its PR");
        assert_eq!((pr.repository.as_str(), pr.number), ("acme/widgets", 12));
        assert_eq!(pr.url(), "https://github.com/acme/widgets/pull/12");
        assert!(pr.revisions.is_empty(), "no head is known yet");
    }
    let before = factory.board().tasks.len();
    for bad in [
        "acme/widgets",
        "#12",
        "acme#12",
        "acme/widgets#0",
        "acme/widgets#x",
        "acme/wid gets#3",
        "https://gitlab.com/acme/widgets/pull/1",
        "https://github.com/acme/widgets/issues/12",
    ] {
        let refused = start_on(&mut factory, Some(&id), Some(bad)).unwrap_err();
        assert_eq!(refused.code, RefusalCode::Invalid, "{bad}");
    }
    assert_eq!(
        factory.board().tasks.len(),
        before,
        "a bad PR starts nothing"
    );

    let one_off = task_id(start_on(&mut factory, None, Some("acme/widgets#7")));
    assert_eq!(
        the(&factory, &one_off).delivery.pr.as_ref().unwrap().number,
        7
    );

    let effects = factory.step(7, &BTreeMap::new());
    let prompts: Vec<String> = starts(&effects).into_iter().map(|(_, _, p)| p).collect();
    assert!(
        prompts
            .iter()
            .all(|prompt| prompt.contains("## Pull request")),
        "{prompts:?}"
    );
    assert!(prompts
        .iter()
        .any(|prompt| prompt.contains("https://github.com/acme/widgets/pull/7")));
}

/// AC: a Worker can report the PR and each new revision; revisions are
/// kept, each with who reported it. A repeat changes nothing.
#[test]
fn a_worker_reports_the_pr_and_each_revision_with_who_reported_it() {
    let mut factory = Factory::in_memory();
    let (id, _) = saved(&mut factory, &[("Build", "Build it"), ("Fix", "Fix it")]);
    let task = task_id(start_on(&mut factory, Some(&id), None));
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);

    let refused = revision(&mut factory, FIRST, HEAD_1, None).unwrap_err();
    assert_eq!(refused.code, RefusalCode::Invalid, "which PR is it?");
    let refused = revision(&mut factory, FIRST, "not-a-sha", Some("acme/widgets#12")).unwrap_err();
    assert_eq!(refused.code, RefusalCode::Invalid);

    assert_eq!(
        revision(&mut factory, FIRST, HEAD_1, Some("acme/widgets#12")),
        Ok(Answer::Changed(true))
    );
    assert_eq!(
        revision(&mut factory, FIRST, HEAD_1, None),
        Ok(Answer::Changed(false)),
        "the same head again is no new revision"
    );
    let refused = revision(&mut factory, FIRST, HEAD_2, Some("acme/widgets#99")).unwrap_err();
    assert_eq!(refused.code, RefusalCode::Invalid, "the Task has its PR");
    assert_eq!(
        revision(&mut factory, SECOND, HEAD_2, None),
        Err(Refusal::new(
            RefusalCode::NotAWorker,
            "this Thread is no Task's Worker"
        ))
    );

    let effects = finished(&mut factory, FIRST, "Opened the PR");
    let prompt = &starts(&effects)[0].2;
    assert!(
        prompt.contains("acme/widgets#12") && prompt.contains("revision 1"),
        "every Step is told the PR and its revision: {prompt}"
    );
    assert!(prompt.contains("1111111"), "{prompt}");
    started(&mut factory, &task, SECOND);
    assert_eq!(
        revision(&mut factory, FIRST, HEAD_2, None),
        Ok(Answer::Changed(false)),
        "an earlier Step's Worker reports into the past"
    );
    revision(&mut factory, SECOND, HEAD_2, None).unwrap();

    let pr = the(&factory, &task).delivery.pr.clone().unwrap();
    let kept: Vec<(u32, &str, &str)> = pr
        .revisions
        .iter()
        .map(|each| (each.revision, each.head.as_str(), each.by.as_str()))
        .collect();
    assert_eq!(kept, [(1, HEAD_1, "Build"), (2, HEAD_2, "Fix")]);
    assert_eq!(pr.current().unwrap().revision, 2);
    let activity = &the(&factory, &task).activity;
    assert!(activity.iter().any(|entry| entry.by == Actor::Agent
        && entry.text.contains("revision 2")
        && entry.revision == Some(2)));
}

fn merge_report(
    factory: &mut Factory,
    worker: ThreadId,
    result: MergeOutcome,
    summary: &str,
) -> Result<Answer, Refusal> {
    factory.command(
        Command::Report {
            worker,
            outcome: Outcome::Merge {
                result,
                summary: summary.into(),
            },
        },
        8,
    )
}

fn problems(factory: &Factory, id: &str) -> Vec<String> {
    workflow(factory, id)
        .draft
        .problems(&Catalog::default())
        .into_iter()
        .map(|problem| problem.message)
        .collect()
}

/// Build → Merge → Resolve: Merge routes by merge result, its conflict
/// to Resolve, which goes back to Merge. Saved.
fn merging(factory: &mut Factory) -> (WorkflowId, Vec<StepId>) {
    let (id, steps) = saved(
        factory,
        &[
            ("Build", "Build it"),
            ("Merge", "Merge it"),
            ("Resolve", "Resolve it"),
        ],
    );
    edit(
        factory,
        &id,
        &steps[1],
        StepChange {
            by_merge_result: Some(true),
            on_conflict: Some(Destination::Step(steps[2].clone())),
            ..Default::default()
        },
    );
    edit(
        factory,
        &id,
        &steps[2],
        StepChange {
            on_finished: Some(Destination::Step(steps[1].clone())),
            ..Default::default()
        },
    );
    assert_eq!(problems(factory, &id), Vec::<String>::new());
    save(factory, &id).unwrap();
    (id, steps)
}

/// The Task on `merging`'s Workflow, at its Merge Step with SECOND, its
/// PR at revision 1 from Build.
fn at_merge(factory: &mut Factory) -> (TaskId, Vec<StepId>) {
    let (id, steps) = merging(factory);
    let task = task_id(start_on(factory, Some(&id), Some("acme/widgets#12")));
    factory.step(7, &BTreeMap::new());
    started(factory, &task, FIRST);
    revision(factory, FIRST, HEAD_1, None).unwrap();
    finished(factory, FIRST, "Built");
    started(factory, &task, SECOND);
    (task, steps)
}

/// AC: a Step can be switched to route by merge result, one route per
/// result; validation refuses any result but merged routing to
/// Completed, and only pending may keep waiting.
#[test]
fn a_step_routes_by_merge_result_and_only_merged_may_complete() {
    let mut factory = Factory::in_memory();
    let (id, steps) = saved(&mut factory, &[("Merge", "Merge it")]);
    edit(
        &mut factory,
        &id,
        &steps[0],
        StepChange {
            by_merge_result: Some(true),
            ..Default::default()
        },
    );
    let step = workflow(&factory, &id).draft.steps[0].clone();
    let kinds: Vec<(&str, String)> = step
        .routes()
        .into_iter()
        .map(|(kind, to)| (kind.label(), to.word()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("Merge confirmed", "complete".to_string()),
            ("Merge conflicts", "human".into()),
            ("Checks pending", "wait".into()),
            ("Access denied", "human".into()),
            ("PR changed since approval", "human".into()),
            ("Result unconfirmed", "human".into()),
            ("If unable to finish", "human".into()),
        ]
    );
    assert_eq!(
        workflow(&factory, &id).draft.completion_paths(),
        ["Merge reports merge confirmed"]
    );
    let options = workflow(&factory, &id)
        .draft
        .route_options(&steps[0], RouteKind::Merge(MergeOutcome::Conflict));
    assert!(!options.contains(&Destination::Complete), "{options:?}");
    assert!(!options.contains(&Destination::Wait), "{options:?}");
    assert!(workflow(&factory, &id)
        .draft
        .route_options(&steps[0], RouteKind::Merge(MergeOutcome::Pending))
        .contains(&Destination::Wait));

    edit(
        &mut factory,
        &id,
        &steps[0],
        StepChange {
            on_conflict: Some(Destination::Complete),
            on_denied: Some(Destination::Wait),
            ..Default::default()
        },
    );
    assert_eq!(
        problems(&factory, &id),
        [
            "Merge \u{b7} Merge conflicts cannot complete the task: only a confirmed merge can.",
            "Merge \u{b7} Access denied cannot keep waiting: only Checks pending waits.",
        ]
    );
    assert_eq!(
        save(&mut factory, &id).unwrap_err().code,
        RefusalCode::Invalid
    );

    edit(
        &mut factory,
        &id,
        &steps[0],
        StepChange {
            by_merge_result: Some(false),
            ..Default::default()
        },
    );
    assert_eq!(workflow(&factory, &id).draft.steps[0].outcome_routes, None);
    assert_eq!(workflow(&factory, &id).draft.steps[0].routes().len(), 2);
}

/// AC: the merge Step receives the PR; merged completes the Task;
/// conflict starts the resolver, which hands back; pending keeps waiting
/// with the same Worker; nothing but merged completes.
#[test]
fn each_merge_result_follows_its_route_and_only_merged_completes() {
    let mut factory = Factory::in_memory();
    let (id, _) = merging(&mut factory);
    let task = task_id(start_on(&mut factory, Some(&id), Some("acme/widgets#12")));
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);
    revision(&mut factory, FIRST, HEAD_1, None).unwrap();
    let effects = finished(&mut factory, FIRST, "Built");
    let prompt = &starts(&effects)[0].2;
    assert!(prompt.contains("acme/widgets#12"), "{prompt}");
    assert!(prompt.contains("report outcome head_changed"), "{prompt}");
    assert!(
        prompt.contains("Confirm the PR actually merged"),
        "{prompt}"
    );
    started(&mut factory, &task, SECOND);

    assert_eq!(
        finished_report(&mut factory, SECOND).unwrap_err().code,
        RefusalCode::Invalid,
        "a merge Step reports a merge result"
    );

    // Pending: the same Worker keeps waiting.
    merge_report(&mut factory, SECOND, MergeOutcome::Pending, "CI running").unwrap();
    let effects = factory.step(9, &seen_at(SECOND, true, 0));
    assert!(prompts(&effects).is_empty(), "told once its turn is over");
    let effects = factory.step(9, &seen_at(SECOND, false, 1));
    assert_eq!(status(&factory, &task), Status::Working);
    assert!(notified(&effects).is_empty(), "waiting is silent");
    assert!(starts(&effects).is_empty());
    let waited = prompts(&effects);
    assert_eq!(waited.len(), 1, "{effects:?}");
    assert!(waited[0].contains("still pending"), "{waited:?}");

    // Conflict: the resolver starts, then hands back to Merge.
    merge_report(
        &mut factory,
        SECOND,
        MergeOutcome::Conflict,
        "Conflicts in lib.rs",
    )
    .unwrap();
    let effects = factory.step(10, &BTreeMap::new());
    let resolver = starts(&effects);
    assert_eq!(resolver.len(), 1);
    assert!(
        resolver[0].2.contains("\"Resolve\" agent"),
        "{}",
        resolver[0].2
    );
    assert_ne!(status(&factory, &task), Status::Completed);
    started(&mut factory, &task, THIRD);
    revision(&mut factory, THIRD, HEAD_2, None).unwrap();
    finished(&mut factory, THIRD, "Resolved");
    assert_eq!(the(&factory, &task).worker, Some(SECOND), "back to Merge");

    // Merged completes, and the PR is known merged.
    merge_report(&mut factory, SECOND, MergeOutcome::Merged, "gh says MERGED").unwrap();
    let effects = factory.step(11, &BTreeMap::new());
    assert_eq!(status(&factory, &task), Status::Completed);
    assert_eq!(notified(&effects), [Status::Completed]);
    assert!(the(&factory, &task).delivery.pr.as_ref().unwrap().merged);
}

/// AC: denied, changed head and unconfirmed follow their own routes
/// (here the default, Needs human), none completes, and a merge result
/// from a Step that does not merge is refused.
#[test]
fn denied_changed_head_and_unconfirmed_follow_their_own_routes() {
    for result in [
        MergeOutcome::Denied,
        MergeOutcome::HeadChanged,
        MergeOutcome::Unknown,
    ] {
        let mut factory = Factory::in_memory();
        let (task, _) = at_merge(&mut factory);
        merge_report(&mut factory, SECOND, result, "what gh said").unwrap();
        let effects = factory.step(9, &BTreeMap::new());
        assert_eq!(status(&factory, &task), Status::NeedsHuman, "{result:?}");
        assert_eq!(notified(&effects), [Status::NeedsHuman]);
        assert!(!the(&factory, &task).delivery.pr.as_ref().unwrap().merged);
    }

    let mut factory = Factory::in_memory();
    let (id, _) = saved(&mut factory, &[("Build", "Build it")]);
    let _task = task_id(start_on(&mut factory, Some(&id), None));
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &_task, FIRST);
    assert_eq!(
        merge_report(&mut factory, FIRST, MergeOutcome::Merged, "merged")
            .unwrap_err()
            .code,
        RefusalCode::Invalid
    );
}

/// AC: an unconfirmed result is reconciled by the merge Step before any
/// retry: whenever it runs again it is told to find out first, and never
/// to merge blindly.
#[test]
fn an_unconfirmed_merge_is_reconciled_before_any_retry() {
    let mut factory = Factory::in_memory();
    let (task, steps) = at_merge(&mut factory);
    merge_report(&mut factory, SECOND, MergeOutcome::Unknown, "gh timed out").unwrap();
    factory.step(9, &BTreeMap::new());
    factory
        .command(
            Command::Redirect {
                task: task.clone(),
                direction: Override {
                    to: steps[1].clone(),
                    skip: Vec::new(),
                    then: ThenReturn::Routes,
                    note: String::new(),
                },
            },
            10,
        )
        .unwrap();
    let effects = factory.step(11, &BTreeMap::new());
    let told: Vec<String> = starts(&effects)
        .into_iter()
        .map(|(_, _, prompt)| prompt)
        .chain(prompts(&effects).into_iter().map(str::to_string))
        .collect();
    assert_eq!(told.len(), 1, "{effects:?}");
    assert!(told[0].contains("## Reconcile first"), "{}", told[0]);
    assert!(
        told[0].contains("acme/widgets#12 is already merged"),
        "{}",
        told[0]
    );
}

/// AC: approval records the revision approved; a later revision is not
/// approved by it, and evidence about an older revision says so.
#[test]
fn approval_is_tied_to_the_revision_and_older_evidence_says_so() {
    let mut factory = Factory::in_memory();
    let (id, steps) = saved(&mut factory, &[("Build", "Build it"), ("Fix", "Fix it")]);
    edit(
        &mut factory,
        &id,
        &steps[0],
        StepChange {
            on_finished: Some(Destination::Approval),
            ..Default::default()
        },
    );
    factory
        .command(
            Command::EditApproval {
                workflow: id.clone(),
                change: ApprovalChange {
                    after: Some(Destination::Step(steps[1].clone())),
                    ..Default::default()
                },
            },
            2,
        )
        .unwrap();
    save(&mut factory, &id).unwrap();
    let task = task_id(start_on(&mut factory, Some(&id), Some("acme/widgets#12")));
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);
    revision(&mut factory, FIRST, HEAD_1, None).unwrap();
    factory
        .command(
            Command::ReportEvidence {
                worker: FIRST,
                text: "cargo test: 12 passed".into(),
            },
            8,
        )
        .unwrap();
    finished(&mut factory, FIRST, "Built");
    assert_eq!(status(&factory, &task), Status::Approval);
    factory
        .command(Command::Approve { task: task.clone() }, 9)
        .unwrap();
    assert_eq!(
        the(&factory, &task).delivery.pr.as_ref().unwrap().approved,
        Some(1)
    );
    factory.step(10, &BTreeMap::new());
    started(&mut factory, &task, SECOND);
    revision(&mut factory, SECOND, HEAD_2, None).unwrap();
    let pr = the(&factory, &task).delivery.pr.clone().unwrap();
    assert_eq!((pr.approved, pr.current().unwrap().revision), (Some(1), 2));
    assert!(!pr.approval_current(), "revision 2 is not approved");

    let shown = the(&factory, &task);
    let kinds: Vec<(EvidenceKind, &str, Option<u32>)> = shown
        .delivery
        .evidence
        .iter()
        .map(|each| (each.kind, each.by.as_str(), each.revision))
        .collect();
    assert_eq!(
        kinds,
        [
            (EvidenceKind::Tool, "Build", Some(1)),
            (EvidenceKind::Conclusion, "Build", Some(1)),
            (EvidenceKind::Decision, "You", Some(1)),
        ]
    );
    let line = short_line(shown, 0);
    assert!(
        line.contains("about revision 1, older than the current revision 2"),
        "{line}"
    );
    assert!(shown
        .activity
        .iter()
        .any(|entry| entry.by == Actor::Tool && entry.revision == Some(1)));
    assert!(shown
        .activity
        .iter()
        .any(|entry| entry.by == Actor::Operator && entry.text == "Approved"));
}

fn short_line(task: &Task, at: usize) -> String {
    evidence_line(task, &task.delivery.evidence[at])
}

/// AC: a Task completing with a merged PR has its worktree removed (the
/// Server only removes a clean one, never forced); otherwise it stays
/// and the Task says why.
#[test]
fn a_merged_task_tidies_its_worktree_and_otherwise_says_why_it_stays() {
    let mut factory = Factory::in_memory();
    let (task, _) = at_merge(&mut factory);
    merge_report(&mut factory, SECOND, MergeOutcome::Merged, "MERGED").unwrap();
    let effects = factory.step(9, &BTreeMap::new());
    let tidy: Vec<&Effect> = effects
        .iter()
        .filter(|effect| matches!(effect, Effect::Tidy { .. }))
        .collect();
    assert_eq!(
        tidy,
        [&Effect::Tidy {
            task: task.clone(),
            path: "/repo/.worktrees/factory/t-1".into(),
        }]
    );
    let tidied = |kept: Option<&str>| Command::WorktreeTidied {
        task: task.clone(),
        kept: kept.map(str::to_string),
    };
    assert_eq!(factory.command(tidied(None), 10), Ok(Answer::Changed(true)));
    assert_eq!(the(&factory, &task).worktree, None);
    assert_eq!(
        factory.command(tidied(None), 10),
        Ok(Answer::Changed(false))
    );

    // A dirty checkout stays, and the Task says why.
    let mut factory = Factory::in_memory();
    let (task, _) = at_merge(&mut factory);
    merge_report(&mut factory, SECOND, MergeOutcome::Merged, "MERGED").unwrap();
    factory.step(9, &BTreeMap::new());
    let why = "The worktree has uncommitted changes, so it was left in place.";
    factory
        .command(
            Command::WorktreeTidied {
                task: task.clone(),
                kept: Some(why.into()),
            },
            10,
        )
        .unwrap();
    assert_eq!(the(&factory, &task).delivery.kept.as_deref(), Some(why));
    assert!(the(&factory, &task).worktree.is_some());

    // Completed without a merged PR: nothing is removed, and it says why.
    let mut factory = Factory::in_memory();
    let (id, _) = saved(&mut factory, &[("Build", "Build it")]);
    let task = task_id(start_on(&mut factory, Some(&id), Some("acme/widgets#12")));
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);
    let effects = finished(&mut factory, FIRST, "Built");
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::Tidy { .. } | Effect::RemoveWorktree { .. })));
    assert_eq!(
        the(&factory, &task).delivery.kept.as_deref(),
        Some("The worktree stays: acme/widgets#12 is not confirmed merged.")
    );
}

/// The template merges after approval: the Merge agent's conflict goes
/// to the Conflict resolver, which hands back to review.
#[test]
fn the_template_merges_after_approval_and_resolves_conflicts_through_review() {
    let mut factory = Factory::in_memory();
    let id = create(&mut factory, Template::IssueToReviewedPr);
    let draft = workflow(&factory, &id).draft.clone();
    let named = |name: &str| draft.steps.iter().find(|step| step.name == name).unwrap();
    let (review, merge, resolve) = (
        named("Reviewer"),
        named("Merge agent"),
        named("Conflict resolver"),
    );
    assert_eq!(draft.approval.after, Destination::Step(merge.id.clone()));
    let routes = merge
        .outcome_routes
        .clone()
        .expect("routes by merge result");
    assert_eq!(routes.conflict, Destination::Step(resolve.id.clone()));
    assert_eq!(routes.merged, Destination::Complete);
    assert_eq!(resolve.on_finished, Destination::Step(review.id.clone()));
    assert_eq!(draft.problems(&Catalog::default()), Vec::<Problem>::new());
}

fn finished_report(factory: &mut Factory, worker: ThreadId) -> Result<Answer, Refusal> {
    factory.command(
        Command::Report {
            worker,
            outcome: Outcome::Finished {
                summary: "Done".into(),
            },
        },
        8,
    )
}

fn seen_at(thread: ThreadId, busy: bool, turns: u64) -> BTreeMap<ThreadId, Seen> {
    BTreeMap::from([(
        thread,
        Seen {
            busy,
            turns,
            decision: None,
            lost: false,
        },
    )])
}

/// Spec: only a confirmed merge completes. A redirect that skips the
/// merge Step leaves nothing merged, so the Task comes back to the
/// operator instead of completing.
#[test]
fn skipping_the_merge_step_never_completes_the_task_as_merged() {
    let mut factory = Factory::in_memory();
    let (id, steps) = merging(&mut factory);
    let task = task_id(start_on(&mut factory, Some(&id), Some("acme/widgets#12")));
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);
    factory
        .command(
            Command::Redirect {
                task: task.clone(),
                direction: Override {
                    to: steps[0].clone(),
                    skip: vec![steps[1].clone()],
                    then: ThenReturn::Routes,
                    note: String::new(),
                },
            },
            8,
        )
        .unwrap();
    factory.step(9, &BTreeMap::new());
    started(&mut factory, &task, SECOND);
    let effects = finished(&mut factory, SECOND, "Built");

    assert_eq!(status(&factory, &task), Status::NeedsHuman);
    assert_eq!(notified(&effects), [Status::NeedsHuman]);
    let the_task = the(&factory, &task);
    assert!(!the_task.delivery.pr.as_ref().unwrap().merged);
    let reason = the_task.reason.clone().unwrap_or_default();
    assert!(reason.contains("nothing was merged"), "{reason}");
}

/// A merge Step refuses `finished`: when it is asked to report, or
/// resumed, it is told the results it may report.
#[test]
fn a_merge_step_is_nudged_and_resumed_with_the_results_it_reports() {
    let mut factory = Factory::in_memory();
    let (task, _) = at_merge(&mut factory);
    let effects = factory.step(9, &seen_at(SECOND, false, 1));
    let told = prompts(&effects);
    assert_eq!(told.len(), 1, "{effects:?}");
    assert!(told[0].contains("report outcome merged"), "{}", told[0]);
    assert!(!told[0].contains("report outcome finished"), "{}", told[0]);

    factory
        .command(Command::Stop { task: task.clone() }, 10)
        .unwrap();
    factory
        .command(Command::Resume { task: task.clone() }, 11)
        .unwrap();
    let effects = factory.step(12, &seen_at(SECOND, false, 2));
    let told = prompts(&effects);
    assert_eq!(told.len(), 1, "{effects:?}");
    assert!(told[0].contains("resumed"), "{}", told[0]);
    assert!(told[0].contains("report outcome merged"), "{}", told[0]);
    assert!(!told[0].contains("report outcome finished"), "{}", told[0]);
}
