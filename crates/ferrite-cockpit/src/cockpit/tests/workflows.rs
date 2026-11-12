//! A Project's Workflows in the window (#221, ADR 0037): the outline and
//! its reorder by drag and keyboard, the inspector and its pickers, the
//! draft's problems, Save, and typed text that survives what the operator
//! does around it. A fixture world's own Factory answers every edit as a
//! Server's would.
#[allow(unused_imports)]
use super::*;
use crate::cockpit::workflows::{Field, Pick, StepDrag, Surface};
use ferrite_core::factory::{Destination, RouteKind, Template};

fn shows(cx: &mut gpui::VisualTestContext, selector: &str) -> bool {
    debug_bounds(cx, selector.to_string()).is_some()
}

fn click(cx: &mut gpui::VisualTestContext, selector: &str) {
    let at = bounds(cx, selector.to_string());
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
}

/// A window on a fixture world whose Factory answers, open on the
/// Project's Workflows.
fn workflows_window<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    fake_setup: impl FnOnce(&Fake),
) -> (
    Entity<CockpitView>,
    &'a mut gpui::VisualTestContext,
    Fake,
    ProjectId,
) {
    // Set up before the Cockpit is made: its Engine asks for models then.
    let fake = Fake::default();
    fake_setup(&fake);
    let dir = scratch(name);
    let mut core = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
    let thread = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    did_something_now(&dir, thread);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_live_factory();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    view.update(cx, |view, cx| view.open_factory(project, cx));
    tick(cx);
    (view, cx, fake, project)
}

fn names(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, _| {
        view.shown_workflow()
            .unwrap()
            .draft
            .steps
            .iter()
            .map(|step| step.name.clone())
            .collect()
    })
}

fn selected(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, _| view.shown_step().unwrap().name.clone())
}

/// AC: Workflows are made Blank or from the template from the Workflows
/// surface, several per Project, and renamed by typing; Steps are added,
/// chosen, reordered by keyboard and by drag ("Next agent" follows), and
/// deleted from their row; nothing starts.
#[gpui::test]
fn the_outline_is_shaped_by_mouse_and_keyboard(cx: &mut TestAppContext) {
    let (view, cx, fake, project) = workflows_window(cx, "workflows-outline", |_| {});
    // The template's six agents, the Merge agent's seven routes, all in view.
    cx.simulate_resize(gpui::size(px(1600.), px(1800.)));
    tick(cx);
    let streams = fake.streams.borrow().len();
    click(cx, "factory-tab-workflows");
    assert!(shows(cx, "workflows-empty"), "a fresh Project has none");

    click(cx, "workflow-new");
    assert!(shows(cx, "context-menu"), "the template picker");
    click(cx, "context-menu-row-1");
    assert_eq!(
        names(&view, cx),
        [
            "Triager",
            "Explorer",
            "Implementer",
            "Reviewer",
            "Merge agent",
            "Conflict resolver"
        ]
    );
    assert!(
        shows(cx, "outline-row-s-1-selected"),
        "selection on the row"
    );
    for row in ["s-1", "s-2", "s-3", "s-4"] {
        let selector = if row == "s-1" {
            format!("outline-row-{row}-selected")
        } else {
            format!("outline-row-{row}")
        };
        assert!(shows(cx, &selector), "{selector}");
        assert!(shows(cx, &format!("outline-route-{row}-finished")));
        assert!(shows(cx, &format!("outline-route-{row}-unable")));
    }
    let first = bounds(cx, "outline-row-s-1-selected".into());
    let second = bounds(cx, "outline-row-s-2".into());
    assert!(first.origin.y < second.origin.y, "in order");

    // Keyboard: ↓ chooses the next row, ⌥↓ moves it down one.
    cx.simulate_keystrokes("down");
    tick(cx);
    assert_eq!(selected(&view, cx), "Explorer");
    cx.simulate_keystrokes("alt-down");
    tick(cx);
    assert_eq!(
        names(&view, cx),
        [
            "Triager",
            "Implementer",
            "Explorer",
            "Reviewer",
            "Merge agent",
            "Conflict resolver"
        ]
    );
    view.read_with(cx, |view, _| {
        let draft = &view.shown_workflow().unwrap().draft;
        assert_eq!(
            draft.route_label("s-1", &Destination::Next),
            "Next agent \u{b7} Implementer",
            "Next follows the outline"
        );
    });

    // Drag: Reviewer let go on the first row takes its place.
    view.update(cx, |view, cx| {
        let reviewer = view
            .shown_workflow()
            .unwrap()
            .draft
            .step("s-4")
            .unwrap()
            .clone();
        view.drop_step(&StepDrag::of(&reviewer), 0, cx);
    });
    tick(cx);
    assert_eq!(
        names(&view, cx),
        [
            "Reviewer",
            "Triager",
            "Implementer",
            "Explorer",
            "Merge agent",
            "Conflict resolver"
        ]
    );

    // Add: a new agent at the end, chosen.
    click(cx, "outline-add");
    assert_eq!(names(&view, cx).len(), 7);
    assert_eq!(selected(&view, cx), "New agent");

    // Delete from its row's menu (the last row's offers no Move down):
    // the destructive row asks twice.
    click(cx, "outline-row-s-7-more");
    click(cx, "context-menu-row-2");
    click(cx, "context-menu-row-2");
    assert_eq!(names(&view, cx).len(), 6);

    // A second Workflow, Blank, renamed by typing.
    click(cx, "workflow-new");
    click(cx, "context-menu-row-0");
    let blank = view.read_with(cx, |view, _| view.shown_workflow().unwrap().id.clone());
    view.update(cx, |view, cx| {
        view.focus_field(Field::WorkflowName(blank.clone()), cx)
    });
    tick(cx);
    cx.simulate_input(" for docs");
    tick(cx);
    view.read_with(cx, |view, _| {
        let board = view.cockpit.factory();
        assert_eq!(board.workflows_of(project).count(), 2);
        assert_eq!(
            board.workflow(&blank).unwrap().draft.name,
            "Untitled workflow for docs"
        );
    });
    click(cx, "workflow-picker");
    view.read_with(cx, |view, _| {
        assert_eq!(view.factory_pick_rows().len(), 2, "both in the picker");
    });
    assert_eq!(fake.streams.borrow().len(), streams, "nothing started");
}

