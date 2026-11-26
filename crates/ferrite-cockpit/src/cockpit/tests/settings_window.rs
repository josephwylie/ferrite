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

/// Connections in the window, with the key table launch binds
/// (`ferrite_settings::connections` has the page's own tests). It is the
/// sheet's Connections page. A pairing URL pasted over the host field
/// fills the host and the code: the paste is the field's, and the
/// Composer takes none of it (on Windows ctrl-v is also the Composer's
/// paste and the window's). Escape cancels the editor and then closes
/// Settings. A finished setup is asked of the window once: its answer
/// ends the editor, and it is saved in Settings' file.
#[gpui::test]
fn connections_take_a_paste_cancel_with_escape_and_save_a_ready_setup_once(cx: &mut TestAppContext) {
    use ferrite_core::machine::{setup::Event, Machine};
    use ferrite_ui::keymap::{bindings, PLATFORM};
    let (core, _fake) = cockpit("connection-editor", 1);
    cx.update(|cx| {
        // As launch binds them: this platform's table, Windows' on Windows.
        ferrite_pane::pane::init_components(cx);
        let table = ferrite_frame::keys::load(bindings(PLATFORM), cx);
        cx.bind_keys(table);
    });
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(800.), px(600.)));
    tick(cx);
    let settings_key = PLATFORM.chord(",");
    cx.simulate_keystrokes(&settings_key);
    tick(cx);
    let click = |selector: &'static str, cx: &mut gpui::VisualTestContext| {
        let hit = cx.debug_bounds(selector).expect(selector).center();
        cx.simulate_click(hit, gpui::Modifiers::none());
        tick(cx);
    };
    click("settings-page-connections", cx);
    click("connection-add", cx);
    assert!(cx.debug_bounds("connection-dialog").is_some());
    let fields = connection_fields(&view, cx);
    cx.simulate_input("https://backend.example.com");
    tick(cx);
    cx.simulate_keystrokes(&PLATFORM.chord("a"));
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
            "https://backend.example.com/pair#code=0123456789abcdef0123456789abcdef&server=fedcba9876543210fedcba9876543210".into(),
        ))
    });
    cx.simulate_keystrokes(&PLATFORM.chord("v"));
    tick(cx);
    view.read_with(cx, |_, cx| {
        assert_eq!(fields.remote_host.read(cx).value().as_ref(), "https://backend.example.com");
        assert_eq!(fields.pairing_code.read(cx).value().as_ref(), "0123456789abcdef0123456789abcdef");
    });
    assert_eq!(composer_text(&view, cx), "", "the Composer takes none of the paste");
    cx.simulate_keystrokes("escape");
    tick(cx);
    view.read_with(cx, |view, cx| {
        assert!(view.settings_open(cx), "Escape cancels the editor before closing Settings");
        assert!(!view.connections(cx).unwrap().read(cx).editing());
    });
    assert!(cx.debug_bounds("connection-add").is_some());
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert!(!view.read_with(cx, |view, cx| view.settings_open(cx)), "a second Escape closes Settings");

    cx.simulate_keystrokes(&settings_key);
    tick(cx);
    click("settings-page-connections", cx);
    click("connection-add", cx);
    let connections = connections_view(&view, cx);
    let heard = ferrite_ui::testing::record(cx, &connections, ferrite_settings::connections::ConnectionsEvent::clone);
    let machine = Machine::new("ready.example");
    // As the attempt's poll says it: once a frame, the window answering
    // between.
    for _ in 0..2 {
        connections.update(cx, |connections, cx| connections.setup_event(Event::Ready(machine.clone()), cx));
        tick(cx);
    }
    let ready = heard.take();
    assert!(
        matches!(&ready[..], [ferrite_settings::connections::ConnectionsEvent::Ready { machine: asked, replacing: None }] if *asked == machine),
        "asked once: {ready:?}"
    );
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
    let dir = view.read_with(cx, |view, _| view.prefs.dir.clone());
    connections_view(&view, cx).update_in(cx, |connections, window, cx| {
        connections.edit(Some(saved.clone()), &dir, window, cx)
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
            .update(cx, |sheet, cx| sheet.search("first.example", window, cx));
    });
    let dir = view.read_with(cx, |view, _| view.prefs.dir.clone());
    connections_view(&view, cx).update_in(cx, |connections, window, cx| {
        connections.edit(Some(saved[0].clone()), &dir, window, cx)
    });
    view.read_with(cx, |view, cx| {
        assert_eq!(view.settings_sheet(cx).unwrap().read(cx).page(), settings::PageKey::Connections);
    });
    tick(cx);
    let search = cx.update(|window, _| window.debug_text("settings-search")).expect("the search field");
    assert!(!search.iter().any(|said| said.contains("first.example")), "the search ended: {search:?}");
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

