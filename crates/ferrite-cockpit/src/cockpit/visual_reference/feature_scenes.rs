//! Scenes for the features the first references predate: the work board
//! (every group, a row that asks, the chat and each panel) and its rows in
//! the nav (ADR 0031), the Review slot (ADR
//! 0021) at rest and mid-slide, a file in the Panel's preview at rest and
//! mid-ride, find in a
//! transcript (ADR 0020), the nav's PR labels and the four
//! states (ADR 0015), a mermaid diagram (ADR 0016) and a finish toast (ADR
//! 0017). Every one is seeded: no network, no provider CLI, no `gh`, and
//! nothing of the operator's (the projects root is under the scene's own).

use ferrite_core::protocol::{
    ReviewPullRequest, WireCheck, WireCheckStatus, WireMerge, WirePr, WirePrComment,
    WirePrCommentKind, WirePrState, WireReply, WireSync,
};
use ferrite_core::store::Provider;
use ferrite_core::workspace::{CheckState, PrState, PullRequest};
use ferrite_core::{SessionEvent, TurnOutcome};
use ferrite_reviewr::diff::Notice;
use ferrite_reviewr::ferrite::FileSides;
use ferrite_reviewr::git::CommitRow;
use ferrite_reviewr::model::{ChangeKind, ChangedFile, Comment, Rev, Scope as ReviewScope, Side};
use ferrite_reviewr::search::{CodeHit, FileHit, SearchResults};
use gpui::{AppContext, Focusable};

use super::{conversation, Scene, Setup};
use crate::board::{fixture, Board, Placement};
use crate::review::ReviewView;
use crate::sidebar::{self, Sidebar};
use ferrite_core::ThreadId;
use sw_board::board::keys::Press;
use sw_board::board::update::{list::relist, update, Event};
use sw_board::flow::sidebar_entry::coordinator_entry;
use sw_board::json::*;
use sw_board::status_words::coordinator_stands;

/// Every state this module draws, at the app size.
pub(super) const STATES: &[&str] = &[
    "projects-nav",
    "projects-board",
    "projects-board-end",
    "projects-board-asks",
    "projects-board-chat",
    "projects-board-panel",
    "projects-board-task",
    "projects-board-plan",
    "projects-board-attempts",
    "projects-board-settings",
    "projects-board-pickers",
    "projects-coordinator",
    "projects-board-automation",
    "projects-board-menu",
    "projects-board-find",
    "projects-board-archive",
    "review-uncommitted",
    "review-commits",
    "review-pr",
    "review-search",
    "reader-line",
    "reader-slide",
    "review-slide",
    "review-comment",
    "review-notices",
    "review-error",
    "find",
    "nav-states",
    "mermaid",
    "toast-done",
];

pub(super) fn handles(state: &str) -> bool {
    STATES.contains(&state)
}

pub(super) fn build(state: &str) -> (Scene, Setup) {
    match state {
        "projects-nav" => projects(state, None),
        "projects-coordinator" => projects(state, Some(on_board("projects-board"))),
        board if board.starts_with("projects-board") => {
            let shown = on_board(board);
            projects(state, Some(shown))
        }
        "review-uncommitted" => review(Staged::Uncommitted),
        "review-commits" => review(Staged::Commits),
        "review-pr" => review(Staged::Pr),
        "review-search" => review(Staged::Search),
        "reader-line" => reader(None),
        "reader-slide" => reader(Some(0.5)),
        "review-slide" => {
            // The Review halfway through its slide in beside the Pane.
            let (scene, setup) = review(Staged::Uncommitted);
            let held: Setup = Box::new(move |view, window, cx| {
                view.fixture_hold_reader(Some(0.5));
                setup(view, window, cx)
            });
            (scene, held)
        }
        "review-comment" => review(Staged::Comment),
        "review-notices" => review(Staged::Notices),
        "review-error" => review(Staged::Error),
        "find" => find(),
        "nav-states" => nav_states(),
        "mermaid" => mermaid(),
        "toast-done" => toast_done(),
        _ => unreachable!("`handles` lists every state"),
    }
}

// ------------------------------------------------------------------ projects

/// What a board scene shows: the board, and the keys pressed on it (what a
/// key starts is not run).
struct OnBoard {
    board: Board,
    keys: &'static [&'static str],
}

