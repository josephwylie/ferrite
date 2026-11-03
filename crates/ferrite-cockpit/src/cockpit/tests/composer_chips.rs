//! The chips under the Composer and the menus they hang.
#[allow(unused_imports)]
use super::*;

/// The panel's Context section reads the Session's reported context
/// window, following live readings and never inventing a window. Account
/// windows are not the panel's: the Composer's ring reads them, and a
/// click opens Claude's `Plan usage limits` popover over it — its foot
/// `USAGE_POP_OFFSET` above the ring's box, their right edges level.
#[gpui::test]
fn the_context_section_reads_the_window_and_the_ring_the_plan(cx: &mut TestAppContext) {
    use ferrite_ui::theme::USAGE_POP_OFFSET;
    let (core, fake) = cockpit("context-tab", 1);
    bind_production_keys(cx);
    let (_view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    let usage = |total_tokens: u64, context_window: Option<u64>| SessionEvent::TokenUsage {
        total_tokens,
        input_tokens: total_tokens,
        cached_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
        context_window,
    };
    fake.streams.borrow()[0]
        .send(usage(64_000, Some(200_000)))
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("context-usage-current-64000").is_some());
    assert!(cx.debug_bounds("context-usage-context-32").is_some());
    assert!(
        cx.debug_bounds("plan-usage-ring").is_none(),
        "no account window reported: no ring"
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    fake.streams.borrow()[0]
        .send(SessionEvent::RateLimits {
            five_hour: Some(ferrite_core::RateLimitWindow {
                used_fraction: 0.52,
                resets_at: Some(now + 3_600),
            }),
            weekly: Some(ferrite_core::RateLimitWindow {
                used_fraction: 0.08,
                resets_at: Some(now + 2 * 86_400),
            }),
        })
        .unwrap();
    fake.streams.borrow()[0]
        .send(usage(124_000, Some(200_000)))
        .unwrap();
    tick(cx);
    assert!(cx.debug_bounds("context-usage-current-124000").is_some());
    assert!(cx.debug_bounds("context-usage-maximum-200000").is_some());
    let ring = cx
        .debug_bounds("plan-usage-ring")
        .expect("the ring reads the plan");
    assert!(
        cx.debug_bounds("plan-usage-card").is_none(),
        "shut until pressed"
    );
    cx.simulate_click(ring.center(), gpui::Modifiers::none());
    cx.run_until_parked();
    let card = cx
        .debug_bounds("plan-usage-card")
        .expect("a press opens it");
    assert!(cx.debug_bounds("plan-usage-52").is_some());
    assert!(cx.debug_bounds("plan-usage-8").is_some());
    assert_eq!(card.bottom(), ring.top() - px(USAGE_POP_OFFSET));
    assert_eq!(card.right(), ring.right());
    fake.streams.borrow()[0].send(usage(31_000, None)).unwrap();
    tick(cx);
    assert!(
        cx.debug_bounds("context-usage-current-31000").is_some(),
        "the section follows live usage"
    );
    assert!(
        cx.debug_bounds("context-usage-maximum-unknown").is_some(),
        "unknown limit is not invented"
    );
}

/// The model picker rests above its chip in the Composer's toolbar
/// (SPEC §5.5): its foot `MODEL_PICKER_GAP` over the chip, its right
/// edge at the chip's (Claude's), inside the Composer, every row one menu
/// row high.
#[gpui::test]
fn the_model_picker_rests_above_the_bar_inside_the_window(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("model-picker-place", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(900.), px(700.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.open_provider_picker(thread, cx));
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    let menu = cx.debug_bounds("choice-menu").expect("the picker is open");
    let chip = debug_bounds(cx, format!("model-picker-{}", thread.get()))
        .expect("the model chip rides the toolbar");
    let composer = cx.debug_bounds("composer-block").unwrap();
    assert!(
        (menu.bottom() - (chip.top() - px(ferrite_ui::theme::MODEL_PICKER_GAP))).abs() <= px(1.),
        "FL-13: the picker's foot 7px over its chip: {menu:?} / {chip:?}"
    );
    assert!(
        (menu.size.width - px(ferrite_ui::theme::MODEL_PICKER_W)).abs() <= px(1.),
        "Claude's compact model menu: {menu:?}"
    );
    assert!(menu.right() <= composer.right() + px(1.5));
    assert!(
        (menu.right() - chip.right()).abs() <= px(1.5),
        "anchored on its chip, not the window's edge: {menu:?} / {chip:?}"
    );
    let row = cx.debug_bounds("choice-row-1").expect("a live model row");
    // Terminal-native (WP-E): a float row is one grid row.
    assert_eq!(row.size.height, px(ferrite_ui::theme::FLOAT_ROW_H));
}

/// Claude's model menu: the effort is a row under the models, `Effort
/// <level> ›`, its levels in a submenu that opens beside the menu (feet
/// level, `SUBMENU_GAP` apart) — by → from the keys or the pointer resting
/// on the row — and a pick there sets the Thread's effort and closes both.
#[gpui::test]
fn the_model_picker_opens_its_effort_as_a_submenu(cx: &mut TestAppContext) {
    use ferrite_ui::theme::{EFFORT_MENU_W, SUBMENU_GAP};
    let (core, _fake) = cockpit("model-picker-effort", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    let ladder = view.read_with(cx, |view, _| view.effort_ladder(thread));
    let Some((steps, _, _)) = ladder else {
        panic!("the premise: the default model takes an effort");
    };
    view.update(cx, |view, cx| view.open_provider_picker(thread, cx));
    tick(cx);
    let menu = cx.debug_bounds("choice-menu").expect("the picker is open");
    let row = cx
        .debug_bounds("choice-submenu-row-0")
        .expect("the effort row rides the model picker");
    assert!(
        cx.debug_bounds("choice-submenu").is_none(),
        "shut until asked"
    );
    cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
    tick(cx);
    let sub = cx
        .debug_bounds("choice-submenu")
        .expect("resting on the row opens the submenu");
    assert!(
        (sub.size.width - px(EFFORT_MENU_W)).abs() <= px(1.),
        "{sub:?}"
    );
    assert!(
        (sub.left() - menu.right() - px(SUBMENU_GAP)).abs() <= px(1.5),
        "beside the menu: {sub:?} / {menu:?}"
    );
    assert!((sub.bottom() - menu.bottom()).abs() <= px(1.), "feet level");
    let last = steps.len() - 1;
    let pick = cx
        .debug_bounds(Box::leak(format!("choice-sub-{last}").into_boxed_str()))
        .expect("every level is a row");
    cx.simulate_click(pick.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.popover.is_none(), "a pick closes the picker");
        assert_eq!(
            view.cockpit.thread(thread).unwrap().effort(),
            Some(steps[last].as_str())
        );
    });
}

/// A Group Pane's Composer carries no chips (SPEC §5.9): the mode,
/// model and effort are Solo's.
#[gpui::test]
fn a_group_panes_composer_carries_no_chips(cx: &mut TestAppContext) {
    let (mut core, _fake) = cockpit("group-no-chips", 2);
    let group = group_all(&mut core);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    view.update(cx, |view, cx| {
        view.enter_group(group, cx);
        view.focus_pane(1);
        cx.notify();
    });
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[1].thread().unwrap());
    assert!(cx.debug_bounds("composer-footer").is_none());
    assert!(debug_bounds(cx, format!("model-picker-{}", thread.get())).is_none());
}

