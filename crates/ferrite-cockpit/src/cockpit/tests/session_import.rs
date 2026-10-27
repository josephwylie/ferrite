//! Importing a provider session from disk.
#[allow(unused_imports)]
use super::*;

/// Fake vendor session roots under a scratch base — the shapes the
/// vendors write (project slugs, date directories), never a real home.
fn session_roots(base: &std::path::Path) -> Vec<(Provider, std::path::PathBuf)> {
    vec![
        (Provider::Claude, base.join("claude-projects")),
        (Provider::Codex, base.join("codex-sessions")),
    ]
}

/// A session file `age_secs` old: written, then stamped with the mtime
/// discovery orders by.
fn write_session_file(path: &std::path::Path, contents: &str, age_secs: u64) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
    let modified = std::time::SystemTime::now() - Duration::from_secs(age_secs);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
}

/// A minimal claude-shaped session body: the import contract's own
/// cut-down lines, stamped with `session`.
fn claude_session_body(session: &str) -> String {
    concat!(
        r#"{"type":"user","sessionId":"SESSION","cwd":"/workspace","message":{"role":"user","content":"first question"}}"#,
        "\n",
        r#"{"type":"assistant","sessionId":"SESSION","message":{"model":"claude-haiku-4-5","content":[{"type":"text","text":"first answer"}]}}"#,
        "\n",
    )
    .replace("SESSION", session)
}

/// #11: on a fresh Thread `/` lists Ferrite's own `import` entry — the
/// whole menu before the provider announces commands, the top row
/// alongside them after — and the first real conversation retires it.
#[gpui::test]
fn a_fresh_thread_lists_the_local_import_entry_atop_the_slash_menu(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("import-entry", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    // No provider menu yet: the local entries are the whole list —
    // #25's model door on top, then the import door.
    cx.simulate_input("/");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view
            .popover
            .as_ref()
            .expect("/ offers import on a fresh Thread");
        let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(names, ["/model", "/effort", "/import"]);
        assert_eq!(
            menu.rows[0].detail.as_ref(),
            "Switch model or hand over to the other provider"
        );
        assert_eq!(
            menu.rows[2].detail.as_ref(),
            "Resume a Claude or Codex session"
        );
    });

    // The Session announces its own commands: import rides on top, and
    // the fuzzy filter treats it like any row (`co` is not in "import").
    fake.streams.borrow()[0]
        .send(SessionEvent::Commands {
            commands: menu_commands(),
        })
        .unwrap();
    tick(cx);
    cx.simulate_input("co");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("open");
        let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(names, ["/code-review", "/commit", "/compact"]);
    });
    cx.simulate_keystrokes("backspace backspace");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("open");
        let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(
            names,
            [
                "/model",
                "/effort",
                "/import",
                "/code-review",
                "/commit",
                "/compact",
                "/to-tickets"
            ],
            "the local entries ride atop the provider's own menu"
        );
    });

    // A conversation starts: import stays, opening alongside now; the
    // model row stays live — the model can change any time — and says
    // what it does now that the provider is fixed.
    cx.simulate_keystrokes("backspace");
    cx.simulate_input("hello");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_input("/");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("the model menu still lists");
        assert!(
            menu.rows.iter().any(|row| row.name.as_ref() == "/import"),
            "a Thread with history still offers import, alongside"
        );
        assert_eq!(menu.rows[0].name.as_ref(), "/model");
        assert!(!menu.rows[0].inert, "the model door stays open");
        assert_eq!(
            menu.rows[0].detail.as_ref(),
            "Switch model or hand over to the other provider"
        );
        assert_eq!(menu.rows[1].name.as_ref(), "/effort");
        assert_eq!(menu.rows.len(), 7);
    });
}