/// The window gives the sheet the keyboard as it opens, so ↓ steps its
/// pages at once; and each opening is a sheet of its own, on the first
/// page with nothing copied.
#[gpui::test]
fn settings_opens_with_the_keyboard_and_afresh_each_time(cx: &mut TestAppContext) {
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
    let sheet = |cx: &mut gpui::VisualTestContext| view.read_with(cx, |view, cx| view.settings_sheet(cx).unwrap());
    let page = |cx: &mut gpui::VisualTestContext| sheet(cx).read_with(cx, |sheet, _| sheet.page());
    assert_eq!(page(cx), settings::PageKey::NewThreads);
    cx.simulate_keystrokes("down");
    tick(cx);
    assert_eq!(page(cx), settings::PageKey::Permissions);
    cx.simulate_keystrokes("down down down down down down");
    tick(cx);
    let threads = cx
        .debug_bounds("settings-fact-Threads")
        .expect("About lists the threads directory")
        .center();
    cx.simulate_click(threads, gpui::Modifiers::none());
    tick(cx);
    let first = sheet(cx);
    assert_eq!(first.read_with(cx, |sheet, _| sheet.copied()), Some("Threads"));
    assert!(view.read_with(cx, |view, cx| view.settings_open(cx)));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    assert_ne!(sheet(cx), first, "a sheet of its own");
    assert_eq!(page(cx), settings::PageKey::NewThreads);
    assert_eq!(sheet(cx).read_with(cx, |sheet, _| sheet.copied()), None);
}

/// The sheet draws what the window has (`settings_inputs`), and the
/// window answers what it asks (`on_settings_event`): this computer's
/// text size, its defaults, its CLIs, its paths, its build and its
/// update; an edit saved, a default changed where it lives, a step
/// taken.
#[gpui::test]
fn the_settings_sheet_reads_and_answers_the_window(cx: &mut TestAppContext) {
    use ferrite_core::settings::{ReadingSize, Settings};
    let (core, _fake) = cockpit("settings-wiring", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    view.update(cx, |view, cx| {
        view.prefs.settings.reading_size = ReadingSize::nearest(18);
        view.prefs.settings.factory_tasks_at_once = 5;
        view.prefs.ferrite_updates = true;
        view.environment.cli_versions = Some(("2.3.1 (Claude Code)".into(), "codex-cli 0.61.0".into()));
        view.show_settings(cx);
    });
    tick(cx);
    let text = |selector: &str, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, _| window.debug_text(selector))
            .unwrap_or_else(|| panic!("nothing drawn as {selector}"))
            .join(" ")
    };
    let click = |selector: &'static str, cx: &mut gpui::VisualTestContext| {
        let hit = cx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector}")).center();
        cx.simulate_click(hit, gpui::Modifiers::none());
        cx.run_until_parked();
    };
    // The card is the sheet's height: a row low on a page is found.
    let seek = |query: &'static str, cx: &mut gpui::VisualTestContext| {
        let sheet = view.read_with(cx, |view, cx| view.settings_sheet(cx).expect("Settings is up"));
        sheet.update_in(cx, |sheet, window, cx| sheet.search(query, window, cx));
        cx.run_until_parked();
    };
    let place = view.read_with(cx, |view, cx| view.machine_place(cx));
    seek("Applies to", cx);
    assert!(text("settings-card", cx).contains(place.as_ref()), "Applies to {place}");
    seek("Text size", cx);
    assert_eq!(text("settings-text-size-value", cx), "18px");
    click("settings-text-size-1", cx);
    seek("Tasks working at once", cx);
    assert_eq!(text("settings-factory-limit-value", cx), "5", "the machine's defaults");
    click("settings-factory-limit-1", cx);
    seek("Name threads automatically", cx);
    click("settings-auto-title", cx);
    let saved = view.read_with(cx, |view, _| Settings::load(&view.prefs.dir).unwrap());
    assert_eq!(saved.reading_size, ReadingSize::nearest(18).step(1));
    assert_eq!(saved.factory_tasks_at_once, 6, "a step of the machine's own");
    assert!(!saved.auto_title, "a default, changed where it lives");
    seek("Tasks working at once", cx);
    assert_eq!(text("settings-factory-limit-value", cx), "6", "and drawn again");

    click("settings-page-server", cx);
    assert!(
        text("settings-card", cx).contains("This window runs without a separate server"),
        "{}",
        text("settings-card", cx)
    );
    assert!(cx.debug_bounds("settings-server-restart").is_none(), "no server of its own to restart");
    // Each CLI's row, found by the version the window gave it.
    for (version, row) in [("2.3.1 (Claude Code)", "Claude CLI"), ("codex-cli 0.61.0", "Codex CLI")] {
        seek(version, cx);
        assert!(text("settings-card", cx).contains(row), "{version}: {}", text("settings-card", cx));
    }
    click("settings-page-about", cx);
    let about = text("settings-card", cx);
    let version = format!("{} · dev", env!("CARGO_PKG_VERSION"));
    assert!(about.contains(&version), "{version}: {about}");
    let threads = view.read_with(cx, |view, _| view.prefs.dir.join("threads").display().to_string());
    click("settings-fact-Threads", cx);
    assert_eq!(cx.read_from_clipboard().and_then(|item| item.text()), Some(threads), "the window's own directory");
    click("settings-copy-diagnostics", cx);
    let report = cx.read_from_clipboard().and_then(|item| item.text()).unwrap_or_default();
    assert!(report.starts_with("Ferrite diagnostics\n"), "{report}");
    assert!(text("settings-copy-diagnostics", cx).contains("Copied"));
    assert!(cx.debug_bounds("settings-update-ferrite").is_some(), "a build that updates itself");
    view.update(cx, |view, cx| {
        view.prefs.ferrite_updates = false;
        cx.notify();
    });
    tick(cx);
    assert!(cx.debug_bounds("settings-update-ferrite").is_none(), "one that does not");
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
