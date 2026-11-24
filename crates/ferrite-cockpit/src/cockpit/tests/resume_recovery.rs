//! A Thread whose provider lost its native conversation says so, keeps the
//! prompt, and starts a fresh native conversation only when asked: once, in
//! the same Thread. A conversation merely in use elsewhere is only refused.
use super::*;
use ferrite_core::transcript::Body;

const OLD: &str = "old-native";
const PROMPT: &str = "carry on from where we stopped";

struct Meter;

impl ferrite_client::cockpit::RssSampler for Meter {
    fn sample(&mut self, _: ThreadId, _: Option<u32>) -> Option<u64> {
        Some(u64::MAX)
    }
}

/// A Thread that has spoken under native id `OLD`, then lost its Session and
/// whose provider refuses to resume `OLD` with `words`.
fn lost<'a>(
    name: &str,
    words: &str,
    cx: &'a mut TestAppContext,
) -> (
    Entity<CockpitView>,
    Fake,
    ThreadKey,
    &'a mut gpui::VisualTestContext,
) {
    let (mut core, fake) = cockpit(name, 1);
    let thread = core.threads()[0];
    bind_production_keys(cx);
    let _ = core.send(thread, "first question".into());
    let stream = fake.streams.borrow()[0].clone();
    stream
        .send(SessionEvent::Init {
            session_id: OLD.into(),
            model: String::new(),
        })
        .unwrap();
    stream
        .send(SessionEvent::TextDelta {
            text: "first answer\n\n".into(),
        })
        .unwrap();
    stream
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    view.update(cx, |view, cx| {
        view.cockpit.update(cx, |cockpit, _| cockpit.watch_memory(Box::new(Meter), 1));
        *fake.fail.borrow_mut() = true;
        assert_eq!(
            view.cockpit.update(cx, |cockpit, _| cockpit.sweep().len()),
            1,
            "the restart fails: no Session"
        );
        *fake.fail.borrow_mut() = false;
    });
    *fake.refused_resume.borrow_mut() = Some((OLD.into(), words.into()));
    (view, fake, thread, cx)
}

fn composer_text(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) -> String {
    view.read_with(cx, |view, cx| {
        view.panes[0].composer.read(cx).text().to_string()
    })
}

fn prompts(
    view: &Entity<CockpitView>,
    thread: ThreadKey,
    cx: &mut gpui::VisualTestContext,
) -> Vec<String> {
    view.read_with(cx, |view, cx| {
        view.cockpit
            .read(cx)
            .thread(thread)
            .unwrap()
            .transcript()
            .blocks()
            .iter()
            .filter_map(|block| match &block.body {
                Body::Prompt(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    })
}

fn settle(cx: &mut gpui::VisualTestContext) {
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(5));
        tick(cx);
    }
}

#[gpui::test]
fn a_lost_conversation_is_recovered_from_the_offer_once_in_the_same_thread(
    cx: &mut TestAppContext,
) {
    let (view, fake, thread, cx) = lost(
        "resume-gone",
        "no rollout found for thread id old-native",
        cx,
    );
    cx.simulate_input(PROMPT);
    cx.simulate_keystrokes("enter");
    settle(cx);

    assert_eq!(composer_text(&view, cx), PROMPT, "the prompt is back");
    let offer = format!("resume-fresh-{}", thread.thread().get());
    let shown = format!("resume-offer-{}", thread.thread().get());
    assert!(
        debug_bounds(cx, shown.clone()).is_some(),
        "the offer is shown"
    );
    assert!(
        debug_bounds(cx, offer.clone()).is_some(),
        "the control is shown"
    );
    let attempts = fake.resumed.borrow().len();

    cx.simulate_keystrokes("enter");
    settle(cx);
    assert_eq!(
        fake.resumed.borrow().len(),
        attempts,
        "sending again does not retry the lost conversation"
    );
    assert_eq!(composer_text(&view, cx), PROMPT, "and keeps the prompt");

    let at = debug_bounds(cx, offer.clone()).unwrap().center();
    cx.simulate_click(at, Default::default());
    settle(cx);

    assert_eq!(
        fake.resumed.borrow().last().unwrap(),
        &None,
        "a new native conversation"
    );
    let sent = fake.sent.borrow().clone();
    assert_eq!(
        sent.iter().filter(|text| text.contains(PROMPT)).count(),
        1,
        "the prompt went out once: {sent:?}"
    );
    assert!(
        sent.last().unwrap().contains("first question"),
        "with the earlier turns as context: {sent:?}"
    );
    assert_eq!(composer_text(&view, cx), "");
    assert!(
        debug_bounds(cx, shown.clone()).is_none(),
        "the offer is gone"
    );
    let said = prompts(&view, thread, cx);
    assert!(
        said.iter().any(|text| text.contains("first question")),
        "the old transcript stays: {said:?}"
    );
    assert_eq!(
        view.read_with(cx, |view, cx| view.cockpit.read(cx).threads().len()),
        1,
        "the same Thread"
    );
}

#[gpui::test]
fn a_conversation_in_use_is_refused_and_never_forked(cx: &mut TestAppContext) {
    let (view, fake, thread, cx) = lost(
        "resume-in-use",
        "thread resumed already has an active writer",
        cx,
    );
    cx.simulate_input(PROMPT);
    cx.simulate_keystrokes("enter");
    settle(cx);

    assert_eq!(composer_text(&view, cx), PROMPT, "the prompt is back");
    let shown = format!("resume-offer-{}", thread.thread().get());
    assert!(
        debug_bounds(cx, shown.clone()).is_none(),
        "no offer to start over"
    );
    let attempts = fake.resumed.borrow().len();
    view.update(cx, |view, cx| {
        assert!(view.cockpit.update(cx, |cockpit, _| cockpit.start_fresh_conversation(thread).now().is_ok()));
    });
    assert_eq!(
        fake.resumed.borrow().len(),
        attempts,
        "nothing to start over: no start of any kind"
    );
    assert!(
        fake.resumed.borrow()[1..]
            .iter()
            .all(|resume| resume.as_deref() == Some(OLD)),
        "every start after the first still asked for the old conversation: {:?}",
        fake.resumed.borrow()
    );
    assert!(fake.sent.borrow().iter().all(|text| !text.contains(PROMPT)));
}

#[gpui::test]
fn a_fresh_start_that_fails_keeps_the_prompt_and_does_not_loop(cx: &mut TestAppContext) {
    let (view, fake, thread, cx) = lost(
        "resume-fresh-fails",
        "no rollout found for thread id old-native",
        cx,
    );
    cx.simulate_input(PROMPT);
    cx.simulate_keystrokes("enter");
    settle(cx);
    let offer = format!("resume-fresh-{}", thread.thread().get());
    let at = debug_bounds(cx, offer).unwrap().center();

    *fake.fail.borrow_mut() = true;
    cx.simulate_click(at, Default::default());
    settle(cx);

    assert_eq!(composer_text(&view, cx), PROMPT, "the prompt is kept");
    assert!(fake.sent.borrow().iter().all(|text| !text.contains(PROMPT)));
    let attempts = fake.resumed.borrow().len();
    settle(cx);
    assert_eq!(
        fake.resumed.borrow().len(),
        attempts,
        "nothing starts again unasked"
    );
}
