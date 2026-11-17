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
        author_avatar_url: None,
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
        reviewers: Some(vec![ferrite_core::protocol::WirePrReviewer {
            login: "ada".into(),
            name: Some("Ada".into()),
            avatar_url: None,
            verdict: Some(WireReviewState::Approved),
        }]),
        labels: Some(vec![ferrite_core::protocol::WirePrLabel {
            name: "bug".into(),
            color: Some("d73a4a".into()),
            description: Some("A regression".into()),
        }]),
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
                    author_avatar_url: None,
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
    lands(cx, "prs-row-7");
    assert!(shows(cx, "prs-row-3"), "its rows");
    world.view.read_with(cx, |view, _| {
        assert_eq!(view.prs.project, Some(world.project))
    });
}

/// Summary's metadata and conversation are reachable without leaving the tab.
#[gpui::test]
fn summary_shows_metadata_and_filters_the_conversation_without_dropping_it(
    cx: &mut TestAppContext,
) {
    let (world, cx) = world("prs-summary", 0, two_rows(), false, cx);
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    click(cx, "prs-row-7");
    lands(cx, "pr-body");
    assert!(shows(cx, "pr-reviewer-0"));
    assert!(shows(cx, "pr-label-0"));
    assert!(!shows(cx, "pr-check-0"));
    assert!(
        !shows(cx, "pr-comment-0"),
        "resolved conversation starts hidden"
    );
    assert!(shows(cx, "pr-comment-1"), "plain comments live on Summary");
    click(cx, "pr-comments-filter");
    cx.update(|window, cx| {
        world
            .view
            .update(cx, |view, cx| view.press_menu_row(1, window, cx))
    });
    tick(cx);
    assert!(
        world
            .view
            .read_with(cx, |view, _| view.context_menu.is_some()),
        "filter menu remains open"
    );
    world.view.update(cx, |view, cx| {
        view.context_menu = None;
        cx.notify();
    });
    tick(cx);
    assert!(shows(cx, "pr-comment-0"));
    assert!(shows(cx, "pr-snippet-0"));
    assert!(shows(cx, "pr-reply-0-0"));
    click(cx, "pr-section-description");
    assert!(!shows(cx, "pr-body"));
    click(cx, "pr-tab-Timeline");
    click(cx, "pr-tab-Summary");
    assert!(!shows(cx, "pr-body"), "section state survives tab changes");
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
    assert!(shows(cx, "pr-checks"));
    assert!(!shows(cx, "pr-activity-count"));

    click(cx, "pr-tab-Timeline");
    lands(cx, "pr-snippet-0");
    assert!(!shows(cx, "pr-checks"));
    assert!(shows(cx, "pr-activity-count"));
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
    world.view.update(cx, |view, _| {
        let files = ferrite_core::protocol::sides_of_unified(concat!(
            "diff --git a/src/remote.rs b/src/remote.rs\n--- a/src/remote.rs\n+++ b/src/remote.rs\n@@ -1 +1 @@\n-old\n+remote\n",
            "diff --git a/tests/remote.rs b/tests/remote.rs\nnew file mode 100644\n--- /dev/null\n+++ b/tests/remote.rs\n@@ -0,0 +1 @@\n+test\n",
        ));
        view.cockpit.fixture_forge_diff(world.path.clone(), 7, files);
        view.cockpit.fixture_forge_diff(world.path.clone(), 3, Vec::new());
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
            .read_with(cx, |view, _| view.changes_read(Diff::PullRequest(target)))
            == Some(2)
    });
    assert!(!shows(cx, "pr-checks"));
    assert!(!shows(cx, "pr-activity-count"));
    assert!(
        !shows(cx, "changes-filter"),
        "PR Code has no local Changes search row"
    );
    assert!(shows(cx, "pr-code-files-2"));
    assert!(shows(cx, "pr-code-viewed-0-2"));
    assert!(shows(cx, "pr-code-path-0"));
    assert!(
        shows(cx, "pr-code-deletions-1"),
        "zero deletions still has a counter"
    );
    assert!(!shows(cx, "changes-stage-0"));
    assert!(!shows(cx, "changes-open-0"));
    assert!(shows(cx, "changes-reviewed-0"));
    assert!(shows(cx, "pr-code-line-0-0-old-1"));
    assert!(shows(cx, "pr-code-line-0-0-new-none"));
    assert!(shows(cx, "pr-code-line-0-1-old-none"));
    assert!(shows(cx, "pr-code-line-0-1-new-1"));

    // A query left in a hidden field cannot silently hide published files.
    let diff = Diff::PullRequest(target);
    let field = world
        .view
        .read_with(cx, |view, _| view.changes_filter(diff).unwrap());
    world.view.update(cx, |_, cx| {
        field.update(cx, |field, cx| field.set("no-such-file".into(), cx))
    });
    tick(cx);
    assert!(shows(cx, "changes-file-0") && shows(cx, "changes-file-1"));

    click(cx, "changes-reviewed-0");
    assert!(world.view.read_with(cx, |view, _| {
        view.changes_reviewed_for_test(diff, "src/remote.rs")
    }));
    assert!(shows(cx, "pr-code-viewed-1-2"));
    click(cx, "changes-hide-tests");
    assert!(shows(cx, "pr-code-files-1"));
    assert!(shows(cx, "pr-code-viewed-1-1"));
    assert!(!shows(cx, "changes-file-1"));
    click(cx, "pr-code-hidden-tests");
    assert!(shows(cx, "pr-code-files-2") && shows(cx, "changes-file-1"));
    click(cx, "changes-reviewed-0");
    assert!(!world.view.read_with(cx, |view, _| {
        view.changes_reviewed_for_test(diff, "src/remote.rs")
    }));
    assert!(shows(cx, "pr-code-viewed-0-2"));
    click(cx, "pr-tab-Timeline");
    assert!(shows(cx, "pr-snippet-0"));
    assert!(shows(cx, "pr-reply-0-0"));
}