/// Each board scene's board: upstream's fixture rows, one task in every
/// state, with what its screen needs added.
fn on_board(state: &str) -> OnBoard {
    let (board, keys): (Board, &[&str]) = match state {
        // Every group, the working task selected.
        "projects-board" => (fixture::board(), &[]),
        // The groups below the fold: to do and done, the done task selected.
        "projects-board-end" => (selected(fixture::board(), "proj/15"), &[]),
        // The task that asks, selected: its question on the line under it,
        // under one task pinned to the top.
        "projects-board-asks" => (selected(pinned("proj/2"), "proj/9"), &[]),
        // space talks to the working task's agent: its newest message boxed.
        "projects-board-chat" => (talking(), &[" "]),
        // alt+p: the task panel beside the list, on the task ready to approve.
        "projects-board-panel" => (selected(fixture::board(), "proj/11"), &["alt+p"]),
        // alt+t: the whole task whose review failed, its findings and history.
        "projects-board-task" => (selected(fixture::board(), "proj/12"), &["alt+t"]),
        "projects-board-plan" => (with_rows(planned(), "proj/21"), &["alt+p"]),
        "projects-board-attempts" => (with_rows(planned(), "proj/22"), &["alt+p"]),
        // ctrl+e: the task's settings page, one value its own.
        "projects-board-pickers" => (with_rows(vec![configured()], "proj/40"), &[]),
        "projects-board-settings" => (with_rows(vec![configured()], "proj/40"), &["ctrl+e"]),
        // enter on an automation's row: the automation whole, its firings.
        "projects-board-automation" => (automated(), &["enter"]),
        "projects-board-menu" => (fixture::board(), &["?"]),
        // ctrl+f, typed: the list filtered live.
        "projects-board-find" => (
            fixture::board(),
            &["ctrl+f", "c", "o", "n", "f", "l", "i", "c", "t"],
        ),
        // alt+a: the archive, read only.
        "projects-board-archive" => (fixture::board(), &["alt+a"]),
        _ => unreachable!("`STATES` lists every board scene"),
    };
    OnBoard { board, keys }
}

/// `board` with `id` selected.
fn selected(mut board: Board, id: &str) -> Board {
    board.state.sel = id.into();
    relist(&mut board.state);
    board
}

/// The fixture board with `id` pinned to the top.
fn pinned(id: &str) -> Board {
    let rows = fixture::rows(fixture::NOW)
        .into_iter()
        .map(|(_, mut row)| {
            if row.s("id") == id {
                row.put("pinned", true);
            }
            row
        })
        .collect();
    fixture::board_of(rows)
}

/// The fixture board with `rows` added, `id` selected.
fn with_rows(rows: Vec<Obj>, id: &str) -> Board {
    let mut all: Vec<Obj> = fixture::rows(fixture::NOW)
        .into_iter()
        .map(|(_, row)| row)
        .collect();
    all.extend(rows);
    selected(fixture::board_of(all), id)
}

/// A task of the fixture's repo: `extra` over its plain fields.
fn task(n: u32, extra: Value) -> Obj {
    let at = fixture::NOW - 60_000;
    let mut task = obj(json!({
        "id": format!("proj/{n}"), "repo": "proj", "owner": "me", "agent": "claude", "app": "claude",
        "created": at - 60 * 60_000, "at": at, "reviews": [], "reviewRound": 0, "description": "",
    }));
    task.extend(obj(extra));
    task
}

/// The working task's agent has a conversation to read.
fn talking() -> Board {
    let rows = fixture::rows(fixture::NOW)
        .into_iter()
        .map(|(_, mut row)| {
            if row.s("id") == "proj/5" {
                row.put("link", json!({ "pane": "w1:p5" }));
                row.put(
                    "snap",
                    json!({ "talk": { "who": "agent", "readable": true, "list": [
                        { "who": "you", "text": "Check every caller of the resize handler too." },
                        { "who": "them", "text": "Every caller had the same mistake; all three are fixed, and the resize test now covers each of them." },
                    ] } }),
                );
            }
            row
        })
        .collect();
    fixture::board_of(rows)
}

