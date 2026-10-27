//! Picking a provider, a model and an effort.
#[allow(unused_imports)]
use super::*;

#[gpui::test]
fn draft_commands_arrive_before_first_send_and_follow_provider_changes(cx: &mut TestAppContext) {
    let fake = Fake::default();
    let (tx, rx) = mpsc::channel();
    *fake.command_discovery.borrow_mut() = Some(rx);
    let core = Cockpit::new(
        Store::open(scratch("draft-commands")).unwrap(),
        Box::new(fake.clone()),
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.simulate_input("Please use /code");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.popover.as_ref().unwrap().rows[0].name.as_ref(),
            "Loading commands…"
        );
    });
    tx.send(Ok(menu_commands())).unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.popover.as_ref().unwrap().rows[0].name.as_ref(),
            "/code-review"
        );
        assert!(view.panes[0].draft().is_some());
    });
    assert!(
        fake.spawned.borrow().is_empty(),
        "metadata creates no Session"
    );
    assert!(fake.sent.borrow().is_empty(), "metadata sends no prompt");
    assert_eq!(
        fake.command_requests.borrow().len(),
        1,
        "typing reuses the in-flight request"
    );

    let (next_tx, next_rx) = mpsc::channel();
    *fake.command_discovery.borrow_mut() = Some(next_rx);
    view.update(cx, |view, cx| {
        view.panes[0].draft_mut().unwrap().binding.choose_provider(
            ProviderChoice {
                provider: Provider::Codex,
                model: None,
            },
            &[],
        );
        view.sync_menu(cx);
        assert_eq!(
            view.popover.as_ref().unwrap().rows[0].name.as_ref(),
            "Loading commands…"
        );
    });
    next_tx
        .send(Ok(vec![ferrite_core::SessionCommand {
            name: "code-codex".into(),
            description: "Codex skill".into(),
            path: Some("/global/SKILL.md".into()),
        }]))
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.popover.as_ref().unwrap().rows[0].name.as_ref(),
            "/code-codex"
        );
    });
    assert_eq!(fake.command_requests.borrow()[1].0, Provider::Codex);
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.panes[0].composer.read(cx).text(),
            "Please use /code-codex "
        )
    });
    assert!(
        fake.spawned.borrow().is_empty(),
        "picking a skill still creates no Session"
    );
}

#[gpui::test]
fn draft_command_discovery_discards_old_workspace_replies_and_shows_failures(
    cx: &mut TestAppContext,
) {
    let fake = Fake::default();
    let (old_tx, old_rx) = mpsc::channel();
    *fake.command_discovery.borrow_mut() = Some(old_rx);
    let core = Cockpit::new(
        Store::open(scratch("draft-command-scopes")).unwrap(),
        Box::new(fake.clone()),
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.simulate_input("/code");
    cx.run_until_parked();

    let elsewhere = scratch("draft-command-new-project");
    std::fs::create_dir_all(&elsewhere).unwrap();
    let (tx, rx) = mpsc::channel();
    *fake.command_discovery.borrow_mut() = Some(rx);
    view.update(cx, |view, cx| {
        view.aim_launch(&elsewhere);
        view.sync_menu(cx);
    });
    assert!(
        old_tx.send(Ok(menu_commands())).is_err(),
        "the old workspace request was discarded"
    );
    assert_eq!(
        fake.command_requests.borrow()[1].1,
        std::fs::canonicalize(&elsewhere).unwrap()
    );
    tx.send(Err(std::io::Error::other("provider unavailable")))
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        let row = &view.popover.as_ref().unwrap().rows[0];
        assert_eq!(row.name.as_ref(), "Could not load commands");
        assert_eq!(row.detail.as_ref(), "provider unavailable");
        assert!(row.consequence_is_inert());
    });
    assert!(fake.spawned.borrow().is_empty());
}

