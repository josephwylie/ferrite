//! Picking a provider, a model and an effort.
#[allow(unused_imports)]
use super::*;
use crate::cockpit::model_picker::Tab;

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
            "Searching workspace skills..."
        );
    });
    tx.send(Ok(menu_commands())).unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.popover.as_ref().unwrap().rows[0].name.as_ref(),
            "code-review"
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
            "Searching workspace skills..."
        );
    });
    next_tx
        .send(Ok(vec![ferrite_core::SessionCommand {
            name: "code-codex".into(),
            description: "Codex skill".into(),
            path: Some("/global/SKILL.md".into()),
            source: None,
        }]))
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.popover.as_ref().unwrap().rows[0].name.as_ref(),
            "code-codex"
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
    view.update(cx, |view, cx| view.pick_model_row(index, cx));
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
        view.popover = None;
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

/// The open model picker, read.
fn picker(view: &CockpitView) -> (&Popover, &ModelPicker) {
    let open = view.popover.as_ref().expect("the model picker is open");
    let Kind::Provider(picker) = &open.kind else {
        panic!("the popover is not the model picker");
    };
    (open, picker)
}

/// The names the picker lists now, in order.
fn listed(view: &CockpitView) -> Vec<String> {
    let (open, picker) = picker(view);
    picker
        .shown
        .iter()
        .map(|at| open.rows[*at].name.to_string())
        .collect()
}

fn announce(fake: &Fake, stream: usize, models: Vec<ferrite_core::ModelInfo>) {
    fake.streams.borrow()[stream]
        .send(SessionEvent::Models { models })
        .unwrap();
}

fn model(value: &str, display: &str, efforts: &[&str]) -> ferrite_core::ModelInfo {
    ferrite_core::ModelInfo {
        value: value.into(),
        display: display.into(),
        detail: String::new(),
        resolved: None,
        efforts: efforts.iter().map(|effort| effort.to_string()).collect(),
        default_effort: None,
    }
}

/// #25 AC: the keyboard-only path. `/` lists the local provider row on
/// top; ↵ opens the picker on the Thread's Provider, the ✓ on its
/// default, every row a model under the name people say; Codex's tab
/// lists its catalogue and ↵ picks its first: Ferrite's own act, the
/// Session replaced on the spot, nothing landing as prompt text, the
/// picker kept open in its Reasoning column (T3's docked picker).
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
    tick(cx);

    assert_eq!(
        composer_text(&view, cx),
        "",
        "the pick never lands as slash text"
    );
    view.read_with(cx, |view, _| {
        let (open, picker) = picker(view);
        assert_eq!(picker.tab, Tab::Provider(Provider::Claude));
        let names = listed(view);
        assert!(
            names
                .iter()
                .all(|name| !name.contains("claude-") && name != "Claude" && name != "Codex"),
            "models only, never a wire id or a section title: {names:?}"
        );
        assert!(open.rows[open.selected].active, "the cursor starts on the ✓");
    });
    view.update(cx, |view, cx| {
        view.model_picker_tab(Tab::Provider(Provider::Codex), cx)
    });
    tick(cx);
    cx.simulate_keystrokes("enter");
    tick(cx);

    view.read_with(cx, |view, _| {
        let (_, picker) = picker(view);
        assert_eq!(picker.column, Column::Reasoning, "the keyboard moves to the column");
        assert_eq!(
            view.cockpit.thread(thread).map(|open| open.provider()),
            Some(Provider::Codex)
        );
        let transcript = view.cockpit.thread(thread).unwrap().transcript();
        assert!(transcript.blocks().is_empty());
        assert!(!view.cockpit.thread(thread).is_some_and(|open| open.busy()));
    });
    let spawned = fake.spawned.borrow().last().unwrap().clone();
    assert_eq!(spawned.provider, Provider::Codex, "the choice drives the spawn");
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