/// A plan that waits for your yes, and two attempts to pick between.
fn planned() -> Vec<Obj> {
    let now = fixture::NOW;
    let tried = |letter: &str| json!({ "letter": letter, "stage": "tried", "agent": "claude", "created": now - 10 * 60_000, "triedAt": now - 2 * 60_000 });
    vec![
        task(
            21,
            json!({ "state": "blocked", "kind": "permission", "stage": "plan", "title": "Plan the cache",
            "plan": { "writer": { "kind": "claude" }, "at": now - 5 * 60_000, "link": "http://127.0.0.1:4/p/21", "file": "plan.md" },
            "snap": { "plan": "Keep the last 50 boards read, keyed by repo.\nDrop an entry when its repo's head moves.\nRead through on a miss.", "problems": [],
                "settings": [{ "name": "worker.agent", "value": "claude", "place": "task" }] } }),
        ),
        task(
            22,
            json!({ "state": "blocked", "kind": "permission", "stage": "pick", "title": "Pick a parser",
            "tries": [tried("A"), tried("B")], "attempts": [{ "letter": "A", "agent": "claude" }, { "letter": "B", "agent": "codex" }],
            "comparison": { "pick": "B", "why": "Smaller change, and it keeps the old error messages." } }),
        ),
    ]
}

/// A task with a setting of its own beside one it inherits.
fn configured() -> Obj {
    task(
        40,
        json!({ "state": "idle", "stage": "todo", "agent": null, "app": "", "title": "Cut long titles",
        "snap": { "problems": [], "settings": [
            { "name": "worker.agent", "kind": "work", "value": "claude/opus", "own": true },
            { "name": "reviewer.agent", "kind": "work", "value": "codex", "own": false },
            { "name": "review.rounds", "kind": "work", "value": 3, "own": false },
        ] } }),
    )
}

/// The fixture board with one automation, its row selected, fired once by
/// hand into the working task.
fn automated() -> Board {
    let now = fixture::NOW;
    let mut auto = obj(json!({
        "id": "a1", "name": "Nightly deps", "on": true, "trigger": { "kind": "manual" }, "repo": "proj", "root": "/work/proj",
        "prompt": "Update the dependencies and say what changed.",
    }));
    let fired = vec![json!({ "at": now - 5 * 60_000, "task": "proj/5", "what": "by hand" })];
    auto.put(
        "shown",
        json!({ "firings": fired, "own": [], "settings": [] }),
    );
    auto.put(
        "rowNow",
        serde_json::to_value(sw_board::automations::show::row_of(
            &auto,
            now,
            Some(&fired),
        ))
        .expect("an automation's row is JSON"),
    );
    let mut board = fixture::board();
    board.state.data.autos = vec![auto];
    selected(board, "auto:a1")
}

/// A key by its chord ("alt+t", "ctrl+f"), or one character typed.
fn press_of(chord: &str) -> Press {
    match chord.chars().count() {
        1 => Press::char(chord.chars().next().expect("one character")),
        _ => Press::chord(chord),
    }
}

