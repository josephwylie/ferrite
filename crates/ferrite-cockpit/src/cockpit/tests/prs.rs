//! The PRs destination: it opens from the footer whatever the board holds,
//! lists a Project's pull requests, reads one whole by number, and comes
//! back to its list as it was left.
use super::*;
use ferrite_core::protocol::{
    ReviewPullRequest, ReviewPullRequests, WireCheck, WireCheckStatus, WirePr, WirePrComment,
    WirePrCommentKind, WirePrFilter, WirePrList, WirePrRow, WirePrState, WirePrUnread, WireReply,
    WireReviewState,
};
use ferrite_core::workspace::registry::ProjectId;

fn shows(cx: &mut gpui::VisualTestContext, selector: &str) -> bool {
    debug_bounds(cx, selector.to_string()).is_some()
}

/// Wait for `selector` to be drawn: answers come from a reader's own
/// thread, so no fixed number of ticks is enough.
fn lands(cx: &mut gpui::VisualTestContext, selector: &str) {
    until(cx, |cx| shows(cx, selector));
}

fn click(cx: &mut gpui::VisualTestContext, selector: &str) {
    let at = debug_bounds(cx, selector.to_string()).unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
}

fn row(number: u64, title: &str, state: WirePrState) -> WirePrRow {
    WirePrRow {
        number,
        title: title.into(),
        url: format!("https://github.com/o/r/pull/{number}"),
        state,
        draft: false,
        author: "ada".into(),
        head: format!("feature/{number}"),
        base: "main".into(),
        updated_at: "2026-11-12".into(),
    }
}

fn list_of(rows: Vec<WirePrRow>, more: bool) -> ReviewPullRequests {
    ReviewPullRequests {
        list: Ok(WirePrList { rows, more }),
        unread: None,
    }
}

fn comment(kind: WirePrCommentKind, author: &str, body: &str) -> WirePrComment {
    WirePrComment {
        id: None,
        author_is_bot: false,
        place: None,
        kind,
        author: author.into(),
        anchor: "comment".into(),
        verdict: None,
        body: body.into(),
        snippet: None,
        created_at: "2026-11-11".into(),
        resolved: false,
        outdated: false,
        replies: Vec::new(),
    }
}

/// PR #7 whole: a Markdown description, a failed check, a resolved inline
/// thread with a nested reply and its code context, a plain comment, and a
/// conversation the read capped.
fn full_pr() -> WirePr {
    WirePr {
        number: 7,
        title: "Keep rows tall".into(),
        url: "https://github.com/o/r/pull/7".into(),
        body: "## What\n\nRows **keep** their height.".into(),
        author: None,
        author_avatar_url: None,
        updated_at: None,
        changed_files: None,
        additions: None,
        deletions: None,
        state: WirePrState::Open,
        draft: false,
        head: "feature/7".into(),
        base: "main".into(),
        merge: ferrite_core::protocol::WireMerge::Clean,
        sync: ferrite_core::protocol::WireSync::Unknown,
        checks: vec![WireCheck {
            name: "fmt".into(),
            status: WireCheckStatus::Failure,
            url: Some("https://ci.example/run/9".into()),
        }],
        comments: vec![
            WirePrComment {
                kind: WirePrCommentKind::Finding,
                anchor: "src/nav.rs:10-12".into(),
                snippet: Some("@@ -10,2 +10,3 @@\n+let h = ROW_H;".into()),
                resolved: true,
                replies: vec![WireReply {
                    id: None,
                    author_is_bot: false,
                    author: "bob".into(),
                    body: "Fixed in the next push.".into(),
                    created_at: "2026-11-12".into(),
                }],
                ..comment(WirePrCommentKind::Finding, "ada", "Use `ROW_H` here.")
            },
            comment(WirePrCommentKind::Comment, "bob", "Looks right."),
            WirePrComment {
                verdict: Some(WireReviewState::Approved),
                anchor: "review".into(),
                ..comment(WirePrCommentKind::Review, "cy", "")
            },
            WirePrComment {
                verdict: Some(WireReviewState::ChangesRequested),
                anchor: "review".into(),
                ..comment(WirePrCommentKind::Review, "di", "Please split this.")
            },
        ],
        truncated: false,
        checks_truncated: false,
        comments_truncated: true,
    }
}