/// AC: routes are chosen on the row; a route to a deleted Step stays,
/// flagged; every problem is listed and the draft cannot be saved until
/// fixed; saving makes version 1 and starts nothing. An unavailable model
/// and skill are flagged, never replaced; skills come from what the
/// Provider has installed, searched by name or description.
#[gpui::test]
fn routes_problems_and_pickers(cx: &mut TestAppContext) {
    let (skills, found) = mpsc::channel();
    let (models, announced) = mpsc::channel();
    let (view, cx, fake, _) = workflows_window(cx, "workflows-routes", |fake| {
        *fake.command_discovery.borrow_mut() = Some(found);
        *fake.model_discovery.borrow_mut() = Some(announced);
    });
    let streams = fake.streams.borrow().len();
    view.update(cx, |view, cx| {
        view.factory_surface(Surface::Workflows, cx);
        view.create_workflow(Template::IssueToReviewedPr, cx);
    });
    tick(cx);
    let skill = |name: &str, description: &str| ferrite_core::SessionCommand {
        name: name.into(),
        description: description.into(),
        path: None,
        source: Some(ferrite_core::CommandSource::Skill),
    };
    skills
        .send(Ok(vec![
            skill("triage", "Sort incoming issues"),
            skill("code-review", "Review a diff"),
            skill("codebase-design", "Plan a change"),
            skill("implement", "Build it"),
            skill("pr", "Open a pull request"),
        ]))
        .unwrap();
    models
        .send((
            Provider::Claude,
            vec![ferrite_core::ModelInfo {
                value: "claude-opus-4".into(),
                display: "Opus 4".into(),
                detail: String::new(),
                resolved: None,
                efforts: vec!["high".into()],
                default_effort: None,
            }],
        ))
        .unwrap();
    tick(cx);
    assert!(!shows(cx, "workflow-problems"), "the template is whole");

    // Delete the Implementer: the Reviewer's route to it stays, flagged.
    view.update(cx, |view, cx| view.delete_step("s-3".into(), cx));
    tick(cx);
    assert!(shows(cx, "outline-route-s-4-unable-flagged"));
    assert!(shows(cx, "workflow-problems"));
    view.read_with(cx, |view, _| {
        let draft = &view.shown_workflow().unwrap().draft;
        let reviewer = draft.step("s-4").unwrap();
        assert_eq!(reviewer.on_unable, Destination::Step("s-3".into()));
        assert_eq!(
            draft.route_label("s-4", &reviewer.on_unable),
            "Deleted agent"
        );
    });
    view.update(cx, |view, cx| view.save_workflow(cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.shown_workflow().unwrap().versions.is_empty(),
            "unsaved"
        );
        assert_eq!(
            view.factory_view.as_ref().unwrap().note.as_deref(),
            Some("Fix the problems listed before saving.")
        );
    });

    // Fix it on the row: its picker offers Needs human first.
    click(cx, "outline-route-s-4-unable-flagged");
    view.read_with(cx, |view, _| {
        let rows: Vec<String> = view
            .factory_pick_rows()
            .iter()
            .map(|item| item.label.to_string())
            .collect();
        assert_eq!(rows[..2], ["Needs human", "Coordinator"]);
        assert!(rows.contains(&"Triager".to_string()), "{rows:?}");
    });
    click(cx, "context-menu-row-0");
    // The approval's rework recipient was the Implementer too (#222).
    view.read_with(cx, |view, _| {
        let draft = &view.shown_workflow().unwrap().draft;
        assert_eq!(
            draft
                .approval_problems()
                .iter()
                .map(|problem| problem.message.as_str())
                .collect::<Vec<_>>(),
            ["Rework goes to a deleted agent. Choose another."]
        );
    });
    view.update(cx, |view, cx| {
        view.apply_pick(Pick::ReworkGoesTo(Destination::Step("s-1".into())), cx)
    });
    tick(cx);
    assert!(!shows(cx, "workflow-problems"), "fixed");

    // An unavailable model is flagged, not replaced; so is a skill.
    view.update(cx, |view, cx| {
        view.select_step("s-4".into(), cx);
        view.apply_pick(Pick::Model("s-4".into(), "claude-gone".into()), cx);
        view.add_skill("retired-skill".into(), cx);
    });
    tick(cx);
    assert!(shows(cx, "inspector-model-flagged"));
    assert!(shows(cx, "skill-chip-retired-skill-flagged"));
    assert!(shows(cx, "skill-chip-code-review"));
    assert!(shows(cx, "workflow-problems"));
    view.read_with(cx, |view, _| {
        let reviewer = view.shown_step().unwrap();
        assert_eq!(reviewer.agent.model.as_deref(), Some("claude-gone"));
        let problems: Vec<String> = view
            .draft_problems()
            .into_iter()
            .map(|problem| problem.message)
            .collect();
        assert_eq!(
            problems,
            [
                "Reviewer: claude-gone is not available from Claude. Choose a model.",
                "Reviewer: the skill retired-skill is not installed for Claude.",
            ]
        );
    });
    view.update(cx, |view, cx| {
        view.apply_pick(Pick::Model("s-4".into(), "claude-opus-4".into()), cx);
        view.remove_skill("retired-skill", cx);
    });
    tick(cx);

    // The skills search: by description too, and a pick adds it.
    click(cx, "skill-add");
    assert!(shows(cx, "skill-search"), "revealed");
    cx.simulate_input("sort");
    tick(cx);
    assert!(shows(cx, "skill-option-triage"), "found by its description");
    assert!(!shows(cx, "skill-option-pr"));
    click(cx, "skill-option-triage");
    view.read_with(cx, |view, _| {
        assert_eq!(view.shown_step().unwrap().skills, ["code-review", "triage"]);
    });

    // A route picked on a row: Reviewer finished → Coordinator, and the
    // Merge agent's confirmed merge (#223) → Needs human.
    view.update(cx, |view, cx| {
        view.apply_pick(
            Pick::Route {
                step: "s-5".into(),
                kind: RouteKind::Merge(ferrite_core::factory::MergeOutcome::Merged),
                to: Destination::Human,
            },
            cx,
        )
    });
    view.update(cx, |view, cx| {
        view.apply_pick(
            Pick::Route {
                step: "s-4".into(),
                kind: RouteKind::Finished,
                to: Destination::Coordinator,
            },
            cx,
        )
    });
    tick(cx);
    assert!(
        shows(cx, "workflow-problems"),
        "nothing ends in Completed now"
    );
    view.update(cx, |view, cx| {
        view.apply_pick(
            Pick::Route {
                step: "s-4".into(),
                kind: RouteKind::Finished,
                to: Destination::Complete,
            },
            cx,
        )
    });
    tick(cx);
    // The Reviewer would complete what was never merged.
    view.read_with(cx, |view, _| {
        let problems: Vec<String> = view
            .draft_problems()
            .into_iter()
            .map(|problem| problem.message)
            .collect();
        assert_eq!(
            problems,
            [
                "Reviewer \u{b7} When finished reaches Completed with nothing merged: route it \
              through Merge agent."
            ]
        );
    });
    view.update(cx, |view, cx| {
        for (step, kind, to) in [
            ("s-4", RouteKind::Finished, Destination::Coordinator),
            (
                "s-5",
                RouteKind::Merge(ferrite_core::factory::MergeOutcome::Merged),
                Destination::Complete,
            ),
        ] {
            view.apply_pick(
                Pick::Route {
                    step: step.into(),
                    kind,
                    to,
                },
                cx,
            );
        }
    });
    tick(cx);

    click(cx, "workflow-save");
    view.read_with(cx, |view, _| {
        let workflow = view.shown_workflow().unwrap();
        assert_eq!(workflow.versions.len(), 1);
        assert!(!workflow.unsaved());
        assert_eq!(
            view.factory_view.as_ref().unwrap().note.as_deref(),
            Some("Saved version 1.")
        );
    });
    assert_eq!(
        fake.streams.borrow().len(),
        streams,
        "saving starts nothing"
    );
    view.update(cx, |view, cx| view.open_new_task(cx));
    tick(cx);
    assert!(shows(cx, "new-task-process"));
    view.read_with(cx, |view, _| {
        let picks = view.process_picks();
        assert_eq!(picks.len(), 2, "one-off and the saved Workflow");
        assert_eq!(
            picks[1].0.label.as_ref(),
            "Issue to reviewed PR \u{b7} version 1"
        );
    });
}

