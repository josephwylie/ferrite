//! `PullRequestView` in a window of its own: what it draws of a PR, what
//! it asks of the window, and how it answers the window's calls. The host
//! is the window's side: the PR the Panel shows, whether an answer is
//! still wanted, the `⋯`'s door and the stops' handles.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use ferrite_core::protocol::{
    ReviewPullRequest, WireCheck, WireMerge, WirePrComment, WireSync,
};
use ferrite_ui::testing::{click, mount, record, shows};
use gpui::{Entity, TestAppContext, VisualTestContext};

use super::*;

fn thread() -> ThreadKey {
    ThreadKey::fixture(1, 3)
}

fn target() -> PrTarget {
    PrTarget::thread(thread(), 48)
}

/// A second PR the window reads, not the one shown.
fn other() -> PrTarget {
    PrTarget::thread(thread(), 51)
}

/// PR #48, open, a failed check and one comment.
fn snapshot(state: WirePrState) -> WirePr {
    WirePr {
        number: 48,
        title: "Nav rows keep their height".into(),
        url: "https://github.com/o/r/pull/48".into(),
        body: "Live rows reserve their status line.".into(),
        author: None,
        author_avatar_url: None,
        updated_at: None,
        changed_files: None,
        additions: None,
        deletions: None,
        state,
        draft: false,
        head: "fix/nav".into(),
        base: "main".into(),
        merge: WireMerge::Clean,
        sync: WireSync::InSync,
        checks: vec![WireCheck {
            name: "fmt".into(),
            status: WireCheckStatus::Failure,
            url: Some("https://ci.example/run/9".into()),
        }],
        comments: vec![WirePrComment {
            id: None,
            author_is_bot: false,
            place: None,
            kind: WirePrCommentKind::Comment,
            author: "ada".into(),
            anchor: "comment".into(),
            verdict: None,
            body: "Looks right.".into(),
            snippet: None,
            created_at: "2026-11-10".into(),
            resolved: false,
            outdated: false,
            replies: Vec::new(),
        }],
        truncated: false,
        checks_truncated: false,
        comments_truncated: false,
    }
}

/// PR #51, open: its own title and address.
fn other_snapshot() -> WirePr {
    WirePr {
        number: 51,
        title: "Keep the bar".into(),
        url: "https://github.com/o/r/pull/51".into(),
        head: "fix/bar".into(),
        ..snapshot(WirePrState::Open)
    }
}

/// Code's body as the window hands it over: a view of its own.
struct Code;

impl Render for Code {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().debug_selector(|| "code-body".into()).size_full()
    }
}

/// The window's side of the reader.
struct Host {
    reader: Entity<PullRequestView>,
    shown: PrShown,
    /// The PRs whose answers the window still wants.
    wanted: HashSet<PrTarget>,
    /// Whether the `⋯`'s menu is up, and where its button was drawn.
    up: bool,
    hung: Rc<Cell<Option<Bounds<Pixels>>>>,
    stops: HashMap<PrStop, FocusHandle>,
}

impl Host {
    fn inputs(&self) -> PrInputs {
        let hung = self.hung.clone();
        PrInputs {
            shown: self.shown,
            reading: Some((PrTone::Open, None)),
            more: MenuDoor::new(self.up, move |at, _, _| hung.set(Some(at))),
            stops: self.stops.clone(),
        }
    }
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().flex_col().child(self.reader.clone())
    }
}

/// The reader on `target()`'s PR in `tab`, the PR of its Thread's branch,
/// the window keeping each view it chooses as the root does.
fn open(tab: PrTab, cx: &mut TestAppContext) -> (Entity<Host>, Entity<PullRequestView>, &mut VisualTestContext) {
    let (host, cx) = mount(cx, |_, cx| {
        let root = cx.entity();
        let inputs = Source::of(&root, |host: &Host, _: &(), _| Some(host.inputs()));
        let wanted = Source::of(&root, |host: &Host, target: &PrTarget, _| Some(host.wanted.contains(target)));
        let code = cx.new(|_| Code);
        let reader = cx.new(|_| PullRequestView::new(inputs, wanted, code.into()));
        cx.subscribe(&reader, |host: &mut Host, _, event, cx| {
            if let PrEvent::SelectTab(_, tab) = event {
                host.shown.tab = *tab;
                cx.notify();
            }
        })
        .detach();
        let stops = [PrStop::Number, PrStop::Primary, PrStop::More]
            .into_iter()
            .chain(PrTab::ALL.map(PrStop::View))
            .map(|stop| (stop, cx.focus_handle()))
            .collect();
        Host {
            reader,
            shown: PrShown {
                target: target(),
                tab,
                branch: true,
            },
            wanted: HashSet::from([target(), other()]),
            up: false,
            hung: Rc::default(),
            stops,
        }
    });
    cx.simulate_resize(gpui::size(px(900.), px(700.)));
    let reader = host.read_with(cx, |host, _| host.reader.clone());
    (host, reader, cx)
}

