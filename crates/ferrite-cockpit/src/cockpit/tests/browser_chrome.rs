//! The docked browser's chrome keeps one heading and one set of control
//! bounds through its ready, loading, empty and failed states.
use super::machines::{browser_core, served};
use super::*;

#[gpui::test]
fn browser_chrome_keeps_its_geometry_through_loading_empty_and_failure(cx: &mut TestAppContext) {
    use ferrite_pane::visual::{
        engine::Visuals,
        web::fake::{Call, FakeEngine},
    };
    let (core, thread, _) = browser_core("browser-chrome");
    bind_production_keys(cx);
    let engine = std::rc::Rc::new(FakeEngine::new());
    cx.update(|cx| Visuals::install(engine.clone(), cx));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    until(cx, |cx| {
        engine.live_views() == 1 && cx.debug_bounds("browser-preview").is_some()
    });
    until(cx, |cx| {
        view.update(cx, |view, cx| {
            !view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs[0].loading)
        })
    });
    // The window has the Server's word from its message, on its next beat.
    tick(cx);
    let controls = |cx: &mut gpui::VisualTestContext| {
        [
            "browser-back",
            "browser-forward",
            "browser-reload",
            "browser-annotate",
            "browser-address",
            "browser-go",
            "browser-new-tab",
        ]
        .map(|id| {
            cx.debug_bounds(id)
                .unwrap_or_else(|| panic!("{id} is drawn"))
        })
    };
    let ready = controls(cx);
    assert!(
        cx.debug_bounds("reader-head").is_none(),
        "the panel's Browser tab is the only heading"
    );
    assert!(
        cx.debug_bounds("browser-tabs").is_none(),
        "one page needs no second tab row"
    );
    assert!(
        cx.debug_bounds("browser-progress").is_none()
            && cx.debug_bounds("browser-notice").is_none()
    );

    engine.defer_browser_navigation(true);
    let address = ready[4].center();
    cx.simulate_click(address, gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("http://localhost:5173/slow");
    cx.simulate_keystrokes("enter");
    until(cx, |_| {
        engine
            .journal()
            .contains(&Call::BrowserNavigate("http://localhost:5173/slow".into()))
    });
    until(cx, |cx| cx.debug_bounds("browser-progress").is_some());
    assert_eq!(
        controls(cx),
        ready,
        "loading adds no row and moves no control"
    );
    let annotate = cx.debug_bounds("browser-annotate").unwrap();
    cx.simulate_click(annotate.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.debug_bounds("browser-annotation-overlay").is_none(),
        "Annotate waits for a settled page"
    );
    engine.finish_browser_navigations();
    until(cx, |cx| cx.debug_bounds("browser-progress").is_none());
    assert_eq!(controls(cx), ready, "finishing the load moves nothing");

    cx.simulate_click(ready[4].center(), gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("file:///not-a-page");
    cx.simulate_keystrokes("enter");
    until(cx, |cx| cx.debug_bounds("browser-notice").is_some());
    assert_eq!(
        controls(cx),
        ready,
        "a failure is drawn over the page, not above it"
    );

    cx.simulate_click(ready[6].center(), gpui::Modifiers::none());
    tick(cx);
    let strip = cx.debug_bounds("browser-tabs").unwrap().size.height;
    assert_eq!(
        controls(cx),
        ready.map(|mut bounds| {
            bounds.origin.y += strip;
            bounds
        }),
        "the pending tab's strip shifts the toolbar as one"
    );
    assert!(
        cx.debug_bounds("browser-notice").is_some(),
        "an empty entry says what to do"
    );
    assert!(cx.debug_bounds("reader-head").is_none());
}

#[gpui::test]
fn a_pending_new_tab_can_be_left_by_its_page_tab_or_escape(cx: &mut TestAppContext) {
    use ferrite_pane::visual::{engine::Visuals, web::fake::FakeEngine};
    let (core, thread, _) = browser_core("bnt-back");
    bind_production_keys(cx);
    let engine = std::rc::Rc::new(FakeEngine::new());
    cx.update(|cx| Visuals::install(engine.clone(), cx));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    until(cx, |cx| {
        engine.live_views() == 1 && cx.debug_bounds("browser-preview").is_some()
    });
    until(cx, |cx| {
        view.update(cx, |view, cx| {
            !view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs[0].loading)
        })
    });
    page_shown(&view, thread, cx);
    let page = view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs[0].id)
    });
    let page_tab: &'static str = Box::leak(format!("browser-tab-{}", page.0).into_boxed_str());
    assert!(cx.debug_bounds("browser-notice").is_none());

    let new_tab = cx.debug_bounds("browser-new-tab").unwrap();
    cx.simulate_click(new_tab.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.debug_bounds("browser-notice").is_some(),
        "a new tab hides the page"
    );
    let tab = cx
        .debug_bounds(page_tab)
        .expect("the page keeps a tab while a new tab is pending");
    cx.simulate_click(tab.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.debug_bounds("browser-notice").is_none(),
        "the page tab brings the page back"
    );

    cx.simulate_click(new_tab.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("browser-notice").is_some());
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(
        cx.debug_bounds("browser-notice").is_none(),
        "Escape in the address leaves the new tab"
    );
    assert!(cx.debug_bounds("browser-tabs").is_none());
}