/// The open import picker's survivors: each row's name and context.
fn session_rows(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> Vec<(String, String)> {
    view.read_with(cx, |view, cx| {
        let palette = view
            .floats
            .palette
            .as_ref()
            .expect("the picker is open")
            .read(cx);
        assert_eq!(palette.scope, crate::palette::PaletteScope::Sessions);
        palette
            .survivors()
            .iter()
            .map(|row| (row.name.to_string(), row.context.to_string()))
            .collect()
    })
}

/// #11: picking `import` is Ferrite's own act — the line is cleared,
/// nothing goes near the provider, and the import picker lists both
/// vendors' sessions newest first with provider and age per row,
/// searched as the operator types.
#[gpui::test]
fn picking_import_opens_the_session_picker_and_sends_the_provider_nothing(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("import-picker", 1);
    let base = scratch("import-picker-roots");
    let roots = session_roots(&base);
    write_session_file(
        &roots[0].1.join("-workspace-alpha").join("aaaa.jsonl"),
        &claude_session_body("aaaa"),
        3 * 60 * 60,
    );
    write_session_file(
        &roots[0].1.join("-workspace-beta").join("bbbb.jsonl"),
        &claude_session_body("bbbb"),
        60,
    );
    write_session_file(
        &roots[1]
            .1
            .join("2026")
            .join("08")
            .join("25")
            .join("rollout-cccc.jsonl"),
        "not read by discovery\n",
        30 * 60,
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.session_file_roots = roots);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

    // `im` filters to the import door — the provider door (#25) rides
    // above it on a bare `/`.
    cx.simulate_input("/im");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(
        composer_text(&view, cx),
        "",
        "the pick never lands as slash text"
    );
    assert_eq!(
        session_rows(&view, cx),
        [
            ("bbbb".to_string(), "claude \u{b7} 1m".to_string()),
            ("rollout-cccc".to_string(), "codex \u{b7} 30m".to_string()),
            ("aaaa".to_string(), "claude \u{b7} 3h".to_string()),
        ],
        "newest first"
    );
    view.read_with(cx, |view, cx| {
        let palette = view.floats.palette.as_ref().unwrap().read(cx);
        // The row and the file it adopts, in place of this blank
        // Thread, ride together.
        assert!(matches!(
            palette.selected().map(|row| &row.act),
            Some(crate::palette::Act::Adopt { path, into: Some(PaneIdentity::Thread(into)) })
                if path.ends_with("bbbb.jsonl") && *into == thread
        ));
        // Nothing reached the provider: no prompt, no running turn.
        let transcript = view.cockpit.thread(thread).unwrap().transcript();
        assert!(
            !transcript
                .blocks()
                .iter()
                .any(|block| matches!(block.body, Body::Prompt(_))),
            "picking import must not prompt the provider"
        );
        assert!(!view.cockpit.thread(thread).is_some_and(|open| open.busy()));
    });

    // Typing searches; escape dismisses with the keyboard back in the
    // Composer.
    cx.simulate_input("codex");
    tick(cx);
    assert_eq!(
        session_rows(&view, cx),
        [("rollout-cccc".to_string(), "codex \u{b7} 30m".to_string())]
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.floats.palette.is_none(), "escape dismissed the picker");
    });
    cx.simulate_input("still typing");
    assert_eq!(
        composer_text(&view, cx),
        "still typing",
        "the keyboard is back in the Composer"
    );
}

/// The listing runs off the UI thread: the picker opens at once and
/// fills when it answers, with the session's own title and project.
#[gpui::test]
fn the_import_picker_fills_when_the_listing_answers(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("import-listing", 1);
    let (tx, rx) = mpsc::channel();
    *fake.session_discovery.borrow_mut() = Some(rx);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    cx.simulate_input("import a session");
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(session_rows(&view, cx).is_empty(), "still listing");

    tx.send(Ok(vec![ferrite_core::import::Candidate {
        provider: Provider::Claude,
        path: "/sessions/one.jsonl".into(),
        title: Some("Fix the startup latency\nand more".into()),
        cwd: Some("/workspace/ferrite".into()),
        session_id: Some("one".into()),
        modified: Some(std::time::SystemTime::now() - Duration::from_secs(2 * 86400)),
    }]))
    .unwrap();
    tick(cx);
    assert_eq!(
        session_rows(&view, cx),
        [(
            "Fix the startup latency".to_string(),
            "ferrite \u{b7} claude \u{b7} 2d".to_string()
        )]
    );
}