/// #25: announced models replace the catalogue for their Provider and a
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

    view.update(cx, |view, cx| view.open_provider_picker(thread, cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["Sonnet", "Opus"]);
        let (open, picker) = picker(view);
        assert_eq!(
            open.selected, picker.shown[0],
            "no choice yet: the cursor starts on the first model"
        );
    });

    cx.simulate_keystrokes("down enter");
    tick(cx);

    assert_eq!(
        fake.spawned.borrow().last().unwrap(),
        &ProviderChoice {
            provider: Provider::Claude,
            model: Some("opus".into()),
        },
        "the model rides the spawn; the provider stands"
    );
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.thread(thread).and_then(|open| open.model()), Some("opus"));
    });

    fake.streams
        .borrow()
        .last()
        .unwrap()
        .send(SessionEvent::Models {
            models: vec!["sonnet".into(), "opus".into()],
        })
        .unwrap();
    tick(cx);
    view.read_with(cx, |view, _| {
        let (open, picker) = picker(view);
        let opus = picker.shown[1];
        assert!(open.rows[opus].active, "✓ on the standing model choice");
        assert!(!open.rows[picker.shown[0]].active);
        assert_eq!(open.selected, opus, "and the cursor there");
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

    view.update(cx, |view, cx| {
        let draft = view.panes[view.focused()].draft().unwrap();
        assert_eq!(draft.binding.provider().model.as_deref(), Some("opus"));
        // Announced once (deduplicated), the standing choice appended
        // where the list did not name it.
        assert_eq!(listed(view), ["Sonnet", "Opus"]);
        let (open, picker) = picker(view);
        let opus = picker.shown[1];
        assert!(open.rows[opus].active, "✓ on the inherited choice");
        assert_eq!(open.selected, opus);
        view.pick_model_row(opus, cx);
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
/// still opens, the other Provider's models stay live and say a pick
/// there is a handover, and the Composer's control stays in every Pane.
#[gpui::test]
fn the_first_prompt_fixes_the_provider_but_the_model_picker_stays_open(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("provider-lock-ui", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        assert!(view.model_chip(0, cx).is_some(), "pre-lock the Composer offers the control");
    });

    cx.simulate_input("hello");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();

    view.update(cx, |view, cx| {
        view.open_provider_picker(thread, cx);
        view.model_picker_tab(Tab::Provider(Provider::Codex), cx);
    });
    tick(cx);
    assert!(
        cx.debug_bounds("model-picker-row-0").is_some(),
        "the other Provider's models are listed"
    );
    view.update(cx, |view, cx| {
        let (open, _) = picker(view);
        assert!(open.rows.iter().all(|row| !row.inert), "every model stays live");
        assert!(view.model_chip(0, cx).is_some(), "and the control itself stays");
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    // The `/model` row stays live and opens the same picker.
    cx.simulate_input("/");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let row = &view.popover.as_ref().expect("open").rows[0];
        assert_eq!(row.name.as_ref(), "model");
        assert!(!row.inert);
        assert_eq!(row.detail.as_ref(), "Switch model or hand over to the other provider");
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(matches!(
            view.popover.as_ref().expect("the picker opened").kind,
            Kind::Provider(_)
        ));
    });
    assert_eq!(composer_text(&view, cx), "", "the pick never lands as slash text");
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
    tick(cx);
    view.read_with(cx, |view, _| assert!(view.popover.is_some(), "the picker opened"));

    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 0, "Escape only closes the picker");
    view.read_with(cx, |view, _| assert!(view.popover.is_none()));
    cx.simulate_keystrokes("escape");
    tick(cx);
    assert_eq!(*fake.interrupts.borrow(), 1, "the Composer's Escape stops it");
}

/// #25 regression: reopening the picker with a standing model choice
/// and pressing bare ↵ changes nothing: the current provider's row
/// carries the choice, so the re-pick is a true no-op — no teardown, no
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

    view.update(cx, |view, cx| view.open_provider_picker(thread, cx));
    tick(cx);
    cx.simulate_keystrokes("down enter escape");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.thread(thread).and_then(|open| open.model()), Some("opus"));
    });
    let spawns = fake.streams.borrow().len();

    view.update(cx, |view, cx| view.open_provider_picker(thread, cx));
    tick(cx);
    view.read_with(cx, |view, _| {
        let (open, _) = picker(view);
        let standing = open.rows.iter().position(|row| row.active).expect("✓ on the choice");
        assert_eq!(
            open.rows[standing].name.as_ref(),
            "Opus",
            "the learned provider label survives the Session replacement"
        );
        assert_eq!(open.selected, standing);
    });
    cx.simulate_keystrokes("enter");
    tick(cx);

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
    });
}

