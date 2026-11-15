//! A send keeps the whole draft until the Engine admits it. Every draft here
//! is typed into the rendered Composer and sent with the production Enter
//! binding; the refusals come from the Engine's own paths.
use super::*;
use ferrite_core::browser::{Document, TabId};
use ferrite_core::browser_annotations::{self, Annotation, Capture, Selection, Viewport};
use ferrite_core::replies::Reply;
use ferrite_core::transcript::Body;
use ferrite_reviewr::model::{Comment, Rev, Side};

const WORDS: &str = "Please check the quoted passage";

/// A Session grown past any limit: `Some(thread)`'s only, or every one's.
struct Meter(Option<ThreadId>);

impl ferrite_core::cockpit::RssSampler for Meter {
    fn sample(&mut self, thread: ThreadId, _: Option<u32>) -> Option<u64> {
        Some(if self.0.is_none_or(|grown| grown == thread) {
            u64::MAX
        } else {
            0
        })
    }
}

/// How a send can be refused.
#[derive(Clone, Copy, Debug)]
enum Refusal {
    ProviderStart,
    Delivery,
}

fn annotation(name: &str) -> Annotation {
    let root = scratch(name);
    let png = include_bytes!("../../../../ferrite-core/tests/fixtures/browser-original.png");
    browser_annotations::retain(
        &root,
        Capture {
            document: Document {
                tab: TabId(88),
                generation: 6,
            },
            native_generation: 9,
            url: "https://example.test/page".into(),
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
        Vec::new(),
    )
    .unwrap()
}

fn review_comment() -> Comment {
    Comment {
        file: "src/lib.rs".into(),
        side: Side::New,
        start: 3,
        end: 4,
        lines: "+let x = 1;\n+let y = 2;".into(),
        text: "Name these".into(),
        diff_anchored: true,
        rev: Rev::Worktree,
        replies: Vec::new(),
    }
}

fn window(
    core: Cockpit,
    cx: &mut TestAppContext,
) -> (Entity<CockpitView>, &mut gpui::VisualTestContext) {
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    (view, cx)
}

/// Fill the focused Pane's Composer with a whole draft: a file, a quote, a
/// review comment, a Browser annotation and typed words.
fn stage(view: &Entity<CockpitView>, name: &str, cx: &mut gpui::VisualTestContext) {
    let file = scratch(&format!("{name}-file.txt"));
    std::fs::write(&file, "attached").unwrap();
    let annotation = annotation(&format!("{name}-annotation"));
    view.update(cx, |view, cx| {
        let focused = view.focused();
        view.panes[focused].composer.update(cx, |composer, cx| {
            composer.add_files(&[file], cx);
            composer.reply(Reply::new("a passage", "what about it"), cx);
            composer.add_review_comment(review_comment(), cx);
            composer.add_browser_annotation(annotation, cx);
        });
    });
    cx.simulate_input(WORDS);
    tick(cx);
}

/// A window on a Thread whose send `refusal` will refuse, with the whole
/// draft staged: typed words, a file, a quote, a review comment and a
/// Browser annotation.
fn staged<'a>(
    name: &str,
    refusal: Option<Refusal>,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, Fake, &'a mut gpui::VisualTestContext) {
    let (mut core, fake) = cockpit(name, 1);
    match refusal {
        Some(Refusal::ProviderStart) => {
            core.watch_memory(Box::new(Meter(None)), 1);
            *fake.fail.borrow_mut() = true;
            assert_eq!(core.sweep().len(), 1, "the restart fails: no Session");
        }
        Some(Refusal::Delivery) => *fake.fail_send.borrow_mut() = true,
        None => {}
    }
    let (view, cx) = window(core, cx);
    stage(&view, name, cx);
    (view, fake, cx)
}

/// What the Composer holds, kind by kind.
fn held(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> (String, usize, usize, usize, usize) {
    view.read_with(cx, |view, cx| {
        let composer = view.panes[0].composer.read(cx);
        (
            composer.text().to_string(),
            composer.files().len(),
            composer.replies().len(),
            composer.review_comments().len(),
            composer.browser_annotations().len(),
        )
    })
}

const WHOLE: (usize, usize, usize, usize) = (1, 1, 1, 1);

fn notices(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        view.cockpit
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .filter_map(|block| match &block.body {
                Body::Notice(line) => Some(line.clone()),
                _ => None,
            })
            .collect()
    })
}

