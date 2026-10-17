//! The board's secondary screens and its mouse, from fixture States in a
//! GPUI window (ADR 0031, Seam 3): the task panel beside the list and full
//! screen (its parts, its alerts, what gives way, a part that fails), the
//! automation panel and its row's keys, the settings page, the ? menu, find,
//! the archive, other machines (their headings, waits, actions, automations
//! and Coordinator, and going to their agents), the words of what is under
//! way; a click, the wheel over the list, the chat and the whole task, a
//! drag or a double click that copies, and a link that opens. Upstream's own tests check each screen's every word
//! against its world; these drive the vendored `update` with the board's
//! keys (what a key starts is not run) and read what the board view draws.

use std::sync::Arc;

use gpui::{px, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use sw_board::board::job::{Done, Job, Then};
use sw_board::board::keys::{key_name, Press};
use sw_board::board::state::{Pending, State};
use sw_board::board::update::{list::relist, update, Effect, Event};
use sw_board::json::*;
use sw_board::shown::ink::{Ink, Inked};

use super::render;
use super::runtime::{Board, BoardView};
use crate::theme::{CH, ROW};

const NOW: i64 = 1_800_000_000_000;
const MIN: i64 = 60_000;

/// Upstream's board reads its settings and keys from its home: an empty one
/// for these tests, and the clock stands still on the test's thread.
fn quiet_world() {
    static HOME: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    HOME.get_or_init(|| {
        let home =
            std::env::temp_dir().join(format!("ferrite-board-panels-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HERDR_PROJECTS_HOME", &home);
        home
    });
    sw_board::clock::set(Some(NOW));
}

fn task(n: u32, extra: Value) -> Obj {
    let mut o = obj(json!({
        "id": format!("proj/{n}"), "title": format!("Task {n}"), "created": NOW - 60 * MIN, "at": NOW - MIN,
        "owner": "me", "agent": "claude", "app": "claude", "reviews": [], "reviewRound": 0, "description": "",
    }));
    for (k, v) in extra.as_object().unwrap() {
        if v.is_null() {
            o.remove(k);
        } else {
            o.insert(k.clone(), v.clone());
        }
    }
    o
}

/// One task in each place of the list, as upstream's fixture rows have them.
fn tasks() -> Vec<Obj> {
    let long: Vec<String> = (1..=80)
        .map(|i| format!("Line {i} of a very long description."))
        .collect();
    vec![
        task(
            1,
            json!({ "agent": null, "app": "", "state": "idle", "stage": "todo", "title": "Write the docs" }),
        ),
        task(
            5,
            json!({ "state": "working", "stage": "working", "progress": 60, "eta": 11, "msg": "Found the cause", "title": "Fix the resize" }),
        ),
        task(
            9,
            json!({ "state": "blocked", "kind": "question", "stage": "asks", "msg": "Should the old settings file be removed?" }),
        ),
        task(
            11,
            json!({ "state": "blocked", "kind": "permission", "stage": "approval", "pr": "https://github.com/a/b/pull/60", "prState": "passing",
            "stat": " 19 files changed, 340 insertions(+), 27 deletions(-)", "msg": "Approve to merge?", "title": "Cut long titles",
            "description": long.join("\n") }),
        ),
        task(
            12,
            json!({ "state": "error", "stage": "failed", "msg": "Conflict with main", "title": "Merge the menus",
            "failure": { "stage": "merge", "phase": "sync", "title": "Conflict with main", "detail": "Conflicting files:\nsrc/menu.rs", "agentCanFix": true, "next": "Resolve the conflict." } }),
        ),
        task(
            15,
            json!({ "state": "done", "stage": "done", "msg": "Merged", "title": "Ship it" }),
        ),
        task(
            16,
            json!({ "state": "done", "stage": "done", "archivedAt": NOW - MIN, "msg": "Merged", "title": "Old zebra notes" }),
        ),
    ]
}

/// An automation as the loader reads it (`data::shown_auto`): its row and
/// its firings, newest first.
fn automation() -> Obj {
    let mut a = obj(json!({
        "id": "a1", "name": "Nightly deps", "on": true, "trigger": { "kind": "manual" }, "repo": "proj", "root": "/tmp/proj",
        "prompt": "Update the dependencies and say what changed.",
    }));
    let fired = vec![json!({ "at": NOW - 5 * MIN, "task": "proj/5", "what": "by hand" })];
    a.put(
        "shown",
        json!({ "firings": fired, "own": [], "settings": [] }),
    );
    a.put(
        "rowNow",
        serde_json::to_value(sw_board::automations::show::row_of(&a, NOW, Some(&fired))).unwrap(),
    );
    a
}

/// A board on this machine, its tasks read and its side panel on.
fn state(tasks: Vec<Obj>, autos: Vec<Obj>) -> State {
    quiet_world();
    let data = sw_board::board::data::Data {
        items: tasks.into_iter().map(Arc::new).collect(),
        autos,
        repo: "proj".into(),
        session: "default".into(),
        host: "here".into(),
        can_dispatch: true,
        loaded: true,
        seq: 1,
        ..Default::default()
    };
    let mut s = State::new(Default::default(), data);
    s.side = true;
    relist(&mut s);
    s
}

/// A key by its chord ("alt+t", "esc", "space"), or one character typed.
fn press_of(chord: &str) -> Press {
    match chord {
        "space" => Press::char(' '),
        "esc" => Press::chord("escape"),
        k if k.chars().count() == 1 => Press::char(k.chars().next().unwrap()),
        k => Press::chord(k),
    }
}

/// The board view in a test window, at a size in cells.
struct Shown<'a> {
    view: Entity<BoardView>,
    cx: &'a mut VisualTestContext,
}

fn shown(cx: &mut TestAppContext, state: State, columns: usize, rows: usize) -> Shown<'_> {
    let board = Board {
        state,
        theme: render::ferrite_theme(false),
    };
    let (view, cx) = cx.add_window_view(|_, cx| BoardView::still(board, cx));
    let mut shown = Shown { view, cx };
    shown.fit(columns, rows);
    shown
}

