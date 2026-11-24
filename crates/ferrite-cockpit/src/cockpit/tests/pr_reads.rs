//! A PR tab shows its own PR's last good content, however the forge's
//! answers come: late, out of order, failed, or for a tab closed since.
use super::pr::snapshot;
use super::*;
use crate::cockpit::surfaces::SurfaceId;
use ferrite_client::cockpit::HeldRead;
use ferrite_core::protocol::{ReviewPullRequest, WirePr, WirePrState, WirePrUnread};
use ferrite_reviewr::forge::PrView;

fn pr_at(number: u64, repo: &str, title: &str) -> WirePr {
    WirePr {
        number,
        title: title.into(),
        url: format!("https://github.com/{repo}/pull/{number}"),
        ..snapshot(WirePrState::Open)
    }
}

fn read_of(pr: WirePr) -> ReviewPullRequest {
    ReviewPullRequest {
        noun: "PR".into(),
        pr: Ok(pr),
        unread: None,
    }
}

fn unread(note: &str, why: Option<WirePrUnread>) -> ReviewPullRequest {
    ReviewPullRequest {
        noun: "PR".into(),
        pr: Err(note.into()),
        unread: why,
    }
}

fn reading_of(number: u64) -> ferrite_core::pull_request::PullRequest {
    ferrite_core::pull_request::PullRequest {
        number,
        ..pr_with_checks()
    }
}

/// `count` Threads, each on a PR numbered 48 (a different repository's),
/// whose whole-PR reads wait for the test.
fn held<'a>(
    name: &str,
    count: usize,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    Vec<ThreadKey>,
    &'a mut gpui::VisualTestContext,
) {
    let (core, _fake) = cockpit(name, count);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let threads = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads());
    view.update(cx, |view, cx| {
        let path = view.thread_path(threads[0], cx).unwrap();
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_forge(path.clone(), PrView::NoPr));
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_hold_pr_reads(path));
        for thread in &threads {
            view.pr_landed(*thread, Some(reading_of(48)), cx);
        }
    });
    tick(cx);
    (view, threads, cx)
}

fn open(view: &Entity<CockpitView>, thread: ThreadKey, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| {
        view.focus_thread(thread, cx);
        view.open_pr_surface(thread, cx);
    });
    tick(cx);
}

fn waiting(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<HeldRead> {
    view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| cockpit.take_held_reads()))
}

fn title(
    view: &Entity<CockpitView>,
    thread: ThreadKey,
    cx: &mut gpui::VisualTestContext,
) -> Option<String> {
    view.read_with(cx, |view, cx| {
        view.pr_snapshot(thread, cx).map(|pr| pr.title.clone())
    })
}

fn click(selector: &'static str, cx: &mut gpui::VisualTestContext) {
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
}

fn tab_open(
    view: &Entity<CockpitView>,
    thread: ThreadKey,
    number: u64,
    cx: &mut gpui::VisualTestContext,
) -> bool {
    view.read_with(cx, |view, _| {
        view.panel
            .surfaces
            .get(thread)
            .is_some_and(|state| state.tabs.contains(&SurfaceId::PullRequest(number)))
    })
}

/// Two repositories' PR #48 each show their own content, though the
/// forge answers the second Thread's read first.
#[gpui::test]
fn two_repositories_with_one_pr_number_each_show_their_own(cx: &mut TestAppContext) {
    let (view, [a, b], cx) = {
        let (view, threads, cx) = held("pr-reads-two-repos", 2, cx);
        (view, [threads[0], threads[1]], cx)
    };
    open(&view, a, cx);
    open(&view, b, cx);
    let mut reads = waiting(&view, cx);
    assert_eq!(reads.len(), 2, "one read per tab");
    let (b_read, a_read) = (reads.pop().unwrap(), reads.pop().unwrap());

    b_read.answer(read_of(pr_at(48, "o/repo-b", "B's change")));
    a_read.answer(read_of(pr_at(48, "o/repo-a", "A's change")));
    tick(cx);

    assert_eq!(title(&view, a, cx).as_deref(), Some("A's change"));
    assert_eq!(title(&view, b, cx).as_deref(), Some("B's change"));
}

