//! A Thread's unsent line outlives its Pane and the window.
use super::sends::{held, stage, staged, Refusal};
#[allow(unused_imports)]
use super::*;
use ferrite_core::draft_content::DraftContent;
use ferrite_core::drafts::Drafts;
use ferrite_core::replies::Reply;

/// `thread` as the drafts file names this computer's.
pub(super) fn saved_here(thread: ThreadId) -> ferrite_core::SavedThread {
    ferrite_core::SavedThread {
        machine: ferrite_core::MachineKey(None),
        thread,
    }
}

fn words(text: &str) -> DraftContent {
    DraftContent {
        text: text.into(),
        ..DraftContent::default()
    }
}

/// A scratch directory for the window's preferences, so no test writes
/// where the operator's own files are.
fn prefs_in(name: &str) -> Preferences {
    let dir = scratch(name);
    std::fs::create_dir_all(&dir).unwrap();
    Preferences {
        dir,
        ..Preferences::ephemeral()
    }
}

/// What is typed is written once the typing pauses, and again when the
/// window quits, for the next launch to find.
#[gpui::test]
fn the_line_typed_is_written_for_the_next_launch(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("drafts-written", 1);
    let thread = core.threads()[0];
    let prefs = prefs_in("drafts-written-prefs");
    let dir = prefs.dir.clone();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    cx.simulate_input("half a thought");
    assert_eq!(Drafts::load(&dir).thread(&saved_here(thread)), None, "not on every key");
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(
        Drafts::load(&dir).thread(&saved_here(thread)),
        Some(&words("half a thought"))
    );

    // Sending empties the line, and the draft goes with it.
    view.update(cx, |view, cx| {
        let composer = view.panes[0].composer.clone();
        composer.update(cx, |composer, cx| composer.take(cx));
    });
    cx.run_until_parked();
    view.update(cx, |view, cx| view.save_drafts(cx));
    assert_eq!(Drafts::load(&dir).thread(&saved_here(thread)), None);
}

/// A Thread's Pane opens with the line it was left with.
#[gpui::test]
fn a_pane_opens_with_the_line_it_was_left_with(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("drafts-restored", 1);
    let thread = core.threads()[0];
    let prefs = prefs_in("drafts-restored-prefs");
    let mut drafts = Drafts::default();
    drafts.keep_thread(saved_here(thread), words("where I left off"));
    drafts.save(&prefs.dir).unwrap();
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(view.panes[0].composer.read(cx).text(), "where I left off");
    });
}

fn prefs_at(dir: &std::path::Path) -> Preferences {
    Preferences {
        dir: dir.to_path_buf(),
        ..Preferences::ephemeral()
    }
}

/// A whole draft (a literal "> " line, a quote, a file, a review comment
/// and an annotation) is on disk after the window quits, and the next
/// launch's Composer shows every part as a control, the "> " line still
/// words.
#[gpui::test]
fn a_whole_draft_comes_back_after_a_restart(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("drafts-restart", 1);
    let thread = core.threads()[0];
    let prefs = prefs_in("drafts-restart-prefs");
    let dir = prefs.dir.clone();
    let (view, cx) = window_with(core, prefs, cx);
    stage(&view, "drafts-restart", cx);
    cx.simulate_input("\n> not a quote");
    tick(cx);
    let before = held(&view, cx);
    view.update(cx, |view, cx| view.save_drafts(cx));
    drop(view);

    let (core, _fake) = cockpit("drafts-restart-again", 1);
    assert_eq!(core.threads()[0], thread);
    let (view, cx) = window_with(core, prefs_at(&dir), cx);
    assert_eq!(held(&view, cx), before, "the same words and attachments");
    assert!(before.0.ends_with("\n> not a quote"));
    for control in [
        "cite-chip-0",
        "review-attachment",
        "browser-annotation-attachment",
    ] {
        assert!(cx.debug_bounds(control).is_some(), "{control} is drawn");
    }
}