/// A draft's Project menu hangs off its chip in the Composer's toolbar
/// (SPEC §5.5): its left edge at the chip's, resting above it.
#[gpui::test]
fn a_draft_band_menu_hangs_off_its_chip_in_the_bar(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("band-menu-place", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    view.update(cx, |view, cx| view.open_draft(DraftTarget::Main, cx));
    tick(cx);
    let chip = cx.debug_bounds("band-chip-0").expect("the Project chip");
    cx.simulate_mouse_down(chip.center(), MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(chip.center(), MouseButton::Left, gpui::Modifiers::none());
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    view.read_with(cx, |view, _| {
        assert!(view
            .popover
            .as_ref()
            .is_some_and(|open| matches!(open.kind, Kind::Band(pane::BandChip::Project))));
    });
    let rows = cx
        .debug_bounds("composer-menu-rows")
        .expect("the menu is drawn");
    assert!(
        (rows.left() - chip.left()).abs() <= px(1.5),
        "the menu stands on its chip: {rows:?} / {chip:?}"
    );
    assert!(
        rows.bottom() <= chip.top() - px(ferrite_ui::theme::FLOAT_OFFSET),
        "it rests above its chip: {rows:?} / {chip:?}"
    );
}

/// Claude's model menu leads with the model in force, its ✓ in the check
/// blue and no digit; the chip that opened it wears the pressed fill.
#[gpui::test]
fn the_model_menu_leads_with_the_model_in_force(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("model-menu-first", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    tick(cx);
    let thread = view.read_with(cx, |view, _| view.panes[0].thread().unwrap());
    view.update(cx, |view, cx| view.open_provider_picker(thread, cx));
    tick(cx);
    let first = cx.debug_bounds("choice-row-0").expect("a first row");
    let check = cx
        .debug_bounds("float-check")
        .expect("a ✓ on the model in force");
    assert!(
        first.contains(&check.center()),
        "the ✓ row leads: {check:?} / {first:?}"
    );
}

/// The `+` menu's `Plugins ›` lists the plugins a Session's commands come
/// from, as the provider says, each with its own commands.
#[test]
fn plugins_are_read_off_their_commands_sources() {
    let command = |name: &str, plugin: Option<&str>| ferrite_core::SessionCommand {
        name: name.into(),
        description: String::new(),
        path: None,
        source: Some(match plugin {
            Some(plugin) => ferrite_core::CommandSource::Plugin {
                plugin: plugin.into(),
            },
            None => ferrite_core::CommandSource::Skill,
        }),
    };
    let commands = [
        command("review", None),
        command("superpowers:brainstorm", Some("superpowers")),
        command("odd:name", None),
        command("superpowers:plan", Some("superpowers")),
        command("figma:inspect", Some("figma")),
    ];
    let plugins = plugins(&commands);
    let listed: Vec<(&str, Vec<&str>)> = plugins
        .iter()
        .map(|(plugin, commands)| {
            (
                plugin.as_str(),
                commands
                    .iter()
                    .map(|command| command.name.as_str())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            (
                "superpowers",
                vec!["superpowers:brainstorm", "superpowers:plan"]
            ),
            ("figma", vec!["figma:inspect"]),
        ]
    );
}

/// The `+` menu hangs from the `+`'s left edge, and its `Connectors ›`
/// lists the Session's MCP servers: a pick signs in to one waiting on it;
/// `Manage connectors` opens the panel.
#[gpui::test]
fn the_plus_menu_reaches_the_connectors(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("plus-connectors", 1);
    bind_production_keys(cx);
    *fake.native_controls.borrow_mut() = true;
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    fake.streams.borrow()[0]
        .send(SessionEvent::McpServers {
            servers: vec![ferrite_core::McpServer {
                name: "search".into(),
                status: ferrite_core::McpStatus::NeedsAuth,
                error: None,
            }],
        })
        .unwrap();
    tick(cx);
    let open = |view: &gpui::Entity<CockpitView>, cx: &mut gpui::VisualTestContext| {
        view.update(cx, |view, cx| {
            view.attach_menu = Some(view.panes[0].identity);
            cx.notify();
        });
        tick(cx);
        let row = cx
            .debug_bounds("choice-submenu-row-0")
            .expect("Connectors rides the + menu");
        cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
        tick(cx);
    };
    open(&view, cx);
    let plus = cx.debug_bounds("composer-attach").expect("the +");
    let first = cx
        .debug_bounds("choice-row-0")
        .expect("Add files or photos");
    assert!(
        first.left() >= plus.left() && first.left() < plus.right(),
        "the menu hangs from the +, over the box, not the sidebar: {first:?} / {plus:?}"
    );
    let server = cx.debug_bounds("choice-sub-0").expect("the server's row");
    cx.simulate_click(server.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(fake
        .controls
        .borrow()
        .contains(&ferrite_core::SessionControl::LoginMcp {
            server: "search".into()
        }));
    view.update(cx, |view, cx| {
        view.change_settings(|settings| settings.panel_shown = false, cx)
    });
    open(&view, cx);
    let manage = cx.debug_bounds("choice-sub-1").expect("Manage connectors");
    cx.simulate_click(manage.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, _| {
        assert!(view.attach_menu.is_none(), "a pick closes the menu");
        assert!(view.prefs.settings.panel_shown, "the panel shows");
    });
}

/// `Plugins ›` opens a list of the Session's plugins, each its own
/// submenu of its commands (Claude's); a pick starts the command on the
/// line as a chip, sent as the plain `/plugin:command` it reads.
#[gpui::test]
fn the_plus_menu_opens_each_plugins_commands(cx: &mut TestAppContext) {
    let (core, fake) = cockpit("plus-plugins", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    let command = |name: &str| ferrite_core::SessionCommand {
        name: name.into(),
        description: "A plugin's command".into(),
        path: None,
        source: Some(ferrite_core::CommandSource::Plugin {
            plugin: "tdd".into(),
        }),
    };
    fake.streams.borrow()[0]
        .send(SessionEvent::Commands {
            commands: vec![command("tdd:red"), command("tdd:green")],
        })
        .unwrap();
    tick(cx);
    view.update(cx, |view, cx| {
        view.attach_menu = Some(view.panes[0].identity);
        cx.notify();
    });
    tick(cx);
    let plugins = cx
        .debug_bounds("choice-submenu-row-0")
        .expect("Plugins rides the + menu");
    cx.simulate_mouse_move(plugins.center(), None, gpui::Modifiers::none());
    tick(cx);
    let plugin = cx
        .debug_bounds("choice-sub-submenu-0")
        .expect("the plugin's own row, a submenu");
    cx.simulate_mouse_move(plugin.center(), None, gpui::Modifiers::none());
    tick(cx);
    assert!(cx.debug_bounds("choice-deep-1").is_some(), "both commands");
    let green = cx.debug_bounds("choice-deep-1").unwrap();
    cx.simulate_click(green.center(), gpui::Modifiers::none());
    tick(cx);
    assert_eq!(composer_text(&view, cx), "/tdd:green ");
    view.read_with(cx, |view, cx| {
        assert!(view.attach_menu.is_none(), "a pick closes the menu");
        assert_eq!(
            view.panes[0].composer.read(cx).chips(),
            [SharedString::from("/tdd:green")]
        );
    });
}

/// An image in the tray is its picture, a `COMPOSER_THUMB` card (Claude's),
/// not a chip: a click opens it in the lightbox among the tray's images,
/// and its corner `×` removes it alone. Any other file keeps its chip.
#[gpui::test]
fn a_tray_image_is_a_thumbnail_card(cx: &mut TestAppContext) {
    let (core, _fake) = cockpit("tray-thumbnail", 1);
    bind_production_keys(cx);
    let (view, cx) = add_cockpit_window(cx, |_, cx| CockpitView::new(core, cx));
    cx.simulate_resize(gpui::size(px(1400.), px(800.)));
    let files = [
        std::path::PathBuf::from("/tmp/tray-a.png"),
        std::path::PathBuf::from("/tmp/notes.md"),
        std::path::PathBuf::from("/tmp/tray-b.jpg"),
    ];
    view.update(cx, |view, cx| {
        view.panes[0]
            .composer
            .update(cx, |composer, cx| composer.add_files(&files, cx));
    });
    tick(cx);
    let card = cx
        .debug_bounds("tray-image-0")
        .expect("the first image's card");
    let thumb = px(ferrite_ui::theme::COMPOSER_THUMB);
    assert_eq!((card.size.width, card.size.height), (thumb, thumb));
    assert!(cx.debug_bounds("tray-image-2").is_some(), "the second's");
    assert!(
        cx.debug_bounds("tray-image-1").is_none(),
        "a file is no card"
    );
    let tray = cx
        .debug_bounds("pending-attachment-tray")
        .expect("the tray");
    assert!(
        tray.size.height > thumb,
        "the file's chip rides under the cards"
    );

    cx.simulate_click(card.center(), gpui::Modifiers::none());
    tick(cx);
    assert!(
        cx.debug_bounds("image-viewer").is_some(),
        "a click opens the lightbox"
    );
    cx.simulate_keystrokes("escape");
    tick(cx);

    cx.simulate_mouse_move(card.center(), None, gpui::Modifiers::none());
    tick(cx);
    let remove = cx
        .debug_bounds("tray-image-0-remove")
        .expect("the card's ×, under the pointer");
    assert!(
        remove.right() <= card.right() && remove.top() >= card.top(),
        "in the corner: {remove:?} / {card:?}"
    );
    cx.simulate_click(remove.center(), gpui::Modifiers::none());
    tick(cx);
    view.read_with(cx, |view, cx| {
        let prompt = view.panes[0].composer.read(cx).prompt();
        assert_eq!(
            ferrite_core::prompt_files::paths(&prompt, None),
            [files[1].clone(), files[2].clone()],
            "the × removes that image alone"
        );
    });
}