impl Shown<'_> {
    fn fit(&mut self, columns: usize, rows: usize) {
        self.view.update(self.cx, |view, cx| {
            view.fit((columns as f32 + 0.5) * CH, (rows as f32 + 0.5) * ROW);
            cx.notify();
        });
        self.cx.run_until_parked();
    }

    /// The board's state, changed or read; the view drawn again.
    fn state<R>(&mut self, f: impl FnOnce(&mut State) -> R) -> R {
        let out = self.view.update(self.cx, |view, cx| {
            cx.notify();
            f(view.state().expect("a still board has its state"))
        });
        self.cx.run_until_parked();
        out
    }

    /// One key through `update`; what it starts is answered, not run.
    fn press(&mut self, chord: &str) -> Vec<Effect> {
        self.state(|s| update(s, Event::Key(press_of(chord))))
    }

    fn typed(&mut self, text: &str) {
        for c in text.chars() {
            self.state(|s| update(s, Event::Key(Press::char(c))));
        }
    }

    fn select(&mut self, id: &str) {
        self.state(|s| {
            s.sel = id.into();
            relist(s);
        });
    }

    /// The lines the view draws now.
    fn inked(&mut self) -> Vec<Inked> {
        self.cx.run_until_parked();
        self.view.update(self.cx, |view, _| view.screen())
    }

    /// Those lines as text, each without the spaces at its end.
    fn lines(&mut self) -> Vec<String> {
        self.inked()
            .iter()
            .map(|l| l.text().trim_end().to_string())
            .collect()
    }

    /// The middle of a cell, on the window.
    fn cell(&mut self, x: usize, y: usize) -> Point<Pixels> {
        let origin = self
            .cx
            .debug_bounds("projects-board")
            .expect("the board is drawn")
            .origin;
        gpui::point(
            origin.x + px((x as f32 + 0.5) * CH),
            origin.y + px((y as f32 + 0.5) * ROW),
        )
    }

    fn click(&mut self, x: usize, y: usize) {
        let at = self.cell(x, y);
        self.cx.simulate_click(at, Modifiers::default());
        self.cx.run_until_parked();
    }

    fn wheel(&mut self, x: usize, y: usize, lines: f32) {
        let position = self.cell(x, y);
        self.cx.simulate_event(gpui::ScrollWheelEvent {
            position,
            delta: gpui::ScrollDelta::Lines(gpui::point(0., lines)),
            modifiers: Modifiers::none(),
            touch_phase: gpui::TouchPhase::default(),
        });
        self.cx.run_until_parked();
    }

    fn clipboard(&mut self) -> Option<String> {
        self.cx
            .update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
    }
}

fn has(lines: &[String], words: &str) -> bool {
    lines.iter().any(|l| l.contains(words))
}

fn line_of(lines: &[String], words: &str) -> usize {
    lines
        .iter()
        .position(|l| l.contains(words))
        .unwrap_or_else(|| panic!("no line with {words:?}: {lines:#?}"))
}

/// What stands left of the side panel's divider, on each line: the list.
fn list_part(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|l| l.split(" │ ").next().unwrap_or_default().to_string())
        .collect()
}

/// What stands right of the side panel's divider, on each line.
fn beside(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|l| {
            l.split_once(" │ ")
                .map(|(_, r)| r.trim_end().to_string())
                .unwrap_or_default()
        })
        .collect()
}

// ------------------------------------------------------------- the screens

#[gpui::test]
fn the_task_panel_stands_beside_the_list_from_90_columns_on_its_sidebar_ink(
    cx: &mut TestAppContext,
) {
    let mut b = shown(cx, state(tasks(), vec![]), 120, 40);
    b.select("proj/11");
    let lines = b.lines();
    // 38% of 120 columns, between 38 and 70: the list keeps 120 - 45 - 3.
    let list_w = 120 - 45 - 3;
    assert_eq!(lines[0].chars().nth(list_w + 1), Some('│'), "{}", lines[0]);
    let side = beside(&lines);
    assert!(
        side.iter().any(|l| l.contains("#11 Cut long titles")),
        "{side:#?}"
    );
    assert!(
        side.iter().any(|l| l.trim_start().starts_with("Status ─")),
        "{side:#?}"
    );
    assert!(
        side.iter().any(|l| l.trim_start().starts_with("State ")),
        "{side:#?}"
    );
    assert!(
        side.iter()
            .any(|l| l.trim_start().starts_with("Change ") && l.contains("PR#60")),
        "{side:#?}"
    );
    // A long description is cut short, and says where the rest is.
    let more = format!("{} shows the whole task", key_name("panel"));
    assert!(side.iter().any(|l| l.trim() == more), "{side:#?}");
    // The panel is drawn on the sidebar's ink, past its divider.
    let ink = b.inked();
    assert_eq!(ink[0].0.last().unwrap().bg, Some(Ink::Sidebar));
}

#[gpui::test]
fn the_task_panel_says_nothing_is_selected_and_needs_90_columns(cx: &mut TestAppContext) {
    let mut b = shown(cx, state(vec![], vec![]), 100, 30);
    assert!(has(&b.lines(), "│  Nothing selected."), "{:#?}", b.lines());
    b.state(|s| {
        s.data.items = tasks().into_iter().map(Arc::new).collect();
        s.sel = "proj/11".into();
        relist(s);
    });
    b.fit(89, 30);
    let narrow = b.lines();
    assert!(
        !narrow[0].contains('│'),
        "below 90 columns the list has the window: {}",
        narrow[0]
    );
    // Turned on in a narrow window, it says where the task is to be read.
    b.press("alt+p");
    b.press("alt+p");
    assert_eq!(
        b.state(|s| s.msg.clone()),
        format!(
            "The side panel needs a wider window. {} shows the task full screen.",
            key_name("panel")
        )
    );
}

fn review(round: u32, pass: bool) -> Value {
    let findings = if pass {
        json!([])
    } else {
        json!([{ "id": "F1", "severity": "critical", "subject": "A title is cut in the middle of a word", "where": "src/titles.rs:14" }])
    };
    json!({ "round": round, "meets": pass, "gate": "major", "pass": pass, "at": NOW - (3 - round as i64) * MIN,
        "text": if pass { "The change meets the task." } else { "A title is cut in the middle of a word." },
        "findings": findings, "checks": ["Ran the whole check."] })
}

/// A task whose second review failed, with evidence behind a link. Its pull
/// request's checks pass: failing checks would come first (`awaits_yes`).
fn reviewed() -> Obj {
    let last = review(2, false);
    let mut shown = last.clone();
    shown["artifacts"] = json!([{ "link": "http://127.0.0.1:4/e/1", "caption": "The screenshot" }]);
    task(
        13,
        json!({ "state": "blocked", "kind": "permission", "stage": "approval", "title": "Cut titles between words",
        "pr": "https://github.com/a/b/pull/61", "reviews": [review(1, true), last], "review": shown, "reviewRound": 2,
        "msg": "Approve to merge? Review: Fail · 1 critical." }),
    )
}

