//! A Thread parked by an API caller (`thread.close`, `ferrite thread
//! park`) as a window parks one (ADR 0027): a Group member leaves its
//! Group first, and every window showing that Group follows, its board
//! left whole.

use super::*;
use crate::groups::{GroupChange, GroupId};
use crate::roster::{PaneIdentity, View};

/// A window on the harness's Server with `n` open Threads, the first
/// three in one Group in that order.
fn grouped(harness: &Harness, n: usize) -> (crate::cockpit::Cockpit, Vec<ThreadId>, GroupId) {
    let mut window = cockpit(harness);
    let threads: Vec<ThreadId> = (0..n)
        .map(|_| {
            let thread = window
                .open(
                    Provider::Claude,
                    crate::workspace::WorkspaceChoice::Main {
                        checkout: harness.dir.clone(),
                    },
                )
                .unwrap();
            harness
                .sessions
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
            thread
        })
        .collect();
    // The window holds every one before an API caller parks any.
    pump_until(&mut window, |window| {
        threads
            .iter()
            .all(|thread| window.thread(*thread).is_some())
    });
    let group = window
        .apply_group(GroupChange::Create {
            first: threads[0],
            second: threads[1],
        })
        .unwrap()
        .group
        .unwrap();
    window
        .apply_group(GroupChange::Join {
            thread: threads[2],
            group,
            index: None,
        })
        .unwrap();
    (window, threads, group)
}

fn members(window: &crate::cockpit::Cockpit, group: GroupId) -> Vec<ThreadId> {
    window
        .groups()
        .get(group)
        .map(|group| group.members.clone())
        .unwrap_or_default()
}

/// The board's own rule: on a Group's view every member holds a Pane, so
/// no slot of its Layout is left without a card.
fn every_member_has_a_pane(window: &crate::cockpit::Cockpit, group: GroupId) {
    for member in members(window, group) {
        assert!(
            window.roster().pane_of(member).is_some(),
            "member {member} holds a slot on the board with no Pane in it"
        );
    }
}

fn close(caller: &mut Caller, id: &str, thread: ThreadId) {
    let closed = caller.answer(id, "thread.close", serde_json::json!({ "thread": thread }));
    assert_eq!(
        closed["result"]["thread"],
        serde_json::json!(thread),
        "{closed}"
    );
}

/// The operator's report: another agent parks a member of the Group on
/// screen. It leaves the Group, as the window's park does, and the board
/// closes up around the two left; focus stays where it was.
#[test]
fn an_api_park_of_a_member_of_the_shown_group_leaves_it() {
    let harness = start("api-park-shown");
    let (mut window, threads, group) = grouped(&harness, 3);
    window.enter_group(group).unwrap();
    window.focus(PaneIdentity::Thread(threads[0]));

    close(&mut call(&harness), "1", threads[1]);
    pump_until(&mut window, |window| window.thread(threads[1]).is_none());

    assert_eq!(members(&window, group), [threads[0], threads[2]]);
    assert_eq!(window.roster().view(), View::Group(group));
    assert_eq!(
        window.visible(),
        [
            PaneIdentity::Thread(threads[0]),
            PaneIdentity::Thread(threads[2])
        ]
    );
    assert_eq!(window.roster().focused_thread(), Some(threads[0]));
    every_member_has_a_pane(&window, group);
    assert!(window.parked().unwrap().contains(&threads[1]));
}

/// Parked while the operator looks elsewhere: it leaves the Group all the
/// same, and the view and focus stay put.
#[test]
fn an_api_park_of_a_member_of_a_group_not_shown_leaves_it() {
    let harness = start("api-park-hidden");
    let (mut window, threads, group) = grouped(&harness, 4);
    window.focus(PaneIdentity::Thread(threads[3]));
    assert_eq!(window.roster().view(), View::Solo);

    close(&mut call(&harness), "1", threads[1]);
    pump_until(&mut window, |window| window.thread(threads[1]).is_none());

    assert_eq!(members(&window, group), [threads[0], threads[2]]);
    assert_eq!(window.roster().view(), View::Solo);
    assert_eq!(window.roster().focused_thread(), Some(threads[3]));

    window.enter_group(group).unwrap();
    every_member_has_a_pane(&window, group);
    assert_eq!(
        window.visible(),
        [
            PaneIdentity::Thread(threads[0]),
            PaneIdentity::Thread(threads[2])
        ]
    );
}