/// Opening the tab again asks afresh, and only that newest ask's answer
/// lands: the earlier one, answering last, is dropped.
#[gpui::test]
fn opening_the_tab_again_supersedes_the_read_in_flight(cx: &mut TestAppContext) {
    let (view, threads, cx) = held("pr-reads-reopen", 1, cx);
    let thread = threads[0];
    open(&view, thread, cx);
    open(&view, thread, cx);
    let mut asks = waiting(&view, cx);
    assert_eq!(asks.len(), 2);
    let (newest, earlier) = (asks.pop().unwrap(), asks.pop().unwrap());
    newest.answer(read_of(pr_at(48, "o/r", "Newest")));
    earlier.answer(read_of(pr_at(48, "o/r", "Earlier")));
    tick(cx);
    assert_eq!(title(&view, thread, cx).as_deref(), Some("Newest"));
}

/// The PR moves under a read in flight (the branch merged, the read is
/// asked again), and the old read answers last: the newer answer stays.
#[gpui::test]
fn an_older_answer_never_replaces_a_newer_one(cx: &mut TestAppContext) {
    let (view, threads, cx) = held("pr-reads-out-of-order", 1, cx);
    let thread = threads[0];
    open(&view, thread, cx);
    let old = waiting(&view, cx).pop().unwrap();

    view.update(cx, |view, cx| {
        let merged = ferrite_core::pull_request::PullRequest {
            state: ferrite_core::pull_request::PrState::Merged,
            ..reading_of(48)
        };
        view.pr_landed(thread, Some(merged), cx);
    });
    let new = waiting(&view, cx)
        .pop()
        .expect("asked again for the merged PR");

    new.answer(read_of(pr_at(48, "o/r", "After the merge")));
    old.answer(read_of(pr_at(48, "o/r", "Before the merge")));
    tick(cx);

    assert_eq!(title(&view, thread, cx).as_deref(), Some("After the merge"));
}

/// A branch change preserves explicit PR tabs and a late answer leaves the selected tab alone.
#[gpui::test]
fn a_pr_that_moved_on_mid_read_does_not_come_back(cx: &mut TestAppContext) {
    let (view, threads, cx) = held("pr-reads-moved", 1, cx);
    let thread = threads[0];
    open(&view, thread, cx);
    let late = waiting(&view, cx).pop().unwrap();

    view.update(cx, |view, cx| {
        view.pr_landed(thread, Some(reading_of(49)), cx);
        view.open_pr_surface(thread, cx);
    });
    tick(cx);
    assert!(
        tab_open(&view, thread, 48, cx),
        "an explicitly opened PR remains available"
    );
    assert!(tab_open(&view, thread, 49, cx));
    let shown = view.read_with(cx, |view, _| view.active_surface(thread));

    late.answer(read_of(pr_at(48, "o/r", "The old PR")));
    tick(cx);

    assert!(
        tab_open(&view, thread, 48, cx),
        "the prior PR remains available"
    );
    assert_eq!(
        view.read_with(cx, |view, _| view.active_surface(thread)),
        shown
    );
    assert_eq!(title(&view, thread, cx), None, "#49 has not been read");
}

/// A closed tab's read answers into nothing: the tab stays closed, and
/// opening it again reads afresh.
#[gpui::test]
fn closing_the_tab_drops_a_late_answer(cx: &mut TestAppContext) {
    let (view, threads, cx) = held("pr-reads-closed", 1, cx);
    let thread = threads[0];
    open(&view, thread, cx);
    let late = waiting(&view, cx).pop().unwrap();
    view.update(cx, |view, cx| {
        view.close_surface(thread, &SurfaceId::PullRequest(48), cx)
    });
    tick(cx);

    late.answer(read_of(pr_at(48, "o/r", "Late")));
    tick(cx);
    assert!(!tab_open(&view, thread, 48, cx));
    assert_eq!(title(&view, thread, cx), None);

    open(&view, thread, cx);
    assert_eq!(waiting(&view, cx).len(), 1, "reading afresh");
}

