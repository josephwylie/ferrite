//! The Composer: history, placeholders and the slash and mention menus.
#[allow(unused_imports)]
use super::*;

#[gpui::test]
fn focused_placeholder_starts_immediately_after_the_caret(cx: &mut TestAppContext) {
    let fake = Fake::default();
    let store = Store::open(scratch("placeholder-caret-gap")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    cx.run_until_parked();

    let editor = cx.debug_bounds("focused-prompt-editor").unwrap();
    let placeholder = cx.debug_bounds("prompt-placeholder").unwrap();
    // SPEC §5.5: Solo's box has no `❯` and its caret is a bar, so the
    // placeholder starts where typed text would.
    assert!(
        (placeholder.left() - editor.left()).abs() <= px(0.5),
        "the placeholder starts at the text's start"
    );
}

#[gpui::test]
fn disabled_placeholder_suggestions_are_hidden_and_tab_does_not_accept_them(
    cx: &mut TestAppContext,
) {
    let (mut core, _fake) = cockpit("suggestion-setting-off", 1);
    let thread = core.threads()[0];
    core.deliver_suggestion(thread, "Run the tests".into());
    core.pump();
    let prefs = Preferences {
        settings: ferrite_core::settings::Settings {
            placeholder_suggestions: false,
            ..Default::default()
        },
        ..Preferences::ephemeral()
    };

    bind_band_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.simulate_keystrokes("tab");

    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.cockpit.thread(thread).unwrap().suggestion(),
            None,
            "turning the setting off clears the visible prediction"
        );
        assert!(
            view.panes[view.focused()].composer.read(cx).is_empty(),
            "Tab must not accept a disabled prediction"
        );
    });
}

#[gpui::test]
fn history_arrows_only_replace_the_composer_and_restore_the_exact_draft(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("prompt-history-view", 1);
    let thread = core.threads()[0];
    core.send(thread, "one".into());
    core.send(thread, "two".into());
    fake.streams.borrow()[0]
        .send(SessionEvent::TurnEnded {
            outcome: ferrite_core::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();

    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("  thr…  ".into(), cx));
    });
    tick(cx);

    for (key, expected) in [("up", "two"), ("up", "one")] {
        cx.simulate_keystrokes(key);
        tick(cx);
        assert_eq!(composer_text(&view, cx), expected);
    }
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("one edited".into(), cx));
    });
    for (key, expected) in [
        ("up", "one edited"),
        ("down", "two"),
        ("down", "  thr…  "),
        ("down", "  thr…  "),
    ] {
        cx.simulate_keystrokes(key);
        tick(cx);
        assert_eq!(composer_text(&view, cx), expected);
    }
    assert_eq!(fake.sent.borrow().as_slice(), ["one", "two"]);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit
                .thread(thread)
                .map(|open| open.transcript())
                .unwrap()
                .blocks()
                .iter()
                .filter(|block| matches!(block.body, Body::Prompt(_)))
                .count(),
            2
        );
    });

    cx.simulate_keystrokes("up");
    cx.simulate_input(" please");
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert_eq!(
        fake.sent.borrow().as_slice(),
        ["one", "two", "two please"],
        "editing a recall uses the ordinary send path exactly once"
    );
}

#[gpui::test]
fn recalling_a_slash_prompt_does_not_open_its_menu_until_a_real_edit(cx: &mut TestAppContext) {
    let (mut core, _) = cockpit("prompt-history-slash", 1);
    let thread = core.threads()[0];
    core.send(thread, "older".into());
    core.send(thread, "/".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);

    cx.simulate_keystrokes("up");
    tick(cx);
    assert_eq!(composer_text(&view, cx), "/");
    view.read_with(cx, |view, _| {
        assert!(
            view.popover.is_none(),
            "recall itself must not derive a menu"
        )
    });
    cx.simulate_keystrokes("up");
    tick(cx);
    assert_eq!(composer_text(&view, cx), "older");

    cx.simulate_keystrokes("down");
    cx.simulate_input("m");
    tick(cx);
    assert_eq!(composer_text(&view, cx), "/m");
    view.read_with(cx, |view, _| {
        assert!(
            view.popover.is_some(),
            "the next real edit restores menu derivation"
        )
    });
}

