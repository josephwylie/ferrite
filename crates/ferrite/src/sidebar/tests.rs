//! The board's sidebar entries as the nav draws them, from fixture tasks as
//! the board shows them: upstream's `tests/board_rows_sidebar.rs`, its
//! expectations on the native rows.

use super::*;
use crate::nav::task_lines;
use sw_board::flow::sidebar_entry::{task_name, CAP};

/// The board's data under a home of the tests' own, never the operator's.
fn isolated() {
    static HOME: std::sync::Once = std::sync::Once::new();
    HOME.call_once(|| {
        let home = std::env::temp_dir().join(format!("ferrite-sidebar-{}", std::process::id()));
        std::env::set_var("HERDR_PROJECTS_HOME", home);
    });
}

const NOW: i64 = 1_800_000_000_000;
const MINUTE: i64 = 60_000;
const AGENT: u64 = 7;
const REVIEWER: u64 = 8;

/// The adapter's id of the pane a Thread was placed in: never the Thread's.
fn pane(thread: u64) -> String {
    format!("p{thread}")
}

/// Each placed pane's open Thread, as the agent list gives them.
fn threads() -> HashMap<String, ThreadId> {
    [AGENT, REVIEWER, 11, 12]
        .into_iter()
        .map(|thread| (pane(thread), ThreadId::new(thread)))
        .collect()
}

fn sidebar(rows: &[Obj]) -> Sidebar {
    of(rows, &threads(), "here", NOW)
}

/// A task as the board shows it, its agent live in Thread 7, with `patch`.
fn task(patch: Value) -> Obj {
    let mut task = obj(json!({
        "id": "proj/1",
        "title": "Show automations in the main list",
        "agent": "claude",
        "app": "claude",
        "link": { "pane": pane(AGENT), "socket": "here" },
        "live": { "status": "working" },
    }));
    task.extend(obj(patch));
    task
}

fn told(rows: &[Obj], thread: u64) -> Option<Told> {
    sidebar(rows).agents.get(&ThreadId::new(thread)).cloned()
}

/// The nav's lines under a root row: row 2, then row 3 where it says
/// something.
fn lines(told: &Told) -> Vec<String> {
    task_lines(&told.rows(), 2.0)
        .into_iter()
        .map(|(text, _)| text.to_string())
        .collect()
}