fn refused_send_keeps_the_whole_draft(refusal: Refusal, name: &str, cx: &mut TestAppContext) {
    let (view, fake, cx) = staged(name, Some(refusal), cx);
    cx.simulate_keystrokes("enter");
    tick(cx);

    let (text, files, replies, review, annotations) = held(&view, cx);
    assert_eq!(text, WORDS, "{refusal:?}: the typed words are back");
    assert_eq!(
        (files, replies, review, annotations),
        WHOLE,
        "{refusal:?}: every attachment is back"
    );
    assert_eq!(
        fake.sent
            .borrow()
            .iter()
            .filter(|text| text.contains(WORDS))
            .count(),
        0,
        "{refusal:?}: nothing reached the provider"
    );
    assert!(
        notices(&view, cx).iter().any(|line| line.contains("fail")
            || line.contains("refused")
            || line.contains("re-pick")),
        "{refusal:?}: the failure is said on the Thread: {:?}",
        notices(&view, cx)
    );
}

#[gpui::test]
fn a_provider_that_will_not_start_leaves_the_draft_whole(cx: &mut TestAppContext) {
    refused_send_keeps_the_whole_draft(Refusal::ProviderStart, "send-start-refused", cx);
}

#[gpui::test]
fn a_delivery_failure_leaves_the_draft_whole(cx: &mut TestAppContext) {
    refused_send_keeps_the_whole_draft(Refusal::Delivery, "send-delivery-failed", cx);
}

#[gpui::test]
fn an_admitted_send_clears_the_draft_and_goes_once(cx: &mut TestAppContext) {
    let (view, fake, cx) = staged("send-admitted", None, cx);
    cx.simulate_keystrokes("enter");
    tick(cx);

    assert_eq!(
        held(&view, cx),
        (String::new(), 0, 0, 0, 0),
        "the draft is gone"
    );
    let sent: Vec<_> = fake.sent.borrow().clone();
    assert_eq!(
        sent.iter().filter(|text| text.contains(WORDS)).count(),
        1,
        "the request appears once: {sent:?}"
    );
    assert!(view.read_with(cx, |view, _| view
        .held_sends
        .count(view.panes[0].thread().unwrap())
        == 0));
}

#[gpui::test]
fn a_refused_draft_sends_once_after_the_cause_is_fixed(cx: &mut TestAppContext) {
    let (view, fake, cx) = staged("send-retry", Some(Refusal::ProviderStart), cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(held(&view, cx).0, WORDS);

    *fake.fail.borrow_mut() = false;
    cx.simulate_keystrokes("enter");
    tick(cx);

    assert!(view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).is_empty()));
    assert_eq!(
        fake.sent
            .borrow()
            .iter()
            .filter(|text| text.contains(WORDS))
            .count(),
        1
    );
}

#[gpui::test]
fn an_unconfirmed_send_returns_the_draft_and_names_the_request(cx: &mut TestAppContext) {
    let (view, fake, cx) = staged("send-unconfirmed", None, cx);
    view.update(cx, |view, cx| {
        let thread = view.panes[0].thread().unwrap();
        let draft = view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.take_draft(cx));
        let admission = ferrite_core::cockpit::Admission::Unconfirmed("no reply".into());
        assert!(!view.settle_send(0, thread, draft, admission, cx));
    });
    tick(cx);

    let (text, files, replies, review, annotations) = held(&view, cx);
    assert_eq!(text, WORDS);
    assert_eq!((files, replies, review, annotations), WHOLE);
    assert!(
        notices(&view, cx)
            .iter()
            .any(|line| line.contains(WORDS) && line.contains("may or may not have been sent")),
        "the notice names the request: {:?}",
        notices(&view, cx)
    );
    assert!(fake.sent.borrow().is_empty(), "nothing retried on its own");
}

/// Two Threads, A (shown) and B. A has no Session: the start its next send
/// makes waits at a gate, and fails with the words the gate is sent.
fn gated<'a>(
    name: &str,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    Fake,
    [ThreadId; 2],
    mpsc::Sender<String>,
    &'a mut gpui::VisualTestContext,
) {
    let (mut core, fake) = cockpit(name, 2);
    let threads = [core.threads()[0], core.threads()[1]];
    core.watch_memory(Box::new(Meter(Some(threads[0]))), 1);
    *fake.fail.borrow_mut() = true;
    assert_eq!(core.sweep().len(), 1, "A's restart fails: no Session");
    *fake.fail.borrow_mut() = false;
    let (gate, hold) = mpsc::channel();
    *fake.start_gate.borrow_mut() = Some(hold);
    let (view, cx) = window(core, cx);
    view.update(cx, |view, cx| view.focus_thread(threads[0], cx));
    tick(cx);
    (view, fake, threads, gate, cx)
}

