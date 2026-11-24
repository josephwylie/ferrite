//! An act whose reply nobody reads never holds the window's thread (ADR
//! 0041): it is told to the Server, and what is seen is told once, though
//! the window looks every frame. Read from core's probe
//! (`ferrite_client::cockpit::waits`).
use super::*;
use ferrite_client::cockpit::waits;
use ferrite_core::protocol::ServerMessage;

fn notice_acts() -> Vec<waits::Wait> {
    waits::take().into_iter().filter(|wait| wait.op == "notice").collect()
}

#[test]
fn what_is_seen_is_told_once_and_no_notice_act_waits() {
    let (mut core, thread, _) = super::machines::browser_core("notice-asks");
    waits::start();
    core.fixture_task_alert(ServerMessage::Notices {
        revision: u64::MAX,
        notices: vec![super::machines::box_request(thread.thread(), 1)],
    });
    for _ in 0..5 {
        core.set_window_active(false);
        core.set_window_active(true);
    }
    let seen = notice_acts();
    assert_eq!(seen.len(), 1, "one unread request is told seen once: {seen:?}");
    assert!(!seen[0].waited, "telling what is seen does not wait: {seen:?}");

    waits::start();
    core.mark_unread(thread);
    core.mark_notices_read();
    core.clear_notices();
    let acts = notice_acts();
    assert_eq!(acts.len(), 3, "each act is sent: {acts:?}");
    assert!(acts.iter().all(|act| !act.waited), "no Notice act waits for its reply: {acts:?}");
}

#[test]
fn a_turns_presses_that_read_no_reply_do_not_wait() {
    let (mut core, thread, _) = super::machines::browser_core("turn-tells");
    waits::start();
    core.interrupt(thread);
    core.say(thread, "a note".into());
    core.send_queued_now(thread, "none");
    core.remove_queued_item(thread, "none", false);
    core.settings_changed(&ferrite_core::settings::Settings::default());
    let waits = waits::take();
    let ops: Vec<_> = waits.iter().map(|wait| wait.op.as_str()).collect();
    assert_eq!(ops, ["interrupt", "say", "send_queued_now", "cancel_queued", "settings_changed"]);
    assert!(waits.iter().all(|wait| !wait.waited), "none waits for its reply: {waits:?}");
}

/// A prompt's send and its queueing are asked, never waited for, and the
/// Server's answer comes through the ask.
#[test]
fn a_send_and_a_queueing_do_not_wait_and_are_answered() {
    let (mut core, thread, _) = super::machines::browser_core("send-asks");
    waits::start();
    let sent = core.send(thread, "first".into());
    let queued = core.queue(thread, "second".into());
    let waits = waits::take();
    let ops: Vec<_> = waits.iter().map(|wait| wait.op.as_str()).collect();
    assert_eq!(ops, ["send", "queue"]);
    assert!(waits.iter().all(|wait| !wait.waited), "neither waits for its reply: {waits:?}");
    let admission = core.wait_for(sent).expect("the Server answers a send");
    let queued = core.wait_for(queued).expect("the Server answers a queueing");
    assert_eq!(admission, ferrite_client::cockpit::Admission::Delivered);
    assert!(!queued, "an idle Thread queues nothing");
}

/// ↵ twice in the Composer over a Server: neither waits, and the second
/// draft is asked only once the first is answered, so it is queued behind
/// the turn the first began, as it was when the window waited. A refusal
/// that comes after the press puts the draft back.
#[gpui::test]
fn the_composers_sends_go_in_order_and_none_waits(cx: &mut gpui::TestAppContext) {
    let (_engine, view, thread, cx) = super::browser_chrome::browser_window(cx, "send-outbox");
    let asked = || -> Vec<waits::Wait> {
        waits::take().into_iter().filter(|wait| ["send", "queue"].contains(&wait.op.as_str())).collect()
    };
    let on_the_way = |cx: &mut gpui::VisualTestContext| view.read_with(cx, |view, _| view.outbox.count(thread));
    waits::start();
    for words in ["one", "two"] {
        cx.simulate_input(words);
        cx.simulate_keystrokes("enter");
    }
    let first = asked();
    assert_eq!(first.len(), 1, "the second waits for the first's answer: {first:?}");
    assert_eq!((first[0].op.as_str(), first[0].waited), ("send", false), "{first:?}");
    assert_eq!(on_the_way(cx), 2);

    waits::start();
    until(cx, |cx| on_the_way(cx) == 0);
    let second = asked();
    assert_eq!(second.len(), 1, "then the second goes: {second:?}");
    assert_eq!((second[0].op.as_str(), second[0].waited), ("queue", false), "{second:?}");
    // The fixture Server's Thread has no agent to queue behind, so it
    // refuses: the refusal arrives after the press, and the draft is back.
    let left = view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).text().to_string());
    assert_eq!(left, "two", "a draft the Server refused later is back in the Composer");
}

