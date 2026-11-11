//! Workflows (#221) through the module's interface: authoring commands,
//! versions, and a Task routed through its Steps.

use std::collections::BTreeMap;

use super::*;

fn codex() -> Agent {
    Agent {
        provider: Provider::Codex,
        model: Some("gpt-codex".into()),
        effort: Some("high".into()),
    }
}

fn create(factory: &mut Factory, template: Template) -> WorkflowId {
    match factory.command(
        Command::CreateWorkflow {
            project: project(),
            template,
            agent: agent(),
        },
        0,
    ) {
        Ok(Answer::Workflow(id)) => id,
        other => panic!("{other:?}"),
    }
}

fn workflow<'a>(factory: &'a Factory, id: &str) -> &'a Workflow {
    factory.workflow(id).expect("the workflow")
}

fn names(factory: &Factory, id: &str) -> Vec<String> {
    workflow(factory, id)
        .draft
        .steps
        .iter()
        .map(|step| step.name.clone())
        .collect()
}

fn edit(factory: &mut Factory, id: &str, step: &str, change: StepChange) {
    factory
        .command(
            Command::EditStep {
                workflow: id.into(),
                step: step.into(),
                change,
            },
            1,
        )
        .unwrap();
}

fn add(factory: &mut Factory, id: &str, name: &str, instructions: &str) -> StepId {
    let Ok(Answer::Step(step)) = factory.command(
        Command::AddStep {
            workflow: id.into(),
            after: None,
            agent: agent(),
        },
        1,
    ) else {
        panic!("no step added");
    };
    edit(
        factory,
        id,
        &step,
        StepChange {
            name: Some(name.into()),
            instructions: Some(instructions.into()),
            ..Default::default()
        },
    );
    step
}

fn save(factory: &mut Factory, id: &str) -> Result<Answer, Refusal> {
    factory.command(
        Command::SaveWorkflow {
            workflow: id.into(),
            catalog: Catalog::default(),
        },
        5,
    )
}

fn run(factory: &mut Factory, id: &str, instructions: &str) -> TaskId {
    match factory.command(
        Command::Start {
            project: project(),
            instructions: instructions.into(),
            agent: agent(),
            request: None,
            workflow: Some(id.into()),
            pr: None,
        },
        6,
    ) {
        Ok(Answer::Task(task)) => task,
        other => panic!("{other:?}"),
    }
}

/// Builder: a Workflow named "Ship it" of Steps with instructions, the
/// last routing to Completed, saved as version 1.
fn saved(factory: &mut Factory, steps: &[(&str, &str)]) -> (WorkflowId, Vec<StepId>) {
    let id = create(factory, Template::Blank);
    factory
        .command(
            Command::RenameWorkflow {
                workflow: id.clone(),
                name: "Ship it".into(),
            },
            1,
        )
        .unwrap();
    let blank = workflow(factory, &id).draft.steps[0].id.clone();
    factory
        .command(
            Command::DeleteStep {
                workflow: id.clone(),
                step: blank,
            },
            1,
        )
        .unwrap();
    let mut ids = Vec::new();
    for (at, (name, instructions)) in steps.iter().enumerate() {
        let step = add(factory, &id, name, instructions);
        let last = at + 1 == steps.len();
        edit(
            factory,
            &id,
            &step,
            StepChange {
                on_finished: Some(if last {
                    Destination::Complete
                } else {
                    Destination::Next
                }),
                ..Default::default()
            },
        );
        ids.push(step);
    }
    assert_eq!(save(factory, &id), Ok(Answer::Version(1)));
    (id, ids)
}

/// The StartWorker effects among `effects`: (agent, worktree, prompt).
fn starts(effects: &[Effect]) -> Vec<(Agent, Option<PathBuf>, String)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::StartWorker {
                agent,
                worktree,
                prompt,
                ..
            } => Some((agent.clone(), worktree.clone(), prompt.clone())),
            _ => None,
        })
        .collect()
}

