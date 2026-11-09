//! Public Conversation browser operations: ownership is independent of URL.
use ferrite_core::{browser::{Action, Service}, ThreadId};

#[test]
fn opening_requires_an_attached_window_and_cannot_duplicate_a_threads_live_tabs() {
    let mut browser = Service::default();
    let first = ThreadId::new(1);
    let second = ThreadId::new(2);
    assert!(browser.apply(first, 11, Action::Open { url: "https://example.com/".into() }).is_err());
    browser.attach(first, 11).unwrap();
    browser.attach(second, 22).unwrap();
    let a = browser.apply(first, 11, Action::Open { url: "https://example.com/".into() }).unwrap();
    let b = browser.apply(second, 22, Action::Open { url: "https://example.com/".into() }).unwrap();
    assert_ne!(a.tabs[0].id, b.tabs[0].id);
    assert!(browser.attach(first, 22).is_err());
    assert!(browser.apply(first, 22, Action::Close { tab: a.tabs[0].id }).is_err());
    assert_eq!(browser.state(first).tabs.len(), 1);
}
