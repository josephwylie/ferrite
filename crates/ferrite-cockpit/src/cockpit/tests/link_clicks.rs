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

fn browser_opened(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    assert_eq!(cx.opened_url(), None, "ordinary web links stay in Ferrite");
    assert!(view.read_with(cx, |view, _| view.panes[0].preview.document().is_some_and(|document| matches!(document.body, ferrite_pane::preview::Body::Browser))));
}

fn browser_closed(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    assert_eq!(cx.opened_url(), None);
    assert!(view.read_with(cx, |view, _| !view.panes[0].preview.is_open()), "selection and context menus do not open a browser");
}

#[gpui::test]
fn a_plain_click_on_a_link_opens_it(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-click-plain", cx);
    let at = on_link(&view, cx);
    cx.simulate_click(at, gpui::Modifiers::none());
    browser_opened(&view, cx);
}

#[gpui::test]
fn a_click_that_wobbles_a_pixel_still_opens_the_link(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-click-wobble", cx);
    let at = on_link(&view, cx);
    drag(cx, at, at + gpui::point(px(1.), px(0.)));
    browser_opened(&view, cx);
}

#[gpui::test]
fn a_click_on_a_link_opens_it_while_other_text_is_highlighted(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-click-highlighted", cx);
    let (from, to) = (caret(&view, cx, 0, 0), caret(&view, cx, 0, 11));
    drag(cx, from, to);
    assert_eq!(copied(cx).as_deref(), Some("Plain words"));
    browser_closed(&view, cx);

    let at = on_link(&view, cx);
    cx.simulate_click(at, gpui::Modifiers::none());
    browser_opened(&view, cx);
    assert_eq!(copied(cx), None, "the click let go of the old highlight");
}

#[gpui::test]
fn a_click_on_a_highlighted_link_opens_it(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-click-inside", cx);
    let (from, to) = (caret(&view, cx, 0, 0), caret(&view, cx, 1, 20));
    drag(cx, from, to);
    assert!(copied(cx).is_some_and(|text| text.contains("example")));
    browser_closed(&view, cx);

    let at = on_link(&view, cx);
    cx.simulate_click(at, gpui::Modifiers::none());
    browser_opened(&view, cx);
}

#[gpui::test]
fn a_drag_that_starts_on_a_link_selects_rather_than_opens(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-drag-from", cx);
    let (from, to) = (on_link(&view, cx), caret(&view, cx, 1, 20));
    drag(cx, from, to);
    browser_closed(&view, cx);
    assert!(copied(cx).is_some(), "the drag selected text");
}

#[gpui::test]
fn a_drag_that_ends_on_a_link_selects_rather_than_opens(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-drag-to", cx);
    let (from, to) = (caret(&view, cx, 0, 0), on_link(&view, cx));
    drag(cx, from, to);
    browser_closed(&view, cx);
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
    browser_opened(&view, cx);
}

#[gpui::test]
fn a_tap_on_a_link_opens_it_while_other_text_is_highlighted(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-tap-highlighted", cx);
    let (from, to) = (caret(&view, cx, 0, 0), caret(&view, cx, 0, 11));
    drag(cx, from, to);
    assert_eq!(copied(cx).as_deref(), Some("Plain words"));

    let at = on_link(&view, cx);
    tap(cx, at);
    browser_opened(&view, cx);
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
    browser_opened(&view, cx);
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
    browser_opened(&view, cx);
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
    browser_opened(&view, cx);
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
        view.read_with(cx, |view, cx| view.files.read(cx).active_path(view.cockpit.read(cx).thread_key(view.panes[0].thread().unwrap()))),
        Some(file),
        "the card opened its reader"
    );
    assert_eq!(copied(cx), None, "the tap let go of the old highlight");
}

fn right_click(at: Point<Pixels>, cx: &mut gpui::VisualTestContext) {
    cx.simulate_mouse_down(at, MouseButton::Right, gpui::Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Right, gpui::Modifiers::none());
    tick(cx);
}

fn menu_row(index: usize, cx: &mut gpui::VisualTestContext) {
    let row = debug_bounds(cx, format!("answer-menu-row-{index}")).expect("a menu row");
    cx.simulate_click(row.center(), gpui::Modifiers::none());
    tick(cx);
}

/// A right-click on a link opens its own menu, and not the message's nor
/// the Pane's: the two browsers, then Copy, which copies the address.
#[gpui::test]
fn a_right_click_on_a_link_offers_the_links_actions(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-menu", cx);
    let at = on_link(&view, cx);
    right_click(at, cx);
    assert!(cx.debug_bounds("answer-menu").is_some(), "a menu opens");
    assert!(cx.debug_bounds("answer-menu-row-2").is_some());
    assert!(
        cx.debug_bounds("answer-menu-row-3").is_none(),
        "a web link has two browsers and Copy"
    );
    browser_closed(&view, cx);
    menu_row(2, cx);
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()).as_deref(),
        Some(URL),
        "the address, not the label"
    );
}

