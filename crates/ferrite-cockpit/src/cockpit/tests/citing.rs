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

/// Pasted, not typed: one edit, where typing it is thousands of keystrokes
/// and a frame for each. `cmd-v` on every OS: these tests bind the mac key
/// table everywhere.
fn paste(cx: &mut gpui::VisualTestContext, text: String) {
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(text)));
    cx.simulate_keystrokes("cmd-v");
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
    assert!(
        cx.debug_bounds("cite-comment-field").is_some(),
        "Cite opens the comment popover directly"
    );
    cx.simulate_keystrokes("enter");
    tick(cx);
}

fn open_comment(cx: &mut gpui::VisualTestContext, index: usize) {
    if cx.debug_bounds("cite-comment-field").is_some() {
        cx.simulate_keystrokes("escape");
        tick(cx);
    }
    click(
        cx,
        ["cite-chip-0-comment-trigger", "cite-chip-1-comment-trigger"][index],
    );
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
    open_comment(cx, 0);
    assert!(
        cx.debug_bounds("cite-panel").is_some(),
        "the pencil opens the editor"
    );
    assert!(
        cx.debug_bounds("cite-quote").is_none(),
        "no inspection step"
    );
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
    open_comment(cx, 0);
    assert!(
        cx.debug_bounds("cite-chip-0-comment").is_some(),
        "the chip marks it"
    );

    open_comment(cx, 0);
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

    open_comment(cx, 0);
    cx.simulate_input(" and more");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(cx.debug_bounds("cite-comment-field").is_none(), "⏎ saves");
    assert_eq!(
        quotes(&replies(&view, cx)),
        [("alph", "first line\nsecond line and more")]
    );

    open_comment(cx, 0);
    click(cx, "cite-chip-0-remove");
    assert!(replies(&view, cx).is_empty());
    assert!(cx.debug_bounds("cite-chip-0").is_none());
}