fn pane_of(
    view: &Entity<CockpitView>,
    thread: ThreadId,
    cx: &mut gpui::VisualTestContext,
) -> usize {
    view.read_with(cx, |view, _| view.pane_for(thread).expect("shown"))
}

fn composer_of(
    view: &Entity<CockpitView>,
    thread: ThreadId,
    cx: &mut gpui::VisualTestContext,
) -> (String, usize, usize, usize, usize) {
    let pane = pane_of(view, thread, cx);
    view.read_with(cx, |view, cx| {
        let composer = view.panes[pane].composer.read(cx);
        (
            composer.text().to_string(),
            composer.files().len(),
            composer.replies().len(),
            composer.review_comments().len(),
            composer.browser_annotations().len(),
        )
    })
}

/// Let the held start fail with `words`, and the Cockpit hear of it.
fn fail_start(gate: &mpsc::Sender<String>, words: &str, cx: &mut gpui::VisualTestContext) {
    gate.send(words.into()).unwrap();
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(5));
        tick(cx);
    }
}

fn held_count(
    view: &Entity<CockpitView>,
    thread: ThreadId,
    cx: &mut gpui::VisualTestContext,
) -> usize {
    view.read_with(cx, |view, _| view.held_sends.count(thread))
}

#[gpui::test]
fn a_held_draft_returns_to_its_thread_though_the_pane_moved_on(cx: &mut TestAppContext) {
    let (view, fake, [a, b], gate, cx) = gated("send-held-switch", cx);
    stage(&view, "send-held-switch", cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        held_count(&view, a, cx),
        1,
        "A's send waits for its Session"
    );
    assert_eq!(held(&view, cx), (String::new(), 0, 0, 0, 0));

    view.update(cx, |view, cx| view.focus_thread(b, cx));
    tick(cx);
    cx.simulate_input("for B");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(fake.sent.borrow().iter().any(|text| text.contains("for B")));
    assert_eq!(
        held_count(&view, a, cx),
        1,
        "B's delivery does not settle A's send"
    );

    fail_start(&gate, "claude would not start", cx);
    assert_eq!(
        composer_of(&view, b, cx),
        (String::new(), 0, 0, 0, 0),
        "B's Composer is not given A's words"
    );

    view.update(cx, |view, cx| view.focus_thread(a, cx));
    tick(cx);
    let (text, files, replies, review, annotations) = composer_of(&view, a, cx);
    assert_eq!(text, WORDS, "A's words are back with A");
    assert_eq!(
        (files, replies, review, annotations),
        WHOLE,
        "so is every attachment"
    );
    assert_eq!(held_count(&view, a, cx), 0);
}

#[gpui::test]
fn a_draft_held_when_its_pane_closes_comes_back_when_the_thread_is_shown(cx: &mut TestAppContext) {
    let (view, _fake, [a, b], gate, cx) = gated("send-held-close", cx);
    stage(&view, "send-held-close", cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(held_count(&view, a, cx), 1);

    view.update(cx, |view, cx| {
        view.close_pane(PaneIdentity::Thread(a), cx);
        view.focus_thread(b, cx);
    });
    tick(cx);
    fail_start(&gate, "claude would not start", cx);
    assert_eq!(held_count(&view, a, cx), 1, "the draft outlives the pane");

    view.update(cx, |view, cx| view.show_thread(a, cx));
    tick(cx);
    let (text, files, replies, review, annotations) = composer_of(&view, a, cx);
    assert_eq!(text, WORDS);
    assert_eq!((files, replies, review, annotations), WHOLE);
    assert_eq!(held_count(&view, a, cx), 0);
}

#[gpui::test]
fn a_held_draft_comes_back_whole_beside_newer_typing(cx: &mut TestAppContext) {
    let (view, _fake, [a, _], gate, cx) = gated("send-held-typing", cx);
    stage(&view, "send-held-typing", cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    cx.simulate_input("newer words");
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.reply(Reply::new("a passage", "what about it"), cx)
        });
    });

    fail_start(&gate, "claude would not start", cx);

    let (text, files, replies, review, annotations) = composer_of(&view, a, cx);
    assert_eq!(
        text,
        format!("newer words\n\n{WORDS}"),
        "the Engine hands a held prompt back behind what was typed since"
    );
    assert_eq!(
        (files, replies, review, annotations),
        WHOLE,
        "the quote typed again is not doubled"
    );
}