/// The Reasoning column lists the ladder the model announces, the ✓ on
/// the level in force and `Default` on the one a Thread starts on; ↑/↓
/// move in it, ↵ picks — the Thread re-aimed, the picker still open — and
/// ← goes back to the list.
#[gpui::test]
fn the_reasoning_column_takes_the_arrows_and_left_returns_to_search(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("effort-column", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    announce(&fake, 0, vec![model("sonnet", "Sonnet 5", &["low", "medium", "high"])]);
    tick(cx);
    view.update(cx, |view, cx| {
        view.cockpit.set_model(thread, Some("sonnet".into())).unwrap();
        view.open_effort_picker(thread, cx);
    });
    tick(cx);
    let rungs = |view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            let (_, picker) = picker(view);
            (
                picker.column,
                picker.effort_cursor,
                picker
                    .efforts
                    .iter()
                    .map(|rung| rung.name.to_string())
                    .collect::<Vec<_>>(),
            )
        })
    };
    let (column, start, names) = rungs(&view, cx);
    assert_eq!(column, Column::Reasoning);
    assert_eq!(names, ["Low", "Medium", "High"]);
    assert!(cx.debug_bounds("model-picker-effort-2").is_some(), "every rung is drawn");
    cx.simulate_keystrokes("up up up");
    tick(cx);
    assert_eq!(rungs(&view, cx).1, 0, "↑ climbs to the top and holds");
    cx.simulate_keystrokes("down down down");
    tick(cx);
    assert_eq!(rungs(&view, cx).1, 2, "↓ walks down and holds at the foot");
    let _ = start;
    cx.simulate_keystrokes("enter");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.thread(thread).unwrap().effort(), Some("high"));
        let (_, picker) = picker(view);
        assert!(picker.efforts[2].active, "the ✓ follows the pick");
    });
    cx.simulate_keystrokes("left");
    tick(cx);
    assert_eq!(rungs(&view, cx).0, Column::List, "← goes back to the list");
}

/// ⌘1…⌘N pick the Nth listed model (T3's `modelPicker.jump.N`), each row
/// wearing its key; a search re-numbers what it lists.
#[gpui::test]
fn command_digits_jump_to_the_listed_models(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("picker-jumps", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    announce(
        &fake,
        0,
        vec![
            model("sonnet", "Sonnet 5", &[]),
            model("opus", "Opus 5.5", &[]),
            model("haiku", "Haiku 4.5", &[]),
        ],
    );
    tick(cx);
    view.update(cx, |view, cx| view.open_provider_picker(thread, cx));
    tick(cx);
    assert!(cx.debug_bounds("model-picker-row-2").is_some());
    cx.simulate_keystrokes("cmd-2");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.thread(thread).and_then(|open| open.model()), Some("opus"));
        assert!(view.popover.is_some(), "the docked picker stays open");
    });
    cx.simulate_input("hai");
    tick(cx);
    view.read_with(cx, |view, _| assert_eq!(listed(view)[0], "Haiku 4.5"));
    cx.simulate_keystrokes("cmd-1");
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.cockpit.thread(thread).and_then(|open| open.model()), Some("haiku"));
    });
}