/// #11 AC: enter adopts the picked session through the core door — the
/// imported Thread opens focused with the conversation replayed and its
/// resume target set, and the blank Thread the door was opened from is
/// gone (clean by the picker's own invariant).
#[gpui::test]
fn enter_adopts_the_picked_session_and_the_blank_thread_goes(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("import-adopt", 1);
    let base = scratch("import-adopt-roots");
    let roots = session_roots(&base);
    write_session_file(
        &roots[0].1.join("-workspace-alpha").join("adopt-4f2a.jsonl"),
        &claude_session_body("adopt-4f2a"),
        60,
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.session_file_roots = roots);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let blank = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

    cx.simulate_input("/im");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        assert!(view.floats.palette.is_none(), "the pick closed the picker");
        assert_eq!(view.panes.len(), 1, "one Pane: the adopted Thread");
        let adopted = view.panes[0].thread().unwrap();
        assert_ne!(adopted, blank);
        assert_eq!(
            view.focused_thread(),
            Some(adopted),
            "the adopted Thread takes focus"
        );
        let transcript = view.cockpit.thread(adopted).unwrap().transcript();
        assert_eq!(
            transcript.session_id(),
            Some("adopt-4f2a"),
            "the next prompt resumes the file's own session"
        );
        assert!(
            transcript
                .blocks()
                .iter()
                .any(|block| matches!(&block.body, Body::Prompt(text) if text == "first question")),
            "the conversation replays: {:?}",
            transcript.blocks()
        );
        // The blank Thread is gone entirely — not parked clutter.
        assert!(view.cockpit.thread(blank).is_none());
        assert!(view.cockpit.parked().unwrap().is_empty());
    });
}

/// From ⌘K over a Thread with a conversation, the import opens beside
/// it and takes focus; the Thread it was opened over is untouched.
#[gpui::test]
fn palette_import_over_a_conversation_opens_alongside(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("import-alongside", 1);
    let base = scratch("import-alongside-roots");
    let roots = session_roots(&base);
    write_session_file(
        &roots[0].1.join("-workspace-alpha").join("beside.jsonl"),
        &claude_session_body("beside"),
        60,
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.session_file_roots = roots);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let busy = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    cx.simulate_input("hello");
    cx.simulate_keystrokes("enter");
    tick(cx);

    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    cx.simulate_input("import a session");
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, cx| {
        let palette = view.floats.palette.as_ref().unwrap().read(cx);
        assert!(matches!(
            palette.selected().map(|row| &row.act),
            Some(crate::palette::Act::Adopt { into: None, .. })
        ));
    });
    cx.simulate_keystrokes("enter");
    tick(cx);

    view.read_with(cx, |view, _| {
        let adopted = view.focused_thread().expect("the import is focused");
        assert_ne!(adopted, busy);
        assert_eq!(
            view.cockpit
                .thread(adopted)
                .unwrap()
                .transcript()
                .session_id(),
            Some("beside")
        );
        assert!(
            view.cockpit.thread(busy).is_some(),
            "the conversation it was opened over stays"
        );
    });
}

/// `/import` over a conversation still offers the picker, and its pick
/// opens alongside: the conversation is not a blank Pane to replace.
#[gpui::test]
fn slash_import_over_a_conversation_opens_alongside(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("import-slash-alongside", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| {
        view.session_file_roots = session_roots(&scratch("import-slash-alongside-roots"))
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.simulate_input("hello");
    cx.simulate_keystrokes("enter");
    tick(cx);

    cx.simulate_input("/im");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        let palette = view.floats.palette.as_ref().expect("the picker is open");
        assert_eq!(
            palette.read(cx).scope,
            crate::palette::PaletteScope::Sessions
        );
        assert_eq!(view.import_target(cx), None, "nothing to replace");
    });
}

/// `/import` typed out with the slash menu muted is the picker too,
/// like `/effort` and `/model` typed out: it never reaches the
/// provider as a prompt.
#[gpui::test]
fn typed_import_with_the_menu_muted_opens_the_picker(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("import-typed", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| {
        view.session_file_roots = session_roots(&scratch("import-typed-roots"))
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

    cx.simulate_input("/import");
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.menu_muted));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(composer_text(&view, cx), "");
    view.read_with(cx, |view, cx| {
        let palette = view.floats.palette.as_ref().expect("the picker is open");
        assert_eq!(
            palette.read(cx).scope,
            crate::palette::PaletteScope::Sessions
        );
        let transcript = view.cockpit.thread(thread).unwrap().transcript();
        assert!(
            !transcript
                .blocks()
                .iter()
                .any(|block| matches!(block.body, Body::Prompt(_))),
            "a typed /import must not prompt the provider"
        );
    });
}