#[gpui::test]
fn menu_and_decision_states_disarm_history_before_their_arrows_dispatch(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("prompt-history-precedence", 1);
    let thread = core.threads()[0];
    core.send(thread, "history".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.update(|window, cx| {
        assert!(view
            .read(cx)
            .history_available(0, view.read(cx).level_now(window)))
    });

    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("/".into(), cx));
    });
    tick(cx);
    view.read_with(cx, |view, _| assert!(view.popover.is_some()));
    cx.update(|window, cx| {
        assert!(!view
            .read(cx)
            .history_available(0, view.read(cx).level_now(window)))
    });
    cx.simulate_keystrokes("up");
    assert_eq!(composer_text(&view, cx), "/", "menu owns the arrow");

    view.update(cx, |view, cx| {
        view.popover = None;
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.set("draft".into(), cx));
    });
    fake.streams.borrow()[0]
        .send(decision("history-decision"))
        .unwrap();
    tick(cx);
    cx.update(|window, cx| {
        assert!(!view
            .read(cx)
            .history_available(0, view.read(cx).level_now(window)))
    });
    cx.simulate_keystrokes("up");
    assert_eq!(composer_text(&view, cx), "draft", "Decision owns its state");
}

/// Slash completion follows the caret anywhere; mentions still require
/// a token boundary so an email address cannot open file completion.
#[test]
fn the_slash_and_mention_triggers_parse_the_line() {
    assert_eq!(slash_token("/", 1), Some((0, "")));
    assert_eq!(slash_token("/co", 3), Some((0, "co")));
    assert_eq!(
        slash_token("/compact now", 12),
        None,
        "a space ends the command"
    );
    assert_eq!(slash_token("say /compact", 12), Some((4, "compact")));
    assert_eq!(slash_token("say/co", 6), Some((3, "co")));
    assert_eq!(slash_token("🦀\n/co", 8), Some((5, "co")));
    assert_eq!(slash_token("say /co later", 7), Some((4, "co")));
    assert_eq!(slash_token("/commit and /co", 15), Some((12, "co")));
    assert_eq!(
        slash_token("/browser:control", 16),
        Some((0, "browser:control"))
    );
    for text in [
        "say /",
        "/ ",
        "/ co",
        "/co ",
        "/co\t",
        "/co\n",
        "/co\u{a0}",
        "/1",
        "/-co",
    ] {
        assert_eq!(slash_token(text, text.len()), None, "{text:?}");
    }
    assert_eq!(slash_token("🦀 /co", 1), None, "invalid UTF-8 cursor");

    assert_eq!(mention_token("@", 1), Some((0, "")));
    assert_eq!(mention_token("fix @Xte", 8), Some((4, "Xte")));
    assert_eq!(
        mention_token("fix @Xte now", 12),
        None,
        "the caret left the token"
    );
    assert_eq!(
        mention_token("mail a@b.example", 16),
        None,
        "interior @ is prose"
    );
    assert_eq!(mention_token("no token here", 13), None);
}

/// The `/` rows: fuzzy-filtered, best first, highlights shifted past the
/// drawn `/`, the description riding as prose detail.
#[test]
#[allow(clippy::single_range_in_vec_init)] // assertions compare literal ranges
fn command_rows_filter_and_highlight_by_fuzzy_match() {
    let commands = menu_commands();
    let all = command_rows(&commands, "");
    assert_eq!(all.len(), 4, "an empty filter lists everything");

    let rows = command_rows(&commands, "co");
    let names: Vec<&str> = rows.iter().map(|row| row.name.as_ref()).collect();
    assert_eq!(
        names,
        ["/code-review", "/commit", "/compact"],
        "to-tickets has no `co` subsequence"
    );
    assert_eq!(rows[0].matched, [1..3], "highlights sit past the drawn /");
    assert_eq!(rows[0].insert.as_ref(), "code-review");
    assert!(rows[0].prose_detail);
    assert!(command_rows(&commands, "zzz").is_empty());
}