/// A new Pane's draft is written with the provider and effort it was aimed
/// at, and comes back as a new Pane with both.
#[gpui::test]
fn a_new_pane_comes_back_with_its_words_and_binding(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("drafts-fresh", 0);
    let prefs = prefs_in("drafts-fresh-prefs");
    let dir = prefs.dir.clone();
    let (view, cx) = window_with(core, prefs, cx);
    cx.simulate_input("> first thoughts");
    tick(cx);
    let binding = view.update(cx, |view, cx| {
        let focused = view.focused(cx);
        view.panes[focused]
            .draft_mut()
            .unwrap()
            .binding
            .choose_effort(Some("high".into()));
        view.save_drafts(cx);
        view.panes[focused].draft().unwrap().binding.clone()
    });
    drop(view);

    let (core, _fake) = cockpit("drafts-fresh-again", 0);
    let (view, cx) = window_with(core, prefs_at(&dir), cx);
    view.read_with(cx, |view, cx| {
        let pane = &view.panes[view.focused(cx)];
        assert_eq!(pane.composer.read(cx).text(), "> first thoughts");
        assert_eq!(pane.draft().map(|draft| &draft.binding), Some(&binding));
    });
}

/// A new Pane written in and then closed is discarded: the next launch
/// neither reopens it nor finds it in the file. `close` is the operator's
/// way of closing, from the production key or the rendered discard control.
fn a_closed_new_pane_is_not_reopened(
    name: &str,
    cx: &mut TestAppContext,
    close: fn(&mut gpui::VisualTestContext),
) {
    let (core, _fake) = cockpit(name, 1);
    let thread = core.threads()[0];
    let prefs = prefs_in(&format!("{name}-prefs"));
    let dir = prefs.dir.clone();
    let (view, cx) = window_with(core, prefs, cx);
    view.update(cx, |view, cx| {
        view.open_draft(DraftTarget::Main, cx)
    });
    tick(cx);
    cx.simulate_input("never send this");
    tick(cx);
    view.update(cx, |view, cx| view.save_drafts(cx));
    assert!(
        std::fs::read_to_string(dir.join(Drafts::FILE))
            .unwrap()
            .contains("never send this"),
        "the pane's words are on disk while it is open"
    );

    close(cx);
    tick(cx);
    view.update(cx, |view, cx| view.save_drafts(cx));
    drop(view);

    assert!(
        !std::fs::read_to_string(dir.join(Drafts::FILE))
            .unwrap()
            .contains("never send this"),
        "the file forgot the discarded pane"
    );
    let (core, _fake) = cockpit(&format!("{name}-again"), 1);
    assert_eq!(core.threads()[0], thread);
    let (view, _cx) = window_with(core, prefs_at(&dir), cx);
    view.read_with(_cx, |view, _| {
        assert!(
            view.panes.iter().all(|pane| pane.draft().is_none()),
            "no new Pane comes back"
        );
    });
}

#[gpui::test]
fn cmd_w_discards_a_new_pane_for_good(cx: &mut TestAppContext) {
    a_closed_new_pane_is_not_reopened("drafts-cmd-w", cx, |cx| cx.simulate_keystrokes("cmd-w"));
}

/// A prompt taken back from the queue (Enter on the empty line) returns as the parts it was sent
/// with: its quote is a control again and a literal "> " line stays words.
#[gpui::test]
fn a_queued_prompt_taken_back_returns_whole(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("drafts-queued", 1);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("enter", Submit, None)]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta {
            text: "working".into(),
        })
        .unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.reply(Reply::new("a passage", "why"), cx)
        });
    });
    cx.simulate_input("\n> literal");
    cx.simulate_keystrokes("enter");
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, cx| {
        let composer = view.panes[0].composer.read(cx);
        assert_eq!(composer.replies(), [Reply::new("a passage", "why")]);
        assert_eq!(composer.text(), "\n> literal");
    });
}

fn window_with(
    core: Cockpit,
    prefs: Preferences,
    cx: &mut TestAppContext,
) -> (Entity<CockpitView>, &mut gpui::VisualTestContext) {
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    (view, cx)
}

/// A send that fails gives back a literal "> " line as words, not as a
/// quote, beside the attachments it was sent with.
#[gpui::test]
fn a_failed_send_returns_a_literal_quote_marker_as_words(cx: &mut TestAppContext) {
    let (view, _fake, cx) = staged("drafts-refused", Some(Refusal::Delivery), cx);
    cx.simulate_input("\n> literal");
    cx.simulate_keystrokes("enter");
    tick(cx);
    let (text, files, replies, review, annotations) = held(&view, cx);
    assert!(text.ends_with("\n> literal"), "{text:?}");
    assert_eq!((files, replies, review, annotations), (1, 1, 1, 1));
}