/// Every stage: row 1 the task's number and title, row 2 who is on it and
/// where it stands with its review round and pull request, row 3 its
/// numbers bare — never `left`, never the row's own second line.
#[test]
fn every_stage_says_where_it_stands_in_the_status_words() {
    isolated();
    let failed_once = json!({ "round": 1, "gate": "major", "meets": true, "pass": false, "findings": [{ "id": "F1", "severity": "critical", "where": "a.js", "what": "Broken" }] });
    let passed_once =
        json!({ "round": 1, "gate": "major", "meets": true, "pass": true, "findings": [] });
    let stages = [
        (
            json!({ "stage": "starting", "state": "working", "note": "Starting" }),
            "board \u{b7} starting",
            "",
        ),
        (
            json!({ "stage": "setup", "state": "working", "note": "Setting up" }),
            "board \u{b7} setting up",
            "",
        ),
        (
            json!({ "stage": "working", "state": "working", "note": "Reading the list code", "progress": 8, "eta": 16 }),
            "agent \u{b7} working",
            "8% 16m",
        ),
        (
            json!({ "stage": "working", "state": "working", "note": "Just started", "plain": true, "progress": 0 }),
            "agent \u{b7} working",
            "0%",
        ),
        (
            json!({ "stage": "working", "state": "blocked", "kind": "permission", "note": "Queued: open the agent and answer its dialog first" }),
            "asks for your OK",
            "",
        ),
        (
            json!({ "stage": "checking", "state": "working", "note": "Running the check \u{b7} 12 of 40", "progress": 30, "eta": 2 }),
            "board \u{b7} checking",
            "30% 2m",
        ),
        (
            json!({ "stage": "pushing", "state": "working", "note": "Pushing the branch" }),
            "board \u{b7} pushing",
            "",
        ),
        (
            json!({ "stage": "reviewing", "state": "working", "reviewRound": 1, "note": "Reading the diff", "progress": 40, "eta": 3, "pr": "https://example.com/pull/113" }),
            "agent \u{b7} under review \u{b7} R#1 PR#113",
            "40% 3m",
        ),
        (
            json!({ "stage": "reviewneeds", "state": "blocked", "reviewRound": 1, "note": "The reviewer needs you" }),
            "needs you \u{b7} R#1",
            "",
        ),
        (
            json!({ "stage": "asks", "state": "blocked", "kind": "question", "note": "Sand or pebbles?" }),
            "asks you",
            "",
        ),
        (
            json!({ "stage": "asks", "state": "blocked", "kind": "permission", "note": "Need your OK to delete the test data" }),
            "asks for your OK",
            "",
        ),
        (
            json!({ "stage": "asks", "state": "blocked", "kind": "auth", "note": "Need a GitHub login to open the PR" }),
            "needs a login",
            "",
        ),
        (
            json!({ "stage": "failed", "state": "blocked", "kind": "auth", "failure": { "auth": true, "title": "Not signed in" } }),
            "needs a login",
            "",
        ),
        (
            json!({ "stage": "failed", "state": "error", "failure": { "title": "Check failed" }, "note": "Check failed" }),
            "went wrong",
            "",
        ),
        (
            json!({ "stage": "nosummary", "state": "error", "note": "Stopped without a summary" }),
            "needs you",
            "",
        ),
        (
            json!({ "stage": "approval", "state": "blocked", "kind": "permission", "reviewRound": 1, "review": failed_once, "reviews": [failed_once], "pr": "https://example.com/pull/113" }),
            "review failed \u{b7} R#1 PR#113",
            "",
        ),
        (
            json!({ "stage": "approval", "state": "blocked", "kind": "permission", "reviewRound": 1, "review": passed_once, "reviews": [passed_once], "pr": "https://example.com/pull/113" }),
            "ready to approve \u{b7} R#1 PR#113",
            "",
        ),
        (
            json!({ "stage": "approval", "state": "blocked", "kind": "permission", "syncBlocked": { "why": "main moved and the merge conflicts" }, "note": "main moved and the merge conflicts" }),
            "behind main",
            "",
        ),
        (
            json!({ "stage": "approval", "state": "blocked", "kind": "permission", "mergeFailed": true, "failure": { "title": "The merge failed" }, "note": "The merge failed" }),
            "went wrong",
            "",
        ),
        (
            json!({ "stage": "merging", "state": "working", "note": "Merging" }),
            "board \u{b7} merging",
            "",
        ),
        (
            json!({ "stage": "queued", "state": "working", "queuePlace": 2, "note": "Waiting for a free slot" }),
            "queued",
            "Q#2",
        ),
        (
            json!({ "stage": "after", "state": "working", "afterLine": "queued after SKL-6" }),
            "queued after SKL-6",
            "",
        ),
        (
            json!({ "stage": "stopped", "state": "idle", "note": "Stopped" }),
            "to do",
            "",
        ),
        (
            json!({ "stage": "done", "state": "done", "note": "Merged into main." }),
            "done",
            "",
        ),
    ];
    for (patch, stands, numbers) in stages {
        let name = format!(
            "{} {} {}",
            patch.s("stage"),
            patch.s("state"),
            patch.s("kind")
        );
        let row = task(patch);
        let told = told(std::slice::from_ref(&row), AGENT).expect(&name);
        assert_eq!(told.entry.name, task_name(&row), "{name}: row 1");
        assert!(
            told.entry
                .name
                .ends_with(" Show automations in the main list"),
            "{name}: the number first"
        );
        let want: Vec<String> = [stands, numbers]
            .into_iter()
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect();
        assert_eq!(lines(&told), want, "{name}");
        assert!(
            !lines(&told).iter().any(|line| line.contains("left")),
            "{name}: never \"left\""
        );
        let note = row.s("note");
        if !note.is_empty() {
            assert!(
                !lines(&told).iter().any(|line| line.contains(note)),
                "{name}: never the row's second line"
            );
        }
        assert_eq!(told.ink, status_ink(&as_row(&row)), "{name}: its row's ink");
    }
}

