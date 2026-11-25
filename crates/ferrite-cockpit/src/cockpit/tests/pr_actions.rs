//! A PR action stays with its PR from the press to the forge's word on
//! it: pending while the agent has it, failed with a way to retry, done
//! once the PR shows it.
use super::pr::snapshot;
use super::*;
use ferrite_core::pull_request::PrTarget;
use crate::cockpit::panel::PrAction;
use ferrite_client::cockpit::HeldRead;
use ferrite_core::protocol::{ReviewPullRequest, WireMerge, WirePr, WirePrState};
use ferrite_reviewr::forge::PrView;

fn read_of(pr: WirePr) -> ReviewPullRequest {
    ReviewPullRequest {
        noun: "PR".into(),
        pr: Ok(pr),
        unread: None,
    }
}

/// A Thread on PR #48, shown in its Surface as `pr`, whose forge reads
/// wait for the test.
fn on_pr<'a>(
    name: &str,
    pr: WirePr,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    ThreadKey,
    Fake,
    &'a mut gpui::VisualTestContext,
) {
    let (core, fake) = cockpit(name, 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        let path = view.thread_path(thread, cx).unwrap();
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_forge(path.clone(), PrView::NoPr));
        view.cockpit.update(cx, |cockpit, _| cockpit.fixture_hold_pr_reads(path));
        view.pr_landed(thread, Some(pr_with_checks()), cx);
        view.fixture_pr_snapshot(thread, pr, cx);
        view.open_pr_surface(thread, cx);
    });
    tick(cx);
    (view, thread, fake, cx)
}

fn click(selector: &'static str, cx: &mut gpui::VisualTestContext) {
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(at.center(), gpui::Modifiers::none());
    tick(cx);
}

fn merge(cx: &mut gpui::VisualTestContext) {
    click("pr-primary", cx);
    click("pr-primary", cx);
}

fn sent(fake: &Fake) -> usize {
    fake.sent.borrow().len()
}

fn reads(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<HeldRead> {
    view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| cockpit.take_held_reads()))
}

/// The words of the notice strip, as it draws them.
fn said(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<String> {
    view.read_with(cx, |view, cx| {
        let thread = view.panes[0].thread()?;
        let reader = view.pull_request.read(cx);
        let (words, ..) = reader.pr_action_words(PrTarget::thread(thread, 48))?;
        Some(words)
    })
}

/// The agent works on what it was handed, then its turn ends.
fn end_turn(fake: &Fake, cx: &mut gpui::VisualTestContext) {
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "on it".into(),
        })
        .unwrap();
    tick(cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
}

/// Merge hands off once, shows it is running and cannot be pressed again,
/// and when the agent's turn ends the PR is read: merged, it is done.
#[gpui::test]
fn asking_about_a_pr_prepares_its_current_composer_without_sending(cx: &mut TestAppContext) {
    let (view, thread, fake, cx) = on_pr("pr-ask-composer", snapshot(WirePrState::Open), cx);
    let before = view.read_with(cx, |view, _| view.panes.len());
    view.update(cx, |view, cx| {
        let index = view.pane_for(thread).unwrap();
        view.panes[index]
            .composer
            .update(cx, |composer, cx| composer.set("My draft".into(), cx));
        view.run_pr_action(PrTarget::thread(thread, 48), PrAction::Ask, cx);
    });
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes.len(), before);
        let index = view.pane_for(thread).unwrap();
        let text = view.panes[index].composer.read(cx).text();
        assert!(text.starts_with("My draft\n\n"));
        assert!(text.contains("https://github.com/o/r/pull/48"));
    });
    assert_eq!(sent(&fake), 0);
}

#[gpui::test]
fn a_merge_is_pending_until_the_pr_shows_it_merged(cx: &mut TestAppContext) {
    let (view, _thread, fake, cx) = on_pr("pr-act-merge", snapshot(WirePrState::Open), cx);
    merge(cx);
    assert_eq!(sent(&fake), 1);
    assert!(fake.sent.borrow()[0].contains("gh pr merge 48"));
    assert!(cx.debug_bounds("pr-action").is_some(), "the strip says so");
    assert!(
        said(&view, cx).unwrap().contains("the agent is on it"),
        "{:?}",
        said(&view, cx)
    );

    merge(cx);
    assert_eq!(sent(&fake), 1, "pressing again sends nothing more");

    reads(&view, cx);
    end_turn(&fake, cx);
    assert!(said(&view, cx).unwrap().contains("checking the PR"));
    let check = reads(&view, cx)
        .pop()
        .expect("the PR is read after the turn");
    check.answer(read_of(snapshot(WirePrState::Merged)));
    tick(cx);
    assert!(said(&view, cx).unwrap().contains("Merged #48"));
    assert!(cx.debug_bounds("pr-pill").is_some(), "the PR shows merged");
    click("pr-action-dismiss", cx);
    assert!(cx.debug_bounds("pr-action").is_none());
}