fn started(factory: &mut Factory, task: &str, thread: ThreadId) {
    factory
        .command(
            Command::WorkerStarted {
                task: task.into(),
                thread,
                worktree: Some("/repo/.worktrees/factory/t-1".into()),
            },
            7,
        )
        .unwrap();
}

fn finished(factory: &mut Factory, worker: ThreadId, summary: &str) -> Vec<Effect> {
    factory
        .command(
            Command::Report {
                worker,
                outcome: Outcome::Finished {
                    summary: summary.into(),
                },
            },
            8,
        )
        .unwrap();
    factory.step(9, &BTreeMap::new())
}

fn unable(factory: &mut Factory, worker: ThreadId, tried: &str) -> Vec<Effect> {
    factory
        .command(
            Command::Report {
                worker,
                outcome: Outcome::Unable {
                    tried: tried.into(),
                },
            },
            8,
        )
        .unwrap();
    factory.step(9, &BTreeMap::new())
}

const FIRST: ThreadId = ThreadId::new(41);
const SECOND: ThreadId = ThreadId::new(42);
const THIRD: ThreadId = ThreadId::new(43);

#[test]
fn workflows_are_made_blank_or_from_the_template_renamed_and_several_per_project() {
    let mut factory = Factory::in_memory();
    let blank = create(&mut factory, Template::Blank);
    let example = create(&mut factory, Template::IssueToReviewedPr);
    assert_ne!(blank, example);
    assert_eq!(names(&factory, &blank), ["Agent"]);
    assert_eq!(
        names(&factory, &example),
        [
            "Triager",
            "Explorer",
            "Implementer",
            "Reviewer",
            "Merge agent",
            "Conflict resolver"
        ]
    );
    assert_eq!(
        workflow(&factory, &example).draft.name,
        "Issue to reviewed PR"
    );
    factory
        .command(
            Command::RenameWorkflow {
                workflow: blank.clone(),
                name: "Docs fixes".into(),
            },
            1,
        )
        .unwrap();
    assert_eq!(workflow(&factory, &blank).draft.name, "Docs fixes");
    let board = factory.board();
    assert_eq!(board.workflows_of(project()).count(), 2);
    assert!(
        factory.step(2, &BTreeMap::new()).is_empty(),
        "authoring starts nothing"
    );
}

/// Steps are added, moved (by drag's index or a keyboard's one place) and
/// deleted; "next" follows the outline after a reorder.
#[test]
fn next_follows_the_outline_after_a_reorder() {
    let mut factory = Factory::in_memory();
    let (id, steps) = saved(
        &mut factory,
        &[
            ("Plan", "Plan it"),
            ("Build", "Build it"),
            ("Check", "Check it"),
        ],
    );
    let draft = &workflow(&factory, &id).draft;
    assert_eq!(
        draft.route_label(&steps[0], &Destination::Next),
        "Next agent \u{b7} Build"
    );
    factory
        .command(
            Command::MoveStep {
                workflow: id.clone(),
                step: steps[2].clone(),
                to: 1,
            },
            2,
        )
        .unwrap();
    assert_eq!(names(&factory, &id), ["Plan", "Check", "Build"]);
    let draft = &workflow(&factory, &id).draft;
    assert_eq!(
        draft.route_label(&steps[0], &Destination::Next),
        "Next agent \u{b7} Check"
    );
    // Check's next is now Build.
    assert_eq!(
        draft.route_label(&steps[2], &Destination::Next),
        "Next agent \u{b7} Build"
    );
}