struct World {
    view: Entity<CockpitView>,
    project: ProjectKey,
    path: std::path::PathBuf,
}

/// A window with `panes` Threads and a registered Project whose repository
/// lists `rows`, PR #7 readable by number.
fn world<'a>(
    name: &str,
    panes: usize,
    rows: Vec<WirePrRow>,
    more: bool,
    cx: &'a mut TestAppContext,
) -> (World, &'a mut gpui::VisualTestContext) {
    let (mut core, _fake) = cockpit(name, panes);
    let repo = repo_in(&scratch(&format!("{name}-repo")));
    let project = core.register_project(core.first_machine(), &repo).now().unwrap();
    let path = core.registry(core.first_machine()).project(project.project()).unwrap().root.clone();
    let roots: Vec<_> = core
        .registry(core.first_machine())
        .projects()
        .iter()
        .map(|project| project.root.clone())
        .collect();
    for root in roots {
        for filter in [WirePrFilter::Open, WirePrFilter::Closed, WirePrFilter::All] {
            core.fixture_pr_list(root.clone(), filter, list_of(rows.clone(), more));
        }
        core.fixture_forge_numbered(
            root,
            7,
            ReviewPullRequest {
                noun: "PR".into(),
                pr: Ok(full_pr()),
                unread: None,
            },
        );
    }
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    (
        World {
            view,
            project,
            path,
        },
        cx,
    )
}

fn two_rows() -> Vec<WirePrRow> {
    vec![
        row(7, "Keep rows tall", WirePrState::Open),
        row(3, "Older change", WirePrState::Merged),
    ]
}

/// With nothing open, the footer still reaches the PRs of the Project.
#[gpui::test]
fn the_footer_opens_the_prs_from_an_empty_board(cx: &mut TestAppContext) {
    let (world, cx) = world("prs-empty-board", 0, two_rows(), false, cx);
    cx.simulate_keystrokes("cmd-w");
    tick(cx);
    assert!(shows(cx, "empty-board"));
    click(cx, "nav-pull-requests");
    assert!(!shows(cx, "empty-board"));
    assert!(shows(cx, "prs-board"));
    lands(cx, "prs-row-7");
    assert!(shows(cx, "prs-row-3"), "its rows");
    world.view.read_with(cx, |view, cx| {
        assert_eq!(view.prs.read(cx).project, Some(world.project))
    });
}

/// A focused Thread whose branch has no PR no longer leaves the button
/// dead: it opens the list, on the Thread's Project.
#[gpui::test]
fn the_footer_opens_the_prs_from_a_thread_without_one(cx: &mut TestAppContext) {
    let (world, cx) = world("prs-no-pr-thread", 1, two_rows(), false, cx);
    assert!(!shows(cx, "prs-board"));
    click(cx, "nav-pull-requests");
    assert!(shows(cx, "prs-board"), "opened without a PR on the branch");
    lands(cx, "prs-row-3");
    world.view.read_with(cx, |view, cx| {
        assert_ne!(view.prs.read(cx).project, Some(world.project))
    });
}