#[gpui::test]
fn the_whole_task_shows_every_review_its_findings_and_evidence(cx: &mut TestAppContext) {
    let mut tasks = tasks();
    tasks.push(reviewed());
    let mut b = shown(cx, state(tasks, vec![]), 120, 80);
    b.select("proj/13");
    b.press("alt+t");
    let lines = b.lines();
    for words in [
        "Review failed",
        "Reviews · 2",
        "History",
        "Review 2",
        "Review 1",
        "Findings",
        "Evidence",
        "↗ The screenshot",
    ] {
        assert!(has(&lines, words), "{words}: {lines:#?}");
    }
    let ink = b.inked();
    let pieces: Vec<_> = ink.iter().flat_map(|l| &l.0).collect();
    assert!(
        pieces
            .iter()
            .any(|p| p.t.trim() == "critical" && p.c == Ink::Red),
        "a critical finding is red"
    );
    // The evidence opens from its arrow's words.
    let link = super::links::links(&ink)
        .into_iter()
        .find(|l| l.url == "http://127.0.0.1:4/e/1")
        .expect("the evidence is a link");
    b.click(link.x, link.y);
    assert_eq!(b.cx.opened_url().as_deref(), Some("http://127.0.0.1:4/e/1"));
}

#[gpui::test]
fn the_whole_task_has_its_own_screen_and_scrolls(cx: &mut TestAppContext) {
    let mut b = shown(cx, state(tasks(), vec![]), 160, 30);
    b.select("proj/11");
    b.press("alt+t");
    assert!(b.state(|s| s.full));
    let lines = b.lines();
    assert!(
        lines[0].starts_with(" Task details  #11 \"Cut long titles\""),
        "{}",
        lines[0]
    );
    let length = b.state(|s| s.panel_length);
    assert!(
        lines[0].contains("lines 1–") && lines[0].ends_with(&format!("of {length}")),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].chars().all(|c| c == '─'),
        "a rule under the title: {}",
        lines[1]
    );
    // Its text keeps to 100 columns however wide the window.
    let height = b.state(|s| s.panel_height);
    assert!(lines[2..2 + height]
        .iter()
        .all(|l| sw_board::text::width(l) <= 100));
    b.press("down");
    assert!(b.lines()[0].contains("lines 2–"));
    b.press("pageDown");
    assert!(
        b.state(|s| s.panel_top) > 1,
        "a page down goes a page at a time"
    );
    let mut whole = vec![];
    for _ in 0..10 {
        b.press("pageDown");
        whole.extend(b.lines());
    }
    assert!(
        has(&whole, "Line 80 of a very long description."),
        "the whole description is read to its end"
    );
    // An alert has its own part: what went wrong, and what is next.
    b.select("proj/12");
    b.state(|s| s.panel_top = 0);
    let lines = b.lines();
    assert!(lines[0].contains("#12 \"Merge the menus\""), "{}", lines[0]);
    assert!(
        has(&lines, "Went wrong") && has(&lines, "Resolve the conflict."),
        "{lines:#?}"
    );
}

#[gpui::test]
fn an_automation_opens_whole_its_firings_lead_to_their_tasks_and_its_row_keys_act(
    cx: &mut TestAppContext,
) {
    let mut b = shown(cx, state(tasks(), vec![automation()]), 120, 40);
    b.select("auto:a1");
    assert!(
        b.state(|s| s.auto().is_some()),
        "the automation's row is selected"
    );
    // space turns it off, tab fires it, ctrl+x asks before it removes it.
    assert!(b
        .press("space")
        .iter()
        .any(|e| matches!(e, Effect::Run(Job::AutoTurn { on: false, .. }))));
    b.state(|s| s.pending.clear());
    assert!(b
        .press("tab")
        .iter()
        .any(|e| matches!(e, Effect::Run(Job::AutoFire { .. }))));
    b.state(|s| s.pending.clear());
    b.press("ctrl+x");
    assert_eq!(
        b.state(|s| s.ask.as_ref().map(|a| a.q.clone())).as_deref(),
        Some("Remove the automation \"Nightly deps\"? The tasks it made stay.")
    );
    b.press("esc");
    // enter opens it whole: its name on top, its firings, the arrows pick one.
    b.press("enter");
    assert!(b.state(|s| s.auto_open));
    let lines = b.lines();
    assert!(
        lines[0].starts_with(" Automation  Nightly deps"),
        "{}",
        lines[0]
    );
    assert!(
        has(&lines, "Firings ─") && has(&lines, "Update the dependencies"),
        "{lines:#?}"
    );
    b.press("down");
    let lines = b.lines();
    assert!(
        lines
            .iter()
            .any(|l| l.trim_start().starts_with("› ") && l.contains("→ made")),
        "{lines:#?}"
    );
    // enter on a firing goes to the task it made.
    b.press("enter");
    assert_eq!(
        b.state(|s| (s.auto_open, s.sel.clone())),
        (false, "proj/5".to_string())
    );
    // ctrl+e on its row opens its settings page.
    b.select("auto:a1");
    b.press("ctrl+e");
    assert!(b.state(|s| s.editing.as_ref().is_some_and(|e| e.auto)));
    assert!(b.lines()[0].starts_with(" Settings  Nightly deps"));
}

#[gpui::test]
fn the_settings_page_names_whose_settings_they_are(cx: &mut TestAppContext) {
    let mut tasks = tasks();
    tasks[3].put(
        "snap",
        json!({ "problems": [], "settings": [
        { "name": "worker.agent", "kind": "work", "value": "claude/opus", "own": true },
        { "name": "review.rounds", "kind": "work", "value": 3, "own": false },
    ] }),
    );
    let mut b = shown(cx, state(tasks, vec![]), 120, 40);
    b.select("proj/11");
    b.press("ctrl+e");
    assert!(
        b.state(|s| s.full && s.editing.is_some()),
        "the settings page opens full screen"
    );
    let lines = b.lines();
    assert!(
        lines[0].starts_with(" Settings  #11 \"Cut long titles\""),
        "{}",
        lines[0]
    );
    // The picked row is marked, its own value starred, and the foot says
    // what the star means and where the board's settings are.
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("▌›") || l.starts_with(" › ")),
        "{lines:#?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains(" * ") && l.contains("worker.agent")),
        "{lines:#?}"
    );
    assert!(has(&lines, "* this task only"), "{lines:#?}");
    assert!(
        has(&lines, "Board settings are yours only: herdr-projects show"),
        "{lines:#?}"
    );
}

