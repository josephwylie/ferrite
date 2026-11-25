//! The PRs destination: it opens from the footer whatever the board holds,
//! lists a Project's pull requests, reads one whole by number, and comes
//! back to its list as it was left.
use super::*;
use ferrite_core::project::ProjectId;
use ferrite_core::protocol::{
    ReviewPullRequest, ReviewPullRequests, WireCheck, WireCheckStatus, WirePr, WirePrComment,
    WirePrCommentKind, WirePrFilter, WirePrList, WirePrRow, WirePrState, WirePrUnread, WireReply,
    WireReviewState,
};

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
        assert_eq!(view.prs.read(cx).project(), Some(world.project))
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
        assert_ne!(view.prs.read(cx).project(), Some(world.project))
    });
}

#[gpui::test]
fn a_project_pr_reads_published_code_without_a_thread(cx: &mut TestAppContext) {
    use ferrite_core::pull_request::PrTarget;
    use ferrite_tools::changes::Diff;
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
    use ferrite_core::pull_request::PrTarget;
    use ferrite_tools::changes::Diff;
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
    use ferrite_core::pull_request::PrTarget;

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

/// The scope offers the window's Projects: one chosen reads its own
/// list from its repository.
#[gpui::test]
fn the_scope_offers_the_windows_projects_and_reads_the_one_chosen(cx: &mut TestAppContext) {
    let (world, cx) = world("prs-scope", 1, two_rows(), false, cx);
    world.view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_pr_list(world.path.clone(), WirePrFilter::Open, list_of(vec![row(9, "Its own", WirePrState::Open)], false)))
    });
    click(cx, "nav-pull-requests");
    lands(cx, "prs-row-3");
    click(
        cx,
        &format!(
            "prs-project-{}-{}",
            world.project.machine().get(),
            world.project.project().get()
        ),
    );
    lands(cx, "prs-row-9");
    assert!(!shows(cx, "prs-row-3"), "the chosen Project's list, not the last one's");
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
