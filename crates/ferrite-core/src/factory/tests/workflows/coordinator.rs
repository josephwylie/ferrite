//! The Coordinator and its proposals (#224) through the module's
//! interface: talking to it starts its Thread once with its briefing and
//! starts nothing else; its settings reach its Session; the four acts
//! are proposals that change nothing until the operator accepts them and
//! nothing when dismissed; a Task routed to it waits for the operator.

use super::*;

const COORDINATOR: ThreadId = ThreadId::new(60);

fn say(factory: &mut Factory, text: &str) -> Result<Answer, Refusal> {
    factory.command(
        Command::CoordinatorSay {
            project: project(),
            text: text.into(),
        },
        1,
    )
}

/// The Coordinator starts: (agent, prompt) of each start among `effects`.
fn coordinator_starts(effects: &[Effect]) -> Vec<(Option<Agent>, String)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::StartCoordinator {
                project: at,
                agent,
                prompt,
            } if *at == project() => Some((agent.clone(), prompt.clone())),
            _ => None,
        })
        .collect()
}

/// The Coordinator up as `COORDINATOR`, having been told `first`.
fn coordinating(factory: &mut Factory) {
    say(factory, "Hello").unwrap();
    factory.step(2, &BTreeMap::new());
    factory
        .command(
            Command::CoordinatorStarted {
                project: project(),
                thread: COORDINATOR,
            },
            3,
        )
        .unwrap();
    factory.step(4, &BTreeMap::new());
}

fn propose(factory: &mut Factory, act: Act) -> Result<Answer, Refusal> {
    factory.command(
        Command::Propose {
            coordinator: COORDINATOR,
            act,
            catalog: Catalog::default(),
        },
        10,
    )
}

fn proposed(factory: &mut Factory, act: Act) -> ProposalId {
    match propose(factory, act) {
        Ok(Answer::Proposal(id)) => id,
        other => panic!("{other:?}"),
    }
}

fn accept(factory: &mut Factory, proposal: &str) -> Result<Answer, Refusal> {
    factory.command(
        Command::AcceptProposal {
            proposal: proposal.into(),
            agent: agent(),
            catalog: Catalog::default(),
        },
        11,
    )
}

fn dismiss(factory: &mut Factory, proposal: &str) -> Result<Answer, Refusal> {
    factory.command(
        Command::DismissProposal {
            proposal: proposal.into(),
        },
        11,
    )
}

fn proposal<'a>(board: &'a Board, id: &str) -> &'a Proposal {
    board.proposal(id).expect("the proposal")
}

/// A Workflow with an automatic start (the operator's own script), saved.
fn scripted(factory: &mut Factory) -> WorkflowId {
    let (id, _) = saved(factory, &[("Build", "Build it")]);
    factory
        .command(
            Command::EditStart {
                workflow: id.clone(),
                start: Start {
                    kind: StartKind::External,
                    ..Default::default()
                },
            },
            1,
        )
        .unwrap();
    assert_eq!(save(factory, &id), Ok(Answer::Version(2)));
    id
}

