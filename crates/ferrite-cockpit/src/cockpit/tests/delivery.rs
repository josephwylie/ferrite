//! A Task's pull request in the window (#223): its card, the panel's PR
//! with its revisions and the approved one, evidence about an older
//! revision said as such, Ready to approve saying the revision approved,
//! the New task sheet's PR, and a Step switched to route by merge result
//! with one route per result on its outline row.
#[allow(unused_imports)]
use super::*;
use crate::cockpit::workflows::{Pick, Surface};
use ferrite_core::factory::{Board, Destination, MergeOutcome, RouteKind, Status, Task, Template};
use ferrite_core::protocol::TaskAction;

fn shows(cx: &mut gpui::VisualTestContext, selector: &str) -> bool {
    debug_bounds(cx, selector.to_string()).is_some()
}

fn click(cx: &mut gpui::VisualTestContext, selector: &str) {
    let at = bounds(cx, selector.to_string());
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
}

const HEAD_1: &str = "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD_2: &str = "2222222bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// A Workflow Task with its PR at revision 2, revision 1 approved, and
/// evidence about each, in `status`.
fn task(project: ProjectId, status: Status) -> Task {
    serde_json::from_value(serde_json::json!({
        "id": "t-1",
        "project": project,
        "title": "Add a --fast flag",
        "instructions": "Add a --fast flag",
        "status": status,
        "agent": {"provider": "claude", "model": "claude-opus-4"},
        "branch": "factory/t-1",
        "activity": [
            {"at_ms": 1, "by": "tool", "text": "Build reported: cargo test: 12 passed", "revision": 1},
        ],
        "created_ms": 1,
        "workflow": {
            "id": "w-1",
            "version": 1,
            "definition": {
                "name": "Ship it",
                "steps": [{
                    "id": "s-1", "name": "Review", "instructions": "Review it",
                    "provider": "claude", "on_finished": "approval", "on_unable": "human",
                }],
            },
        },
        "step": "s-1",
        "result": "Looks right",
        "pr": {
            "repository": "acme/widgets",
            "number": 12,
            "revisions": [
                {"revision": 1, "head": HEAD_1, "by": "Build", "at_ms": 1},
                {"revision": 2, "head": HEAD_2, "by": "Resolve", "at_ms": 2},
            ],
            "approved": 1,
        },
        "evidence": [
            {"kind": "tool", "by": "Build", "text": "cargo test: 12 passed", "revision": 1, "at_ms": 1},
            {"kind": "conclusion", "by": "Review", "text": "finished: Looks right", "revision": 2, "at_ms": 2},
        ],
    }))
    .unwrap()
}

fn panel<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    status: Status,
) -> (Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let (mut core, _fake) = cockpit(name, 1);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_record_tasks();
    core.fixture_factory(Board {
        workflows: Vec::new(),
        tasks: vec![task(project, status)],
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(1000.)));
    tick(cx);
    view.update(cx, |view, cx| view.open_task("t-1", cx));
    tick(cx);
    (view, cx)
}

/// AC: the card and the panel show the PR; the panel lists its revisions
/// with who reported each, which was approved and that the current one
/// is not; evidence about revision 1 is said to be older; the activity
/// keeps a tool's result apart, with its revision.
#[gpui::test]
fn the_panel_shows_the_pr_its_revisions_and_which_was_approved(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx, "delivery-panel", Status::Working);
    assert!(shows(cx, "factory-card-t-1-pr"));
    assert!(shows(cx, "task-pr"));
    assert!(shows(cx, "task-pr-link"));
    assert!(shows(cx, "task-pr-revision-1-approved"));
    assert!(shows(cx, "task-pr-revision-2"));
    assert!(shows(cx, "task-pr-state"));
    assert!(shows(cx, "task-evidence-0-tool-older"), "about revision 1");
    assert!(shows(cx, "task-evidence-1-conclusion"));
    view.read_with(cx, |view, _| {
        let task = view.open_task_now().unwrap();
        assert_eq!(
            crate::cockpit::delivery::card_line(task).as_deref(),
            Some("acme/widgets#12 \u{b7} revision 2")
        );
    });

    view.update(cx, |view, cx| {
        view.task_tab(crate::cockpit::factory::TaskTab::Activity, cx)
    });
    tick(cx);
    assert!(shows(cx, "task-activity-0"));
}