/// The agent finished and the PR still is not merged: the action failed,
/// the PR's content stays, and Retry hands it off again.
#[gpui::test]
fn a_merge_the_pr_does_not_show_fails_and_retries(cx: &mut TestAppContext) {
    let (view, _thread, fake, cx) = on_pr("pr-act-unmerged", snapshot(WirePrState::Open), cx);
    merge(cx);
    reads(&view, cx);
    end_turn(&fake, cx);
    reads(&view, cx)
        .pop()
        .unwrap()
        .answer(read_of(snapshot(WirePrState::Open)));
    tick(cx);
    assert!(
        said(&view, cx).unwrap().contains("failed"),
        "{:?}",
        said(&view, cx)
    );
    assert!(
        cx.debug_bounds("pr-title").is_some(),
        "the PR's content stays"
    );
    assert!(cx.debug_bounds("pr-action-retry").is_some());

    click("pr-action-retry", cx);
    assert_eq!(sent(&fake), 2, "retry asks the agent again");
    assert!(said(&view, cx).unwrap().contains("the agent is on it"));
}

/// A send the Session refuses clears at once into a failure; the words
/// typed in the Composer are still there, and Retry works once it can.
#[gpui::test]
fn a_refused_hand_off_fails_and_keeps_the_typed_words(cx: &mut TestAppContext) {
    let (view, _thread, fake, cx) = on_pr("pr-act-refused", snapshot(WirePrState::Open), cx);
    cx.simulate_input("half a thought");
    *fake.fail_send.borrow_mut() = true;
    merge(cx);
    assert_eq!(sent(&fake), 0);
    assert!(
        said(&view, cx).unwrap().contains("failed"),
        "{:?}",
        said(&view, cx)
    );
    let typed = view.read_with(cx, |view, cx| {
        view.panes[0].composer.read(cx).text().to_string()
    });
    assert_eq!(typed, "half a thought");

    *fake.fail_send.borrow_mut() = false;
    click("pr-action-retry", cx);
    assert_eq!(sent(&fake), 1);
    assert!(said(&view, cx).unwrap().contains("the agent is on it"));
}

/// Behind a running turn the action is queued, and stays its PR's while
/// the Panel shows another tab.
#[gpui::test]
fn a_queued_action_stays_with_its_pr_through_another_tab(cx: &mut TestAppContext) {
    use crate::cockpit::surfaces::SurfaceId;
    let (view, thread, fake, cx) = on_pr("pr-act-queued", snapshot(WirePrState::Open), cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "working".into(),
        })
        .unwrap();
    tick(cx);
    merge(cx);
    assert!(
        said(&view, cx).unwrap().contains("queued behind"),
        "{:?}",
        said(&view, cx)
    );

    view.update(cx, |view, cx| {
        view.upsert_surface(thread, SurfaceId::Overview, cx)
    });
    tick(cx);
    assert!(
        said(&view, cx).unwrap().contains("queued behind"),
        "kept off its tab"
    );
    view.update(cx, |view, cx| {
        view.activate_surface(thread, &SurfaceId::PullRequest(48), cx)
    });
    tick(cx);
    assert!(
        cx.debug_bounds("pr-action").is_some(),
        "and shown on its return"
    );
}

/// A PR the forge reports as conflicting offers no Merge, and says why.
#[gpui::test]
fn a_conflicting_pr_cannot_be_merged(cx: &mut TestAppContext) {
    let pr = WirePr {
        merge: WireMerge::Conflicting,
        ..snapshot(WirePrState::Open)
    };
    let (view, thread, fake, cx) = on_pr("pr-act-conflict", pr, cx);
    assert!(cx.debug_bounds("pr-action").is_some(), "the reason shows");
    merge(cx);
    assert_eq!(sent(&fake), 0);
    let rows = view.read_with(cx, |view, cx| {
        view.context_rows(MenuTarget::PrMore(PrTarget::thread(thread, 48)), cx)
            .into_iter()
            .flatten()
            .filter(|(item, _)| item.label.as_ref() == "Merge now")
            .map(|(item, _)| item.disabled)
            .collect::<Vec<_>>()
    });
    assert_eq!(rows, [true], "Merge now is there, off");
}

