//! The docked browser's chrome keeps one heading and one set of control
//! bounds through its ready, loading, empty and failed states.
use super::machines::browser_core;
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
        view.update(cx, |view, _| {
            !view.cockpit.browser_attach(thread).unwrap().tabs[0].loading
        })
    });
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
    assert_eq!(
        controls(cx),
        ready,
        "the empty address entry keeps the toolbar"
    );
    assert!(
        cx.debug_bounds("browser-notice").is_some(),
        "an empty entry says what to do"
    );
    assert!(cx.debug_bounds("reader-head").is_none());
}
