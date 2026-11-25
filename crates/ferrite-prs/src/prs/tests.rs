//! `PrsView` in a window of its own, with the window's PR reader as its
//! child: what it lists, what its detail draws, what it asks of the window
//! and how it walks by keys. The host is the window's side: the Projects
//! it offers, what the forge lists for each, and the PR each detail reads.
//! The window's part (opening the destination, reading the forge, the
//! board back, the Project card) is the Cockpit's to test.

use std::collections::HashMap;
use std::time::Duration;

use ferrite_core::project::ProjectId;
use ferrite_core::protocol::{
    WireCheck, WireCheckStatus, WireMerge, WirePr, WirePrComment,
    WirePrCommentKind, WirePrState, WireReply, WireReviewState, WireSync,
};
use ferrite_ui::testing::{click, mount, record, shows};
use gpui::{
    DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate, TestAppContext,
    VisualTestContext,
};

use super::*;
use crate::pull_request::PrEvent;

fn project() -> ProjectKey {
    ProjectKey::fixture(1, ProjectId(4))
}

fn other() -> ProjectKey {
    ProjectKey::fixture(1, ProjectId(5))
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

fn two_rows() -> Vec<WirePrRow> {
    vec![
        row(7, "Keep rows tall", WirePrState::Open),
        row(3, "Older change", WirePrState::Merged),
    ]
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
/// thread with a nested reply and its code context, a plain comment, two
/// reviews' verdicts, and a conversation the read capped.
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
        merge: WireMerge::Clean,
        sync: WireSync::Unknown,
        checks: vec![WireCheck {
            name: "fmt".into(),
            status: WireCheckStatus::Failure,
            url: Some("https://ci.example/run/9".into()),
        }],
        comments: vec![
            WirePrComment {
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

/// PR #3, merged: its own title, address and description.
fn older_pr() -> WirePr {
    WirePr {
        number: 3,
        title: "Older change".into(),
        url: "https://github.com/o/r/pull/3".into(),
        body: "An older change.".into(),
        state: WirePrState::Merged,
        head: "feature/3".into(),
        checks: Vec::new(),
        comments: Vec::new(),
        comments_truncated: false,
        ..full_pr()
    }
}

/// Code's body as the window hands it over.
struct Code;

impl Render for Code {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().debug_selector(|| "code-body".into()).size_full()
    }
}

/// The window's side of the PRs.
struct Host {
    prs: Entity<PrsView>,
    projects: Vec<(ProjectKey, SharedString)>,
    /// What the forge lists, by Project and state.
    lists: HashMap<(ProjectKey, WirePrFilter), ReviewPullRequests>,
    /// What each PR's detail reads, by number.
    pulls: HashMap<u64, WirePr>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().flex_col().child(self.prs.clone())
    }
}

/// The production key table, as far as the actions linked here go.
fn bind_keys(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let table = ferrite_ui::keymap::bindings(ferrite_ui::keymap::Platform::Mac);
        let bindings = table
            .into_iter()
            .filter_map(|(keystroke, action, context)| {
                let action = cx.build_action(action, None).ok()?;
                let context = context.map(|context| KeyBindingContextPredicate::parse(context).unwrap().into());
                Some(KeyBinding::load(&keystroke, action, context, false, None, &DummyKeyboardMapper).unwrap())
            })
            .collect::<Vec<_>>();
        cx.bind_keys(bindings);
    });
}