/// Under review the task has two entries, each its own Thread's: the
/// reviewer's and the agent's whose work is reviewed.
#[test]
fn a_review_has_the_agents_entry_and_the_reviewers() {
    isolated();
    let row = task(json!({
        "stage": "reviewing", "state": "working", "reviewRound": 2, "note": "Reading the diff", "progress": 40,
        "link": { "pane": pane(AGENT), "socket": "here", "reviewer": { "pane": pane(REVIEWER) } },
    }));
    let rows = std::slice::from_ref(&row);
    let agent = told(rows, AGENT).expect("the agent's entry");
    let reviewer = told(rows, REVIEWER).expect("the reviewer's entry");
    assert_eq!(
        agent.entry.name, reviewer.entry.name,
        "one task, named alike"
    );
    assert_eq!(
        lines(&agent),
        ["agent \u{b7} under review \u{b7} R#2", "40%"]
    );
    assert_eq!(
        lines(&reviewer),
        ["reviewer \u{b7} under review \u{b7} R#2", "40%"]
    );
}

/// The row's time by the minute, last on row 2: how long it has worked,
/// how long it has waited, `<1m` before the first.
#[test]
fn row_two_ends_with_the_rows_time_by_the_minute() {
    isolated();
    let at = NOW - 10 * MINUTE;
    let stands = |patch: Value| told(&[task(patch)], AGENT).unwrap().rows().stands_timed;
    assert_eq!(
        stands(json!({ "stage": "working", "state": "working", "at": at, "since": at })).as_ref(),
        "agent \u{b7} working \u{b7} 10m"
    );
    assert_eq!(
        stands(json!({ "stage": "asks", "state": "blocked", "kind": "question", "at": at }))
            .as_ref(),
        "asks you \u{b7} 10m ago"
    );
    assert_eq!(
        stands(
            json!({ "stage": "working", "state": "working", "at": NOW - 5000, "since": NOW - 5000 })
        )
        .as_ref(),
        "agent \u{b7} working \u{b7} <1m"
    );
}

/// The tags, the hours to go bare, and the 80 characters herdr keeps: a
/// longer name is cut with `…`, never its number.
#[test]
fn tags_hours_and_the_cap_are_upstreams() {
    isolated();
    let one = |patch: Value| told(&[task(patch)], AGENT).unwrap();
    assert_eq!(
        lines(&one(
            json!({ "stage": "working", "state": "working", "research": true, "automation": { "id": "a1" } })
        )),
        ["agent \u{b7} working \u{b7} [research] [auto]"]
    );
    assert_eq!(
        one(json!({ "stage": "working", "state": "working", "progress": 1, "eta": 1200 }))
            .rows()
            .third
            .as_ref(),
        "1% 20h"
    );
    let long = one(
        json!({ "title": "T".repeat(100), "stage": "asks", "state": "blocked", "kind": "question" }),
    );
    assert_eq!(long.entry.name.chars().count(), CAP);
    assert!(long.entry.name.ends_with('\u{2026}'));
    assert!(long.entry.name.contains(" TTT"), "the number stays whole");
}

/// No entry where herdr's sidebar has none: an archived task, a task on
/// another machine's herdr, a pane gone or a herdr down, a task nobody is
/// on.
#[test]
fn only_a_live_agent_on_this_machine_has_an_entry() {
    isolated();
    let working = json!({ "stage": "working", "state": "working" });
    let with = |more: Value| {
        let mut row = task(working.clone());
        row.extend(obj(more));
        row
    };
    assert!(told(&[with(json!({}))], AGENT).is_some());
    for (why, row) in [
        ("archived", with(json!({ "archivedAt": NOW }))),
        (
            "another machine's",
            with(json!({ "live": { "away": "far" } })),
        ),
        ("its pane gone", with(json!({ "live": { "gone": true } }))),
        ("its herdr down", with(json!({ "live": { "down": true } }))),
        ("never looked up", with(json!({ "live": null }))),
        (
            "nobody on it",
            with(json!({ "stage": "todo", "state": "idle", "link": {} })),
        ),
    ] {
        assert!(sidebar(&[row]).agents.is_empty(), "{why}");
    }
}

