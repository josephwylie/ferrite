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

/// A Session grown past any limit.
struct Meter;

impl ferrite_core::cockpit::RssSampler for Meter {
    fn sample(&mut self, _: ThreadId, _: Option<u32>) -> Option<u64> {
        Some(u64::MAX)
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

/// A window on a Thread whose send `refusal` will refuse, with the whole
/// draft staged: typed words, a file, a quote, a review comment and a
/// Browser annotation.
fn staged<'a>(
    name: &str,
    refusal: Option<Refusal>,
    cx: &'a mut TestAppContext,
) -> (Entity<CockpitView>, Fake, &'a mut gpui::VisualTestContext) {
    let (mut core, fake) = cockpit(name, 1);
    let thread = core.threads()[0];
    match refusal {
        Some(Refusal::ProviderStart) => {
            core.watch_memory(Box::new(Meter), 1);
            *fake.fail.borrow_mut() = true;
            assert_eq!(core.sweep().len(), 1, "the restart fails: no Session");
        }
        Some(Refusal::Delivery) => *fake.fail_send.borrow_mut() = true,
        None => {}
    }
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let file = scratch(&format!("{name}-file.txt"));
    std::fs::write(&file, "attached").unwrap();
    let annotation = annotation(&format!("{name}-annotation"));
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.add_files(&[file], cx);
            composer.reply(Reply::new("a passage", "what about it"), cx);
            composer.add_review_comment(review_comment(), cx);
            composer.add_browser_annotation(annotation, cx);
        });
    });
    cx.simulate_input(WORDS);
    tick(cx);
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
    assert!(view.read_with(cx, |view, _| view.panes[0].held_sends.is_empty()));
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

#[gpui::test]
fn a_held_draft_comes_back_whole_beside_newer_typing(cx: &mut TestAppContext) {
    let (view, _fake, cx) = staged("send-held", None, cx);
    view.update(cx, |view, cx| {
        let thread = view.panes[0].thread().unwrap();
        let draft = view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.take_draft(cx));
        let prompt = draft.prompt().to_string();
        let held = ferrite_core::cockpit::Admission::Held;
        assert!(view.settle_send(0, thread, draft, held, cx));
        assert_eq!(view.panes[0].held_sends.len(), 1);
        assert!(view.held_draft(0, "something else").is_none());
        let draft = view.held_draft(0, &prompt).expect("the held draft");
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.insert("newer words", cx));
        view.put_back_send(0, draft, cx);
    });
    tick(cx);

    let (text, files, replies, review, annotations) = held(&view, cx);
    assert_eq!(text, format!("{WORDS}\n\nnewer words"));
    assert_eq!((files, replies, review, annotations), WHOLE);
}
