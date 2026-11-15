//! Citing text from a conversation, as an operator does it: drag across the
//! rendered words, press Cite (or its key), open the chip, write a comment
//! in its field, and see the quote again after a restart or a jump back.
use super::transcript_pointer::{drag, offered};
#[allow(unused_imports)]
use super::*;
use ferrite_core::drafts::Drafts;
use ferrite_core::replies::Source;
use ferrite_pane::composer::Caret;

const PARAGRAPHS: &str = "zero\n\none\n\ntwo\n\nalpha\n\nbravo\n\ncharlie\n\n";

fn conversation<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
    text: &str,
) -> (gpui::Entity<CockpitView>, &'a mut gpui::VisualTestContext) {
    let (view, _sent, cx) = conversation_sent(name, cx, text);
    (view, cx)
}

/// `conversation`, and the prompts the provider is sent.
fn conversation_sent<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
    text: &str,
) -> (
    gpui::Entity<CockpitView>,
    Rc<RefCell<Vec<String>>>,
    &'a mut gpui::VisualTestContext,
) {
    let (core, fake) = cockpit(name, 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta { text: text.into() })
        .unwrap();
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    (view, fake.sent.clone(), cx)
}

fn replies(
    view: &gpui::Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> Vec<ferrite_core::replies::Reply> {
    view.read_with(cx, |view, cx| {
        view.panes[0].composer.read(cx).replies().to_vec()
    })
}

fn cite_alpha(view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    drag(view, cx, (3, 0), (3, 4));
    let cite = cx
        .debug_bounds("selection-reply-quote")
        .expect("Cite draws");
    cx.simulate_click(cite.center(), gpui::Modifiers::none());
    tick(cx);
}

fn open_chip(cx: &mut gpui::VisualTestContext, index: usize) {
    let selector = ["cite-chip-0", "cite-chip-1"][index];
    let chip = cx.debug_bounds(selector).expect("a chip");
    cx.simulate_click(
        gpui::point(chip.left() + px(24.), chip.center().y),
        gpui::Modifiers::none(),
    );
    tick(cx);
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    tick(cx);
}

/// The chip opens to its quote and source, a multiline comment is written
/// in its field (⇧⏎ breaks a line, ⏎ saves), edited again, cancelled with
/// esc, and the whole reply removed.
#[gpui::test]
fn a_citation_is_commented_edited_and_removed_through_its_controls(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-comment", cx, PARAGRAPHS);
    cite_alpha(&view, cx);
    open_chip(cx, 0);
    assert!(cx.debug_bounds("cite-panel").is_some(), "the chip opens");
    assert!(
        cx.debug_bounds("cite-quote").is_some(),
        "to its whole quote"
    );
    assert!(cx.debug_bounds("cite-source").is_some(), "and its source");

    click(cx, "cite-edit");
    assert!(cx.debug_bounds("cite-comment-field").is_some());
    cx.simulate_input("first line");
    cx.simulate_keystrokes("shift-enter");
    cx.simulate_input("second line");
    tick(cx);
    click(cx, "cite-save");
    assert_eq!(
        quotes(&replies(&view, cx)),
        [("alph", "first line\nsecond line")]
    );
    assert!(
        cx.debug_bounds("cite-comment").is_some(),
        "the comment shows"
    );
    assert!(
        cx.debug_bounds("cite-chip-0-comment").is_some(),
        "the chip marks it"
    );

    click(cx, "cite-edit");
    cx.simulate_input(" and more");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(
        cx.debug_bounds("cite-comment-field").is_none(),
        "esc closes the field"
    );
    assert_eq!(
        quotes(&replies(&view, cx)),
        [("alph", "first line\nsecond line")],
        "esc keeps what was saved"
    );

    click(cx, "cite-edit");
    cx.simulate_input(" and more");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(cx.debug_bounds("cite-comment-field").is_none(), "⏎ saves");
    assert_eq!(
        quotes(&replies(&view, cx)),
        [("alph", "first line\nsecond line and more")]
    );

    click(cx, "cite-remove");
    assert!(replies(&view, cx).is_empty());
    assert!(cx.debug_bounds("cite-chip-0").is_none());
}

/// While a comment is being written, a click on the main line takes the
/// keyboard: what is typed next goes to the draft, not into the quote.
#[gpui::test]
fn a_click_on_the_main_line_leaves_the_comment_field(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-main-line", cx, PARAGRAPHS);
    cite_alpha(&view, cx);
    open_chip(cx, 0);
    click(cx, "cite-edit");
    cx.simulate_input("note");

    let line = cx
        .debug_bounds("prompt-editor")
        .or_else(|| cx.debug_bounds("focused-prompt-editor"))
        .expect("the main line is drawn");
    cx.simulate_click(line.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input(" MAINTEXT");
    tick(cx);
    click(cx, "cite-save");

    let main = view.read_with(cx, |view, cx| {
        view.panes[0].composer.read(cx).text().to_string()
    });
    assert_eq!(main, " MAINTEXT");
    assert_eq!(quotes(&replies(&view, cx)), [("alph", "note")]);
}

/// Cite has a key: with a selection standing, even after esc took the
/// button away, the key quotes it.
#[gpui::test]
fn the_cite_key_quotes_the_selection(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-key", cx, PARAGRAPHS);
    drag(&view, cx, (4, 0), (4, 3));
    assert!(offered(&view, cx));
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!offered(&view, cx), "esc dismisses the button");
    cx.simulate_keystrokes("cmd-shift-l");
    tick(cx);
    assert_eq!(quotes(&replies(&view, cx)), [("bra", "")]);
    assert!(replies(&view, cx)[0].source.is_some());
}

/// A citation names its Thread and message, and comes back from disk with
/// both and its comment.
#[gpui::test]
fn a_citation_keeps_its_comment_and_source_across_a_restart(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("cite-restart", 1);
    let thread = core.threads()[0];
    let dir = scratch("cite-restart-prefs");
    std::fs::create_dir_all(&dir).unwrap();
    let prefs = Preferences {
        dir: dir.clone(),
        ..Preferences::ephemeral()
    };
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: PARAGRAPHS.into(),
        })
        .unwrap();
    tick(cx);
    cite_alpha(&view, cx);
    open_chip(cx, 0);
    click(cx, "cite-edit");
    cx.simulate_input("why this?");
    cx.simulate_keystrokes("enter");
    tick(cx);
    let before = replies(&view, cx);
    let source = before[0].source.expect("cited from a message");
    assert_eq!(source.thread, thread);
    view.update(cx, |view, cx| view.save_drafts(cx));
    drop(view);

    let saved = Drafts::load(&dir);
    assert_eq!(
        saved.thread(thread).map(|draft| draft.replies.clone()),
        Some(before.clone())
    );
    let (core, _fake) = cockpit("cite-restart-again", 1);
    assert_eq!(core.threads()[0], thread);
    let prefs = Preferences {
        dir,
        ..Preferences::ephemeral()
    };
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    cx.run_until_parked();
    assert_eq!(replies(&view, cx), before);
    assert!(cx.debug_bounds("cite-chip-0-comment").is_some());
}