/// AC: a fresh Coordinator is nothing until the operator speaks; then its
/// Thread starts once, in the Project, with the briefing and what was
/// said, on the operator's default (no setting); what is said while it
/// starts reaches it once it has, and later words go straight to it.
/// Talking starts no Task.
#[test]
fn talking_starts_the_coordinator_once_with_its_briefing_and_nothing_else() {
    let mut factory = Factory::in_memory();
    assert!(factory.board().coordinator(project()).is_none());
    assert!(factory.step(1, &BTreeMap::new()).is_empty());

    assert_eq!(say(&mut factory, "What is left?"), Ok(Answer::Changed(true)));
    assert_eq!(say(&mut factory, "And the docs?"), Ok(Answer::Changed(true)));
    let effects = factory.step(2, &BTreeMap::new());
    let starts = coordinator_starts(&effects);
    assert_eq!(starts.len(), 1, "{effects:?}");
    let (agent_chosen, prompt) = &starts[0];
    assert_eq!(*agent_chosen, None, "the operator's default");
    assert!(prompt.contains("Coordinator"), "{prompt}");
    assert!(prompt.ends_with("What is left?"), "{prompt}");

    factory
        .command(
            Command::CoordinatorStarted {
                project: project(),
                thread: COORDINATOR,
            },
            3,
        )
        .unwrap();
    let effects = factory.step(4, &BTreeMap::new());
    assert_eq!(
        effects,
        [Effect::Prompt {
            thread: COORDINATOR,
            text: "And the docs?".into()
        }]
    );
    say(&mut factory, "Thanks").unwrap();
    let effects = factory.step(5, &BTreeMap::new());
    assert_eq!(
        effects,
        [Effect::Prompt {
            thread: COORDINATOR,
            text: "Thanks".into()
        }]
    );
    let board = factory.board();
    assert_eq!(
        board.coordinator(project()).and_then(|c| c.thread),
        Some(COORDINATOR)
    );
    assert!(board.tasks.is_empty(), "talking starts nothing");
    assert!(board.coordinator(project_id(2)).is_none());
    assert!(say(&mut factory, "  ").is_err(), "nothing said");
}

/// A Coordinator that could not start says why and starts on the next
/// word.
#[test]
fn a_coordinator_that_could_not_start_says_why_and_tries_again() {
    let mut factory = Factory::in_memory();
    say(&mut factory, "Hello").unwrap();
    factory.step(2, &BTreeMap::new());
    factory
        .command(
            Command::CoordinatorFailed {
                project: project(),
                why: "no claude".into(),
            },
            3,
        )
        .unwrap();
    let board = factory.board();
    let coordinator = board.coordinator(project()).unwrap();
    assert_eq!(coordinator.error.as_deref(), Some("no claude"));
    say(&mut factory, "Again").unwrap();
    let starts = coordinator_starts(&factory.step(4, &BTreeMap::new()));
    assert_eq!(starts.len(), 1);
}

/// AC: the Coordinator's Provider, model and effort are the Factory's:
/// chosen before it starts, it starts on them; changed after, its Session
/// is tuned for its next turn.
#[test]
fn the_coordinators_settings_reach_its_start_and_then_its_session() {
    let mut factory = Factory::in_memory();
    let chosen = codex();
    factory
        .command(
            Command::CoordinatorAgent {
                project: project(),
                agent: chosen.clone(),
            },
            1,
        )
        .unwrap();
    say(&mut factory, "Hello").unwrap();
    let starts = coordinator_starts(&factory.step(2, &BTreeMap::new()));
    assert_eq!(starts[0].0, Some(chosen.clone()));
    factory
        .command(
            Command::CoordinatorStarted {
                project: project(),
                thread: COORDINATOR,
            },
            3,
        )
        .unwrap();
    factory.step(4, &BTreeMap::new());

    let changed = agent();
    assert_eq!(
        factory.command(
            Command::CoordinatorAgent {
                project: project(),
                agent: changed.clone(),
            },
            5,
        ),
        Ok(Answer::Changed(true))
    );
    assert_eq!(
        factory.step(6, &BTreeMap::new()),
        [Effect::Tune {
            thread: COORDINATOR,
            agent: changed.clone()
        }]
    );
    assert_eq!(
        factory.command(
            Command::CoordinatorAgent {
                project: project(),
                agent: changed,
            },
            7,
        ),
        Ok(Answer::Changed(false)),
        "the same again changes nothing"
    );
    assert!(factory.step(8, &BTreeMap::new()).is_empty());
}

/// AC: the briefing teaches reading, drafting and proposing, and that it
/// never acts without the operator.
#[test]
fn the_briefing_teaches_reading_drafting_proposing_and_never_acting_alone() {
    let mut factory = Factory::in_memory();
    say(&mut factory, "Hello").unwrap();
    let (_, prompt) = coordinator_starts(&factory.step(2, &BTreeMap::new())).remove(0);
    for taught in [
        "task list",
        "task get",
        "workflow get",
        "step edit",
        "proposal raise start",
        "proposal raise save",
        "proposal raise automation",
        "proposal raise redirect",
        "--skill",
        "never",
    ] {
        assert!(prompt.contains(taught), "{taught}: {prompt}");
    }
}

