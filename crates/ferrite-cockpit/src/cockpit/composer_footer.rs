//! The Solo Composer's controls (SPEC §5.5): the queue strip's acts, send
//! and stop, the `+` and its menu, and a Thread's permission mode chip or a
//! draft's mode word. They get no view (ADR 0041, What does not get a
//! view): what they keep is the Pane's own (its Composer) and the popup
//! slot's (the `+` menu's focus, `Popup::Attach`; the mode menu,
//! `Popup::Mode`), and what they show is the Thread's, which the root reads
//! (`composer_toolbar`). Here they are drawn from what was read; each
//! press acts through the root.

use super::pane_cell::{ControlsInputs, DraftInputs, ThreadInputs};
use super::*;
use ferrite_core::{ControlKind, McpStatus, SessionControl};

/// Whose box the controls are: a draft's, with its band, or a Thread's.
pub(super) enum Kind<'a> {
    Draft(&'a DraftInputs),
    Thread(ThreadKey, &'a ThreadInputs),
}

/// The Solo Composer's controls (SPEC §5.5, Claude's): the `+` that
/// attaches files and the send circle on the box's row; under the box,
/// where the work runs at the left (a draft's Project and Workspace
/// pickers, an empty Thread's Project and branch), and at the right the
/// model and effort and a Thread's permission mode as plain words, then
/// the plan-usage ring. A board Pane's compact Composer (SPEC §5.9) has
/// only the send circle, on the focused Pane; a Subagent's tab has none
/// (its footer stands in). Either carries the queue strip's acts.
#[allow(clippy::too_many_arguments)]
pub(super) fn toolbar(
    index: usize,
    identity: PaneIdentity,
    controls: &ControlsInputs,
    triggers: &popups::Triggers,
    kind: Kind<'_>,
    pane: &Entity<PaneView>,
    model: &Entity<Cockpit>,
    cx: &mut Context<CockpitView>,
) -> pane::ComposerControls {
    let (running, line_empty) = running_and_empty(identity.thread(), pane, model, cx);
    let queue = identity
        .thread()
        .map_or_else(Vec::new, |thread| queue_acts(index, thread, model, running, cx));
    let attach = {
        let open_thread = identity
            .thread()
            .and_then(|thread| Some((thread, model.read(cx).thread(thread)?)));
        AttachMenu::of(
            index,
            identity,
            controls.attach_focus.clone(),
            pane.read(cx).composer.focus_handle(cx),
            controls.attach_place,
            open_thread.as_ref().map(|(thread, open)| (*thread, open)),
        )
    };
    let attach = attach.render(cx);
    let (env, mut words, mode, ring, waits) = match kind {
        Kind::Draft(draft) => {
            // A draft has no Session to ask: its mode is the one a new
            // Thread starts in (Settings), and its ring the provider's
            // account limits, as the Thread it becomes will read them.
            let ring = pane.read(cx).draft().and_then(|binding| {
                pane::plan_usage_ring(
                    ("draft-usage-ring", index),
                    model.read(cx).account_limits(
                        binding.binding.project().machine(),
                        binding.binding.provider().provider,
                    ),
                    None,
                    controls.usage_popover_held,
                    cx,
                )
            });
            (
                Some(band_chips(draft, triggers, pane, cx)),
                draft_model_chip(identity, controls, triggers, pane, model, cx)
                    .into_iter()
                    .collect::<Vec<_>>(),
                Some(draft_mode_word(index, &draft.mode)),
                ring.map(IntoElement::into_any_element),
                draft.copy_live.is_some(),
            )
        }
        Kind::Thread(thread, inputs) => {
            let open = model.read(cx).thread(thread);
            let open = open.as_ref();
            // An empty Thread says where its work runs, as a draft does;
            // its Project and checkout are settled, so as words.
            let empty = open.is_some_and(|open| pane::offers_import(Some(open.transcript())));
            let env = inputs.facts.as_ref().filter(|_| empty).map(|(project, branch)| {
                [
                    (ferrite_ui::icons::FOLDER, project.clone()),
                    (ferrite_ui::icons::BRANCH, branch.clone()),
                ]
                .into_iter()
                .filter_map(|(mark, word)| {
                    word.map(|word| pane::env_word(mark, word).into_any_element())
                })
                .collect()
            });
            let ring = open.and_then(|open| {
                pane::plan_usage_ring(
                    ("plan-usage-ring", thread.thread().get() as usize),
                    model.read(cx).account_limits(thread.machine(), open.provider()),
                    open.transcript().plan().map(SharedString::from),
                    controls.usage_popover_held,
                    cx,
                )
            });
            let chip = open.and_then(|open| ModeChip::of(index, thread, open, inputs.mode_menu_up));
            (
                env,
                model_chip(identity, thread, controls, triggers, model, cx)
                    .into_iter()
                    .collect(),
                chip.map(|chip| chip.render(triggers.record(mode_menu::chip_trigger(thread)), cx)),
                ring.map(IntoElement::into_any_element),
                false,
            )
        }
    };
    // The model's word gives way first; the effort and the mode hold.
    let model_word = (!words.is_empty()).then(|| words.remove(0));
    words.extend(mode);
    pane::ComposerControls {
        attach: Some(attach),
        stop: stop_button(index, running, line_empty, cx),
        // A draft waiting on a copy is sent there once it is in.
        send: send_button(index, running, line_empty, cx).filter(|_| !waits),
        footer: Some(pane::composer_footer(model_word, words, ring).into_any_element()),
        env: env
            .filter(|chips: &Vec<AnyElement>| !chips.is_empty())
            .map(|chips| pane::env_strip(chips).into_any_element()),
        queue,
    }
}

/// Whether this Pane's turn runs, and whether its line is empty: what
/// decides its primary actions (`pane::primary_actions`).
fn running_and_empty(
    thread: Option<ThreadKey>,
    pane: &Entity<PaneView>,
    model: &Entity<Cockpit>,
    cx: &gpui::App,
) -> (bool, bool) {
    let running = thread
        .and_then(|thread| model.read(cx).thread(thread))
        .is_some_and(|open| turn_running(&open));
    (running, pane.read(cx).composer.read(cx).is_empty())
}

/// The draft's setup chips (#29) — Project and Workspace, and where it
/// runs — for the strip over the Composer (`env_strip`), wired to their
/// popovers, which hang above them (`band_menu_element`). A chip click
/// shares `open_band_popover` with ↵ on a tab-focused chip; the closure
/// re-finds the Pane by its Composer, a draft's one stable identity.
fn band_chips(
    inputs: &DraftInputs,
    triggers: &popups::Triggers,
    pane: &Entity<PaneView>,
    cx: &mut Context<CockpitView>,
) -> Vec<AnyElement> {
    let Some(draft) = pane.read(cx).draft() else {
        return Vec::new();
    };
    let composer = pane.read(cx).composer.clone();
    let (workspace_mark, workspace_label) = match draft.binding.target() {
        DraftTarget::Main => (ferrite_ui::icons::FOLDER, SharedString::from("Current checkout")),
        DraftTarget::Branch { name } => (ferrite_ui::icons::BRANCH, SharedString::from(name.clone())),
        DraftTarget::NewBranch => (ferrite_ui::icons::BRANCH, SharedString::from("New branch")),
        DraftTarget::Existing { branch, .. } => (
            ferrite_ui::icons::FOLDER_GIT_EXISTING,
            SharedString::from(format!("Previous worktree ({branch})")),
        ),
        DraftTarget::New => (ferrite_ui::icons::FOLDER_GIT, SharedString::from("New worktree")),
    };
    // Where the draft runs: a chip while there is more than one
    // environment to choose, or a copy to wait on, else a word for a
    // machine.
    let environment = inputs
        .environment
        .clone()
        .map(|(mark, label)| (pane::BandChip::Environment, 2, mark, label));
    let word = inputs
        .machine
        .clone()
        .map(|name| pane::env_machine(name.into()).into_any_element());
    let chips = environment.into_iter().chain([
        (
            pane::BandChip::Project,
            0,
            pane::ChipMark::Icon(ferrite_ui::icons::FOLDER),
            pane::band_chip_label(&inputs.project),
        ),
        (
            pane::BandChip::Workspace,
            1,
            pane::ChipMark::Icon(workspace_mark),
            pane::band_chip_label(&workspace_label),
        ),
    ]);
    let chips = chips.map(|(chip, slot, mark, label)| {
        let focused = draft.band_focus == Some(chip);
        let chip_composer = composer.clone();
        let chip_element = pane::band_chip(slot, mark, label, focused).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                // The chip is this Pane's: land on it first, then toggle —
                // and stop the press so the root's dismissal cannot close
                // what this just opened.
                cx.stop_propagation();
                if let Some(at) = view
                    .panes
                    .iter()
                    .position(|pane| pane.read(cx).composer == chip_composer)
                {
                    view.focus_pane(at, cx);
                }
                view.open_band_menu(chip, false, cx);
            }),
        );
        ferrite_ui::components::on_bounds(chip_element, triggers.record(band_trigger(chip)))
            .into_any_element()
    });
    let mut chips: Vec<AnyElement> = word.into_iter().chain(chips).collect();
    // Why Send is not there, beside the copy's chip, while it waits on a
    // copy.
    if let Some(live) = inputs.copy_live {
        chips.insert(1, draft_environment::copy_waits_note(live).into_any_element());
    }
    chips
}

