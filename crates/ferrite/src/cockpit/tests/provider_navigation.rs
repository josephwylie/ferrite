use super::*;

#[gpui::test]
fn contract_native_file_menu_excludes_git_metadata_and_duplicate_paths(cx: &mut TestAppContext) {
    let (core, fake, _checkout) = bound_cockpit("native-file-filter", Provider::Codex);
    *fake.native_files.borrow_mut() = true;
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.simulate_input("@c");
    cx.run_until_parked();
    tick(cx);
    let reply = fake.file_searches.borrow().last().unwrap().1.clone();
    let file = |path: &str| ferrite_core::providers::FileSuggestion {
        path: path.into(),
        is_directory: false,
        matched: vec![],
    };
    // A live Codex response put more than one menu's worth of .git backup
    // paths before the working files. Filtering must happen before the cap.
    let mut files: Vec<_> = (0..12)
        .map(|n| file(&format!(".git/backup-{n}/cockpit.rs")))
        .collect();
    files.extend([
        file("src/cockpit.rs"),
        file("src/cockpit.rs"),
        file("other/cockpit.rs"),
        file("nested/.git/config"),
        file(".gitignore"),
        file(".github/CODEOWNERS"),
        file("/external/cockpit.rs"),
    ]);
    reply.send(Ok(files)).unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.popover
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .map(|row| row.insert.as_ref())
                .collect::<Vec<_>>(),
            [
                "src/cockpit.rs",
                "other/cockpit.rs",
                ".gitignore",
                ".github/CODEOWNERS",
                "/external/cockpit.rs"
            ]
        );
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(composer_text(&view, cx), "@src/cockpit.rs ");
    assert!(fake.sent.borrow().is_empty());
}

#[gpui::test]
fn contract_native_files_keep_provider_order_and_ignore_old_queries(cx: &mut TestAppContext) {
    let (core, fake, _checkout) = bound_cockpit("native-file-menu", Provider::Codex);
    *fake.native_files.borrow_mut() = true;
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.simulate_input("@s");
    cx.run_until_parked();
    tick(cx);
    assert!(!fake.file_searches.borrow().is_empty());
    let old = fake.file_searches.borrow().last().unwrap().1.clone();
    cx.simulate_input("r");
    cx.run_until_parked();
    tick(cx);
    let (query, reply) = fake.file_searches.borrow().last().unwrap().clone();
    assert_eq!(query, "sr");
    reply
        .send(Ok(vec![
            ferrite_core::providers::FileSuggestion {
                path: "src/zé.rs".into(),
                is_directory: false,
                matched: vec![5],
            },
            ferrite_core::providers::FileSuggestion {
                path: "src/a.rs".into(),
                is_directory: false,
                matched: vec![0, 1],
            },
        ]))
        .unwrap();
    tick(cx);
    let _ = old.send(Ok(vec![ferrite_core::providers::FileSuggestion {
        path: "stale.rs".into(),
        is_directory: false,
        matched: vec![],
    }]));
    tick(cx);
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().unwrap();
        assert_eq!(
            menu.rows
                .iter()
                .map(|r| r.name.as_ref())
                .collect::<Vec<_>>(),
            ["zé.rs", "a.rs"]
        );
        assert_eq!(menu.rows[0].matched, [1..3]);
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(composer_text(&view, cx), "@src/zé.rs ");
    assert!(fake.sent.borrow().is_empty());
}

#[gpui::test]
fn contract_native_session_picker_shows_titles_without_spawning(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("native-session-picker", 1);
    let (tx, rx) = mpsc::channel();
    *fake.session_discovery.borrow_mut() = Some(rx);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let count = fake.spawned.borrow().len();
    cx.simulate_input("/import");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    tx.send(Ok(vec![ferrite_core::import::Candidate {
        provider: Provider::Codex,
        path: "/sessions/native.jsonl".into(),
        title: Some("Fix startup latency".into()),
        cwd: Some("/workspace/ferrite".into()),
        session_id: Some("native-id".into()),
        modified: Some(std::time::SystemTime::now()),
    }]))
    .unwrap();
    tick(cx);
    view.read_with(cx, |view, cx| {
        let palette = view.floats.palette.as_ref().unwrap().read(cx);
        let rows = palette.survivors();
        assert_eq!(rows[0].name.as_ref(), "Fix startup latency");
        assert!(rows[0].context.contains("ferrite"));
    });
    assert_eq!(fake.spawned.borrow().len(), count);
    assert!(fake.sent.borrow().is_empty());
}

#[gpui::test]
fn contract_native_discovery_dismissal_does_not_reopen_picker(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("native-session-dismiss", 1);
    let (tx, rx) = mpsc::channel();
    *fake.session_discovery.borrow_mut() = Some(rx);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.simulate_input("/import");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter escape");
    cx.run_until_parked();
    let _ = tx.send(Ok(vec![]));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.popover.is_none() && view.floats.palette.is_none())
    });
}

/// Folded (cmd-B), the column is the rail and the board takes the width;
/// the rail keeps the sidebar toggle at its top, under the head row
/// (§5.1.7), and the toggle opens the column again.
#[gpui::test]
fn a_folded_nav_keeps_its_toggle_at_the_rails_top(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("folded-nav-doors", 3);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(600.)));
    view.update(cx, |view, cx| view.set_nav_collapsed(true, cx));
    tick(cx);

    let first = view.read_with(cx, |view, _| view.nav_state().ordered_rows()[0].thread);
    let row: &'static str = format!("nav-thread-{}", first.get()).leak();
    assert!(
        cx.debug_bounds(row).is_none(),
        "folded, the column draws no tree rows: it is the rail"
    );
    assert!(
        (cx.debug_bounds("nav-column").unwrap().size.width - px(nav::FOLDED_WIDTH)).abs() < px(0.5)
    );
    let toggle = bounds(cx, "nav-rail-collapse".to_string());
    assert!(toggle.top() >= px(crate::theme::WIN_CHROME_H) - px(0.5));
    assert!(toggle.right() <= px(nav::FOLDED_WIDTH));
    assert_eq!(toggle.size.width, px(crate::theme::ICON_BUTTON));
    assert!(
        cx.debug_bounds("nav-collapse").is_none(),
        "the bar no longer carries it"
    );

    cx.simulate_click(toggle.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(!view.read_with(cx, |view, _| view.nav_railed()));
    assert!(
        cx.debug_bounds(row).is_some(),
        "open again, the rows are back"
    );
}