/// A route to a deleted Step stays and is flagged; the draft lists every
/// problem and cannot be saved or run until fixed.
#[test]
fn a_draft_with_problems_lists_them_all_and_cannot_be_saved_or_run() {
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
            ..Default::default()
        },
    );
    factory
        .command(
            Command::DeleteStep {
                workflow: id.clone(),
                step: steps[0].clone(),
            },
            2,
        )
        .unwrap();
    let new = add(&mut factory, &id, "", "");
    let route = workflow(&factory, &id).draft.steps[0].on_unable.clone();
    assert_eq!(
        route,
        Destination::Step(steps[0].clone()),
        "kept, not rewired"
    );
    assert_eq!(
        workflow(&factory, &id).draft.route_label(&steps[1], &route),
        "Deleted agent"
    );

    let catalog = Catalog {
        models: vec![(
            Provider::Claude,
            vec![crate::ModelInfo::bare("claude-other")],
        )],
        skills: vec![(Provider::Claude, vec!["review".into()])],
    };
    edit(
        &mut factory,
        &id,
        &steps[1],
        StepChange {
            skills: Some(vec!["review".into(), "missing-skill".into()]),
            ..Default::default()
        },
    );
    edit(
        &mut factory,
        &id,
        &new,
        StepChange {
            on_finished: Some(Destination::Human),
            ..Default::default()
        },
    );
    edit(
        &mut factory,
        &id,
        &steps[1],
        StepChange {
            on_finished: Some(Destination::Next),
            ..Default::default()
        },
    );
    let problems: Vec<String> = workflow(&factory, &id)
        .draft
        .problems(&catalog)
        .into_iter()
        .map(|problem| problem.message)
        .collect();
    assert_eq!(
        problems,
        [
            "Review: claude-model is not available from Claude. Choose a model.",
            "Review: the skill missing-skill is not installed for Claude.",
            "Review \u{b7} If unable to finish points to a deleted agent.",
            "Name this agent.",
            "An unnamed agent has no instructions.",
            "An unnamed agent: claude-model is not available from Claude. Choose a model.",
            "No route ends in Completed. Route an outcome to Completed, or set After approval to mark the task complete.",
        ]
    );
    // Flagged, never replaced.
    let review = workflow(&factory, &id).draft.step(&steps[1]).unwrap();
    assert_eq!(review.agent.model.as_deref(), Some("claude-model"));
    assert_eq!(review.skills, ["review", "missing-skill"]);

    let refused = factory
        .command(
            Command::SaveWorkflow {
                workflow: id.clone(),
                catalog: catalog.clone(),
            },
            3,
        )
        .unwrap_err();
    assert_eq!(refused.code, RefusalCode::Invalid);
    assert!(
        refused.message.contains("Name this agent."),
        "{}",
        refused.message
    );
    assert_eq!(workflow(&factory, &id).versions.len(), 1, "still version 1");
}

/// A Workflow never saved cannot run; a saved one runs its latest version.
#[test]
fn an_unsaved_workflow_cannot_run() {
    let mut factory = Factory::in_memory();
    let id = create(&mut factory, Template::Blank);
    let refused = factory
        .command(
            Command::Start {
                project: project(),
                instructions: "Fix it".into(),
                agent: agent(),
                request: None,
                workflow: Some(id),
                pr: None,
            },
            1,
        )
        .unwrap_err();
    assert_eq!(refused.code, RefusalCode::Invalid);
    assert!(factory.board().tasks.is_empty());
}

/// Saving makes the next version and starts nothing; saving an unchanged
/// draft makes none.
#[test]
fn saving_numbers_versions_and_starts_nothing() {
    let mut factory = Factory::in_memory();
    let (id, steps) = saved(&mut factory, &[("Build", "Build it")]);
    assert!(factory.step(5, &BTreeMap::new()).is_empty());
    assert_eq!(save(&mut factory, &id), Ok(Answer::Version(1)), "unchanged");
    edit(
        &mut factory,
        &id,
        &steps[0],
        StepChange {
            instructions: Some("Build it well".into()),
            ..Default::default()
        },
    );
    assert!(workflow(&factory, &id).unsaved());
    assert_eq!(save(&mut factory, &id), Ok(Answer::Version(2)));
    assert_eq!(workflow(&factory, &id).versions.len(), 2);
    assert!(factory.step(6, &BTreeMap::new()).is_empty());
}

