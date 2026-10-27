//! A link in a transcript opens on a plain click, whatever is highlighted:
//! the operator's "links aren't working consistently, especially if I have
//! something highlighted". A drag that starts or ends on a link selects.

use super::*;

const URL: &str = "https://example.com/";

/// A Thread whose answer is a plain paragraph above a linked one.
fn linked_answer<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (gpui::Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let (core, fake) = cockpit(name, 1);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: format!("Plain words to select here.\n\nOpen [example]({URL}) and read on.\n\n"),
        })
        .unwrap();
    tick(cx);
    (view, cx)
}

/// On the link's label: `Open example and read on.`, two bytes into it.
fn on_link(view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Point<Pixels> {
    caret(view, cx, 1, 7)
}

fn drag(cx: &mut gpui::VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    cx.simulate_mouse_down(from, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, gpui::Modifiers::none());
    tick(cx);
}

fn copied(cx: &mut gpui::VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(String::new())));
    cx.simulate_keystrokes("cmd-c");
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        .filter(|text| !text.is_empty())
}

#[gpui::test]
fn a_plain_click_on_a_link_opens_it(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-click-plain", cx);
    let at = on_link(&view, cx);
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

#[gpui::test]
fn a_click_that_wobbles_a_pixel_still_opens_the_link(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-click-wobble", cx);
    let at = on_link(&view, cx);
    drag(cx, at, at + gpui::point(px(1.), px(0.)));
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

#[gpui::test]
fn a_click_on_a_link_opens_it_while_other_text_is_highlighted(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-click-highlighted", cx);
    let (from, to) = (caret(&view, cx, 0, 0), caret(&view, cx, 0, 11));
    drag(cx, from, to);
    assert_eq!(copied(cx).as_deref(), Some("Plain words"));
    assert_eq!(cx.opened_url(), None);

    let at = on_link(&view, cx);
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(
        cx.opened_url().as_deref(),
        Some(URL),
        "a click on a link opens it whatever is highlighted"
    );
    assert_eq!(copied(cx), None, "the click let go of the old highlight");
}

#[gpui::test]
fn a_click_on_a_highlighted_link_opens_it(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-click-inside", cx);
    let (from, to) = (caret(&view, cx, 0, 0), caret(&view, cx, 1, 20));
    drag(cx, from, to);
    assert!(copied(cx).is_some_and(|text| text.contains("example")));
    assert_eq!(
        cx.opened_url(),
        None,
        "the drag across the link opens nothing"
    );

    let at = on_link(&view, cx);
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

#[gpui::test]
fn a_drag_that_starts_on_a_link_selects_rather_than_opens(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-drag-from", cx);
    let (from, to) = (on_link(&view, cx), caret(&view, cx, 1, 20));
    drag(cx, from, to);
    assert_eq!(cx.opened_url(), None);
    assert!(copied(cx).is_some(), "the drag selected text");
}

#[gpui::test]
fn a_drag_that_ends_on_a_link_selects_rather_than_opens(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-drag-to", cx);
    let (from, to) = (caret(&view, cx, 0, 0), on_link(&view, cx));
    drag(cx, from, to);
    assert_eq!(cx.opened_url(), None);
    assert!(copied(cx).is_some(), "the drag selected text");
}

/// A press and release with no frame between them, as a trackpad's tap
/// delivers them: both dispatch against the paint before the press.
fn tap(cx: &mut gpui::VisualTestContext, at: Point<Pixels>) {
    cx.update(|window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: at,
                modifiers: gpui::Modifiers::none(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                button: MouseButton::Left,
                position: at,
                modifiers: gpui::Modifiers::none(),
                click_count: 1,
            }),
            cx,
        );
    });
    tick(cx);
}

#[gpui::test]
fn a_tap_on_a_link_opens_it(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-tap-plain", cx);
    let at = on_link(&view, cx);
    tap(cx, at);
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

#[gpui::test]
fn a_tap_on_a_link_opens_it_while_other_text_is_highlighted(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-tap-highlighted", cx);
    let (from, to) = (caret(&view, cx, 0, 0), caret(&view, cx, 0, 11));
    drag(cx, from, to);
    assert_eq!(copied(cx).as_deref(), Some("Plain words"));

    let at = on_link(&view, cx);
    tap(cx, at);
    assert_eq!(
        cx.opened_url().as_deref(),
        Some(URL),
        "a tap on a link opens it whatever is highlighted"
    );
    assert_eq!(copied(cx), None, "the tap let go of the old highlight");
}

/// The answer streams on between press and release, into the link's own
/// paragraph: the re-parse makes the paragraph's text anew, and the click
/// still opens the link.
#[gpui::test]
fn a_click_on_a_link_opens_it_while_the_answer_streams(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("link-click-streaming", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let stream = |text: &str| {
        fake.streams.borrow()[0]
            .send(SessionEvent::TextDelta { text: text.into() })
            .unwrap()
    };
    stream(&format!(
        "Plain words to select here.\n\nOpen [example]({URL}) and"
    ));
    tick(cx);
    let at = on_link(&view, cx);
    cx.simulate_mouse_down(at, MouseButton::Left, gpui::Modifiers::none());
    stream(" read on.");
    tick(cx);
    assert_eq!(on_link(&view, cx), at, "the link stays under the pointer");
    cx.simulate_mouse_up(at, MouseButton::Left, gpui::Modifiers::none());
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

/// A double-click on a link takes its word, as on any text: the first click
/// opened it; the second does not let go of the word. (The test platform
/// keeps only the last URL opened, so a second open cannot be seen here;
/// `Inline` records no press with a click count past one.)
#[gpui::test]
fn a_double_click_on_a_link_selects_its_word(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-double-click", cx);
    let at = on_link(&view, cx);
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui::MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: gpui::Modifiers::none(),
        click_count: 2,
    });
    tick(cx);
    assert_eq!(copied(cx).as_deref(), Some("example"));
}

/// A bare URL in an answer is a link like any other.
#[gpui::test]
fn a_click_on_a_bare_url_opens_it(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("link-click-bare", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: format!("See {URL} for more.\n\n"),
        })
        .unwrap();
    tick(cx);
    let at = caret(&view, cx, 0, 8);
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}

/// A file link's card opens its reader on a tap, while other text is
/// highlighted, and lets go of the highlight as a web link does.
#[gpui::test]
fn a_tap_on_a_file_card_opens_it_while_other_text_is_highlighted(cx: &mut TestAppContext) {
    let (core, fake, workspace) = bound_cockpit("link-tap-file-card", Provider::Claude);
    let file = workspace.join("guide.md");
    std::fs::write(&file, "guide\n").unwrap();
    cx.update(|cx| cx.bind_keys([KeyBinding::new("cmd-c", CopySelection, None)]));
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1200.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Plain words to select here.\n\nRead [guide](guide.md) next.\n\n".into(),
        })
        .unwrap();
    tick(cx);
    let (from, to) = (caret(&view, cx, 0, 0), caret(&view, cx, 0, 11));
    drag(cx, from, to);
    assert_eq!(copied(cx).as_deref(), Some("Plain words"));

    let card = debug_bounds(cx, format!("file-attachment-{}", file.display()))
        .expect("the file link is a card");
    tap(cx, card.center() + gpui::point(px(1.), px(0.)));
    assert_eq!(
        view.read_with(cx, |view, _| view.panes[0]
            .preview
            .document()
            .map(|document| document.path)),
        Some(file),
        "the card opened its reader"
    );
    assert_eq!(copied(cx), None, "the tap let go of the old highlight");
}
