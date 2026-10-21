//! The panel's PR tab (SPEC §5.6.2): what it draws of each snapshot, its
//! reads' cadence, and that a hidden tab reads nothing.
use super::*;
use crate::cockpit::panel::PanelTab;
use ferrite_reviewr::forge::{
    Check, CheckStatus, Comment, CommentKind, Merge, PrSnapshot, PrState, PrView, Sync,
};

/// The crash fix's pull request, as the forge reads it.
fn snapshot(state: PrState, merge: Merge, sync: Sync, checks: &[CheckStatus]) -> PrView {
    let comment = |kind, at: &str, resolved| Comment {
        kind,
        author: "claude".into(),
        author_is_bot: true,
        anchor: "src/scheduled_work.rs:89-91".into(),
        place: None,
        body: "`requeue` retries forever.".into(),
        snippet: None,
        created_at: at.into(),
        is_resolved: resolved,
        is_outdated: false,
        replies: Vec::new(),
    };
    PrView::Pr(Box::new(PrSnapshot {
        number: 146,
        title: "Requeue scheduled work whose worktree is gone".into(),
        url: "https://github.com/o/r/pull/146".into(),
        body: String::new(),
        state,
        is_draft: false,
        head_ref: "scheduled-work-crash".into(),
        head_is_fork: false,
        head_oid: "5be1c0d4".into(),
        base_ref: "main".into(),
        merge,
        sync,
        checks: checks
            .iter()
            .enumerate()
            .map(|(at, status)| Check {
                name: format!("check {at}"),
                status: *status,
            })
            .collect(),
        comments: vec![
            comment(CommentKind::Finding, "2026-10-21T01:25:30Z", false),
            comment(CommentKind::Finding, "2026-10-21T01:00:00Z", true),
        ],
        comments_truncated: false,
        checks_truncated: false,
    }))
}

/// A Solo window of one Thread at 1600 wide, its forge answering `answer`
/// from the start, so the nav label's read on opening answers too.
fn solo<'a>(
    name: &str,
    answer: PrView,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    ThreadId,
    &'a mut gpui::VisualTestContext,
) {
    let (mut core, _fake) = cockpit(name, 1);
    let thread = core.threads()[0];
    let open = core.thread(thread).expect("open");
    let path =
        ferrite_core::workspace::effective_cwd(open.session_project_root(), open.workspace())
            .expect("a checkout")
            .to_path_buf();
    core.fixture_forge(path, answer);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1600.), px(960.)));
    tick(cx);
    (view, thread, cx)
}

/// From now on the Thread's checkout's forge answers `answer`.
fn forge(
    view: &Entity<CockpitView>,
    thread: ThreadId,
    answer: PrView,
    cx: &mut gpui::VisualTestContext,
) {
    view.update(cx, |view, _| {
        let path = view.thread_path(thread).expect("a checkout");
        view.cockpit.fixture_forge(path, answer);
    });
}