/// The draft's model chip (#29), where a live Thread's stands and as it
/// reads (Claude's new-session `Opus 5.5 Medium`): the provider's mark,
/// the model, then the effort in force when the model takes one. It opens
/// the model picker on the draft's band choice.
fn draft_model_chip(
    identity: PaneIdentity,
    controls: &ControlsInputs,
    triggers: &popups::Triggers,
    pane: &Entity<PaneView>,
    model: &Entity<Cockpit>,
    cx: &mut Context<CockpitView>,
) -> Option<AnyElement> {
    let draft = pane.read(cx).draft()?;
    let provider = draft.binding.provider().provider;
    let chosen = draft.binding.provider().model.as_deref();
    let announced = model
        .read(cx)
        .announced_models(draft.binding.project().machine(), provider);
    // The model's name, the provider's default until one is chosen.
    let model_label = SharedString::from(ferrite_core::models::footer_name(
        provider, chosen, &announced,
    ));
    let effort = (!ferrite_core::models::efforts_for(provider, chosen, &announced).is_empty())
        .then(|| {
            effort_in_force(
                draft.binding.effort(),
                controls.effort_default.as_deref(),
                provider,
                chosen,
                &announced,
            )
        })
        .flatten();
    let group = model_picker::chip_trigger(identity);
    let picker_up = controls.picker_up;
    let face = pane::model_effort_chip(pane::ModelChip {
        mark: ferrite_ui::provider::mark(provider),
        model: model_label,
        effort: effort.map(|effort| pane::sentence_case(&effort).into()),
        loading: false,
        open: picker_up,
        group: group.clone(),
    });
    let control = pane::draft_picker(
        "draft-model-picker",
        draft.band_focus == Some(pane::BandChip::Provider),
        face,
        cx,
    )
    .tip_unless_open(picker_up, "Model and effort");
    Some(model_picker::chip_control(identity, control, triggers, cx))
}