/// Each Step's Worker gets that Step's instructions, skills, Provider,
/// model and Effort, the Task's request and the earlier Steps' results,
/// in the Task's one worktree; one Step runs at a time; a custom-named
/// last Step's plain result completes the Task.
#[test]
fn a_task_runs_its_steps_in_order_with_their_context() {
    let mut factory = Factory::in_memory();
    let (id, steps) = saved(
        &mut factory,
        &[("Scout", "Map the code"), ("Writer", "Write the changelog")],
    );
    edit(
        &mut factory,
        &id,
        &steps[1],
        StepChange {
            skills: Some(vec!["changelog".into()]),
            provider: Some(Provider::Codex),
            model: Some("gpt-codex".into()),
            effort: Some("high".into()),
            ..Default::default()
        },
    );
    save(&mut factory, &id).unwrap();
    let task = run(&mut factory, &id, "Release notes for 2.0");
    let effects = factory.step(7, &BTreeMap::new());
    let [(with, tree, prompt)] = starts(&effects).try_into().unwrap();
    assert_eq!((with, tree), (agent(), None));
    assert!(prompt.contains("Map the code"), "{prompt}");
    assert!(prompt.contains("Release notes for 2.0"), "{prompt}");
    assert!(prompt.contains("Scout"), "{prompt}");
    started(&mut factory, &task, FIRST);
    assert_eq!(
        factory.task(&task).unwrap().step.as_deref(),
        Some(steps[0].as_str())
    );

    let effects = finished(&mut factory, FIRST, "Found three modules");
    assert!(notified(&effects).is_empty(), "silent between Steps");
    let [(with, tree, prompt)] = starts(&effects).try_into().unwrap();
    assert_eq!(with, codex());
    assert_eq!(
        tree,
        Some("/repo/.worktrees/factory/t-1".into()),
        "the same worktree"
    );
    assert!(prompt.contains("Write the changelog"), "{prompt}");
    assert!(prompt.contains("changelog"), "{prompt}");
    assert!(prompt.contains("Release notes for 2.0"), "{prompt}");
    assert!(
        prompt.contains("Found three modules"),
        "earlier result: {prompt}"
    );
    assert_eq!(status(&factory, &task), Status::Working);
    assert_eq!(factory.task(&task).unwrap().agent, codex());
    started(&mut factory, &task, SECOND);

    let stale = factory.command(
        Command::Report {
            worker: FIRST,
            outcome: Outcome::Finished {
                summary: "again".into(),
            },
        },
        10,
    );
    assert_eq!(stale, Ok(Answer::Changed(false)), "one Step at a time");

    let effects = finished(&mut factory, SECOND, "Wrote CHANGELOG.md");
    assert_eq!(notified(&effects), [Status::Completed]);
    let done = factory.task(&task).unwrap();
    assert_eq!(done.status, Status::Completed);
    assert_eq!(done.result.as_deref(), Some("Wrote CHANGELOG.md"));
}

/// A Task keeps the version it started with when the Workflow is edited
/// and saved again.
#[test]
fn a_running_task_keeps_its_version() {
    let mut factory = Factory::in_memory();
    let (id, steps) = saved(
        &mut factory,
        &[("Build", "Build it"), ("Check", "Check it")],
    );
    let task = run(&mut factory, &id, "Do it");
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);
    edit(
        &mut factory,
        &id,
        &steps[1],
        StepChange {
            instructions: Some("Check it twice".into()),
            ..Default::default()
        },
    );
    assert_eq!(save(&mut factory, &id), Ok(Answer::Version(2)));
    let effects = finished(&mut factory, FIRST, "Built");
    let [(_, _, prompt)] = starts(&effects).try_into().unwrap();
    assert!(prompt.contains("Check it"), "{prompt}");
    assert!(!prompt.contains("twice"), "{prompt}");
    let running = factory.task(&task).unwrap();
    assert_eq!(running.workflow.as_ref().map(|run| run.version), Some(1));
}