/// Pump until no read of the Thread's PR is on its way (each answer comes
/// back on a thread of its own).
fn settle(view: &Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) {
    for _ in 0..200 {
        tick(cx);
        if !view.read_with(cx, |view, _| view.pr_tab.fetching(thread)) {
            tick(cx);
            return;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    panic!("the read never landed");
}

fn open_tab(
    view: &Entity<CockpitView>,
    thread: ThreadId,
    tab: PanelTab,
    cx: &mut gpui::VisualTestContext,
) {
    view.update(cx, |view, cx| view.open_panel_tab(thread, tab, cx));
    tick(cx);
}

fn reads(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> usize {
    view.update(cx, |view, _| std::mem::take(&mut view.pr_tab_reads).len())
}

fn shows(cx: &mut gpui::VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

/// Each state draws what it should: an open PR's pill and rows, a failing
/// one's, a merged one's, the empty state, and the line for no `gh`.
#[gpui::test]
fn the_tab_renders_each_state(cx: &mut TestAppContext) {
    use CheckStatus::{Failure, Running, Success};
    let open = snapshot(
        PrState::Open,
        Merge::Clean,
        Sync::Unpushed(1),
        &[Running, Success, Success],
    );
    let (view, thread, cx) = solo("pr-tab-states", open, cx);
    open_tab(&view, thread, PanelTab::Pr, cx);
    settle(&view, thread, cx);
    for selector in [
        "pr-head",
        "pr-pill-Open",
        "pr-title",
        "pr-ready-checks-running",
        "pr-ready-clean",
        "pr-ready-unpushed",
        "pr-check-0",
        "pr-check-2",
        "pr-comments-head",
        "pr-comment-0-anchor",
        "pr-resolved",
        "pr-review",
    ] {
        assert!(shows(cx, selector), "open: {selector}");
    }
    assert!(!shows(cx, "pr-comment-1"), "the resolved one is folded");
    let resolved = cx.debug_bounds("pr-resolved").unwrap();
    cx.simulate_click(resolved.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(shows(cx, "pr-comment-1"), "unfolded");

    let mut states = vec![
        (
            snapshot(
                PrState::Open,
                Merge::Conflicting,
                Sync::InSync,
                &[Failure, Success, Success],
            ),
            vec![
                "pr-pill-Open",
                "pr-ready-checks-failed",
                "pr-ready-conflicts",
                "pr-ready-in-sync",
            ],
        ),
        (
            snapshot(
                PrState::Merged,
                Merge::Clean,
                Sync::InSync,
                &[Success, Success, Success],
            ),
            vec![
                "pr-pill-Merged",
                "pr-ready-checks-passed",
                "pr-ready-merged",
            ],
        ),
        (PrView::NoPr, vec!["panel-no-pr"]),
        (
            PrView::NotAuthed(ferrite_reviewr::git::Forge::GitHub, "github.com".into()),
            vec!["panel-no-gh"],
        ),
    ];
    for (answer, wanted) in states.drain(..) {
        forge(&view, thread, answer, cx);
        let refresh = cx.debug_bounds("pr-refresh");
        match refresh {
            Some(refresh) => cx.simulate_click(refresh.center(), gpui::Modifiers::none()),
            None => view.update(cx, |view, cx| view.read_pr_tab(thread, cx)),
        }
        settle(&view, thread, cx);
        for selector in &wanted {
            assert!(shows(cx, selector), "{wanted:?}: {selector}");
        }
    }
    assert!(!shows(cx, "pr-ready-merged"), "nothing of the PR is left");
}

/// The cadence: a snapshot whose sync moved flips the readiness row on
/// the next `AUTO_EVERY` tick, not before; its checks move the nav's
/// label too.
#[gpui::test]
fn a_newer_snapshot_flips_the_readiness_row_on_the_next_tick(cx: &mut TestAppContext) {
    use CheckStatus::{Failure, Success};
    let unpushed = snapshot(PrState::Open, Merge::Clean, Sync::Unpushed(1), &[Success]);
    let (view, thread, cx) = solo("pr-tab-cadence", unpushed, cx);
    open_tab(&view, thread, PanelTab::Pr, cx);
    settle(&view, thread, cx);
    assert!(shows(cx, "pr-ready-unpushed"));
    assert_eq!(
        view.read_with(cx, |view, _| view.thread_row(thread).details.pr),
        Some("#146 \u{2713}".into()),
        "the snapshot gives the label"
    );

    forge(
        &view,
        thread,
        snapshot(PrState::Open, Merge::Clean, Sync::InSync, &[Failure]),
        cx,
    );
    cx.executor().advance_clock(crate::pr::AUTO_EVERY / 2);
    tick(cx);
    assert!(shows(cx, "pr-ready-unpushed"), "not due yet");
    cx.executor().advance_clock(crate::pr::AUTO_EVERY / 2);
    tick(cx);
    settle(&view, thread, cx);
    assert!(shows(cx, "pr-ready-in-sync"), "pushed: in sync");
    assert!(!shows(cx, "pr-ready-unpushed"));
    assert_eq!(
        view.read_with(cx, |view, _| view.thread_row(thread).details.pr),
        Some("#146 \u{2717}".into()),
        "a check failing moves the label"
    );
}

/// The tab reads when it opens and on the cadence while it shows; hidden
/// — another tab, or the panel itself — it reads nothing however long.
#[gpui::test]
fn a_hidden_tab_makes_no_forge_reads(cx: &mut TestAppContext) {
    let open = snapshot(PrState::Open, Merge::Clean, Sync::InSync, &[]);
    let (view, thread, cx) = solo("pr-tab-hidden", open, cx);
    tick(cx);
    assert_eq!(reads(&view, cx), 0, "Changes shows: no read");

    open_tab(&view, thread, PanelTab::Pr, cx);
    settle(&view, thread, cx);
    assert_eq!(reads(&view, cx), 1, "read on opening");
    cx.executor().advance_clock(crate::pr::AUTO_EVERY);
    tick(cx);
    settle(&view, thread, cx);
    assert_eq!(reads(&view, cx), 1, "and on the cadence");

    open_tab(&view, thread, PanelTab::Changes, cx);
    for _ in 0..3 {
        cx.executor().advance_clock(crate::pr::AUTO_EVERY);
        tick(cx);
    }
    assert_eq!(reads(&view, cx), 0, "another tab: never");

    open_tab(&view, thread, PanelTab::Pr, cx);
    settle(&view, thread, cx);
    assert_eq!(reads(&view, cx), 1, "back: read at once");
    cx.simulate_keystrokes("cmd-alt-b");
    for _ in 0..3 {
        cx.executor().advance_clock(crate::pr::AUTO_EVERY);
        tick(cx);
    }
    assert_eq!(reads(&view, cx), 0, "the panel hidden: never");
}