/// The Thread's model chip: the model it runs and the effort in force,
/// when the model takes one — one control opening the model picker, its
/// effort a submenu there. It opens even while working: the current choice
/// stays visible and availability follows the Thread's busy state, said in
/// the tooltip. It reads the cached answers, so it works with no Session.
fn model_chip(
    identity: PaneIdentity,
    thread: ThreadKey,
    controls: &ControlsInputs,
    triggers: &popups::Triggers,
    model: &Entity<Cockpit>,
    cx: &mut Context<CockpitView>,
) -> Option<AnyElement> {
    let open = model.read(cx).thread(thread)?;
    let provider = open.provider();
    let hold = tuning_hold(&open);
    // The model it runs; until one is known, the provider's default model
    // — never the provider, and never the id on the wire.
    let label = SharedString::from(ferrite_core::models::footer_name(
        provider,
        open.running_model(),
        open.models(),
    ));
    let effort = thread_effort(&open, controls.effort_default.as_deref());
    let key = thread.thread().get() as usize;
    let picker_up = controls.picker_up;
    let face = pane::model_effort_chip(pane::ModelChip {
        mark: ferrite_ui::provider::mark(provider),
        model: label,
        effort: effort.map(|effort| pane::sentence_case(&effort).into()),
        loading: open.starting(),
        open: picker_up,
        group: model_picker::chip_trigger(identity),
    });
    let control = pane::toolbar_chip(("model-picker", key), cx)
        .debug_selector(move || format!("model-picker-{key}"))
        .tip_unless_open(picker_up, hold.unwrap_or("Model and effort"))
        .child(face);
    Some(model_picker::chip_control(identity, control, triggers, cx))
}