/// AC: Ready to approve says which revision Approve is for, and its
/// evidence marks what is about an older revision.
#[gpui::test]
fn ready_to_approve_says_the_revision_approved(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx, "delivery-approve", Status::Approval);
    assert!(shows(cx, "task-approval"));
    assert!(shows(cx, "task-approval-revision"));
    assert!(shows(cx, "task-evidence-0-tool-older"));
    assert!(
        !shows(cx, "task-evidence-section"),
        "the approval section shows the evidence"
    );
    view.read_with(cx, |view, _| {
        let task = view.open_task_now().unwrap();
        assert_eq!(
            crate::cockpit::delivery::approving_line(task).as_deref(),
            Some("You approve acme/widgets#12 at revision 2 (head 2222222).")
        );
    });
    click(cx, "task-approve");
    assert_eq!(
        view.read_with(cx, |view, _| view.cockpit.fixture_task_asks().to_vec()),
        [TaskAction::Approve { task: "t-1".into() }]
    );
}

/// AC: the New task sheet takes an optional PR, sent with the start.
#[gpui::test]
fn the_new_task_sheet_sends_its_pr(cx: &mut TestAppContext) {
    let (view, cx) = panel(cx, "delivery-new-task", Status::Working);
    view.update(cx, |view, cx| view.open_new_task(cx));
    tick(cx);
    cx.simulate_input("Ship the flag");
    tick(cx);
    click(cx, "new-task-pr");
    cx.simulate_input("https://github.com/acme/widgets/pull/12");
    tick(cx);
    assert!(shows(cx, "new-task-pr-focused"));
    view.update(cx, |view, cx| view.start_new_task(cx));
    tick(cx);
    let asks = view.read_with(cx, |view, _| view.cockpit.fixture_task_asks().to_vec());
    match asks.as_slice() {
        [TaskAction::Start {
            instructions, pr, ..
        }] => {
            assert_eq!(instructions, "Ship the flag");
            assert_eq!(pr.as_deref(), Some("https://github.com/acme/widgets/pull/12"));
        }
        other => panic!("{other:?}"),
    }
}

/// AC: a Step switched to route by merge result shows one route per
/// result on its outline row, its pickers keep Completed for a confirmed
/// merge only, and the template's merge agent sends a conflict to the
/// resolver.
#[gpui::test]
fn a_step_routes_by_merge_result_with_a_route_per_result(cx: &mut TestAppContext) {
    let fake = Fake::default();
    let dir = scratch("delivery-merge-routes");
    let mut core = Cockpit::new(Store::open(&dir).unwrap(), Box::new(fake.clone()));
    let thread = core
        .open(Provider::Claude, WorkspaceChoice::Main { checkout: here() })
        .unwrap();
    did_something_now(&dir, thread);
    let project = core.project_id(core.threads()[0]).unwrap();
    core.fixture_live_factory();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(1400.)));
    tick(cx);
    view.update(cx, |view, cx| {
        view.open_factory(project, cx);
        view.factory_surface(Surface::Workflows, cx);
        view.create_workflow(Template::IssueToReviewedPr, cx);
    });
    tick(cx);
    for kind in ["merged", "conflict", "pending", "denied", "head_changed", "unknown", "unable"] {
        assert!(shows(cx, &format!("outline-route-s-5-{kind}")), "{kind}");
    }
    assert!(!shows(cx, "outline-route-s-5-finished"));

    view.update(cx, |view, cx| view.select_step("s-2".into(), cx));
    tick(cx);
    assert!(shows(cx, "inspector-merge"));
    click(cx, "inspector-merge");
    assert!(shows(cx, "inspector-merge-on"));
    assert!(shows(cx, "outline-route-s-2-pending"));

    click(cx, "outline-route-s-2-conflict");
    let rows: Vec<String> = view.read_with(cx, |view, _| {
        view.factory_pick_rows()
            .iter()
            .map(|item| item.label.to_string())
            .collect()
    });
    assert!(!rows.contains(&"Completed".to_string()), "{rows:?}");
    assert!(rows.contains(&"Conflict resolver".to_string()), "{rows:?}");
    view.update(cx, |view, cx| {
        view.apply_pick(
            Pick::Route {
                step: "s-2".into(),
                kind: RouteKind::Merge(MergeOutcome::Conflict),
                to: Destination::Step("s-6".into()),
            },
            cx,
        )
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        let draft = &view.shown_workflow().unwrap().draft;
        let explorer = draft.step("s-2").unwrap();
        assert_eq!(
            explorer.outcome_routes.as_ref().unwrap().conflict,
            Destination::Step("s-6".into())
        );
    });
}