/// One project's coordinator and three workers fixture Threads, the
/// board's Coordinator and three tasks' agents in the nav, the board's
/// tally in the bottom bar. With `board`, the work board (⌘⇧A) over them
/// in the board's place, its keys pressed.
fn projects(name: &str, board: Option<OnBoard>) -> (Scene, Setup) {
    let chat = name == "projects-coordinator";
    let mut scene = Scene::new(name);
    let checkout = scene.project("checkout");

    let (head, head_feed) = scene.open(Provider::Claude, &checkout, "Checkout revamp");
    scene
        .core
        .send(head, "Coordinate the checkout revamp.".into());
    head_feed
        .boot(Provider::Claude, 52_000)
        .text(
            "Three threads are out. Webhook retries is writing its tests; the cart \
             copy is blocked on your call on tone; order emails has a PR up for review.",
        )
        .end(0.31);
    let workers = [
        ("Stripe webhook retries", "working"),
        ("Cart copy pass", "waiting-on-you"),
        ("Order confirmation emails", "ready-for-review"),
    ];
    // Each worker's Thread says what its task does: one working, one
    // asking, one finished with its PR up.
    let mut threads = Vec::new();
    let mut finished = None;
    for (title, group) in workers {
        let (id, feed) = scene.open(Provider::Claude, &checkout, title);
        scene.core.send(id, format!("{title}."));
        feed.boot(Provider::Claude, 20_000);
        match group {
            "working" => {
                feed.text("Writing the retry tests.");
            }
            "waiting-on-you" => {
                feed.text("Checking the current copy first.\n\n")
                    .approval("copy", "rg -n 'Your cart is empty' web/");
            }
            _ => finished = Some(feed),
        }
        threads.push(id);
    }
    scene.core.focus_thread(head);
    scene.core.pump();
    if let Some(feed) = finished {
        feed.text("PR #48 is up for review.").end(0.14);
    }

    let mut sidebar = board_sidebar(head, &threads);
    sidebar.owned.extend(sidebar.agents.keys().copied());
    let _ = scene.open(Provider::Claude, &checkout, "Checkout notes");
    scene.core.focus_thread(head);
    let setup: Setup = Box::new(move |view, window, cx| {
        view.projects.sidebar = sidebar;
        view.projects.sidebar.coordinator = Some(head);
        view.projects.live = true;
        // The cart copy's request stood before the window opened: heard,
        // not toasted over the board.
        view.ring_alerts(cx);
        view.needs_you.clear_toasts();
        if let Some(OnBoard { board, keys }) = board {
            sw_board::clock::set(Some(fixture::NOW));
            view.stage_board(board, Placement::Tab, window, cx);
            let slot = view.projects.board.as_ref().expect("the board is staged");
            slot.view.update(cx, |board, _| {
                let state = board.state().expect("a still board has its state");
                for key in keys {
                    update(state, Event::Key(press_of(key)));
                }
            });
        }
        if chat {
            view.enter_coordinator(head, cx);
        }
        cx.notify();
    });
    (scene, setup)
}

/// The board as the nav shows it: `head` its Coordinator, at work, and the
/// workers three tasks' agents — one working, one asking, one ready to
/// approve with its PR — read under the scene's own HOME.
fn board_sidebar(head: ThreadId, workers: &[ThreadId]) -> Sidebar {
    let now = sw_board::clock::now();
    let at = now - 12 * 60_000;
    let passed =
        json!({ "round": 1, "gate": "major", "meets": true, "pass": true, "findings": [] });
    let task = |n: usize, title: &str, more: Value| {
        let mut task = obj(json!({
            "id": format!("checkout/{}", 152 + n),
            "title": title,
            "agent": "claude",
            "link": { "pane": format!("p{n}"), "socket": "here" },
            "live": { "status": "working" },
            "at": at,
            "since": at,
        }));
        task.extend(obj(more));
        task
    };
    let rows = [
        task(
            0,
            "Stripe webhook retries",
            json!({ "stage": "working", "state": "working", "progress": 60, "eta": 4 }),
        ),
        task(
            1,
            "Cart copy pass",
            json!({ "stage": "asks", "state": "blocked", "kind": "question" }),
        ),
        task(
            2,
            "Order confirmation emails",
            json!({ "stage": "approval", "state": "blocked", "kind": "permission", "reviewRound": 1, "review": passed, "reviews": [passed], "pr": "https://github.com/acme/checkout/pull/48" }),
        ),
    ];
    // The adapter's panes, each placed for its task's agent.
    let threads = (0..rows.len())
        .map(|n| (format!("p{n}"), workers[n]))
        .collect();
    let theme = crate::board::render::ferrite_theme(false);
    let mut sidebar = sidebar::of(&rows, &threads, "here", now, &theme);
    let coordinator = coordinator_entry(
        &coordinator_stands("working", ""),
        0,
        "Three threads are out. Webhook retries is writing its tests.",
    );
    sidebar
        .agents
        .insert(head, sidebar::coordinator_told(coordinator, &theme));
    sidebar
}

// -------------------------------------------------------------------- review

#[derive(Clone, Copy)]
enum Staged {
    /// Uncommitted, the new `summary` lines picked and a comment under the
    /// line after them.
    Uncommitted,
    /// The commits scope's list.
    Commits,
    /// The PR tab.
    Pr,
    /// File search over the diff.
    Search,
    /// A comment being written under a one-line pick.
    Comment,
    /// The files the model declines (binary, too large) beside a renamed
    /// file deep in the tree whose lines run past a thousand.
    Notices,
    /// A path that is no repository.
    Error,
}