pub(super) type Window<'a> = (
    std::rc::Rc<ferrite_pane::visual::web::fake::FakeEngine>,
    Entity<CockpitView>,
    ThreadId,
    &'a mut gpui::VisualTestContext,
);

pub(super) fn browser_window<'a>(cx: &'a mut TestAppContext, name: &str) -> Window<'a> {
    use ferrite_pane::visual::{engine::Visuals, web::fake::FakeEngine};
    let (core, thread, _) = browser_core(name);
    bind_production_keys(cx);
    let engine = std::rc::Rc::new(FakeEngine::new());
    cx.update(|cx| Visuals::install(engine.clone(), cx));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    until(cx, |cx| {
        engine.live_views() == 1 && cx.debug_bounds("browser-preview").is_some()
    });
    until(cx, |cx| {
        view.update(cx, |view, cx| {
            !view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs[0].loading)
        })
    });
    page_shown(&view, thread, cx);
    (engine, view, thread, cx)
}

/// Wait until the window itself has the Server's word that `thread`'s
/// active tab is live and loaded, and has drawn it: its controls act on
/// the page only from then. The word comes as a message, a beat or more
/// after the Server has it.
pub(super) fn page_shown(view: &Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) {
    page_shown_where(view, thread, cx, |_| true);
}

/// `page_shown`, for the page at `url`: after a navigation or a new tab,
/// the window still has the page before it as loaded.
pub(super) fn page_shown_at(view: &Entity<CockpitView>, thread: ThreadId, url: &str, cx: &mut gpui::VisualTestContext) {
    page_shown_where(view, thread, cx, |tab| tab.url == url);
}

fn page_shown_where(
    view: &Entity<CockpitView>,
    thread: ThreadId,
    cx: &mut gpui::VisualTestContext,
    is: impl Fn(&ferrite_core::browser::Tab) -> bool,
) {
    until(cx, |cx| {
        view.read_with(cx, |view, cx| {
            view.browsers
                .shown_for_test(view.cockpit.read(cx).thread_key(thread))
        })
        .is_some_and(|(state, _)| {
            state.tabs.iter().any(|tab| Some(tab.id) == state.active && tab.live && !tab.loading && is(tab))
        })
    });
    tick(cx);
}

fn tab_count(view: &Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) -> usize {
    view.update(cx, |view, cx| view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs.len()))
}

#[gpui::test]
fn a_new_tab_acts_on_no_page_and_cancel_returns_to_the_page(cx: &mut TestAppContext) {
    use ferrite_pane::visual::web::fake::Call;
    let (engine, view, thread, cx) = browser_window(cx, "bnt-cancel");
    let navigations = |engine: &ferrite_pane::visual::web::fake::FakeEngine| {
        engine
            .journal()
            .iter()
            .filter(|call| matches!(call, Call::BrowserNavigate(_)))
            .count()
    };
    let new_tab = cx.debug_bounds("browser-new-tab").unwrap();
    cx.simulate_click(new_tab.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("browser-tabs").is_some());
    let before = navigations(&engine);
    for control in ["browser-back", "browser-forward", "browser-reload", "browser-annotate"] {
        let bounds = cx.debug_bounds(control).unwrap();
        cx.simulate_click(bounds.center(), gpui::Modifiers::none());
        tick(cx);
    }
    assert_eq!(navigations(&engine), before, "no control reaches the hidden page");
    assert!(cx.debug_bounds("browser-annotation-overlay").is_none());

    let cancel = cx.debug_bounds("browser-cancel-new-tab").unwrap();
    cx.simulate_click(cancel.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("browser-tabs").is_none());
    assert!(cx.debug_bounds("browser-notice").is_none(), "the page is back");
    assert_eq!(tab_count(&view, thread, cx), 1);
}

