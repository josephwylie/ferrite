//! The Settings sheet as a view of its own (ADR 0041), made when it opens
//! and dropped once it has gone (ADR 0042). It owns what only the sheet
//! reads: the page shown, the search field, the path last copied, its
//! focus, and the Connections page's own view. It draws its card for the
//! sheet host from what the Cockpit gives it (`SettingsInputs`), asked for
//! each frame, so the Cockpit's own notify still redraws it; and what the
//! operator asks of the window it says as a `SettingsEvent`, which the
//! Cockpit answers once the sheet's own update has ended.

use super::connections_view::{ConnectionsView, PageEvent};
use super::*;
use ferrite_core::settings::Settings;
use ferrite_core::ModelInfo;
use gpui::component::input::InputState;
use gpui::{App, WeakEntity};
use std::path::PathBuf;
use std::rc::Rc;

pub(crate) struct SettingsView {
    cockpit: WeakEntity<CockpitView>,
    /// The page the sidebar has selected; each opening starts on the first.
    pub(crate) page: settings::PageKey,
    /// The search field, made on the first frame (it needs a window).
    pub(crate) search: Option<Entity<InputState>>,
    /// The About path copied last, shown with a check until the sheet
    /// closes.
    pub(crate) copied: Option<&'static str>,
    pub(crate) focus: FocusHandle,
    pub(crate) connections: Entity<ConnectionsView>,
    /// What the Connections page asks of the sheet, heard from the first
    /// frame: answering needs the window.
    page_events: Option<gpui::Subscription>,
}

/// What the sheet draws from, as the Cockpit has it this frame.
pub(crate) struct SettingsInputs {
    pub settings: Settings,
    /// In `Provider::ALL` order.
    pub providers: Vec<ProviderInputs>,
    /// The CLIs' versions, once probed: (claude, codex).
    pub cli_versions: Option<(SharedString, SharedString)>,
    /// The machine Settings' defaults and CLIs are about, as it names it.
    pub place: SharedString,
    /// What the pages about that machine say atop them.
    pub scope: Option<SharedString>,
    /// Where the settings, the Threads and the logs live.
    pub dir: PathBuf,
    // Rows of features that have no view of their own yet, built by the
    // Cockpit with its own listeners.
    pub permissions: Vec<settings::Group>,
    pub machine: settings::Group,
    pub labs: Vec<settings::Group>,
    pub server: Vec<settings::Row>,
    pub ferrite_update: Vec<settings::Row>,
}

/// One provider's models and CLI, as the sheet's rows need them.
pub(crate) struct ProviderInputs {
    pub provider: Provider,
    pub catalog: Vec<ModelInfo>,
    /// What its CLI announced: the effort ladder reads it.
    pub announced: Vec<ModelInfo>,
    /// Its CLI's update line: what the check found, and the button.
    pub update: (SharedString, Option<SharedString>),
}

/// What the sheet asks of the window.
#[derive(Clone)]
pub(crate) enum SettingsEvent {
    /// A setting changed: saved and applied through `change_settings`.
    Change(Rc<dyn Fn(&mut Settings)>),
    StepTextSize(i32),
    PlaySound(Sound),
    ChooseSound(Sound),
    UpdateCli(Provider),
    CopyDiagnostics,
    /// The close button closed it.
    Closed,
}

impl EventEmitter<SettingsEvent> for SettingsView {}

impl SettingsView {
    pub(super) fn new(cockpit: WeakEntity<CockpitView>, cx: &mut Context<Self>) -> Self {
        let connections = cx.new(|_| ConnectionsView::new(cockpit.clone()));
        Self {
            cockpit,
            page: settings::PageKey::default(),
            search: None,
            copied: None,
            focus: cx.focus_handle(),
            connections,
            page_events: None,
        }
    }