/// The conversation scene's Thread with its review beside it, seeded the
/// way its readers would answer: no git read and no forge call.
fn review(staged: Staged) -> (Scene, Setup) {
    let scene = conversation("review");
    let setup: Setup = Box::new(move |view, window, cx| {
        let index = view.focused();
        let composer = view.panes[index].composer.clone();
        let reading = view.prefs.settings.reading_size;
        let review =
            cx.new(|cx| ReviewView::new(composer.clone(), ReviewScope::Uncommitted, reading, cx));
        view.empty_shown = false;
        view.panes[index].review.show(review.clone(), "main".into());
        window.focus(&review.focus_handle(cx), cx);
        review.update(cx, |review, cx| {
            review.show(ReviewScope::Uncommitted, changed_files(), cx)
        });
        match staged {
            Staged::Uncommitted => {
                composer.update(cx, |composer, cx| {
                    composer.add_review_comment(
                        Comment {
                            file: "crates/ferrite/src/nav.rs".into(),
                            side: Side::New,
                            start: 8,
                            end: 8,
                            lines: "+    row = row.child(status_line(summary).h(px(theme::ROW_LINE_H)));"
                                .into(),
                            text: "Parked rows have no facts: keep the reserved line to open Threads."
                                .into(),
                            diff_anchored: true,
                            rev: Rev::Worktree,
                        },
                        cx,
                    )
                });
                // Rows 6–9: the four lines that build `summary` (new 4–7).
                review.update(cx, |review, cx| review.stage_pick(6, 9, cx));
            }
            Staged::Commits => review.update(cx, |review, cx| {
                review.pick_scope(ReviewScope::Commits, cx);
                review.show_commits(commits(), cx);
            }),
            Staged::Pr => review.update(cx, |review, cx| review.stage_pr(pull_request(), cx)),
            Staged::Search => review.update(cx, |review, cx| {
                review.stage_search("status_line", search_results(), window, cx)
            }),
            Staged::Comment => review.update(cx, |review, cx| {
                review.stage_pick(8, 8, cx);
                review.stage_comment("Keep this to open Threads?", window, cx);
            }),
            Staged::Notices => review.update(cx, |review, cx| {
                review.show(ReviewScope::Uncommitted, notice_files(), cx)
            }),
            Staged::Error => review.update(cx, |review, cx| {
                review.fail("not a git repository".into(), cx)
            }),
        }
        cx.notify();
    });
    (scene, setup)
}

/// The fix the conversation made, as the uncommitted scope reads it.
fn changed_files() -> Vec<FileSides> {
    let nav_old = "\
fn thread_row(&self, thread: ThreadId, cx: &App) -> Div {
    let facts = state.facts(thread);
    let mut row = div().h(px(theme::ROW_H));
    if let Some(summary) = facts.summary.clone() {
        row = row.child(status_line(summary));
    }
    row
}
";
    let nav_new = "\
fn thread_row(&self, thread: ThreadId, cx: &App) -> Div {
    let facts = state.facts(thread);
    let mut row = div().h(px(theme::ROW_H));
    let summary = facts
        .summary
        .clone()
        .unwrap_or_else(|| \"waiting for first output\".into());
    row = row.child(status_line(summary).h(px(theme::ROW_LINE_H)));
    row
}
";
    let theme_old = "pub const ROW_H: f32 = 20.0;\n";
    let theme_new = "pub const ROW_H: f32 = 20.0;\npub const ROW_LINE_H: f32 = 18.0;\n";
    vec![
        sides("crates/ferrite/src/nav.rs", nav_old, nav_new, 5, 3),
        sides("crates/ferrite/src/theme.rs", theme_old, theme_new, 1, 0),
    ]
}