/// AC: each of the four acts is a proposal: raising it changes nothing
/// but the proposals, and nothing runs; dismissing changes nothing;
/// accepting runs the act and marks the card; a second accept does
/// nothing more. A one-off handoff needs no Workflow.
#[test]
fn each_proposal_changes_nothing_until_accepted_and_nothing_when_dismissed() {
    let mut factory = Factory::in_memory();
    coordinating(&mut factory);
    let (built, steps) = saved(&mut factory, &[("Build", "Build it"), ("Check", "Check it")]);
    let scripts = scripted(&mut factory);
    let running = run(&mut factory, &built, "Running");
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &running, FIRST);
    edit(
        &mut factory,
        &built,
        &steps[1],
        StepChange {
            instructions: Some("Check it twice".into()),
            ..Default::default()
        },
    );
    let before = factory.board();

    let acts = [
        Act::Start {
            instructions: "Fix the toolbar overflow\nOn narrow windows.".into(),
            workflow: None,
        },
        Act::Start {
            instructions: "Ship the docs".into(),
            workflow: Some(built.clone()),
        },
        Act::Save {
            workflow: built.clone(),
            version: 2,
            draft: workflow(&factory, &built).draft.clone(),
        },
        Act::Automation {
            workflow: scripts.clone(),
        },
        Act::Redirect {
            task: running.clone(),
            direction: Override {
                to: steps[1].clone(),
                skip: Vec::new(),
                then: ThenReturn::Me,
                note: "Only the docs changed".into(),
            },
        },
    ];
    let mut ids = Vec::new();
    for act in acts {
        ids.push(proposed(&mut factory, act));
    }
    let raised = factory.board();
    assert_eq!(raised.tasks, before.tasks, "raising starts nothing");
    assert_eq!(raised.workflows, before.workflows, "and saves nothing");
    assert!(factory.step(12, &BTreeMap::new()).is_empty());
    for id in &ids {
        assert_eq!(proposal(&raised, id).status, ProposalStatus::Pending);
    }
    assert_eq!(raised.proposals_of(project()).count(), 5);

    // Exactly what will happen, as the card says it.
    let lines = proposal(&raised, &ids[0]).lines(&raised);
    assert!(lines.contains(&("Task".into(), "Fix the toolbar overflow".into())), "{lines:?}");
    assert!(lines.contains(&("Process".into(), "One-off handoff".into())), "{lines:?}");
    let lines = proposal(&raised, &ids[1]).lines(&raised);
    assert!(lines.contains(&("Process".into(), "Ship it \u{b7} version 1".into())), "{lines:?}");
    let lines = proposal(&raised, &ids[4]).lines(&raised);
    assert!(lines.contains(&("Send to".into(), "Check".into())), "{lines:?}");
    assert!(lines.contains(&("Feedback".into(), "Only the docs changed".into())), "{lines:?}");

    // Dismissed: nothing changes, and it stays dismissed.
    for id in &ids {
        assert_eq!(dismiss(&mut factory, id), Ok(Answer::Changed(true)));
    }
    assert_eq!(dismiss(&mut factory, &ids[0]), Ok(Answer::Changed(false)));
    let dismissed = factory.board();
    assert_eq!(dismissed.tasks, before.tasks);
    assert_eq!(dismissed.workflows, before.workflows);
    assert!(factory.step(13, &BTreeMap::new()).is_empty());
    assert_eq!(
        accept(&mut factory, &ids[0]).map_err(|refusal| refusal.code),
        Err(RefusalCode::NotNow),
        "a dismissed proposal is not accepted"
    );

    // Raised again and accepted: each act runs as the operator's.
    let one_off = proposed(
        &mut factory,
        Act::Start {
            instructions: "Fix the toolbar overflow".into(),
            workflow: None,
        },
    );
    let Ok(Answer::Task(task)) = accept(&mut factory, &one_off) else {
        panic!("no task");
    };
    let board = factory.board();
    let made = board.task(&task).unwrap();
    assert!(made.workflow.is_none(), "one-off");
    assert_eq!(made.agent, agent());
    assert_eq!(made.activity[0].by, Actor::Operator);
    let card = proposal(&board, &one_off);
    assert_eq!(card.status, ProposalStatus::Accepted);
    assert_eq!(card.task.as_deref(), Some(task.as_str()));
    assert_eq!(accept(&mut factory, &one_off), Ok(Answer::Changed(false)));
    assert_eq!(factory.board().tasks.len(), board.tasks.len(), "accepted once");
    assert_eq!(dismiss(&mut factory, &one_off).map_err(|r| r.code), Err(RefusalCode::NotNow));

    let draft = workflow(&factory, &built).draft.clone();
    let save_it = proposed(
        &mut factory,
        Act::Save {
            workflow: built.clone(),
            version: 2,
            draft,
        },
    );
    assert_eq!(accept(&mut factory, &save_it), Ok(Answer::Version(2)));
    let turn_on = proposed(
        &mut factory,
        Act::Automation {
            workflow: scripts.clone(),
        },
    );
    accept(&mut factory, &turn_on).unwrap();
    assert_eq!(workflow(&factory, &scripts).automation.switch, Switch::On);
    let send_back = proposed(
        &mut factory,
        Act::Redirect {
            task: running.clone(),
            direction: Override {
                to: steps[1].clone(),
                skip: Vec::new(),
                then: ThenReturn::Me,
                note: String::new(),
            },
        },
    );
    accept(&mut factory, &send_back).unwrap();
    let redirected = factory.task(&running).unwrap();
    assert_eq!(redirected.step.as_deref(), Some(steps[1].as_str()));
    assert!(redirected.oversight.to_me);
}