/// The queue strip's acts on Pane `index`, each row's own, aimed at its
/// message by its id: send now sends it next (`send_queued_now`), edit is
/// ↑'s act and remove ⌫'s — neither while Codex steers it.
pub(super) fn queue_acts(
    index: usize,
    thread: ThreadKey,
    model: &Entity<Cockpit>,
    running: bool,
    cx: &mut Context<CockpitView>,
) -> Vec<Vec<AnyElement>> {
    type Act = fn(&mut Cockpit, ThreadKey, &str);
    // What each queued item offers, read before the buttons are made.
    let queue: Vec<_> = model
        .read(cx)
        .thread(thread)
        .map(|open| {
            open.queue()
                .iter()
                .map(|item| {
                    (item.id.clone(), pane::queued_takes_back(item), pane::queued_sends_now(item, running))
                })
                .collect()
        })
        .unwrap_or_default();
    queue
        .into_iter()
        .enumerate()
        .map(|(at, (id, takes_back, sends_now))| {
            let acts: [(Option<gpui::component::button::Button>, Act); 3] = [
                (
                    sends_now.then(|| pane::queued_send_now(at, cx)),
                    |cockpit, thread, id| cockpit.send_queued_now(thread, id),
                ),
                (
                    takes_back.then(|| pane::queued_edit(at, cx)),
                    |cockpit, thread, id| cockpit.edit_queued_item(thread, id),
                ),
                (
                    takes_back.then(|| pane::queued_close(at, cx)),
                    |cockpit, thread, id| cockpit.remove_queued_item(thread, id, false),
                ),
            ];
            acts.into_iter()
                .filter_map(|(button, act)| {
                    let id = id.clone();
                    let button = button?.on_click(cx.listener(
                        move |view, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            view.focus_pane(index, cx);
                            let focus = view.panes[index].read(cx).composer.read(cx).focus_handle(cx);
                            window.focus(&focus, cx);
                            view.cockpit.update(cx, |cockpit, _| act(cockpit, thread, &id));
                            view.facts.acted(view.cockpit.read(cx), thread);
                            cx.notify();
                        },
                    ));
                    Some(button.into_any_element())
                })
                .collect()
        })
        .collect()
}

/// The send circle on Pane `index`: Enter's act, steering a running turn.
/// The box's stands only once there is something to send (an empty box
/// shows none).
pub(super) fn send_button(
    index: usize,
    running: bool,
    empty: bool,
    cx: &mut Context<CockpitView>,
) -> Option<AnyElement> {
    if !pane::primary_actions(running, empty).send {
        return None;
    }
    let button = pane::send_button(running, empty, cx)
        .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            view.focus_pane(index, cx);
            // A press on the circle sends, wherever tab left the ring.
            view.panes[index].update_draft(cx, |draft| draft.band_focus = None);
            let focus = view.panes[index].read(cx).composer.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
            view.submit(&Submit, window, cx);
        }))
        .into_any_element();
    Some(button)
}

/// Stop on Pane `index`, while the turn runs: Esc's act, which keeps what
/// is typed.
pub(super) fn stop_button(
    index: usize,
    running: bool,
    empty: bool,
    cx: &mut Context<CockpitView>,
) -> Option<AnyElement> {
    if !pane::primary_actions(running, empty).stop {
        return None;
    }
    let button = pane::stop_button(cx)
        .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            view.focus_pane(index, cx);
            let focus = view.panes[index].read(cx).composer.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
            if let Some(thread) = view.panes[index].thread() {
                view.cockpit.update(cx, |cockpit, _| cockpit.interrupt(thread));
                view.facts.acted(view.cockpit.read(cx), thread);
                cx.notify();
            }
        }))
        .into_any_element();
    Some(button)
}

/// The box's `+` and its menu (Claude's), as it reads for Pane `index`:
/// add files or photos, add a folder, or start a slash command — the `/`
/// menu on the line; then, for a Thread whose Session reports them,
/// `Connectors ›` and `Plugins ›`.
pub(super) struct AttachMenu {
    index: usize,
    pane: PaneIdentity,
    /// The menu's focus while it is up.
    open: Option<FocusHandle>,
    /// The Composer's, which the keyboard goes back to.
    return_focus: FocusHandle,
    place: ferrite_ui::components::FloatPlace,
    connectors: Option<Connectors>,
    plugins: Vec<Plugin>,
}