/// A PR is chosen from the list by its number, whatever the checkout is on,
/// and shows as its tab does: Markdown for the prose, the thread's code
/// context, its resolved mark, its reply, its failed check, and a way past
/// the cap.
#[gpui::test]
fn a_listed_pr_shows_whole_with_its_review_threads(cx: &mut TestAppContext) {
    let (_world, cx) = world("prs-detail", 0, two_rows(), false, cx);
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    click(cx, "prs-row-7");
    assert!(shows(cx, "prs-detail"));
    lands(cx, "pr-body");
    let title = debug_bounds(cx, "pr-title".into()).unwrap();
    let tabs = debug_bounds(cx, "pr-views".into()).unwrap();
    assert!(
        title.bottom() <= tabs.top(),
        "PR identity stays above the views"
    );
    assert!(
        shows(cx, "pr-tab-Code"),
        "published Code is available without a checkout branch"
    );
    assert!(!shows(cx, "pr-primary"), "no actions: they are a Thread's");

    click(cx, "pr-tab-Timeline");
    lands(cx, "pr-snippet-0");
    assert!(
        shows(cx, "pr-title"),
        "PR identity remains visible on Timeline"
    );
    assert!(shows(cx, "pr-snippet-0"), "the thread's code context");
    assert!(shows(cx, "pr-resolved-0"), "resolved");
    assert!(!shows(cx, "pr-resolved-1"));
    assert!(shows(cx, "pr-reply-0-0"), "its nested reply");
    assert!(
        shows(cx, "pr-more-comments"),
        "the cap, with the way past it"
    );
}

#[gpui::test]
fn a_project_pr_reads_published_code_without_a_thread(cx: &mut TestAppContext) {
    use crate::cockpit::panel::pr_read::PrTarget;
    use crate::cockpit::tools::Diff;
    let (world, cx) = world("prs-published-code", 0, two_rows(), false, cx);
    world.view.update(cx, |view, cx| {
        let files = ferrite_core::protocol::sides_of_unified("diff --git a/remote.rs b/remote.rs\n--- a/remote.rs\n+++ b/remote.rs\n@@ -1 +1 @@\n-old\n+remote\n");
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_forge_diff(world.path.clone(), 7, files));
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_forge_diff(world.path.clone(), 3, Vec::new()));
    });
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    click(cx, "prs-row-7");
    lands(cx, "pr-body");
    click(cx, "pr-tab-Code");
    let target = PrTarget::project(world.project, 7);
    until(cx, |cx| {
        world
            .view
            .read_with(cx, |view, cx| view.changes.read(cx).files_read(Diff::PullRequest(target)))
            == Some(1)
    });
    assert!(!shows(cx, "changes-stage-0"));
    assert!(!shows(cx, "changes-open-0"));
    assert!(shows(cx, "changes-reviewed-0"));
    click(cx, "pr-tab-Timeline");
    assert!(shows(cx, "pr-snippet-0"));
    assert!(shows(cx, "pr-reply-0-0"));
}

/// A PR opened with Code already chosen reads its diff with no hover on
/// the Code segment, and the PR left keeps its own.
#[gpui::test]
fn a_project_pr_opened_on_code_reads_its_diff(cx: &mut TestAppContext) {
    use crate::cockpit::panel::pr_read::PrTarget;
    use crate::cockpit::tools::Diff;
    let (world, cx) = world("prs-code-reads", 0, two_rows(), false, cx);
    world.view.update(cx, |view, cx| {
        let files = ferrite_core::protocol::sides_of_unified("diff --git a/remote.rs b/remote.rs\n--- a/remote.rs\n+++ b/remote.rs\n@@ -1 +1 @@\n-old\n+remote\n");
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_forge_diff(world.path.clone(), 7, files));
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_forge_diff(world.path.clone(), 3, Vec::new()));
    });
    let read = |number: u64, cx: &mut gpui::VisualTestContext| {
        let diff = Diff::PullRequest(PrTarget::project(world.project, number));
        world.view.read_with(cx, |view, cx| view.changes.read(cx).files_read(diff))
    };
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    click(cx, "prs-row-7");
    lands(cx, "pr-body");
    click(cx, "pr-tab-Code");
    until(cx, |cx| read(7, cx) == Some(1));
    click(cx, "prs-back");
    click(cx, "prs-row-3");
    until(cx, |cx| read(3, cx) == Some(0));
    assert_eq!(read(7, cx), Some(1), "the PR left keeps what it read");
}