/// The PRs, scoped to nothing yet, over `project()` and `other()` whose
/// lists are `rows` in every state, answering each ask as the window does.
fn open(rows: Vec<WirePrRow>, more: bool, cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
    bind_keys(cx);
    let (host, cx) = mount(cx, |_, cx| {
        let root = cx.entity();
        let inputs = Source::of(&root, |host: &Host, _: &(), _| {
            Some(PrsInputs {
                projects: host.projects.clone(),
                stops: HashMap::new(),
            })
        });
        let none = Source::of(&root, |_: &Host, _: &(), _| None);
        let wanted = Source::of(&root, |_: &Host, _: &PrTarget, _| Some(true));
        let reader_code = cx.new(|_| Code);
        let reader = cx.new(|_| PullRequestView::new(none, wanted, reader_code.into()));
        let code = cx.new(|_| Code);
        let prs = cx.new(|cx| PrsView::new(inputs, reader.clone(), code.into(), cx));
        cx.subscribe(&prs, |host: &mut Host, prs, event, cx| match *event {
            PrsEvent::Read(project, filter) => {
                let (send, answer) = std::sync::mpsc::channel();
                if let Some(list) = host.lists.get(&(project, filter)) {
                    send.send(Ok(list.clone())).unwrap();
                }
                prs.update(cx, |prs, cx| prs.reading((project, filter), Ok(answer), cx));
            }
            PrsEvent::Opened(target) => {
                if let Some(pr) = host.pulls.get(&target.number).cloned() {
                    prs.read(cx).reader.clone().update(cx, |reader, cx| {
                        reader.install(target, pr);
                        cx.notify();
                    });
                }
            }
            PrsEvent::Leave | PrsEvent::AddProject => {}
        })
        .detach();
        let shown = prs.clone();
        cx.subscribe(&reader, move |_, _, event, cx| {
            if let PrEvent::SelectTab(_, tab) = *event {
                shown.update(cx, |prs, cx| prs.set_tab(tab, cx));
            }
        })
        .detach();
        let lists = [project(), other()]
            .into_iter()
            .flat_map(|key| {
                FILTERS.map(|(filter, _)| ((key, filter), list_of(rows.clone(), more)))
            })
            .collect();
        Host {
            prs,
            projects: vec![(project(), "ferrite".into()), (other(), "other".into())],
            lists,
            pulls: HashMap::from([(7, full_pr()), (3, older_pr())]),
        }
    });
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    (host, cx)
}

/// The PRs shown on `project`, holding the keyboard as the window has them.
fn show(host: &Entity<Host>, project: Option<ProjectKey>, cx: &mut VisualTestContext) {
    let prs = host.read_with(cx, |host, _| host.prs.clone());
    prs.update(cx, |prs, cx| prs.show(project, cx));
    cx.update(|window, cx| {
        let focus = prs.read(cx).focus().clone();
        window.focus(&focus, cx);
    });
    settle(cx);
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(64));
        cx.run_until_parked();
    }
}

fn prs(host: &Entity<Host>, cx: &mut VisualTestContext) -> Entity<PrsView> {
    host.read_with(cx, |host, _| host.prs.clone())
}

/// A listed PR pressed shows whole: its identity over the views, Code
/// with no checkout, no actions (they are a Thread's), and on Timeline
/// each review thread's code context, resolution and replies.
#[gpui::test]
fn a_listed_pr_shows_whole_with_its_review_threads(cx: &mut TestAppContext) {
    let (host, cx) = open(two_rows(), false, cx);
    show(&host, Some(project()), cx);
    click(cx, "prs-row-7");
    assert!(shows(cx, "prs-detail"));
    assert!(shows(cx, "pr-body"));
    let title = cx.update(|window, _| window.debug_bounds("pr-title")).unwrap();
    let views = cx.update(|window, _| window.debug_bounds("pr-views")).unwrap();
    assert!(title.bottom() <= views.top(), "PR identity stays above the views");
    assert!(shows(cx, "pr-tab-Code"), "published Code without a checkout's branch");
    assert!(!shows(cx, "pr-primary"), "no actions: they are a Thread's");

    click(cx, "pr-tab-Timeline");
    assert!(shows(cx, "pr-title"), "PR identity remains on Timeline");
    assert!(shows(cx, "pr-snippet-0"), "the thread's code context");
    assert!(shows(cx, "pr-resolved-0"), "resolved");
    assert!(!shows(cx, "pr-resolved-1"));
    assert!(shows(cx, "pr-reply-0-0"), "its nested reply");
    assert!(shows(cx, "pr-more-comments"), "the cap, with the way past it");
    click(cx, "pr-tab-Code");
    assert!(shows(cx, "code-body"), "Code's body is the window's view");
}

/// Each review's verdict stands, with or without words, and a failed
/// check opens its run.
#[gpui::test]
fn a_pr_shows_each_review_verdict_and_its_failed_checks_run(cx: &mut TestAppContext) {
    let (host, cx) = open(two_rows(), false, cx);
    show(&host, Some(project()), cx);
    click(cx, "prs-row-7");
    click(cx, "pr-tab-Timeline");
    assert!(shows(cx, "pr-verdict-2") && shows(cx, "pr-comment-2"), "the approval stands with no body");
    assert!(shows(cx, "pr-verdict-3"), "changes requested, with its words");
    assert!(!shows(cx, "pr-verdict-0"), "a finding has no verdict");
    click(cx, "pr-tab-Summary");
    assert_eq!(cx.opened_url(), None);
    click(cx, "pr-check-0");
    assert_eq!(cx.opened_url().as_deref(), Some("https://ci.example/run/9"));
}