#[gpui::test]
fn discovery_updates_an_open_draft_picker_without_starting_a_session(cx: &mut TestAppContext) {
    let fake = Fake::default();
    let (tx, rx) = mpsc::channel();
    *fake.model_discovery.borrow_mut() = Some(rx);
    let core = Cockpit::new(
        Store::open(scratch("discovered-models")).unwrap(),
        Box::new(fake.clone()),
    );
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    view.update(cx, |view, cx| {
        view.open_band_popover(pane::BandChip::Provider, cx)
    });
    let models = vec![ferrite_core::ModelInfo {
        value: "gpt-future-model".into(),
        display: "Future Model".into(),
        detail: "Discovered, never bundled".into(),
        resolved: None,
        efforts: vec!["medium".into(), "ultra".into()],
        default_effort: Some("medium".into()),
    }];
    tx.send((Provider::Codex, models.clone())).unwrap();
    tick(cx);
    let index = view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.model_catalog(Provider::Codex), models);
        view.popover
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .position(|row| row.name.as_ref() == "Future Model")
            .expect("the open picker updates when discovery arrives")
    });
    assert!(
        fake.spawned.borrow().is_empty(),
        "discovery creates no Session"
    );
    view.update_in(cx, |view, window, cx| view.pick(index, window, cx));
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.panes[0]
                .draft()
                .unwrap()
                .binding
                .provider()
                .model
                .as_deref(),
            Some("gpt-future-model")
        );
        assert!(view.cockpit.threads().is_empty());
    });
    assert!(
        fake.spawned.borrow().is_empty(),
        "choosing a model creates no Session"
    );
    tx.send((Provider::Codex, Vec::new())).unwrap();
    drop(tx);
    tick(cx);
    view.update(cx, |view, cx| {
        assert_eq!(
            view.cockpit.model_catalog(Provider::Codex),
            models,
            "an empty discovery keeps the last usable menu"
        );
        view.settings_open = true;
        cx.notify();
    });
    tick(cx);
    let search = cx
        .debug_bounds("settings-search")
        .expect("the search field tops the sidebar");
    cx.simulate_click(search.center(), gpui::Modifiers::none());
    cx.simulate_input("Future Model");
    tick(cx);
    let choice = cx
        .debug_bounds("settings-codex-model")
        .expect("Settings search indexes models inside the chooser")
        .center();
    cx.simulate_click(choice, gpui::Modifiers::none());
    cx.run_until_parked();
    cx.simulate_keystrokes("down down enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.prefs.settings.codex_model.as_deref(),
            Some("gpt-future-model")
        );
    });
    let choice = cx.debug_bounds("settings-codex-model").unwrap().center();
    cx.simulate_click(choice, gpui::Modifiers::none());
    cx.run_until_parked();
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.prefs.settings.codex_model, None,
            "CLI default is an explicit reversible choice"
        );
    });
}

/// #25 AC: the keyboard-only path. `/` lists the local provider row on
/// top; ↵ opens the picker — the two Providers with the ✓ on the
/// current one, and no invented model rows before an announcement —
/// and ↓↵ picks codex: Ferrite's own act, the Session replaced on the
/// spot, nothing landing as prompt text.
#[gpui::test]
fn the_slash_provider_row_opens_the_picker_and_picks_codex(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("provider-pick", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());

    cx.simulate_input("/");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(
        composer_text(&view, cx),
        "",
        "the pick never lands as slash text"
    );
    let first_codex = view.read_with(cx, |view, _| {
        let picker = view.popover.as_ref().expect("the provider picker is open");
        assert!(matches!(picker.kind, Kind::Provider));
        let names: Vec<&str> = picker
            .rows
            .iter()
            .map(|pick| pick.row.name.as_ref())
            .collect();
        // Two sections, each headed by its Provider, each listing the
        // catalog under the names people say — never a wire id.
        assert_eq!(names[0], "Claude");
        assert!(picker.rows[0].inert, "a section title is not a pick");
        let codex = names
            .iter()
            .position(|name| *name == "Codex")
            .expect("a Codex section");
        assert!(
            codex > 1,
            "Claude's models sit between the titles: {names:?}"
        );
        assert!(
            names
                .iter()
                .all(|name| !name.contains("claude-") && !name.contains("gpt-5")
                    || name.starts_with("GPT")),
            "{names:?}"
        );
        assert!(picker.rows[1].active, "✓ on the Provider's default");
        assert_eq!(picker.selected, 1, "the arrows start on it");
        codex + 1
    });

    view.update_in(cx, |view, window, cx| view.pick(first_codex, window, cx));
    cx.run_until_parked();

    view.read_with(cx, |view, _| {
        assert!(view.popover.is_none(), "the pick closed the picker");
        assert_eq!(
            view.cockpit.thread(thread).map(|open| open.provider()),
            Some(Provider::Codex)
        );
        // The switch was Ferrite's own act: no prompt, no running turn.
        let transcript = view.cockpit.thread(thread).unwrap().transcript();
        assert!(transcript.blocks().is_empty());
        assert!(!view.cockpit.thread(thread).is_some_and(|open| open.busy()));
    });
    let spawned = fake.spawned.borrow().last().unwrap().clone();
    assert_eq!(
        spawned.provider,
        Provider::Codex,
        "the choice drives the spawn"
    );
    assert_eq!(
        spawned.model.as_deref(),
        Some(
            ferrite_core::providers::models::fallback(Provider::Codex)[0]
                .value
                .as_str()
        ),
        "the first Codex row is its first catalog model"
    );
}