/// A refresh that fails leaves the last good content in view, says why and
/// how to recover, and Retry asks the forge again; the answer replaces
/// the content and the notice goes.
#[gpui::test]
fn a_failed_refresh_keeps_the_content_and_retry_asks_again(cx: &mut TestAppContext) {
    let (view, threads, cx) = held("pr-reads-retry", 1, cx);
    let thread = threads[0];
    open(&view, thread, cx);
    waiting(&view, cx)
        .pop()
        .unwrap()
        .answer(read_of(pr_at(48, "o/r", "First read")));
    tick(cx);
    assert!(cx.debug_bounds("pr-failed").is_none());

    view.update(cx, |view, cx| view.refresh_pr_tab(thread, cx));
    tick(cx);
    assert!(
        cx.debug_bounds("pr-refreshing").is_some(),
        "the refresh shows it is out"
    );
    assert_eq!(
        title(&view, thread, cx).as_deref(),
        Some("First read"),
        "content stays"
    );
    waiting(&view, cx).pop().unwrap().answer(unread(
        "GitHub rate limit reached: API rate limit exceeded. Wait a few minutes, then retry.",
        Some(WirePrUnread::RateLimited),
    ));
    tick(cx);
    assert!(cx.debug_bounds("pr-refreshing").is_none());
    assert!(
        cx.debug_bounds("pr-failed").is_some(),
        "says what went wrong"
    );
    assert_eq!(
        title(&view, thread, cx).as_deref(),
        Some("First read"),
        "content stays"
    );
    assert!(cx.debug_bounds("pr-summary").is_some(), "and is drawn");

    click("pr-retry", cx);
    assert!(
        cx.debug_bounds("pr-failed").is_none(),
        "the retry clears the notice"
    );
    assert!(cx.debug_bounds("pr-refreshing").is_some());
    let retry = waiting(&view, cx);
    assert_eq!(retry.len(), 1, "a fresh request");
    retry
        .into_iter()
        .next()
        .unwrap()
        .answer(read_of(pr_at(48, "o/r", "Second read")));
    tick(cx);
    assert_eq!(title(&view, thread, cx).as_deref(), Some("Second read"));
    assert!(cx.debug_bounds("pr-failed").is_none());
    assert!(cx.debug_bounds("pr-refreshing").is_none());
}

/// With nothing read yet, the failure and Retry take the body's place.
#[gpui::test]
fn a_first_read_that_fails_offers_retry(cx: &mut TestAppContext) {
    let (view, threads, cx) = held("pr-reads-first-fail", 1, cx);
    let thread = threads[0];
    open(&view, thread, cx);
    waiting(&view, cx).pop().unwrap().fail("the read ended");
    tick(cx);
    assert!(cx.debug_bounds("pr-failed").is_some());
    click("pr-retry", cx);
    assert_eq!(waiting(&view, cx).len(), 1);
}

/// Refresh PR status reads an open PR tab again, so a check or comment
/// that changed shows with no change to the PR's number or state.
#[gpui::test]
fn refresh_pr_status_rereads_the_open_tab(cx: &mut TestAppContext) {
    use crate::palette::{Act, Command};
    let (view, threads, cx) = held("pr-reads-refresh", 1, cx);
    let thread = threads[0];
    open(&view, thread, cx);
    waiting(&view, cx)
        .pop()
        .unwrap()
        .answer(read_of(pr_at(48, "o/r", "Before")));
    tick(cx);

    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_act(Act::Command(Command::RefreshPr), window, cx)
        })
    });
    let mut changed = pr_at(48, "o/r", "Before");
    changed.checks[0].status = ferrite_core::protocol::WireCheckStatus::Success;
    changed.comments.clear();
    waiting(&view, cx)
        .pop()
        .expect("the tab was read again")
        .answer(read_of(changed));
    tick(cx);

    let (checks, comments) = view.read_with(cx, |view, cx| {
        let pr = view.pr_snapshot(thread, cx).unwrap();
        (pr.checks[0].status, pr.comments.len())
    });
    assert_eq!(checks, ferrite_core::protocol::WireCheckStatus::Success);
    assert_eq!(comments, 0);
}

#[gpui::test]
fn a_wrong_number_never_replaces_the_requested_pr(cx: &mut TestAppContext) {
    let (view, threads, cx) = held("pr-reads-wrong-number", 1, cx);
    let thread = threads[0];
    open(&view, thread, cx);
    let read = waiting(&view, cx).pop().unwrap();
    read.answer(read_of(pr_at(99, "o/r", "Another pull request")));
    until(cx, |cx| cx.debug_bounds("pr-failed").is_some());
    assert!(tab_open(&view, thread, 48, cx));
    assert!(cx.debug_bounds("pr-body").is_none());
    assert!(view.read_with(cx, |view, cx| view.pr_snapshot(thread, cx).is_none()));
}
