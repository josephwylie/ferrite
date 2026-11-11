//! The Factory's Coordinator surface (#224): empty until the operator
//! speaks, then the Coordinator's ordinary Pane; its settings; and its
//! proposals as cards that change nothing until accepted. A fixture
//! world's own Factory answers as a Server's would (it starts no Thread).
#[allow(unused_imports)]
use super::*;
use crate::cockpit::workflows::Surface;
use ferrite_core::factory::{Act, Catalog, Command, ProposalStatus};

fn shows(cx: &mut gpui::VisualTestContext, selector: &str) -> bool {
    debug_bounds(cx, selector.to_string()).is_some()
}

fn click(cx: &mut gpui::VisualTestContext, selector: &str) {
    let at = bounds(cx, selector.to_string());
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
}

/// A fixture world with a live Factory: the core, the Project, and the
/// operator's own Thread.
fn world(name: &str) -> (Cockpit, ProjectId, ThreadId) {
    let fake = Fake::default();
    let dir = scratch(name);
    let mut core = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));
    let mine = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    did_something_now(&dir, mine);
    let project = core.project_id(mine).unwrap();
    core.fixture_live_factory();
    (core, project, mine)
}

fn window<'a>(
    cx: &'a mut TestAppContext,
    core: Cockpit,
    project: ProjectId,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    view.update(cx, |view, cx| view.open_factory(project, cx));
    tick(cx);
    click(cx, "factory-tab-coordinator");
    (view, cx)
}

/// AC: a fresh Coordinator is an empty conversation with a field; its
/// Provider, model and effort are chosen here and kept by the Factory;
/// sending the first words starts it (the Factory hears them) and starts
/// no Task.
#[gpui::test]
fn the_coordinator_is_empty_until_the_operator_speaks(cx: &mut TestAppContext) {
    let (core, project, _) = world("coordinator-empty");
    let (view, cx) = window(cx, core, project);
    assert!(shows(cx, "coordinator-surface"));
    assert!(shows(cx, "coordinator-empty"));
    assert!(shows(cx, "coordinator-field"));
    assert!(!shows(cx, "coordinator-pane"));
    assert!(!shows(cx, "coordinator-proposals"), "no proposals yet");
    for chooser in [
        "coordinator-provider",
        "coordinator-model",
        "coordinator-effort",
    ] {
        assert!(shows(cx, chooser), "{chooser}");
    }

    // Settings: Codex, chosen from the provider chooser.
    click(cx, "coordinator-provider");
    assert!(shows(cx, "context-menu"));
    let codex = view.read_with(cx, |view, _| {
        view.factory_view
            .as_ref()
            .unwrap()
            .picks
            .iter()
            .position(|(_, pick)| {
                *pick
                    == crate::cockpit::workflows::Pick::Coordinator(
                        crate::cockpit::coordinator::CoordinatorPick::Provider(Provider::Codex),
                    )
            })
            .unwrap()
    });
    click(cx, &format!("context-menu-row-{codex}"));
    view.read_with(cx, |view, _| {
        let agent = view
            .cockpit
            .factory()
            .coordinator(project)
            .unwrap()
            .agent
            .clone();
        assert_eq!(agent.map(|agent| agent.provider), Some(Provider::Codex));
        assert_eq!(view.coordinator_agent().provider, Provider::Codex);
    });

    // The first words start it.
    click(cx, "coordinator-field");
    cx.simulate_input("What is left on the board?");
    tick(cx);
    click(cx, "coordinator-send");
    view.read_with(cx, |view, cx| {
        let board = view.cockpit.factory();
        let coordinator = board.coordinator(project).unwrap();
        assert!(coordinator.starting, "the Factory starts it");
        assert!(board.tasks.is_empty(), "talking starts nothing");
        let field = view.factory_view.as_ref().unwrap().opening.clone().unwrap();
        assert_eq!(field.read(cx).text(), "", "sent");
    });
}

/// AC: once it runs, the Coordinator's ordinary Pane is the conversation
/// (an owned Thread, not the operator's); each proposal is a card saying
/// exactly what will happen; accepting runs it and marks the card, with a
/// way to the Task it started; dismissing changes nothing.
#[gpui::test]
fn proposals_are_cards_accepted_or_dismissed_beside_the_conversation(cx: &mut TestAppContext) {
    let (mut core, project, _) = world("coordinator-cards");
    let coordinator = core
        .fixture_open_owned(
            Provider::Claude,
            WorkspaceChoice::Main { checkout: here() },
            ferrite_core::ThreadOwner::Factory(project),
        )
        .unwrap();
    core.fixture_factory_command(Command::CoordinatorStarted {
        project,
        thread: coordinator,
    });
    for act in [
        Act::Start {
            instructions: "Fix the toolbar overflow\nOn narrow windows.".into(),
            workflow: None,
        },
        Act::Start {
            instructions: "Tidy the README".into(),
            workflow: None,
        },
    ] {
        core.fixture_factory_command(Command::Propose {
            coordinator,
            act,
            catalog: Catalog::default(),
        });
    }
    let (view, cx) = window(cx, core, project);
    assert!(shows(cx, "coordinator-pane"), "its ordinary Pane");
    assert!(!shows(cx, "coordinator-empty"));
    view.read_with(cx, |view, _| {
        assert!(
            !view.cockpit.operators(coordinator),
            "not the operator's own"
        );
        assert_eq!(view.coordinator_drawn(), Some(coordinator));
    });
    assert!(shows(cx, "coordinator-proposals"));
    assert!(shows(cx, "proposal-p-1") && shows(cx, "proposal-p-2"));
    assert!(shows(cx, "proposal-p-1-line-0") && shows(cx, "proposal-p-1-line-2"));
    assert!(shows(cx, "proposal-p-1-accept") && shows(cx, "proposal-p-1-dismiss"));
    assert!(shows(cx, "factory-tab-coordinator"));

    click(cx, "proposal-p-2-dismiss");
    assert!(shows(cx, "proposal-p-2-dismissed"));
    assert!(!shows(cx, "proposal-p-2-accept"));
    view.read_with(cx, |view, _| {
        assert!(
            view.cockpit.factory().tasks.is_empty(),
            "dismissed: nothing"
        );
    });

    click(cx, "proposal-p-1-accept");
    let task = view.read_with(cx, |view, _| {
        let board = view.cockpit.factory();
        let card = board.proposal("p-1").unwrap();
        assert_eq!(card.status, ProposalStatus::Accepted);
        assert_eq!(board.tasks.len(), 1, "accepted: the Task");
        assert_eq!(board.tasks[0].title, "Fix the toolbar overflow");
        card.task.clone().unwrap()
    });
    assert!(shows(cx, "proposal-p-1-accepted"));
    assert!(!shows(cx, "proposal-p-1-accept"), "accepted once");

    click(cx, "proposal-p-1-open");
    view.read_with(cx, |view, _| {
        let factory = view.factory_view.as_ref().unwrap();
        assert_eq!(factory.surface, Surface::Board);
        assert_eq!(factory.open.as_deref(), Some(task.as_str()));
    });
}