/// One popover, `MODEL_PICKER_H` tall over its footer in every state: the
/// 44 rail, the list and the 192 Reasoning column; switching tabs or
/// searching never resizes it. A search hides the rail and ranks every
/// Provider's models together.
#[gpui::test]
fn the_model_picker_is_one_popover_of_one_height(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{MODEL_PICKER_H, MODEL_PICKER_RAIL_W, REASONING_W};
    let (core, _fake) = cockpit("picker-height", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.open_provider_picker(thread, cx));
    tick(cx);
    let body = cx.debug_bounds("model-picker-body").expect("the picker's columns");
    let whole = cx.debug_bounds("model-picker").expect("the picker");
    assert_eq!(body.size.height, px(MODEL_PICKER_H));
    let rail = cx.debug_bounds("model-picker-rail").expect("the rail");
    assert_eq!(rail.size.width, px(MODEL_PICKER_RAIL_W));
    let column = cx.debug_bounds("model-picker-reasoning").expect("the column");
    assert_eq!(column.size.width, px(REASONING_W));
    let composer = cx.debug_bounds("composer-block").expect("the box");
    assert!(whole.bottom() <= composer.top(), "over the Composer: {whole:?} / {composer:?}");
    let footer = cx.debug_bounds("model-picker-footer").expect("the footer");
    let permissions = cx.debug_bounds("model-picker-permissions");
    if let Some(permissions) = permissions {
        assert!(
            footer.right() - permissions.right() <= px(ferrite_ui::theme::SPACE_3 + 1.),
            "the permissions sit at the footer's far end"
        );
    }

    let pill = cx.debug_bounds("model-picker-pill").unwrap();
    let codex = cx.debug_bounds("model-picker-rail-codex").unwrap();
    cx.simulate_click(codex.center(), gpui::Modifiers::none());
    for _ in 0..20 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    view.read_with(cx, |view, _| {
        assert_eq!(picker(view).1.tab, Tab::Provider(Provider::Codex))
    });
    let slid = cx.debug_bounds("model-picker-pill").unwrap();
    assert!(
        (slid.center().y - codex.center().y).abs() <= px(1.),
        "the pill slides to its button: {pill:?} → {slid:?} / {codex:?}"
    );
    assert_eq!(cx.debug_bounds("model-picker").unwrap(), whole, "a tab never resizes it");

    cx.simulate_input("gpt");
    tick(cx);
    assert!(cx.debug_bounds("model-picker-rail").is_none(), "a search hides the rail");
    assert_eq!(
        cx.debug_bounds("model-picker").unwrap().size,
        whole.size,
        "nor does a search"
    );
    view.read_with(cx, |view, _| {
        let names = listed(view);
        assert!(!names.is_empty() && names.iter().all(|name| name.starts_with("GPT")), "{names:?}");
    });
    cx.simulate_input("zzzz");
    tick(cx);
    assert!(cx.debug_bounds("model-picker-row-0").is_none(), "nothing matches");
    assert_eq!(cx.debug_bounds("model-picker").unwrap().size, whole.size);
}

/// A star puts a model in Favourites, saved at once: the next picker,
/// in the next launch too, opens on Favourites listing it.
#[gpui::test]
fn favourites_persist_and_open_the_picker(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("picker-favourites", 1);
    let thread = core.threads()[0];
    let dir = scratch("picker-favourites-prefs");
    std::fs::create_dir_all(&dir).unwrap();
    let prefs = Preferences {
        dir: dir.clone(),
        ..Preferences::ephemeral()
    };
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| {
        CockpitView::new_with_settings(core, Provider::Claude, prefs, cx)
    });
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    view.update(cx, |view, cx| view.open_provider_picker(thread, cx));
    tick(cx);
    let (starred, name) = view.read_with(cx, |view, _| {
        let (open, picker) = picker(view);
        assert_eq!(picker.tab, Tab::Provider(Provider::Claude), "nothing starred yet");
        let at = picker.shown[0];
        (at, open.rows[at].name.to_string())
    });
    let star = cx.debug_bounds("model-picker-star-0").expect("every row has its star");
    cx.simulate_click(star.center(), gpui::Modifiers::none());
    tick(cx);
    let saved = ferrite_core::settings::Settings::load(&dir).unwrap();
    assert_eq!(saved.favourite_models.len(), 1, "saved at once");
    view.update(cx, |view, cx| {
        assert!(view.popover.is_some(), "the star is not a pick");
        view.popover = None;
        view.open_provider_picker(thread, cx);
        assert_eq!(picker(view).1.tab, Tab::Favourites);
        assert_eq!(listed(view), [name.clone()]);
        let _ = starred;
    });
}