/// The `@` rows: name and directory split apart, the path the pick
/// inserts kept whole.
#[test]
#[allow(clippy::single_range_in_vec_init)]
fn mention_rows_split_name_from_directory() {
    let files = vec!["README.md".to_string(), "src/lib.rs".to_string()];
    let rows = mention_rows(&files, "");
    assert_eq!(rows[0].name.as_ref(), "README.md");
    assert_eq!(rows[0].detail.as_ref(), "");
    assert_eq!(rows[1].name.as_ref(), "lib.rs");
    assert_eq!(rows[1].detail.as_ref(), "src");
    assert!(!rows[1].prose_detail);

    let rows = mention_rows(&files, "lib");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].insert.as_ref(), "src/lib.rs");
    assert_eq!(rows[0].matched, [0..3], "highlights land inside the name");

    // A directory row keeps its trailing slash in the name and inserts
    // whole, so `@src/nested/ ` reads as the folder it is.
    let dirs = vec!["src/".to_string(), "src/nested/".to_string()];
    let rows = mention_rows(&dirs, "nest");
    assert_eq!(rows[0].name.as_ref(), "nested/");
    assert_eq!(rows[0].detail.as_ref(), "src");
    assert_eq!(rows[0].insert.as_ref(), "src/nested/");
}

/// #23: `/` at the line's start opens the Session's own menu, typing
/// filters it, ↓/↵ pick — and the pick lands as `/name ` ready for args.
#[gpui::test]
fn typing_slash_opens_the_command_menu_and_enter_inserts_the_pick(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("slash-menu", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    // The Session announces its menu — the popover's only source.
    fake.streams.borrow()[0]
        .send(SessionEvent::Commands {
            commands: menu_commands(),
        })
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.popover.is_none(),
            "nothing opens until the operator types"
        );
    });

    cx.simulate_input("/");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("/ opens the menu");
        // Everything the Session listed — plus, on this still-fresh
        // Thread, Ferrite's own model (#25), effort and import (#11)
        // entries on top.
        assert_eq!(menu.rows.len(), 7);
        assert_eq!(menu.selected, 0);
    });

    cx.simulate_input("co");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("still open while filtering");
        let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(names, ["/code-review", "/commit", "/compact"]);
    });

    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.popover.as_ref().expect("open").selected, 1);
    });

    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(composer_text(&view, cx), "/commit ");
    view.read_with(cx, |view, _| {
        assert!(view.popover.is_none(), "the pick closed the menu");
    });
}

#[gpui::test]
fn skill_picker_opens_after_prose_and_preserves_it_when_picking(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("skill-picker-inline", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::Commands {
            commands: menu_commands(),
        })
        .unwrap();
    tick(cx);

    cx.simulate_input("Please review this with /co");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("/co after prose opens skills");
        assert_eq!(menu.rows[menu.selected].name.as_ref(), "/code-review");
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        composer_text(&view, cx),
        "Please review this with /code-review "
    );
    view.read_with(cx, |view, _| assert!(view.popover.is_none()));
    assert!(
        fake.sent.borrow().is_empty(),
        "picking must not send a prompt"
    );
}

#[gpui::test]
fn skill_picker_tab_inserts_the_highlighted_skill(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("skill-picker-tab", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::Commands {
            commands: menu_commands(),
        })
        .unwrap();
    tick(cx);

    cx.simulate_input("/co");
    cx.run_until_parked();
    cx.simulate_keystrokes("down tab");
    cx.run_until_parked();
    assert_eq!(composer_text(&view, cx), "/commit ");
    view.read_with(cx, |view, _| assert!(view.popover.is_none()));
    assert!(fake.sent.borrow().is_empty(), "Tab must not send a prompt");
}