/// `pr` read, as the forge's answer would leave it.
fn read(reader: &Entity<PullRequestView>, pr: WirePr, cx: &mut VisualTestContext) {
    reader.update(cx, |reader, cx| {
        reader.install(target(), pr);
        cx.notify();
    });
    cx.run_until_parked();
}

/// A read of `target`'s PR asked for and answered with `answer`.
fn answered(
    reader: &Entity<PullRequestView>,
    target: PrTarget,
    answer: std::io::Result<ReviewPullRequest>,
    cx: &mut VisualTestContext,
) {
    let (send, receive) = std::sync::mpsc::channel();
    send.send(answer).unwrap();
    reader.update(cx, |reader, cx| {
        let request = reader.begin_read(target, true).expect("a read to wait on");
        reader.wait(target, request, Ok(receive), cx);
    });
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(64));
        cx.run_until_parked();
    }
}

/// The name each declared stop says, in order.
fn stop_names(reader: &Entity<PullRequestView>, shown: PrShown, cx: &mut VisualTestContext) -> Vec<String> {
    reader.read_with(cx, |reader, _| {
        reader
            .tab_stops(shown, Some(PrTone::Open))
            .into_iter()
            .map(|(_, semantics)| semantics.name.to_string())
            .collect()
    })
}

/// The PR's identity and its facts stand over every view.
#[gpui::test]
fn the_header_and_identity_stay_on_every_view(cx: &mut TestAppContext) {
    let (host, reader, cx) = open(PrTab::Summary, cx);
    let mut pr = snapshot(WirePrState::Open);
    pr.author = Some("ada".into());
    pr.updated_at = Some("2026-11-15T08:00:00Z".into());
    pr.changed_files = Some(3);
    pr.additions = Some(12);
    pr.deletions = Some(2);
    read(&reader, pr, cx);
    for tab in PrTab::ALL {
        click(cx, &format!("pr-tab-{}", tab.label()));
        assert_eq!(host.read_with(cx, |host, _| host.shown.tab), tab, "the window keeps {tab:?}");
        for selector in ["pr-title", "pr-author", "pr-updated", "pr-files", "pr-additions", "pr-deletions"] {
            assert!(shows(cx, selector), "{selector} missing on {tab:?}");
        }
    }
}

/// Another view fades in once; the same view again, or an update that
/// changes nothing, does not replay it.
#[gpui::test]
fn a_view_fades_in_once_when_it_changes(cx: &mut TestAppContext) {
    let (_host, reader, cx) = open(PrTab::Summary, cx);
    read(&reader, snapshot(WirePrState::Open), cx);
    let picked = |cx: &mut VisualTestContext| reader.read_with(cx, |reader, _| reader.picked());
    assert_eq!(picked(cx), None, "opening plays nothing");
    click(cx, "pr-tab-Timeline");
    let at = picked(cx);
    assert!(at.is_some(), "a new view fades in");
    click(cx, "pr-tab-Timeline");
    assert_eq!(picked(cx), at, "the same view again does not replay");
    reader.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert_eq!(picked(cx), at, "an unrelated update does not replay");
}

/// An open PR's Merge asks once more before it asks the window to hand
/// the merge to the agent.
#[gpui::test]
fn merge_arms_on_the_first_press_and_asks_on_the_second(cx: &mut TestAppContext) {
    let (host, reader, cx) = open(PrTab::Summary, cx);
    read(&reader, snapshot(WirePrState::Open), cx);
    let heard = record(cx, &reader, |event: &PrEvent| *event);
    let shown = host.read_with(cx, |host, _| host.shown);
    assert!(stop_names(&reader, shown, cx).contains(&"Merge".to_string()));
    click(cx, "pr-primary");
    assert_eq!(heard.take(), [], "armed, not asked");
    assert!(
        stop_names(&reader, shown, cx).contains(&"Confirm merge?".to_string()),
        "it says it is armed"
    );
    click(cx, "pr-primary");
    assert_eq!(heard.take(), [PrEvent::Run(target(), PrAction::MergeNow)]);
}

