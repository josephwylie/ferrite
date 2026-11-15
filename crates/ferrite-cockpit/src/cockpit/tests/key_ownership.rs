//! The focused control owns its keys. Every key here goes through the
//! production binding table and a rendered, focused control, so a binding
//! that takes a key from its owner fails the test.
use super::*;
use crate::cockpit::surfaces::SurfaceId;
use ferrite_pane::visual::web::fake::{Call, FakeEngine};
use ferrite_pane::visual::web::Input;

const SENTINEL: &str = "an unsent draft";

fn sentinel_draft(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| composer.set(SENTINEL.into(), cx));
    });
    tick(cx);
}

fn composer_unchanged(view: &Entity<CockpitView>, fake: &Fake, cx: &mut gpui::VisualTestContext) {
    assert_eq!(composer_text(view, cx), SENTINEL, "the draft is untouched");
    assert!(fake.sent.borrow().is_empty(), "nothing was sent to the provider");
}

#[gpui::test]
fn the_launcher_opens_its_highlighted_tool_on_enter(cx: &mut TestAppContext) {
    let (core, fake, _checkout) = bound_cockpit("key-launcher", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.close_surface(thread, &SurfaceId::Overview, cx));
    tick(cx);
    sentinel_draft(&view, cx);
    let head = cx.debug_bounds("panel-empty-head").expect("the launcher shows");
    cx.simulate_click(head.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_keystrokes("down enter");
    tick(cx);
    assert_eq!(panel_tabs(&view, thread, cx).1, Some(SurfaceId::Terminal(1)));
    composer_unchanged(&view, &fake, cx);
}

#[gpui::test]
fn approval_words_type_into_the_composer_and_answer_nothing(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("key-approval", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0].send(decision("perm_words")).unwrap();
    tick(cx);
    let pending = |view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            view.cockpit.thread(thread).and_then(|open| open.pending()).is_some()
        })
    };
    assert!(pending(&view, cx), "the approval is up");
    for word in ["also", "no", "yes"] {
        view.update(cx, |view, cx| {
            view.panes[0].composer.update(cx, |composer, cx| composer.set(String::new(), cx));
        });
        for letter in word.chars() {
            cx.simulate_keystrokes(&letter.to_string());
        }
        tick(cx);
        assert_eq!(composer_text(&view, cx), word, "{word:?} is typed text");
        assert!(pending(&view, cx), "{word:?} answered nothing");
    }
    assert!(fake.sent.borrow().is_empty());
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| composer.set("with a note".into(), cx));
    });
    cx.simulate_keystrokes("cmd-enter");
    cx.run_until_parked();
    assert!(!pending(&view, cx), "the explicit chord still answers");
}

#[gpui::test]
fn files_keeps_the_keys_typed_into_its_editor_and_filter(cx: &mut TestAppContext) {
    let (core, fake, checkout) = bound_cockpit("key-files", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1900.), px(900.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    sentinel_draft(&view, cx);

    let file = checkout.join("src/lib.rs");
    view.update(cx, |view, cx| view.open_in_files(thread, file.clone(), None, true, cx));
    tick(cx);
    let line = cx.debug_bounds("editor").expect("the editor shows");
    cx.simulate_click(line.origin + gpui::point(px(80.), px(10.)), gpui::Modifiers::none());
    tick(cx);
    for key in ["x", "y", "z"] {
        cx.simulate_keystrokes(key);
        fake.streams.borrow()[0].send(SessionEvent::TextDelta { text: "…".into() }).unwrap();
        tick(cx);
    }
    cx.simulate_keystrokes("cmd-s");
    tick(cx);
    assert!(
        std::fs::read_to_string(&file).unwrap().contains("xyz"),
        "every key stayed in the file"
    );
    composer_unchanged(&view, &fake, cx);

    view.update_in(cx, |view, window, cx| view.go_to_file(thread, window, cx));
    tick(cx);
    for key in ["l", "i", "b"] {
        cx.simulate_keystrokes(key);
        fake.streams.borrow()[0].send(SessionEvent::TextDelta { text: "…".into() }).unwrap();
        tick(cx);
    }
    assert_eq!(view.read_with(cx, |view, cx| view.files_filter_text_for_test(thread, cx)), "lib");
    composer_unchanged(&view, &fake, cx);
}

#[gpui::test]
fn the_browser_page_gets_the_keys_the_host_binds(cx: &mut TestAppContext) {
    let (core, _, _) = super::machines::browser_core("key-browser");
    bind_production_keys(cx);
    let engine = std::rc::Rc::new(FakeEngine::new());
    cx.update(|cx| ferrite_pane::visual::engine::Visuals::install(engine.clone(), cx));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    until(cx, |cx| engine.live_views() == 1 && cx.debug_bounds("browser-preview").is_some());
    sentinel_draft(&view, cx);
    let page = cx.debug_bounds("browser-preview").unwrap();
    cx.simulate_click(page.center(), gpui::Modifiers::none());
    tick(cx);
    let key_downs = || {
        engine
            .journal()
            .iter()
            .filter(|call| matches!(call, Call::Input(Input::Key { down: true, .. })))
            .count()
    };
    for key in ["enter", "tab", "shift-tab", "escape", "cmd-v", "cmd-c"] {
        let before = key_downs();
        cx.simulate_keystrokes(key);
        tick(cx);
        assert_eq!(key_downs(), before + 1, "{key} reached the page");
    }
    assert_eq!(composer_text(&view, cx), SENTINEL, "the draft is untouched");
}