/// #25: announced models ride the picker below the providers and a
/// pick re-aims the model without touching the provider; the fresh
/// Session's own announcement then lists them again with the ✓ on the
/// standing choice.
#[gpui::test]
fn announced_models_ride_the_picker_and_a_pick_reaims_the_model(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("provider-models", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0]
        .send(SessionEvent::Models {
            models: vec!["sonnet".into(), "opus".into()],
        })
        .unwrap();
    tick(cx);

    cx.simulate_input("/");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let picker = view.popover.as_ref().expect("open");
        let names: Vec<&str> = picker
            .rows
            .iter()
            .map(|pick| pick.row.name.as_ref())
            .collect();
        // The announced list replaces the catalog for its Provider;
        // Codex, which announces none, keeps its catalog.
        assert_eq!(&names[..4], ["Claude", "Sonnet", "Opus", "Codex"]);
        assert_eq!(
            picker.selected, 1,
            "no choice yet: the arrows start on the first model"
        );
    });

    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();

    assert_eq!(
        fake.spawned.borrow().last().unwrap(),
        &ProviderChoice {
            provider: Provider::Claude,
            model: Some("opus".into()),
        },
        "the model rides the spawn; the provider stands"
    );
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.thread(thread).and_then(|open| open.model()),
            Some("opus")
        );
    });

    // The replacement Session announces its own list — reopening the
    // picker shows it with the ✓ on the standing choice.
    fake.streams
        .borrow()
        .last()
        .unwrap()
        .send(SessionEvent::Models {
            models: vec!["sonnet".into(), "opus".into()],
        })
        .unwrap();
    tick(cx);
    cx.simulate_input("/");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let picker = view.popover.as_ref().expect("reopened");
        assert!(picker.rows[2].active, "✓ on the standing model choice");
        assert!(!picker.rows[1].active);
        assert_eq!(picker.selected, 2, "and the arrows start there");
    });
}

#[gpui::test]
fn a_draft_inherits_the_focused_provider_model_and_only_announced_models(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("draft-provider-choice", 2);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[1]
        .send(SessionEvent::Models {
            models: vec!["sonnet".into(), "sonnet".into()],
        })
        .unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        view.cockpit
            .set_provider(
                thread,
                ProviderChoice {
                    provider: Provider::Claude,
                    model: Some("opus".into()),
                },
            )
            .unwrap();
        view.open_draft(DraftTarget::Main, cx);
        view.open_band_popover(pane::BandChip::Provider, cx);
    });

    view.read_with(cx, |view, _| {
        let draft = view.panes[view.focused()].draft().unwrap();
        assert_eq!(draft.binding.provider().model.as_deref(), Some("opus"));
        let labels: Vec<&str> = view
            .popover
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .map(|row| row.name.as_ref())
            .collect();
        // Announced once (deduplicated), the standing choice appended
        // where the list did not name it, then the Codex section.
        assert_eq!(&labels[..4], ["Claude", "Sonnet", "Opus", "Codex"]);
        let rows = &view.popover.as_ref().unwrap().rows;
        assert!(rows[2].active, "✓ on the inherited choice");
        assert_eq!(view.popover.as_ref().unwrap().selected, 2);
    });
    view.update_in(cx, |view, window, cx| {
        let selected = view.popover.as_ref().unwrap().selected;
        view.pick(selected, window, cx);
        assert_eq!(
            view.panes[view.focused()]
                .draft()
                .unwrap()
                .binding
                .provider()
                .model
                .as_deref(),
            Some("opus")
        );
    });
}

