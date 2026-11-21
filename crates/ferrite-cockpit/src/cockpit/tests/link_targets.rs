//! Every door to a link opens the one destination it names: a PR badge in
//! the nav (press, ⌘-press, right-click, Copy), and a link in an answer.
use super::*;
use crate::cockpit::surfaces::SurfaceId;

const REPO_A: &str = "https://github.com/o/repo-a/pull/101";
const REPO_B: &str = "https://github.com/o/repo-b/pull/202";

fn pr(number: u64, url: Option<&str>) -> ferrite_core::workspace::PullRequest {
    ferrite_core::workspace::PullRequest {
        number,
        url: url.map(str::to_string),
        ..pr_with_checks()
    }
}

/// Two Threads on two forges' PRs, A shown with its panel hidden, B in the
/// nav: the audit's cross-thread fixture.
fn two_prs<'a>(
    name: &str,
    b: Option<&str>,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, [ThreadId; 2], &'a mut gpui::VisualTestContext) {
    let (core, _fake) = cockpit(name, 2);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let threads = view.read_with(cx, |view, cx| view.cockpit.read(cx).threads());
    let (a, b_thread) = (threads[0], threads[1]);
    view.update(cx, |view, cx| {
        view.pr_landed(a, Some(pr(101, Some(REPO_A))), cx);
        view.pr_landed(b_thread, Some(pr(202, b)), cx);
        let mut snapshot = super::pr::snapshot(ferrite_core::protocol::WirePrState::Open);
        snapshot.number = 202;
        snapshot.url = REPO_B.into();
        view.fixture_pr_snapshot(b_thread, snapshot, cx);
        view.focus_thread(a, cx);
    });
    tick(cx);
    view.update(cx, |view, cx| {
        if view.panel_open(a) {
            view.toggle_panel_now(cx);
        }
    });
    tick(cx);
    (view, [a, b_thread], cx)
}

fn press(selector: String, button: MouseButton, modifiers: gpui::Modifiers, cx: &mut gpui::VisualTestContext) {
    let at = debug_bounds(cx, selector.clone()).unwrap_or_else(|| panic!("{selector}")).center();
    cx.simulate_mouse_down(at, button, modifiers);
    cx.simulate_mouse_up(at, button, modifiers);
    tick(cx);
}

fn badge(thread: ThreadId) -> String {
    format!("nav-card-pr-{}", thread.get())
}

fn clipboard(cx: &mut gpui::VisualTestContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

/// The audit's NAV-01: A is shown, its panel hidden, and B's PR badge is
/// pressed. B's PR is what shows.
#[gpui::test]
fn another_threads_pr_badge_shows_that_threads_pr(cx: &mut TestAppContext) {
    let (view, [a, b], cx) = two_prs("link-pr-badge-b", Some(REPO_B), cx);
    assert!(cx.debug_bounds("pr-header").is_none(), "nothing of a PR shows yet");
    assert_eq!(view.read_with(cx, |view, cx| view.focused_thread(cx)), Some(a));

    press(badge(b), MouseButton::Left, gpui::Modifiers::none(), cx);

    assert_eq!(view.read_with(cx, |view, cx| view.focused_thread(cx)), Some(b));
    let shown = view.read_with(cx, |view, _| view.panel.surfaces.get(b).map(|s| (s.open, s.active.clone())));
    assert_eq!(shown, Some((true, Some(SurfaceId::PullRequest(202)))));
    assert!(cx.debug_bounds("pr-header").is_some(), "its native detail is drawn");
    assert_eq!(cx.opened_url(), None, "nothing left Ferrite");
}

/// Copy on the badge's own menu is the forge address of the PR pressed;
/// the rest of the row keeps the Thread's menu, whose link is named for
/// what it is.
#[gpui::test]
fn the_badges_menu_copies_the_pr_and_the_row_copies_the_thread(cx: &mut TestAppContext) {
    let (view, [a, b], cx) = two_prs("link-pr-badge-menu", Some(REPO_B), cx);

    press(badge(b), MouseButton::Right, gpui::Modifiers::none(), cx);
    let labels: Vec<String> = view.read_with(cx, |view, cx| {
        view.context_rows(MenuTarget::PrLink(b), cx)
            .into_iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect()
    });
    assert_eq!(
        labels,
        ["Open pull request", "Open in integrated browser", "Open in system browser", "Copy link address"]
    );
    let copy = debug_bounds(cx, "context-menu-row-4".into()).expect("Copy link address, under the rule");
    cx.simulate_click(copy.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(clipboard(cx).as_deref(), Some(REPO_B), "B's forge URL");

    press(format!("nav-thread-{}", a.get()), MouseButton::Right, gpui::Modifiers::none(), cx);
    let labels: Vec<String> = view.read_with(cx, |view, cx| {
        view.context_rows(MenuTarget::Thread(a), cx)
            .into_iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect()
    });
    assert!(cx.debug_bounds("context-menu").is_some(), "the row's own menu opens");
    assert!(labels.contains(&"Copy thread link".to_string()), "{labels:?}");
    assert!(!labels.contains(&"Copy link".to_string()), "the label is not ambiguous");
}

#[gpui::test]
fn a_command_press_on_the_badge_opens_the_forge_in_the_system_browser(cx: &mut TestAppContext) {
    let (view, [a, b], cx) = two_prs("link-pr-badge-system", Some(REPO_B), cx);
    let secondary = if cfg!(target_os = "macos") {
        gpui::Modifiers::command()
    } else {
        gpui::Modifiers::control()
    };
    press(badge(b), MouseButton::Left, secondary, cx);
    assert_eq!(cx.opened_url().as_deref(), Some(REPO_B));
    assert_eq!(view.read_with(cx, |view, cx| view.focused_thread(cx)), Some(a), "the Thread stays");
}

/// A PR the forge has not given an address for opens natively, and says
/// why on every row that needs the address.
#[gpui::test]
fn a_pr_without_an_address_says_why_on_its_rows(cx: &mut TestAppContext) {
    let (view, [_, b], cx) = two_prs("link-pr-badge-no-url", None, cx);
    let rows = view.read_with(cx, |view, cx| view.context_rows(MenuTarget::PrLink(b), cx));
    let disabled: Vec<(String, bool)> = rows
        .into_iter()
        .flatten()
        .map(|(item, _)| (item.label.to_string(), item.disabled))
        .collect();
    assert_eq!(
        disabled,
        [
            ("Open pull request".to_string(), false),
            ("Open in integrated browser".to_string(), true),
            ("Open in system browser".to_string(), true),
            ("Copy link address".to_string(), true),
        ]
    );
}