/// ⇥ previews a Thread beside; on a session row it does nothing, and
/// the picker stays.
#[gpui::test]
fn tab_on_a_session_row_does_nothing(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("import-tab", 1);
    let base = scratch("import-tab-roots");
    let roots = session_roots(&base);
    write_session_file(
        &roots[0].1.join("-workspace-alpha").join("tab.jsonl"),
        &claude_session_body("tab"),
        60,
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.session_file_roots = roots);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_input("/im");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(session_rows(&view, cx).len(), 1);
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();

    assert_eq!(session_rows(&view, cx).len(), 1, "the picker stays open");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 1, "nothing opened beside");
        assert_eq!(view.cockpit.threads().len(), 1, "nothing imported");
    });
}

/// A listing that answers after its picker closed, and the palette
/// reopened in another scope, is dropped rather than filling it.
#[gpui::test]
fn a_late_listing_is_dropped_by_a_palette_reopened_elsewhere(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("import-late", 1);
    let (tx, rx) = mpsc::channel();
    *fake.session_discovery.borrow_mut() = Some(rx);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    cx.simulate_input("import a session");
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    cx.simulate_keystrokes("escape");
    tick(cx);
    cx.simulate_keystrokes("cmd-k");
    tick(cx);

    view.read_with(cx, |view, cx| {
        let palette = view.floats.palette.as_ref().expect("⌘K is open").read(cx);
        assert_ne!(palette.scope, crate::palette::PaletteScope::Sessions);
        assert!(view.pending_discovery.is_none(), "the listing was let go");
    });
    let late = tx.send(Ok(vec![ferrite_core::import::Candidate {
        provider: Provider::Claude,
        path: "/sessions/late.jsonl".into(),
        title: Some("Too late".into()),
        cwd: None,
        session_id: Some("late".into()),
        modified: None,
    }]));
    assert!(late.is_err(), "its answer has nowhere to land");
}

/// ⌘K's import over the empty board shown on purpose lands on the
/// import, not on the board it was picked from.
#[gpui::test]
fn palette_import_from_the_empty_board_shows_the_import(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("import-empty-board", 1);
    let base = scratch("import-empty-board-roots");
    let roots = session_roots(&base);
    write_session_file(
        &roots[0].1.join("-workspace-alpha").join("board.jsonl"),
        &claude_session_body("board"),
        60,
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| {
        view.session_file_roots = roots;
        view.show_empty_board(cx);
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    cx.simulate_input("import a session");
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);

    view.read_with(cx, |view, _| {
        let adopted = view.focused_thread().expect("the import is focused");
        assert_eq!(
            view.cockpit
                .thread(adopted)
                .unwrap()
                .transcript()
                .session_id(),
            Some("board")
        );
        assert!(
            !view.empty_board_shown(),
            "the board gave way to the import"
        );
    });
}

/// A refused import with no Pane to speak in says so as a toast, and
/// leaves no Thread behind.
#[gpui::test]
fn a_refused_import_with_no_pane_toasts(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("import-refused-toast", 1);
    let base = scratch("import-refused-toast-roots");
    let roots = session_roots(&base);
    write_session_file(
        &roots[1].1.join("2026").join("junk.jsonl"),
        "not a session file at all\n",
        60,
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.session_file_roots = roots);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.simulate_input("hello");
    cx.simulate_keystrokes("enter");
    tick(cx);
    let toasts = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            gpui::component::Root::read(window, cx)
                .notification
                .read(cx)
                .notifications()
                .len()
        })
    };
    let before = toasts(cx);

    cx.simulate_keystrokes("cmd-k");
    tick(cx);
    cx.simulate_input("import a session");
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);

    assert_eq!(toasts(cx), before + 1, "the refusal is a toast");
    view.read_with(cx, |view, _| {
        assert_eq!(view.panes.len(), 1, "no Thread was added");
        assert!(view.cockpit.parked().unwrap().is_empty());
    });
}

