//! Scenes for the features the first references predate: the work board
//! (ADR 0031) and the nav's project blocks (ADR 0030), the Review slot (ADR
//! 0021), find in a transcript (ADR 0020), the nav's PR labels and the four
//! states (ADR 0015), a mermaid diagram (ADR 0016) and a finish toast (ADR
//! 0017). Every one is seeded: no network, no provider CLI, no `gh`, and
//! nothing of the operator's (the projects root is under the scene's own).

use std::fs::File;

use ferrite_core::protocol::{
    ReviewPullRequest, WireCheck, WireCheckStatus, WireMerge, WirePr, WirePrComment,
    WirePrCommentKind, WirePrState, WireReply, WireSync,
};
use ferrite_core::store::Provider;
use ferrite_core::workspace::{CheckState, PrState, PullRequest};
use ferrite_core::{SessionEvent, TurnOutcome};
use ferrite_projects::{coordinator, project, thread};
use ferrite_reviewr::ferrite::FileSides;
use ferrite_reviewr::git::CommitRow;
use ferrite_reviewr::model::{ChangeKind, ChangedFile, Comment, Rev, Scope as ReviewScope, Side};
use ferrite_reviewr::search::{CodeHit, FileHit, SearchResults};
use gpui::{AppContext, Focusable};

use super::{conversation, Scene, Setup};
use crate::nav::NavFold;
use crate::projects::{Model, Place};
use crate::review::ReviewView;

/// Every state this module draws, at the app size.
pub(super) const STATES: &[&str] = &[
    "projects-nav",
    "projects-board",
    "review-uncommitted",
    "review-commits",
    "review-pr",
    "review-search",
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
        "projects-nav" => projects(false),
        "projects-board" => projects(true),
        "review-uncommitted" => review(Staged::Uncommitted),
        "review-commits" => review(Staged::Commits),
        "review-pr" => review(Staged::Pr),
        "review-search" => review(Staged::Search),
        "find" => find(),
        "nav-states" => nav_states(),
        "mermaid" => mermaid(),
        "toast-done" => toast_done(),
        _ => unreachable!("`handles` lists every state"),
    }
}

// ------------------------------------------------------------------ projects

/// One project under the scene's own projects root, its coordinator and
/// three workers fixture Threads, the ticker's lock held so the records
/// read as live: the nav and the bottom bar. With `board`, the work board
/// (⌘⇧A) over them in the board's place, on upstream's fixture rows.
fn projects(board: bool) -> (Scene, Setup) {
    let mut scene = Scene::new(&format!("projects-{}", if board { "board" } else { "nav" }));
    let checkout = scene.project("checkout");
    let place = Place {
        root: scene.root.join("projects"),
        config_dir: scene.root.join("projects-config"),
    };
    std::fs::create_dir_all(&place.root).expect("create the projects root");
    std::fs::create_dir_all(&place.config_dir).expect("create the projects config");

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
        (
            "Stripe webhook retries",
            "working",
            "working · ~60%",
            "Writing the retry tests",
        ),
        (
            "Cart copy pass",
            "waiting-on-you",
            "needs you · blocked",
            "",
        ),
        (
            "Order confirmation emails",
            "ready-for-review",
            "ready for review · PR #48",
            "",
        ),
    ];
    // Each worker's Thread says what its record does: one working, one
    // asking, one finished with its PR up.
    let mut threads = Vec::new();
    let mut finished = None;
    for (title, group, ..) in workers {
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

    let alpha = project::create(
        &place.root,
        "Checkout revamp",
        "Ship the new checkout before the sale",
        Vec::new(),
    )
    .expect("create the fixture project");
    alpha
        .update_coordinator(|record| record.pane_id = head.to_string())
        .expect("record the coordinator");
    coordinator::save_live(
        &alpha,
        &[coordinator::LivePane {
            pane_id: head.to_string(),
            agent_status: "working".into(),
            ..Default::default()
        }],
    )
    .expect("record the coordinator's pane");
    for ((title, group, state, activity), id) in workers.into_iter().zip(&threads) {
        thread::allocate(&alpha, |record| {
            record.title = title.into();
            record.status = thread::Status::Open;
            record.last_group = group.into();
            record.pane_id = id.to_string();
            record.state_line = state.into();
            record.activity = activity.into();
        })
        .expect("record a fixture thread");
    }
    // The ticker runs while its lock is held: the records are live, and
    // the bottom bar counts what needs the operator.
    let lock = File::create(place.root.join(".ticker.lock")).expect("create the ticker lock");
    lock.try_lock().expect("hold the ticker lock");
    scene.hold.push(Box::new(lock));

    let setup: Setup = Box::new(move |view, window, cx| {
        view.set_nav_fold(NavFold::Parked, false, cx);
        view.projects.model = Model::load(&place);
        view.projects.place = Some(place);
        // The cart copy's request stood before the window opened: heard,
        // not toasted over the board.
        view.ring_bell(cx);
        view.bell.clear_toasts();
        if board {
            sw_board::clock::set(Some(crate::board::fixture::NOW));
            view.stage_board(crate::board::fixture::board(), window, cx);
        }
        cx.notify();
    });
    (scene, setup)
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
}

/// The conversation scene's Thread with its review beside it, seeded the
/// way its readers would answer: no git read and no forge call.
fn review(staged: Staged) -> (Scene, Setup) {
    let scene = conversation("review");
    let workspace = scene.root.join("work").join("ferrite");
    let setup: Setup = Box::new(move |view, window, cx| {
        view.set_nav_fold(NavFold::Parked, false, cx);
        let index = view.focused();
        let composer = view.panes[index].composer.clone();
        let reading = view.prefs.settings.reading_size;
        let review =
            cx.new(|cx| ReviewView::new(composer.clone(), ReviewScope::Uncommitted, reading, cx));
        view.empty_shown = false;
        view.panes[index]
            .preview
            .open_review(workspace, "main".into(), review.clone());
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
        }),
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

// ---------------------------------------------------------------------- find

/// ⌘F on the conversation's transcript, `nav` typed: the bar's count and
/// every occurrence marked, the active one brought into view.
fn find() -> (Scene, Setup) {
    let scene = conversation("find");
    let setup: Setup = Box::new(|view, window, cx| {
        view.set_nav_fold(NavFold::Parked, false, cx);
        view.open_find(&crate::find::Open, window, cx);
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
        view.set_nav_fold(NavFold::Parked, false, cx);
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