    /// A listener that says `event` to the Cockpit.
    fn teller(cx: &Context<Self>) -> impl Fn(SettingsEvent, &mut App) + Clone + 'static {
        let view = cx.entity().downgrade();
        move |event, cx| {
            view.update(cx, |_, cx| cx.emit(event)).ok();
        }
    }

    /// A row's change, said to the Cockpit to save.
    fn change<T: Clone + 'static>(
        cx: &Context<Self>,
        write: impl Fn(&mut Settings, T) + 'static,
    ) -> impl Fn(T, &mut App) + 'static {
        let tell = Self::teller(cx);
        let write = Rc::new(write);
        move |value, cx| {
            let write = write.clone();
            tell(
                SettingsEvent::Change(Rc::new(move |settings| write(settings, value.clone()))),
                cx,
            );
        }
    }

    /// The searchable sheet's card, for the sheet host.
    pub(super) fn card(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ferrite_ui::sheet::Drawn> {
        if self.page_events.is_none() {
            self.page_events = Some(cx.subscribe_in(
                &self.connections,
                window,
                |view, _, event: &PageEvent, window, cx| match event {
                    PageEvent::Editing => {
                        view.reveal(settings::PageKey::Connections, window, cx)
                    }
                    PageEvent::Cancelled => view.refocus(window, cx),
                },
            ));
        }
        let search = self
            .search
            .get_or_insert_with(|| {
                cx.new(|cx| InputState::new(window, cx).placeholder("Search"))
            })
            .clone();
        let inputs = self
            .cockpit
            .update(cx, |cockpit, cx| cockpit.settings_inputs(cx))
            .ok()?;
        let connections = self
            .connections
            .update(cx, |connections, cx| connections.groups(window, cx));
        let pages = self.pages(inputs, connections, cx);
        let select = {
            let view = cx.entity().downgrade();
            move |page: settings::PageKey, window: &mut Window, cx: &mut App| {
                let _ = view.update(cx, |view, cx| view.reveal(page, window, cx));
            }
        };
        let view = cx.entity().downgrade();
        let step = {
            let view = view.clone();
            move |delta, _: &mut Window, cx: &mut App| {
                let _ = view.update(cx, |view, cx| view.show_page(view.page.step(delta), cx));
            }
        };
        let close = move |_: &mut Window, cx: &mut App| {
            let _ = view.update(cx, |_, cx| cx.emit(SettingsEvent::Closed));
        };
        Some(crate::frame().settings(
            ferrite_ui::frame::SettingsSheet {
                pages,
                selected: self.page,
                search,
                focus: self.focus.clone(),
                select: Rc::new(select),
                step: Box::new(step),
                close: Box::new(close),
            },
            window,
            cx,
        ))
    }

    fn show_page(&mut self, page: settings::PageKey, cx: &mut Context<Self>) {
        self.page = page;
        if page != settings::PageKey::Connections {
            self.connections
                .update(cx, |connections, _| connections.close_editor());
        }
        cx.notify();
    }

    /// Show `page` whole: choosing a page ends a search.
    fn reveal(
        &mut self,
        page: settings::PageKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = &self.search {
            search.update(cx, |search, cx| search.set_value("", window, cx));
        }
        self.show_page(page, cx);
    }

    /// Give the sheet back the keyboard.
    fn refocus(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
    }

    fn pages(
        &self,
        inputs: SettingsInputs,
        connections: Vec<settings::Group>,
        cx: &Context<Self>,
    ) -> Vec<settings::Page> {
        let SettingsInputs {
            settings,
            providers,
            cli_versions,
            place,
            scope,
            dir,
            permissions,
            machine,
            labs,
            server,
            ferrite_update,
        } = inputs;
        let tell = Self::teller(cx);
        let defaults = vec![settings::choices(
            "settings-provider",
            "Provider",
            "What a new thread starts on",
            Provider::ALL
                .into_iter()
                .map(|provider| {
                    (
                        ferrite_ui::provider::title(provider).into(),
                        settings.default_provider == provider,
                        provider,
                    )
                })
                .collect(),
            Self::change(cx, |settings, provider| settings.default_provider = provider),
        )];
        let mut new_thread_groups = vec![settings::Group::new(None).rows(defaults)];
        for ProviderInputs {
            provider,
            catalog,
            announced,
            ..
        } in &providers
        {
            let provider = *provider;
            let chosen = settings.model_for(provider).map(str::to_string);
            let model = settings::chooser(
                if provider == Provider::Claude {
                    "settings-claude-model"
                } else {
                    "settings-codex-model"
                },
                // The group names the provider: `Claude › Model`.
                "Model",
                "",
                settings::model_options(
                    ferrite_core::providers::models::menu(catalog.clone(), chosen.as_deref()),
                    chosen.as_deref(),
                ),
                Self::change(cx, move |settings, value| {
                    settings.set_model_for(provider, value)
                }),
            );
            let effort = settings.effort_for(provider).map(str::to_string);
            let mut ladder = ferrite_core::providers::models::efforts_for(
                provider,
                chosen.as_deref(),
                announced,
            );
            // A saved value remains visible even if the current model's
            // announcement no longer includes it; presentation never rewrites it.
            if let Some(effort) = &effort {
                if !ladder.contains(effort) {
                    ladder.push(effort.clone());
                }
            }
            let effort = settings::choices(
                if provider == Provider::Claude {
                    "settings-claude-effort"
                } else {
                    "settings-codex-effort"
                },
                "Effort",
                // Said once, in the first group: it holds for both.
                if provider == Provider::Claude {
                    "Each thread can change its own"
                } else {
                    ""
                },
                std::iter::once(None)
                    .chain(ladder.into_iter().map(Some))
                    .map(|value| {
                        let label = value
                            .as_deref()
                            .map(effort_title)
                            .unwrap_or_else(|| settings::CLI_DEFAULT.into());
                        (label.into(), effort == value, value)
                    })
                    .collect(),
                Self::change(cx, move |settings, value| {
                    settings.set_effort_for(provider, value)
                }),
            );
            new_thread_groups.push(provider_group(provider).rows([model, effort]));
        }
        let behaviour = vec![
            settings::toggle(
                "settings-auto-title",
                "Name threads automatically",
                "Titled from the first prompt until you rename it",
                settings.auto_title,
                Self::change(cx, |s, v| s.auto_title = v),
            ),
            settings::toggle(
                "settings-confirm-delete",
                "Confirm before deleting a thread",
                "Ask before removing a thread and its transcript",
                settings.confirm_delete,
                Self::change(cx, |s, v| s.confirm_delete = v),
            ),
            settings::toggle(
                "settings-nav-collapsed",
                "Start with the sidebar collapsed",
                "toggles it any time",
                settings.nav_collapsed,
                Self::change(cx, |s, v| s.nav_collapsed = v),
            )
            .bound_keys("cockpit::ToggleNav"),
        ];
        let size = settings.reading_size;
        let reading = vec![settings::stepper(
            "settings-text-size",
            "Text size",
            "Transcript text in every Pane, from 12 to 24px",
            format!("{}px", size.px()).into(),
            size > ferrite_core::settings::ReadingSize::SMALLEST,
            size < ferrite_core::settings::ReadingSize::LARGEST,
            {
                let tell = tell.clone();
                move |delta, cx| tell(SettingsEvent::StepTextSize(delta), cx)
            },
        )];
        let sounds = vec![
            settings::toggle(
                "settings-sounds",
                "Play sounds",
                "A chime when a thread finishes or needs you, unless you are looking at it",
                settings.sounds,
                Self::change(cx, |s, v| s.sounds = v),
            ),
            // Under `Play sounds`, which they depend on.
            Self::sound_file_row(&settings, "settings-done-sound", "Finished sound", Sound::Done, cx)
                .nested(),
            Self::sound_file_row(
                &settings,
                "settings-needs-you-sound",
                "Needs-you sound",
                Sound::NeedsYou,
                cx,
            )
            .nested(),
        ];
        let provider_sounds = Provider::ALL.map(|provider| {
            let (id, title) = match provider {
                Provider::Claude => ("settings-claude-sounds", "Sounds for Claude threads"),
                Provider::Codex => ("settings-codex-sounds", "Sounds for Codex threads"),
            };
            settings::toggle(
                id,
                title,
                "",
                settings.provider_sounds(provider),
                Self::change(cx, move |s, v| s.set_provider_sounds(provider, v)),
            )
            .nested()
        });
        let system = vec![settings::toggle(
            "settings-system-notifications",
            "System notifications",
            "When Ferrite is in the background · clicking one opens its thread",
            settings.system_notifications,
            Self::change(cx, |s, v| s.system_notifications = v),
        )];
        let (claude, codex) =
            cli_versions.unwrap_or_else(|| ("checking…".into(), "checking…".into()));
        let copied = self.copied;
        let copy = |key: &'static str, cx: &Context<Self>| {
            let view = cx.entity().downgrade();
            move |cx: &mut App| {
                let _ = view.update(cx, |view, cx| {
                    view.copied = Some(key);
                    cx.notify();
                });
            }
        };
        let mut clis: Vec<settings::Row> = providers
            .into_iter()
            .map(|ProviderInputs { provider, update, .. }| {
                let (id, version) = match provider {
                    Provider::Claude => ("settings-update-claude", claude.clone()),
                    Provider::Codex => ("settings-update-codex", codex.clone()),
                };
                let (detail, button) = update;
                let tell = tell.clone();
                settings::action(
                    id,
                    ferrite_ui::provider::cli(provider),
                    detail,
                    version,
                    button,
                    move |_, cx| tell(SettingsEvent::UpdateCli(provider), cx),
                )
            })
            .collect();
        clis.push(settings::toggle(
            "settings-auto-update-clis",
            "Update the CLIs automatically",
            // One line (`settings::hint`): the reason first.
            format!("New models need a newer CLI · installed on {place} when its Threads are idle"),
            settings.auto_update_clis,
            Self::change(cx, |s, v| s.auto_update_clis = v),
        ));
        let mut about = vec![
            settings::fact(
                "Version",
                version_label(env!("CARGO_PKG_VERSION"), crate::DEV).into(),
            ),
            settings::path_fact(
                "Threads",
                dir.join("threads").display().to_string(),
                copied == Some("Threads"),
                copy("Threads", cx),
            ),
            settings::path_fact(
                "Settings file",
                dir.join(ferrite_core::settings::Settings::FILE)
                    .display()
                    .to_string(),
                copied == Some("Settings file"),
                copy("Settings file", cx),
            ),
            settings::action(
                "settings-copy-diagnostics",
                "Diagnostics",
                "Version, CLIs, the last crash and the log's tail, for a bug report",
                ferrite_core::diagnostics::logs(&dir)
                    .display()
                    .to_string()
                    .into(),
                Some(
                    if copied == Some("Diagnostics") {
                        "Copied"
                    } else {
                        "Copy"
                    }
                    .into(),
                ),
                move |_, cx| tell(SettingsEvent::CopyDiagnostics, cx),
            ),
        ];
        about.splice(1..1, ferrite_update);
        let mut about = vec![settings::Group::new(None).rows(about)];
        about.push(settings::Group::new(Some("Provider CLIs")).rows(clis));
        vec![
            settings::page(settings::PageKey::NewThreads, new_thread_groups),
            settings::page(settings::PageKey::Permissions, permissions).scope(scope.clone()),
            settings::page(
                settings::PageKey::Behaviour,
                vec![
                    settings::Group::new(Some("Reading")).rows(reading),
                    settings::Group::new(Some("Threads and navigation")).rows(behaviour),
                    machine,
                ],
            ),
            settings::page(
                settings::PageKey::Notifications,
                vec![
                    settings::Group::new(Some("Sounds"))
                        .rows(sounds)
                        .rows(provider_sounds),
                    settings::Group::new(Some("System")).rows(system),
                ],
            ),
            settings::page(
                settings::PageKey::Server,
                vec![settings::Group::new(None).rows(server)],
            ),
            // Where Open picks the machine, so where it is named first.
            settings::page(settings::PageKey::Connections, connections).scope(scope.clone()),
            settings::page(settings::PageKey::Labs, labs).scope(scope),
            settings::page(settings::PageKey::About, about),
        ]
    }

    /// The line for one chime: the file it plays (or the built-in), `Play`
    /// to hear it, `Choose…` for a file of the operator's own, and `Use
    /// built-in` to go back.
    fn sound_file_row(
        settings: &Settings,
        id: &'static str,
        title: &'static str,
        sound: Sound,
        cx: &Context<Self>,
    ) -> settings::Row {
        let file = settings.sound_file(sound);
        let tell = Self::teller(cx);
        let play: settings::Act = Rc::new({
            let tell = tell.clone();
            move |cx| tell(SettingsEvent::PlaySound(sound), cx)
        });
        let choose: settings::Act = Rc::new(move |cx| tell(SettingsEvent::ChooseSound(sound), cx));
        let mut acts = vec![("Play", play), ("Choose\u{2026}", choose)];
        if file.is_some() {
            let reset = Self::change(cx, move |s, ()| s.set_sound_file(sound, None));
            acts.push(("Use built-in", Rc::new(move |cx: &mut App| reset((), cx))));
        }
        settings::file_choice(
            id,
            title,
            if cfg!(windows) { "A WAV file" } else { "" },
            file,
            "Built-in",
            acts,
        )
    }
}