/// A review shows its verdict whether or not it has words, and a failed
/// check opens its run.
#[gpui::test]
fn a_pr_shows_each_review_verdict_and_its_failed_checks_run(cx: &mut TestAppContext) {
    let (_world, cx) = world("prs-verdicts", 0, two_rows(), false, cx);
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    click(cx, "prs-row-7");
    lands(cx, "pr-body");

    click(cx, "pr-tab-Timeline");
    lands(cx, "pr-verdict-2");
    assert!(
        shows(cx, "pr-comment-2"),
        "the approval stands with no body"
    );
    assert!(
        shows(cx, "pr-verdict-3"),
        "changes requested, with its words"
    );
    assert!(!shows(cx, "pr-verdict-0"), "a finding has no verdict");

    click(cx, "pr-tab-Summary");
    lands(cx, "pr-check-0");
    assert_eq!(cx.opened_url(), None);
    click(cx, "pr-check-0");
    assert_eq!(cx.opened_url().as_deref(), Some("https://ci.example/run/9"));
}

/// Leaving a PR's detail returns to the list as it was: its filter, its
/// marked row, and where it was scrolled.
#[gpui::test]
fn back_from_a_pr_returns_to_the_list_as_it_was_left(cx: &mut TestAppContext) {
    let rows: Vec<WirePrRow> = (1..=40)
        .map(|n| row(n + 100, &format!("Change {n}"), WirePrState::Open))
        .chain([row(7, "Keep rows tall", WirePrState::Open)])
        .collect();
    let (world, cx) = world("prs-retained", 0, rows, false, cx);
    click(cx, "nav-pull-requests");
    lands(cx, "prs-rows");
    click(cx, "prs-filter-All");
    lands(cx, "prs-rows");
    world.view.update(cx, |view, cx| {
        view.prs
            .read(cx)
            .scroll
            .set_offset(gpui::point(px(0.), px(-300.)));
        cx.notify();
    });
    tick(cx);
    let at = world
        .view
        .read_with(cx, |view, cx| view.prs.read(cx).scroll.offset());
    assert!(at.y < px(0.), "scrolled");
    world.view.update(cx, |view, cx| {
        view.prs.update(cx, |prs, cx| prs.open_detail(7, cx))
    });
    tick(cx);
    assert!(shows(cx, "prs-detail") && !shows(cx, "prs-rows"));
    click(cx, "prs-back");
    assert!(shows(cx, "prs-rows"), "the list again");
    world.view.read_with(cx, |view, cx| {
        let prs = view.prs.read(cx);
        assert_eq!(prs.filter, WirePrFilter::All, "its filter");
        assert_eq!(prs.selected, Some(7), "its marked row");
        assert_eq!(prs.scroll.offset(), at, "its scroll");
    });
}

/// A list the cap held back says so, with the way to the rest.
#[gpui::test]
fn a_capped_list_says_so(cx: &mut TestAppContext) {
    let (_world, cx) = world("prs-capped", 0, two_rows(), true, cx);
    click(cx, "nav-pull-requests");
    lands(cx, "prs-more");
}

/// A list that cannot be read says why and offers Retry, which reads again.
#[gpui::test]
fn a_failed_list_offers_retry(cx: &mut TestAppContext) {
    let (world, cx) = world("prs-failed", 0, two_rows(), false, cx);
    world.view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_pr_list(
            world.path.clone(),
            WirePrFilter::Open,
            ReviewPullRequests {
                list: Err("Rate limit reached".into()),
                unread: Some(WirePrUnread::RateLimited),
            },
        ))
    });
    click(cx, "nav-pull-requests");
    lands(cx, "prs-failed");
    assert!(!shows(cx, "prs-row-7"));
    world.view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_pr_list(
            world.path.clone(),
            WirePrFilter::Open,
            list_of(two_rows(), false),
        ))
    });
    click(cx, "prs-retry");
    lands(cx, "prs-row-7");
    assert!(!shows(cx, "prs-failed"));
}

fn set_list(world: &World, list: ReviewPullRequests, cx: &mut gpui::VisualTestContext) {
    world.view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_pr_list(world.path.clone(), WirePrFilter::Open, list))
    });
}