#[gpui::test]
fn skill_picker_tab_inserts_the_hovered_skill(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("skill-picker-hover-tab", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::Commands {
            commands: menu_commands(),
        })
        .unwrap();
    tick(cx);
    cx.simulate_input("/co");
    cx.run_until_parked();

    let row = cx.debug_bounds("composer-menu-row-2").expect("compact row");
    cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.popover.as_ref().unwrap().selected, 2)
    });
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_eq!(composer_text(&view, cx), "/compact ");
    view.read_with(cx, |view, _| assert!(view.popover.is_none()));
    assert!(fake.sent.borrow().is_empty());
}

#[gpui::test]
fn skill_picker_follows_the_caret_and_closes_on_whitespace(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("skill-picker-caret", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::Commands {
            commands: menu_commands(),
        })
        .unwrap();
    tick(cx);

    cx.simulate_input("🦀 Review /cod carefully");
    cx.simulate_keystrokes("alt-left left left");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.popover.is_some()));
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_eq!(composer_text(&view, cx), "🦀 Review /code-review carefully");
    view.read_with(cx, |view, _| assert!(view.popover.is_none()));

    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("Please /co");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.popover.is_some()));
    cx.simulate_input(" ");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.popover.is_none()));
    cx.simulate_input("more");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.popover.is_none()));
    cx.simulate_input(" /co");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.popover.is_some()));
    assert!(fake.sent.borrow().is_empty());
}

/// Escape closes the menu and only the menu: the text stays, escape's
/// Interrupt meaning waits for the next press, and more typing reopens.
#[gpui::test]
fn escape_dismisses_the_menu_keeps_the_text_and_typing_reopens(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("slash-escape", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::Commands {
            commands: menu_commands(),
        })
        .unwrap();
    tick(cx);

    cx.simulate_input("/c");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.popover.is_some()));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.popover.is_none(), "escape dismissed the popover");
    });
    assert_eq!(composer_text(&view, cx), "/c", "and kept the text");

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(
            view.popover.is_none(),
            "a second escape is Interrupt, not a reopen"
        );
    });

    cx.simulate_input("o");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.popover.is_some(), "typing again reopens the menu");
    });
}

/// #23: `@` opens the file menu over the Thread's workspace binding;
/// the pick lands as `@relative/path ` in the line.
#[gpui::test]
fn typing_at_completes_files_from_the_workspace_binding(cx: &mut TestAppContext) {
    let (core, _fake, _checkout) = bound_cockpit("mention-menu", Provider::Claude);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_input("read ");
    cx.simulate_input("@");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("@ opens the file menu");
        let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(
            names,
            ["README.md", "src/", "lib.rs"],
            "the walk, breadth-first, folders too"
        );
    });

    cx.simulate_input("src/li");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("open");
        assert_eq!(menu.rows.len(), 1, "the fuzzy filter narrowed it");
        assert_eq!(menu.rows[0].insert.as_ref(), "src/lib.rs");
    });

    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(composer_text(&view, cx), "read @src/lib.rs ");
    view.read_with(cx, |view, cx| {
        assert!(view.popover.is_none());
        // The pill is provider-agnostic: a Claude pick paints it too —
        // the wire stays plain `@path` text the CLI itself reads.
        assert_eq!(
            view.panes[0].composer.read(cx).mentions(),
            [SharedString::from("@src/lib.rs")],
            "the picked token is staged as the comp's pill"
        );
    });
}