/// While a comment is being written, a click on the main line takes the
/// keyboard: what is typed next goes to the draft, not into the quote.
#[gpui::test]
fn a_click_on_the_main_line_leaves_the_comment_field(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-main-line", cx, PARAGRAPHS);
    cite_alpha(&view, cx);
    open_comment(cx, 0);
    cx.simulate_input("note");

    let line = cx
        .debug_bounds("prompt-editor")
        .or_else(|| cx.debug_bounds("focused-prompt-editor"))
        .expect("the main line is drawn");
    cx.simulate_click(line.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_input(" MAINTEXT");
    tick(cx);
    assert!(
        cx.debug_bounds("cite-panel").is_none(),
        "outside press closes and saves the comment"
    );

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
    open_comment(cx, 0);
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

    click(cx, "cite-chip-0-source");
    assert!(
        cx.debug_bounds("cite-source-missing").is_none(),
        "the message is there"
    );
    for _ in 0..8 {
        tick(cx);
    }
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
    click(cx, "cite-chip-1-source");
    assert!(view.read_with(cx, |view, cx| {
        view.panes[0].composer.read(cx).is_source_missing(1)
    }));
    assert!(
        cx.debug_bounds("cite-chip-1-source").is_some(),
        "the source can be retried"
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
        cx.simulate_keystrokes("enter");
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
    open_comment(cx, 0);
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

#[gpui::test]
fn citation_editor_controls_place_cancel_before_save(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-tab-order", cx, PARAGRAPHS);
    cite_alpha(&view, cx);
    open_comment(cx, 0);
    let cancel = cx.debug_bounds("cite-cancel").unwrap();
    let save = cx.debug_bounds("cite-save").unwrap();
    assert!(cancel.left() < save.left());
    assert_eq!(cancel.top(), save.top());
    assert!(cx.debug_bounds("cite-source").is_none());
    assert!(cx.debug_bounds("cite-close").is_none());
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
    open_comment(cx, 0);
    assert_eq!(
        carets(cx),
        0,
        "a comment field holds the keyboard, the main line draws no caret"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(carets(cx) > 0, "the main line has it back");
}

/// A sent prompt's Quote row keeps where it was cited from, on Ferrite's
/// side (the provider is sent the quote alone): Go to source scrolls back to
/// that message, the file keeps it for the next launch, and a message that
/// is gone is said so on the row.
#[gpui::test]
fn a_sent_quote_offers_go_to_source(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("cite-sent", 1);
    let thread = core.threads()[0];
    let dir = scratch("cite-sent-prefs");
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
    stream
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    let line = cx.debug_bounds("focused-prompt-editor").expect("the line");
    cx.simulate_click(line.center(), gpui::Modifiers::none());
    cx.simulate_input("go on");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        fake.sent.borrow().last().map(String::as_str),
        Some("> alph\n\ngo on"),
        "the provider is sent the quote alone"
    );
    assert!(
        cx.debug_bounds("quote-row-0").is_some(),
        "the sent quote is drawn"
    );
    let kept = ferrite_core::cited_sources::CitedSources::load(&dir);
    assert_eq!(kept.of(thread).map(|sources| sources.len()), Some(1));

    let transcript = view.read_with(cx, |view, _| view.panes[0].transcript().unwrap());
    let away = transcript.read_with(cx, |transcript, _| transcript.scroll().offset());
    click(cx, "quote-row-0-go");
    let back = transcript.read_with(cx, |transcript, _| transcript.scroll().offset());
    assert_ne!(back, away, "the transcript scrolled to the cited message");
}

/// A sent quote whose message is no longer in the transcript says so.
#[gpui::test]
fn a_sent_quote_says_when_its_source_is_gone(cx: &mut TestAppContext) {
    let (view, sent, cx) = conversation_sent("cite-sent-gone", cx, PARAGRAPHS);
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
    let line = cx.debug_bounds("focused-prompt-editor").expect("the line");
    cx.simulate_click(line.center(), gpui::Modifiers::none());
    cx.simulate_input("go on");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(sent.borrow().last().is_some());
    click(cx, "quote-row-0-go");
    assert!(cx.debug_bounds("quote-row-0-go").is_none());
    assert!(
        cx.debug_bounds("quote-row-0-source").is_some(),
        "it says so"
    );
}

/// A source the transcript does not hold yet is "unavailable" only until it
/// does: when the Block turns up (earlier history arriving), the quote offers
/// Go to source again.
#[gpui::test]
fn a_source_that_arrives_later_is_no_longer_unavailable(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("cite-sent-late", 1);
    let thread = core.threads()[0];
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let stream = fake.streams.borrow()[0].clone();
    let answer = |text: &str| {
        stream
            .send(SessionEvent::TextDelta { text: text.into() })
            .unwrap();
        stream
            .send(SessionEvent::TurnEnded {
                outcome: ferrite_core::TurnOutcome::Completed,
                cost_usd: None,
            })
            .unwrap();
    };
    answer(PARAGRAPHS);
    tick(cx);
    cite_alpha(&view, cx);
    let cited = replies(&view, cx)[0].source.expect("cited").block;
    let id: u64 = serde_json::to_string(&cited).unwrap().parse().unwrap();
    let later: ferrite_core::transcript::BlockId =
        serde_json::from_str(&(id + 20).to_string()).unwrap();
    let reply = ferrite_core::replies::Reply::new("a passage to come", "").cited_from(Source {
        thread,
        block: later,
    });
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.reply(reply, cx))
    });
    tick(cx);
    let line = cx.debug_bounds("focused-prompt-editor").expect("the line");
    cx.simulate_click(line.center(), gpui::Modifiers::none());
    cx.simulate_input("go on");
    cx.simulate_keystrokes("enter");
    tick(cx);
    click(cx, "quote-row-1-go");
    assert!(
        cx.debug_bounds("quote-row-1-go").is_none(),
        "it says it is unavailable"
    );
    let transcript = view.read_with(cx, |view, _| view.panes[0].transcript().unwrap());
    for round in 0..30 {
        if transcript.read_with(cx, |transcript, _| transcript.holds_block(later)) {
            break;
        }
        let _ = view.update(cx, |view, _| {
            view.cockpit.send(thread, format!("more {round}"))
        });
        answer(&format!("answer {round}\n\n"));
        tick(cx);
    }
    tick(cx);
    assert!(
        transcript.read_with(cx, |transcript, _| transcript.holds_block(later)),
        "the block arrived"
    );
    transcript.update(cx, |transcript, cx| {
        transcript.scroll_to(ferrite_pane::transcript::ScrollTarget::Top, cx)
    });
    tick(cx);
    assert!(
        cx.debug_bounds("quote-row-1-go").is_some(),
        "no longer unavailable: Go to source is offered again"
    );
}

