//! An act whose reply nobody reads never holds the window's thread (ADR
//! 0041): it is told to the Server, and what is seen is told once, though
//! the window looks every frame. Read from core's probe
//! (`ferrite_core::cockpit::waits`).
use super::*;
use ferrite_core::cockpit::waits;
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
    assert_eq!(admission, ferrite_core::cockpit::Admission::Delivered);
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

/// The Turn and Session presses whose reply the window reads are asked,
/// never waited for, and each is answered through its ask.
#[test]
fn a_turns_presses_that_read_a_reply_do_not_wait_and_are_answered() {
    use ferrite_core::store::Provider;
    let (mut core, thread, _) = super::machines::browser_core("turn-asks");
    waits::start();
    let model = core.set_model(thread, Some("sonnet".into()));
    let effort = core.set_effort(thread, Some("high".into()));
    let provider = core.set_provider(thread, ferrite_core::cockpit::ProviderChoice { provider: Provider::Codex, model: None });
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