/// The first prompt fixes the Provider, never the model: the picker
/// still opens, the other Provider's section is drawn inert and says
/// why, and the Composer's control stays in every Pane.
#[gpui::test]
fn the_first_prompt_fixes_the_provider_but_the_model_picker_stays_open(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("provider-lock-ui", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        assert!(
            !view.model_chips(0, cx).is_empty(),
            "pre-lock the Composer offers the control"
        );
    });

    cx.simulate_input("hello");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    view.update(cx, |view, cx| {
        view.open_provider_picker(thread, cx);
        let picker = view
            .popover
            .as_ref()
            .expect("the picker opens after the first prompt");
        let codex = picker
            .rows
            .iter()
            .position(|row| row.name.as_ref() == "Codex")
            .unwrap();
        // The other Provider is a handover now, not a dead door: its
        // section says so and its rows stay live.
        assert_eq!(picker.rows[codex].detail.as_ref(), "handover");
        assert!(
            picker.rows[codex + 1..].iter().all(|row| !row.inert),
            "the other Provider's rows are live"
        );
        assert!(
            picker.rows[1..codex].iter().all(|row| !row.inert),
            "this Provider's models are live"
        );
        assert!(
            !view.model_chips(0, cx).is_empty(),
            "and the control itself stays — every Pane draws one"
        );
        view.popover = None;
    });

    // The `/model` row stays live and opens the same picker.
    cx.simulate_input("/");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let row = &view.popover.as_ref().expect("open").rows[0];
        assert_eq!(row.name.as_ref(), "/model");
        assert!(!row.inert);
        assert_eq!(
            row.detail.as_ref(),
            "Switch model or hand over to the other provider"
        );
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(matches!(
            view.popover.as_ref().expect("the picker opened").kind,
            Kind::Provider
        ));
    });
    assert_eq!(
        composer_text(&view, cx),
        "",
        "the pick never lands as slash text"
    );
}

/// Escape in an open picker closes the picker: the agent at work keeps
/// working, and the next Escape stops it.
#[gpui::test]
fn escape_in_an_open_picker_never_interrupts_the_agent(cx: &mut TestAppContext) {
    let (mut core, fake) = cockpit("picker-escape", 1);
    core.send(core.threads()[0], "keep working".into());
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    cx.simulate_input("/");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.popover.is_some(), "the picker opened")
    });

    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(
        *fake.interrupts.borrow(),
        0,
        "Escape only closes the picker"
    );
    view.read_with(cx, |view, _| assert!(view.popover.is_none()));
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(
        *fake.interrupts.borrow(),
        1,
        "the Composer's Escape stops it"
    );
}

/// #25 regression: reopening the picker with a standing model choice
/// and pressing bare ↵ changes nothing — the current provider's row
/// carries the choice, so the re-pick is a true no-op: no teardown, no
/// respawn, the model kept.
#[gpui::test]
fn reopening_the_picker_and_pressing_enter_keeps_the_standing_choice(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("provider-reopen-noop", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0]
        .send(SessionEvent::Models {
            models: vec!["sonnet".into(), "opus".into()],
        })
        .unwrap();
    tick(cx);

    // Pick opus: claude · opus is now the standing choice.
    cx.simulate_input("/");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.thread(thread).and_then(|open| open.model()),
            Some("opus")
        );
    });
    let spawns = fake.streams.borrow().len();

    // Reopen; the arrows start on the standing choice — bare ↵
    // re-picks it whole.
    cx.simulate_input("/");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let picker = view.popover.as_ref().expect("open");
        let standing = picker
            .rows
            .iter()
            .position(|row| row.active)
            .expect("✓ on the choice");
        assert_eq!(
            picker.rows[standing].name.as_ref(),
            "Opus",
            "the learned provider label survives the Session replacement"
        );
        assert_eq!(picker.selected, standing);
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    assert_eq!(
        fake.streams.borrow().len(),
        spawns,
        "a re-pick of the standing choice must not respawn"
    );
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.cockpit.thread(thread).and_then(|open| open.model()),
            Some("opus"),
            "the model stands"
        );
        assert_eq!(
            view.cockpit.thread(thread).map(|open| open.provider()),
            Some(Provider::Claude)
        );
    });
}

/// The effort chip opens a ladder from the provider's own announcement
/// with the operator's default on top; a pick re-aims the Thread and
/// the chip names the level. `/effort` opens the same picker.
#[gpui::test]
fn the_effort_picker_lists_the_models_ladder_and_a_pick_reaims_it(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("effort-picker", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0]
        .send(SessionEvent::Models {
            models: vec![ferrite_core::ModelInfo {
                value: "sonnet".into(),
                display: "Sonnet 5".into(),
                detail: String::new(),
                resolved: None,
                efforts: vec!["low".into(), "high".into()],
                default_effort: Some("high".into()),
            }],
        })
        .unwrap();
    tick(cx);

    view.update_in(cx, |view, window, cx| {
        view.open_effort_picker(thread, cx);
        let picker = view.popover.as_ref().expect("the effort picker opens");
        assert!(matches!(picker.kind, Kind::Effort));
        let names: Vec<&str> = picker.rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(names, ["Default", "Low", "High"]);
        assert!(
            picker.rows[0].active,
            "no choice yet: the default is in force"
        );
        view.pick(2, window, cx);
        assert!(view.popover.is_none(), "a pick closes the picker");
        assert_eq!(view.cockpit.thread(thread).unwrap().effort(), Some("high"));
    });

    cx.simulate_input("/eff");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("the / menu");
        assert_eq!(menu.rows[0].name.as_ref(), "/effort");
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let picker = view.popover.as_ref().expect("/effort opens the picker");
        // FL-4: `/effort` opens the model picker, whose effort row is
        // its ladder; the level in force stays High.
        assert!(matches!(picker.kind, Kind::Provider));
        assert_eq!(view.cockpit.thread(thread).unwrap().effort(), Some("high"));
    });
    assert_eq!(composer_text(&view, cx), "", "the /effort line is cleared");
}

