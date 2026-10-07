//! Retained history at its limits. A long Thread trims its oldest turns in
//! place, keeping its transcript's text identity; a history regenerated in
//! place (a retraction replays it) leaves nothing of its old generation
//! behind in the Pane's caches.
use super::*;
use ferrite_core::activity::{
    ActivityEvent, AgentInfo, AgentKey, ExecutionEvent, Subject, TranscriptCoverage,
};

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

/// Main's history regenerated in place: an aside, then its retraction,
/// which the core replays the remaining history without.
fn regenerate(fake: &Fake, aside: usize) {
    let id = format!("aside-{aside}");
    for event in [
        ExecutionEvent::Text {
            text: format!("aside {aside}\n\n"),
        },
        ExecutionEvent::Retract {
            ids: vec![id.clone()],
        },
    ] {
        fake.streams.borrow()[0]
            .send(SessionEvent::Activity(ActivityEvent::MainContent {
                id: Some(id.clone()),
                event,
            }))
            .unwrap();
    }
}

fn child(fake: &Fake, name: &str) -> Subject {
    let key = AgentKey::new(Provider::Claude, "ui-fixture", name);
    let mut info = AgentInfo::new(key.clone());
    info.name = Some(name.into());
    info.parent = Some(Subject::Main);
    info.coverage = TranscriptCoverage::Live;
    for event in [
        ActivityEvent::Discovered(info),
        ActivityEvent::Content {
            key: key.clone(),
            id: Some(format!("{name}-text")),
            event: ExecutionEvent::Text {
                text: format!("{name} transcript\n\n"),
            },
        },
    ] {
        fake.streams.borrow()[0]
            .send(SessionEvent::Activity(event))
            .unwrap();
    }
    Subject::Subagent(key)
}

fn select(view: &Entity<CockpitView>, subject: Subject, cx: &mut gpui::VisualTestContext) {
    view.update(cx, |view, cx| {
        let thread = view.panes[0].thread().unwrap();
        view.select_subject_from_notice(thread, subject, cx);
    });
    tick(cx);
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

/// ⌘-click routes and laid-out link boxes belong to live transcripts. A
/// history regenerated in place re-keys its transcript; the old key's
/// entries go with it. A Subject switch keeps the hidden Subject's own.
#[gpui::test]
fn path_scopes_follow_live_transcripts_not_every_history_generation(cx: &mut TestAppContext) {
    let (core, fake, workspace) = bound_cockpit("retention-path-scopes", Provider::Claude);
    let file = workspace.join("docs").join("guide.md");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "guide\n").unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    hold_nav_open(&view, cx);
    cx.simulate_resize(gpui::size(px(1200.), px(600.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Before [guide](docs/guide.md:12) after.\n\n".into(),
        })
        .unwrap();
    tick(cx);
    let generation = view.read_with(cx, |view, _| view.panes[0].generation);
    for aside in 0..6 {
        regenerate(&fake, aside);
        tick(cx);
    }
    assert_ne!(
        view.read_with(cx, |view, _| view.panes[0].generation),
        generation,
        "each retraction regenerated Main's history"
    );
    let main = view.read_with(cx, |view, _| view.panes[0].text_namespace());
    assert_eq!(
        cx.update(|_, cx| crate::file_links::scopes(cx)),
        (vec![main.clone()], vec![main.clone()]),
        "only the live transcript routes ⌘-click and keeps laid-out targets"
    );

    // A Subject switch, there and back, prunes nothing.
    let atlas = child(&fake, "Atlas");
    tick(cx);
    select(&view, atlas, cx);
    let child = view.read_with(cx, |view, _| view.panes[0].text_namespace());
    let both = {
        let mut both = vec![main.clone(), child];
        both.sort();
        both
    };
    assert_eq!(cx.update(|_, cx| crate::file_links::scopes(cx)).0, both);
    select(&view, Subject::Main, cx);
    assert_eq!(cx.update(|_, cx| crate::file_links::scopes(cx)).0, both);

    // ⌘-click on the live transcript's link still opens its reader (the
    // reader opens in the active window).
    cx.update(|window, _| window.activate_window());
    let card = debug_bounds(cx, format!("file-attachment-{}", file.display()))
        .or_else(|| debug_bounds(cx, "inline-file".to_string()))
        .expect("the link is laid out");
    cx.simulate_click(card.center(), gpui::Modifiers::secondary_key());
    tick(cx);
    assert_eq!(cx.opened_url(), None, "not handed to the system");
    assert_eq!(
        view.read_with(cx, |view, _| view.panes[0]
            .preview
            .document()
            .map(|document| document.path)),
        Some(file),
        "the transcript's ⌘-click opened its reader"
    );
}

/// Native text built for a Subject's older history generation can never be
/// drawn again: a regenerated history releases it from the Pane's cache.
#[gpui::test]
fn the_text_cache_releases_a_subjects_older_history_generations(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("retention-text-cache", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "Main's answer stays.\n\n".into(),
        })
        .unwrap();
    tick(cx);
    for aside in 0..6 {
        regenerate(&fake, aside);
        tick(cx);
    }
    let (namespace, ids) = view.read_with(cx, |view, _| {
        (
            view.panes[0].text_namespace(),
            view.panes[0].rich.cached_ids(),
        )
    });
    let current = format!("-{namespace}-");
    let main: Vec<_> = ids.iter().filter(|id| id.contains("-main-")).collect();
    assert!(
        main.iter().any(|id| id.starts_with("markdown-")),
        "the live answer is cached"
    );
    assert!(
        main.iter().all(|id| id.contains(&current)),
        "older generations still cached: {main:?}"
    );
}