#[gpui::test]
fn the_only_page_closes_from_the_toolbar_and_a_new_tab_needs_a_page_to_show_a_strip(
    cx: &mut TestAppContext,
) {
    let (_engine, view, thread, cx) = browser_window(cx, "bnt-close");
    let close = cx.debug_bounds("browser-close-page").unwrap();
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    until(cx, |cx| tab_count(&view, thread, cx) == 0);
    tick(cx);
    assert!(cx.debug_bounds("browser-notice").is_some());
    let new_tab = cx.debug_bounds("browser-new-tab").unwrap();
    cx.simulate_click(new_tab.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.debug_bounds("browser-tabs").is_none(),
        "with no page there is nothing to tell a new tab apart from"
    );
}

#[gpui::test]
fn a_page_script_dialog_names_its_origin_and_answers_the_page(cx: &mut TestAppContext) {
    use ferrite_pane::visual::web::{
        fake::Call, DialogAnswer, DialogKind, DialogRequest,
    };
    let (engine, _view, _thread, cx) = browser_window(cx, "bnt-dialog");
    let raise = |kind, message: &str| {
        engine.raise_dialog(DialogRequest {
            origin: "http://localhost:5173".into(),
            kind,
            message: message.into(),
        })
    };
    raise(DialogKind::Alert, "Saved");
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_some());
    assert!(cx.debug_bounds("browser-dialog-cancel").is_none());
    let ok = cx.debug_bounds("browser-dialog-ok").unwrap();
    cx.simulate_click(ok.center(), gpui::Modifiers::none());
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_none());

    raise(DialogKind::Confirm, "Discard?");
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_some());
    cx.simulate_keystrokes("escape");
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_none());

    raise(DialogKind::Prompt { default: "Ada".into() }, "Name?");
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_some());
    tick(cx);
    cx.simulate_input("Grace");
    cx.simulate_keystrokes("enter");
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_none());

    let answers: Vec<_> = engine
        .journal()
        .into_iter()
        .filter_map(|call| match call {
            Call::DialogAnswered(answer) => Some(answer),
            _ => None,
        })
        .collect();
    assert_eq!(
        answers,
        [
            DialogAnswer::Accept(String::new()),
            DialogAnswer::Cancel,
            DialogAnswer::Accept("Grace".into())
        ]
    );
}