/// A Provider whose CLI is missing stands dimmed on the rail, its tooltip
/// the reason, and its button does nothing.
#[gpui::test]
fn a_provider_that_cannot_serve_is_dimmed_on_the_rail(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("picker-unavailable", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| {
        view.cli_updates.checked(
            Provider::Codex,
            Ok(ferrite_core::providers::update::Status::Missing),
            true,
        );
        view.open_provider_picker(thread, cx);
    });
    tick(cx);
    let codex = cx.debug_bounds("model-picker-rail-codex").expect("still on the rail");
    view.read_with(cx, |view, _| {
        let (_, picker) = picker(view);
        assert_eq!(
            picker.unavailable,
            [(Provider::Codex, SharedString::from("Codex CLI is not installed"))],
            "its reason, its tooltip"
        );
    });
    cx.simulate_click(codex.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(picker(view).1.tab, Tab::Provider(Provider::Claude), "a dead button");
    });
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
                source: None,
            }],
        })
        .unwrap();
    tick(cx);
    cx.simulate_input("/eff");
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let menu = view.popover.as_ref().expect("the / menu");
        let names: Vec<&str> = menu.rows.iter().map(|row| row.name.as_ref()).collect();
        assert_eq!(names, ["effort"], "one row, Ferrite's");
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

/// A new thread's footer says its permission mode, as a live Thread's does:
/// the mode a new Thread starts in, beside the model, with no menu yet.
#[gpui::test]
fn a_draft_says_its_permission_mode(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("draft-mode", 0);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    tick(cx);
    let index = view.read_with(cx, |view, _| view.focused());
    let mode = cx
        .debug_bounds(Box::leak(format!("draft-mode-{index}").into_boxed_str()))
        .expect("the draft's permission word");
    let model = cx.debug_bounds("draft-model-picker").expect("the model chip");
    assert!(mode.left() > model.right(), "it follows the model: {mode:?} {model:?}");
}


/// A draft's model chip is the live Thread's: one chip, the effort in
/// force beside the model, opening the model picker on the draft's band.
/// A rung picked in its Reasoning column names the chip, and the first
/// send starts the Thread on that effort.
#[gpui::test]
fn the_draft_model_chip_sets_its_effort_in_the_reasoning_column(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("draft-effort", 0);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    tick(cx);
    assert!(
        cx.debug_bounds("draft-effort-picker").is_none(),
        "no effort chip of its own"
    );
    let steps = view.read_with(cx, |view, _| {
        let (steps, _, chosen) = view
            .draft_effort_ladder(view.focused())
            .expect("the premise: the default model takes an effort");
        assert!(chosen.is_some(), "the chip names the level in force");
        steps
    });
    let chip = cx.debug_bounds("draft-model-picker").expect("the model chip");
    cx.simulate_click(chip.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        let (open, _) = picker(view);
        assert!(matches!(open.pane, PaneIdentity::Draft(_)));
    });
    let high = steps
        .iter()
        .position(|step| step == "high")
        .expect("the ladder offers high");
    let pick = cx
        .debug_bounds(Box::leak(format!("model-picker-effort-{high}").into_boxed_str()))
        .expect("every level is a rung");
    cx.simulate_click(pick.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.popover.is_some(), "the docked picker stays open");
        let draft = view.panes[view.focused()].draft().expect("still a draft");
        assert_eq!(draft.binding.effort(), Some("high"));
    });
    cx.simulate_keystrokes("escape");
    tick(cx);
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
/// model chip rides, left of send) so the test does not
/// encode the chip's exact position.
#[gpui::test]
fn clicking_the_footer_chip_opens_the_provider_picker(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("provider-chip-click", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1000.), px(700.)));
    tick(cx);

    let toolbar = cx.debug_bounds("composer-footer").unwrap();
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
    view.read_with(cx, |view, _| {
        assert!(matches!(
            view.popover.as_ref().expect("open").kind,
            Kind::Provider(_)
        ));
    });
}