/// A prompt sent again over a Server does not wait: it goes behind the
/// draft sent before it, and what follows it hears where it went once the
/// Server answers.
#[gpui::test]
fn a_prompt_sent_again_does_not_wait_and_hears_where_it_went(cx: &mut gpui::TestAppContext) {
    use std::{cell::RefCell, rc::Rc};
    let (_engine, view, thread, cx) = super::browser_chrome::browser_window(cx, "send-again");
    let heard = Rc::new(RefCell::new(None));
    waits::start();
    cx.simulate_input("one");
    cx.simulate_keystrokes("enter");
    view.update(cx, |view, cx| {
        let heard = heard.clone();
        view.send_again_then(0, thread, "again".into(), move |_, handoff, _| *heard.borrow_mut() = Some(handoff), cx);
    });
    let first: Vec<_> = waits::take().into_iter().filter(|wait| ["send", "queue"].contains(&wait.op.as_str())).collect();
    assert_eq!(first.len(), 1, "the prompt sent again waits for the draft's answer: {first:?}");
    assert!(!first[0].waited, "{first:?}");
    assert_eq!(*heard.borrow(), None, "nothing is heard before the Server answers");

    until(cx, |_| heard.borrow().is_some());
    // The fixture Server's Thread has no agent to queue behind.
    assert_eq!(*heard.borrow(), Some(Err("The agent's queue did not take it.".into())));
    assert_eq!(view.read_with(cx, |view, _| view.outbox.count(thread)), 0);
}

/// The Turn and Session presses whose reply the window reads are asked,
/// never waited for, and each is answered through its ask.
#[test]
fn a_turns_presses_that_read_a_reply_do_not_wait_and_are_answered() {
    use ferrite_core::store::Provider;
    let (mut core, thread, _) = super::machines::browser_core("turn-asks");
    waits::start();
    let model = core.set_model(thread, Some("sonnet".into()));
    let effort = core.set_effort(thread, Some("high".into()));
    let provider = core.set_provider(thread, ferrite_client::cockpit::ProviderChoice { provider: Provider::Codex, model: None });
    let fresh = core.start_fresh_conversation(thread);
    let rewind = core.rewind(thread, "none", false, false);
    let fork = core.fork(thread, None, None);
    let waits = waits::take();
    let ops: Vec<_> = waits.iter().map(|wait| wait.op.as_str()).collect();
    assert_eq!(ops, ["set_model", "set_effort", "set_provider", "start_fresh", "rewind", "fork"]);
    assert!(waits.iter().all(|wait| !wait.waited), "none waits for its reply: {waits:?}");
    for ask in [model, effort, provider, fresh, rewind, fork] {
        // The fixture Server's Thread has no agent, so some are refused:
        // either way the Server's word came through the ask.
        let _ = core.wait_for(ask);
    }
}

/// Naming a Thread, snoozing it and a Session control are asked, never
/// waited for, and each is answered through its ask.
#[test]
fn a_threads_name_snooze_and_controls_do_not_wait_and_are_answered() {
    let (mut core, thread, _) = super::machines::browser_core("thread-asks");
    waits::start();
    let renamed = core.rename_thread(thread, "Release notes");
    let later = ferrite_core::clock::system_time() + std::time::Duration::from_secs(3600);
    let snoozed = core.snooze_thread(thread, Some(later));
    let control = core.control(thread, ferrite_core::SessionControl::RefreshContext);
    let waits = waits::take();
    let ops: Vec<_> = waits.iter().map(|wait| wait.op.as_str()).collect();
    assert_eq!(ops, ["rename", "snooze", "control"]);
    assert!(waits.iter().all(|wait| !wait.waited), "none waits for its reply: {waits:?}");
    core.wait_for(renamed).expect("the Server names the Thread");
    for ask in [snoozed, control] {
        // The fixture Server may refuse either: its word came through the ask.
        let _ = core.wait_for(ask);
    }
}

/// ⌘W over a Server does not wait: the Pane goes once the Server answers
/// the park, and the model has taken the Thread off the roster by then.
#[gpui::test]
fn closing_a_pane_does_not_wait_and_the_pane_goes_when_answered(cx: &mut gpui::TestAppContext) {
    use ferrite_core::roster::PaneIdentity;
    let (_engine, view, thread, cx) = super::browser_chrome::browser_window(cx, "close-ask");
    let shown = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.panes.iter().any(|pane| pane.thread() == Some(thread)))
    };
    assert!(shown(cx));
    waits::start();
    view.update(cx, |view, cx| view.close_pane(PaneIdentity::Thread(thread), cx));
    let parks: Vec<_> = waits::take().into_iter().filter(|wait| wait.op == "park").collect();
    assert_eq!(parks.len(), 1, "{parks:?}");
    assert!(!parks[0].waited, "the park does not wait for its reply: {parks:?}");
    until(cx, |cx| !shown(cx));
}