/// A merged PR shows its pill in Merge's place, and its `⋯` offers what a
/// merged PR can still take.
#[gpui::test]
fn a_merged_pr_shows_its_pill_and_its_menu_what_it_can_still_take(cx: &mut TestAppContext) {
    let (_host, reader, cx) = open(PrTab::Summary, cx);
    read(&reader, snapshot(WirePrState::Open), cx);
    assert!(shows(cx, "pr-primary") && !shows(cx, "pr-pill"));
    read(&reader, snapshot(WirePrState::Merged), cx);
    assert!(shows(cx, "pr-pill"), "merged: the pill");
    assert!(!shows(cx, "pr-primary"), "in the action's place");
    let rows: Vec<String> = reader.read_with(cx, |reader, _| {
        reader
            .menu_rows(target(), true)
            .into_iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect()
    });
    for row in ["Explain this PR", "Revert changes", "Delete branch", "Copy link"] {
        assert!(rows.contains(&row.to_string()), "{row} in {rows:?}");
    }
    assert!(!rows.contains(&"Merge now".to_string()));
}

/// A view chosen and the `⋯` pressed are asked of the window, each for its
/// PR; the `⋯` hangs its menu from where it was drawn.
#[gpui::test]
fn the_views_and_the_more_button_ask_the_window(cx: &mut TestAppContext) {
    let (host, reader, cx) = open(PrTab::Summary, cx);
    read(&reader, snapshot(WirePrState::Open), cx);
    let heard = record(cx, &reader, |event: &PrEvent| *event);
    click(cx, "pr-tab-Guide");
    assert_eq!(heard.take(), [PrEvent::SelectTab(target(), PrTab::Guide)]);
    click(cx, "pr-more");
    assert_eq!(heard.take(), [PrEvent::More(target())]);
    host.update(cx, |host, cx| {
        host.up = true;
        cx.notify();
    });
    cx.run_until_parked();
    let at = host.read_with(cx, |host, _| host.hung.get()).expect("the door records where it is");
    let drawn = cx.update(|window, _| window.debug_bounds("pr-more")).unwrap();
    assert_eq!(at, drawn, "its own bounds");
}

