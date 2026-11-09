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
                visible: false,
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

#[test]
fn hidden_selected_pages_release_only_after_native_acknowledgement() {
    use ferrite_core::browser::{Dirty, PageState, Protection};
    let thread = ThreadId::new(1);
    let mut browser = Service::default();
    let owner = browser.attach(thread, 7).unwrap();
    let opened = browser
        .apply(
            thread,
            7,
            Action::Open {
                url: "https://example.com/form".into(),
            },
        )
        .unwrap();
    let document = opened.tabs[0].document();
    let mut page = PageState {
        navigation: false,
        visible: false,
        document,
        url: opened.tabs[0].url.clone(),
        loading: false,
        can_back: false,
        can_forward: false,
        error: None,
        dirty: Dirty::Clean,
    };
    browser.update(thread, owner, page.clone()).unwrap();
    let lease = browser
        .begin_reclaim(thread, owner, document)
        .unwrap()
        .expect("a hidden Thread's selected page may unload");
    assert!(browser
        .protect(thread, owner, document, Protection::Command)
        .is_err());
    assert!(browser
        .apply(thread, 7, Action::Select { tab: document.tab })
        .is_err());
    let retained = browser.finish_reclaim(thread, owner, lease, false).unwrap();
    assert!(retained.tabs[0].live);
    assert_eq!(retained.tabs[0].document(), document);
    let capture = browser
        .protect(thread, owner, document, Protection::Annotation)
        .unwrap();
    assert!(browser.state(thread).tabs[0].protected);
    assert!(browser
        .protect(thread, owner, document, Protection::Command)
        .is_err());
    assert!(browser
        .begin_reclaim(thread, owner, document)
        .unwrap()
        .is_none());
    browser.release(thread, owner, capture).unwrap();
    page.dirty = Dirty::Unknown;
    browser.update(thread, owner, page.clone()).unwrap();
    assert!(browser
        .begin_reclaim(thread, owner, document)
        .unwrap()
        .is_none());
    page.dirty = Dirty::Clean;
    browser.update(thread, owner, page.clone()).unwrap();
    let lease = browser
        .begin_reclaim(thread, owner, document)
        .unwrap()
        .unwrap();
    let released = browser.finish_reclaim(thread, owner, lease, true).unwrap();
    assert!(!released.tabs[0].live);
    assert!(browser.update(thread, owner, page.clone()).is_err());
    let selected = browser
        .apply(thread, 7, Action::Select { tab: document.tab })
        .unwrap();
    page.document = selected.tabs[0].document();
    page.visible = true;
    browser.update(thread, owner, page.clone()).unwrap();
    assert!(browser
        .begin_reclaim(thread, owner, page.document)
        .unwrap()
        .is_none());
}

#[test]
fn malformed_website_authorities_never_create_tabs() {
    let thread = ThreadId::new(1);
    let mut browser = Service::default();
    browser.attach(thread, 7).unwrap();
    for url in [
        "https://:443/",
        "https://bad host/",
        "https://[::1/",
        "https://user@:443/",
    ] {
        assert!(
            browser
                .apply(thread, 7, Action::Open { url: url.into() })
                .is_err(),
            "invalid website authority: {url}"
        );
        assert!(browser.state(thread).tabs.is_empty());
    }
    browser
        .apply(
            thread,
            7,
            Action::Open {
                url: "http://localhost:3000/page#section".into(),
            },
        )
        .unwrap();
}