/// A task done several times at once: each attempt's agent its own entry,
/// saying `pick one` while the task waits for your pick.
#[test]
fn a_tasks_attempts_waiting_for_a_pick_say_pick_one() {
    isolated();
    let attempt = |n: u64| {
        json!({
            "id": format!("proj/1.{n}"), "title": "Show automations in the main list", "stage": "tried",
            "state": "done", "link": { "pane": pane(n), "socket": "here" }, "live": { "status": "idle" },
        })
    };
    let row = task(
        json!({ "stage": "pick", "state": "blocked", "kind": "permission", "link": {}, "tries": [attempt(11), attempt(12)] }),
    );
    let rows = std::slice::from_ref(&row);
    for thread in [11, 12] {
        let told = told(rows, thread).expect("an attempt's entry");
        assert!(
            lines(&told)[0].starts_with("pick one"),
            "{:?}",
            lines(&told)
        );
    }
    assert_eq!(sidebar(rows).agents.len(), 2, "the task itself has no pane");
}

/// A task's own space is named with its number and title, its numbers
/// under it, by the Space the adapter named: a Project's id, or a linked
/// worktree's path.
#[test]
fn a_tasks_own_space_carries_its_name_and_numbers() {
    isolated();
    for space in ["3", "/repo/.worktrees/hsw-152"] {
        let row = task(json!({
            "stage": "working", "state": "working", "progress": 8, "eta": 16,
            "link": { "pane": pane(AGENT), "socket": "here", "workspace": space, "ownsWorkspace": true },
        }));
        let sidebar = sidebar(std::slice::from_ref(&row));
        let entry = &sidebar.spaces[space];
        assert_eq!(
            (entry.name.as_str(), entry.progress.as_str()),
            (task_name(&row).as_str(), "8% 16m"),
            "{space}"
        );
    }
    let shared = task(
        json!({ "stage": "working", "state": "working", "link": { "pane": pane(AGENT), "socket": "here", "workspace": "3" } }),
    );
    assert!(
        sidebar(&[shared]).spaces.is_empty(),
        "a space the task does not own keeps its name"
    );
}

/// A task that waits for your pick has no pane: its own space keeps the
/// task's number and title, as upstream leaves the name it wrote, and loses
/// its numbers. Another machine's task, or one in a space it does not own,
/// names nothing here.
#[test]
fn a_space_waiting_for_a_pick_keeps_its_name_without_numbers() {
    isolated();
    let waiting = |link: Value| {
        task(
            json!({ "stage": "pick", "state": "blocked", "kind": "permission", "progress": 8, "eta": 16, "link": link }),
        )
    };
    let row = waiting(json!({ "socket": "here", "workspace": "s1", "ownsWorkspace": true }));
    let named = sidebar(std::slice::from_ref(&row));
    let entry = &named.spaces["s1"];
    assert_eq!(
        (entry.name.as_str(), entry.progress.as_str()),
        (task_name(&row).as_str(), "")
    );
    for (link, why) in [
        (
            json!({ "socket": "there", "workspace": "s1", "ownsWorkspace": true }),
            "another machine's",
        ),
        (
            json!({ "socket": "here", "workspace": "3" }),
            "a space it does not own",
        ),
    ] {
        assert!(sidebar(&[waiting(link)]).spaces.is_empty(), "{why}");
    }
}

/// A Project heading says a task's own space when every Thread under it the
/// adapter places is in that Space: a Space the board made, a worktree's,
/// a Project's own. Threads in two Spaces, or none placed, say nothing.
#[test]
fn a_heading_says_the_space_its_threads_share() {
    let entry = |name: &str| Entry {
        name: name.into(),
        ..coordinator_entry("idle", 0, "")
    };
    let (made, tree) = ("s2", "/repo/.worktrees/hsw-152");
    let mut sidebar = Sidebar::default();
    sidebar.spaces.insert(made.into(), entry("HSW-152"));
    sidebar.spaces.insert(tree.into(), entry("SKL-6"));
    for (thread, space) in [(1, made), (2, made), (3, tree), (4, "3")] {
        sidebar.places.insert(ThreadId::new(thread), space.into());
    }
    let heading = |threads: &[u64]| {
        sidebar
            .heading(threads.iter().copied().map(ThreadId::new))
            .map(|entry| entry.name.clone())
    };
    assert_eq!(heading(&[1, 2]).as_deref(), Some("HSW-152"), "a made Space");
    assert_eq!(
        heading(&[3, 9]).as_deref(),
        Some("SKL-6"),
        "9 is not placed"
    );
    assert_eq!(heading(&[1, 3]), None, "two Spaces");
    assert_eq!(heading(&[4]), None, "a Project's own Space no task owns");
    assert_eq!(heading(&[9]), None, "nothing placed");
}