impl CockpitView {
    /// Everything the sheet draws from. It never reads the sheet: the
    /// sheet calls it while its own update is running.
    pub(super) fn settings_inputs(&mut self, cx: &mut Context<Self>) -> SettingsInputs {
        let place = self.machine_place(cx);
        let machine = self.settings_machine(cx);
        let cockpit = self.cockpit.read(cx);
        let providers = Provider::ALL
            .into_iter()
            .map(|provider| ProviderInputs {
                provider,
                catalog: machine
                    .map(|machine| cockpit.model_catalog(machine, provider))
                    .unwrap_or_default(),
                announced: machine
                    .map(|machine| cockpit.announced_models(machine, provider))
                    .unwrap_or_default(),
                update: crate::cli_updates::describe(
                    provider,
                    self.cli_updates.state(provider),
                    self.prefs.settings.auto_update_clis,
                    machine.map_or(0, |machine| cockpit.live_sessions(machine, provider)),
                    &place,
                ),
            })
            .collect();
        SettingsInputs {
            settings: self.prefs.settings.clone(),
            providers,
            cli_versions: self.cli_versions.clone(),
            place,
            scope: self.settings_scope(cx),
            dir: self.prefs.dir.clone(),
            permissions: self.permission_groups(cx),
            machine: self.machine_group(cx),
            labs: self.labs_groups(cx),
            server: self.server_rows(cx),
            ferrite_update: self.ferrite_update_rows(cx),
        }
    }

