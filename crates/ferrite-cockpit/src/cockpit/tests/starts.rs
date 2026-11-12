//! How a Workflow's Tasks start, in the window (#225, ADR 0037): the one
//! "Tasks start" picker whose sources each show only their own settings,
//! typed settings that survive switching between them, repositories and
//! labels listed from the login, the automatic starts' switch apart from
//! Save, a backlog's question and a failed check, and the Settings limit.
#[allow(unused_imports)]
use super::*;
use crate::cockpit::workflows::{Field, Surface};
use ferrite_core::factory::{
    Command, GitHubItem, GitHubRead, Missed, Overlap, Recurrence, Start, StartKind, StepChange,
    Switch, Template, Trigger,
};

fn shows(cx: &mut gpui::VisualTestContext, selector: &str) -> bool {
    debug_bounds(cx, selector.to_string()).is_some()
}

/// A click where the operator would make it: a control of the Start
/// inspector is scrolled into view first.
fn click(cx: &mut gpui::VisualTestContext, selector: &str) {
    let inspected = ["start-", "automation-", "backlog-"]
        .iter()
        .any(|prefix| selector.starts_with(prefix));
    if inspected && shows(cx, "inspector-start") {
        reveal(cx, "inspector-start", selector);
    }
    let at = bounds(cx, selector.to_string());
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
}

/// Let the lists the login answers land.
fn settle(cx: &mut gpui::VisualTestContext) {
    for _ in 0..20 {
        cx.run_until_parked();
        tick(cx);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// A window on a fixture world whose Factory answers and whose GitHub
/// login reaches two repositories, on a Blank Workflow's start.
fn starts_window<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let fake = Fake::default();
    let dir = scratch(name);
    let mut core = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake));
    let thread = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    did_something_now(&dir, thread);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_live_factory();
    core.fixture_github(vec![
        (
            "acme/app".into(),
            vec!["bug".into(), "ready-for-agent".into()],
        ),
        ("acme/site".into(), vec!["docs".into()]),
    ]);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    view.update(cx, |view, cx| {
        view.open_factory(project, cx);
        view.factory_surface(Surface::Workflows, cx);
        view.create_workflow(Template::Blank, cx);
    });
    tick(cx);
    (view, cx)
}