#[gpui::test]
fn draft_mentions_are_scoped_to_the_selected_project(cx: &mut TestAppContext) {
    let project = scratch("draft-mentions-project");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("only-here.txt"), "project\n").unwrap();
    let elsewhere = scratch("draft-mentions-elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("not-here.txt"), "elsewhere\n").unwrap();
    let fake = Fake::default();
    let store = Store::open(scratch("draft-mentions-store")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| view.aim_launch(&project));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_input("@");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let names: Vec<&str> = view
            .popover
            .as_ref()
            .expect("draft @ menu")
            .rows
            .iter()
            .map(|row| row.name.as_ref())
            .collect();
        assert_eq!(names, ["only-here.txt"]);
    });
}

#[gpui::test]
fn draft_mentions_follow_the_selected_workspace(cx: &mut TestAppContext) {
    let base = scratch("draft-mention-workspace");
    let repo = repo_in(&base);
    std::fs::write(repo.join("main-only.txt"), "main\n").unwrap();
    let fake = Fake::default();
    let store = Store::open(base.join("threads")).unwrap();
    let core = Cockpit::new(store, Box::new(fake));
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| {
        view.aim_launch(&repo);
        let thread = view
            .cockpit
            .open(
                Provider::Claude,
                WorkspaceChoice::NewWorktree { repo: repo.clone() },
            )
            .unwrap();
        view.cockpit.park(thread).unwrap();
        let focused = view.focused();
        let entry = view.panes[focused].draft().unwrap().binding.project();
        let worktree = view.cockpit.registry().worktrees(entry)[0].clone();
        std::fs::write(worktree.path.join("worktree-only.txt"), "tree\n").unwrap();
        view.panes[focused]
            .draft_mut()
            .unwrap()
            .binding
            .choose_target(DraftTarget::Existing {
                branch: worktree.branch,
                path: worktree.path,
            });
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_input("@");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let names: Vec<&str> = view
            .popover
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .map(|r| r.name.as_ref())
            .collect();
        assert!(names.contains(&"worktree-only.txt"), "rows: {names:?}");
    });

    view.update(cx, |view, cx| {
        view.panes[view.focused()]
            .composer
            .update(cx, |composer, cx| composer.set(String::new(), cx));
        let focused = view.focused();
        view.panes[focused]
            .draft_mut()
            .unwrap()
            .binding
            .choose_target(DraftTarget::New);
    });
    cx.simulate_input("@");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let names: Vec<&str> = view
            .popover
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .map(|r| r.name.as_ref())
            .collect();
        assert!(names.contains(&"main-only.txt"), "rows: {names:?}");
        assert!(!names.contains(&"worktree-only.txt"), "rows: {names:?}");
    });

    // Back on the existing worktree: complete its unique file and
    // send. The resulting Thread keeps that same directory, not the
    // Project checkout.
    view.update(cx, |view, cx| {
        view.panes[view.focused()]
            .composer
            .update(cx, |composer, cx| composer.set(String::new(), cx));
        let focused = view.focused();
        let project = view.panes[focused].draft().unwrap().binding.project();
        let worktree = view.cockpit.registry().worktrees(project)[0].clone();
        view.panes[focused]
            .draft_mut()
            .unwrap()
            .binding
            .choose_target(DraftTarget::Existing {
                branch: worktree.branch,
                path: worktree.path,
            });
    });
    cx.simulate_input("@worktree-only");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    assert_eq!(composer_text(&view, cx), "@worktree-only.txt ");
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, _| {
        let thread = view.panes[view.focused()]
            .thread()
            .expect("first send bound the Thread");
        let cwd = view
            .cockpit
            .thread(thread)
            .unwrap()
            .workspace()
            .unwrap()
            .cwd();
        assert!(cwd.join("worktree-only.txt").is_file());
        assert!(!cwd.join("main-only.txt").exists());
    });
}