/// The focused Pane parked under the operator: as closing it does, the
/// member that took its ordinal takes focus.
#[test]
fn an_api_park_of_the_focused_member_lands_on_its_ordinal_survivor() {
    let harness = start("api-park-focused");
    let (mut window, threads, group) = grouped(&harness, 3);
    window.enter_group(group).unwrap();
    window.focus(PaneIdentity::Thread(threads[1]));

    close(&mut call(&harness), "1", threads[1]);
    pump_until(&mut window, |window| window.thread(threads[1]).is_none());

    assert_eq!(window.roster().view(), View::Group(group));
    assert_eq!(window.roster().focused_thread(), Some(threads[2]));
    every_member_has_a_pane(&window, group);
}

/// A pair parked down to one is no Group: the window falls back to Solo
/// on the survivor, as the window's own park leaves it.
#[test]
fn an_api_park_that_dissolves_the_shown_pair_lands_on_its_survivor() {
    let harness = start("api-park-pair");
    let (mut window, threads, group) = grouped(&harness, 3);
    window
        .apply_group(GroupChange::Leave { thread: threads[2] })
        .unwrap();
    window.enter_group(group).unwrap();
    window.focus(PaneIdentity::Thread(threads[1]));

    close(&mut call(&harness), "1", threads[1]);
    pump_until(&mut window, |window| window.thread(threads[1]).is_none());

    assert!(window.groups().get(group).is_none());
    assert_eq!(window.roster().view(), View::Solo);
    assert_eq!(window.roster().focused_thread(), Some(threads[0]));
}

/// Revived by the API, a Thread parked out of the Group comes back open,
/// as cmd-o brings it back: out of the Group, with the board on screen
/// left as it was.
#[test]
fn an_api_revive_after_an_api_park_reopens_the_thread_outside_the_group() {
    let harness = start("api-park-revive");
    let (mut window, threads, group) = grouped(&harness, 3);
    window.enter_group(group).unwrap();
    window.focus(PaneIdentity::Thread(threads[0]));
    let mut caller = call(&harness);
    close(&mut caller, "1", threads[1]);
    pump_until(&mut window, |window| window.thread(threads[1]).is_none());

    let revived = caller.answer(
        "2",
        "thread.revive",
        serde_json::json!({ "thread": threads[1] }),
    );
    assert!(revived.get("result").is_some(), "{revived}");
    pump_until(&mut window, |window| window.thread(threads[1]).is_some());

    assert_eq!(members(&window, group), [threads[0], threads[2]]);
    assert_eq!(window.roster().view(), View::Group(group));
    assert_eq!(window.roster().focused_thread(), Some(threads[0]));
    assert!(window.roster().pane_of(threads[1]).is_some());
    every_member_has_a_pane(&window, group);
}

/// A park that keeps the member's seat (a window's bare `park`, as an
/// older one sends): the window showing the Group keeps its Pane, now the
/// parked tile, rather than a slot with nothing in it — and a revive
/// wakes it in that slot.
#[test]
fn a_member_parked_in_its_seat_keeps_its_tile_on_the_board() {
    let harness = start("api-park-seat");
    let (mut window, threads, group) = grouped(&harness, 3);
    window.enter_group(group).unwrap();
    window.focus(PaneIdentity::Thread(threads[0]));
    let mut other = cockpit(&harness);
    other.park(threads[1]).unwrap();
    pump_until(&mut window, |window| window.thread(threads[1]).is_none());

    assert_eq!(members(&window, group), threads[..3]);
    every_member_has_a_pane(&window, group);
    assert_eq!(window.visible().len(), 3);

    let revived = call(&harness).answer(
        "1",
        "thread.revive",
        serde_json::json!({ "thread": threads[1] }),
    );
    assert!(revived.get("result").is_some(), "{revived}");
    pump_until(&mut window, |window| window.thread(threads[1]).is_some());
    assert_eq!(window.visible().len(), 3);
    every_member_has_a_pane(&window, group);
}

/// `project.close` parks "as closing their Panes does": its members
/// leave their Groups too.
#[test]
fn closing_a_project_parks_its_members_out_of_their_groups() {
    let harness = start("api-park-project");
    let (mut window, threads, group) = grouped(&harness, 3);
    window.enter_group(group).unwrap();
    let project = window
        .project_id(threads[0])
        .expect("the harness's Threads share a Project");
    let mut caller = call(&harness);
    let closed = caller.answer(
        "1",
        "project.close",
        serde_json::json!({ "project": crate::api::project_id(project) }),
    );
    assert!(closed.get("result").is_some(), "{closed}");
    pump_until(&mut window, |window| window.threads().is_empty());

    assert!(window.groups().get(group).is_none());
    assert_eq!(window.roster().view(), View::Solo);
}