/// The same proposal again, while it waits, is the one card.
#[test]
fn a_repeated_proposal_is_the_same_card() {
    let mut factory = Factory::in_memory();
    coordinating(&mut factory);
    let act = Act::Start {
        instructions: "Fix it".into(),
        workflow: None,
    };
    let first = proposed(&mut factory, act.clone());
    assert_eq!(proposed(&mut factory, act), first);
    assert_eq!(factory.board().proposals.len(), 1);
}

/// A proposal the Factory would refuse is refused when raised, so no card
/// promises what cannot happen; only a Coordinator raises one.
#[test]
fn a_proposal_that_cannot_happen_is_refused_when_raised() {
    let mut factory = Factory::in_memory();
    coordinating(&mut factory);
    let (built, steps) = saved(&mut factory, &[("Build", "Build it")]);
    let code = |result: Result<Answer, Refusal>| result.map_err(|refusal| refusal.code);

    assert_eq!(
        code(factory.command(
            Command::Propose {
                coordinator: FIRST,
                act: Act::Start {
                    instructions: "x".into(),
                    workflow: None
                },
                catalog: Catalog::default(),
            },
            1,
        )),
        Err(RefusalCode::NotACoordinator)
    );
    let unsaved = create(&mut factory, Template::Blank);
    for act in [
        Act::Start {
            instructions: " ".into(),
            workflow: None,
        },
        Act::Start {
            instructions: "x".into(),
            workflow: Some(unsaved.clone()),
        },
        Act::Save {
            workflow: built.clone(),
            version: 2,
            draft: workflow(&factory, &built).draft.clone(),
        },
        Act::Automation {
            workflow: built.clone(),
        },
    ] {
        assert!(propose(&mut factory, act.clone()).is_err(), "{act:?}");
    }
    assert_eq!(
        code(propose(
            &mut factory,
            Act::Start {
                instructions: "x".into(),
                workflow: Some("w-99".into()),
            }
        )),
        Err(RefusalCode::UnknownWorkflow)
    );
    let task = run(&mut factory, &built, "Running");
    assert_eq!(
        code(propose(
            &mut factory,
            Act::Redirect {
                task,
                direction: Override {
                    to: "s-99".into(),
                    skip: Vec::new(),
                    then: ThenReturn::Routes,
                    note: String::new(),
                },
            }
        )),
        Err(RefusalCode::UnknownStep)
    );
    let _ = steps;
    assert!(factory.board().proposals.is_empty());
}

