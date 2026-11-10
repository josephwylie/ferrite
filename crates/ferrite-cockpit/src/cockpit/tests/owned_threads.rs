//! Owned Threads (#219, ADR 0037): a Worker or a Coordinator is an
//! ordinary Thread that the operator's own lists and alerts leave out.
#[allow(unused_imports)]
use super::*;
use crate::palette::Act;

/// A Worker, open and asking, and a parked Coordinator are in none of the
/// nav's rows (tree, Groups, Parked), the palette's Threads or the
/// Inbox, and the Worker's request rings nothing, while the operator's
/// own request toasts as ever.
#[gpui::test]
fn owned_threads_are_out_of_the_lists_and_ring_nothing(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("owned-unlisted", 1);
    let mine = core.threads()[0];
    let worker = core
        .fixture_open_owned(
            Provider::Claude,
            WorkspaceChoice::Main { checkout: here() },
            ferrite_core::ThreadOwner::Task("t-1".into()),
        )
        .unwrap();
    let project = core.project_id(mine).unwrap();
    let coordinator = core
        .fixture_open_owned(
            Provider::Claude,
            WorkspaceChoice::Main { checkout: here() },
            ferrite_core::ThreadOwner::Factory(project),
        )
        .unwrap();
    core.park(coordinator).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);

    let toasts = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.needs_you.toasts().len())
    };
    fake.streams.borrow()[1]
        .send(decision("perm_worker"))
        .unwrap();
    tick(cx);
    ring(cx);
    assert_eq!(toasts(cx), 0, "a Worker's request rings nothing");

    let listed = |view: &CockpitView, thread: ThreadId| {
        let state = view.nav_state();
        let inbox = view.inbox(&state);
        state
            .pinned
            .iter()
            .chain(&state.solos)
            .chain(state.groups.iter().flat_map(|group| &group.members))
            .chain(&state.parked)
            .chain(&inbox.inbox)
            .chain(&inbox.working)
            .chain(&inbox.snoozed)
            .any(|row| row.thread == thread)
    };
    view.read_with(cx, |view, _| {
        assert!(listed(view, mine), "the operator's own is listed");
        assert!(!listed(view, worker), "the Worker is not");
        assert!(!listed(view, coordinator), "nor the parked Coordinator");
        assert!(view.cockpit.needs_you().is_empty(), "⌘D has nothing");
    });
    let rows = cx.update(|window, cx| view.read(cx).palette_thread_rows(window));
    assert!(rows.iter().any(|row| row.act == Act::Thread(mine)));
    assert!(
        !rows
            .iter()
            .any(|row| row.act == Act::Thread(worker) || row.act == Act::Thread(coordinator)),
        "the palette finds neither"
    );

    fake.streams.borrow()[0]
        .send(decision("perm_mine"))
        .unwrap();
    tick(cx);
    ring(cx);
    assert_eq!(toasts(cx), 1, "the operator's own request toasts");
}
