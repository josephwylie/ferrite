//! Retained history at its limits. A long Thread trims its oldest turns in
//! place, keeping its transcript's text identity.
use super::*;

fn pump(view: &Entity<CockpitView>, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| {
        view.pump(cx);
        view.sync_visible_transcripts(cx);
    });
}

/// One Main turn: `bytes` of answer, then its end.
fn turn(fake: &Fake, turn: usize, bytes: usize) {
    let stream = fake.streams.borrow();
    stream[0]
        .send(SessionEvent::TextDelta {
            text: format!("turn {turn} {}\n\n", "w".repeat(bytes)),
        })
        .unwrap();
    stream[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
}

/// At its retention cap a long Thread trims its oldest turn on every
/// append. That trim must not start a new history generation: the Pane's
/// text namespace (every native text identity under it) stays put.
#[gpui::test]
fn a_transcript_at_its_retention_cap_keeps_its_text_identity_as_it_streams(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("retention-cap-identity", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    // Past the 4 MiB of answer the display keeps.
    for at in 0..520 {
        turn(&fake, at, 8 * 1024);
        if at % 100 == 99 {
            pump(&view, cx);
        }
    }
    for _ in 0..4 {
        pump(&view, cx);
    }
    let first = view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        transcript_text(view.cockpit.thread(thread).unwrap().transcript().blocks())
    });
    assert!(
        !first.starts_with("turn 0 "),
        "the cap trimmed the oldest turns"
    );
    let namespace = view.read_with(cx, |view, _| view.panes[0].text_namespace());
    let mut namespaces = std::collections::BTreeSet::new();
    for at in 520..540 {
        turn(&fake, at, 8 * 1024);
        pump(&view, cx);
        namespaces.insert(view.read_with(cx, |view, _| view.panes[0].text_namespace()));
    }
    assert_eq!(
        namespaces,
        std::collections::BTreeSet::from([namespace]),
        "appends at the cap re-keyed the transcript"
    );
}
