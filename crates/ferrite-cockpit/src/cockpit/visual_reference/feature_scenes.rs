//! Scenes for the features the first references predate: the Review slot (ADR
//! 0021) at rest and mid-slide, a file in the Panel's preview, find in a
//! transcript (ADR 0020), the nav's PR labels and the four
//! states (ADR 0015), a mermaid diagram (ADR 0016), a finish toast (ADR
//! 0017), a turn's file and page cards, a page beside the transcript and
//! the lightbox. Every one is seeded: no network, no provider CLI, no `gh`, and
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
use crate::cockpit::panel::Opener;
use crate::cockpit::surfaces::{SurfaceId, ThreadSurfaces};
use crate::review::ReviewView;

/// Every state this module draws, at the app size.
pub(super) const STATES: &[&str] = &[
    "review-uncommitted",
    "review-commits",
    "review-pr",
    "review-search",
    "reader-line",
    "review-slide",
    "review-comment",
    "review-notices",
    "review-error",
    "find",
    "reply-card",
    "side-chat",
    "nav-states",
    "mermaid",
    "toast-done",
    "wrap-up",
    "wrap-up-spent",
    "output-cards",
    "page-pane",
    "lightbox",
    "files-code",
    "files-empty",
    "changes-split",
    "terminal",
    "panel-tabs",
    "panel-plusmenu",
    "panel-empty",
    "panel-maximized",
    "panel-pr",
    "scroll-to-end",
    "send-landed",
];

pub(super) fn handles(state: &str) -> bool {
    STATES.contains(&state)
}

