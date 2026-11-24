//! Settings and the menus that reach it.
#[allow(unused_imports)]
use super::*;

fn connection_fields(
    view: &Entity<CockpitView>,
    cx: &mut gpui::VisualTestContext,
) -> settings::ConnectionFields {
    connection_groups(view, cx)
        .into_iter()
        .flat_map(|group| group.rows)
        .find_map(|row| match row.control {
            settings::Control::ConnectionForm(settings::ConnectionForm::Editing {
                fields, ..
            }) => Some(fields),
            _ => None,
        })
        .expect("the connection editor is open")
}

#[gpui::test]
fn connections_keep_failed_input_save_only_ready_and_cancel_with_escape(cx: &mut TestAppContext) {
    use ferrite_core::machine::{setup::Event, Machine};
    let (core, _fake) = cockpit("connection-editor", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(800.), px(600.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    let hit = cx
        .debug_bounds("settings-page-connections")
        .unwrap()
        .center();
    cx.simulate_click(hit, gpui::Modifiers::none());
    tick(cx);
    let hit = cx.debug_bounds("connection-add").unwrap().center();
    cx.simulate_click(hit, gpui::Modifiers::none());
    tick(cx);
    let hit = cx.debug_bounds("connection-mode-ssh").unwrap().center();
    cx.simulate_click(hit, gpui::Modifiers::none());
    tick(cx);
    let fields = connection_fields(&view, cx);
    cx.update(|window, cx| {
        fields
            .host
            .update(cx, |field, cx| field.set_value("-bad", window, cx));
        fields
            .username
            .update(cx, |field, cx| field.set_value("developer", window, cx));
        fields
            .port
            .update(cx, |field, cx| field.set_value("2222", window, cx));
    });
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(cx.debug_bounds("connection-error").is_some());
    let card = cx.debug_bounds("connection-dialog").unwrap();
    for selector in [
        "connection-host",
        "connection-username",
        "connection-port",
        "connection-submit",
        "connection-close",
    ] {
        let bounds = cx.debug_bounds(selector).expect(selector);
        assert!(
            bounds.left() >= card.left() && bounds.right() <= card.right(),
            "{selector} fits the small sheet"
        );
    }
    view.read_with(cx, |view, cx| {
        assert!(view.prefs.settings.machines.is_empty());
        assert_eq!(fields.host.read(cx).value().as_ref(), "-bad");
        assert_eq!(fields.username.read(cx).value().as_ref(), "developer");
        assert_eq!(fields.port.read(cx).value().as_ref(), "2222");
    });
    connections_view(&view, cx).update(cx, |connections, cx| {
        connections.setup_event(
            Event::Failed("Connection refused. Check the host and port.".into()),
            cx,
        )
    });
    tick(cx);
    assert_eq!(
        connection_fields(&view, cx).host,
        fields.host,
        "connection failure retains the same fields"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(
            view.settings_open(cx),
            "Escape cancels the editor before closing Settings"
        );
        assert!(!view.connections(cx).unwrap().read(cx).editing());
    });
    assert!(cx.debug_bounds("connection-add").is_some());

    let hit = cx.debug_bounds("connection-add").unwrap().center();
    cx.simulate_click(hit, gpui::Modifiers::none());
    tick(cx);
    let machine = Machine::new("ready.example");
    connections_view(&view, cx).update(cx, |connections, cx| {
        connections.setup_event(Event::Ready(machine.clone()), cx);
        connections.setup_event(Event::Ready(machine.clone()), cx);
    });
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(!view.connections(cx).unwrap().read(cx).editing());
        assert_eq!(view.prefs.settings.machines, vec![machine.clone()]);
        assert_eq!(
            ferrite_core::settings::Settings::load(&view.prefs.dir)
                .unwrap()
                .machines,
            vec![machine]
        );
    });
}