#[gpui::test]
fn a_stale_draft_workspace_neither_completes_nor_sends_from_main(cx: &mut TestAppContext) {
    let base = scratch("draft-stale-workspace");
    let repo = repo_in(&base);
    std::fs::write(repo.join("main-only.txt"), "main\n").unwrap();
    let fake = Fake::default();
    let core = Cockpit::new(
        Store::open(base.join("threads")).unwrap(),
        Box::new(fake.clone()),
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    view.update(cx, |view, _| {
        view.aim_launch(&repo);
        view.focused_draft_mut()
            .unwrap()
            .binding
            .choose_target(DraftTarget::Existing {
                branch: "removed-worktree".into(),
                path: base.join("removed-worktree"),
            });
    });
    tick(cx);
    cx.simulate_input("@main-only");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.popover.is_none()));
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        let draft = view.panes[view.focused()].draft().expect("still a draft");
        assert!(draft.error.as_ref().unwrap().contains("removed-worktree"));
    });
    assert_eq!(composer_text(&view, cx), "@main-only");
    assert!(fake.streams.borrow().is_empty());
}

/// A Thread with no binding has nothing to walk: `@` opens nothing and
/// typing carries on.
#[gpui::test]
fn a_thread_without_a_binding_opens_no_file_menu(cx: &mut TestAppContext) {
    let dir = scratch("mention-unbound");
    let thread_dir = dir.join("9");
    std::fs::create_dir_all(&thread_dir).unwrap();
    std::fs::write(
        thread_dir.join("log.jsonl"),
        concat!(
            r#"{"schema":2,"provider":"claude"}"#,
            "\n",
            r#"{"type":"prompt","text":"hello"}"#,
            "\n",
        ),
    )
    .unwrap();
    let fake = Fake::default();
    let store = Store::open(&dir).unwrap();
    let mut core = Cockpit::new(store, Box::new(fake));
    core.revive(ThreadId::new(9)).unwrap();
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_input("@");
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        assert!(view.popover.is_none(), "no binding, no popover");
    });
    assert_eq!(composer_text(&view, cx), "@", "typing was not eaten");
}

fn local_render_probe(id: u64, cx: &mut TestAppContext) {
    let fake = Fake::default();
    let store = Store::open("/tmp/ferrite-perf.7xYdTK").unwrap();
    let mut core = Cockpit::new(store, Box::new(fake.clone()));
    let revive = std::time::Instant::now();
    core.revive(ThreadId::new(id)).unwrap();
    let revive = revive.elapsed();
    bind_production_keys(cx);
    let render = std::time::Instant::now();
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let render = render.elapsed();
    fake.streams.borrow()[0]
        .send(SessionEvent::TextDelta { text: "x".into() })
        .unwrap();
    let pump = std::time::Instant::now();
    _view.update(cx, |view, cx| view.pump(cx));
    let pump = pump.elapsed();
    let repaint = std::time::Instant::now();
    cx.run_until_parked();
    let repaint = repaint.elapsed();
    eprintln!(
        "PERF_UI thread={id} revive_ms={:.3} initial_ms={:.3} pump_ms={:.3} repaint_ms={:.3}",
        revive.as_secs_f64() * 1000.0,
        render.as_secs_f64() * 1000.0,
        pump.as_secs_f64() * 1000.0,
        repaint.as_secs_f64() * 1000.0,
    );
}

#[gpui::test]
#[ignore = "local performance probe"]
fn local_render_probe_small(cx: &mut TestAppContext) {
    local_render_probe(1, cx);
}

#[gpui::test]
#[ignore = "local performance probe"]
fn local_render_probe_slow(cx: &mut TestAppContext) {
    local_render_probe(5, cx);
}

/// #24's dismissal law holds for the menus: a press the popover did not
/// swallow closes it, and it stays shut until the text moves.
#[gpui::test]
fn a_press_on_the_transcript_dismisses_the_open_menu(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("menu-press-dismiss", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    fake.streams.borrow()[0]
        .send(SessionEvent::Commands {
            commands: menu_commands(),
        })
        .unwrap();
    tick(cx);
    cx.simulate_input("/");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.popover.is_some()));

    // The middle of the Pane's transcript — nowhere near the popover.
    cx.simulate_mouse_down(
        gpui::point(px(600.), px(200.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        assert!(view.popover.is_none(), "the press dismissed the popover");
    });
    assert_eq!(composer_text(&view, cx), "/", "the text survived the press");
}