pub(super) fn build(state: &str) -> (Scene, Setup) {
    match state {
        "review-uncommitted" => review(Staged::Uncommitted),
        "review-commits" => review(Staged::Commits),
        "review-pr" => review(Staged::Pr),
        "review-search" => review(Staged::Search),
        "reader-line" => reader(),
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
        "reply-card" => reply_card(),
        "side-chat" => side_chat(),
        "nav-states" => nav_states(),
        "mermaid" => mermaid(),
        "toast-done" => toast_done(),
        "wrap-up" => wrap_up(false),
        "wrap-up-spent" => wrap_up(true),
        "output-cards" => {
            let (scene, _) = output_cards("output-cards");
            (scene, Box::new(|_, _, _| {}))
        }
        "page-pane" => page_pane(),
        "lightbox" => lightbox(),
        "files-code" => tools("files-code", &[Opener::Files], Some("crates/ferrite-ui/src/motion.rs")),
        "files-empty" => tools("files-empty", &[Opener::Files], None),
        "changes-split" => tools("changes-split", &[Opener::Files, Opener::Changes], Some("crates/ferrite-ui/src/motion.rs")),
        "terminal" => tools("terminal", &[Opener::Terminal], None),
        "panel-tabs" => panel("panel-tabs", PanelStaged::Tabs),
        "panel-plusmenu" => panel("panel-plusmenu", PanelStaged::PlusMenu),
        "panel-empty" => panel("panel-empty", PanelStaged::Empty),
        "panel-maximized" => panel("panel-maximized", PanelStaged::Maximized),
        "panel-pr" => panel_pr(),
        "scroll-to-end" => scroll_to_end(),
        "send-landed" => send_landed(),
        _ => unreachable!("`handles` lists every state"),
    }
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
                            replies: Vec::new(),
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
/// Preview tab at a line.
fn reader() -> (Scene, Setup) {
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

// ------------------------------------------------------------ send scroll

/// The conversation scrolled back to its top while a third prompt runs and
/// a fourth waits: Scroll to end over the transcript's foot, `· 1 queued`
/// (the capture's frames land its show delay).
fn scroll_to_end() -> (Scene, Setup) {
    let mut scene = conversation("scroll-to-end");
    let thread = scene.core.roster().focused_thread().expect("focus");
    scene
        .core
        .send(thread, "Now check the parked rows by hand.".into());
    scene.core.pump();
    scene.core.queue(thread, "Then push the branch.".into());
    scene.core.pump();
    let setup: Setup = Box::new(|view, _, cx| {
        if let Some(transcript) = view.panes[view.focused()].transcript() {
            transcript.update(cx, |transcript, cx| {
                transcript.scroll_to(ferrite_pane::transcript::ScrollTarget::Top, cx)
            });
        }
    });
    (scene, setup)
}

/// A prompt just sent on the finished conversation: it has eased up to
/// the top of the transcript, `TX_SEND_ANCHOR_OFFSET` under it, with the
/// room for its answer below (the capture's frames land the ease).
fn send_landed() -> (Scene, Setup) {
    let scene = conversation("send-landed");
    let setup: Setup = Box::new(|view, _, cx| {
        let Some(thread) = view.focused_thread() else {
            return;
        };
        view.cockpit
            .send(thread, "Push the branch with the nav fix.".into());
        view.scroll_transcript_to_new_prompt(view.focused(), cx);
    });
    (scene, setup)
}

// ---------------------------------------------------------------- reply card

/// `Reply` pressed on a passage of the answer (`claude/reply-popover.png`):
/// the card over the selection, and a sent reply waiting in the tray
/// (`claude/reply-sent-chip.png`).
fn reply_card() -> (Scene, Setup) {
    let scene = conversation("reply-card");
    let setup: Setup = Box::new(|view, _, cx| {
        let index = view.focused();
        view.panes[index].composer.update(cx, |composer, cx| {
            composer.reply(
                ferrite_pane::quote::Reply::new("Both commands ran.", "testing"),
                cx,
            );
            // An image beside it: both are tiles of the one tray row.
            composer.add_files(
                &[std::path::PathBuf::from(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/fixtures/selection-pill.png"
                ))],
                cx,
            );
        });
        if let Some(transcript) = view.panes[index].transcript() {
            let line = gpui::Bounds::new(
                gpui::point(gpui::px(560.), gpui::px(430.)),
                gpui::size(gpui::px(320.), gpui::px(20.)),
            );
            transcript.update(cx, |transcript, cx| {
                transcript.fixture_reply_card(
                    "Both commands ran. ls /definitely-missing-dir failed as expected, and sleep 15 && echo done ran in the foreground this time.",
                    vec![line],
                    cx,
                )
            });
        }
    });
    (scene, setup)
}

// ------------------------------------------------------------- title tools

/// The conversation with the Panel's `tools` open on its Thread (the
/// checkout is this repository), `file` opened in Files
/// (`claude/parity/files-code.png`, `files-empty.png`, `changes-split.png`,
/// `terminal-command.png`). A terminal runs the command Claude's capture ran.
fn tools(state: &str, tools: &'static [Opener], file: Option<&'static str>) -> (Scene, Setup) {
    let scene = conversation(state);
    let setup: Setup = Box::new(move |view, _, cx| {
        let Some(thread) = view.focused_thread() else {
            return;
        };
        for tool in tools {
            view.run_opener(thread, *tool, cx);
        }
        if let (Some(file), Ok(root)) = (file, std::env::current_dir()) {
            view.open_in_files(thread, root.join(file), None, true, cx);
        }
        if tools.contains(&Opener::Terminal) {
            view.fixture_terminal_command(thread, b"echo hello; ls | head -5\r", cx);
        }
    });
    (scene, setup)
}

// --------------------------------------------------------------------- panel

/// How a `panel-*` scene leaves the Panel.
#[derive(Clone, Copy)]
enum PanelStaged {
    /// Four tabs, Changes shown.
    Tabs,
    /// The `+` menu up, Pull request dimmed (no PR on the branch).
    PlusMenu,
    /// Every tab closed: "Open a surface".
    Empty,
    /// Changes shown, maximized.
    Maximized,
}

/// The conversation with its Panel's tabs staged (ADR 0039).
fn panel(state: &str, staged: PanelStaged) -> (Scene, Setup) {
    let scene = conversation(state);
    let setup: Setup = Box::new(move |view, _, cx| {
        let Some(thread) = view.focused_thread() else {
            return;
        };
        for opener in [Opener::Changes, Opener::Files, Opener::Terminal] {
            view.run_opener(thread, opener, cx);
        }
        view.upsert_surface(thread, SurfaceId::Changes, cx);
        match staged {
            PanelStaged::Tabs => {}
            PanelStaged::PlusMenu => view.open_surface_menu(cx),
            PanelStaged::Empty => view.close_surfaces(thread, ThreadSurfaces::close_all, cx),
            PanelStaged::Maximized => view.toggle_panel_maximized(thread, cx),
        }
    });
    (scene, setup)
}

/// The pull request Surface on Summary, the PR open and ready to merge:
/// every check green, Merge as the primary action.
fn panel_pr() -> (Scene, Setup) {
    let scene = conversation("panel-pr");
    let setup: Setup = Box::new(|view, _, cx| {
        let Some(thread) = view.focused_thread() else {
            return;
        };
        let Ok(mut pr) = pull_request().pr else {
            return;
        };
        for check in &mut pr.checks {
            check.status = WireCheckStatus::Success;
        }
        view.facts.pr_answered(
            thread,
            Some(PullRequest {
                number: pr.number,
                state: PrState::Open,
                draft: false,
                url: Some(pr.url.clone()),
                title: Some(pr.title.clone()),
                checks: Some(CheckState::Passing),
                runs: Vec::new(),
            }),
        );
        view.fixture_pr_snapshot(thread, pr);
        view.open_pr_surface(thread, cx);
    });
    (scene, setup)
}

// ----------------------------------------------------------------- side chat

/// `Start a side chat` over a selection (`claude/sidechat-open.png`): the
/// right pane's empty first tab, the passage quoted in its composer.
fn side_chat() -> (Scene, Setup) {
    let scene = conversation("side-chat");
    let setup: Setup = Box::new(|view, _, cx| {
        if let Some(owner) = view.focused_thread() {
            view.fixture_side_chat(
                owner,
                Some(
                    "y-missing-dir failed as expected, and sleep 15 && echo done ran in the foreground this time."
                        .into(),
                ),
                cx,
            );
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
                title: None,
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

/// A turn that did a little work and closed with its wrap-up box: recap, the
/// question and two steps (`claude/parity/wrapup-question.png`); `spent`,
/// the first step sent (`wrapup-used.png`).
fn wrap_up(spent: bool) -> (Scene, Setup) {
    let mut scene = Scene::new(if spent { "wrap-up-spent" } else { "wrap-up" });
    let ferrite = scene.project("ferrite");
    let (thread, feed) = scene.open(Provider::Claude, &ferrite, "Append line to notes.txt");
    scene.core.send(
        thread,
        "Append the line \"second line\" to notes.txt. Then stop and ask me whether to commit it."
            .into(),
    );
    feed.boot(Provider::Claude, 76_000)
        .tool(
            "edit",
            "Edit",
            serde_json::json!({"file_path": "notes.txt", "old_string": "hello", "new_string": "hello\nsecond line"}),
        )
        .ok("edit", "Updated notes.txt")
        .text("I added \u{201c}second line\u{201d} to the end of notes.txt, so it now has two lines. Nothing is committed yet.")
        .tool(
            "wrap",
            ferrite_core::wrap_up::CLAUDE_NAME,
            serde_json::json!({
                "recap": "Added \"second line\" to notes.txt, so the file now has two lines. The change hasn't been committed yet.",
                "ask": "Commit the change to notes.txt?",
                "next": ["Commit it", "Leave it uncommitted"],
            }),
        )
        .ev(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.02),
        });
    if spent {
        scene.core.pump();
        scene.core.send(thread, "Commit it".into());
    }
    (scene, Box::new(|_, _, _| {}))
}

// ------------------------------------------------------------------ outputs

/// A turn that wrote a note and a page and started a dev server: its cards
/// under the answer — the page's, the server's and the note's. The page
/// file is on disk, so its card reads its `<title>`.
fn output_cards(label: &str) -> (super::Scene, std::path::PathBuf) {
    let mut scene = super::Scene::new(label);
    let ferrite = scene.project("ferrite");
    let page = ferrite.join("site/report.html");
    std::fs::create_dir_all(page.parent().unwrap()).unwrap();
    std::fs::write(
        &page,
        "<html><head><title>Q3 sales report</title></head>\
         <body><h1>Q3 sales</h1><p>Revenue up 18% on Q2.</p></body></html>",
    )
    .unwrap();
    std::fs::write(ferrite.join("NOTES.md"), "# Q3 notes\n").unwrap();
    let (thread, feed) = scene.open(Provider::Claude, &ferrite, "Q3 sales report");
    scene.core.send(
        thread,
        "Build a one-page Q3 sales report and serve it.".into(),
    );
    let created = |line: &str| ferrite_core::Hunk {
        old_start: 0,
        old_lines: 0,
        new_start: 1,
        new_lines: 1,
        lines: vec![format!("+{line}")],
        section: None,
    };
    feed.boot(Provider::Claude, 22_000)
        .tool(
            "write-page",
            "Write",
            serde_json::json!({ "file_path": "site/report.html" }),
        )
        .done(
            "write-page",
            "created",
            false,
            ferrite_core::ToolResult::FileEdit {
                path: "site/report.html".into(),
                hunks: vec![created("<title>Q3 sales report</title>")],
            },
        )
        .tool(
            "write-notes",
            "Write",
            serde_json::json!({ "file_path": "NOTES.md" }),
        )
        .done(
            "write-notes",
            "created",
            false,
            ferrite_core::ToolResult::FileEdit {
                path: "NOTES.md".into(),
                hunks: vec![created("# Q3 notes")],
            },
        )
        .text(
            "The report is in `site/report.html`, and the dev server is up at \
             http://localhost:5173/. I kept the open questions in `NOTES.md`.",
        )
        .end(0.06);
    (scene, page)
}

/// The report's page card opened: the page beside the transcript, in the
/// Panel, its head naming it by its title. No Chromium in a reference
/// build, so the body is the "can't show" note.
fn page_pane() -> (Scene, Setup) {
    let (scene, page) = output_cards("page-pane");
    let setup: Setup = Box::new(move |view, window, _| {
        view.empty_shown = false;
        view.panes[0]
            .preview
            .open_page(ferrite_pane::page::PageSource::File(page.clone()), window);
    });
    (scene, setup)
}

/// A prompt's three screenshots, the second open in the lightbox: the dim,
/// the bar's name and doors, the ← → doors and `2 of 3`.
fn lightbox() -> (Scene, Setup) {
    let scene = conversation("lightbox");
    let shots = scene.root.join("shots");
    std::fs::create_dir_all(&shots).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures");
    let gallery: Vec<std::path::PathBuf> = ["approval-wide", "decision-wide", "edges-wide"]
        .iter()
        .map(|name| {
            let to = shots.join(format!("{name}.png"));
            let _ = std::fs::copy(source.join(format!("{name}.png")), &to);
            to
        })
        .collect();
    let setup: Setup = Box::new(move |view, window, cx| {
        view.panes[0]
            .preview
            .open(gallery[1].clone(), None, &gallery, window, cx);
    });
    (scene, setup)
}