/// A binary and an over-budget file, and a renamed file deep in the tree
/// whose change sits past line a thousand.
fn notice_files() -> Vec<FileSides> {
    let declined = |path: &str, notice| FileSides {
        file: ChangedFile {
            path: path.into(),
            kind: ChangeKind::Modified,
            additions: 0,
            deletions: 0,
            previous_path: None,
            binary: false,
            old_size: 0,
            new_size: None,
        },
        sides: Err(notice),
    };
    let old: String = (1..=1204).map(|n| format!("    step({n});\n")).collect();
    let new = old.replace("    step(1200);\n", "    step(1200).await;\n");
    let mut long = sides(
        "crates/ferrite/src/cockpit/visual_reference/feature_scenes/review_states.rs",
        &old,
        &new,
        1,
        1,
    );
    long.file.previous_path =
        Some("crates/ferrite/src/cockpit/visual_reference/review_scenes.rs".into());
    vec![
        declined("assets/icons/app.icns", Notice::Binary),
        declined("fixtures/transcript-large.jsonl", Notice::TooLarge),
        long,
    ]
}

fn sides(path: &str, old: &str, new: &str, additions: u32, deletions: u32) -> FileSides {
    FileSides {
        file: ChangedFile {
            path: path.into(),
            kind: ChangeKind::Modified,
            additions,
            deletions,
            previous_path: None,
            binary: false,
            old_size: old.len() as u64,
            new_size: Some(new.len() as u64),
        },
        sides: Ok((old.into(), new.into())),
    }
}

fn commits() -> Vec<CommitRow> {
    [
        (
            "9f3c2e1",
            "Nav: reserve the status line for open Threads",
            "Joseph",
        ),
        (
            "4b7a910",
            "Nav: rows keep their height while streaming",
            "Joseph",
        ),
        (
            "e21d4c8",
            "Theme: ROW_LINE_H for the nav's sub-line",
            "Claude",
        ),
        (
            "77c09ab",
            "Facts: refresh summaries on the turn's end",
            "Claude",
        ),
        ("0d5e3f2", "Release version 0.4.0", "Joseph"),
    ]
    .into_iter()
    .map(|(sha, subject, author)| CommitRow {
        sha: format!("{sha}{}", "0".repeat(33)),
        subject: subject.into(),
        time: 0,
        author: author.into(),
        refs: Vec::new(),
        merge: false,
    })
    .collect()
}

fn pull_request() -> ReviewPullRequest {
    let check = |name: &str, status| WireCheck {
        name: name.into(),
        status,
    };
    ReviewPullRequest {
        noun: "PR".into(),
        pr: Ok(WirePr {
            number: 118,
            title: "Nav rows keep their height while a Thread streams".into(),
            url: "https://github.com/example/ferrite/pull/118".into(),
            body: "Live rows reserve their status line from the first frame, so a \
                   Thread that starts streaming no longer shifts the tree."
                .into(),
            state: WirePrState::Open,
            draft: false,
            head: "fix/nav-jitter".into(),
            base: "main".into(),
            merge: WireMerge::Clean,
            sync: WireSync::Unpushed { commits: 1 },
            checks: vec![
                check("test (macos-latest)", WireCheckStatus::Success),
                check("test (windows-latest)", WireCheckStatus::Running),
                check("clippy", WireCheckStatus::Success),
                check("fmt", WireCheckStatus::Failure),
            ],
            comments: vec![
                WirePrComment {
                    kind: WirePrCommentKind::Review,
                    author: "maria".into(),
                    anchor: "review".into(),
                    body: "Looks right. One question on parked rows.".into(),
                    snippet: None,
                    created_at: "2026-10-15T09:12:00Z".into(),
                    resolved: false,
                    outdated: false,
                    replies: Vec::new(),
                },
                WirePrComment {
                    kind: WirePrCommentKind::Finding,
                    author: "maria".into(),
                    anchor: "crates/ferrite/src/nav.rs:439-442".into(),
                    body: "Does a parked row still take the reserved line?".into(),
                    snippet: Some("+    let summary = facts\n+        .summary".into()),
                    created_at: "2026-10-15T09:14:00Z".into(),
                    resolved: false,
                    outdated: false,
                    replies: vec![WireReply {
                        author: "joseph".into(),
                        body: "No: the next commit keeps it to open Threads.".into(),
                        created_at: "2026-10-15T09:20:00Z".into(),
                    }],
                },
            ],
            truncated: false,
            checks_truncated: false,
            comments_truncated: false,
        }),
        unread: None,
    }
}

