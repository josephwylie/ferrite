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