#[gpui::test]
fn summary_headings_stick_and_collapsing_keeps_the_heading_in_place(cx: &mut TestAppContext) {
    use crate::cockpit::panel::pr_read::PrTarget;

    let (world, cx) = world("prs-summary-scroll", 0, two_rows(), false, cx);
    let mut pr = full_pr();
    pr.body = "A paragraph in the long description.\n\n".repeat(60);
    pr.comments = vec![comment(
        WirePrCommentKind::Comment,
        "ada",
        &"A paragraph in the long conversation.\n\n".repeat(60),
    )];
    world.view.update(cx, |view, _| {
        view.cockpit.fixture_forge_numbered(
            world.path.clone(),
            7,
            ReviewPullRequest {
                noun: "PR".into(),
                pr: Ok(pr),
                unread: None,
            },
        );
    });
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    click(cx, "prs-row-7");
    lands(cx, "pr-body");
    let target = PrTarget::project(world.project, 7);
    let scroll = world.view.read_with(cx, |view, _| {
        view.panel
            .pr_reads
            .get(target)
            .unwrap()
            .summary_scroll
            .clone()
    });
    let viewport = debug_bounds(cx, "pr-summary".into()).unwrap();
    let heading = debug_bounds(cx, "pr-section-description".into()).unwrap();
    assert!(
        heading.top() > viewport.top(),
        "metadata precedes the section"
    );
    click(cx, "pr-section-description");
    assert_eq!(
        scroll.offset().y,
        px(0.),
        "an unpinned collapse leaves the scroll alone"
    );
    click(cx, "pr-section-description");

    cx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-250.))),
        ..Default::default()
    });
    tick(cx);
    let viewport = debug_bounds(cx, "pr-summary".into()).unwrap();
    let pinned = debug_bounds(cx, "pr-section-description".into()).unwrap();
    assert!(
        (pinned.top() - viewport.top()).abs() <= px(1.),
        "the real heading stays at the viewport edge"
    );
    let before = scroll.offset();
    assert!(before.y < px(-100.));
    click(cx, "pr-tab-Timeline");
    click(cx, "pr-tab-Summary");
    assert_eq!(scroll.offset(), before, "Summary restores its own position");

    // The next section pushes this heading away rather than overlapping it.
    let description = scroll.bounds_for_item(1).unwrap();
    let near_end = description.bottom() - viewport.top() - pinned.size.height / 2.;
    world.view.update(cx, |_, cx| {
        scroll.set_offset(gpui::point(px(0.), -near_end));
        cx.notify();
    });
    tick(cx);
    let leaving = debug_bounds(cx, "pr-section-description".into()).unwrap();
    let next = debug_bounds(cx, "pr-section-checks".into()).unwrap();
    assert!(leaving.top() < viewport.top());
    assert!(
        (leaving.bottom() - next.top()).abs() <= px(1.),
        "the sticky heading stops at its section boundary"
    );

    world.view.update(cx, |_, cx| {
        scroll.set_offset(before);
        cx.notify();
    });
    tick(cx);
    click(cx, "pr-section-description");
    assert!(!shows(cx, "pr-body"));
    let collapsed = debug_bounds(cx, "pr-section-description".into()).unwrap();
    assert!(
        (collapsed.top() - viewport.top()).abs() <= px(1.),
        "collapse preserves the pinned heading's place"
    );
    assert!(
        scroll.offset().y > before.y,
        "the removed body is compensated before layout clamps"
    );
}

