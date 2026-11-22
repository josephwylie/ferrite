//! A Notice act never holds the window's thread (ADR 0041): each is told
//! to the Server, and what is seen is told once, though the window looks
//! every frame. Read from core's probe (`ferrite_core::cockpit::waits`).
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
