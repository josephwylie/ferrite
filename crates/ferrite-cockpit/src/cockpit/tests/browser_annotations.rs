//! A sent Browser annotation remains an inspectable attachment in history.
use super::*;

#[gpui::test]
fn browser_annotation_history_keeps_a_distinct_reviewable_capture(cx: &mut TestAppContext) {
    use ferrite_core::browser::{Document, TabId};
    use ferrite_core::browser_annotations::{
        self, Capture, Drawing, Ink, Point, Selection, Viewport,
    };
    let (core, _fake) = cockpit("ba-history", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1500.), px(900.)));
    let root = std::env::temp_dir().join(format!("ferrite-history-image-{}", std::process::id()));
    let png = include_bytes!("../../../../ferrite-engine/tests/fixtures/browser-original.png");
    let annotation = browser_annotations::retain(
        &root,
        Capture {
            document: Document {
                tab: TabId(88),
                generation: 6,
            },
            native_generation: 9,
            url: "https://example.test/earlier-page".into(),
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
        vec![Drawing::Arrow {
            from: Point { x: 0.1, y: 0.1 },
            to: Point { x: 0.8, y: 0.8 },
            ink: Ink {
                color: "#ff0000".into(),
                width: 0.1,
            },
        }],
    )
    .unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.set("Review this captured page".into(), cx);
            composer.add_browser_annotation(annotation.clone(), cx);
        });
    });
    tick(cx);
    assert!(cx.debug_bounds("browser-annotation-attachment").is_some());
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).is_empty()));
    let card = cx
        .debug_bounds("browser-annotation-attachment")
        .expect("the sent message retains a distinct Browser annotation card");
    cx.simulate_click(card.center(), gpui::Modifiers::none());
    until(cx, |cx| cx.debug_bounds("annotation-stage").is_some());
    assert!(
        cx.debug_bounds("browser-annotation-review").is_some(),
        "review opens the retained capture and drawings without a live browser"
    );
    assert!(cx.debug_bounds("annotation-stage").is_some());
    assert_eq!(std::fs::read(annotation.original().unwrap()).unwrap(), png);
    std::fs::remove_dir_all(root).unwrap();
}

/// The review of a sent annotation open in the Panel, then the app
/// restarted on the same store and settings. `quit` parks the Thread as
/// quitting does, which writes the prompt history reads back; without it
/// history no longer holds the annotation.
struct Restarted<'a> {
    view: Entity<CockpitView>,
    window: &'a mut gpui::VisualTestContext,
    thread: ThreadKey,
    capture: std::path::PathBuf,
    settings: std::path::PathBuf,
    root: std::path::PathBuf,
}

fn restarted<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    quit: bool,
    launch: impl FnOnce(&mut Cockpit),
) -> Restarted<'a> {
    use ferrite_core::browser::{Document, TabId};
    use ferrite_core::browser_annotations::{self, Capture, Selection, Viewport};
    let (core, fake) = cockpit(name, 1);
    let store = std::env::temp_dir().join(format!("ferrite-view-{}-{name}", std::process::id()));
    let mut prefs = Preferences::ephemeral();
    prefs.dir = scratch(&format!("{name}-settings"));
    let settings = prefs.dir.clone();
    bind_production_keys(cx);
    let (view, window) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    window.simulate_resize(gpui::size(px(1500.), px(900.)));
    let root = std::env::temp_dir().join(format!("ferrite-{name}-{}", std::process::id()));
    let annotation = browser_annotations::retain(
        &root,
        Capture {
            document: Document {
                tab: TabId(5),
                generation: 1,
            },
            native_generation: 1,
            url: "https://example.test/restarted".into(),
            viewport: Viewport {
                width_css: 1.,
                height_css: 1.,
                device_scale: 1.,
            },
            selection: Selection::Region {
                x: 0.,
                y: 0.,
                width: 1.,
                height: 1.,
            },
        },
        include_bytes!("../../../../ferrite-engine/tests/fixtures/browser-original.png"),
        "Still here".into(),
        vec![],
    )
    .unwrap();
    tick(window);
    view.update(window, |view, cx| {
        view.panes[0].composer.update(cx, |composer, cx| {
            composer.set("Review after restart".into(), cx);
            composer.add_browser_annotation(annotation.clone(), cx);
        });
    });
    tick(window);
    window.simulate_keystrokes("enter");
    tick(window);
    let chip = window.debug_bounds("review-browser-annotation-0").unwrap();
    window.simulate_click(chip.center(), gpui::Modifiers::none());
    until(window, |cx| cx.debug_bounds("annotation-stage").is_some());
    let thread = view.update(window, |view, cx| {
        view.flush_panel_persistence(cx);
        let thread = view.panes[0].thread().unwrap();
        if quit {
            view.cockpit.update(cx, |cockpit, _| cockpit.park(thread).now().unwrap());
        }
        thread
    });
    drop(view);

    let mut core = Cockpit::new(Store::open(&store).unwrap(), Box::new(fake));
    // The relaunch holds its machine under a new id.
    let thread = core.key(thread.thread());
    core.reopen(thread).now().unwrap();
    launch(&mut core);
    let mut prefs = Preferences::ephemeral();
    prefs.dir = settings.clone();
    let (view, window) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    window.simulate_resize(gpui::size(px(1500.), px(900.)));
    Restarted {
        view,
        window,
        thread,
        capture: annotation.original().unwrap().to_path_buf(),
        settings,
        root,
    }
}