/// A pane's Thread is the agent's terminal in the adapter's agent list, in
/// the Space the agent names: a placed pane keeps its own id, and a pane
/// with no open Thread, or herdr down, names none.
#[test]
fn a_panes_thread_is_its_agents_terminal() {
    let agents = [
        (
            "p3",
            json!({ "pane_id": "p3", "terminal_id": "7", "workspace_id": "s1", "agent_status": "working" }),
        ),
        (
            "12",
            json!({ "pane_id": "12", "terminal_id": "12", "agent_status": "idle" }),
        ),
        ("p4", json!({ "pane_id": "p4", "terminal_id": "p4" })),
    ];
    let session = Session::Up {
        agents: agents
            .into_iter()
            .map(|(pane, agent)| (pane.to_string(), agent))
            .collect(),
        panes: HashMap::new(),
    };
    let threads = threads_of(&session);
    assert_eq!(threads.get("p3"), Some(&ThreadId::new(7)));
    assert_eq!(threads.get("12"), Some(&ThreadId::new(12)));
    assert_eq!(threads.get("p4"), None);
    assert!(threads_of(&Session::Down).is_empty());
    let places = places_of(&session);
    assert_eq!(
        places.get(&ThreadId::new(7)).map(String::as_str),
        Some("s1")
    );
    assert_eq!(places.len(), 1, "no Space named, none placed");
    assert!(places_of(&Session::Down).is_empty());
}

/// A task whose pane has no open Thread here is no nav row: its agent
/// ended, or the pane was never started.
#[test]
fn a_pane_with_no_open_thread_has_no_row() {
    isolated();
    let row = task(
        json!({ "stage": "working", "state": "working", "link": { "pane": "p9", "socket": "here" } }),
    );
    assert!(sidebar(&[row]).agents.is_empty());
}

/// The bottom bar's tally is the board header's: how many tasks stand in
/// each group, archived ones aside.
#[test]
fn the_tally_is_the_board_headers() {
    isolated();
    let rows = [
        task(json!({ "id": "proj/1", "stage": "asks", "state": "blocked", "kind": "question" })),
        task(
            json!({ "id": "proj/2", "stage": "approval", "state": "blocked", "kind": "permission" }),
        ),
        task(json!({ "id": "proj/3", "stage": "working", "state": "working" })),
        task(json!({ "id": "proj/4", "stage": "checking", "state": "working" })),
        task(json!({ "id": "proj/5", "stage": "todo", "state": "idle", "link": {} })),
        task(json!({ "id": "proj/6", "stage": "done", "state": "done", "archivedAt": NOW })),
    ];
    assert_eq!(
        sidebar(&rows).tally,
        "1 needs you \u{b7} 1 ready \u{b7} 2 working \u{b7} 1 to do"
    );
    assert_eq!(sidebar(&[]).tally, "", "no tasks, nothing to say");
}

/// The Coordinator: named `Coordinator`, where it stands as the line above
/// the board's box says it with what waits to go in, its latest message
/// dim under it.
#[test]
fn the_coordinators_entry_says_where_it_stands_and_its_last_message() {
    let stands = |state: &str| coordinator_stands(state, "");
    let told = coordinator_told(coordinator_entry(
        &stands("idle"),
        1,
        "Done.\nBoth tasks are added.",
    ));
    assert_eq!(told.entry.name, "Coordinator");
    assert_eq!(
        lines(&told),
        [
            "Coordinator \u{b7} idle \u{b7} 1 message waits to be sent",
            "Done. Both tasks are added."
        ]
    );
    for (state, said) in [
        ("working", "Coordinator \u{b7} working"),
        ("blocked", "Coordinator \u{b7} asks you"),
    ] {
        let told = coordinator_told(coordinator_entry(&stands(state), 0, ""));
        assert_eq!(lines(&told), [said]);
    }
}

/// Only the Coordinator's message is dim, as the board greys it: where it
/// stands is plain text.
#[test]
fn only_the_coordinators_message_is_dim() {
    let told = coordinator_told(coordinator_entry("idle", 0, "Done."));
    let inks: Vec<u32> = task_lines(&told.rows(), 2.0)
        .into_iter()
        .map(|(_, ink)| ink)
        .collect();
    assert_eq!(inks, [theme::TEXT, theme::CHROME_MUTED]);
}