#[gpui::test]
fn the_menu_opens_with_nothing_picked_in_three_groups_and_scrolls(cx: &mut TestAppContext) {
    let mut b = shown(cx, state(tasks(), vec![]), 120, 200);
    b.select("proj/5");
    b.press("?");
    assert_eq!(
        b.state(|s| s.menu.as_ref().map(|m| m.at)),
        Some(-1),
        "nothing is picked"
    );
    let tall = b.lines();
    assert!(
        tall[0].starts_with(" Menu  for #5 \"Fix the resize\""),
        "{}",
        tall[0]
    );
    for group in [
        "plain keys",
        "ctrl: does something",
        "alt: only shows or hides",
        "on a row",
    ] {
        assert!(tall.iter().any(|l| l.trim() == group), "{group}: {tall:#?}");
    }
    for legend in ["R#2", "PR#16", "[auto]", "65% 2m left"] {
        assert!(has(&tall, legend), "{legend}");
    }
    // Shorter than the menu, the window scrolls and says how much is below.
    b.fit(120, 24);
    let short = b.lines();
    assert!(
        short
            .iter()
            .any(|l| l.trim_start().starts_with("↓ ") && l.contains(" more")),
        "{short:#?}"
    );
    // The arrows pick a ctrl or alt line.
    b.press("down");
    assert!(b.state(|s| s.menu.as_ref().is_some_and(|m| m.at >= 0)));
    b.fit(120, 200);
    assert!(b
        .lines()
        .iter()
        .any(|l| l.contains("❯ ctrl+") || l.contains("❯ alt+")));
}

#[gpui::test]
fn find_filters_live_with_the_archive_and_an_exact_tag_selects_its_task(cx: &mut TestAppContext) {
    let mut b = shown(cx, state(tasks(), vec![]), 120, 30);
    b.select("proj/1");
    b.press("ctrl+f");
    b.typed("zebra");
    let lines = b.lines();
    let all = b.state(|s| s.items.len());
    assert!(has(&lines, "find ❯ zebra"), "{lines:#?}");
    assert!(
        has(&lines, &format!("filter \"zebra\" · 1 of {all}")),
        "{lines:#?}"
    );
    assert!(
        has(&lines, "Old zebra notes"),
        "find takes archived tasks in: {lines:#?}"
    );
    for _ in 0.."zebra".len() {
        b.press("backspace");
    }
    b.typed("#12");
    assert_eq!(
        b.state(|s| s.sel.clone()),
        "proj/12",
        "a task's own tag selects it"
    );
}

#[gpui::test]
fn the_archive_shows_only_archived_tasks_read_only(cx: &mut TestAppContext) {
    let mut b = shown(cx, state(tasks(), vec![]), 120, 30);
    b.press("alt+a");
    assert!(b.state(|s| s.archive_view));
    let lines = b.lines();
    assert!(lines[0].contains("archive · 1 archived"), "{}", lines[0]);
    assert!(
        has(&lines, "Old zebra notes") && !has(&lines, "Fix the resize"),
        "{lines:#?}"
    );
    b.select("proj/16");
    let lines = b.lines();
    assert!(
        lines.last().unwrap().contains("restore this task"),
        "{lines:#?}"
    );
    assert!(has(&beside(&lines), "Archived"), "{lines:#?}");
    b.press("alt+a");
    assert!(!b.state(|s| s.archive_view));
}

fn far_task(n: u32) -> Value {
    json!({ "id": format!("far/{n}"), "title": format!("Theirs {n}"), "state": "idle", "stage": "todo", "at": NOW - MIN })
}

fn machine(label: &str) -> sw_board::remote::Machine {
    sw_board::remote::Machine {
        label: label.into(),
        target: format!("me@{label}"),
        ..Default::default()
    }
}

/// alt+m once: whose board it is now, what it said, and what it shows.
fn ring(b: &mut Shown) -> ((String, String), Vec<String>) {
    b.press("alt+m");
    let said = b.state(|s| (s.scope.clone(), s.msg.clone()));
    (said, b.lines())
}

#[gpui::test]
fn other_machines_ring_from_this_one_to_all_to_each_and_say_who_did_not_answer(
    cx: &mut TestAppContext,
) {
    let mut b = shown(cx, state(tasks(), vec![]), 120, 40);
    // None yet: alt+m says how to add one.
    b.press("alt+m");
    b.state(|s| update(s, Event::Machines(vec![])));
    let (scope, msg) = b.state(|s| (s.scope.clone(), s.msg.clone()));
    assert!(
        scope.is_empty()
            && msg.starts_with("No other machines. Add one in Settings, under Machines"),
        "{msg}"
    );
    // The machines in machine.list's order; a new list asks each at once.
    let fx = b.state(|s| update(s, Event::Machines(vec![machine("vm-b"), machine("vm-a")])));
    assert!(fx.contains(&Effect::Poll(true)));
    let order = b.state(|s| {
        s.order
            .iter()
            .filter(|m| !m.is_empty())
            .cloned()
            .collect::<Vec<_>>()
    });
    assert_eq!(order, ["vm-b", "vm-a"]);
    let answer = Ok(json!({ "host": "vm-b", "version": "1.0.1", "tasks": [far_task(1)] }));
    b.state(|s| {
        update(
            s,
            Event::Remote {
                label: "vm-b".into(),
                answer,
            },
        )
    });
    let ((scope, msg), lines) = ring(&mut b);
    assert_eq!(
        (scope.as_str(), msg.as_str()),
        ("*", "Every machine you can reach.")
    );
    assert!(
        lines[0].contains("all machines: vm-b, vm-a (no answer)"),
        "{}",
        lines[0]
    );
    assert!(
        has(&lines, "Theirs 1") && has(&lines, "Write the docs"),
        "{lines:#?}"
    );
    assert!(has(&lines, "vm-a: asking for its tasks…"), "{lines:#?}");
    // Every machine shown, the beat polls them every fifth second.
    let polls = (1..=5)
        .map(|n| b.state(|s| update(s, Event::Tick(NOW + n * 1000))))
        .filter(|fx| fx.contains(&Effect::Poll(false)))
        .count();
    assert_eq!(polls, 1);
    let ((scope, msg), lines) = ring(&mut b);
    assert_eq!(
        (scope.as_str(), msg.as_str()),
        ("vm-b", "Only vm-b. What you type goes to vm-b.")
    );
    assert!(
        lines[0].contains("only vm-b")
            && has(&list_part(&lines), "Theirs 1")
            && !has(&list_part(&lines), "Write the docs"),
        "{lines:#?}"
    );
    // One that cannot be reached: why, in its own words, on its own board.
    let ended = sw_board::remote::why::Ended {
        code: Some(127),
        stderr: "bash: line 1: ferrited: command not found\n".into(),
        ..Default::default()
    };
    let why = sw_board::remote::why::why_not(&ended, "vm-a", "me@vm-a");
    b.state(|s| {
        update(
            s,
            Event::Remote {
                label: "vm-a".into(),
                answer: Err(why),
            },
        )
    });
    let ((scope, _), lines) = ring(&mut b);
    assert_eq!(scope, "vm-a");
    assert!(lines[0].contains("only vm-a (no answer)"), "{}", lines[0]);
    let list = list_part(&lines)
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        list.contains("vm-a: its tasks cannot be read: ferrited is not found there.")
            && list.contains("What came back:"),
        "{lines:#?}"
    );
    let ((scope, msg), lines) = ring(&mut b);
    assert_eq!((scope.as_str(), msg.as_str()), ("", "This machine only."));
    // One Server per machine: its session is `default`, and no header names it.
    assert!(!lines[0].contains("session"), "{}", lines[0]);
}