/// A merged PR shows its state, and nothing to merge.
#[gpui::test]
fn a_merged_pr_shows_its_state_and_no_merge_line(cx: &mut TestAppContext) {
    let (host, cx) = open(two_rows(), false, cx);
    host.update(cx, |host, _| host.pulls.get_mut(&7).unwrap().state = WirePrState::Merged);
    show(&host, Some(project()), cx);
    click(cx, "prs-row-7");
    assert!(shows(cx, "pr-body"));
    assert!(shows(cx, "pr-pill"), "its state");
    assert!(!shows(cx, "pr-merge"), "merged: nothing to merge");
}

/// Leaving a PR's detail returns to the list as it was: its filter, its
/// marked row, and where it was scrolled.
#[gpui::test]
fn back_from_a_pr_returns_to_the_list_as_it_was_left(cx: &mut TestAppContext) {
    let rows: Vec<WirePrRow> = (1..=40)
        .map(|n| row(n + 100, &format!("Change {n}"), WirePrState::Open))
        .chain([row(7, "Keep rows tall", WirePrState::Open)])
        .collect();
    let (host, cx) = open(rows, false, cx);
    show(&host, Some(project()), cx);
    click(cx, "prs-filter-All");
    settle(cx);
    let prs = prs(&host, cx);
    prs.update(cx, |prs, cx| {
        prs.scroll().set_offset(gpui::point(px(0.), px(-300.)));
        cx.notify();
    });
    cx.run_until_parked();
    let at = prs.read_with(cx, |prs, _| prs.scroll().offset());
    assert!(at.y < px(0.), "scrolled");
    prs.update(cx, |prs, cx| prs.open_detail(7, cx));
    cx.run_until_parked();
    assert!(shows(cx, "prs-detail") && !shows(cx, "prs-rows"));
    click(cx, "prs-back");
    assert!(shows(cx, "prs-rows"), "the list again");
    prs.read_with(cx, |prs, _| {
        assert_eq!(prs.filter(), WirePrFilter::All, "its filter");
        assert_eq!(prs.selected(), Some(7), "its marked row");
        assert_eq!(prs.scroll().offset(), at, "its scroll");
    });
}

/// A list the cap held back says so, with the way to the rest; one the
/// cap did not touch says nothing.
#[gpui::test]
fn a_capped_list_says_so(cx: &mut TestAppContext) {
    let (host, cx) = open(two_rows(), true, cx);
    show(&host, Some(project()), cx);
    assert!(shows(cx, "prs-more"));
    host.update(cx, |host, _| {
        host.lists.insert((project(), WirePrFilter::Closed), list_of(two_rows(), false));
    });
    click(cx, "prs-filter-Closed");
    settle(cx);
    assert!(shows(cx, "prs-rows") && !shows(cx, "prs-more"));
}

/// A list that cannot be read says why and offers Retry, which asks the
/// window to read it again.
#[gpui::test]
fn a_failed_list_offers_retry(cx: &mut TestAppContext) {
    let (host, cx) = open(two_rows(), false, cx);
    let failed = ReviewPullRequests {
        list: Err("Rate limit reached".into()),
        unread: Some(WirePrUnread::RateLimited),
    };
    host.update(cx, |host, _| {
        host.lists.insert((project(), WirePrFilter::Open), failed);
    });
    show(&host, Some(project()), cx);
    assert!(shows(cx, "prs-failed") && !shows(cx, "prs-row-7"));
    host.update(cx, |host, _| {
        host.lists.insert((project(), WirePrFilter::Open), list_of(two_rows(), false));
    });
    let prs = prs(&host, cx);
    let heard = record(cx, &prs, |event: &PrsEvent| format!("{event:?}"));
    click(cx, "prs-retry");
    settle(cx);
    assert_eq!(heard.take(), [format!("{:?}", PrsEvent::Read(project(), WirePrFilter::Open))]);
    assert!(shows(cx, "prs-row-7") && !shows(cx, "prs-failed"));
}

