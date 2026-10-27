//! Settings and the menus that reach it.
#[allow(unused_imports)]
use super::*;

/// A menu row names the key that does the same thing: a chord the
/// keymap binds, never one it has moved (⌘F is find, not fullscreen).
#[gpui::test]
fn a_menu_shortcut_is_a_bound_chord(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("menu-shortcuts", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let bound: Vec<String> = ferrite_ui::keymap::bindings(ferrite_ui::keymap::PLATFORM)
        .into_iter()
        .map(|(keys, _, _)| ferrite_ui::components::spell_chord(&keys))
        .collect();
    view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        for target in [MenuTarget::Thread(thread), MenuTarget::Pane(thread)] {
            for (item, _) in view.context_rows(target).into_iter().flatten() {
                if let Some(keys) = &item.shortcut {
                    assert!(
                        bound.iter().any(|chord| chord == keys.as_ref()),
                        "{}: {keys} is bound to nothing",
                        item.label
                    );
                }
            }
        }
    });
    let fullscreen = view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().unwrap();
        view.context_rows(MenuTarget::Pane(thread))
            .into_iter()
            .flatten()
            .find(|(_, verb)| *verb == MenuVerb::Fullscreen)
            .and_then(|(item, _)| item.shortcut)
    });
    assert_eq!(fullscreen.as_deref(), Some("cmd-shift-⏎"));
}

/// A right-click on a Pane summons the menu for its Thread; escape
/// closes it; a destructive row arms on the first press and runs on
/// the second — a deleted Thread is gone from the grid and the store.
#[gpui::test]
fn a_right_click_summons_the_menu_and_delete_takes_two_presses(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("context-menu", 2);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

    // Right-click inside the focused Pane's transcript.
    cx.simulate_mouse_down(
        gpui::point(px(600.), px(300.)),
        gpui::MouseButton::Right,
        gpui::Modifiers::none(),
    );
    cx.run_until_parked();
    let (delete, count) = view.read_with(cx, |view, _| {
        let menu = view
            .context_menu
            .as_ref()
            .expect("a right-click opens the menu");
        assert_eq!(menu.target, MenuTarget::Pane(thread));
        let labels: Vec<&str> = menu
            .rows
            .iter()
            .flatten()
            .map(|(item, _)| item.label.as_ref())
            .collect();
        assert!(labels.contains(&"Rename"), "{labels:?}");
        assert!(labels.contains(&"Park thread"), "{labels:?}");
        assert!(labels.contains(&"Copy transcript"), "{labels:?}");
        // The transcript's menu is about what is on screen; deleting
        // a Thread is the nav row's act.
        assert!(!labels.contains(&"Delete thread"), "{labels:?}");
        let delete = view
            .context_rows(MenuTarget::Thread(thread))
            .iter()
            .position(|row| matches!(row, Some((_, MenuVerb::Delete))))
            .expect("the nav row's menu deletes");
        (
            delete,
            view.cockpit.threads().len() + view.cockpit.parked().unwrap().len(),
        )
    });

    // Escape closes it without running anything.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.context_menu.is_none()));

    view.update_in(cx, |view, window, cx| {
        view.open_context_menu(
            MenuTarget::Thread(thread),
            gpui::point(px(600.), px(300.)),
            cx,
        );
        view.press_menu_row(delete, window, cx);
        let menu = view.context_menu.as_ref().expect("armed, still up");
        assert_eq!(menu.armed, Some(delete), "the first press only arms");
        view.press_menu_row(delete, window, cx);
        assert!(
            view.context_menu.is_none(),
            "the second press runs and closes"
        );
        assert!(
            view.cockpit.thread(thread).is_none(),
            "the Thread is gone from the grid"
        );
        assert_eq!(
            view.cockpit.threads().len() + view.cockpit.parked().unwrap().len(),
            count - 1,
            "and from the store"
        );
        assert!(view.panes.iter().all(|pane| pane.thread() != Some(thread)));
    });
}