/// The Session's MCP servers, a troubled one's state as its tag, and what
/// a pick on each does: sign in to one that waits on it, reconnect a failed
/// one, else open the Panel, where Connectors live — as `Manage connectors`
/// does.
struct Connectors {
    thread: ThreadKey,
    generation: u64,
    choices: Vec<ferrite_ui::components::Choice>,
    acts: Vec<Option<SessionControl>>,
}

/// A plugin the Session's commands come from, as the provider says
/// (`CommandSource::Plugin`), and its commands, each with its description.
struct Plugin {
    name: String,
    commands: Vec<(String, String)>,
}

impl AttachMenu {
    /// The menu on Pane `index`, with `thread` the Pane's Thread as it
    /// stands, if it has one.
    pub(super) fn of(
        index: usize,
        pane: PaneIdentity,
        open: Option<FocusHandle>,
        return_focus: FocusHandle,
        place: ferrite_ui::components::FloatPlace,
        thread: Option<(ThreadKey, &ThreadView)>,
    ) -> Self {
        let connectors = thread.and_then(|(thread, open)| {
            let servers = open.transcript().mcp_servers();
            (!servers.is_empty()).then(|| Connectors {
                thread,
                generation: open.generation(),
                acts: servers
                    .iter()
                    .map(|server| match server.status {
                        McpStatus::NeedsAuth if open.supports_control(ControlKind::LoginMcp) => {
                            Some(SessionControl::LoginMcp {
                                server: server.name.clone(),
                            })
                        }
                        McpStatus::Failed if open.supports_control(ControlKind::ReconnectMcp) => {
                            Some(SessionControl::ReconnectMcp {
                                server: server.name.clone(),
                            })
                        }
                        _ => None,
                    })
                    .collect(),
                choices: servers
                    .iter()
                    .map(|server| ferrite_ui::components::Choice {
                        label: server.name.clone().into(),
                        badge: match server.status {
                            McpStatus::Connected => None,
                            McpStatus::Connecting => Some("connecting".into()),
                            McpStatus::NeedsAuth => Some("needs sign-in".into()),
                            McpStatus::Failed => Some("failed".into()),
                            McpStatus::Disabled => Some("disabled".into()),
                            McpStatus::Unknown => None,
                        },
                        why: server.error.clone().map(SharedString::from),
                        ..Default::default()
                    })
                    .chain(std::iter::once(ferrite_ui::components::Choice {
                        label: "Manage connectors".into(),
                        ..Default::default()
                    }))
                    .collect(),
            })
        });
        let plugins = thread
            .map(|(_, open)| plugins(open.commands()))
            .unwrap_or_default()
            .into_iter()
            .map(|(name, commands)| Plugin {
                name,
                commands: commands
                    .iter()
                    .map(|command| (command.name.clone(), command.description.clone()))
                    .collect(),
            })
            .collect();
        Self {
            index,
            pane,
            open,
            return_focus,
            place,
            connectors,
            plugins,
        }
    }