/// A prompt, then an answer of `lines` paragraphs, the last one cited and
/// saved. The pinned band covers the top of the view.
fn cited_answer<'a>(
    name: &str,
    lines: usize,
    cx: &'a mut TestAppContext,
) -> (
    gpui::Entity<ferrite_pane::transcript::TranscriptView>,
    ferrite_core::transcript::BlockId,
    &'a mut gpui::VisualTestContext,
) {
    let (mut core, fake) = cockpit(name, 1);
    let thread = core.threads()[0];
    let _ = core.send(thread, "Explain the lines".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let stream = fake.streams.borrow()[0].clone();
    stream
        .send(SessionEvent::TextDelta {
            text: (0..lines).map(|n| format!("Entry {n:02}\n\n")).collect(),
        })
        .unwrap();
    stream
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    tick(cx);
    drag(&view, cx, (lines, 0), (lines, 8));
    let cite = cx
        .debug_bounds("selection-reply-quote")
        .expect("Cite draws");
    cx.simulate_click(cite.center(), gpui::Modifiers::none());
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(
        cx.debug_bounds("cite-panel").is_none(),
        "save the citation before navigating"
    );
    let block = replies(&view, cx)[0].source.expect("cited").block;
    let transcript = view.read_with(cx, |view, _| view.panes[0].transcript().unwrap());
    (transcript, block, cx)
}

/// What the cited row shows of itself, against the pinned band: its top
/// and the foot of the band, both in window space.
fn row_against_band(
    transcript: &gpui::Entity<ferrite_pane::transcript::TranscriptView>,
    block: ferrite_core::transcript::BlockId,
    cx: &mut gpui::VisualTestContext,
) -> (gpui::Pixels, gpui::Pixels, gpui::Pixels) {
    transcript.read_with(cx, |transcript, _| {
        let scroll = transcript.scroll();
        let row = transcript
            .block_row_bounds(block)
            .expect("the row is laid out");
        let viewport = scroll.bounds();
        (
            row.top(),
            viewport.top() + scroll.top_mask(),
            viewport.bottom(),
        )
    })
}

/// Where the landed passage is, against the band and the view's foot.
fn passage_against_view(
    transcript: &gpui::Entity<ferrite_pane::transcript::TranscriptView>,
    cx: &mut gpui::VisualTestContext,
) -> (
    Option<gpui::Bounds<gpui::Pixels>>,
    gpui::Pixels,
    gpui::Pixels,
) {
    transcript.read_with(cx, |transcript, _| {
        let scroll = transcript.scroll();
        let viewport = scroll.bounds();
        (
            transcript.cited_painted(),
            viewport.top() + scroll.top_mask(),
            viewport.bottom(),
        )
    })
}

/// Go to source brings a cited paragraph far below the top of a long answer
/// into the reading band, marked, not the answer's top.
#[gpui::test]
fn go_to_source_lands_the_cited_passage_in_view(cx: &mut TestAppContext) {
    let (transcript, block, cx) = cited_answer("cite-passage-in-view", 40, cx);
    transcript.update(cx, |transcript, cx| {
        transcript.scroll_to(ferrite_pane::transcript::ScrollTarget::Top, cx)
    });
    tick(cx);
    let (top, _, bottom) = row_against_band(&transcript, block, cx);
    assert!(top < bottom, "the answer is on screen");
    click(cx, "cite-chip-0-source");
    for _ in 0..4 {
        tick(cx);
    }
    let (found, floor, bottom) = passage_against_view(&transcript, cx);
    let found = found.expect("the passage is marked and has painted");
    assert!(
        found.top() >= floor && found.bottom() <= bottom,
        "the passage {found:?} lies between the band's foot {floor:?} and the view's foot {bottom:?}"
    );
}

/// A passage already fully readable does not move, whatever is above it.
#[gpui::test]
fn go_to_source_leaves_a_visible_passage_where_it_is(cx: &mut TestAppContext) {
    let (transcript, block, cx) = cited_answer("cite-passage-in-place", 40, cx);
    let (top, floor, _) = row_against_band(&transcript, block, cx);
    assert!(
        top < floor,
        "the answer's top is under the band; the passage is in view"
    );
    let before = transcript.read_with(cx, |transcript, _| transcript.scroll().offset());
    click(cx, "cite-chip-0-source");
    for _ in 0..4 {
        tick(cx);
    }
    let after = transcript.read_with(cx, |transcript, _| transcript.scroll().offset());
    assert_eq!(after, before, "the view did not move");
    let (found, floor, bottom) = passage_against_view(&transcript, cx);
    let found = found.expect("the passage is marked");
    assert!(found.top() >= floor && found.bottom() <= bottom);
}

/// A wheel straight after Go to source belongs to the operator: the
/// landing never pulls the row back under the band.
#[gpui::test]
fn a_wheel_right_after_go_to_source_is_not_undone(cx: &mut TestAppContext) {
    let (transcript, _, cx) = cited_answer("cite-then-wheel", 40, cx);
    transcript.update(cx, |transcript, cx| {
        transcript.scroll_to(ferrite_pane::transcript::ScrollTarget::Top, cx)
    });
    tick(cx);
    click(cx, "cite-chip-0-source");
    let viewport = transcript.read_with(cx, |transcript, _| transcript.scroll().bounds());
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-150.))),
        ..Default::default()
    });
    let held = transcript.read_with(cx, |transcript, _| transcript.scroll().offset());
    cx.run_until_parked();
    for _ in 0..4 {
        tick(cx);
    }
    let (found, ..) = passage_against_view(&transcript, cx);
    assert!(
        found.is_none(),
        "the wheel dropped the landing and its mark"
    );
    let after = transcript.read_with(cx, |transcript, _| transcript.scroll().offset());
    assert_eq!(after, held, "the landing did not pull the view back");
}