/// The list and the detail answer the keyboard alone: ↑ ↓ mark a row, ⏎
/// opens it, esc goes back to the list, then asks the window for the
/// board.
#[gpui::test]
fn the_prs_are_walked_by_keys(cx: &mut TestAppContext) {
    let (host, cx) = open(two_rows(), false, cx);
    show(&host, Some(project()), cx);
    let prs = prs(&host, cx);
    let heard = record(cx, &prs, |event: &PrsEvent| format!("{event:?}"));
    let marked = |cx: &mut VisualTestContext| prs.read_with(cx, |prs, _| prs.selected());
    cx.simulate_keystrokes("down");
    assert_eq!(marked(cx), Some(7), "the first row");
    cx.simulate_keystrokes("down");
    assert_eq!(marked(cx), Some(3));
    cx.simulate_keystrokes("down");
    assert_eq!(marked(cx), Some(3), "held at the last");
    cx.simulate_keystrokes("up");
    assert_eq!(marked(cx), Some(7));
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(shows(cx, "prs-detail"));
    assert_eq!(heard.take(), [format!("{:?}", PrsEvent::Opened(PrTarget::project(project(), 3)))]);
    click(cx, "prs-open-external");
    assert_eq!(cx.opened_url().as_deref(), Some("https://github.com/o/r/pull/3"), "the row opened, read as itself");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(shows(cx, "prs-rows"), "esc goes back to the list");
    assert_eq!(marked(cx), Some(3), "on the row it left");
    assert_eq!(heard.take(), Vec::<String>::new(), "the board stays");
    cx.simulate_keystrokes("escape");
    assert_eq!(heard.take(), [format!("{:?}", PrsEvent::Leave)], "esc again asks for the board");
}

/// Another Project chosen reads its list, and the mark on the last one
/// goes with it.
#[gpui::test]
fn choosing_another_project_reads_its_list_and_lets_the_mark_go(cx: &mut TestAppContext) {
    let (host, cx) = open(two_rows(), false, cx);
    show(&host, Some(project()), cx);
    let prs = prs(&host, cx);
    cx.simulate_keystrokes("down");
    assert_eq!(prs.read_with(cx, |prs, _| prs.selected()), Some(7));
    let heard = record(cx, &prs, |event: &PrsEvent| format!("{event:?}"));
    click(cx, &format!("prs-project-{}-{}", other().machine().get(), other().project().get()));
    settle(cx);
    assert_eq!(heard.take(), [format!("{:?}", PrsEvent::Read(other(), WirePrFilter::Open))]);
    assert_eq!(prs.read_with(cx, |prs, _| prs.project()), Some(other()));
    assert!(shows(cx, "prs-row-7"));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(!shows(cx, "prs-detail"), "no row marked on the new list");
    cx.simulate_keystrokes("down");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(shows(cx, "prs-detail"), "a row marked again opens");
}

/// A state filter reads that state's list, and an empty one says so.
#[gpui::test]
fn a_filter_reads_its_own_list(cx: &mut TestAppContext) {
    let (host, cx) = open(two_rows(), false, cx);
    host.update(cx, |host, _| {
        host.lists.insert((project(), WirePrFilter::Closed), list_of(Vec::new(), false));
    });
    show(&host, Some(project()), cx);
    assert!(shows(cx, "prs-row-7"));
    click(cx, "prs-filter-Closed");
    settle(cx);
    assert!(shows(cx, "prs-empty") && !shows(cx, "prs-row-7"));
    click(cx, "prs-filter-Open");
    settle(cx);
    assert!(shows(cx, "prs-row-7"));
}

/// With no Project, the view says so and asks the window for one.
#[gpui::test]
fn with_no_project_the_view_asks_to_add_one(cx: &mut TestAppContext) {
    let (host, cx) = open(two_rows(), false, cx);
    host.update(cx, |host, _| host.projects.clear());
    show(&host, None, cx);
    assert!(shows(cx, "prs-no-project"));
    let prs = prs(&host, cx);
    let heard = record(cx, &prs, |event: &PrsEvent| format!("{event:?}"));
    click(cx, "prs-add-project");
    assert_eq!(heard.take(), [format!("{:?}", PrsEvent::AddProject)]);
}