#[gpui::test]
fn pr_header_compensates_scroll_and_retains_each_tabs_state(cx: &mut TestAppContext) {
    use crate::cockpit::panel::pr_read::PrTarget;
    use crate::cockpit::surfaces::PrTab;
    use crate::cockpit::tools::Diff;
    let (world, cx) = world("pr-header-scroll", 1, two_rows(), false, cx);
    let thread = world
        .view
        .read_with(cx, |view, _| view.cockpit.threads()[0]);
    let target = PrTarget::thread(thread, 7);
    world.view.update(cx, |view, cx| {
        let path = view.thread_path(thread).unwrap();
        let base = ferrite_pane::file_links::origin_base(&path).unwrap();
        let mut pr = full_pr();
        pr.body = "A long description paragraph.\n\n".repeat(60);
        pr.comments = vec![comment(
            WirePrCommentKind::Comment,
            "ada",
            &"A long comment paragraph.\n\n".repeat(60),
        )];
        view.cockpit.fixture_forge_numbered(
            path.clone(),
            7,
            ReviewPullRequest { noun: "PR".into(), pr: Ok(pr), unread: None },
        );
        let patch = format!("diff --git a/header.rs b/header.rs\nnew file mode 100644\n--- /dev/null\n+++ b/header.rs\n@@ -0,0 +1,100 @@\n{}", "+let value = 1;\n".repeat(100));
        view.cockpit.fixture_forge_diff(
            path,
            7,
            ferrite_core::protocol::sides_of_unified(&patch),
        );
        view.open_named_pr(thread, 7, format!("{base}/pull/7"), cx);
    });
    lands(cx, "pr-body");
    let (summary, timeline, state) = world.view.read_with(cx, |view, _| {
        let read = view.panel.pr_reads.get(target).unwrap();
        (
            read.summary_scroll.clone(),
            read.timeline_scroll.clone(),
            read.header.clone(),
        )
    });
    let (expanded, compact) = {
        let state = state.borrow();
        (state.expanded_height, state.compact_height)
    };
    assert!(
        expanded > compact && compact > 0.,
        "both real header layouts are measured"
    );
    assert!(
        !shows(cx, "pr-identity-compact"),
        "the inactive form does not prepaint"
    );
    let original_body_top = debug_bounds(cx, "pr-body".into()).unwrap().top();
    let set_top = |scroll: &gpui::ScrollHandle, top: f32, cx: &mut gpui::VisualTestContext| {
        world.view.update(cx, |_, cx| {
            scroll.set_offset(gpui::point(px(0.), px(-top)));
            cx.notify();
        });
        tick(cx);
    };
    set_top(&summary, expanded + 32., cx);
    assert!(
        !world
            .view
            .read_with(cx, |view, _| view.pr_condensed(target)),
        "the threshold is strict"
    );
    let requested_top = expanded + 96.;
    set_top(&summary, requested_top, cx);
    lands(cx, "pr-identity-compact");
    assert!(shows(cx, "pr-condensed-title"));
    assert!(
        !shows(cx, "pr-title") && !shows(cx, "pr-copy-head"),
        "the inactive expanded form has no drawn controls"
    );
    let expected_top = requested_top - (expanded - compact);
    assert!(
        (-f32::from(summary.offset().y) - expected_top).abs() <= 1.,
        "scroll uses the saved absolute compensation target"
    );
    let moved_body_top = debug_bounds(cx, "pr-body".into()).unwrap().top();
    assert!(
        (moved_body_top - (original_body_top - px(requested_top))).abs() <= px(1.),
        "the content does not jump when chrome shrinks"
    );
    let summary_offset = summary.offset();

    click(cx, "pr-tab-Timeline");
    assert!(!shows(cx, "pr-identity-compact"));
    set_top(&timeline, requested_top, cx);
    lands(cx, "pr-identity-compact");
    click(cx, "pr-tab-Guide");
    assert!(
        !shows(cx, "pr-identity-compact"),
        "Guide has no condensing scroller"
    );
    assert!(shows(cx, "pr-title"));
    assert!(!state.borrow().condensed(PrTab::Guide));
    click(cx, "pr-tab-Code");
    lands(cx, "pr-code-files-1");
    assert!(!shows(cx, "pr-identity-compact"));
    let code = world.view.read_with(cx, |view, _| {
        view.changes_scroll(Diff::PullRequest(target)).unwrap()
    });
    set_top(&code, requested_top, cx);
    lands(cx, "pr-identity-compact");
    click(cx, "pr-tab-Summary");
    assert!(shows(cx, "pr-identity-compact"));
    assert_eq!(
        summary.offset(),
        summary_offset,
        "switching tabs restores Summary's compensated position"
    );
    set_top(&summary, 3., cx);
    until(cx, |cx| !shows(cx, "pr-identity-compact"));
    assert!(shows(cx, "pr-title"));
    let state = state.borrow();
    assert!(!state.condensed(PrTab::Summary));
    assert!(state.condensed(PrTab::Timeline) && state.condensed(PrTab::Code));
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
    assert!(!shows(cx, "pr-check-0"), "Checks starts closed");
    click(cx, "pr-section-checks");
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
    lands(cx, "prs-more");
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
    lands(cx, "prs-failed");
    assert!(!shows(cx, "prs-row-7"));
    world.view.update(cx, |view, _| {
        view.cockpit.fixture_pr_list(
            world.path.clone(),
            WirePrFilter::Open,
            list_of(two_rows(), false),
        )
    });
    click(cx, "prs-retry");
    lands(cx, "prs-row-7");
    assert!(!shows(cx, "prs-failed"));
}

