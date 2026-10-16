//! The board's secondary screens and its mouse, from fixture States in a
//! GPUI window (ADR 0031, Seam 3): the task panel beside the list and full
//! screen, the automation panel and its row's keys, the settings page, the
//! ? menu, find, the archive, other machines, the words of what is under
//! way; a click, the wheel, a drag or a double click that copies, and a
//! link that opens. Upstream's own tests check each screen's every word
//! against its world; these drive the vendored `update` with the board's
//! keys (what a key starts is not run) and read what the board view draws.

use std::sync::Arc;

use gpui::{px, Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use sw_board::board::job::Job;
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

/// A task whose second review failed, with evidence behind a link.
fn reviewed() -> Obj {
    let last = review(2, false);
    let mut shown = last.clone();
    shown["artifacts"] = json!([{ "link": "http://127.0.0.1:4/e/1", "caption": "The screenshot" }]);
    task(
        13,
        json!({ "state": "blocked", "kind": "permission", "stage": "approval", "title": "Cut titles between words",
        "pr": "https://github.com/a/b/pull/61", "prState": "failing", "reviews": [review(1, true), last], "review": shown, "reviewRound": 2,
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
        stderr: "bash: line 1: herdr-projects: command not found\n".into(),
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
        list.contains("vm-a: its tasks cannot be read: herdr-projects is not found there.")
            && list.contains("What came back:"),
        "{lines:#?}"
    );
    let ((scope, msg), lines) = ring(&mut b);
    assert_eq!((scope.as_str(), msg.as_str()), ("", "This machine only."));
    // One Server per machine: its session is `default`, and no header names it.
    assert!(!lines[0].contains("session"), "{}", lines[0]);
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
    let list = list_part(&b.lines());
    let row = line_of(&list, "Merge the menus");
    let x = sw_board::text::width(&list[row][..list[row].find("Merge").unwrap()]);
    let at = b.cell(x + 1, row);
    b.cx.simulate_event(gpui::MouseDownEvent {
        position: at,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    b.cx.run_until_parked();
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