/// With the sheet itself focused, as it opens, ↓ and ↑ step the
/// sidebar's pages and hold at the ends; a path in About copies whole
/// on a click and shows its check until the sheet closes; reopening
/// starts on the first page again.
#[gpui::test]
fn settings_arrows_step_pages_and_about_paths_copy(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("settings-keys", 1);
    bind_production_keys(cx);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| CockpitView::new(core, cx));
        gpui::component::Root::new(
            cx.new(|cx| crate::cockpit_window::CockpitWindow::new(view, window, cx)),
            window,
            cx,
        )
    });
    let view = root.read_with(cx, |root, cx| {
        crate::cockpit_window::testing::cockpit(root.view(), cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    let page = |cx: &mut gpui::VisualTestContext| view.read_with(cx, |view, _| view.settings_page);
    assert_eq!(page(cx), prefs::PageKey::NewThreads);
    cx.simulate_keystrokes("down");
    tick(cx);
    assert_eq!(page(cx), prefs::PageKey::Permissions);
    assert!(cx.debug_bounds("settings-codex-sandbox").is_some());
    assert!(cx.debug_bounds("settings-provider-0").is_none());
    cx.simulate_keystrokes("down down down down");
    tick(cx);
    assert_eq!(page(cx), prefs::PageKey::About, "held at the foot");
    cx.simulate_keystrokes("up");
    tick(cx);
    assert_eq!(page(cx), prefs::PageKey::Server);
    cx.simulate_keystrokes("up up");
    tick(cx);
    assert_eq!(page(cx), prefs::PageKey::Behaviour);
    cx.simulate_keystrokes("down down down");
    tick(cx);

    let threads = cx
        .debug_bounds("settings-fact-Threads")
        .expect("About lists the threads directory")
        .center();
    cx.simulate_click(threads, gpui::Modifiers::none());
    tick(cx);
    let dir = view.read_with(cx, |view, _| {
        assert_eq!(view.settings_copied, Some("Threads"));
        view.prefs.dir.join("threads").display().to_string()
    });
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(dir),
        "the whole path, never the ~ form"
    );
    assert!(view.read_with(cx, |view, _| view.settings_open));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    assert_eq!(page(cx), prefs::PageKey::NewThreads);
    view.read_with(cx, |view, _| assert_eq!(view.settings_copied, None));
}

/// cmd-, opens the Settings panel and escape closes it; a chip press
/// writes the setting and saves it to disk at once, and the Session
/// defaults the spawner reads follow.
/// cmd-= / cmd-- step the transcript's text size one step at a time,
/// stopping at either end, cmd-0 returns it to 13, and every step is
/// saved like a Settings change.
#[gpui::test]
fn cmd_plus_and_minus_step_the_reading_size_and_save_it(cx: &mut TestAppContext) {
    use ferrite_core::settings::{ReadingSize, Settings};
    let (core, _fake) = cockpit("text-size-keys", 1);
    bind_production_keys(cx);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| CockpitView::new(core, cx));
        gpui::component::Root::new(
            cx.new(|cx| crate::cockpit_window::CockpitWindow::new(view, window, cx)),
            window,
            cx,
        )
    });
    let view = root.read_with(cx, |root, cx| {
        crate::cockpit_window::testing::cockpit(root.view(), cx)
    });
    tick(cx);
    let size = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            let saved = Settings::load(&view.prefs.dir).unwrap().reading_size;
            assert_eq!(saved, view.prefs.settings.reading_size, "saved at once");
            saved
        })
    };
    for (keys, expected) in [
        ("cmd-=", 14),
        ("cmd-=", 15),
        ("cmd-=", 16),
        ("cmd-0", 13),
        ("cmd--", 12),
        ("cmd--", 12),
    ]
    .map(|(keys, px)| (keys, ReadingSize::nearest(px)))
    {
        cx.simulate_keystrokes(keys);
        cx.run_until_parked();
        assert_eq!(size(cx), expected, "after {keys}");
    }
}