    pub(super) fn render(self, cx: &mut Context<CockpitView>) -> AnyElement {
        let Self {
            index,
            pane: identity,
            open,
            return_focus,
            place,
            connectors,
            plugins,
        } = self;
        let choices = [
            ("Add files or photos", ferrite_ui::icons::PAPERCLIP),
            ("Add folder", ferrite_ui::icons::FOLDER),
            ("Slash commands", ferrite_ui::icons::SQUARE_SLASH),
        ]
        .into_iter()
        .enumerate()
        .map(|(at, (label, mark))| ferrite_ui::components::Choice {
            label: label.into(),
            icon: Some((mark, ferrite_ui::theme::INK_STRONG)),
            // The key that does it from anywhere (the menu's digits are
            // the modes'; this one is a chord).
            shortcut: (at == 0)
                .then(|| ferrite_ui::components::bound_chord("cockpit::AttachFiles"))
                .flatten()
                .map(SharedString::from),
            ..Default::default()
        })
        .collect();
        let weak = cx.entity().downgrade();
        let picker = weak.clone();
        ferrite_ui::menu::ChoiceMenu {
            submenus: submenus(index, connectors, plugins, cx),
            width: Some(ferrite_ui::theme::ATTACH_MENU_W),
            hang_left: true,
            id: SharedString::from(format!("attach-menu-{index}")),
            trigger: pane::attach_button(open.is_some(), cx),
            choices,
            open,
            return_focus,
            on_open: std::rc::Rc::new(move |open, _, cx| {
                let _ = weak.update(cx, |view, cx| {
                    if open {
                        view.focus_pane(index, cx);
                        let focus = cx.focus_handle();
                        view.show_popup(Popup::Attach { pane: identity, focus }, cx);
                    } else if view.attach_menu_focus(identity).is_some() {
                        view.close_popup(cx);
                    }
                });
            }),
            // The kit closes the menu after every pick.
            on_pick: std::rc::Rc::new(move |at, _, cx| {
                let _ = picker.update(cx, |view, cx| {
                    match at {
                        0 => view.attach_files(index, false, cx),
                        1 => view.attach_files(index, true, cx),
                        _ => view.panes[index]
                            .read(cx).composer
                            .clone()
                            .update(cx, |composer, cx| composer.insert("/", cx)),
                    }
                    cx.notify();
                });
            }),
            // Over the box, as the `/` menu stands, hanging from the `+`'s
            // left edge (Claude's), never out over the nav.
            place: Some(place),
        }
        .into_any_element()
    }
}

/// The `+` menu's submenus, each only when the Pane's Thread has
/// something to put in it (Claude's rows, as far as Ferrite can back
/// them): `Connectors ›`, then `Plugins ›`, whose rows are each
/// plugin's own submenu; a pick there starts `/plugin:command ` on the
/// line as a chip.
///
/// Claude's `+` menu reaches nothing else Ferrite could back: a
/// plugin the CLI loads with no commands has nothing to pick.
fn submenus(
    index: usize,
    connectors: Option<Connectors>,
    plugins: Vec<Plugin>,
    cx: &mut Context<CockpitView>,
) -> Vec<ferrite_ui::menu::Submenu> {
    let mut submenus = Vec::new();
    if let Some(Connectors {
        thread,
        generation,
        choices,
        acts,
    }) = connectors
    {
        let view = cx.entity().downgrade();
        submenus.push(ferrite_ui::menu::Submenu {
            icon: Some((ferrite_ui::icons::BLOCKS, ferrite_ui::theme::INK_STRONG)),
            label: "Connectors".into(),
            value: SharedString::default(),
            choices,
            submenus: Vec::new(),
            width: ferrite_ui::theme::ATTACH_MENU_W,
            on_pick: std::rc::Rc::new(move |at, _, cx| {
                let act = acts.get(at).cloned().flatten();
                let _ = view.update(cx, |view, cx| {
                    match act {
                        Some(act) => view.run_session_control(thread, generation, act, cx),
                        None => view.open_panel(thread, cx),
                    }
                    cx.notify();
                });
            }),
        });
    }
    if !plugins.is_empty() {
        let view = cx.entity().downgrade();
        let submenus_per_plugin = plugins
            .into_iter()
            .map(|plugin| {
                let names: Vec<String> =
                    plugin.commands.iter().map(|(name, _)| name.clone()).collect();
                let choices = plugin
                    .commands
                    .iter()
                    .map(|(name, description)| ferrite_ui::components::Choice {
                        // The command as the plugin names it: the
                        // submenu already says whose it is.
                        label: name
                            .strip_prefix(&format!("{}:", plugin.name))
                            .unwrap_or(name)
                            .to_string()
                            .into(),
                        why: (!description.is_empty()).then(|| description.clone().into()),
                        ..Default::default()
                    })
                    .collect();
                let view = view.clone();
                ferrite_ui::menu::Submenu {
                    icon: None,
                    label: plugin.name.into(),
                    value: SharedString::default(),
                    choices,
                    submenus: Vec::new(),
                    width: ferrite_ui::theme::ATTACH_MENU_W,
                    // A pick starts the command on the line, a chip
                    // as the `/` menu's pick is.
                    on_pick: std::rc::Rc::new(move |at, _, cx| {
                        let Some(name) = names.get(at) else {
                            return;
                        };
                        let token = format!("/{name}");
                        let _ = view.update(cx, |view, cx| {
                            view.panes[index].read(cx).composer.clone().update(cx, |composer, cx| {
                                composer.insert(&format!("{token} "), cx);
                                composer.stage_chip(SharedString::from(token), cx);
                            });
                            cx.notify();
                        });
                    }),
                }
            })
            .collect();
        submenus.push(ferrite_ui::menu::Submenu {
            icon: Some((ferrite_ui::icons::PLUG, ferrite_ui::theme::INK_STRONG)),
            label: "Plugins".into(),
            value: SharedString::default(),
            choices: Vec::new(),
            submenus: submenus_per_plugin,
            width: ferrite_ui::theme::ATTACH_MENU_W,
            // Its rows are the plugins' submenus: nothing to pick here.
            on_pick: std::rc::Rc::new(|_: usize, _: &mut Window, _: &mut gpui::App| {}),
        });
    }
    submenus
}

