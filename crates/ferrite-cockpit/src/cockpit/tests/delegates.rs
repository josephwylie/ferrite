//! A Thread's Delegates in the window (ADR 0047), from its digest: T3's
//! remote row in the details card's Lineage, the Working shelf and the
//! background line while one is open, its stop, its `delegate_task`
//! call's row in the transcript, and the row and the divider of a Thread
//! another machine delegated.
use super::*;
use ferrite_core::delegate::{DelegateId, Started, Status};
use ferrite_core::server::ServerId;
use ferrite_pane::pane::{AgentClock, AgentRun, Elsewhere};

const ROW: &str = "details-lineage-delegate-Fix the tests";
const STOP: &str = "details-lineage-delegate-Fix the tests-stop";

/// A window on one Thread with its details card open.
fn window<'a>(name: &str, cx: &'a mut TestAppContext) -> (Entity<CockpitView>, ThreadKey, &'a mut gpui::VisualTestContext) {
    let (core, _fake) = cockpit(name, 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    (view, thread, cx)
}

fn amend(
    view: &Entity<CockpitView>,
    thread: ThreadKey,
    change: impl FnOnce(&mut ferrite_core::delegate::Delegates) -> bool,
    cx: &mut gpui::VisualTestContext,
) {
    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_delegates(thread, change))
    });
    tick(cx);
}

/// `Fix the tests`, started on `pc`.
fn start(view: &Entity<CockpitView>, thread: ThreadKey, cx: &mut gpui::VisualTestContext) -> DelegateId {
    let mut id = None;
    amend(
        view,
        thread,
        |delegates| {
            let started = Started {
                server: ServerId::new("s-pc"),
                machine: "pc".into(),
                thread: ThreadId::new(7),
                request: "delegate:here:1:7".into(),
                title: Some("Fix the tests".into()),
                task: "Fix the tests".into(),
                provider: Provider::Codex,
                model: None,
            };
            id = Some(delegates.start(started).id);
            true
        },
        cx,
    );
    id.unwrap()
}

/// While a Delegate is open its parent is on the Working shelf, the line
/// under its transcript names it as T3's transcript row does, and the
/// Lineage draws T3's remote row; once it ends and its wake is told, the
/// row moves under `Previous agents` and nothing holds the Thread.
#[gpui::test]
fn an_open_delegate_holds_its_parent_working_and_draws_in_the_lineage(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("delegates-working", cx);
    let seen = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            (
                view.nav_state(cx).working.iter().any(|row| row.key == thread),
                view.background_work_title(0, cx),
            )
        })
    };
    assert_eq!(seen(cx), (false, None), "the premise: at rest");

    let id = start(&view, thread, cx);
    until(cx, |cx| cx.debug_bounds(ROW).is_some());
    assert_eq!(
        seen(cx),
        (true, Some("Waiting \u{b7} Fix the tests on pc".into())),
        "an open Delegate is background work Main waits on"
    );
    assert!(
        cx.debug_bounds("details-lineage-running-1").is_none(),
        "T3 counts a Delegate that is Running, not one Starting"
    );
    amend(&view, thread, |delegates| delegates.seen(id, Status::Running, None), cx);
    until(cx, |cx| cx.debug_bounds("details-lineage-running-1").is_some());

    amend(&view, thread, |delegates| delegates.end(id, Status::Completed, "done".into()), cx);
    assert!(seen(cx).0, "its wake is owed: the Thread waits for it");
    amend(&view, thread, |delegates| delegates.tell(&[id]), cx);
    assert_eq!(seen(cx), (false, None), "told: nothing holds it");
    assert!(cx.debug_bounds(ROW).is_none(), "an ended one is under Previous agents, folded");
    let previous = cx.debug_bounds("details-lineage-previous").unwrap();
    cx.simulate_click(previous.center(), gpui::Modifiers::none());
    until(cx, |cx| cx.debug_bounds(ROW).is_some());
    let row = cx.debug_bounds(ROW).unwrap();
    cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds(STOP).is_none(), "an ended one has no stop");
}