/// The pinned band follows the turn on screen: from a later turn's short
/// prompt, Go to source lands an earlier answer below that earlier turn's
/// taller band, which is not the one the jump started under.
#[gpui::test]
fn go_to_source_clears_the_band_of_the_turn_it_lands_in(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("cite-other-turn", 1);
    let thread = core.threads()[0];
    let long = "A long first prompt that wraps over several lines of the pane. ".repeat(12);
    let _ = core.send(thread, long);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    let stream = fake.streams.borrow()[0].clone();
    let answer = |label: &str| SessionEvent::TextDelta {
        text: (0..40).map(|n| format!("{label} {n}\n\n")).collect(),
    };
    let ended = || SessionEvent::TurnEnded {
        outcome: ferrite_core::TurnOutcome::Completed,
        cost_usd: None,
    };
    stream.send(answer("First")).unwrap();
    stream.send(ended()).unwrap();
    tick(cx);
    let _ = view.update(cx, |view, _| view.cockpit.send(thread, "Next".into()));
    stream.send(answer("Second")).unwrap();
    stream.send(ended()).unwrap();
    tick(cx);
    let transcript = view.read_with(cx, |view, _| view.panes[0].transcript().unwrap());
    transcript.update(cx, |transcript, cx| {
        transcript.scroll_to(ferrite_pane::transcript::ScrollTarget::Top, cx)
    });
    tick(cx);
    drag(&view, cx, (5, 0), (5, 7));
    let cite = cx
        .debug_bounds("selection-reply-quote")
        .expect("Cite draws");
    cx.simulate_click(cite.center(), gpui::Modifiers::none());
    tick(cx);
    transcript.update(cx, |transcript, cx| transcript.scroll_to_bottom(cx));
    tick(cx);
    let later_floor = transcript.read_with(cx, |transcript, _| transcript.scroll().top_mask());
    click(cx, "cite-chip-0-source");
    for _ in 0..4 {
        tick(cx);
    }
    let (found, floor, bottom) = passage_against_view(&transcript, cx);
    let found = found.expect("the passage is marked and has painted");
    assert!(
        found.top() >= floor && found.bottom() <= bottom,
        "the passage {found:?} is visible at or below its turn's band {floor:?} (was {later_floor:?} from the later turn)"
    );
    let mask = transcript.read_with(cx, |transcript, _| transcript.scroll().top_mask());
    assert!(mask > later_floor, "the earlier turn's band is the taller");
}

