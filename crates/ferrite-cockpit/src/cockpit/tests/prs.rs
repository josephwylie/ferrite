//! The PRs destination: it opens from the footer whatever the board holds,
//! lists a Project's pull requests, reads one whole by number, and comes
//! back to its list as it was left.
use super::*;
use ferrite_core::protocol::{
    ReviewPullRequest, ReviewPullRequests, WireCheck, WireCheckStatus, WirePr, WirePrComment,
    WirePrCommentKind, WirePrFilter, WirePrList, WirePrRow, WirePrState, WirePrUnread, WireReply,
};
use ferrite_core::workspace::registry::ProjectId;

fn shows(cx: &mut gpui::VisualTestContext, selector: &str) -> bool {
    debug_bounds(cx, selector.to_string()).is_some()
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
        kind,
        author: author.into(),
        anchor: "comment".into(),
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
        state: WirePrState::Open,
        draft: false,
        head: "feature/7".into(),
        base: "main".into(),
        merge: ferrite_core::protocol::WireMerge::Clean,
        sync: ferrite_core::protocol::WireSync::Unknown,
        checks: vec![WireCheck {
            name: "fmt".into(),
            status: WireCheckStatus::Failure,
        }],
        comments: vec![
            WirePrComment {
                kind: WirePrCommentKind::Finding,
                anchor: "src/nav.rs:10-12".into(),
                snippet: Some("@@ -10,2 +10,3 @@\n+let h = ROW_H;".into()),
                resolved: true,
                replies: vec![WireReply {
                    author: "bob".into(),
                    body: "Fixed in the next push.".into(),
                    created_at: "2026-11-12".into(),
                }],
                ..comment(WirePrCommentKind::Finding, "ada", "Use `ROW_H` here.")
            },
            comment(WirePrCommentKind::Comment, "bob", "Looks right."),
        ],
        truncated: false,
        checks_truncated: false,
        comments_truncated: true,
    }
}

struct World {
    view: Entity<CockpitView>,
    project: ProjectId,
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
    let project = core.register_project(&repo).unwrap();
    let path = core.registry().project(project).unwrap().root.clone();
    let roots: Vec<_> = core
        .registry()
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
    world.view.update(cx, |view, cx| view.show_empty_board(cx));
    tick(cx);
    assert!(shows(cx, "empty-board"));
    click(cx, "nav-pull-requests");
    assert!(!shows(cx, "empty-board"));
    assert!(shows(cx, "prs-board"));
    assert!(shows(cx, "prs-row-7") && shows(cx, "prs-row-3"), "its rows");
    world.view.read_with(cx, |view, _| {
        assert_eq!(view.prs.project, Some(world.project))
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
    assert!(shows(cx, "prs-row-3"));
    world.view.read_with(cx, |view, _| {
        assert_ne!(view.prs.project, Some(world.project))
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
    click(cx, "prs-row-7");
    assert!(shows(cx, "prs-detail"));
    assert!(shows(cx, "pr-body"), "the description");
    assert!(!shows(cx, "pr-tab-Code"), "no checkout, no Code");
    assert!(!shows(cx, "pr-primary"), "no actions: they are a Thread's");

    click(cx, "pr-tab-Timeline");
    assert!(shows(cx, "pr-snippet-0"), "the thread's code context");
    assert!(shows(cx, "pr-resolved-0"), "resolved");
    assert!(!shows(cx, "pr-resolved-1"));
    assert!(shows(cx, "pr-reply-0-0"), "its nested reply");
    assert!(
        shows(cx, "pr-more-comments"),
        "the cap, with the way past it"
    );
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
    click(cx, "prs-filter-All");
    world.view.update(cx, |view, cx| {
        view.prs.scroll.set_offset(gpui::point(px(0.), px(-300.)));
        cx.notify();
    });
    tick(cx);
    let at = world.view.read_with(cx, |view, _| view.prs.scroll.offset());
    assert!(at.y < px(0.), "scrolled");
    world.view.update(cx, |view, cx| view.open_pr_detail(7, cx));
    tick(cx);
    assert!(shows(cx, "prs-detail") && !shows(cx, "prs-rows"));
    click(cx, "prs-back");
    assert!(shows(cx, "prs-rows"), "the list again");
    world.view.read_with(cx, |view, _| {
        assert_eq!(view.prs.filter, WirePrFilter::All, "its filter");
        assert_eq!(view.prs.selected, Some(7), "its marked row");
        assert_eq!(view.prs.scroll.offset(), at, "its scroll");
    });
}

/// A list the cap held back says so, with the way to the rest.
#[gpui::test]
fn a_capped_list_says_so(cx: &mut TestAppContext) {
    let (_world, cx) = world("prs-capped", 0, two_rows(), true, cx);
    click(cx, "nav-pull-requests");
    assert!(shows(cx, "prs-more"));
}

/// A list that cannot be read says why and offers Retry, which reads again.
#[gpui::test]
fn a_failed_list_offers_retry(cx: &mut TestAppContext) {
    let (world, cx) = world("prs-failed", 0, two_rows(), false, cx);
    world.view.update(cx, |view, _| {
        view.cockpit.fixture_pr_list(
            world.path.clone(),
            WirePrFilter::Open,
            ReviewPullRequests {
                list: Err("Rate limit reached".into()),
                unread: Some(WirePrUnread::RateLimited),
            },
        )
    });
    click(cx, "nav-pull-requests");
    assert!(shows(cx, "prs-failed") && !shows(cx, "prs-row-7"));
    world.view.update(cx, |view, _| {
        view.cockpit.fixture_pr_list(
            world.path.clone(),
            WirePrFilter::Open,
            list_of(two_rows(), false),
        )
    });
    click(cx, "prs-retry");
    assert!(shows(cx, "prs-row-7") && !shows(cx, "prs-failed"));
}