/// Deleting a Thread over a Server does not wait, and what follows hears
/// how it went.
#[gpui::test]
fn deleting_a_thread_does_not_wait_and_is_answered(cx: &mut gpui::TestAppContext) {
    use std::{cell::Cell, rc::Rc};
    let (_engine, view, thread, cx) = super::browser_chrome::browser_window(cx, "delete-ask");
    let heard = Rc::new(Cell::new(false));
    waits::start();
    view.update(cx, |view, cx| {
        let heard = heard.clone();
        view.delete_threads(vec![thread], move |_, deleted, _| heard.set(deleted.len() == 1), cx);
    });
    let deletes: Vec<_> = waits::take().into_iter().filter(|wait| wait.op == "delete").collect();
    assert_eq!(deletes.len(), 1, "{deletes:?}");
    assert!(!deletes[0].waited, "the delete does not wait for its reply: {deletes:?}");
    assert!(!heard.get(), "nothing is heard before the Server answers");
    until(cx, |_| heard.get());
}

/// A parked Thread asked back over a Server does not wait: its Pane is
/// there, with the screen, once the Server answers.
#[gpui::test]
fn reopening_a_parked_thread_does_not_wait_and_takes_the_screen_when_answered(cx: &mut gpui::TestAppContext) {
    use ferrite_core::roster::PaneIdentity;
    let (_engine, view, thread, cx) = super::browser_chrome::browser_window(cx, "reopen-ask");
    let shown = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| view.panes.iter().any(|pane| pane.thread() == Some(thread)))
    };
    view.update(cx, |view, cx| view.close_pane(PaneIdentity::Thread(thread), cx));
    until(cx, |cx| !shown(cx));

    waits::start();
    view.update(cx, |view, cx| view.revive_thread(thread, cx));
    let revives: Vec<_> = waits::take().into_iter().filter(|wait| wait.op == "revive").collect();
    assert_eq!(revives.len(), 1, "{revives:?}");
    assert!(!revives[0].waited, "the revive does not wait for its reply: {revives:?}");
    until(cx, |cx| shown(cx));
    let focused = view.read_with(cx, |view, cx| view.cockpit.read(cx).roster().focused_thread());
    assert_eq!(focused, Some(thread), "the Thread asked back takes the screen");
}

/// ↑ twice in the Composer over a Server: neither waits, and the second
/// step is asked only once the first is answered, since it starts from
/// what the first put in the Composer.
#[gpui::test]
fn history_steps_go_in_order_and_none_waits(cx: &mut gpui::TestAppContext) {
    use ferrite_client::cockpit::HistoryDirection;
    let (_engine, view, thread, cx) = super::browser_chrome::browser_window(cx, "recall-asks");
    let recalls = || -> Vec<waits::Wait> { waits::take().into_iter().filter(|wait| wait.op == "recall").collect() };
    waits::start();
    view.update(cx, |view, cx| {
        view.step_history_of(thread, HistoryDirection::Older, cx);
        view.step_history_of(thread, HistoryDirection::Older, cx);
    });
    let first = recalls();
    assert_eq!(first.len(), 1, "the second step waits for the first's answer: {first:?}");
    assert!(!first[0].waited, "{first:?}");

    waits::start();
    until(cx, |cx| view.read_with(cx, |view, _| view.returning_steps(thread) == 0));
    let second = recalls();
    assert_eq!(second.len(), 1, "then the second goes: {second:?}");
    assert!(!second[0].waited, "{second:?}");
}

/// Taking a held prompt back is asked, never waited for, and answered.
#[test]
fn taking_a_held_prompt_back_does_not_wait_and_is_answered() {
    let (mut core, thread, _) = super::machines::browser_core("unqueue-ask");
    waits::start();
    let held = core.unqueue(thread);
    let waits = waits::take();
    let ops: Vec<_> = waits.iter().map(|wait| wait.op.as_str()).collect();
    assert_eq!(ops, ["unqueue"]);
    assert!(!waits[0].waited, "{waits:?}");
    assert_eq!(core.wait_for(held).expect("the Server answers"), None, "the Thread held none");
}

/// A Project's acts are asked, never waited for, and each is answered.
#[test]
fn a_projects_acts_do_not_wait_and_are_answered() {
    let (mut core, thread, _) = super::machines::browser_core("project-asks");
    let root = std::env::temp_dir().join("ferrite-project-asks");
    std::fs::create_dir_all(&root).unwrap();
    waits::start();
    let registered = core.register_project(thread.machine(), &root);
    let waits = waits::take();
    let ops: Vec<_> = waits.iter().map(|wait| wait.op.as_str()).collect();
    assert_eq!(ops, ["project"]);
    assert!(!waits[0].waited, "registering does not wait for its reply: {waits:?}");
    let project = core.wait_for(registered).expect("the Server registers the Project");

    waits::start();
    let named = core.rename_project(project, "Asks");
    let removed = core.remove_project(project);
    let waits = waits::take();
    assert_eq!(waits.len(), 2, "{waits:?}");
    assert!(waits.iter().all(|wait| !wait.waited), "neither waits for its reply: {waits:?}");
    core.wait_for(named).expect("the Server names it");
    core.wait_for(removed).expect("the Server removes it");
}