/// The stop on an open Delegate's row shows under the pointer and is the
/// agent's `delegate.cancel`, asked of the Server that follows it; an
/// Engine in this process follows none, and its refusal is said in the
/// nav's notice.
#[gpui::test]
fn a_delegates_stop_asks_its_server_and_a_refusal_is_said(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("delegates-stop", cx);
    start(&view, thread, cx);
    until(cx, |cx| cx.debug_bounds(ROW).is_some());
    let row = cx.debug_bounds(ROW).unwrap();
    cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
    tick(cx);
    let stop = cx.debug_bounds(STOP).unwrap();
    cx.simulate_click(stop.center(), gpui::Modifiers::none());
    until(cx, |cx| view.read_with(cx, |view, _| view.nav_notice.text().is_some()));
    assert_eq!(
        view.read_with(cx, |view, _| view.nav_notice.text().map(|text| text.to_string())),
        Some("Cancelling a Delegate needs the Ferrite server.".into())
    );
}

/// A Thread another machine delegated heads its Lineage with T3's
/// delegated-from row, from its digest by its Facts.
#[gpui::test]
fn a_delegated_thread_heads_its_lineage_with_whose_it_is(cx: &mut TestAppContext) {
    let (view, thread, cx) = window("delegates-from", cx);
    assert!(cx.debug_bounds("details-lineage-delegated-from").is_none());
    let from = ferrite_core::DelegatedFrom {
        server: ServerId::new("s-mac"),
        label: "mac".into(),
        thread: ThreadId::new(3),
        title: "Ship it".into(),
    };
    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_delegated_from(thread, from))
    });
    until(cx, |cx| cx.debug_bounds("details-lineage-delegated-from").is_some());
    let kept = view.read_with(cx, |view, _| {
        view.facts.get(thread).and_then(|facts| facts.delegated_from.clone())
    });
    assert_eq!(kept.map(|from| from.title), Some("Ship it".into()));
    until(cx, |cx| cx.debug_bounds("parent-divider").is_some());
    assert_eq!(
        view.read_with(cx, |view, cx| view.delegator_link(0, cx)),
        Some(ferrite_pane::transcript::ParentLink {
            title: "Ship it".into(),
            machine: "mac".into(),
            opens: false,
        }),
        "T3's `Subagent of` divider heads Main's rows, `on` the label mac's Server sent"
    );
    assert!(
        cx.debug_bounds("parent-divider-open").is_none(),
        "this window does not reach mac: the divider opens nothing"
    );
}

/// T3's `SubagentTimelineLink` for a task on another machine: once the
/// parent hands the task over and stops, its `delegate_task` call's row is
/// its Delegate's, `on pc`, running until the digest says it ended, then
/// its result and how long it took. This window reaches no `pc`, so the
/// row opens nothing.
#[gpui::test]
fn a_delegate_call_is_t3s_task_row_on_its_machine(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("delegates-row", 1);
    let _ = core.send(core.threads()[0], "Hand the tests to pc".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1280.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let id = start(&view, thread, cx);
    let send = |event| fake.streams.borrow()[0].send(event).unwrap();
    send(SessionEvent::ToolStarted {
        id: "call-d".into(),
        name: "mcp__ferrite__delegate_task".into(),
        input: serde_json::json!({"title": "Fix the tests", "task": "Make cargo test pass", "machine": "pc"}),
    });
    send(SessionEvent::ToolCompleted {
        id: "call-d".into(),
        output: format!("{{\"taskId\":\"{id}\",\"status\":\"running\"}}"),
        is_error: false,
        result: ferrite_core::ToolResult::Opaque,
    });
    send(SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    });
    until(cx, |cx| cx.debug_bounds("agent-row-call-d").is_some());
    let face = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let open = view.cockpit.read(cx).thread(thread).unwrap();
            let faces = view.delegate_faces(thread, open.transcript(), cx);
            assert_eq!(faces.len(), 1, "the one call, by its id");
            assert_eq!(faces[0].0, "call-d");
            faces[0].1.clone()
        })
    };
    let running = face(cx);
    assert_eq!(running.run, AgentRun::Running, "the call returned at once; its Delegate runs");
    assert!(matches!(running.clock, Some(AgentClock::Since(_))));
    assert_eq!(
        running.on,
        Some(Elsewhere {
            machine: "pc".into(),
            reached: false
        })
    );
    assert!(cx.debug_bounds("agent-on").is_some(), "`on pc` beside its title");
    assert!(cx.debug_bounds("agent-open-call-d").is_none(), "pc is not reached here");

    amend(&view, thread, |delegates| delegates.end(id, Status::Completed, "All green.".into()), cx);
    let done = face(cx);
    assert_eq!((done.run, done.result.as_deref()), (AgentRun::Done, Some("All green.")));
    assert!(matches!(done.clock, Some(AgentClock::Took(_))));
}
