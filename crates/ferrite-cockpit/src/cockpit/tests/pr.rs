//! A Thread's PR reading (`crate::pr`): what it holds, when `gh` is
//! asked, and the two actions on it.
use super::*;
use crate::palette::{Act, Command};

fn asked(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<(ThreadId, bool)> {
    view.update(cx, |view, _| std::mem::take(&mut view.pr_asked))
}

/// The number of the PR the Thread's last reading names.
fn reading(
    view: &Entity<CockpitView>,
    thread: ThreadId,
    cx: &mut gpui::VisualTestContext,
) -> Option<u64> {
    view.read_with(cx, |view, _| view.facts.pr(thread).map(|pr| pr.number))
}

fn land(
    view: &Entity<CockpitView>,
    thread: ThreadId,
    reading: Option<ferrite_core::workspace::PullRequest>,
    cx: &mut gpui::VisualTestContext,
) {
    view.update(cx, |view, cx| view.pr_landed(thread, reading, cx));
}

fn run(view: &Entity<CockpitView>, command: Command, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_palette_act(Act::Command(command), false, window, cx)
        })
    });
}

/// Opening a Thread asks for its PR; the answer is its reading, kept while
/// the next one is out, and gone once `gh` has nothing to say.
#[gpui::test]
fn a_thread_keeps_its_prs_reading(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("pr-label", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    assert_eq!(asked(&view, cx), [(thread, false)], "opened, asked once");
    assert_eq!(reading(&view, thread, cx), None, "no PR known yet");

    land(&view, thread, Some(pr_with_checks()), cx);
    assert_eq!(reading(&view, thread, cx), Some(48));

    run(&view, Command::RefreshPr, cx);
    assert_eq!(
        asked(&view, cx),
        [(thread, true)],
        "a manual ask waits for nothing"
    );
    assert_eq!(reading(&view, thread, cx), Some(48), "kept while asking");

    land(&view, thread, None, cx);
    assert_eq!(reading(&view, thread, cx), None, "no gh, no PR: no reading");
}

/// Focusing a Thread asks for its PR, at most once per thirty seconds.
#[gpui::test]
fn focusing_a_thread_asks_at_most_every_thirty_seconds(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("pr-focus", 2);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let threads = view.read_with(cx, |view, _| view.cockpit.threads());
    let mut opened = asked(&view, cx);
    opened.sort();
    assert_eq!(opened, [(threads[0], false), (threads[1], false)]);
    for thread in &threads {
        land(&view, *thread, None, cx);
    }
    let focus = |thread: ThreadId, cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| view.focus_thread(thread, cx));
        tick(cx);
    };

    focus(threads[0], cx);
    focus(threads[1], cx);
    assert!(asked(&view, cx).is_empty(), "both were asked just now");

    leap(cx, crate::pr::AUTO_EVERY);
    focus(threads[0], cx);
    assert_eq!(asked(&view, cx), [(threads[0], false)]);
}

/// The Thread's right-click menu refreshes its PR and, once one is known,
/// opens it; the palette offers both for the focused Thread.
#[gpui::test]
fn the_menu_and_the_palette_offer_the_pr_actions(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("pr-actions", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    let menu = |view: &CockpitView| -> Vec<String> {
        view.context_rows(MenuTarget::Thread(thread))
            .into_iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect()
    };
    let palette = |cx: &mut gpui::VisualTestContext| -> Vec<String> {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.palette_command_rows(window, cx)
                    .into_iter()
                    .map(|row| row.name.to_string())
                    .collect()
            })
        })
    };
    let refresh = "Refresh PR status".to_string();
    let open = "Open PR in browser".to_string();
    view.read_with(cx, |view, _| {
        assert!(menu(view).contains(&refresh));
        assert!(!menu(view).contains(&open), "no PR to open yet");
    });
    assert!(palette(cx).contains(&refresh));
    assert!(!palette(cx).contains(&open));

    land(&view, thread, Some(pr_with_checks()), cx);
    view.read_with(cx, |view, _| assert!(menu(view).contains(&open)));
    assert!(palette(cx).contains(&open));

    asked(&view, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.run_menu_verb(MenuTarget::Thread(thread), MenuVerb::RefreshPr, window, cx)
        })
    });
    assert_eq!(asked(&view, cx), [(thread, true)]);
}

/// A push, a PR or background work handed back can move the workspace
/// behind the agent's own edits: its PR is read again at once, so Changes
/// follows (HD27). Any other command asks nothing.
#[gpui::test]
fn a_push_reads_the_pr_again(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("pr-after-push", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.cockpit.threads()[0]);
    land(&view, thread, None, cx);
    asked(&view, cx);
    let send = |event: SessionEvent| fake.streams.borrow()[0].send(event).unwrap();
    for (id, command) in [("ls", "ls -la"), ("push", "git push -u origin feature")] {
        send(SessionEvent::ToolStarted {
            id: id.into(),
            name: "Bash".into(),
            input: serde_json::json!({"command": command}),
        });
        send(SessionEvent::ToolCompleted {
            id: id.into(),
            output: String::new(),
            is_error: false,
            result: ferrite_core::ToolResult::Opaque,
        });
        tick(cx);
        let expected: &[(ThreadId, bool)] = if id == "push" { &[(thread, true)] } else { &[] };
        assert_eq!(asked(&view, cx), expected, "after {command}");
    }
}