#[gpui::test]
fn escape_closes_the_links_menu(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-menu-escape", cx);
    bind_production_keys(cx);
    let at = on_link(&view, cx);
    right_click(at, cx);
    assert!(cx.debug_bounds("answer-menu").is_some(), "a menu opens");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(cx.debug_bounds("answer-menu").is_none(), "esc closes it");
    assert_eq!(cx.opened_url(), None, "and opens nothing");
}

#[gpui::test]
fn the_links_menu_chooses_the_browser(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-menu-doors", cx);
    let at = on_link(&view, cx);
    right_click(at, cx);
    menu_row(0, cx);
    browser_opened(&view, cx);

    let at = on_link(&view, cx);
    right_click(at, cx);
    menu_row(1, cx);
    assert_eq!(cx.opened_url().as_deref(), Some(URL), "the system's browser");
}

/// ⌘-click (Ctrl elsewhere) is the system's browser, as on the PR badge.
#[gpui::test]
fn a_command_click_on_a_link_opens_the_system_browser(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-click-command", cx);
    let at = on_link(&view, cx);
    let secondary = if cfg!(target_os = "macos") {
        gpui::Modifiers::command()
    } else {
        gpui::Modifiers::control()
    };
    cx.simulate_click(at, secondary);
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
    assert!(view.read_with(cx, |view, _| !view.panes[0].preview.is_open()));
}

/// A link activated by key is a click with no modifier: the primary door.
#[test]
fn a_key_press_is_the_primary_door() {
    let key = gpui::ClickEvent::Keyboard(gpui::KeyboardClickEvent {
        button: gpui::KeyboardButton::Enter,
        bounds: gpui::Bounds::default(),
    });
    assert_eq!(ferrite_pane::link_target::Open::of(&key), ferrite_pane::link_target::Open::Primary);
}

/// An answer with `#212` in a checkout whose origin is `origin`, if any:
/// the click and the menu agree on the address, and the renderer's own
/// `issue:` never reaches the OS.
fn issue_answer<'a>(
    name: &str,
    origin: Option<&str>,
    cx: &'a mut TestAppContext,
) -> (gpui::Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let (core, fake, checkout) = bound_cockpit(name, Provider::Claude);
    if let Some(origin) = origin {
        std::fs::create_dir_all(checkout.join(".git")).unwrap();
        std::fs::write(
            checkout.join(".git/config"),
            format!("[remote \"origin\"]\n\turl = {origin}\n"),
        )
        .unwrap();
    }
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Fixed in #212 already.\n\n".into(),
        })
        .unwrap();
    tick(cx);
    (view, cx)
}

/// The PR the Panel shows beside the conversation, if it shows one.
fn shown_pr(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Option<u64> {
    view.read_with(cx, |view, cx| {
        assert!(!view.destination_shown(), "the conversation keeps the board");
        match view.shown_surface(0, cx)? {
            crate::cockpit::surfaces::SurfaceId::PullRequest(number) => Some(number),
            _ => None,
        }
    })
}

fn close_panel(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    hide_panel(view, cx);
    tick(cx);
}

/// `#212` in a GitHub checkout is that repository's PR: a press opens its
/// native detail beside the conversation, not a browser; the menu still
/// offers both browsers, and Copy gives the forge address.
#[gpui::test]
fn a_pr_number_opens_its_native_detail_and_copies_its_forge_address(cx: &mut TestAppContext) {
    let page = "https://github.com/owner/repo/issues/212";
    let (view, cx) = issue_answer("link-issue-origin", Some("git@github.com:owner/repo.git"), cx);
    let at = caret(&view, cx, 0, 11);
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(shown_pr(&view, cx), Some(212));
    browser_closed(&view, cx);
    close_panel(&view, cx);

    let at = caret(&view, cx, 0, 11);
    right_click(at, cx);
    menu_row(3, cx);
    assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()).as_deref(), Some(page));
    let at = caret(&view, cx, 0, 11);
    right_click(at, cx);
    menu_row(2, cx);
    assert_eq!(cx.opened_url().as_deref(), Some(page));
}

/// The repository's own PR address in prose opens the same native detail;
/// another repository's stays a web page.
#[gpui::test]
fn a_pr_address_of_this_repository_opens_its_native_detail(cx: &mut TestAppContext) {
    let (core, fake, checkout) = bound_cockpit("link-pr-address", Provider::Claude);
    std::fs::create_dir_all(checkout.join(".git")).unwrap();
    std::fs::write(
        checkout.join(".git/config"),
        "[remote \"origin\"]\n\turl = https://github.com/owner/repo.git\n",
    )
    .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Opened https://github.com/owner/repo/pull/305 today.\n\nSee https://github.com/other/repo/pull/9 too.\n\n".into(),
        })
        .unwrap();
    tick(cx);
    let at = caret(&view, cx, 0, 12);
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(shown_pr(&view, cx), Some(305));
    assert_eq!(cx.opened_url(), None);
    close_panel(&view, cx);

    let at = caret(&view, cx, 1, 8);
    cx.simulate_click(at, gpui::Modifiers::none());
    browser_opened(&view, cx);
}