#[gpui::test]
fn a_page_asking_for_a_new_window_gets_a_tab(cx: &mut TestAppContext) {
    let (engine, view, thread, cx) = browser_window(cx, "bnt-window");
    engine.request_window("http://localhost:5173/popup");
    until(cx, |cx| tab_count(&view, thread, cx) == 2);
    let urls = view.update(cx, |view, cx| {
        view.cockpit
            .update(cx, |cockpit, _| served(cockpit, thread))
            .tabs
            .iter()
            .map(|tab| tab.url.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(urls[1], "http://localhost:5173/popup");
}

#[gpui::test]
fn a_right_click_on_a_link_offers_the_page_menu(cx: &mut TestAppContext) {
    use ferrite_pane::visual::web::PageContext;
    let (engine, view, thread, cx) = browser_window(cx, "bnt-menu");
    engine.right_click(PageContext {
        link: Some("http://localhost:5173/next".into()),
        selection: None,
    });
    until(cx, |cx| view.read_with(cx, |view, _| view.context_menu().is_some()));
    let labels = view.read_with(cx, |view, _| {
        view.context_menu()
            .unwrap()
            .rows
            .iter()
            .flatten()
            .map(|(item, _)| item.label.to_string())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        labels,
        [
            "Open link in new tab",
            "Copy link address",
            "Back",
            "Forward",
            "Reload",
            "Copy page address",
            "Open in system browser"
        ]
    );
    let row = cx.debug_bounds("context-menu-row-0").expect("the first row is drawn");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    until(cx, |cx| tab_count(&view, thread, cx) == 2);
}

fn open_second_page(
    view: &Entity<CockpitView>,
    first: ThreadId,
    url: &str,
    cx: &mut gpui::VisualTestContext,
) -> ThreadId {
    let second = view.update(cx, |view, cx| {
        let second = view
            .cockpit
            .update(cx, |cockpit, _| cockpit.open(Provider::Claude, WorkspaceChoice::Main { checkout: here() }))
            .unwrap();
        view.browser_action(view.cockpit.read(cx).thread_key(second), ferrite_core::browser::Action::Open { url: url.into() }, cx);
        view.focus_thread(first, cx);
        second
    });
    tick(cx);
    second
}

fn alert(message: &str) -> ferrite_pane::visual::web::DialogRequest {
    ferrite_pane::visual::web::DialogRequest {
        origin: "http://localhost:5173".into(),
        kind: ferrite_pane::visual::web::DialogKind::Alert,
        message: message.into(),
    }
}

fn type_in_composer(view: &Entity<CockpitView>, text: &str, cx: &mut gpui::VisualTestContext) {
    let composer = cx.debug_bounds("composer-block").expect("the composer is drawn");
    cx.simulate_click(composer.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input(text);
    tick(cx);
    assert_eq!(composer_text(view, cx), text);
}

#[gpui::test]
fn another_threads_dialog_never_takes_the_composer(cx: &mut TestAppContext) {
    let (engine, view, thread, cx) = browser_window(cx, "bnt-dialog-other");
    let other = open_second_page(&view, thread, "http://localhost:5173/other", cx);
    until(cx, |_| engine.live_views() == 2);
    engine.raise_dialog_at("http://localhost:5173/other", alert("Elsewhere"));
    tick(cx);
    tick(cx);
    assert!(
        cx.debug_bounds("browser-dialog").is_none(),
        "the shown Browser has no dialog"
    );
    type_in_composer(&view, "still mine", cx);

    view.update(cx, |view, cx| view.focus_thread(other, cx));
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_some());
    let ok = cx.debug_bounds("browser-dialog-ok").unwrap();
    cx.simulate_click(ok.center(), gpui::Modifiers::none());
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_none());
}

#[gpui::test]
fn a_dialog_that_arrives_under_an_open_menu_closes_it_and_takes_the_keys(cx: &mut TestAppContext) {
    use crate::cockpit::context_menu::MenuTarget;
    let (engine, view, thread, cx) = browser_window(cx, "bnt-dialog-menu");
    view.update(cx, |view, cx| {
        view.open_context_menu(MenuTarget::Thread(keyed(view, thread, cx)), gpui::point(px(200.), px(200.)), cx)
    });
    tick(cx);
    engine.raise_dialog_at("http://localhost:5173/start", alert("Over a menu"));
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_some());
    tick(cx);
    let (open, held) = cx.update(|window, cx| {
        let view = view.read(cx);
        (view.context_menu().is_some(), view.dialog_holds_focus(window, cx))
    });
    assert!(!open && held, "the dialog closed the menu and holds the keys");
}

#[gpui::test]
fn a_dialog_is_modal_to_its_page_and_two_dialogs_each_wait_for_their_thread(
    cx: &mut TestAppContext,
) {
    let (engine, view, thread, cx) = browser_window(cx, "bnt-dialog-two");
    let other = open_second_page(&view, thread, "http://localhost:5173/other", cx);
    until(cx, |_| engine.live_views() == 2);
    engine.raise_dialog_at("http://localhost:5173/other", alert("Second"));
    engine.raise_dialog_at("http://localhost:5173/start", alert("First"));
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_some());
    tick(cx);
    let held = cx.update(|window, cx| view.read(cx).dialog_holds_focus(window, cx));
    assert!(held, "a dialog takes the keyboard when it appears");
    type_in_composer(&view, "left the page", cx);
    tick(cx);
    assert!(
        cx.debug_bounds("browser-dialog").is_some(),
        "the dialog waits; it did not take the keyboard back"
    );
    let card = cx.debug_bounds("browser-dialog").unwrap();
    cx.simulate_click(card.center(), gpui::Modifiers::none());
    tick(cx);
    let held = cx.update(|window, cx| {
        view.read(cx).dialog_holds_focus(window, cx)
    });
    assert!(held, "a click into the page area hands the dialog the keyboard");
    cx.simulate_keystrokes("enter");
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_none());
    assert_eq!(composer_text(&view, cx), "left the page");

    view.update(cx, |view, cx| view.focus_thread(other, cx));
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_some());
    cx.simulate_keystrokes("enter");
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_none());
}

fn address_text(view: &Entity<CockpitView>, thread: ThreadId, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, cx| {
        view.browser_chrome.read(cx).address_for_test(view.cockpit.read(cx).thread_key(thread), cx)
    })
}