#[gpui::test]
fn cite_opens_an_anchored_comment_popover_without_growing_the_tray(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-popover", cx, PARAGRAPHS);
    let line_before = cx.debug_bounds("focused-prompt-editor").unwrap();
    drag(&view, cx, (3, 0), (3, 4));
    click(cx, "selection-reply-quote");
    let popup = cx.debug_bounds("cite-panel").expect("Cite opens a popover");
    assert_eq!(popup.size.width, px(320.));
    assert!(
        cx.debug_bounds("cite-quote").is_none(),
        "the popup opens directly to the editor"
    );
    assert!(
        cx.debug_bounds("cite-edit").is_none(),
        "there is no second Add comment step"
    );
    let field = cx.debug_bounds("cite-comment-field").unwrap();
    assert!(field.size.height >= px(64.));
    assert_eq!(cx.debug_bounds("cite-save").unwrap().size.height, px(28.));
    assert!(
        popup.bottom() < line_before.top(),
        "the popup floats by the selected answer"
    );
    cx.simulate_input("This belongs to the selected text");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(
        cx.debug_bounds("cite-panel").is_none(),
        "saving closes the popup"
    );
    assert_eq!(
        quotes(&replies(&view, cx)),
        [("alph", "This belongs to the selected text")]
    );
    let tray = cx.debug_bounds("pending-attachment-body").unwrap();
    assert!(
        tray.size.height <= px(40.),
        "the tray holds the chip, not the popup"
    );
    click(cx, "cite-chip-0-comment-trigger");
    let popup = cx.debug_bounds("cite-panel").unwrap();
    let chip = cx.debug_bounds("cite-chip-0").unwrap();
    assert!(
        popup.bottom() <= chip.top(),
        "reopened comment hangs above its chip"
    );
    cx.simulate_input(" discard");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(
        quotes(&replies(&view, cx)),
        [("alph", "This belongs to the selected text")]
    );
}

#[gpui::test]
fn citation_comment_limit_blocks_save_send_and_outside_dismissal(cx: &mut TestAppContext) {
    let (view, sent, cx) = conversation_sent("cite-comment-limit", cx, PARAGRAPHS);
    cite_alpha(&view, cx);
    open_comment(cx, 0);
    paste(cx, "😀".repeat(4_001));
    tick(cx);
    assert!(cx.debug_bounds("cite-comment-too-long").is_some());
    cx.simulate_keystrokes("enter");
    cx.simulate_keystrokes("cmd-enter");
    tick(cx);
    assert!(sent.borrow().is_empty());
    assert_eq!(replies(&view, cx)[0].text, "");
    let line = cx
        .debug_bounds("focused-prompt-editor")
        .or_else(|| cx.debug_bounds("prompt-editor"))
        .unwrap();
    cx.simulate_click(line.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.debug_bounds("cite-panel").is_some(),
        "an invalid draft remains open"
    );
    click(cx, "cite-cancel");
    assert!(cx.debug_bounds("cite-panel").is_none());
    assert_eq!(replies(&view, cx)[0].text, "");
}