/// A save proposal holds the draft it showed: a draft changed since is
/// not saved by accepting it.
#[test]
fn a_save_proposal_saves_only_the_draft_it_showed() {
    let mut factory = Factory::in_memory();
    coordinating(&mut factory);
    let (built, steps) = saved(&mut factory, &[("Build", "Build it")]);
    edit(
        &mut factory,
        &built,
        &steps[0],
        StepChange {
            instructions: Some("Build it well".into()),
            ..Default::default()
        },
    );
    let draft = workflow(&factory, &built).draft.clone();
    let save_it = proposed(
        &mut factory,
        Act::Save {
            workflow: built.clone(),
            version: 2,
            draft,
        },
    );
    edit(
        &mut factory,
        &built,
        &steps[0],
        StepChange {
            instructions: Some("Build it badly".into()),
            ..Default::default()
        },
    );
    assert_eq!(
        accept(&mut factory, &save_it).map_err(|refusal| refusal.code),
        Err(RefusalCode::NotNow)
    );
    assert_eq!(workflow(&factory, &built).versions.len(), 1);
}

/// A route to the Coordinator holds the Task for the operator, and the
/// Coordinator (started for it when it has not been) is told the Task
/// and what was reported, to discuss with the operator.
#[test]
fn a_task_routed_to_the_coordinator_waits_and_the_coordinator_is_told() {
    let mut factory = Factory::in_memory();
    let (id, steps) = saved(&mut factory, &[("Build", "Build it"), ("Check", "Check it")]);
    edit(
        &mut factory,
        &id,
        &steps[0],
        StepChange {
            on_finished: Some(Destination::Coordinator),
            ..Default::default()
        },
    );
    save(&mut factory, &id).unwrap();
    let task = run(&mut factory, &id, "One");
    factory.step(7, &BTreeMap::new());
    started(&mut factory, &task, FIRST);
    let effects = finished(&mut factory, FIRST, "Built, but the API is unclear");
    assert_eq!(notified(&effects), [Status::NeedsHuman]);
    let held = factory.task(&task).unwrap();
    let reason = held.reason.clone().unwrap();
    assert!(reason.contains("Coordinator"), "{reason}");
    assert!(!reason.contains("does not run"), "{reason}");
    let starts = coordinator_starts(&effects);
    assert_eq!(starts.len(), 1, "{effects:?}");
    assert!(starts[0].1.contains(&task), "{}", starts[0].1);
    assert!(starts[0].1.contains("Built, but the API is unclear"));
    assert!(
        factory.step(10, &BTreeMap::new()).is_empty(),
        "told once"
    );
}

/// The Coordinator, its settings and its proposals are written with the
/// Factory and read back.
#[test]
fn the_coordinator_and_proposals_survive_a_reload() {
    let dir = std::env::temp_dir().join(format!("factory-coordinator-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("factory.json");
    let mut factory = Factory::load(&path).unwrap();
    coordinating(&mut factory);
    let id = proposed(
        &mut factory,
        Act::Start {
            instructions: "Fix it".into(),
            workflow: None,
        },
    );
    let reread = Factory::load(&path).unwrap();
    assert_eq!(reread.board().proposals, factory.board().proposals);
    assert_eq!(
        reread.board().coordinator(project()).and_then(|c| c.thread),
        Some(COORDINATOR)
    );
    let mut reread = reread;
    let second = proposed(
        &mut reread,
        Act::Start {
            instructions: "Another".into(),
            workflow: None,
        },
    );
    assert_ne!(second, id, "ids are never given twice");
    std::fs::remove_dir_all(&dir).unwrap();
}