/// Returning to a visited Step continues its Worker with what happened
/// since; with "Start fresh every time" a new Worker starts and the old
/// one is parked. Unable routes to a named Step.
#[test]
fn a_returning_task_continues_or_starts_fresh() {
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

    // Review cannot finish: back to Build's own Worker, told why.
    let effects = unable(&mut factory, SECOND, "The flag has no test");
    assert!(starts(&effects).is_empty(), "no new Worker: {effects:?}");
    assert_eq!(factory.task(&task).unwrap().worker, Some(FIRST));
    let effects = factory.step(
        10,
        &BTreeMap::from([(
            FIRST,
            Seen {
                busy: false,
                turns: 1,
                decision: None,
                lost: false,
            },
        )]),
    );
    let told = prompts(&effects);
    assert_eq!(told.len(), 1, "{effects:?}");
    assert!(told[0].contains("The flag has no test"), "{}", told[0]);

    // Build finishes again: Review starts fresh, its old Worker parked.
    let effects = finished(&mut factory, FIRST, "Added the test");
    assert!(
        effects.contains(&Effect::Park { thread: SECOND }),
        "{effects:?}"
    );
    let [(_, _, prompt)] = starts(&effects).try_into().unwrap();
    assert!(prompt.contains("Review it"), "{prompt}");
    assert!(prompt.contains("Added the test"), "{prompt}");
    started(&mut factory, &task, THIRD);
    assert_eq!(factory.task(&task).unwrap().worker, Some(THIRD));
}

/// Needs human and the Coordinator routes stop for the operator; a route
/// to a deleted Step in a running version cannot occur (it could not be
/// saved), and Unable defaults to Needs human with what was tried.
#[test]
fn human_and_coordinator_routes_wait_for_the_operator() {
    let mut factory = Factory::in_memory();
    let (id, steps) = saved(
        &mut factory,
        &[("Build", "Build it"), ("Check", "Check it")],
    );
    edit(
        &mut factory,
        &id,
        &steps[0],
        StepChange {
            on_finished: Some(Destination::Coordinator),
            ..Default::default()
        },
    );
    edit(
        &mut factory,
        &id,
        &steps[1],
        StepChange {
            on_finished: Some(Destination::Complete),
            ..Default::default()
        },
    );
    save(&mut factory, &id).unwrap();
    let task = run(&mut factory, &id, "One");
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);
    let effects = finished(&mut factory, FIRST, "Built");
    assert_eq!(notified(&effects), [Status::NeedsHuman]);
    let stuck = factory.task(&task).unwrap();
    assert!(stuck.reason.as_deref().unwrap().contains("Coordinator"));

    let other = run(&mut factory, &id, "Two");
    factory.step(11, &BTreeMap::new());
    started(&mut factory, &other, SECOND);
    let effects = unable(&mut factory, SECOND, "No access");
    assert_eq!(notified(&effects), [Status::NeedsHuman]);
    assert_eq!(
        factory.task(&other).unwrap().reason.as_deref(),
        Some("No access")
    );
}

/// Deleting a Task parks every Worker it had.
#[test]
fn deleting_a_task_deletes_all_its_workers() {
    let mut factory = Factory::in_memory();
    let (id, _) = saved(
        &mut factory,
        &[("Build", "Build it"), ("Check", "Check it")],
    );
    let task = run(&mut factory, &id, "Do it");
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);
    finished(&mut factory, FIRST, "Built");
    started(&mut factory, &task, SECOND);
    factory
        .command(Command::Stop { task: task.clone() }, 10)
        .unwrap();
    factory.step(11, &BTreeMap::new());
    factory
        .command(Command::Delete { task: task.clone() }, 12)
        .unwrap();
    let effects = factory.step(13, &BTreeMap::new());
    assert!(
        effects.contains(&Effect::DeleteThread { thread: FIRST }),
        "{effects:?}"
    );
    assert!(
        effects.contains(&Effect::DeleteThread { thread: SECOND }),
        "{effects:?}"
    );
}

/// Workflows are written with the Factory and read back.
#[test]
fn workflows_survive_a_reload() {
    let dir = std::env::temp_dir().join(format!("factory-workflows-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("factory.json");
    let mut factory = Factory::load(&path).unwrap();
    let (id, _) = saved(&mut factory, &[("Build", "Build it")]);
    let reread = Factory::load(&path).unwrap();
    assert_eq!(reread.workflow(&id), factory.workflow(&id));
    let mut reread = reread;
    assert_ne!(
        create(&mut reread, Template::Blank),
        id,
        "ids are never given twice"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

mod direction;
mod merging;
mod coordinator;