#[gpui::test]
fn command_enter_saves_the_comment_and_sends_the_prompt(cx: &mut TestAppContext) {
    let (view, sent, cx) = conversation_sent("cite-comment-send", cx, PARAGRAPHS);
    cite_alpha(&view, cx);
    open_comment(cx, 0);
    cx.simulate_input("Please explain this");
    cx.simulate_keystrokes("cmd-enter");
    tick(cx);
    assert_eq!(
        sent.borrow().last().map(String::as_str),
        Some("> alph\n\n↩ Please explain this")
    );
    assert!(cx.debug_bounds("cite-panel").is_none());
}

#[gpui::test]
fn citation_body_navigates_without_opening_an_inspection_popup(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-direct-source", cx, PARAGRAPHS);
    cite_alpha(&view, cx);
    click(cx, "cite-chip-0-source");
    assert!(cx.debug_bounds("cite-panel").is_none());
    assert_eq!(replies(&view, cx)[0].quote, "alph");
}

#[gpui::test]
fn cancelling_a_new_citation_removes_it(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-cancel-new", cx, PARAGRAPHS);
    drag(&view, cx, (3, 0), (3, 4));
    click(cx, "selection-reply-quote");
    cx.simulate_input("discard this");
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(replies(&view, cx).is_empty());
    assert!(cx.debug_bounds("cite-panel").is_none());
}

#[gpui::test]
fn citation_popup_follows_the_source_after_scrolling(cx: &mut TestAppContext) {
    let text = (0..35)
        .map(|n| format!("Paragraph {n:02} with enough words to identify it.\n\n"))
        .collect::<String>();
    let (view, cx) = conversation("cite-moving-source", cx, &text);
    let transcript = view.read_with(cx, |view, _| view.panes[0].transcript().unwrap());
    transcript.update(cx, |transcript, cx| {
        transcript.scroll_to(ferrite_pane::transcript::ScrollTarget::Top, cx)
    });
    tick(cx);
    drag(&view, cx, (5, 0), (5, 12));
    click(cx, "selection-reply-quote");
    let before = cx.debug_bounds("cite-panel").unwrap();
    let viewport = transcript.read_with(cx, |transcript, _| transcript.scroll().bounds());
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: gpui::point(viewport.left() + px(20.), viewport.top() + px(20.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-40.))),
        ..Default::default()
    });
    tick(cx);
    let after = cx
        .debug_bounds("cite-panel")
        .expect("source remains mounted");
    assert!(
        after.top() < before.top(),
        "the popup follows native source geometry"
    );
    assert_eq!(replies(&view, cx)[0].quote, "Paragraph 05");
}

#[gpui::test]
fn citation_comment_survives_source_text_appends(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("cite-streaming-source", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: PARAGRAPHS.into(),
        })
        .unwrap();
    tick(cx);
    drag(&view, cx, (3, 0), (3, 4));
    click(cx, "selection-reply-quote");
    cx.simulate_input("keep this draft");
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "more source text\n\n".into(),
        })
        .unwrap();
    tick(cx);
    assert!(
        cx.debug_bounds("cite-comment-field").is_some(),
        "streaming repairs source membership"
    );
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(replies(&view, cx)[0].text, "keep this draft");
}