#[gpui::test]
fn an_address_draft_survives_the_pages_own_navigation(cx: &mut TestAppContext) {
    let (_, view, thread, cx) = browser_window(cx, "bnt-draft");
    let address = cx.debug_bounds("browser-address").unwrap();
    cx.simulate_click(address.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("http://localhost:5173/typed-half");
    let document = view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| served(cockpit, thread).tabs[0].document())
    });
    view.update(cx, |view, cx| {
        view.browser_action(
            view.cockpit.read(cx).thread_key(thread),
            ferrite_core::browser::Action::Navigate { document, url: "http://localhost:5173/moved".into() },
            cx,
        )
    });
    until(cx, |cx| {
        view.update(cx, |view, cx| {
            view.cockpit.update(cx, |cockpit, _| {
                served(cockpit, thread).tabs[0].url == "http://localhost:5173/moved"
            })
        })
    });
    tick(cx);
    assert_eq!(address_text(&view, thread, cx), "http://localhost:5173/typed-half");
}

#[gpui::test]
fn a_popup_during_a_new_tab_leaves_what_was_typed_for_the_new_tab(cx: &mut TestAppContext) {
    let (engine, view, thread, cx) = browser_window(cx, "bnt-popup-typing");
    let new_tab = cx.debug_bounds("browser-new-tab").unwrap();
    cx.simulate_click(new_tab.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("http://localhost:5173/half");
    engine.request_window("http://localhost:5173/popup");
    until(cx, |cx| tab_count(&view, thread, cx) == 2);
    tick(cx);
    assert!(cx.debug_bounds("browser-notice").is_some(), "the new tab is still pending");
    assert_eq!(address_text(&view, thread, cx), "http://localhost:5173/half");
    cx.simulate_keystrokes("enter");
    until(cx, |cx| tab_count(&view, thread, cx) == 3);
}

#[gpui::test]
fn retry_reloads_a_failed_page(
    cx: &mut TestAppContext,
) {
    use ferrite_pane::visual::web::fake::Call;
    let (engine, _view, _thread, cx) = browser_window(cx, "bnt-retry");
    let address = cx.debug_bounds("browser-address").unwrap();
    cx.simulate_click(address.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("file:///not-a-page");
    cx.simulate_keystrokes("enter");
    until(cx, |cx| cx.debug_bounds("browser-retry").is_some());
    let navigations = |engine: &ferrite_pane::visual::web::fake::FakeEngine| {
        engine
            .journal()
            .iter()
            .filter(|call| matches!(call, Call::BrowserNavigate(_)))
            .count()
    };
    let before = navigations(&engine);
    let retry = cx.debug_bounds("browser-retry").unwrap();
    cx.simulate_click(retry.center(), gpui::Modifiers::none());
    until(cx, |_| navigations(&engine) > before);
}

fn dialog_holds(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> bool {
    cx.update(|window, cx| view.read(cx).dialog_holds_focus(window, cx))
}

#[gpui::test]
fn closing_the_page_under_a_dialog_gives_the_keyboard_back(cx: &mut TestAppContext) {
    let (engine, view, _thread, cx) = browser_window(cx, "bnt-dialog-close");
    engine.raise_dialog_at("http://localhost:5173/start", alert("Going"));
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_some());
    tick(cx);
    assert!(dialog_holds(&view, cx));
    let close = cx.debug_bounds("browser-close-page").unwrap();
    cx.simulate_click(close.center(), gpui::Modifiers::none());
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_none());
    tick(cx);
    assert!(!dialog_holds(&view, cx), "a closed page's dialog holds nothing");
    type_in_composer(&view, "typed", cx);
}

#[gpui::test]
fn a_dialog_left_on_another_thread_holds_no_keyboard(cx: &mut TestAppContext) {
    let (engine, view, thread, cx) = browser_window(cx, "bnt-dialog-switch");
    let other = open_second_page(&view, thread, "http://localhost:5173/other", cx);
    until(cx, |_| engine.live_views() == 2);
    engine.raise_dialog_at("http://localhost:5173/start", alert("Here"));
    until(cx, |cx| cx.debug_bounds("browser-dialog").is_some());
    tick(cx);
    assert!(dialog_holds(&view, cx));
    view.update(cx, |view, cx| view.focus_thread(other, cx));
    tick(cx);
    tick(cx);
    assert!(cx.debug_bounds("browser-dialog").is_none());
    assert!(!dialog_holds(&view, cx), "a dialog off screen holds nothing");
}