/// vm-b as it answers: a task to do, one at work with its agent, one that
/// waits for a task here, an automation and its Coordinator.
fn vm_b_answers() -> Event {
    let at_work = json!({ "id": "far/2", "title": "Theirs 2", "state": "working", "stage": "working", "agent": "claude", "app": "claude",
        "msg": "Building", "at": NOW - MIN, "link": { "pane": "w2:p1" } });
    let waits = json!({ "id": "far/3", "title": "Theirs 3", "state": "working", "stage": "after", "at": NOW - MIN,
        "after": ["here:proj/5"], "afterLeft": ["here:proj/5"], "afterLine": "queued after here:PRJ-5" });
    let weekly = json!({ "id": "b1", "name": "Weekly report", "on": true, "trigger": { "kind": "manual" }, "repo": "far",
        "prompt": "Write the weekly report.", "shown": { "firings": [], "own": [], "settings": [] } });
    let coordinator = json!({ "state": "idle", "readable": true,
        "talk": [{ "you": true, "text": "What is left?" }, { "you": false, "text": "Two tasks on vm-b." }] });
    Event::Remote {
        label: "vm-b".into(),
        answer: Ok(
            json!({ "host": "vm-b", "version": "1.0.1", "tasks": [far_task(1), at_work, waits],
            "automations": [weekly], "coordinator": coordinator }),
        ),
    }
}

/// A board on this machine and vm-b, every machine shown; one task here
/// waits for one of vm-b's.
fn with_vm_b(cx: &mut TestAppContext, columns: usize) -> Shown<'_> {
    let mut all = tasks();
    all.push(task(
        20,
        json!({ "state": "working", "stage": "after", "title": "Use their build", "after": ["vm-b:far/2"], "afterLeft": ["vm-b:far/2"] }),
    ));
    let mut s = state(all, vec![]);
    // As the runtime opens it (`prepare`): a window, never a bare terminal.
    s.ctx.in_herdr = true;
    let mut b = shown(cx, s, columns, 50);
    b.state(|s| update(s, Event::Machines(vec![machine("vm-b")])));
    b.state(|s| update(s, vm_b_answers()));
    b.press("alt+m");
    b
}

#[gpui::test]
fn every_machine_shown_has_its_heading_its_waits_and_its_automations_and_acts_there(
    cx: &mut TestAppContext,
) {
    let mut b = with_vm_b(cx, 120);
    let lines = b.lines();
    let list = list_part(&lines);
    for machine in ["here", "vm-b"] {
        assert!(
            list.iter()
                .any(|l| l.trim_start().starts_with(&format!("▾ {machine} ─"))),
            "{machine}: {list:#?}"
        );
    }
    let ink = b.inked();
    assert!(
        ink.iter()
            .flat_map(|l| &l.0)
            .any(|p| p.t == "vm-b" && p.c == Ink::Accent && p.bold),
        "a machine's heading is bold in the accent"
    );
    // Waits across machines: here for vm-b's task, there for one of here.
    assert!(has(&list, "queued after vm-b:far/2"), "{list:#?}");
    assert!(has(&list, "queued after here:PRJ-5"), "{list:#?}");
    assert!(has(&list, "Weekly report"), "{list:#?}");
    // An action on vm-b's task goes to vm-b, and says so meanwhile.
    b.select("vm-b:far/2");
    let fx = b.press("ctrl+x");
    assert!(
        fx.iter().any(|e| matches!(e, Effect::Run(Job::Far { host, action, .. }) if host == "vm-b" && action == "stop")),
        "{fx:?}"
    );
    let msg = b.state(|s| s.msg.clone());
    assert!(
        msg.starts_with("Stopping ") && msg.ends_with(" on vm-b…"),
        "{msg}"
    );
    b.state(|s| s.pending.clear());
    // So does one on vm-b's automation.
    b.select("vm-b:auto:b1");
    let fx = b.press("space");
    assert!(
        fx.iter().any(|e| matches!(e, Effect::Run(Job::Far { host, action, .. }) if host == "vm-b" && action == "auto-turn")),
        "{fx:?}"
    );
}

#[gpui::test]
fn on_vm_bs_board_what_you_type_goes_to_its_coordinator(cx: &mut TestAppContext) {
    let mut b = with_vm_b(cx, 120);
    b.press("alt+m");
    assert_eq!(b.state(|s| s.scope.clone()), "vm-b");
    assert!(has(&b.lines(), "Coordinator on vm-b"), "{:#?}", b.lines());
    b.typed("Start the report");
    let fx = b.press("enter");
    assert!(
        fx.iter().any(|e| matches!(e, Effect::Run(Job::Dispatch { machine, text, .. }) if machine.label == "vm-b" && text == "Start the report")),
        "{fx:?}"
    );
}

#[gpui::test]
fn entering_another_machines_agent_says_to_go_there_from_the_nav(cx: &mut TestAppContext) {
    let mut b = with_vm_b(cx, 160);
    b.select("vm-b:far/2");
    let fx = b.press("enter");
    assert!(
        fx.iter().any(
            |e| matches!(e, Effect::Run(Job::Far { host, action, req, .. })
            if host == "vm-b" && action == "reopen" && req.at("focus") == &json!(true))
        ),
        "vm-b is asked to put it in front: {fx:?}"
    );
    // What vm-b answered (`job_far`, tests/ferrite_remote_open.rs runs it).
    let there = Done::default().and(Then::OpenedThere {
        id: "vm-b:far/2".into(),
        host: "vm-b".into(),
        agent: true,
    });
    b.state(|s| update(s, Event::Done(there)));
    assert_eq!(
        b.state(|s| s.msg.clone()),
        "vm-b: its agent is in front there. Pick vm-b in the nav to go to it: “Theirs 2”; local there brings you back here."
    );
    assert!(
        has(&b.lines(), "Pick vm-b in the nav to go to it"),
        "{:#?}",
        b.lines()
    );
}