fn search_results() -> SearchResults {
    let span = |text: &str| {
        let at = text.find("status_line").expect("the query is in the hit") as u32;
        vec![(at, at + "status_line".len() as u32)]
    };
    let code = |path: &str, line: u64, text: &str| CodeHit {
        path: path.into(),
        line,
        text: text.into(),
        spans: span(text),
    };
    SearchResults {
        files: vec![FileHit {
            path: "crates/ferrite/src/status_line.rs".into(),
            spans: vec![(19, 30)],
        }],
        code: vec![
            code(
                "crates/ferrite/src/nav.rs",
                388,
                "fn status_line(text: SharedString) -> Div {",
            ),
            code(
                "crates/ferrite/src/nav.rs",
                443,
                "row = row.child(status_line(summary).h(px(theme::ROW_LINE_H)));",
            ),
            code(
                "crates/ferrite/src/facts.rs",
                233,
                "fn status_line_text(transcript: &Transcript) -> Option<String> {",
            ),
        ],
        file_total: 1,
        code_more: false,
    }
}

// -------------------------------------------------------------------- reader

/// The conversation scene's Thread with a file open in the Panel's
/// Preview tab at a line: at rest, or held `held` of the way through the
/// Panel's ride out to its preview width (the board giving way, the
/// preview laid out at its final width and pinned to the board's edge).
fn reader(held: Option<f32>) -> (Scene, Setup) {
    let scene = conversation("reader");
    let file = scene
        .root
        .join("work")
        .join("ferrite")
        .join("crates/ferrite/src/nav.rs");
    let setup: Setup = Box::new(move |view, window, cx| {
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, NAV_RS).unwrap();
        view.empty_shown = false;
        view.fixture_hold_panel(held);
        view.open_preview(file.clone(), Some(14), window, cx);
    });
    (scene, setup)
}

/// What the reader scene's file reads: the nav row the conversation is
/// about.
const NAV_RS: &str = "\
use crate::facts::Facts;
use ferrite_ui::theme::{ROW_H, ROW_LINE_H};

/// One Thread's row in the nav: its mark, its title and, while the Thread
/// is open, the status line under it.
pub(crate) struct ThreadRow {
    pub title: SharedString,
    pub status: Option<SharedString>,
}