/// Opening the view reads the list again: a PR opened since shows, and a
/// read that fails leaves the rows it had, with Retry.
#[gpui::test]
fn the_list_reads_again_each_time_the_view_opens(cx: &mut TestAppContext) {
    let (world, cx) = world("prs-list-refresh", 0, two_rows(), false, cx);
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!shows(cx, "prs-board"), "esc closes the view");

    let mut rows = two_rows();
    rows.push(row(9, "Opened since", WirePrState::Open));
    set_list(&world, list_of(rows, false), cx);
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-9");

    cx.simulate_keystrokes("escape");
    tick(cx);
    set_list(
        &world,
        ReviewPullRequests {
            list: Err("Rate limit reached".into()),
            unread: Some(WirePrUnread::RateLimited),
        },
        cx,
    );
    click(cx, "nav-pull-requests");
    lands(cx, "prs-stale");
    assert!(
        shows(cx, "prs-row-9"),
        "the last rows stay under the failure"
    );
    assert!(shows(cx, "prs-retry"));
}

/// Opening a PR again reads it again over what it showed: the old content
/// stays with a mark while the read is out, then follows the forge.
#[gpui::test]
fn a_pr_opened_again_is_read_again_and_follows_the_forge(cx: &mut TestAppContext) {
    use crate::cockpit::panel::pr_read::PrTarget;

    let (world, cx) = world("prs-detail-refresh", 0, two_rows(), false, cx);
    let target = PrTarget::project(world.project, 7);
    let state = |cx: &mut gpui::VisualTestContext| {
        world.view.read_with(cx, |view, cx| {
            view.pull_request.read(cx).snapshot(target).map(|pr| pr.state)
        })
    };
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    click(cx, "prs-row-7");
    lands(cx, "pr-body");
    assert!(shows(cx, "pr-pill"), "the header shows the PR lifecycle");
    assert_eq!(state(cx), Some(WirePrState::Open));
    click(cx, "prs-back");

    world.view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_hold_pr_reads(world.path.clone()))
    });
    click(cx, "prs-row-7");
    assert!(shows(cx, "pr-body"), "the last read stays");
    assert!(shows(cx, "pr-refreshing"), "while the new one is out");
    assert_eq!(
        state(cx),
        Some(WirePrState::Open),
        "the last state stays too"
    );
    let mut merged = full_pr();
    merged.state = WirePrState::Merged;
    let reads = world
        .view
        .update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| cockpit.take_held_reads()));
    assert_eq!(reads.len(), 1, "one read for the opening");
    for read in reads {
        read.answer(ReviewPullRequest {
            noun: "PR".into(),
            pr: Ok(merged.clone()),
            unread: None,
        });
    }
    until(cx, |cx| !shows(cx, "pr-refreshing"));
    assert_eq!(
        state(cx),
        Some(WirePrState::Merged),
        "the new forge state lands"
    );
    assert!(shows(cx, "pr-body"));
    assert!(shows(cx, "pr-pill"));
}

/// A merged PR says it is merged and does not offer to merge; an open one
/// says whether it can. Its state shows in the destination as in a tab.
#[gpui::test]
fn a_merged_pr_shows_its_state_and_no_merge_line(cx: &mut TestAppContext) {
    let (world, cx) = world("prs-merged", 0, two_rows(), false, cx);
    let mut merged = full_pr();
    merged.state = WirePrState::Merged;
    world.view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_forge_numbered(
            world.path.clone(),
            7,
            ReviewPullRequest {
                noun: "PR".into(),
                pr: Ok(merged),
                unread: None,
            },
        ))
    });
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    click(cx, "prs-row-7");
    lands(cx, "pr-body");
    assert!(shows(cx, "pr-pill"), "its state");
    assert!(!shows(cx, "pr-merge"), "merged: nothing to merge");
}