    pub(super) fn on_settings_event(
        &mut self,
        _: Entity<SettingsView>,
        event: &SettingsEvent,
        cx: &mut Context<Self>,
    ) {
        match event.clone() {
            SettingsEvent::Change(change) => self.change_settings(|settings| change(settings), cx),
            SettingsEvent::StepTextSize(delta) => self.step_text_size(delta, cx),
            SettingsEvent::PlaySound(sound) => self.chime(sound),
            SettingsEvent::ChooseSound(sound) => self.choose_sound_file(sound, cx),
            SettingsEvent::UpdateCli(provider) => self.update_cli(provider, cx),
            SettingsEvent::CopyDiagnostics => self.copy_diagnostics(cx),
            SettingsEvent::Closed => {
                self.hide_sheet(cx);
            }
        }
    }

    pub(crate) fn settings_open(&self, cx: &App) -> bool {
        self.sheets.read(cx).settings().is_some()
    }

    /// The Settings sheet, while it is up.
    pub(crate) fn settings_sheet(&self, cx: &App) -> Option<Entity<SettingsView>> {
        self.sheets.read(cx).settings().cloned()
    }

    /// Put Settings up, on its first page, in place of any other sheet,
    /// and ask for what it shows: the CLIs' versions and the machine's
    /// defaults. Every opener comes here.
    pub(crate) fn show_settings(&mut self, cx: &mut Context<Self>) -> Entity<SettingsView> {
        let cockpit = cx.weak_entity();
        let view = cx.new(|cx| SettingsView::new(cockpit, cx));
        cx.subscribe(&view, Self::on_settings_event).detach();
        let connections = view.read(cx).connections.clone();
        cx.subscribe(&connections, Self::on_connections_event).detach();
        self.show_sheet(sheets::Sheet::Settings(view.clone()), cx);
        self.close_composer_menu(cx);
        if self.cli_versions.is_none() {
            self.probe_cli_versions(cx);
        }
        self.ask_server_defaults(cx);
        view
    }
}
