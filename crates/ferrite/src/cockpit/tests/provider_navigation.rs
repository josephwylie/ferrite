use super::*;

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
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().unwrap();
        assert_eq!(menu.rows[0].name.as_ref(), "Fix startup latency");
        assert!(menu.rows[0].detail.contains("ferrite"));
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
    view.read_with(cx, |view, _| assert!(view.popover.is_none()));
}