/// The list and the detail answer the keyboard alone: ↑ ↓ mark a row, ⏎
/// opens it, esc goes back to the list, then closes the view.
#[gpui::test]
fn the_prs_view_is_walked_by_keys(cx: &mut TestAppContext) {
    let (world, cx) = world("prs-keys", 0, two_rows(), false, cx);
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-3");
    let marked = |cx: &mut gpui::VisualTestContext| {
        world.view.read_with(cx, |view, cx| view.prs.read(cx).selected)
    };
    cx.simulate_keystrokes("down");
    assert_eq!(marked(cx), Some(7), "the first row");
    cx.simulate_keystrokes("down");
    assert_eq!(marked(cx), Some(3));
    cx.simulate_keystrokes("down");
    assert_eq!(marked(cx), Some(3), "held at the last");
    cx.simulate_keystrokes("up");
    assert_eq!(marked(cx), Some(7));
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(shows(cx, "prs-detail"));
    lands(cx, "pr-body");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(shows(cx, "prs-rows"), "esc goes back to the list");
    assert_eq!(marked(cx), Some(7), "on the row it left");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!shows(cx, "prs-board"), "esc again closes the view");
}

/// On a Thread whose branch has a PR the footer keeps opening that PR in
/// the Panel; the palette's command always opens the destination.
#[gpui::test]
fn the_footer_keeps_a_threads_own_pr_and_the_palette_opens_the_list(cx: &mut TestAppContext) {
    use crate::cockpit::surfaces::SurfaceId;
    let (world, cx) = world("prs-own-pr", 1, two_rows(), false, cx);
    let thread = world
        .view
        .read_with(cx, |view, _| view.panes[0].thread().unwrap());
    world.view.update(cx, |view, cx| {
        view.pr_landed(thread, Some(pr_with_checks()), cx)
    });
    world.view.update(cx, |view, cx| {
        view.fixture_pr_snapshot(thread, super::pr::snapshot(WirePrState::Open), cx)
    });
    tick(cx);
    click(cx, "nav-pull-requests");
    assert!(!shows(cx, "prs-board"), "the Thread's PR, not the list");
    let tabs = world.view.read_with(cx, |view, _| {
        view.panel.surfaces.get(thread).unwrap().tabs.clone()
    });
    assert!(tabs.contains(&SurfaceId::PullRequest(48)), "{tabs:?}");

    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    cx.simulate_input("pull requests");
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(shows(cx, "prs-board"), "the palette always opens the list");
}

#[gpui::test]
fn a_transcript_pr_keeps_the_conversation_and_loads_the_requested_number(cx: &mut TestAppContext) {
    let (world, cx) = world("pr-transcript-number", 1, two_rows(), false, cx);
    let thread = world
        .view
        .read_with(cx, |view, cx| view.cockpit.read(cx).threads()[0]);
    world.view.update(cx, |view, cx| {
        view.pr_landed(thread, Some(pr_with_checks()), cx);
        let base =
            ferrite_pane::file_links::origin_base(&view.thread_path(thread, cx).unwrap()).unwrap();
        view.open_named_pr(thread, 7, format!("{base}/pull/7"), cx);
    });
    lands(cx, "pr-body");
    world.view.read_with(cx, |view, cx| {
        assert!(!view.destination_shown(), "the conversation stays visible");
        assert_eq!(
            view.active_surface(thread),
            Some(super::super::surfaces::SurfaceId::PullRequest(7))
        );
        assert_eq!(view.pr_snapshot(thread, cx).map(|pr| pr.number), Some(7));
    });
    assert!(
        !shows(cx, "pr-primary"),
        "another branch's PR has no checkout actions"
    );
    assert!(
        shows(cx, "pr-tab-Code"),
        "Code reads the requested PR independently of the checkout branch"
    );
}