/// Claude lists its own `effort` command; Ferrite's row of that name
/// replaces it, so the menu shows one `/effort` and it opens the
/// picker. Typed out with a level, `/effort high` is honoured by the
/// cockpit itself and never sent as text; an unknown level is.
#[gpui::test]
fn ferrites_effort_row_replaces_the_providers_and_a_typed_level_is_honoured(
    cx: &mut TestAppContext,
) {
    let (core, fake) = cockpit("effort-typed", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    fake.streams.borrow()[0]
        .send(SessionEvent::Commands {
            commands: vec![ferrite_core::SessionCommand {
                name: "effort".into(),
                description: "Set effort level for model usage".into(),
                path: None,
            }],
        })
        .unwrap();
    tick(cx);
    cx.simulate_input("/eff");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("the / menu");
        let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(names, ["/effort"], "one row, Ferrite's");
        assert!(matches!(
            menu.rows[0].consequence,
            Consequence::OpenEffortPicker
        ));
    });
    cx.simulate_input("ort high");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.thread(thread).unwrap().effort(), Some("high"));
    });
    assert!(
        fake.sent.borrow().is_empty(),
        "a known level is not a prompt"
    );

    cx.simulate_input("/effort turbo");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(
        fake.sent.borrow().last().map(String::as_str),
        Some("/effort turbo"),
        "an unknown level goes to the provider as text"
    );
}

/// The draft band's effort chip: its rows are the model's ladder under
/// the operator's default, a pick names the chip, and the first send
/// starts the Thread on that effort.
#[gpui::test]
fn the_effort_chip_starts_the_thread_on_its_pick(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("draft-effort", 0);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    tick(cx);
    view.update_in(cx, |view, window, cx| {
        view.open_band_popover(pane::BandChip::Effort, cx);
        let band = view.popover.as_ref().expect("the effort popover");
        assert!(matches!(band.kind, Kind::Band(pane::BandChip::Effort)));
        let names: Vec<&str> = band.rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(names[0], "Default");
        assert!(names.contains(&"High"), "{names:?}");
        let high = names.iter().position(|name| *name == "High").unwrap();
        view.pick(high, window, cx);
        let draft = view.panes[view.focused()].draft().expect("still a draft");
        assert_eq!(draft.binding.effort(), Some("high"));
    });
    cx.simulate_input("go");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let thread = view.panes[0].thread().expect("bootstrapped");
        assert_eq!(view.cockpit.thread(thread).unwrap().effort(), Some("high"));
    });
}

/// #25: the mouse door — a click on the model chip opens the picker.
/// The sweep covers the Composer's toolbar from its right (where the
/// model and effort chips ride, left of send) so the test does not
/// encode the chip's exact position.
#[gpui::test]
fn clicking_the_footer_chip_opens_the_provider_picker(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("provider-chip-click", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    let toolbar = cx.debug_bounds("composer-toolbar").unwrap();
    let row = toolbar.center().y;
    let right = f32::from(toolbar.right());
    let mut opened = false;
    'sweep: for y in [row - px(4.), row, row + px(4.)] {
        for x in (0..60).map(|step| right - 40. - step as f32 * 8.) {
            cx.simulate_mouse_down(
                gpui::point(px(x), y),
                gpui::MouseButton::Left,
                gpui::Modifiers::none(),
            );
            cx.run_until_parked();
            if view.read_with(cx, |view, _| view.popover.is_some()) {
                opened = true;
                break 'sweep;
            }
        }
    }
    assert!(opened, "the sweep never found the chip");
    // The sweep runs right to left and the effort chip sits right of
    // the model chip: either picker proves the mouse door.
    view.read_with(cx, |view, _| {
        let picker = view.popover.as_ref().expect("open");
        assert!(matches!(picker.kind, Kind::Provider | Kind::Effort));
    });
}