#[gpui::test]
fn a_connection_save_failure_keeps_the_form_and_runtime_unchanged(cx: &mut TestAppContext) {
    use ferrite_core::machine::{setup::Event, Machine};
    let (core, _fake) = cockpit("connection-save-failure", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let open = connection_groups(&view, cx)
        .into_iter()
        .flat_map(|group| group.rows)
        .find_map(|row| match row.control {
            settings::Control::ConnectionForm(settings::ConnectionForm::Closed { open, .. }) => {
                Some(open)
            }
            _ => None,
        })
        .unwrap();
    cx.update(|window, cx| open(window, cx));
    let blocked = scratch("connection-settings-file").join("not-a-directory");
    std::fs::create_dir_all(blocked.parent().unwrap()).unwrap();
    std::fs::write(&blocked, b"keep").unwrap();
    view.update(cx, |view, _| view.prefs.dir = blocked.clone());
    connections_view(&view, cx).update(cx, |connections, cx| {
        connections.setup_event(Event::Ready(Machine::new("ready.example")), cx)
    });
    view.read_with(cx, |view, _| assert!(view.prefs.settings.machines.is_empty()));
    let error = connection_groups(&view, cx)
        .into_iter()
        .flat_map(|group| group.rows)
        .find_map(|row| match row.control {
            settings::Control::ConnectionForm(settings::ConnectionForm::Editing {
                error, ..
            }) => error,
            _ => None,
        })
        .expect("saving failed inline");
    assert!(error.contains("not saved"));
    assert_eq!(std::fs::read(blocked).unwrap(), b"keep");
}

#[gpui::test]
fn leaving_the_connections_page_drops_the_connection_editor(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("connection-page-leave", 1);
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    let click = |selector: &'static str, cx: &mut gpui::VisualTestContext| {
        let hit = cx.debug_bounds(selector).expect(selector).center();
        cx.simulate_click(hit, gpui::Modifiers::none());
        tick(cx);
    };
    click("settings-page-connections", cx);
    click("connection-add", cx);
    assert!(cx.debug_bounds("connection-dialog").is_some());
    click("settings-page-about", cx);
    click("settings-page-connections", cx);
    assert!(cx.debug_bounds("connection-dialog").is_none(), "another page dropped the editor");
    assert!(cx.debug_bounds("connection-add").is_some());
}

/// Behaviour's sidebar hint names the key this platform binds to the
/// sidebar toggle, drawn as keys: `⌘B` on macOS, `⌃B` on Windows.
#[gpui::test]
fn the_sidebar_hint_names_the_key_this_platform_binds(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("settings-sidebar-key", 1);
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    let page = cx.debug_bounds("settings-page-behaviour").expect("Behaviour");
    cx.simulate_click(page.center(), gpui::Modifiers::none());
    tick(cx);
    let keys = ferrite_ui::keymap::PLATFORM.chord("B");
    let lines = cx
        .update(|window, _| window.debug_text("settings-label-Start with the sidebar collapsed"))
        .expect("the row");
    assert_eq!(
        lines,
        [
            "Start with the sidebar collapsed",
            ferrite_ui::components::chord_text(&keys).as_str(),
            "toggles it any time",
        ]
    );
}

/// Settings › Labs turns Linked machines on and off, and its file says so
/// for the next start (the Delegate tools are offered by it).
#[gpui::test]
fn labs_turns_linked_machines_on_and_it_stays_on(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("settings-labs", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    let click = |selector: &'static str, cx: &mut gpui::VisualTestContext| {
        let hit = cx.debug_bounds(selector).expect(selector).center();
        cx.simulate_click(hit, gpui::Modifiers::none());
        cx.run_until_parked();
    };
    click("settings-page-labs", cx);
    let saved = |cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let shown = view.shown_defaults(cx).unwrap().linked_machines;
            let file = ferrite_core::settings::Settings::load(&view.prefs.dir).unwrap();
            assert_eq!(shown, file.labs.linked_machines, "shown as saved");
            shown
        })
    };
    assert!(!saved(cx), "off until turned on");
    click("settings-linked-machines", cx);
    assert!(saved(cx));
    click("settings-linked-machines", cx);
    assert!(!saved(cx));
}