/// Code's diff is asked for as the pointer comes near Code, before the
/// press, so the view is read by the time it is chosen.
#[gpui::test]
fn code_is_asked_for_as_the_pointer_comes_near(cx: &mut TestAppContext) {
    let (_host, reader, cx) = open(PrTab::Summary, cx);
    read(&reader, snapshot(WirePrState::Open), cx);
    let heard = record(cx, &reader, |event: &PrEvent| *event);
    let guide = cx.update(|window, _| window.debug_bounds("pr-tab-Guide")).unwrap();
    cx.simulate_mouse_move(guide.center(), None, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(heard.take(), [], "near Guide, not Code");
    let code = cx.update(|window, _| window.debug_bounds("pr-tab-Code")).unwrap();
    cx.simulate_mouse_move(code.center(), None, gpui::Modifiers::none());
    cx.run_until_parked();
    assert_eq!(heard.take(), [PrEvent::EnsureCode(target())]);
}

/// A PR of the Thread's branch with no guide offers Write guide on Guide,
/// which asks the window to have the agent write one.
#[gpui::test]
fn write_guide_asks_the_window_for_the_agents_guide(cx: &mut TestAppContext) {
    let (_host, reader, cx) = open(PrTab::Guide, cx);
    read(&reader, snapshot(WirePrState::Open), cx);
    let heard = record(cx, &reader, |event: &PrEvent| *event);
    click(cx, "pr-write-guide");
    assert_eq!(heard.take(), [PrEvent::Run(target(), PrAction::WriteGuide)]);
}

/// A read that fails says why, with Retry, which asks the window again.
#[gpui::test]
fn a_failed_read_offers_retry_which_asks_again(cx: &mut TestAppContext) {
    let (_host, reader, cx) = open(PrTab::Summary, cx);
    answered(&reader, target(), Err(std::io::Error::other("gh is not installed")), cx);
    assert!(shows(cx, "pr-failed"), "what went wrong");
    let heard = record(cx, &reader, |event: &PrEvent| *event);
    click(cx, "pr-retry");
    assert_eq!(heard.take(), [PrEvent::Read(target())]);
}

/// An answer the window no longer wants (its tab closed meanwhile) is let
/// go: it neither shows nor fails the PR. Another PR's, still wanted,
/// lands.
#[gpui::test]
fn an_answer_no_longer_wanted_is_let_go(cx: &mut TestAppContext) {
    let (host, reader, cx) = open(PrTab::Summary, cx);
    host.update(cx, |host, _| host.wanted.remove(&target()));
    let answer = |pr| ReviewPullRequest {
        noun: "PR".into(),
        pr: Ok(pr),
        unread: None,
    };
    answered(&reader, target(), Ok(answer(snapshot(WirePrState::Open))), cx);
    answered(&reader, other(), Ok(answer(other_snapshot())), cx);
    let held = |target, cx: &mut VisualTestContext| {
        reader.read_with(cx, |reader, _| reader.snapshot(target).map(|pr| pr.url.clone()))
    };
    assert_eq!(held(target(), cx), None, "let go");
    assert_eq!(held(other(), cx).as_deref(), Some("https://github.com/o/r/pull/51"), "still wanted");
    assert!(!shows(cx, "pr-body") && !shows(cx, "pr-failed"));
}

/// Code's body is the window's view, drawn only while Code is shown.
#[gpui::test]
fn code_is_the_windows_view_drawn_only_on_code(cx: &mut TestAppContext) {
    let (_host, reader, cx) = open(PrTab::Summary, cx);
    read(&reader, snapshot(WirePrState::Open), cx);
    assert!(shows(cx, "pr-body") && !shows(cx, "code-body"));
    click(cx, "pr-tab-Code");
    assert!(shows(cx, "code-body"), "the window's view");
    assert!(!shows(cx, "pr-body"));
}

/// The reader's controls, declared as stops in the order it draws them:
/// the primary action only on an open PR of the Thread's branch.
#[gpui::test]
fn its_stops_are_declared_in_the_order_drawn(cx: &mut TestAppContext) {
    let (host, reader, cx) = open(PrTab::Timeline, cx);
    read(&reader, snapshot(WirePrState::Open), cx);
    let shown = host.read_with(cx, |host, _| host.shown);
    let names = stop_names(&reader, shown, cx);
    assert_eq!(
        names,
        [
            "Pull request #48, open on GitHub",
            "Merge",
            "More pull request actions",
            "Summary",
            "Guide",
            "Timeline",
            "Code"
        ]
    );
    let selected: Vec<_> = reader.read_with(cx, |reader, _| {
        reader
            .tab_stops(shown, None)
            .into_iter()
            .filter_map(|(_, semantics)| semantics.selected.filter(|on| *on).map(|_| semantics.name))
            .collect()
    });
    assert_eq!(selected, ["Timeline"], "the view shown is the one selected");
    let elsewhere = PrShown { branch: false, ..shown };
    assert!(!stop_names(&reader, elsewhere, cx).contains(&"Merge".to_string()), "not its branch's");
    read(&reader, snapshot(WirePrState::Merged), cx);
    assert!(!stop_names(&reader, shown, cx).contains(&"Merge".to_string()), "settled");
}

#[test]
fn the_newest_guide_wins_among_a_newest_first_read() {
    let said = |body: &str| WirePrComment {
        body: body.into(),
        ..snapshot(WirePrState::Open).comments[0].clone()
    };
    let pr = WirePr {
        comments: vec![
            said("## Review guide\nnewest"),
            said("thanks"),
            said("## Review guide\noldest"),
        ],
        ..snapshot(WirePrState::Open)
    };
    assert_eq!(guide(&pr), Some("## Review guide\nnewest"));
}

#[test]
fn the_repo_comes_from_the_prs_address() {
    assert_eq!(repo_of("https://github.com/o/r/pull/48").as_deref(), Some("o/r"));
    assert_eq!(repo_of("not a url"), None);
}

#[test]
fn each_state_offers_only_what_applies() {
    let offered = |tone| -> Vec<PrAction> {
        MENU.iter()
            .flat_map(|group| group.iter().copied())
            .filter(|action| action.applies(tone))
            .collect()
    };
    assert!(offered(PrTone::Open).contains(&PrAction::MergeNow));
    assert!(!offered(PrTone::Open).contains(&PrAction::Revert));
    assert!(offered(PrTone::Draft).contains(&PrAction::ReadyForReview));
    assert!(!offered(PrTone::Draft).contains(&PrAction::MergeNow));
    let merged = offered(PrTone::Merged);
    assert!(merged.contains(&PrAction::Revert) && merged.contains(&PrAction::DeleteBranch));
    assert!(!merged.contains(&PrAction::Close));
}