impl NavState {
    fn thread_row(&self, thread: ThreadId) -> Div {
        let facts = self.facts.get(thread);
        let mut row = div().h(px(ROW_H));
        let summary = facts
            .summary
            .clone()
            .unwrap_or_else(|| \"waiting for first output\".into());
        row = row.child(status_line(summary));
        row
    }

    /// A parked Thread keeps its compact height: it has no live facts.
    fn parked_row(&self, thread: ThreadId) -> Div {
        div().h(px(ROW_H)).child(self.title(thread))
    }
}
";

// ---------------------------------------------------------------------- find

/// ⌘F on the conversation's transcript, `nav` typed: the bar's count and
/// every occurrence marked, the active one brought into view.
fn find() -> (Scene, Setup) {
    let scene = conversation("find");
    let setup: Setup = Box::new(|view, window, cx| {
        view.open_find(&ferrite_pane::find::Open, window, cx);
        if let Some(transcript) = view.panes[view.focused()].transcript() {
            transcript.update(cx, |transcript, cx| transcript.type_find("nav", cx));
        }
    });
    (scene, setup)
}

// ---------------------------------------------------------------- nav states

/// One Project's Threads in each of the four states (ADR 0015) — working,
/// needs you, done ✓ and idle — each wearing a PR label in a different
/// state: `#12 ✓` passing, `✗` failing, `●` pending, `◆` merged, `⊘`
/// closed and `⟳` being checked.
fn nav_states() -> (Scene, Setup) {
    let mut scene = Scene::new("nav-states");
    let shop = scene.project("shop");

    let (working, feed) = scene.open(Provider::Claude, &shop, "Stripe webhook retries");
    scene.core.send(working, "Retry failed webhooks.".into());
    feed.boot(Provider::Claude, 48_000)
        .text("Writing the backoff; the idempotency key already lands in orders.");

    let (blocked, feed) = scene.open(Provider::Codex, &shop, "Cart copy pass");
    scene
        .core
        .send(blocked, "Rewrite the cart's empty state.".into());
    feed.boot(Provider::Codex, 12_000)
        .text("Two tones read well; checking the current copy first.\n\n")
        .approval("grep", "rg -n 'Your cart is empty' web/");

    let (done, done_feed) = scene.open(Provider::Claude, &shop, "Order confirmation emails");
    scene
        .core
        .send(done, "Send confirmations from the new template.".into());
    done_feed
        .boot(Provider::Claude, 30_000)
        .text("Done: the template renders and the PR is up.");

    let (checking, feed) = scene.open(Provider::Claude, &shop, "Tax rounding");
    scene
        .core
        .send(checking, "Round tax per line, not per order.".into());
    feed.boot(Provider::Claude, 22_000)
        .text("Rounding per line now; rerunning the totals suite.");

    let (merged, _) = scene.open(Provider::Codex, &shop, "Refund flow");
    let (closed, _) = scene.open(Provider::Claude, &shop, "Legacy coupons");

    // The finish lands while another Thread holds the board: unseen, done.
    scene.core.focus_thread(working);
    scene.core.pump();
    done_feed.end(0.12);

    let setup: Setup = Box::new(move |view, _, cx| {
        let pr = |number, state, checks| {
            Some(PullRequest {
                number,
                state,
                draft: false,
                url: None,
                checks,
                runs: Vec::new(),
            })
        };
        let facts = &mut view.facts;
        facts.pr_answered(working, pr(12, PrState::Open, Some(CheckState::Passing)));
        facts.pr_answered(blocked, pr(31, PrState::Open, Some(CheckState::Failing)));
        facts.pr_answered(done, pr(48, PrState::Open, Some(CheckState::Pending)));
        facts.pr_answered(merged, pr(40, PrState::Merged, None));
        facts.pr_answered(closed, pr(22, PrState::Closed, None));
        facts.pr_answered(checking, pr(53, PrState::Open, Some(CheckState::Passing)));
        facts.ask_pr(checking, cx.background_executor().now(), true);
        cx.notify();
    });
    (scene, setup)
}

// ------------------------------------------------------------------- mermaid

/// A mermaid fence in an answer (ADR 0016). Chromium draws it: with
/// `--features cef` (run from the bundle) the diagram, without it the
/// fence as code — what a build with no engine shows.
fn mermaid() -> (Scene, Setup) {
    let mut scene = Scene::new("mermaid");
    let ferrite = scene.project("ferrite");
    let (thread, feed) = scene.open(Provider::Claude, &ferrite, "Webhook retry flow");
    scene
        .core
        .send(thread, "Draw how a failed webhook is retried.".into());
    feed.boot(Provider::Claude, 26_000)
        .text(
            "A failed delivery is retried with backoff until it lands or the window closes:\n\n\
             ```mermaid\n\
             flowchart LR\n  \
               A[Stripe event] --> B{Handler ok?}\n  \
               B -- yes --> C[Mark processed]\n  \
               B -- no --> D[Backoff 2^n s]\n  \
               D --> E{Under 3 days?}\n  \
               E -- yes --> B\n  \
               E -- no --> F[Dead letter + alert]\n\
             ```\n\n\
             The idempotency key makes every retry safe to repeat.",
        )
        .end(0.08);
    (scene, Box::new(|_, _, _| {}))
}

// --------------------------------------------------------------------- toast

/// A Thread off the board finishes while the operator reads another: its
/// done toast, bottom right over the board.
fn toast_done() -> (Scene, Setup) {
    let mut scene = conversation("toast-done");
    let ferrite = scene.project("ferrite");
    let focused = scene.core.roster().focused_thread().expect("focus");
    let (notes, feed) = scene.open(Provider::Claude, &ferrite, "Release notes 0.9");
    scene
        .core
        .send(notes, "Draft the 0.9 release notes.".into());
    feed.boot(Provider::Claude, 31_000)
        .text("Drafting from the merged PRs.");
    scene.core.pump();
    scene.core.focus_thread(focused);
    scene.core.pump();
    feed.text(" Drafted `CHANGELOG.md` for 0.9: 14 changes under four headings.")
        .ev(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.09),
        });
    (scene, Box::new(|_, _, _| {}))
}