#[gpui::test]
fn a_ready_setup_replaces_the_environment_it_edits_and_keeps_its_warning(
    cx: &mut TestAppContext,
) {
    use ferrite_core::machine::{setup::Event, Machine};
    let (core, _fake) = cockpit("connection-ready-replaces", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let saved = Machine {
        name: "box".into(),
        enabled: false,
        ..Machine::new("me@box")
    };
    let other = Machine::new("other.example");
    view.update(cx, |view, _| {
        view.prefs.settings.machines = vec![saved.clone(), other.clone()];
    });
    connections_view(&view, cx).update_in(cx, |connections, window, cx| {
        connections.edit(Some(saved.clone()), window, cx)
    });
    tick(cx);
    let ready = Machine {
        enabled: true,
        ..saved.clone()
    };
    connections_view(&view, cx).update(cx, |connections, cx| {
        connections.setup_event(Event::Warning("Host key added.".into()), cx);
        connections.setup_event(Event::Ready(ready.clone()), cx);
    });
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.prefs.settings.machines, vec![ready.clone(), other.clone()]);
    });
    let note = connection_groups(&view, cx)
        .into_iter()
        .find(|group| group.title == Some("Environments"))
        .and_then(|group| group.note);
    assert_eq!(note.as_deref(), Some("Host key added."));
}

/// What Connections said goes with the Settings sheet it was said on.
#[gpui::test]
fn connections_say_nothing_of_a_closed_settings_sheet(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("connections-reopen", 1);
    let prefs = Preferences {
        unreadable: Some("expected value at line 1 column 1".into()),
        ..Preferences::ephemeral()
    };
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Codex, prefs, cx)
    });
    tick(cx);
    let error = |cx: &mut gpui::VisualTestContext| {
        connection_groups(&view, cx)
            .into_iter()
            .flat_map(|group| group.rows)
            .find_map(|row| match row.control {
                settings::Control::ConnectionForm(settings::ConnectionForm::Closed {
                    error, ..
                }) => error,
                _ => None,
            })
    };
    let toggle = connection_act(&view, "connection-toggle-local", cx);
    cx.update(|window, cx| toggle(window, cx));
    assert!(error(cx).is_some_and(|error| error.contains("cannot be saved")));
    view.update(cx, |view, cx| {
        view.hide_sheet(cx);
    });
    tick(cx);
    assert_eq!(error(cx), None, "Settings opens again with no error");
}

