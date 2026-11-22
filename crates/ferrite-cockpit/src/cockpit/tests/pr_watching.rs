//! A watched PR in the details card (ADR 0046): T3's eye beside its checks
//! while the Server watches it, a press on it that only stops the watch,
//! and the palette's `Stop watching PR`.
use super::*;
use crate::cockpit::surfaces::SurfaceId;
use ferrite_core::pr_watch::{Target, WatchFact};

fn palette(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.palette_command_rows(window, cx)
                .into_iter()
                .map(|row| row.name.to_string())
                .collect()
        })
    })
}

#[gpui::test]
fn the_eye_shows_while_the_pr_is_watched_and_its_press_stops_the_watch(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("pr-watch-eye", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.pr_landed(thread, Some(pr_with_checks()), cx));
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    assert!(cx.debug_bounds("details-card").is_some(), "the card is open");
    assert!(
        cx.debug_bounds("details-pr-watch").is_none(),
        "nobody watches the PR yet"
    );
    assert!(!palette(&view, cx).contains(&"Stop watching PR".to_string()));

    let mut fact = WatchFact::default();
    let target = Target::of_url("https://github.com/o/r/pull/48").unwrap();
    fact.watch(target, None, Some(true), 0);
    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_pr_watch(thread, fact))
    });
    until(cx, |cx| cx.debug_bounds("details-pr-watch").is_some());
    assert!(palette(&view, cx).contains(&"Stop watching PR".to_string()));
    assert!(
        !palette(&view, cx).contains(&"Watch PR for changes".to_string()),
        "no Server runs a watcher for this window: none can start"
    );

    let eye = cx.debug_bounds("details-pr-watch").unwrap();
    cx.simulate_click(eye.center(), gpui::Modifiers::none());
    until(cx, |cx| cx.debug_bounds("details-pr-watch").is_none());
    assert!(!palette(&view, cx).contains(&"Stop watching PR".to_string()));
    assert_ne!(
        panel_tabs(&view, thread, cx).1,
        Some(SurfaceId::PullRequest(48)),
        "the press only stopped the watch: the PR did not open"
    );
}

/// A watch that holds nothing (every check finished) brings no message
/// after its digest. When that digest lands while the window acts — every
/// two seconds it tells its Server what git listed — the card, the palette,
/// the PR menu and the background line still read it, together.
#[gpui::test]
fn a_watch_whose_digest_lands_while_the_window_acts_still_reaches_the_card(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("pr-watch-acted", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.pr_landed(thread, Some(pr_with_checks()), cx));
    cx.simulate_keystrokes("cmd-shift-b");
    tick(cx);
    assert!(cx.debug_bounds("details-pr-watch").is_none(), "the premise: unwatched");

    let mut fact = WatchFact::default();
    let target = Target::of_url("https://github.com/o/r/pull/48").unwrap();
    fact.watch(target, None, Some(false), 0);
    view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| {
            cockpit.fixture_pr_watch(thread, fact);
            cockpit.worktrees_listed(thread, vec![here()], std::time::Instant::now());
        })
    });
    until(cx, |cx| cx.debug_bounds("details-pr-watch").is_some());
    assert!(palette(&view, cx).contains(&"Stop watching PR".to_string()));
    let menu: Vec<String> = view.read_with(cx, |view, cx| {
        view.context_rows(MenuTarget::PrLink(thread), cx)
            .into_iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect()
    });
    assert!(menu.contains(&"Stop watching".to_string()), "{menu:?}");
    assert_eq!(
        view.read_with(cx, |view, cx| view.background_work_title(0, cx)),
        Some("Running \u{b7} Watching pull request #48".into())
    );
}

/// A watch is background work (T3's `Watching pull request #N`): while it
/// holds, the Thread is on the Working shelf and its line waits on it;
/// once every check finished it still runs, holding nothing, unless it
/// waits for the merge.
#[gpui::test]
fn a_watch_holds_the_thread_working_while_checks_run_or_until_merged(cx: &mut TestAppContext) {
    use ferrite_core::pr_watch::Until;
    let (core, _fake) = cockpit("pr-watch-working", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let watch = |fact: &WatchFact, cx: &mut gpui::VisualTestContext| {
        let fact = fact.clone();
        view.update(cx, |view, cx| {
            view.cockpit
                .update(cx, |cockpit, _| cockpit.fixture_pr_watch(thread, fact))
        });
        tick(cx);
    };
    let seen = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            (
                view.nav_state(cx).working.iter().any(|row| row.key == thread),
                view.background_work_title(0, cx),
            )
        })
    };
    assert_eq!(seen(cx), (false, None), "the premise: at rest");

    let target = Target::of_url("https://github.com/o/r/pull/48").unwrap();
    let mut fact = WatchFact::default();
    fact.watch(target.clone(), None, Some(true), 0);
    watch(&fact, cx);
    assert_eq!(
        seen(cx),
        (true, Some("Waiting \u{b7} Watching pull request #48".into())),
        "checks run on the head"
    );

    let mut cursor = fact.on[0].cursor.clone();
    cursor.checks_pending = false;
    fact.advance("o/r", 48, 0, cursor);
    watch(&fact, cx);
    assert_eq!(
        seen(cx),
        (false, Some("Running \u{b7} Watching pull request #48".into())),
        "every check finished: the watch goes on, holding nothing"
    );

    fact.watch(target, Some(Until::Merged), None, 0);
    watch(&fact, cx);
    assert!(seen(cx).0, "until merged, it holds whatever the checks say");

    fact.end("o/r", 48, None, None);
    watch(&fact, cx);
    assert_eq!(seen(cx), (false, None), "an ended watch is no work");
}

/// Escape is T3's Stop, which a thread takes only while a turn runs
/// (`canInterruptRunningThread`): mid-turn it stops the turn and, with it,
/// every watch, waking nothing; with no turn running it sends nothing, and
/// a watch goes on.
#[gpui::test]
fn escape_ends_a_watch_only_with_the_turn_it_stops(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("pr-watch-escape", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let watched = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            !view.cockpit.read(cx).thread(thread).unwrap().activity().watching().is_empty()
        })
    };
    let mut fact = WatchFact::default();
    fact.watch(Target::of_url("https://github.com/o/r/pull/48").unwrap(), None, Some(false), 0);
    view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| cockpit.fixture_pr_watch(thread, fact))
    });
    tick(cx);
    assert!(watched(cx), "the premise: watched, at rest");

    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 0, "nothing runs: nothing to stop");
    assert!(watched(cx), "the watch goes on");

    view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.send(thread, "go".into()))
    });
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta { text: "working".into() })
        .unwrap();
    tick(cx);
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 1, "the turn is stopped");
    assert!(!watched(cx), "and its watches with it");
}