/// #11 AC: a malformed or foreign pick surfaces the core's readable
/// refusal in this Thread's transcript — never a crash — and the door
/// stays open for the next try.
#[gpui::test]
fn a_foreign_file_pick_surfaces_the_refusal_in_the_transcript(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("import-refused", 1);
    let base = scratch("import-refused-roots");
    let roots = session_roots(&base);
    write_session_file(
        &roots[1].1.join("2026").join("junk.jsonl"),
        "not a session file at all\n",
        60,
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.session_file_roots = roots);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

    cx.simulate_input("/im");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        assert!(
            view.floats.palette.is_none(),
            "the refusal closed the picker"
        );
        assert_eq!(view.panes.len(), 1);
        assert_eq!(view.panes[0].thread().unwrap(), thread, "the Thread stays");
        let transcript = view.cockpit.thread(thread).unwrap().transcript();
        assert!(
            transcript.blocks().iter().any(|block| matches!(
                &block.body,
                Body::Notice(line) if line.contains("not an importable session file")
            )),
            "the core's own words reach the transcript: {:?}",
            transcript.blocks()
        );
        assert!(
            view.cockpit.parked().unwrap().is_empty(),
            "no half-imported Thread was left"
        );
    });

    // The refusal is Ferrite speaking, not a conversation: `/` still
    // offers import for the next try.
    cx.simulate_input("/");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("the menu reopens");
        assert!(
            menu.rows.iter().any(|row| row.name.as_ref() == "/import"),
            "the import door stays open"
        );
    });
}

/// #11: with nothing to list, the picker says so and where it looked.
#[gpui::test]
fn with_no_session_files_found_the_picker_says_so(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("import-none", 1);
    let base = scratch("import-none-roots");
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.session_file_roots = session_roots(&base));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_input("/im");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert!(session_rows(&view, cx).is_empty());
    let note = format!(
        "No CLI sessions found under {}",
        base.join("claude-projects").display()
    );
    view.read_with(cx, |view, cx| {
        let palette = view.floats.palette.as_ref().unwrap().read(cx);
        assert!(
            palette.empty_note().starts_with(&note),
            "the empty listing is said out loud: {}",
            palette.empty_note()
        );
    });
}

#[gpui::test]
fn draft_import_replaces_the_draft_and_focuses_the_imported_thread(cx: &mut TestAppContext) {
    let fake = Fake::default();
    let store = Store::open(scratch("draft-import-store")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    let base = scratch("draft-import-roots");
    let roots = session_roots(&base);
    write_session_file(
        &roots[0].1.join("-workspace-alpha").join("draft.jsonl"),
        &claude_session_body("draft-import"),
        1,
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.session_file_roots = roots);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_input("/im");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("draft slash menu");
        assert_eq!(menu.rows.len(), 1);
        assert_eq!(menu.rows[0].insert.as_ref(), "import");
    });
    view.update(cx, |view, cx| {
        assert!(
            view.popover_element(0, cx).is_some(),
            "the derived Draft menu reaches its one rendered popover slot"
        );
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.floats.palette.is_some()));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    view.read_with(cx, |view, cx| {
        assert!(view.panes[0].draft().is_none());
        assert!(view.panes[0].thread().is_some());
        assert_eq!(view.panes[0].composer.read(cx).text(), "");
        assert_eq!(view.focused(), 0);
    });
}

#[gpui::test]
fn refused_draft_import_preserves_the_draft_and_command(cx: &mut TestAppContext) {
    let fake = Fake::default();
    let store = Store::open(scratch("draft-import-refused-store")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    let base = scratch("draft-import-refused-roots");
    let roots = session_roots(&base);
    write_session_file(
        &roots[0].1.join("-workspace-alpha").join("bad.jsonl"),
        "not a session\n",
        1,
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.session_file_roots = roots);
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_input("/im");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    view.read_with(cx, |view, cx| {
        let draft = view.panes[0].draft().expect("the draft survives");
        assert!(draft
            .error
            .as_ref()
            .is_some_and(|error| error.contains("cannot import")));
        assert_eq!(view.panes[0].composer.read(cx).text(), "/im");
    });
}
