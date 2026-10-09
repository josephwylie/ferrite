//! An agent's visual in the transcript (ADR 0013): its row, its live page
//! through the engine seam (the fake engine here), its words in copy, and
//! the fallbacks when there is no engine.
use super::*;
use crate::visual::web::fake::{Call, FakeEngine};
use crate::visual::web::Engine;

const TOOL: &str = "mcp__ferrite__show_visual";

fn visual_input(html: &str) -> serde_json::Value {
    serde_json::json!({
        "title": "Token usage",
        "caption": "Daily cost, last 30 days",
        "html": html,
    })
}

fn draft(id: &str, html: &str) -> SessionEvent {
    SessionEvent::ToolDraft { id: id.into(), name: TOOL.into(), input: visual_input(html) }
}

fn started(id: &str, html: &str) -> SessionEvent {
    SessionEvent::ToolStarted { id: id.into(), name: TOOL.into(), input: visual_input(html) }
}

fn shown(id: &str) -> SessionEvent {
    SessionEvent::ToolCompleted {
        id: id.into(),
        output: "Shown to the operator inline.".into(),
        is_error: false,
        result: ferrite_core::ToolResult::default(),
    }
}

/// The fake engine, installed as the app's.
fn install_fake(cx: &mut TestAppContext) -> Rc<FakeEngine> {
    let engine = Rc::new(FakeEngine::new());
    let dyn_engine: Rc<dyn Engine> = engine.clone();
    cx.update(|cx| crate::visual::engine::Visuals::install(dyn_engine, cx));
    engine
}

#[gpui::test]
fn a_visual_draws_its_call_line_caption_and_live_page(cx: &mut TestAppContext) {
    let engine = install_fake(cx);
    let (core, fake) = cockpit("vis-row", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    fake.streams.borrow()[0].send(started("v1", "<p>hi</p>")).unwrap();
    tick(cx);
    let row = cx.debug_bounds("visual-row").expect("a visual row");
    let frame = cx.debug_bounds("visual-frame").expect("the page's frame");
    assert!(frame.top() > row.top(), "the page hangs under its words");
    assert!(cx.debug_bounds("visual-caption").is_some());
    assert!(cx.debug_bounds("visual-spinner").is_some(), "checking: the working spinner");
    let opened = engine.journal().iter().any(|call| matches!(call, Call::Open(_)));
    assert!(opened, "the page opened: {:?}", engine.journal());
    // The fake page is 360 px tall, plus its 1px frame.
    assert_eq!(frame.size.height, px(362.));

    fake.streams.borrow()[0].send(shown("v1")).unwrap();
    tick(cx);
    assert!(cx.debug_bounds("visual-spinner").is_none(), "shown: the spinner stops");
    assert!(cx.debug_bounds("visual-status").is_none(), "shown says nothing more");
}

#[gpui::test]
fn a_drawing_visual_reloads_as_its_html_streams(cx: &mut TestAppContext) {
    let engine = install_fake(cx);
    let (core, fake) = cockpit("vis-draft", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    fake.streams.borrow()[0].send(draft("v1", "<p>h")).unwrap();
    tick(cx);
    assert!(cx.debug_bounds("visual-status").is_some(), "drawing says so");
    fake.streams.borrow()[0].send(draft("v1", "<p>hi there")).unwrap();
    tick(cx);
    fake.streams.borrow()[0].send(started("v1", "<p>hi there</p>")).unwrap();
    tick(cx);
    let opens = engine.journal().iter().filter(|call| matches!(call, Call::Open(_))).count();
    let html: Vec<_> = engine
        .journal()
        .into_iter()
        .filter_map(|call| match call {
            Call::Html(html) => Some(html),
            _ => None,
        })
        .collect();
    assert_eq!(opens, 1, "one page, reloaded in place");
    assert_eq!(html, ["<p>hi there", "<p>hi there</p>"]);
}

#[gpui::test]
fn without_an_engine_the_row_says_why_and_keeps_its_fallbacks(cx: &mut TestAppContext) {
    // No engine installed: the test build has no Chromium.
    let (core, fake) = cockpit("vis-none", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    fake.streams.borrow()[0].send(started("v1", "<p>hi</p>")).unwrap();
    tick(cx);
    assert!(cx.debug_bounds("visual-row").is_some());
    assert!(cx.debug_bounds("visual-note").is_some(), "why there is no page");
    assert!(cx.debug_bounds("visual-frame").is_none());
    assert!(cx.debug_bounds("visual-open-in-browser").is_some());
    assert!(cx.debug_bounds("visual-copy-html").is_some());
}

#[gpui::test]
fn copy_html_puts_the_page_on_the_clipboard(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("vis-copy", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    fake.streams.borrow()[0].send(started("v1", "<p>copy me</p>")).unwrap();
    tick(cx);
    let copy = cx.debug_bounds("visual-copy-html").expect("copy html");
    cx.simulate_click(copy.center(), gpui::Modifiers::default());
    tick(cx);
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard()).and_then(|item| item.text());
    assert_eq!(clipboard.as_deref(), Some("<p>copy me</p>"));
}

#[gpui::test]
fn a_visual_opens_its_page_once_however_often_it_draws(cx: &mut TestAppContext) {
    let engine = install_fake(cx);
    let (core, fake) = cockpit("vis-once", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(720.), px(1400.)));
    fake.streams.borrow()[0].send(started("v1", "<p>hi</p>")).unwrap();
    fake.streams.borrow()[0].send(shown("v1")).unwrap();
    for _ in 0..5 {
        tick(cx);
    }
    let opens = engine.journal().iter().filter(|call| matches!(call, Call::Open(_))).count();
    assert_eq!(opens, 1, "{:?}", engine.journal());
}

/// A failed call shows no page, and nothing (the row, or the reach pass
/// pre-opening pages near the screen) keeps opening one for it.
#[gpui::test]
fn a_failed_visual_never_opens_a_page(cx: &mut TestAppContext) {
    let engine = install_fake(cx);
    let (core, fake) = cockpit("vis-failed", 1);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(900.)));
    fake.streams.borrow()[0].send(started("v1", "<p>")).unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolCompleted {
            id: "v1".into(),
            output: "The visual did not finish rendering within 45s.".into(),
            is_error: true,
            result: ferrite_core::ToolResult::default(),
        })
        .unwrap();
    for _ in 0..5 {
        tick(cx);
    }
    assert!(cx.debug_bounds("visual-frame").is_none());
    let opens = engine.journal().iter().filter(|call| matches!(call, Call::Open(_))).count();
    assert!(opens <= 1, "opened {opens} pages for a failed visual");
}