/// No origin, no address: the press and the rows say why, and nothing is
/// handed to the OS.
#[gpui::test]
fn an_issue_reference_without_an_origin_is_unavailable_not_issue_colon(cx: &mut TestAppContext) {
    let (view, cx) = issue_answer("link-issue-no-origin", None, cx);
    let at = caret(&view, cx, 0, 11);
    cx.simulate_click(at, gpui::Modifiers::none());
    assert_eq!(cx.opened_url(), None);
    assert!(view.read_with(cx, |view, _| !view.panes[0].preview.is_open()));

    right_click(at, cx);
    menu_row(1, cx);
    assert_eq!(cx.opened_url(), None, "a disabled row opens nothing");
    assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()), None);
}

/// A file chip that names a line opens its reader with the caret there.
#[gpui::test]
fn a_tap_on_a_file_chip_with_a_line_opens_the_reader_at_that_line(cx: &mut TestAppContext) {
    let (core, fake, workspace) = bound_cockpit("link-tap-file-line", Provider::Claude);
    let file = workspace.join("guide.rs");
    std::fs::write(&file, "a\nb\nc\n").unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hide_panel(&view, cx);
    cx.simulate_resize(gpui::size(px(1200.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Read [guide](guide.rs:2) next.\n\n".into(),
        })
        .unwrap();
    tick(cx);
    let card = debug_bounds(cx, format!("file-attachment-{}", file.display()))
        .expect("the file link is a card");
    tap(cx, card.center() + gpui::point(px(1.), px(0.)));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let editor = view.read_with(cx, |view, cx| view.files.read(cx).shown_editor(view.cockpit.read(cx).thread_key(thread)).expect("a code file"));
    editor.update(cx, |editor, cx| {
        editor.type_text("X", cx);
        editor.save(cx);
    });
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "a\nXb\nc\n");
}

/// A link in an answer is a keyboard stop in the transcript's Tab walk,
/// ahead of a later code block's Copy: Tab reaches it from the composer, ⏎
/// opens exactly its address, and the next render leaves focus on it.
#[gpui::test]
fn tab_reaches_a_link_and_enter_opens_it(cx: &mut TestAppContext) {
    fn on(cx: &mut gpui::VisualTestContext, context: &str) -> bool {
        cx.update(|window, _| {
            window
                .context_stack()
                .iter()
                .any(|stack| stack.contains(context))
        })
    }
    let (core, fake) = cockpit("link-tab-enter", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let stream = fake.streams.borrow();
    stream[0]
        .send(SessionEvent::TextDelta {
            text: "Read [the docs](https://example.com/docs) now.\n\n```rust\n    first();\n```"
                .into(),
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    drop(stream);
    tick(cx);
    cx.update(|window, cx| {
        let composer = view.read(cx).panes[0].composer.focus_handle(cx);
        window.focus(&composer, cx);
    });

    cx.simulate_keystrokes("tab");
    assert!(on(cx, "LinkStop"), "Tab from the composer reaches the link first");
    cx.simulate_keystrokes("tab");
    assert!(on(cx, "TranscriptCodeActions"), "then the code block's actions");
    cx.simulate_keystrokes("shift-tab");
    assert!(on(cx, "LinkStop"));

    cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    tick(cx);
    assert!(on(cx, "LinkStop"), "a render does not pull focus back to the composer");

    cx.simulate_keystrokes("enter");
    // The press queues the link for the pump and rings its bell, and the
    // pump answers the ring as soon as the window is idle (ADR 0041): the
    // queue is read as the key lifts, before the pump takes it.
    let urls = cx.update(|window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::KeyUp(gpui::KeyUpEvent {
                keystroke: gpui::Keystroke::parse("enter").unwrap(),
            }),
            cx,
        );
        view.read(cx).panes[0].preview.take_browser_urls()
    });
    assert_eq!(urls, ["https://example.com/docs"]);
    assert_eq!(cx.opened_url(), None, "the link opens inside Ferrite");
}

/// The pointer keeps its own press path: pressing a link opens it and never
/// leaves the keyboard's stop focused, so no focus ring flashes on a click.
#[gpui::test]
fn a_press_on_a_link_leaves_no_stop_focused(cx: &mut TestAppContext) {
    let (view, cx) = linked_answer("link-click-unfocused", cx);
    let at = on_link(&view, cx);
    cx.simulate_click(at, gpui::Modifiers::none());
    browser_opened(&view, cx);
    assert!(!cx.update(|window, _| ferrite_pane::rich::controls_focused(window)));
}