#[gpui::test]
fn a_bare_pr_reference_uses_the_contextual_panel(cx: &mut TestAppContext) {
    let (world, cx) = world("pr-bare-number", 1, two_rows(), false, cx);
    let thread = world
        .view
        .read_with(cx, |view, cx| view.cockpit.read(cx).threads()[0]);
    world.view.update(cx, |view, cx| {
        view.pr_landed(thread, Some(pr_with_checks()), cx);
        let base =
            ferrite_pane::file_links::origin_base(&view.thread_path(thread, cx).unwrap()).unwrap();
        view.open_named_pr(thread, 7, format!("{base}/issues/7"), cx);
    });
    lands(cx, "pr-body");
    world.view.read_with(cx, |view, cx| {
        assert!(!view.destination_shown());
        assert_eq!(view.pr_snapshot(thread, cx).map(|pr| pr.number), Some(7));
    });
}

#[gpui::test]
fn a_foreign_pr_with_the_same_number_keeps_its_repository(cx: &mut TestAppContext) {
    let (world, cx) = world("pr-foreign-number", 1, two_rows(), false, cx);
    let thread = world
        .view
        .read_with(cx, |view, cx| view.cockpit.read(cx).threads()[0]);
    world.view.update(cx, |view, cx| {
        let pr = pr_with_checks();
        let number = pr.number;
        view.pr_landed(thread, Some(pr), cx);
        view.open_named_pr(
            thread,
            number,
            format!("https://github.com/other/repository/pull/{number}"),
            cx,
        );
        assert_ne!(
            view.active_surface(thread),
            Some(super::super::surfaces::SurfaceId::PullRequest(number))
        );
    });
}

/// Another Project chosen in the scope bar reads its own list, and the row
/// marked on the last one is let go: ⏎ opens nothing until a row is marked.
#[gpui::test]
fn choosing_another_project_reads_its_list_and_lets_the_mark_go(cx: &mut TestAppContext) {
    let (world, cx) = world("prs-scope", 1, two_rows(), false, cx);
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-3");
    cx.simulate_keystrokes("down");
    click(
        cx,
        &format!(
            "prs-project-{}-{}",
            world.project.machine().get(),
            world.project.project().get()
        ),
    );
    lands(cx, "prs-row-7");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(!shows(cx, "prs-detail"), "no row marked on the new list");
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(shows(cx, "prs-detail"), "a row marked again opens");
}

/// A state filter reads that state's list, and an empty one says so.
#[gpui::test]
fn a_filter_reads_its_own_list(cx: &mut TestAppContext) {
    let (world, cx) = world("prs-filter-empty", 0, two_rows(), false, cx);
    world.view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_pr_list(world.path.clone(), WirePrFilter::Closed, list_of(Vec::new(), false)))
    });
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    click(cx, "prs-filter-Closed");
    lands(cx, "prs-empty");
    assert!(!shows(cx, "prs-row-7"));
    click(cx, "prs-filter-Open");
    lands(cx, "prs-row-7");
}

/// With no Project, the view says so and offers to add one.
#[gpui::test]
fn with_no_project_the_view_offers_to_add_one(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("prs-no-project", 0);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    view.update(cx, |view, cx| {
        let projects: Vec<ProjectId> =
            view.cockpit.read(cx).registry(view.cockpit.read(cx).first_machine()).projects().iter().map(|project| project.id).collect();
        for project in projects {
            view.cockpit.update(cx, |cockpit, _| {
                let project = cockpit.registry(cockpit.first_machine()).project_key(project);
                cockpit.remove_project(project).now().unwrap()
            });
        }
    });
    click(cx, "nav-pull-requests");
    assert!(shows(cx, "prs-no-project"));
    click(cx, "prs-add-project");
    lands(cx, "project-editor-card");
}

/// A PR opened again shows the view it was left on, under the view's head.
#[gpui::test]
fn a_pr_opened_again_keeps_the_view_it_was_left_on(cx: &mut TestAppContext) {
    let (_world, cx) = world("prs-tab-kept", 0, two_rows(), false, cx);
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    assert!(shows(cx, "view-head-prs"), "the head names the destination");
    click(cx, "prs-row-7");
    lands(cx, "pr-body");
    click(cx, "pr-tab-Timeline");
    lands(cx, "pr-snippet-0");
    click(cx, "prs-back");
    click(cx, "prs-row-7");
    lands(cx, "pr-snippet-0");
}