#[gpui::test]
fn what_a_key_started_shows_on_its_row_at_once_and_red_when_it_did_not_work(
    cx: &mut TestAppContext,
) {
    let mut b = shown(cx, state(tasks(), vec![]), 120, 40);
    b.select("proj/5");
    assert!(b
        .press("ctrl+x")
        .iter()
        .any(|e| matches!(e, Effect::Run(Job::Stop { .. }))));
    let lines = b.lines();
    let list: Vec<&str> = lines
        .iter()
        .map(|l| l.split(" │ ").next().unwrap_or_default())
        .collect();
    assert!(list.iter().any(|l| l.contains("Stopping…")), "{lines:#?}");
    assert!(
        beside(&lines)
            .iter()
            .any(|l| l.trim_start().starts_with("Now") && l.contains("Stopping…")),
        "{lines:#?}"
    );
    // A second press waits for the first.
    assert!(!b
        .press("ctrl+x")
        .iter()
        .any(|e| matches!(e, Effect::Run(_))));
    let msg = b.state(|s| s.msg.clone());
    assert!(
        msg.starts_with("Still stopping") && msg.ends_with(". One moment."),
        "{msg}"
    );
    // It did not work: the row says so in red.
    b.state(|s| {
        let failed = Pending {
            word: "Stopping…".into(),
            fail: "Could not stop".into(),
            at: NOW,
            failed: Some("Could not stop".into()),
            kind: "stop".into(),
            ..Default::default()
        };
        s.pending.insert("proj/5".into(), failed);
        relist(s);
    });
    let ink = b.inked();
    assert!(ink
        .iter()
        .flat_map(|l| &l.0)
        .any(|p| p.t.contains("Could not stop") && p.c == Ink::Red));
    // For ten seconds, then the row is its own again.
    let red = |b: &mut Shown| has(&b.lines(), "Could not stop");
    b.state(|s| update(s, Event::Tick(NOW + 9_999)));
    assert!(red(&mut b));
    b.state(|s| update(s, Event::Tick(NOW + 10_000)));
    assert!(!red(&mut b));
}

/// The task panel beside the list for `id`: what stands right of its divider.
fn side_of(b: &mut Shown, id: &str) -> Vec<String> {
    b.select(id);
    beside(&b.lines())
}

/// A task whose plan waits for your yes, with one setting of its own, and
/// one done twice at once that waits for a pick.
fn planned() -> Vec<Obj> {
    let tried = |letter: &str| json!({ "letter": letter, "stage": "tried", "agent": "claude", "created": NOW - 10 * MIN, "triedAt": NOW - 2 * MIN });
    vec![
        task(
            21,
            json!({ "state": "blocked", "kind": "permission", "stage": "plan", "title": "Plan the cache",
            "plan": { "writer": { "kind": "claude" }, "at": NOW - 5 * MIN, "link": "http://127.0.0.1:4/p/21", "file": "plan.md" },
            "snap": { "plan": "Step one.\nStep two.", "problems": [],
                "settings": [{ "name": "worker.agent", "value": "claude", "place": "task" }] } }),
        ),
        task(
            22,
            json!({ "state": "blocked", "kind": "permission", "stage": "pick", "title": "Pick a parser",
            "tries": [tried("A"), tried("B")], "attempts": [{ "letter": "A", "agent": "claude" }, { "letter": "B", "agent": "claude" }],
            "comparison": { "pick": "B", "why": "Smaller change" } }),
        ),
    ]
}

#[gpui::test]
fn the_task_panel_has_the_plan_the_attempts_and_the_settings_and_full_screen_the_session(
    cx: &mut TestAppContext,
) {
    let mut b = shown(cx, state(planned(), vec![]), 120, 70);
    let side = side_of(&mut b, "proj/21");
    assert!(
        has(&side, "Plan ─") && has(&side, "waits for your yes") && has(&side, "Step one."),
        "{side:#?}"
    );
    assert!(has(&side, "Open plan"), "{side:#?}");
    let link = super::links::links(&b.inked())
        .into_iter()
        .find(|l| l.url == "http://127.0.0.1:4/p/21");
    assert!(link.is_some(), "the plan's words open it");
    // A value set for this task only is starred, and the foot says what the
    // star means; the agent's session is the whole task's alone.
    assert!(
        side.iter().any(|l| l.contains("default model *")),
        "{side:#?}"
    );
    assert!(has(&side, "* this task only"), "{side:#?}");
    assert!(!has(&side, "Session ─"), "{side:#?}");
    b.press("alt+t");
    let whole = b.lines();
    assert!(
        has(&whole, "Session ─") && has(&whole, "Task id"),
        "{whole:#?}"
    );
    b.press("alt+t");
    // The attempts: the one picked by the arrows marked ›, the reviewer's ★.
    b.state(|s| {
        s.attempt_at.insert("proj/22".into(), "A".into());
    });
    let side = side_of(&mut b, "proj/22");
    assert!(has(&side, "Pick one of 2"), "{side:#?}");
    assert!(has(&side, "A ›") && has(&side, "B ★"), "{side:#?}");
    assert!(has(&side, "recommends B: Smaller change"), "{side:#?}");
}

/// One task in each state whose panel says what it needs from you.
fn alerted() -> Vec<Obj> {
    vec![
        task(
            30,
            json!({ "state": "error", "stage": "failed", "title": "Cut off task",
            "failure": { "at": NOW - MIN, "title": "Provider overloaded", "detail": "529 overloaded" },
            "waiting": { "who": "agent", "for": NOW - MIN, "reason": "model is overloaded", "until": NOW + MIN, "error": "529 overloaded" } }),
        ),
        task(
            31,
            json!({ "state": "blocked", "kind": "auth", "stage": "asks", "msg": "Needs a login", "title": "Log in task" }),
        ),
        task(
            32,
            json!({ "state": "blocked", "kind": "permission", "stage": "approval", "title": "Behind task", "msg": "Approve to merge?",
            "syncBlocked": { "why": "main has moved on" } }),
        ),
        task(
            33,
            json!({ "state": "error", "stage": "stopped", "title": "Limit task", "msg": "Stopped at its time limit",
            "limitStop": { "what": "time", "used": 600_000, "role": "worker", "limit": "10m" } }),
        ),
        task(
            34,
            json!({ "state": "working", "stage": "working", "title": "Handed task", "msg": "Going on",
            "handovers": [{ "at": NOW - 2 * MIN, "role": "worker", "from": "claude", "to": "codex", "why": "out of allowance" }] }),
        ),
    ]
}