/// After a restart the Panel brings back the review's tab, keyed by the
/// capture's PNG; the review, not the PNG in the lightbox, comes back with it.
#[gpui::test]
fn browser_annotation_review_reopens_after_a_restart(cx: &mut TestAppContext) {
    let Restarted { window, root, .. } = restarted(cx, "ba-restart", true, |_| {});
    until(window, |cx| cx.debug_bounds("annotation-stage").is_some());
    assert!(
        window.debug_bounds("lightbox").is_none(),
        "the restored tab is the review, not its capture in the lightbox"
    );
    let chip = window
        .debug_bounds("review-browser-annotation-0")
        .expect("history draws the sent annotation");
    window.simulate_click(chip.center(), gpui::Modifiers::none());
    until(window, |cx| cx.debug_bounds("annotation-stage").is_some());
    assert!(window.debug_bounds("browser-annotation-review").is_some());
    assert!(window.debug_bounds("lightbox").is_none());
    std::fs::remove_dir_all(root).unwrap();
}

/// A restored image tab that no prompt in history claims cannot be filled:
/// it goes, for good, and never traps the window under the lightbox.
#[gpui::test]
fn a_restored_image_tab_nothing_claims_goes_without_the_lightbox(cx: &mut TestAppContext) {
    let Restarted {
        view,
        window,
        thread,
        capture,
        settings,
        root,
    } = restarted(cx, "ba-restart-unclaimed", false, |_| {});
    for _ in 0..5 {
        tick(window);
    }
    assert!(window.debug_bounds("lightbox").is_none(), "no lightbox over the window");
    window.simulate_input("still mine");
    tick(window);
    assert_eq!(composer_text(&view, window), "still mine", "the keyboard is the Pane's");
    let tab = crate::cockpit::surfaces::SurfaceId::File(capture);
    view.update(window, |view, cx| {
        assert!(!view.panel.surfaces.get(thread).unwrap().tabs.contains(&tab));
        view.apply_surface(thread, crate::cockpit::surfaces::SurfaceAction::Reopen);
        assert!(!view.panel.surfaces.get(thread).unwrap().tabs.contains(&tab), "nor on Reopen");
        view.flush_panel_persistence(cx);
    });
    let saved = crate::cockpit::surfaces::Surfaces::saved_in(&settings, thread.thread()).unwrap();
    assert!(!saved.tabs.contains(&tab), "nor at the next launch");
    std::fs::remove_dir_all(root).unwrap();
}

/// A window streams a Thread's history only once it draws it, so the first
/// frames after launch hold the Thread's facts with no history yet. The
/// review's tab waits for it rather than going, or opening its capture.
#[gpui::test]
fn a_restored_review_waits_for_history_that_lands_after_the_first_frame(cx: &mut TestAppContext) {
    let Restarted {
        view,
        window,
        thread,
        capture,
        root,
        ..
    } = restarted(cx, "ba-restart-late", true, |core| {
        core.draws_transcripts([]);
        let _ = core.pump();
    });
    let tab = crate::cockpit::surfaces::SurfaceId::File(capture);
    // Each look draws the frame the window asked for, as a real window
    // does: the restored strip scrolling its tab into view asks for one,
    // and the redraw the history needs joins it.
    until(window, |cx| {
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        cx.debug_bounds("annotation-stage").is_some()
    });
    assert!(window.debug_bounds("lightbox").is_none());
    assert!(view.read_with(window, |view, _| view.active_surface(thread) == Some(tab)));
    std::fs::remove_dir_all(root).unwrap();
}