/// AC: what was typed survives changing a picker, revealing a field,
/// dismissing a notice, scrolling, switching surfaces, and saving then
/// reopening; a notice never covers the surface's buttons.
#[gpui::test]
fn typed_text_survives_what_happens_around_it(cx: &mut TestAppContext) {
    let (view, cx, _fake, project) = workflows_window(cx, "workflows-text", |_| {});
    view.update(cx, |view, cx| {
        view.factory_surface(Surface::Workflows, cx);
        view.create_workflow(Template::Blank, cx);
    });
    tick(cx);
    let (workflow, step) = view.read_with(cx, |view, _| {
        (
            view.shown_workflow().unwrap().id.clone(),
            view.shown_step().unwrap().id.clone(),
        )
    });
    let instructions = Field::Instructions(workflow.clone(), step.clone());
    click(cx, "inspector-instructions");
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.factory_view.as_ref().unwrap().workflows.focused,
            Some(instructions.clone())
        );
    });
    cx.simulate_input("Check the build");
    tick(cx);
    let typed = "Check the build";
    let holds = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext, when: &str| {
        view.read_with(cx, |view, cx| {
            assert_eq!(
                view.field_text(&instructions, cx).as_deref(),
                Some(typed),
                "{when}"
            );
            assert_eq!(
                view.shown_step().unwrap().instructions,
                typed,
                "{when}: the draft"
            );
        })
    };
    holds(&view, cx, "typed");

    view.update(cx, |view, cx| {
        view.apply_pick(Pick::Provider(step.clone(), Provider::Codex), cx)
    });
    tick(cx);
    holds(&view, cx, "a picker changed");

    click(cx, "skill-add");
    holds(&view, cx, "a field revealed");

    view.update(cx, |view, cx| {
        view.factory_view.as_mut().unwrap().note = Some("A notice".into());
        cx.notify();
    });
    tick(cx);
    let note = bounds(cx, "factory-note".into());
    for button in ["workflow-save", "workflow-run", "factory-new-task"] {
        assert!(
            !note.intersects(&bounds(cx, button.into())),
            "the notice covers {button}"
        );
    }
    let board = bounds(cx, "factory-board".into());
    for button in ["factory-note-dismiss", "factory-new-task"] {
        let at = bounds(cx, button.into());
        assert!(
            board.left() <= at.left() && at.right() <= board.right(),
            "{button} in the head: {at:?} in {board:?}"
        );
    }
    click(cx, "factory-note-dismiss");
    assert!(!shows(cx, "factory-note"));
    holds(&view, cx, "a notice dismissed");

    let inspector = bounds(cx, "inspector".into());
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: inspector.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-400.))),
        modifiers: gpui::Modifiers::none(),
        touch_phase: gpui::TouchPhase::default(),
    });
    tick(cx);
    holds(&view, cx, "scrolled");

    click(cx, "factory-tab-board");
    assert!(!shows(cx, "workflows"));
    click(cx, "factory-tab-workflows");
    holds(&view, cx, "surfaces switched");

    click(cx, "workflow-save");
    view.update(cx, |view, cx| {
        view.factory_view = None;
        cx.notify();
    });
    tick(cx);
    view.update(cx, |view, cx| {
        view.open_factory(project, cx);
        view.factory_surface(Surface::Workflows, cx);
    });
    tick(cx);
    holds(&view, cx, "saved and reopened");
    view.read_with(cx, |view, _| {
        assert_eq!(view.shown_workflow().unwrap().versions.len(), 1);
    });
}