/// Go to source scrolls the transcript to the cited message; when that
/// message is not there, the chip says so and keeps the quote.
#[gpui::test]
fn go_to_source_returns_to_the_message_or_says_it_is_gone(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("cite-source", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let stream = fake.streams.borrow()[0].clone();
    stream
        .send(SessionEvent::TextDelta {
            text: PARAGRAPHS.into(),
        })
        .unwrap();
    tick(cx);
    cite_alpha(&view, cx);
    let filler: String = (0..80).map(|n| format!("filler {n}\n\n")).collect();
    stream
        .send(SessionEvent::TextDelta { text: filler })
        .unwrap();
    tick(cx);
    let transcript = view.read_with(cx, |view, _| view.panes[0].transcript().unwrap());
    transcript.update(cx, |transcript, cx| transcript.scroll_to_bottom(cx));
    tick(cx);
    let away = transcript.read_with(cx, |transcript, _| transcript.scroll().offset());

    open_chip(cx, 0);
    click(cx, "cite-source");
    assert!(
        cx.debug_bounds("cite-source-missing").is_none(),
        "the message is there"
    );
    let back = transcript.read_with(cx, |transcript, _| transcript.scroll().offset());
    assert_ne!(back, away, "the transcript scrolled to it");

    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let gone = Source {
        thread,
        block: serde_json::from_str("999999").unwrap(),
    };
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.reply(
                ferrite_core::replies::Reply::new("a lost passage", "").cited_from(gone),
                cx,
            )
        })
    });
    tick(cx);
    open_chip(cx, 1);
    click(cx, "cite-source");
    assert!(
        cx.debug_bounds("cite-source-missing").is_some(),
        "the chip says so"
    );
    assert_eq!(
        quotes(&replies(&view, cx))[1],
        ("a lost passage", ""),
        "and keeps the quote"
    );
}