/// A Thread's permission mode chip (SPEC §5.5, §D.7), as it reads: the
/// mode's label (`pane::permission_mode_label`), in `MODE_ATTENTION_INK`
/// for `Manual` (`theme::mode_is_manual`). Always shown once the Thread
/// has a list or a mode, so the mode can be changed, from the cached
/// answers when no Session runs (ADR 0032). It opens the permission menu
/// (`mode_menu.rs`); ⇧⇥ still cycles (`cycle_mode`). A Session that cannot
/// switch modes shows the chip without a menu.
pub(super) struct ModeChip {
    index: usize,
    thread: ThreadKey,
    generation: u64,
    mode: String,
    label: SharedString,
    /// Whether its menu is up.
    menu_up: bool,
    /// Whether the Session switches modes, so the chip opens the menu.
    switches: bool,
}

impl ModeChip {
    /// The chip on Pane `index` for `open`, the Pane's Thread.
    pub(super) fn of(
        index: usize,
        thread: ThreadKey,
        open: &ThreadView,
        menu_up: bool,
    ) -> Option<Self> {
        let modes = open.permission_modes();
        if modes.is_empty() && open.permission_mode().is_none() {
            return None;
        }
        let mode = open.permission_mode().unwrap_or("default").to_string();
        Some(Self {
            index,
            thread,
            generation: open.generation(),
            label: pane::permission_mode_label(&mode, &modes),
            mode,
            menu_up,
            switches: !modes.is_empty() && open.supports_control(ControlKind::SetPermissionMode),
        })
    }

    /// The chip, with `bounds` recording where it was laid out, which the
    /// menu hangs from.
    pub(super) fn render(
        self,
        bounds: impl FnOnce(gpui::Bounds<Pixels>, &mut Window, &mut gpui::App) + 'static,
        cx: &mut Context<CockpitView>,
    ) -> AnyElement {
        let Self {
            index,
            thread,
            generation,
            ..
        } = self;
        let key = thread.thread().get() as usize;
        let trigger = pane::toolbar_chip(("mode-picker", key), cx)
            .debug_selector(move || format!("mode-picker-{key}"))
            .tip_unless_open(self.menu_up, "Permission mode")
            .child(pane::mode_chip(self.label, &self.mode));
        let wrap = |chip: AnyElement| {
            div()
                .debug_selector(move || format!("composer-mode-{key}"))
                .flex_shrink_0()
                .child(chip)
                .into_any_element()
        };
        if !self.switches {
            return wrap(trigger.into_any_element());
        }
        let trigger = trigger.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |view, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                view.focus_pane(index, cx);
                view.toggle_mode_menu(thread, generation, cx);
            }),
        );
        wrap(ferrite_ui::components::on_bounds(div().flex().child(trigger), bounds).into_any_element())
    }
}

/// A draft's permission word on Pane `index`, where a Thread's mode chip
/// stands: `mode`, the mode a new Thread starts in (Settings,
/// `permission_mode_for`), read-only until the Session exists to change it.
pub(super) fn draft_mode_word(index: usize, mode: &str) -> AnyElement {
    let label = pane::permission_mode_label(mode, &[]);
    div()
        .id(("draft-mode", index))
        .debug_selector(move || format!("draft-mode-{index}"))
        .flex_shrink_0()
        .tooltip(ferrite_ui::menu::tooltip("Permission mode"))
        .child(pane::mode_chip(label, mode))
        .into_any_element()
}