#[gpui::test]
fn the_task_panel_says_what_each_task_needs_from_you(cx: &mut TestAppContext) {
    let mut all = tasks();
    all.extend(alerted());
    let mut b = shown(cx, state(all, vec![]), 120, 70);
    for (id, said) in [
        ("proj/30", ["Cut off ─", "Cut off by"]),
        ("proj/31", ["Needs a login ─", "Log in task"]),
        ("proj/32", ["Behind main ─", "Cannot be approved yet:"]),
        (
            "proj/33",
            ["Stopped at a limit ─", "Its work is kept as it is"],
        ),
        ("proj/34", ["Took over", "took over from"]),
    ] {
        let side = side_of(&mut b, id);
        for words in said {
            assert!(has(&side, words), "{id}: {words}: {side:#?}");
        }
    }
    // Archived, in the whole task too: read-only, and how to restore it.
    b.press("alt+a");
    b.select("proj/16");
    b.press("alt+t");
    let whole = b.lines();
    assert!(
        has(&whole, "Archived ─") && has(&whole, "Read-only."),
        "{whole:#?}"
    );
}

#[gpui::test]
fn beside_the_list_what_is_least_needed_gives_way_first(cx: &mut TestAppContext) {
    let described = |b: &mut Shown| {
        let side = side_of(b, "proj/11");
        let lines = side
            .iter()
            .filter(|l| l.contains("of a very long description"))
            .count();
        (side, lines)
    };
    let mut b = shown(cx, state(tasks(), vec![]), 120, 60);
    let (_, tall) = described(&mut b);
    b.fit(120, 16);
    let (side, short) = described(&mut b);
    assert!(
        short < tall,
        "the description gives way: {tall} then {short}"
    );
    assert!(
        side.iter().any(|l| l.trim_start().starts_with("State ")),
        "where the task stands stays: {side:#?}"
    );
    let more = format!("{} shows the whole task", key_name("panel"));
    assert!(side.iter().any(|l| l.trim() == more), "{side:#?}");
}

/// Upstream's board_more.rs: a part that fails to draw says so in its place,
/// in red under its own headline; drawn whole, no panel says so.
#[gpui::test]
fn a_part_that_fails_to_draw_says_so_in_its_place(cx: &mut TestAppContext) {
    use sw_board::board::view::{guard, panel::kit::Kit};
    let why =
        guard::part(|| -> usize { panic!("{}", "artifactLines is not defined") }).unwrap_err();
    assert_eq!(why, "artifactLines is not defined");
    let mut k = Kit::new(46);
    k.failed("Reviews", &why);
    let lines: Vec<String> = Kit::out(k.parts[0].lines.clone())
        .iter()
        .map(|l| l.text())
        .collect();
    assert!(lines[0].trim().starts_with("Reviews ─"), "{lines:?}");
    assert_eq!(
        lines[1..]
            .iter()
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join(" "),
        "✗ This part could not be shown: artifactLines is not defined"
    );
    assert!(k.parts[0].lines[1..]
        .iter()
        .all(|l| l.0.iter().all(|p| p.c == Ink::Red)));
    let mut b = shown(cx, state(tasks(), vec![]), 120, 40);
    b.select("proj/11");
    for _ in 0..2 {
        assert!(!has(&b.lines(), "could not be shown"));
        b.press("alt+t");
    }
}

// --------------------------------------------------------------- the mouse

#[gpui::test]
fn a_click_selects_a_row_and_folds_a_heading(cx: &mut TestAppContext) {
    let mut b = shown(cx, state(tasks(), vec![]), 120, 40);
    b.select("proj/1");
    let row = line_of(&b.lines(), "Ship it");
    b.click(6, row);
    assert_eq!(b.state(|s| s.sel.clone()), "proj/15");
    let heading = b
        .lines()
        .iter()
        .position(|l| l.contains('▾'))
        .expect("an open heading");
    b.click(6, heading);
    assert!(
        !b.state(|s| s.folded.is_empty()),
        "a click on a heading folds it"
    );
    // Right of the list, in the side panel, a click selects nothing.
    b.select("proj/1");
    b.click(100, row);
    assert_eq!(b.state(|s| s.sel.clone()), "proj/1");
}

#[gpui::test]
fn with_a_draft_a_click_selects_nothing_and_says_why(cx: &mut TestAppContext) {
    let mut b = shown(cx, state(tasks(), vec![]), 120, 40);
    b.select("proj/1");
    b.state(|s| s.input.insert("Half typed"));
    let row = line_of(&b.lines(), "Ship it");
    b.click(6, row);
    let (sel, msg) = b.state(|s| (s.sel.clone(), s.msg.clone()));
    assert_eq!(sel, "proj/1");
    assert_eq!(
        msg,
        "Send or put away what you typed first: a click does not change who gets it."
    );
}

#[gpui::test]
fn the_wheel_moves_between_tasks_and_in_the_menu_a_click_does_nothing(cx: &mut TestAppContext) {
    let mut b = shown(cx, state(tasks(), vec![]), 120, 40);
    b.select("proj/1");
    let row = line_of(&b.lines(), "Write the docs");
    b.wheel(6, row, -1.);
    let moved = b.state(|s| s.sel.clone());
    assert_ne!(moved, "proj/1", "a step down moves to the next row");
    b.wheel(6, row, 1.);
    assert_eq!(b.state(|s| s.sel.clone()), "proj/1", "a step up moves back");
    b.press("?");
    b.wheel(6, 5, -1.);
    assert!(
        b.state(|s| s.menu.as_ref().is_some_and(|m| m.at >= 0)),
        "over the menu the wheel picks"
    );
    b.click(6, 5);
    assert_eq!(b.state(|s| s.sel.clone()), "proj/1");
}