#[gpui::test]
fn reconnect_leaves_search_and_rejects_another_saved_target(cx: &mut TestAppContext) {
    use ferrite_core::machine::Machine;
    let (core, _fake) = cockpit("connection-collision", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    let saved = vec![
        Machine::new("first.example"),
        Machine::new("second.example"),
    ];
    view.update_in(cx, |view, window, cx| {
        view.prefs.settings.machines = saved.clone();
        view.settings_sheet(cx)
            .unwrap()
            .read(cx)
            .search
            .clone()
            .unwrap()
            .update(cx, |search, cx| {
                search.set_value("first.example", window, cx)
            });
    });
    connections_view(&view, cx).update_in(cx, |connections, window, cx| {
        connections.edit(Some(saved[0].clone()), window, cx)
    });
    view.read_with(cx, |view, cx| {
        assert_eq!(view.settings_sheet(cx).unwrap().read(cx).page, settings::PageKey::Connections);
        assert!(view
            .settings_sheet(cx)
            .unwrap()
            .read(cx)
            .search
            .as_ref()
            .unwrap()
            .read(cx)
            .value()
            .is_empty());
    });
    tick(cx);
    let fields = connection_fields(&view, cx);
    cx.update(|window, cx| {
        fields.host.update(cx, |field, cx| {
            field.set_value("second.example", window, cx)
        })
    });
    cx.simulate_keystrokes("enter");
    tick(cx);
    assert!(cx.debug_bounds("connection-error").is_some());
    view.read_with(cx, |view, _| {
        assert_eq!(view.prefs.settings.machines, saved)
    });
}

/// A menu row names the key that does the same thing: a chord the
/// keymap binds, never one it has moved (⌘F is find, not fullscreen). A
/// Thread row's menu keys are Claude's one-letter mnemonics instead, each
/// its own letter; a parent row (`Open in ›`) has its chevron instead.
#[gpui::test]
fn a_menu_shortcut_is_a_bound_chord(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("menu-shortcuts", 1);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    let bound: Vec<String> = ferrite_ui::keymap::bindings(ferrite_ui::keymap::PLATFORM)
        .into_iter()
        .map(|(keys, _, _)| ferrite_ui::components::spell_chord(&keys))
        .collect();
    view.read_with(cx, |view, cx| {
        let thread = view.panes[0].thread().unwrap();
        let letters: Vec<String> = view
            .context_rows(MenuTarget::Thread(thread), cx)
            .into_iter()
            .flatten()
            .filter(|(_, verb)| !matches!(verb, MenuVerb::Submenu(_)))
            .map(|(item, _)| item.shortcut.expect("every row has its letter").to_string())
            .collect();
        assert!(letters
            .iter()
            .all(|key| key.len() == 1 && key.chars().all(|c| c.is_ascii_uppercase())));
        let mut unique = letters.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), letters.len(), "{letters:?}");
        for target in [MenuTarget::Pane(thread)] {
            for (item, _) in view.context_rows(target, cx).into_iter().flatten() {
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
    let card = cx.update(|window, cx| view.read(cx).pane_rects(window, cx))[0].1;
    cx.simulate_mouse_down(
        gpui::point(px(card.x + card.w / 2.), px(300.)),
        gpui::MouseButton::Right,
        gpui::Modifiers::none(),
    );
    cx.run_until_parked();
    let (delete, count) = view.read_with(cx, |view, cx| {
        let menu = view
            .context_menu()
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
            .context_rows(MenuTarget::Thread(thread), cx)
            .iter()
            .position(|row| matches!(row, Some((_, MenuVerb::Delete))))
            .expect("the nav row's menu deletes");
        (
            delete,
            view.cockpit.read(cx).threads().len() + view.cockpit.read(cx).parked().unwrap().len(),
        )
    });

    // Escape closes it without running anything.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, _| assert!(view.context_menu().is_none()));

    view.update_in(cx, |view, window, cx| {
        view.open_context_menu(
            MenuTarget::Thread(thread),
            gpui::point(px(600.), px(300.)),
            cx,
        );
        view.press_menu_row(delete, window, cx);
        let menu = view.context_menu().expect("armed, still up");
        assert_eq!(menu.armed, Some(delete), "the first press only arms");
        view.press_menu_row(delete, window, cx);
        assert!(
            view.context_menu().is_none(),
            "the second press runs and closes"
        );
        assert!(
            view.cockpit.read(cx).thread(thread).is_none(),
            "the Thread is gone from the grid"
        );
        assert_eq!(
            view.cockpit.read(cx).threads().len() + view.cockpit.read(cx).parked().unwrap().len(),
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
    let page = |cx: &mut gpui::VisualTestContext| view.read_with(cx, |view, cx| view.settings_sheet(cx).unwrap().read(cx).page);
    assert_eq!(page(cx), settings::PageKey::NewThreads);
    cx.simulate_keystrokes("down");
    tick(cx);
    assert_eq!(page(cx), settings::PageKey::Permissions);
    assert!(cx.debug_bounds("settings-codex-sandbox").is_some());
    assert!(cx.debug_bounds("settings-provider-0").is_none());
    cx.simulate_keystrokes("down down down down down down");
    tick(cx);
    assert_eq!(page(cx), settings::PageKey::About, "held at the foot");
    cx.simulate_keystrokes("up");
    tick(cx);
    assert_eq!(page(cx), settings::PageKey::Labs);
    assert!(cx.debug_bounds("settings-linked-machines").is_some());
    cx.simulate_keystrokes("up up up up");
    tick(cx);
    assert_eq!(page(cx), settings::PageKey::Behaviour);
    cx.simulate_keystrokes("down down down down down");
    tick(cx);

    let threads = cx
        .debug_bounds("settings-fact-Threads")
        .expect("About lists the threads directory")
        .center();
    cx.simulate_click(threads, gpui::Modifiers::none());
    tick(cx);
    let dir = view.read_with(cx, |view, cx| {
        assert_eq!(view.settings_sheet(cx).unwrap().read(cx).copied, Some("Threads"));
        view.prefs.dir.join("threads").display().to_string()
    });
    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some(dir),
        "the whole path, never the ~ form"
    );
    assert!(view.read_with(cx, |view, cx| view.settings_open(cx)));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    assert_eq!(page(cx), settings::PageKey::NewThreads);
    view.read_with(cx, |view, cx| assert_eq!(view.settings_sheet(cx).unwrap().read(cx).copied, None));
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
        ("cmd-=", 15),
        ("cmd-=", 16),
        ("cmd-=", 18),
        ("cmd-0", 14),
        ("cmd--", 13),
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
    view.read_with(cx, |view, cx| {
        assert!(view.settings_open(cx), "cmd-, opens the panel")
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
    // The stepper's `+` twice: 14 → 15 → 16.
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
            ferrite_core::settings::ReadingSize::nearest(16)
        );
        assert_eq!(
            ferrite_core::settings::Settings::load(&view.prefs.dir)
                .unwrap()
                .reading_size,
            ferrite_core::settings::ReadingSize::nearest(16)
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
    view.read_with(cx, |view, cx| {
        assert!(!view.settings_open(cx), "escape from search closes Settings")
    });
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    let close = cx.debug_bounds("settings-close").unwrap().center();
    cx.simulate_click(close, gpui::Modifiers::none());
    cx.run_until_parked();
    view.read_with(cx, |view, cx| assert!(!view.settings_open(cx)));
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
    view.read_with(cx, |view, cx| assert!(view.settings_open(cx)));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert!(!view.settings_open(cx), "escape closes it")
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
            .context_menu()
            .unwrap()
            .rows
            .iter()
            .position(|row| matches!(row, Some((_, MenuVerb::Delete))))
            .unwrap();
        view.press_menu_row(delete, window, cx);
        assert!(view.context_menu().is_none());
        assert!(view.cockpit.read(cx).thread(thread).is_none(), "gone on one press");
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui::test]
fn connection_modal_defaults_to_link_traps_tab_and_resets_on_reopen(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("connection-modal-focus", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    let hit = cx
        .debug_bounds("settings-page-connections")
        .unwrap()
        .center();
    cx.simulate_click(hit, gpui::Modifiers::none());
    tick(cx);
    let hit = cx.debug_bounds("connection-add").unwrap().center();
    cx.simulate_click(hit, gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("connection-remote-host").is_some());
    assert!(cx.debug_bounds("connection-pairing-code").is_some());
    assert!(cx.debug_bounds("connection-host").is_none());
    let fields = connection_fields(&view, cx);
    cx.update(|window, cx| {
        assert!(fields
            .remote_host
            .read(cx)
            .focus_handle(cx)
            .is_focused(window))
    });
    let mut visited = Vec::new();
    for _ in 0..12 {
        cx.simulate_keystrokes("tab");
        tick(cx);
        let focused = view.update_in(cx, |view, window, cx| {
            assert!(view.connections(cx).unwrap().read(cx).holds_focus(window, cx));
            window.focused(cx).unwrap()
        });
        if !visited.contains(&focused) {
            visited.push(focused);
        }
    }
    assert!(visited.len() >= 6, "Tab reaches the fields, modes, close, and submit");
    for _ in 0..12 {
        cx.simulate_keystrokes("shift-tab");
        tick(cx);
        view.update_in(cx, |view, window, cx| {
            assert!(view.connections(cx).unwrap().read(cx).holds_focus(window, cx))
        });
    }
    cx.update(|window, cx| {
        fields.remote_host.update(cx, |field, cx| {
            field.set_value("backend.example.com", window, cx)
        });
        fields
            .pairing_code
            .update(cx, |field, cx| field.set_value("", window, cx));
    });
    let hit = cx.debug_bounds("connection-submit").unwrap().center();
    cx.simulate_click(hit, gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("connection-error").is_some());
    let hit = cx.debug_bounds("connection-mode-ssh").unwrap().center();
    cx.simulate_click(hit, gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("connection-error").is_none());
    let hit = cx.debug_bounds("connection-close").unwrap().center();
    cx.simulate_click(hit, gpui::Modifiers::none());
    tick(cx);
    let hit = cx.debug_bounds("connection-add").unwrap().center();
    cx.simulate_click(hit, gpui::Modifiers::none());
    tick(cx);
    let reopened = connection_fields(&view, cx);
    assert_ne!(fields.remote_host, reopened.remote_host);
    assert!(cx.debug_bounds("connection-remote-host").is_some());
    assert!(cx.debug_bounds("connection-host").is_none());
    view.read_with(cx, |_, cx| {
        assert!(reopened.remote_host.read(cx).value().is_empty());
        assert!(reopened.pairing_code.read(cx).value().is_empty());
    });
    cx.simulate_keystrokes("escape");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.settings_open(cx) && !view.connections(cx).unwrap().read(cx).editing())
    });
    cx.simulate_keystrokes("escape");
    tick(cx);
    view.read_with(cx, |view, cx| assert!(!view.settings_open(cx)));
}

#[gpui::test]
fn connection_modal_does_not_submit_or_close_during_ime_composition(cx: &mut TestAppContext) {
    use gpui::EntityInputHandler;
    let (core, _fake) = cockpit("connection-modal-ime", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    connections_view(&view, cx)
        .update_in(cx, |connections, window, cx| connections.edit(None, window, cx));
    tick(cx);
    let fields = connection_fields(&view, cx);
    cx.update(|window, cx| {
        fields.remote_host.update(cx, |input, cx| {
            input.focus(window, cx);
            input.replace_and_mark_text_in_range(None, "仮", None, window, cx);
        });
    });
    for key in ["enter", "escape"] {
        cx.simulate_keystrokes(key);
        tick(cx);
        assert!(cx.debug_bounds("connection-dialog").is_some());
        assert!(cx.debug_bounds("connection-error").is_none());
    }
    cx.update(|window, cx| {
        fields
            .remote_host
            .update(cx, |input, cx| input.unmark_text(window, cx))
    });
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(cx.debug_bounds("connection-dialog").is_none());
}

#[gpui::test]
fn connection_pairing_url_can_be_typed_without_rewriting_its_prefix(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("connection-typed-pairing", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    connections_view(&view, cx)
        .update_in(cx, |connections, window, cx| connections.edit(None, window, cx));
    tick(cx);
    let fields = connection_fields(&view, cx);
    let url = "https://backend.example.com/pair#code=0123456789abcdef0123456789abcdef&server=fedcba9876543210fedcba9876543210";
    let mut typed = String::new();
    for character in url.chars() {
        typed.push(character);
        cx.simulate_input(&character.to_string());
        cx.run_until_parked();
        view.read_with(cx, |_, cx| {
            assert_eq!(fields.remote_host.read(cx).value().as_ref(), typed.as_str());
            assert!(fields.pairing_code.read(cx).value().is_empty());
        });
    }
    view.read_with(cx, |_, cx| {
        let parsed = ferrite_core::machine::remote::PairingFields::parse(
            fields.remote_host.read(cx).value().as_ref(),
            fields.pairing_code.read(cx).value().as_ref(),
        )
        .unwrap();
        assert_eq!(parsed.host, "https://backend.example.com");
        assert_eq!(parsed.pairing_code, "0123456789abcdef0123456789abcdef");
    });
}

#[gpui::test]
fn connection_pairing_url_paste_fills_host_and_code_after_replacing_text(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("connection-pasted-pairing", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    cx.simulate_keystrokes(&ferrite_ui::keymap::PLATFORM.chord(","));
    tick(cx);
    connections_view(&view, cx)
        .update_in(cx, |connections, window, cx| connections.edit(None, window, cx));
    tick(cx);
    let fields = connection_fields(&view, cx);
    cx.simulate_input("https://backend.example.com");
    cx.run_until_parked();
    cx.simulate_keystrokes(&ferrite_ui::keymap::PLATFORM.chord("a"));
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            "https://backend.example.com/pair#code=0123456789abcdef0123456789abcdef&server=fedcba9876543210fedcba9876543210".into(),
        ))
    });
    cx.simulate_keystrokes(&ferrite_ui::keymap::PLATFORM.chord("v"));
    cx.run_until_parked();
    view.read_with(cx, |_, cx| {
        assert_eq!(
            fields.remote_host.read(cx).value().as_ref(),
            "https://backend.example.com"
        );
        assert_eq!(
            fields.pairing_code.read(cx).value().as_ref(),
            "0123456789abcdef0123456789abcdef"
        );
    });
}

/// A toast stands over Settings: it draws over the sheet's veil, short of
/// the sheet's close button, which stays in reach, and a press on it is
/// the toast's, not the veil's, which would close Settings.
#[gpui::test]
fn a_toast_stands_over_the_settings_sheet(cx: &mut TestAppContext) {
    use gpui::component::notification::Notification;
    use gpui::component::WindowExt as _;
    let (core, _fake) = cockpit("toast-over-settings", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1440.), px(900.)));
    tick(cx);
    cx.simulate_keystrokes("cmd-,");
    tick(cx);
    // Connections is the widest page: its close button is nearest the
    // window's right edge, where toasts hang.
    view.update(cx, |view, cx| {
        let sheet = view.settings_sheet(cx).expect("Settings is up");
        sheet.update(cx, |sheet, _| sheet.page = settings::PageKey::Connections);
        cx.notify();
    });
    tick(cx);
    let pressed = std::rc::Rc::new(std::cell::Cell::new(0));
    cx.update(|window, cx| {
        let pressed = pressed.clone();
        let toast = Notification::new()
            .title("Over Settings")
            .autohide(false)
            .on_click(move |_, _, _| pressed.set(pressed.get() + 1));
        window.push_notification(toast, cx);
    });
    for _ in 0..40 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    let card = bounds(cx, "notification-card".into());
    let close = bounds(cx, "settings-close".into());
    // Where the toasts end, as the app's theme hangs them (the tests' kit
    // theme hangs them elsewhere).
    let end = cx.update(|window, cx| {
        let shifted = view.read(cx).toasts_shifted(window, cx);
        window.viewport_size().width - px(ferrite_ui::theme::TOAST_BOX_RIGHT) - shifted
    });
    assert!(end <= close.left(), "toasts end at {end:?}, over Settings' close button at {close:?}");
    cx.simulate_click(card.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(pressed.get(), 1, "the press reached the toast");
    view.read_with(cx, |view, cx| assert!(view.settings_sheet(cx).is_some(), "Settings stays up"));
}