#[gpui::test]
fn browser_annotation_retained_editor_adds_original_evidence_without_a_live_page(
    cx: &mut TestAppContext,
) {
    use ferrite_core::browser::{Document, TabId};
    use ferrite_core::browser_annotations::{self, Capture, Selection, Viewport};
    let (core, _fake) = cockpit("ba-editor-retained", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1500.), px(900.)));
    tick(cx);
    cx.simulate_input("Start the conversation");
    cx.simulate_keystrokes("enter");
    until(cx, |cx| {
        view.read_with(cx, |view, _| view.panes[0].thread().is_some())
    });
    let root = std::env::temp_dir().join(format!("ferrite-retained-editor-{}", std::process::id()));
    let png = include_bytes!("../../../../ferrite-engine/tests/fixtures/browser-original.png");
    let original = browser_annotations::retain(
        &root,
        Capture {
            document: Document {
                tab: TabId(901),
                generation: 44,
            },
            native_generation: 71,
            url: "https://example.test/page-that-is-now-closed".into(),
            viewport: Viewport {
                width_css: 1.,
                height_css: 1.,
                device_scale: 1.,
            },
            selection: Selection::Region {
                x: 0.,
                y: 0.,
                width: 1.,
                height: 1.,
            },
        },
        png,
        String::new(),
        vec![],
    )
    .unwrap();
    view.update(cx, |view, cx| {
        view.panes[0].preview.edit_annotation(original.clone());
        cx.notify();
    });
    ferrite_core::doorbell::until("retained editor must open without any live page", || {
        leap(cx, super::super::user_commands::ENDED_POLL);
        cx.debug_bounds("annotation-editor").is_some()
    });
    assert!(
        cx.debug_bounds("browser-notice").is_none(),
        "the empty-page notice must not cover the retained editor"
    );
    let comment = cx.debug_bounds("annotation-comment").unwrap();
    cx.simulate_click(comment.center(), gpui::Modifiers::none());
    cx.simulate_input("Keep this feedback on the closed page");
    let add = cx.debug_bounds("annotation-add").unwrap();
    cx.simulate_click(add.center(), gpui::Modifiers::none());
    until(cx, |cx| {
        cx.debug_bounds("browser-annotation-attachment").is_some()
    });
    let prompt = view.read_with(cx, |view, cx| view.panes[0].composer.read(cx).prompt());
    let (_, _, annotations) = browser_annotations::split(prompt);
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].capture(), original.capture());
    assert_eq!(annotations[0].original_hash().unwrap(), original.original_hash().unwrap());
    assert_eq!(
        annotations[0].comment(),
        "Keep this feedback on the closed page"
    );
    assert_eq!(std::fs::read(original.original().unwrap()).unwrap(), png);
    std::fs::remove_dir_all(root).unwrap();
}

/// A live annotation's Send sends the Composer under it, so it is off
/// while any sheet covers that Composer, and back once the sheet closes.
#[gpui::test]
fn a_live_annotation_cannot_send_under_a_sheet(cx: &mut TestAppContext) {
    use ferrite_pane::visual::web::fake::Call;
    let (engine, view, _thread, cx) = super::browser_chrome::browser_window(cx, "ba-under-sheet");
    let send_enabled = || {
        engine.journal().iter().rev().find_map(|call| match call {
            Call::AnnotationStarted(_, enabled) | Call::AnnotationSendEnabled(enabled) => Some(*enabled),
            _ => None,
        })
    };
    let annotate = cx.debug_bounds("browser-annotate").expect("live page offers annotation");
    cx.simulate_click(annotate.center(), gpui::Modifiers::none());
    // The editor starts once the Server has answered for the page's
    // protection, a beat or more after the press.
    until(cx, |_| send_enabled().is_some());
    assert_eq!(send_enabled(), Some(true), "Send sends, with no sheet up");
    view.update(cx, |view, cx| view.toggle_settings(cx));
    tick(cx);
    assert_eq!(send_enabled(), Some(false), "not under Settings");
    view.update(cx, |view, cx| view.hide_sheet(cx));
    tick(cx);
    assert_eq!(send_enabled(), Some(true), "back once it closes");
    view.update(cx, |view, cx| view.open_project_creator(cx));
    tick(cx);
    assert_eq!(send_enabled(), Some(false), "not under the Project card");
}
