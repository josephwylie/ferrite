//! Public Conversation browser operations: ownership is independent of URL.
use ferrite_core::{
    browser::{Action, Service},
    ThreadId,
};

#[test]
fn opening_requires_an_attached_window_and_cannot_duplicate_a_threads_live_tabs() {
    let mut browser = Service::default();
    let first = ThreadId::new(1);
    let second = ThreadId::new(2);
    assert!(browser
        .apply(
            first,
            11,
            Action::Open {
                url: "https://example.com/".into()
            }
        )
        .is_err());
    browser.attach(first, 11).unwrap();
    browser.attach(second, 22).unwrap();
    let a = browser
        .apply(
            first,
            11,
            Action::Open {
                url: "https://example.com/".into(),
            },
        )
        .unwrap();
    let b = browser
        .apply(
            second,
            22,
            Action::Open {
                url: "https://example.com/".into(),
            },
        )
        .unwrap();
    assert_ne!(a.tabs[0].id, b.tabs[0].id);
    assert!(browser.attach(first, 22).is_err());
    assert!(browser
        .apply(first, 22, Action::Close { tab: a.tabs[0].id })
        .is_err());
    assert_eq!(browser.state(first).tabs.len(), 1);
}

#[test]
fn restart_keeps_urls_and_selection_without_realizing_a_page() {
    let root = std::env::temp_dir().join(format!("ferrite-browser-restore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = ferrite_core::store::Store::open(&root).unwrap();
    let thread = ThreadId::new(1);
    let mut browser = Service::default();
    browser.attach(thread, 3).unwrap();
    let opened = browser
        .apply(
            thread,
            3,
            Action::Open {
                url: "https://example.com/account".into(),
            },
        )
        .unwrap();
    store.save_browser(&browser.save()).unwrap();
    let mut restored = Service::restore(store.load_browser().unwrap().unwrap()).unwrap();
    let metadata = restored.state(thread);
    assert_eq!(metadata.active, opened.active);
    assert_eq!(metadata.tabs[0].url, "https://example.com/account");
    assert!(!metadata.tabs[0].live);
    assert!(metadata.owner.is_none());
    restored.attach(thread, 4).unwrap();
    let selected = restored
        .apply(
            thread,
            4,
            Action::Select {
                tab: metadata.tabs[0].id,
            },
        )
        .unwrap();
    assert!(selected.tabs[0].live);
    assert!(
        restored
            .check_document(thread, opened.tabs[0].document())
            .is_err(),
        "pre-restart references are stale"
    );
    store.quit(std::time::Duration::from_secs(1));
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn closing_the_visible_tab_realizes_its_dormant_successor() {
    let thread = ThreadId::new(1);
    let mut browser = Service::default();
    browser.attach(thread, 7).unwrap();
    browser
        .apply(
            thread,
            7,
            Action::Open {
                url: "https://example.com/first".into(),
            },
        )
        .unwrap();
    browser
        .apply(
            thread,
            7,
            Action::Open {
                url: "https://example.com/second".into(),
            },
        )
        .unwrap();
    let mut browser = Service::restore(browser.save()).unwrap();
    browser.attach(thread, 8).unwrap();
    let saved = browser.state(thread);
    browser
        .apply(
            thread,
            8,
            Action::Select {
                tab: saved.tabs[1].id,
            },
        )
        .unwrap();
    let closed = browser
        .apply(
            thread,
            8,
            Action::Close {
                tab: saved.tabs[1].id,
            },
        )
        .unwrap();
    assert_eq!(closed.active, Some(saved.tabs[0].id));
    assert!(
        closed.tabs[0].live,
        "the owning preview must display the newly selected page"
    );
}

#[test]
fn a_loading_page_is_retained_even_when_its_form_is_clean() {
    use ferrite_core::browser::{Dirty, PageState};
    let thread = ThreadId::new(1);
    let mut browser = Service::default();
    let owner = browser.attach(thread, 7).unwrap();
    let first = browser
        .apply(
            thread,
            7,
            Action::Open {
                url: "https://example.com/first".into(),
            },
        )
        .unwrap();
    browser
        .update(
            thread,
            owner,
            PageState {
                navigation: false,
                document: first.tabs[0].document(),
                url: first.tabs[0].url.clone(),
                loading: true,
                can_back: false,
                can_forward: false,
                error: None,
                dirty: Dirty::Clean,
            },
        )
        .unwrap();
    browser
        .apply(
            thread,
            7,
            Action::Open {
                url: "https://example.com/second".into(),
            },
        )
        .unwrap();
    assert!(!browser.reclaim(thread, owner, first.tabs[0].id).unwrap());
    assert!(browser.state(thread).tabs[0].live);
}
