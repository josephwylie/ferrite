//! A sent Browser annotation remains an inspectable attachment in history.
use super::*;

#[gpui::test]
fn browser_annotation_history_keeps_a_distinct_reviewable_capture(cx: &mut TestAppContext) {
    use ferrite_core::browser::{Document, TabId};
    use ferrite_core::browser_annotations::{
        self, Capture, Drawing, Ink, Point, Selection, Viewport,
    };
    let (core, _fake) = cockpit("ba-history", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1500.), px(900.)));
    let root = std::env::temp_dir().join(format!("ferrite-history-image-{}", std::process::id()));
    let png = include_bytes!("../../../../ferrite-core/tests/fixtures/browser-original.png");
    let annotation = browser_annotations::retain(
        &root,
        Capture {
            document: Document {
                tab: TabId(88),
                generation: 6,
            },
            native_generation: 9,
            url: "https://example.test/earlier-page".into(),
            viewport: Viewport {
                width_css: 1.0,
                height_css: 1.0,
                device_scale: 1.0,
            },
            selection: Selection::Region {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
        },
        png,
        "Align this".into(),
        vec![Drawing::Arrow {
            from: Point { x: 0.1, y: 0.1 },
            to: Point { x: 0.8, y: 0.8 },
            ink: Ink {
                color: "#ff0000".into(),
                width: 0.1,
            },
        }],
    )
    .unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.set("Review this captured page".into(), cx);
            composer.add_browser_annotation(annotation.clone(), cx);
        });
    });
    tick(cx);
    assert!(cx.debug_bounds("browser-annotation-attachment").is_some());
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).is_empty()));
    let card = cx
        .debug_bounds("browser-annotation-attachment")
        .expect("the sent message retains a distinct Browser annotation card");
    cx.simulate_click(card.center(), gpui::Modifiers::none());
    until(cx, |cx| cx.debug_bounds("annotation-stage").is_some());
    assert!(
        cx.debug_bounds("browser-annotation-review").is_some(),
        "review opens the retained capture and drawings without a live browser"
    );
    assert!(cx.debug_bounds("annotation-stage").is_some());
    assert_eq!(std::fs::read(annotation.original()).unwrap(), png);
    std::fs::remove_dir_all(root).unwrap();
}

#[gpui::test]
fn browser_annotation_retained_editor_adds_original_evidence_without_a_live_page(
    cx: &mut TestAppContext,
) {
    use ferrite_core::browser::{Document, TabId};
    use ferrite_core::browser_annotations::{self, Capture, Selection, Viewport};
    let (core, _fake) = cockpit("ba-editor-retained", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1500.), px(900.)));
    tick(cx);
    cx.simulate_input("Start the conversation");
    cx.simulate_keystrokes("enter");
    until(cx, |cx| {
        view.read_with(cx, |view, _| view.panes[0].thread().is_some())
    });
    let root = std::env::temp_dir().join(format!("ferrite-retained-editor-{}", std::process::id()));
    let png = include_bytes!("../../../../ferrite-core/tests/fixtures/browser-original.png");
    let original = browser_annotations::retain(
        &root,
        Capture {
            document: Document {
                tab: TabId(901),
                generation: 44,
            },
            native_generation: 71,
            url: "https://example.test/page-that-is-now-closed".into(),
            viewport: Viewport {
                width_css: 1.,
                height_css: 1.,
                device_scale: 1.,
            },
            selection: Selection::Region {
                x: 0.,
                y: 0.,
                width: 1.,
                height: 1.,
            },
        },
        png,
        String::new(),
        vec![],
    )
    .unwrap();
    view.update(cx, |view, cx| {
        view.panes[0].preview.edit_annotation(original.clone());
        cx.notify();
    });
    ferrite_core::doorbell::until("retained editor must open without any live page", || {
        leap(cx, super::super::user_commands::ENDED_POLL);
        cx.debug_bounds("annotation-editor").is_some()
    });
    let comment = cx.debug_bounds("annotation-comment").unwrap();
    cx.simulate_click(comment.center(), gpui::Modifiers::none());
    cx.simulate_input("Keep this feedback on the closed page");
    let add = cx.debug_bounds("annotation-add").unwrap();
    cx.simulate_click(add.center(), gpui::Modifiers::none());
    until(cx, |cx| {
        cx.debug_bounds("browser-annotation-attachment").is_some()
    });
    let prompt = view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).prompt());
    let (_, _, sent) = browser_annotations::split(prompt);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].capture(), original.capture());
    assert_eq!(sent[0].original_hash(), original.original_hash());
    assert_eq!(sent[0].comment(), "Keep this feedback on the closed page");
    assert_eq!(std::fs::read(original.original()).unwrap(), png);
    std::fs::remove_dir_all(root).unwrap();
}