fn start(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Start {
    view.read_with(cx, |view, _| {
        view.shown_workflow().unwrap().draft.start.clone()
    })
}

/// Choose row `row` of the picker `selector` opens.
fn pick(cx: &mut gpui::VisualTestContext, selector: &str, row: usize) {
    click(cx, selector);
    assert!(shows(cx, "context-menu"), "{selector} opens its picker");
    click(cx, &format!("context-menu-row-{row}"));
}

/// AC: the picker offers the five sources; each shows only its own
/// settings; typed settings survive switching between them; repositories
/// and labels come from the login's lists, and a missing one is flagged.
#[gpui::test]
fn each_source_shows_its_own_settings_and_keeps_what_was_typed(cx: &mut TestAppContext) {
    let (view, cx) = starts_window(cx, "starts-sources");
    click(cx, "outline-start");
    assert!(shows(cx, "outline-start-selected"));
    assert!(shows(cx, "inspector-start"));
    click(cx, "start-kind");
    let rows = view.read_with(cx, |view, _| view.factory_pick_rows());
    let labels: Vec<String> = rows.iter().map(|row| row.label.to_string()).collect();
    assert_eq!(
        labels,
        [
            "Manually",
            "On a schedule",
            "From a GitHub issue",
            "From a GitHub pull request",
            "From your own script",
        ]
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!shows(cx, "start-recurrence"), "manual has no settings");
    assert!(!shows(cx, "automation-off"), "nor a switch");

    pick(cx, "start-kind", 1);
    assert_eq!(start(&view, cx).kind, StartKind::Schedule);
    for selector in [
        "start-recurrence",
        "start-time-flagged",
        "start-schedule-when",
        "start-schedule-instructions",
        "start-missed-0-on",
        "start-overlap-0-on",
        "automation-off",
    ] {
        assert!(shows(cx, selector), "{selector}");
    }
    assert!(!shows(cx, "start-repository-flagged"), "not GitHub's");
    click(cx, "start-schedule-instructions");
    cx.simulate_input("Triage the new issues");
    tick(cx);
    pick(cx, "start-time-flagged", 18);
    click(cx, "start-overlap-1");
    click(cx, "start-missed-1");
    let schedule = start(&view, cx).schedule;
    assert_eq!(schedule.instructions, "Triage the new issues");
    assert_eq!(schedule.time.as_deref(), Some("09:00"));
    assert_eq!(
        (schedule.overlap, schedule.missed),
        (Overlap::Queue, Missed::RunOnce)
    );
    pick(cx, "start-recurrence", 2);
    assert!(shows(cx, "start-weekday"), "weekly names its day");
    pick(cx, "start-recurrence", 3);
    assert!(shows(cx, "start-interval"), "every few hours");
    assert!(!shows(cx, "start-time"), "has no time");
    assert_eq!(start(&view, cx).schedule.recurrence, Recurrence::Interval);

    pick(cx, "start-kind", 2);
    settle(cx);
    assert!(!shows(cx, "start-recurrence"), "the schedule's settings go");
    for selector in [
        "start-repository-flagged",
        "start-trigger",
        "start-label-flagged",
        "start-github-sentence",
        "start-receives",
        "start-extra-instructions",
    ] {
        assert!(shows(cx, selector), "{selector}");
    }
    click(cx, "start-repository-flagged");
    let repositories: Vec<String> = view.read_with(cx, |view, _| {
        view.factory_pick_rows()
            .iter()
            .map(|row| row.label.to_string())
            .collect()
    });
    assert_eq!(repositories, ["acme/app", "acme/site"], "from the login");
    click(cx, "context-menu-row-0");
    settle(cx);
    click(cx, "start-label-flagged");
    let labels: Vec<String> = view.read_with(cx, |view, _| {
        view.factory_pick_rows()
            .iter()
            .map(|row| row.label.to_string())
            .collect()
    });
    assert_eq!(labels, ["bug", "ready-for-agent"], "that repository's");
    click(cx, "context-menu-row-1");
    assert!(shows(cx, "start-label"), "no longer flagged");
    click(cx, "start-extra-instructions");
    cx.simulate_input("Open a PR that closes it");
    tick(cx);
    let github = start(&view, cx).github;
    assert_eq!(github.repository.as_deref(), Some("acme/app"));
    assert_eq!(github.label.as_deref(), Some("ready-for-agent"));
    assert_eq!(github.extra_instructions, "Open a PR that closes it");
    click(cx, "start-trigger");
    assert_eq!(
        view.read_with(cx, |view, _| view.factory_pick_rows().len()),
        2,
        "an issue cannot request a review"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);

    pick(cx, "start-kind", 3);
    pick(cx, "start-trigger", 2);
    assert_eq!(start(&view, cx).github.trigger, Trigger::ReviewRequested);
    assert!(!shows(cx, "start-label"), "a review request has no label");

    pick(cx, "start-kind", 1);
    let kept = start(&view, cx);
    assert_eq!(kept.schedule.instructions, "Triage the new issues", "kept");
    assert_eq!(kept.github.repository.as_deref(), Some("acme/app"), "kept");
    view.read_with(cx, |view, cx| {
        let workflow = view.shown_workflow().unwrap().id.clone();
        assert_eq!(
            view.field_text(&Field::ScheduleInstructions(workflow), cx)
                .as_deref(),
            Some("Triage the new issues")
        );
    });

    pick(cx, "start-kind", 4);
    assert!(shows(cx, "start-script-command"));
    assert!(shows(cx, "automation-off"));
    let workflow = view.read_with(cx, |view, _| view.shown_workflow().unwrap().id.clone());
    let command = crate::cockpit::starts::script_command("/Apps/Ferrite/ferrite", &workflow);
    assert_eq!(
        command,
        format!(
            "\"/Apps/Ferrite/ferrite\" task submit {workflow} \"<identity>\" \"<instructions>\""
        )
    );

    // The row's number, clear of the route choosers in the middle.
    let row = bounds(cx, "outline-row-s-1".to_string());
    cx.simulate_click(
        row.origin + gpui::point(gpui::px(12.), gpui::px(12.)),
        gpui::Modifiers::none(),
    );
    tick(cx);
    assert!(shows(cx, "inspector"), "a Step's inspector again");
    assert!(!shows(cx, "inspector-start"));
}

fn issue(number: u64) -> GitHubItem {
    GitHubItem {
        number,
        title: format!("Issue {number}"),
        body: String::new(),
        url: format!("https://github.com/acme/app/issues/{number}"),
    }
}

/// AC: automatic starts are off after saving and turned on, paused and
/// resumed apart from Save; a backlog at the first check asks once,
/// defaulting to new ones only; a failed check is said in the settings.
#[gpui::test]
fn automatic_starts_are_a_switch_apart_from_saving(cx: &mut TestAppContext) {
    let (view, cx) = starts_window(cx, "starts-switch");
    view.update(cx, |view, cx| {
        let step = view.shown_step().unwrap().id.clone();
        view.edit_step(
            step,
            StepChange {
                instructions: Some("Do it".into()),
                ..Default::default()
            },
            cx,
        );
        view.edit_start(
            |start| {
                start.kind = StartKind::GithubIssue;
                start.github.repository = Some("acme/app".into());
                start.github.label = Some("bug".into());
            },
            cx,
        );
        view.open_start(cx);
    });
    tick(cx);
    assert!(shows(cx, "automation-off"));
    click(cx, "automation-turn");
    let switch = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.shown_workflow().unwrap().automation.switch
        })
    };
    assert_eq!(switch(&view, cx), Switch::Off, "nothing saved to turn on");

    view.update(cx, |view, cx| view.save_workflow(cx));
    tick(cx);
    assert!(shows(cx, "automation-off"), "saving turns nothing on");
    click(cx, "automation-turn");
    assert!(shows(cx, "automation-on"));
    click(cx, "automation-turn");
    assert!(shows(cx, "automation-paused"));
    click(cx, "automation-turn");
    assert!(shows(cx, "automation-on"), "resumed");

    let workflow = view.read_with(cx, |view, _| view.shown_workflow().unwrap().id.clone());
    view.update(cx, |view, cx| {
        view.cockpit.fixture_factory_command(Command::GitHubRead {
            workflow: workflow.clone(),
            version: 1,
            result: Ok(GitHubRead::Matches {
                etag: None,
                items: vec![issue(1), issue(2)],
            }),
        });
        cx.notify();
    });
    tick(cx);
    assert!(shows(cx, "automation-backlog"), "asks about what matches");
    assert!(shows(cx, "backlog-new-only"), "new ones only, the default");
    click(cx, "backlog-new-only");
    assert!(!shows(cx, "automation-backlog"), "asked once");
    view.read_with(cx, |view, _| {
        let automation = &view.shown_workflow().unwrap().automation;
        assert_eq!(automation.seen.len(), 2, "passed over");
        assert!(view.cockpit.factory().tasks.is_empty(), "nothing started");
    });

    view.update(cx, |view, cx| {
        view.cockpit.fixture_factory_command(Command::GitHubRead {
            workflow: workflow.clone(),
            version: 1,
            result: Err("gh: not logged in".into()),
        });
        cx.notify();
    });
    tick(cx);
    assert!(shows(cx, "automation-error"), "a failed check is said");
    assert!(
        view.read_with(cx, |view, _| view.cockpit.factory().tasks.is_empty()),
        "never as a Task"
    );
}

/// AC: Settings has a limit on Tasks working at once, 3 by default,
/// saved where the Server rereads it.
#[gpui::test]
fn settings_bound_how_many_tasks_work_at_once(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("starts-limit", 1);
    bind_production_keys(cx);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| CockpitView::new(core, cx));
        gpui::component::Root::new(
            cx.new(|cx| crate::cockpit_window::CockpitWindow::new(view, window, cx)),
            window,
            cx,
        )
    });
    let view = root.read_with(cx, |root, cx| {
        crate::cockpit_window::testing::cockpit(root.view(), cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.prefs.settings.factory_tasks_at_once, 3)
    });
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_input("Tasks working at once");
    tick(cx);
    let more = cx
        .debug_bounds("settings-factory-limit-1")
        .expect("the limit's stepper")
        .center();
    cx.simulate_click(more, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.prefs.settings.factory_tasks_at_once, 4);
        assert_eq!(
            ferrite_core::settings::Settings::load(&view.prefs.dir)
                .unwrap()
                .factory_tasks_at_once,
            4
        );
    });
}