#[gpui::test]
fn clicking_an_open_pencil_commits_and_closes_the_comment(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-toggle-pencil", cx, PARAGRAPHS);
    cite_alpha(&view, cx);
    open_comment(cx, 0);
    cx.simulate_input("save on toggle");
    click(cx, "cite-chip-0-comment-trigger");
    assert!(cx.debug_bounds("cite-panel").is_none());
    assert_eq!(replies(&view, cx)[0].text, "save on toggle");
}

fn remove_citation_source(view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    let document = view.read_with(cx, |view, cx| {
        view.panes[0]
            .transcript()
            .unwrap()
            .read(cx)
            .selection_document()
    });
    cx.update(|window, cx| {
        document.sync_members(Vec::new(), cx);
        window.refresh();
    });
    tick(cx);
}

#[gpui::test]
fn disappearing_source_saves_a_valid_comment_and_restores_focus(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-source-disappears", cx, PARAGRAPHS);
    drag(&view, cx, (3, 0), (3, 4));
    click(cx, "selection-reply-quote");
    cx.simulate_input("keep when source disappears");
    remove_citation_source(&view, cx);
    assert!(cx.debug_bounds("cite-panel").is_none());
    assert_eq!(replies(&view, cx)[0].text, "keep when source disappears");
    cx.simulate_input("main prompt");
    assert_eq!(
        view.read_with(cx, |view, cx| view.panes[0]
            .composer
            .read(cx)
            .text()
            .to_owned()),
        "main prompt"
    );
}

#[gpui::test]
fn disappearing_source_keeps_an_invalid_comment_above_its_pencil(cx: &mut TestAppContext) {
    let (view, cx) = conversation("cite-source-invalid", cx, PARAGRAPHS);
    drag(&view, cx, (3, 0), (3, 4));
    click(cx, "selection-reply-quote");
    paste(cx, "x".repeat(8_001));
    remove_citation_source(&view, cx);
    let popup = cx
        .debug_bounds("cite-panel")
        .expect("invalid comment remains editable");
    let pencil = cx.debug_bounds("cite-chip-0-comment-trigger").unwrap();
    assert!(popup.bottom() <= pencil.top());
    assert!(cx.debug_bounds("cite-comment-too-long").is_some());
    assert_eq!(replies(&view, cx)[0].text, "");
}

#[gpui::test]
fn reopened_citation_highlights_the_contextual_repeated_passage(cx: &mut TestAppContext) {
    let text = "first context repeated passage ends here\n\nsecond context repeated passage ends differently\n\n";
    let (view, cx) = conversation("cite-repeated-source", cx, text);
    drag(&view, cx, (1, 15), (1, 31));
    click(cx, "selection-reply-quote");
    cx.simulate_keystrokes("enter");
    tick(cx);
    let reply = replies(&view, cx)[0].clone();
    assert_eq!(reply.quote, "repeated passage");
    assert!(
        reply
            .selector
            .as_ref()
            .is_some_and(|selector| selector.prefix.contains("second context"))
    );
    let composer = view.read_with(cx, |view, _| view.panes[0].composer.clone());
    let mut content = composer.read_with(cx, |composer, _| composer.content());
    content.replies =
        serde_json::from_str(&serde_json::to_string(&content.replies).unwrap()).unwrap();
    composer.update(cx, |composer, cx| composer.put(content, cx));
    tick(cx);
    open_comment(cx, 0);
    let marked = composer
        .read_with(cx, |composer, _| composer.comment_source_bounds())
        .expect("restored selector highlights its source");
    let transcript = view.read_with(cx, |view, _| view.panes[0].transcript().unwrap());
    let source = transcript.read_with(cx, |transcript, _| transcript.scroll().bounds());
    assert!(marked.top() >= source.top());
    let expected = caret(&view, cx, 1, 15);
    assert!(
        marked.top() <= expected.y && marked.bottom() >= expected.y,
        "the second occurrence is highlighted"
    );
    assert!(
        cx.debug_bounds("cite-panel").unwrap().bottom()
            <= cx
                .debug_bounds("cite-chip-0-comment-trigger")
                .unwrap()
                .top()
    );
}