fn set_list(world: &World, list: ReviewPullRequests, cx: &mut gpui::VisualTestContext) {
    world.view.update(cx, |view, _| {
        view.cockpit
            .fixture_pr_list(world.path.clone(), WirePrFilter::Open, list)
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
        world.view.read_with(cx, |view, _| {
            view.panel.pr_reads.snapshot(target).map(|pr| pr.state)
        })
    };
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    click(cx, "prs-row-7");
    lands(cx, "pr-body");
    assert!(shows(cx, "pr-pill"), "the header shows the PR lifecycle");
    assert_eq!(state(cx), Some(WirePrState::Open));
    click(cx, "prs-back");

    world.view.update(cx, |view, _| {
        view.cockpit.fixture_hold_pr_reads(world.path.clone())
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
        .update(cx, |view, _| view.cockpit.take_held_reads());
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
    world.view.update(cx, |view, _| {
        view.cockpit.fixture_forge_numbered(
            world.path.clone(),
            7,
            ReviewPullRequest {
                noun: "PR".into(),
                pr: Ok(merged),
                unread: None,
            },
        )
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
    let marked =
        |cx: &mut gpui::VisualTestContext| world.view.read_with(cx, |view, _| view.prs.selected);
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
    world.view.update(cx, |view, _| {
        view.fixture_pr_snapshot(thread, super::pr::snapshot(WirePrState::Open))
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
        .read_with(cx, |view, _| view.cockpit.threads()[0]);
    world.view.update(cx, |view, cx| {
        view.pr_landed(thread, Some(pr_with_checks()), cx);
        let base =
            ferrite_pane::file_links::origin_base(&view.thread_path(thread).unwrap()).unwrap();
        view.open_named_pr(thread, 7, format!("{base}/pull/7"), cx);
    });
    lands(cx, "pr-body");
    world.view.read_with(cx, |view, _| {
        assert!(!view.destination_shown(), "the conversation stays visible");
        assert_eq!(
            view.active_surface(thread),
            Some(super::super::surfaces::SurfaceId::PullRequest(7))
        );
        assert_eq!(view.pr_snapshot(thread).map(|pr| pr.number), Some(7));
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
fn a_restored_code_tab_loads_and_wraps_without_repeated_reads(cx: &mut TestAppContext) {
    use crate::cockpit::panel::pr_read::PrTarget;
    use crate::cockpit::surfaces::PrTab;
    use crate::cockpit::tools::Diff;

    let (world, cx) = world("pr-restored-code", 1, two_rows(), false, cx);
    let thread = world
        .view
        .read_with(cx, |view, _| view.cockpit.threads()[0]);
    let target = PrTarget::thread(thread, 7);
    let diff = Diff::PullRequest(target);
    let path = world.view.update(cx, |view, cx| {
        view.pr_landed(thread, Some(pr_with_checks()), cx);
        let path = view.thread_path(thread).unwrap();
        let base = ferrite_pane::file_links::origin_base(&path).unwrap();
        let wide = format!("pub fn requested() {{ let text = {:?}; }}", "source text ".repeat(24));
        let files = ferrite_core::protocol::sides_of_unified(&format!("diff --git a/requested.rs b/requested.rs\n--- a/requested.rs\n+++ b/requested.rs\n@@ -1 +1 @@\n-old\n+{wide}\n"));
        view.cockpit.fixture_forge_diff(path.clone(), 7, files);
        // This is what session restoration retains, without runtime Changes state.
        view.surfaces_mut(thread).pr_tabs.insert(7, PrTab::Code);
        assert_eq!(view.changes_generation_for_test(diff), None);
        view.open_named_pr(thread, 7, format!("{base}/pull/7"), cx);
        path
    });
    lands(cx, "pr-code-files-1");
    world.view.read_with(cx, |view, _| {
        assert!(!view.destination_shown(), "the conversation is retained");
        assert_eq!(view.pr_snapshot(thread).map(|pr| pr.number), Some(7));
        assert_eq!(view.changes_generation_for_test(diff), Some(1));
    });
    let wrapped = debug_bounds(cx, "panel-hunk-0-1".into()).unwrap();
    let viewport = debug_bounds(cx, "changes-scroll".into()).unwrap();
    assert!(
        wrapped.size.height > px(ferrite_ui::theme::LH_PANEL_DIFF),
        "selected Wrap grows the source row inside a nowrap Panel"
    );
    assert!(wrapped.right() <= viewport.right() + px(1.));
    assert!(!shows(cx, "panel-hunks-scroll-0"));
    click(cx, "changes-wrap");
    let scroller = debug_bounds(cx, "panel-hunks-scroll-0".into()).unwrap();
    let unwrapped = debug_bounds(cx, "panel-hunk-0-1".into()).unwrap();
    assert_eq!(unwrapped.size.height, px(ferrite_ui::theme::LH_PANEL_DIFF));
    assert!(unwrapped.size.width > scroller.size.width);
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: scroller.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(-5_000.), px(0.))),
        ..Default::default()
    });
    tick(cx);
    let slid = debug_bounds(cx, "panel-hunk-0-1".into()).unwrap();
    assert!(
        slid.left() < unwrapped.left(),
        "unwrapped text remains horizontally reachable"
    );
    assert!(slid.right() <= scroller.right() + px(1.));
    world
        .view
        .update(cx, |view, _| view.cockpit.fixture_hold_pr_reads(path));
    for _ in 0..3 {
        world.view.update(cx, |_, cx| cx.notify());
        tick(cx);
    }
    world.view.update(cx, |view, _| {
        assert_eq!(
            view.changes_generation_for_test(diff),
            Some(1),
            "rendering does not reissue the diff"
        );
        assert!(
            view.cockpit.take_held_reads().is_empty(),
            "rendering does not reissue metadata"
        );
    });
}

#[gpui::test]
fn retained_project_code_loads_each_requested_number(cx: &mut TestAppContext) {
    use crate::cockpit::panel::pr_read::PrTarget;
    use crate::cockpit::tools::Diff;

    let (world, cx) = world("prs-retained-code-number", 0, two_rows(), false, cx);
    world.view.update(cx, |view, _| {
        view.cockpit.fixture_forge_diff(world.path.clone(), 7, Vec::new());
        let mut next = full_pr();
        next.number = 3;
        next.title = "Another published PR".into();
        view.cockpit.fixture_forge_numbered(world.path.clone(), 3, ReviewPullRequest { noun: "PR".into(), pr: Ok(next), unread: None });
        let files = ferrite_core::protocol::sides_of_unified("diff --git a/third.rs b/third.rs\n--- a/third.rs\n+++ b/third.rs\n@@ -1 +1 @@\n-old\n+third\n");
        view.cockpit.fixture_forge_diff(world.path.clone(), 3, files);
    });
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-7");
    click(cx, "prs-row-7");
    lands(cx, "pr-body");
    click(cx, "pr-tab-Code");
    click(cx, "prs-back");
    click(cx, "prs-row-3");
    lands(cx, "pr-code-files-1");
    world.view.read_with(cx, |view, _| {
        let target = PrTarget::project(world.project, 3);
        assert_eq!(
            view.panel.pr_reads.snapshot(target).map(|pr| pr.number),
            Some(3)
        );
        assert_eq!(view.changes_read(Diff::PullRequest(target)), Some(1));
        assert_eq!(
            view.changes_generation_for_test(Diff::PullRequest(target)),
            Some(1)
        );
    });
}

#[gpui::test]
fn a_bare_pr_reference_uses_the_contextual_panel(cx: &mut TestAppContext) {
    let (world, cx) = world("pr-bare-number", 1, two_rows(), false, cx);
    let thread = world
        .view
        .read_with(cx, |view, _| view.cockpit.threads()[0]);
    world.view.update(cx, |view, cx| {
        view.pr_landed(thread, Some(pr_with_checks()), cx);
        let base =
            ferrite_pane::file_links::origin_base(&view.thread_path(thread).unwrap()).unwrap();
        view.open_named_pr(thread, 7, format!("{base}/issues/7"), cx);
    });
    lands(cx, "pr-body");
    world.view.read_with(cx, |view, _| {
        assert!(!view.destination_shown());
        assert_eq!(view.pr_snapshot(thread).map(|pr| pr.number), Some(7));
    });
}

#[gpui::test]
fn a_foreign_pr_with_the_same_number_keeps_its_repository(cx: &mut TestAppContext) {
    let (world, cx) = world("pr-foreign-number", 1, two_rows(), false, cx);
    let thread = world
        .view
        .read_with(cx, |view, _| view.cockpit.threads()[0]);
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