#[gpui::test]
fn a_drag_copies_the_boards_text_and_a_double_click_a_word(cx: &mut TestAppContext) {
    let mut b = shown(cx, state(tasks(), vec![]), 120, 40);
    b.select("proj/1");
    assert!(
        b.lines()[0].starts_with(" herdr-projects  "),
        "{}",
        b.lines()[0]
    );
    let (from, to) = (b.cell(1, 0), b.cell(14, 0));
    b.cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    b.cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
    let painted = b.view.update(b.cx, |view, _| {
        let lines = view.screen();
        view.pointer.frame(lines)
    });
    assert!(
        painted[0]
            .0
            .iter()
            .any(|p| p.inv && p.t.contains("projects")),
        "what is selected is drawn in reverse"
    );
    b.cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    b.cx.run_until_parked();
    assert_eq!(b.clipboard().as_deref(), Some("herdr-projects"));
    assert_eq!(b.state(|s| s.msg.clone()), "Copied 14 characters.");
    // Two presses on one cell of the selected row (whose click changes
    // nothing), as upstream counts a double click: the board's clock stands
    // still, so they are within 400 ms.
    b.select("proj/12");
    let list = list_part(&b.lines());
    let row = line_of(&list, "Merge the menus");
    let x = sw_board::text::width(&list[row][..list[row].find("Merge").unwrap()]);
    b.click(x + 1, row);
    b.click(x + 2, row);
    assert_eq!(
        b.clipboard().as_deref(),
        Some("herdr-projects"),
        "two clicks on neighbouring cells copy nothing"
    );
    b.click(x + 2, row);
    assert_eq!(b.clipboard().as_deref(), Some("Merge"));
    assert_eq!(b.state(|s| s.msg.clone()), "Copied 5 characters.");
}

#[gpui::test]
fn a_click_on_a_link_opens_it(cx: &mut TestAppContext) {
    let mut b = shown(cx, state(tasks(), vec![]), 120, 40);
    b.select("proj/11");
    let ink = b.inked();
    let link = super::links::links(&ink)
        .into_iter()
        .find(|l| l.url == "https://github.com/a/b/pull/60")
        .expect("the panel links its pull request");
    b.click(link.x + 1, link.y);
    assert_eq!(
        b.cx.opened_url().as_deref(),
        Some("https://github.com/a/b/pull/60")
    );
    assert_eq!(
        b.state(|s| s.sel.clone()),
        "proj/11",
        "the link took the click"
    );
}

/// `n` tasks to do, numbered.
fn numbered(n: u32) -> Vec<Obj> {
    (1..=n)
        .map(|i| {
            task(
                i,
                json!({ "agent": null, "app": "", "state": "idle", "stage": "todo", "title": format!("Task number {i}") }),
            )
        })
        .collect()
}

/// The wheel turned `by` steps (down positive, as upstream's `by`) over a
/// line of the board.
fn turn(b: &mut Shown, y: usize, by: i64) {
    b.wheel(5, y, -(by as f32));
}

/// The first line of the chat's message, as upstream's board_chat_scroll.rs
/// reads it: inside the box, past the line that names who said it.
fn chat_first(lines: &[String]) -> String {
    let (top, end) = chat_box(lines);
    lines[top + 1..end]
        .iter()
        .map(|l| {
            let c: Vec<char> = l.chars().collect();
            c[2..c.len() - 2]
                .iter()
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .find(|l| !l.starts_with("Coordinator"))
        .unwrap_or_default()
}

/// The chat box's first and last line.
fn chat_box(lines: &[String]) -> (usize, usize) {
    let top = lines
        .iter()
        .rposition(|l| l.starts_with("╭─") && l.ends_with('╮'))
        .expect("a box");
    let end = lines
        .iter()
        .rposition(|l| l.starts_with("╰─") && l.ends_with('╯'))
        .expect("a box's end");
    (top, end)
}

#[gpui::test]
fn the_wheel_over_the_chat_scrolls_it_two_lines_a_step_also_with_a_draft(cx: &mut TestAppContext) {
    let mut s = state(numbered(30), vec![]);
    s.side = false;
    // What the Coordinator said, as the loader reads it (upstream's `apply`).
    let answer: Vec<String> = (1..=40)
        .map(|i| format!("Line {i} of the answer."))
        .collect();
    let c = &mut s.data.coord;
    c.state = "idle".into();
    c.readable = true;
    c.said = answer.join("\n");
    c.turn = Some(sw_board::agents::read_turn::Turn {
        id: "turn-1".into(),
        ..Default::default()
    });
    let mut b = shown(cx, s, 120, 30);
    let (top, end) = chat_box(&b.lines());
    turn(&mut b, top + 3, 1);
    assert_eq!(chat_first(&b.lines()), "Line 2 of the answer.");
    turn(&mut b, top - 1, 3);
    assert_eq!(chat_first(&b.lines()), "Line 8 of the answer.");
    turn(&mut b, end, -1);
    assert_eq!(chat_first(&b.lines()), "Line 6 of the answer.");
    b.typed("a draft");
    turn(&mut b, top + 1, 2);
    assert_eq!(chat_first(&b.lines()), "Line 10 of the answer.");
    assert_eq!(b.state(|s| s.value()), "a draft");
    turn(&mut b, top + 1, -100);
    assert_eq!(chat_first(&b.lines()), "Line 1 of the answer.");
}

#[gpui::test]
fn the_wheel_scrolls_the_whole_task_also_with_a_draft(cx: &mut TestAppContext) {
    let description: Vec<String> = (1..=80).map(|i| format!("Description line {i}.")).collect();
    let mut one = numbered(1);
    one[0].put("description", description.join("\n"));
    let mut b = shown(cx, state(one, vec![]), 120, 30);
    b.select("proj/1");
    b.press("alt+t");
    assert!(b.lines()[0].contains("lines 1–"), "{}", b.lines()[0]);
    turn(&mut b, 5, 1);
    assert!(b.lines()[0].contains("lines 3–"), "{}", b.lines()[0]);
    assert_eq!(b.state(|s| s.sel.clone()), "proj/1");
    turn(&mut b, 5, 2);
    assert!(b.lines()[0].contains("lines 7–"), "{}", b.lines()[0]);
    b.typed("typing");
    turn(&mut b, 5, -1);
    assert!(b.lines()[0].contains("lines 5–"), "{}", b.lines()[0]);
    assert_eq!(b.state(|s| s.value()), "typing");
}

#[gpui::test]
fn with_a_draft_the_wheel_moves_the_view_and_never_who_gets_it(cx: &mut TestAppContext) {
    let mut s = state(numbered(22), vec![]);
    s.side = false;
    let mut b = shown(cx, s, 120, 20);
    // The list's first task, the view at its top.
    let first = b.state(|s| {
        s.list_ids
            .iter()
            .find(|id| !id.is_empty() && !id.starts_with("head:"))
            .cloned()
            .expect("a task in the list")
    });
    b.select(&first);
    b.press("space");
    b.typed("pebbles please");
    let top = b.state(|s| s.top);
    turn(&mut b, 4, 1);
    turn(&mut b, 4, 1);
    let (sel, moved, value) = b.state(|s| (s.sel.clone(), s.top, s.value()));
    assert_eq!(sel, first);
    assert!(moved > top, "the view moves: {top} then {moved}");
    assert_eq!(value, "pebbles please");
}