/// #23: while a Decision pends at L1 the keyboard stays in the Composer
/// — the input is live (typing queues, since the turn is running) and
/// the empty line makes y the keycap's answer.
#[gpui::test]
fn a_pending_decision_keeps_the_composer_live_and_an_empty_line_answers(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("decision-live", 1);
    bind_production_keys(cx);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0]
        .send(SessionEvent::ToolStarted {
            id: "toolu_1".into(),
            name: "Write".into(),
            input: serde_json::json!({ "file_path": "ferrite-perm.txt" }),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(decision("perm_live"))
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(
            view.cockpit
                .thread(thread)
                .and_then(|open| open.pending())
                .is_some(),
            "the card is up"
        );
        assert!(
            view.cockpit.thread(thread).is_some_and(|open| open.busy()),
            "the turn is running"
        );
    });

    // The input is still live: typing lands, enter queues behind the
    // turn. (The first key of an empty line is where y/n/a mean their
    // keycaps, so the sentence starts past them.)
    cx.simulate_input("fix the tests too");
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.thread(thread).and_then(|open| open.queued()),
            Some("fix the tests too")
        );
        assert!(
            view.cockpit
                .thread(thread)
                .and_then(|open| open.pending())
                .is_some(),
            "typing answered nothing"
        );
    });

    // Emptied, y is the keycap's answer.
    cx.simulate_keystrokes("y");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(
            view.cockpit
                .thread(thread)
                .and_then(|open| open.pending())
                .is_none(),
            "y on the empty line answered the Decision"
        );
    });
}

/// The other half of the y/n/a rule: with text on the line they are
/// letters — an operator half-way through a word keeps typing it.
#[gpui::test]
fn the_answer_keys_stay_letters_while_the_line_holds_text(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("decision-letters", 1);
    bind_production_keys(cx);
    cx.update(|cx| {
        cx.bind_keys([KeyBinding::new("y", Allow, Some("Decision"))]);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0]
        .send(decision("perm_type"))
        .unwrap();
    tick(cx);

    cx.simulate_input("wait");
    cx.simulate_keystrokes("y");
    cx.run_until_parked();

    assert_eq!(
        composer_text(&view, cx),
        "waity",
        "y typed instead of answering"
    );
    view.read_with(cx, |view, _| {
        assert!(
            view.cockpit
                .thread(thread)
                .and_then(|open| open.pending())
                .is_some(),
            "the Decision is still waiting"
        );
    });
}

/// #23: the Session's announced permission mode becomes the meta row's
/// chip state — display-only, absent until announced, and rendered
/// through the same frame the assertions ride.
#[gpui::test]
fn the_announced_permission_mode_reaches_the_meta_row_state(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("mode-chip", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit
                .thread(thread)
                .and_then(|open| open.permission_mode()),
            None,
            "no chip is invented before the Session speaks"
        );
    });

    fake.streams.borrow()[0]
        .send(SessionEvent::PermissionMode {
            mode: "acceptEdits".into(),
        })
        .unwrap();
    tick(cx);

    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit
                .thread(thread)
                .and_then(|open| open.permission_mode()),
            Some("acceptEdits")
        );
    });
}

/// #23: on a Codex Thread a picked file also stages the @-pill — the
/// send will carry the typed mention item, and the input paints the
/// token as the comp draws it.
#[gpui::test]
fn picking_a_mention_on_a_codex_thread_stages_the_pill(cx: &mut TestAppContext) {
    let (core, _fake, _checkout) = bound_cockpit("mention-codex", Provider::Codex);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_input("@li");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(composer_text(&view, cx), "@src/lib.rs ");
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.panes[0].composer.read(cx).mentions(),
            [SharedString::from("@src/lib.rs")],
            "the pill token is staged for the paint"
        );
    });
}