/// A queued action the user takes back never ran: an unrelated turn's end
/// resolves nothing, and the strip is gone.
#[gpui::test]
fn a_queued_action_taken_back_is_withdrawn(cx: &mut TestAppContext) {
    let (view, _thread, fake, cx) = on_pr("pr-act-withdrawn", snapshot(WirePrState::Open), cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "working".into(),
        })
        .unwrap();
    tick(cx);
    merge(cx);
    assert!(said(&view, cx).unwrap().contains("queued behind"));

    click("queued-close-0", cx);
    reads(&view, cx);
    end_turn(&fake, cx);
    assert_eq!(said(&view, cx), None, "withdrawn, not merged or failed");
    assert!(cx.debug_bounds("pr-action").is_none());
    assert!(reads(&view, cx).is_empty(), "no PR read was asked");
}

/// Retry is hit twice in a row: the second click lands where the first
/// did, and must not abandon the action the first handed off.
#[gpui::test]
fn a_second_click_on_retry_does_not_stop_waiting(cx: &mut TestAppContext) {
    let (view, _thread, fake, cx) = on_pr("pr-act-retry-twice", snapshot(WirePrState::Open), cx);
    *fake.fail_send.borrow_mut() = true;
    merge(cx);
    *fake.fail_send.borrow_mut() = false;
    let retry = cx.debug_bounds("pr-action-retry").unwrap().center();
    cx.simulate_click(retry, gpui::Modifiers::none());
    tick(cx);
    assert!(said(&view, cx).unwrap().contains("the agent is on it"));
    cx.simulate_click(retry, gpui::Modifiers::none());
    tick(cx);
    assert!(
        said(&view, cx).unwrap().contains("the agent is on it"),
        "{:?}",
        said(&view, cx)
    );
    assert_eq!(sent(&fake), 1);
}

/// The PR cannot be read after the agent finished: the outcome is not
/// judged, the strip says so, and Retry reads again.
#[gpui::test]
fn a_failed_read_back_offers_a_retry_of_the_read(cx: &mut TestAppContext) {
    let (view, _thread, fake, cx) = on_pr("pr-act-read-failed", snapshot(WirePrState::Open), cx);
    merge(cx);
    reads(&view, cx);
    end_turn(&fake, cx);
    reads(&view, cx).pop().unwrap().fail("offline");
    tick(cx);
    let said_now = said(&view, cx).unwrap();
    assert!(said_now.contains("could not be read"), "{said_now}");
    click("pr-action-retry", cx);
    assert_eq!(sent(&fake), 1, "the read again, not the merge");
    reads(&view, cx)
        .pop()
        .expect("the PR is read again")
        .answer(read_of(snapshot(WirePrState::Merged)));
    tick(cx);
    assert!(said(&view, cx).unwrap().contains("Merged #48"));
}

/// A queued action is picked up in the frame the turn before it ends: that
/// turn's end does not settle it; the turn that runs it does.
#[gpui::test]
fn a_queued_action_a_turn_picks_up_runs_and_is_checked(cx: &mut TestAppContext) {
    let (view, _thread, fake, cx) = on_pr("pr-act-picked-up", snapshot(WirePrState::Open), cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "working".into(),
        })
        .unwrap();
    tick(cx);
    merge(cx);
    let id = view.read_with(cx, |view, cx| {
        let thread = view.panes[0].thread().unwrap();
        view.cockpit.read(cx).thread(thread).unwrap().queue()[0].id.clone()
    });
    let stream = fake.streams.borrow()[0].clone();
    stream
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    stream
        .send(SessionEvent::Queue(ferrite_core::QueueEvent::Started {
            historical: false,
            client_id: id,
            text: None,
        }))
        .unwrap();
    tick(cx);
    for read in reads(&view, cx) {
        read.answer(read_of(snapshot(WirePrState::Open)));
    }
    tick(cx);
    let said_now = said(&view, cx).unwrap();
    assert!(said_now.contains("the agent is on it"), "{said_now}");
    end_turn(&fake, cx);
    reads(&view, cx)
        .pop()
        .expect("the PR is read once the merge turn ends")
        .answer(read_of(snapshot(WirePrState::Merged)));
    tick(cx);
    assert!(said(&view, cx).unwrap().contains("Merged #48"));
}