/// A narrow Pane wraps its chips into rows, but the tray stays bounded and
/// shuts to its count: the line and its field stay on screen.
#[gpui::test]
fn the_tray_stays_bounded_in_a_narrow_pane(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-narrow", cx, PARAGRAPHS);
    for (from, to) in [
        ((3, 0), (3, 2)),
        ((3, 2), (3, 4)),
        ((4, 0), (4, 2)),
        ((4, 0), (4, 5)),
        ((5, 0), (5, 7)),
    ] {
        drag(&view, cx, from, to);
        let cite = cx
            .debug_bounds("selection-reply-quote")
            .unwrap_or_else(|| panic!("Cite draws after {from:?} {to:?}"));
        cx.simulate_click(cite.center(), gpui::Modifiers::none());
        tick(cx);
    }
    assert_eq!(replies(&view, cx).len(), 5);
    cx.simulate_resize(gpui::size(px(650.), px(700.)));
    tick(cx);
    let tray = cx
        .debug_bounds("pending-attachment-body")
        .expect("the tray");
    assert!(
        tray.size.height <= px(ferrite_ui::theme::ATTACH_TRAY_MAX_H) + px(1.),
        "bounded: {tray:?}"
    );
    let editor = cx.debug_bounds("focused-prompt-editor").expect("the line");
    assert!(
        editor.bottom() <= px(700.),
        "the line is on screen: {editor:?}"
    );

    click(cx, "tray-toggle");
    assert!(cx.debug_bounds("pending-attachment-body").is_none(), "shut");
    assert!(cx.debug_bounds("tray-toggle").is_some(), "to its count");
    assert!(cx.debug_bounds("cite-chip-0").is_none());
    click(cx, "tray-toggle");
    assert!(cx.debug_bounds("cite-chip-0").is_some());
}

/// A comment still open in its field when Send is pressed goes with its
/// quote: the operator wrote it, so Send keeps it.
#[gpui::test]
fn send_keeps_a_comment_still_open_in_its_field(cx: &mut TestAppContext) {
    let (view, sent, cx) = conversation_sent("cite-send-open", cx, PARAGRAPHS);
    cite_alpha(&view, cx);
    open_chip(cx, 0);
    click(cx, "cite-edit");
    cx.simulate_input("note to keep");
    let line = cx
        .debug_bounds("focused-prompt-editor")
        .expect("the main line is drawn");
    cx.simulate_click(line.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input("go");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        sent.borrow().last().map(String::as_str),
        Some("> alph\n\n\u{21a9} note to keep\n\ngo")
    );
    assert!(replies(&view, cx).is_empty(), "the draft is sent");
}

/// An open chip's controls stand in the order Tab walks them (paint
/// order): the comment action, then where it came from, then Close, and
/// Remove, which deletes the quote, last.
#[gpui::test]
fn an_open_chips_controls_end_with_remove(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-tab-order", cx, PARAGRAPHS);
    cite_alpha(&view, cx);
    open_chip(cx, 0);
    let at = |cx: &mut gpui::VisualTestContext, selector: &'static str| {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"))
    };
    let (edit, source) = (at(cx, "cite-edit"), at(cx, "cite-source"));
    let (close, remove) = (at(cx, "cite-close"), at(cx, "cite-remove"));
    assert!(edit.top() < source.top(), "comment before source");
    assert!(source.top() < close.top(), "source before Close");
    assert!(close.left() < remove.left(), "Remove after Close");
}

/// Only the field that holds the keyboard shows a caret: the main line's
/// lit one goes dark while the comment field is open and focused, and comes
/// back when the keyboard does.
#[gpui::test]
fn only_the_field_holding_the_keyboard_shows_a_caret(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-one-caret", cx, PARAGRAPHS);
    let main = view.read_with(cx, |view, _| view.panes[0].composer.clone());
    let carets = |cx: &mut gpui::VisualTestContext| {
        main.update(cx, |composer, _| composer.clear_caret_log());
        for _ in 0..40 {
            cx.executor().advance_clock(Duration::from_millis(16));
            cx.run_until_parked();
            cx.update(|window, cx| window.simulate_next_frame(cx));
            cx.run_until_parked();
        }
        main.read_with(cx, |composer, _| {
            composer
                .caret_log()
                .1
                .iter()
                .filter(|(_, caret)| matches!(caret, Some(Caret::Bar(_))))
                .count()
        })
    };
    ferrite_ui::motion::testing::drive();
    cx.update(|window, _| window.activate_window());
    view.update(cx, |view, cx| {
        view.focus_pane(0);
        cx.notify();
    });
    tick(cx);
    assert!(carets(cx) > 0, "the premise: the main line draws a caret");
    cite_alpha(&view, cx);
    open_chip(cx, 0);
    click(cx, "cite-edit");
    assert_eq!(
        carets(cx),
        0,
        "a comment field holds the keyboard, the main line draws no caret"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(carets(cx) > 0, "the main line has it back");
}