#[gpui::test]
fn settings_open_on_cmd_comma_and_every_change_saves(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("settings-panel", 1);
    bind_production_keys(cx);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| CockpitView::new(core, cx));
        gpui::component::Root::new(
            cx.new(|cx| crate::cockpit_window::CockpitWindow::new(view, window, cx)),
            window,
            cx,
        )
    });
    let view = root.read_with(cx, |root, cx| {
        crate::cockpit_window::testing::cockpit(root.view(), cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.settings_open, "cmd-, opens the panel")
    });
    let search = cx
        .debug_bounds("settings-search")
        .expect("the search field tops the sidebar");
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_input("Suggest follow-up prompts");
    tick(cx);
    assert!(
        cx.debug_bounds("settings-placeholder-suggestions")
            .is_some(),
        "Behaviour exposes the follow-up suggestion toggle"
    );
    assert!(
        cx.debug_bounds("settings-provider-0").is_none()
            && cx.debug_bounds("settings-confirm-delete").is_none(),
        "search filters out every row that does not match"
    );
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-a backspace"
    } else {
        "ctrl-a backspace"
    });
    tick(cx);
    assert!(
        cx.debug_bounds("settings-provider-0").is_some(),
        "an empty search shows the selected page whole"
    );
    let card = cx.debug_bounds("settings-card").unwrap();
    let behaviour = cx.debug_bounds("settings-page-behaviour").unwrap();
    let about_row = cx
        .debug_bounds("settings-page-about")
        .expect("About is a sidebar row");
    assert!(
        about_row.top() > behaviour.bottom() + px(ferrite_ui::theme::SETTINGS_NAV_ROW_H),
        "About is pinned to the sidebar's foot, apart from the settings"
    );
    assert!(about_row.bottom() <= card.bottom());
    assert_eq!(
        behaviour.size.height,
        px(ferrite_ui::theme::SETTINGS_NAV_ROW_H)
    );
    cx.simulate_click(about_row.center(), gpui::Modifiers::none());
    tick(cx);
    let about = cx
        .debug_bounds("settings-fact-Settings file")
        .expect("About navigation reveals stored paths");
    assert!(
        cx.debug_bounds("settings-fact-Version").is_some(),
        "About shows the Ferrite version"
    );
    assert!(
        cx.debug_bounds("settings-fact-Development build").is_none(),
        "a development build says so on its Version, not in a row of its own"
    );
    assert_eq!(version_label("0.3.0", true), "0.3.0 · dev");
    assert_eq!(
        version_label("0.3.0", false),
        "0.3.0",
        "About identifies development builds without labeling releases"
    );
    assert!(
        about.bottom() <= card.bottom(),
        "About must scroll into the panel"
    );
    let first = cx.debug_bounds("settings-page-new-threads").unwrap();
    cx.simulate_click(first.center(), gpui::Modifiers::none());
    tick(cx);
    let codex = cx.debug_bounds("settings-provider-1").unwrap().center();
    cx.simulate_click(codex, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.prefs.settings.default_provider, Provider::Codex);
        assert_eq!(
            ferrite_core::settings::Settings::load(&view.prefs.dir)
                .unwrap()
                .default_provider,
            Provider::Codex,
            "the restyled component must save through the real settings action"
        );
    });
    // Click the native search field in the Settings sidebar.
    let search = cx
        .debug_bounds("settings-search")
        .expect("the search field tops the sidebar");
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_input("Text size");
    tick(cx);
    // The stepper's `+` twice: 13 → 14 → 15.
    for _ in 0..2 {
        let larger = cx
            .debug_bounds("settings-text-size-1")
            .expect("the size stepper's +")
            .center();
        cx.simulate_click(larger, gpui::Modifiers::none());
        cx.run_until_parked();
    }
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.prefs.settings.reading_size,
            ferrite_core::settings::ReadingSize::nearest(15)
        );
        assert_eq!(
            ferrite_core::settings::Settings::load(&view.prefs.dir)
                .unwrap()
                .reading_size,
            ferrite_core::settings::ReadingSize::nearest(15)
        );
    });
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-a backspace"
    } else {
        "ctrl-a backspace"
    });
    cx.simulate_input("Confirm before deleting");
    tick(cx);
    let toggle = cx.debug_bounds("settings-confirm-delete").unwrap().center();
    cx.simulate_click(toggle, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.prefs.settings.confirm_delete);
        assert!(
            !ferrite_core::settings::Settings::load(&view.prefs.dir)
                .unwrap()
                .confirm_delete
        );
    });
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.settings_open, "escape from search closes Settings")
    });
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    let close = cx.debug_bounds("settings-close").unwrap().center();
    cx.simulate_click(close, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(!view.settings_open));
    // The gear left the bar (SPEC §5.7) for the nav's foot; ⌘, (and
    // macOS' app menu, and the palette's `settings`) still opens it,
    // folded or not.
    assert!(cx.debug_bounds("settings-gear").is_none());
    cx.simulate_keystrokes("cmd-b");
    tick(cx);
    assert!(cx.debug_bounds("settings-gear").is_none());
    cx.simulate_keystrokes("cmd-b");
    tick(cx);
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.settings_open));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(!view.settings_open, "escape closes it")
    });

    let dir = view.read_with(cx, |view, _| view.prefs.dir.clone());
    let _ = std::fs::remove_dir_all(&dir);
    view.update(cx, |view, cx| {
        view.change_settings(
            |settings| {
                settings.default_provider = Provider::Codex;
                settings.claude_permission_mode = Some("acceptEdits".into());
                settings.confirm_delete = false;
            },
            cx,
        );
    });
    let saved = ferrite_core::settings::Settings::load(&dir).unwrap();
    assert_eq!(saved.default_provider, Provider::Codex, "saved on change");
    assert_eq!(saved.claude_permission_mode.as_deref(), Some("acceptEdits"));
    view.read_with(cx, |view, _| {
        let defaults = view.prefs.defaults.lock().unwrap();
        assert_eq!(
            defaults.claude_permission_mode.as_deref(),
            Some("acceptEdits"),
            "the spawner's defaults follow"
        );
        assert_eq!(
            view.default_choice().provider,
            Provider::Codex,
            "new drafts follow"
        );
    });

    // With confirmation off, a destructive row runs on its first press.
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update_in(cx, |view, window, cx| {
        view.open_context_menu(
            MenuTarget::Thread(thread),
            gpui::point(px(600.), px(300.)),
            cx,
        );
        let delete = view
            .context_menu
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .position(|row| matches!(row, Some((_, MenuVerb::Delete))))
            .unwrap();
        view.press_menu_row(delete, window, cx);
        assert!(view.context_menu.is_none());
        assert!(view.cockpit.thread(thread).is_none(), "gone on one press");
    });
    let _ = std::fs::remove_dir_all(&dir);
}
