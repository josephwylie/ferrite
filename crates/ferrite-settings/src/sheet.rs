//! The Settings sheet (ADR 0041), made when it opens and dropped once it
//! has gone (ADR 0042). It owns what only the sheet reads: the page shown,
//! the search field, the path last copied, its focus, and the Connections
//! page's own view. What it shows of the window it reads through a
//! `Source` (`SettingsInputs`), asked for each frame, so the window's own
//! notify still redraws it; what the operator asks of the window it says
//! as a `SettingsEvent`, which the window answers once the sheet's own
//! update has ended.

use std::path::PathBuf;
use std::rc::Rc;

use ferrite_core::provider::Provider;
use ferrite_core::settings::{ReadingSize, ServerDefaults, Settings};
use ferrite_core::sound::Sound;
use ferrite_core::ModelInfo;
use ferrite_ui::frame::settings::{self, PageKey};
use ferrite_ui::source::Source;
use gpui::component::input::InputState;
use gpui::prelude::*;
use gpui::{
    AnyWindowHandle, App, Context, Entity, EventEmitter, FocusHandle, SharedString, Subscription,
    Window,
};

use crate::connections::{ConnectionsInputs, ConnectionsView, PageEvent};

pub struct SettingsView {
    /// What the window has, as the pages draw it.
    inputs: Source<SettingsInputs, ()>,
    /// The page the sidebar has selected; each opening starts on the first.
    page: PageKey,
    /// The search field, made on the first frame (it needs a window).
    search: Option<Entity<InputState>>,
    /// The About path copied last, shown with a check until the sheet
    /// closes.
    copied: Option<&'static str>,
    focus: FocusHandle,
    connections: Entity<ConnectionsView>,
    /// What the Connections page asks of the sheet, heard from the first
    /// frame: answering needs the window.
    page_events: Option<Subscription>,
}

/// What the sheet draws from, as the window has it this frame.
#[derive(Clone)]
pub struct SettingsInputs {
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
    /// That machine's defaults, or what stands in for them until it has
    /// said them.
    pub defaults: Result<ServerDefaults, SharedString>,
    /// Its refusal of the last edit to them, said beside the values it kept.
    pub unsaved: Option<SharedString>,
    /// This computer's Server, as Settings › Server says it.
    pub server: ServerInputs,
    /// Where Ferrite's own update stands, and the button that moves it
    /// on; `None` in a build that does not update itself.
    pub ferrite_update: Option<(SharedString, Option<SharedString>)>,
    /// About's Version.
    pub version: SharedString,
    /// A development build, which never installs the LaunchAgent and must
    /// not point the operator's at itself.
    pub dev: bool,
}

/// One provider's models and CLI, as the sheet's rows need them.
#[derive(Clone)]
pub struct ProviderInputs {
    pub provider: Provider,
    pub catalog: Vec<ModelInfo>,
    /// What its CLI announced: the effort ladder reads it.
    pub announced: Vec<ModelInfo>,
    /// Its CLI's update line: what the check found, and the button.
    pub update: (SharedString, Option<SharedString>),
    /// Its CLI on another machine said it is not signed in there: a row
    /// under it signs it in from here (ADR 0051).
    pub signed_out: bool,
}

/// This computer's Server, as Settings › Server has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerInputs {
    /// None this window can act on, and why: turned off, or an Engine of
    /// the window's own.
    None(SharedString),
    /// Where it stands: its build, its uptime and its sessions, or why it
    /// is not running.
    Home(SharedString),
}

/// What the sheet asks of the window.
#[derive(Clone)]
pub enum SettingsEvent {
    /// A setting changed: the window saves and applies it.
    Change(Rc<dyn Fn(&mut Settings)>),
    /// One of the machine's defaults changed, where it lives.
    Defaults(Rc<dyn Fn(&mut ServerDefaults)>),
    StepTextSize(i32),
    StepFactoryLimit(i32),
    StartAtLogin(bool),
    PlaySound(Sound),
    ChooseSound(Sound),
    UpdateCli(Provider),
    /// Sign the CLI in on the machine Settings is about.
    SignIn(Provider),
    CopyDiagnostics,
    // Each asks in the window it was pressed in.
    RestartServer(AnyWindowHandle),
    StopServer(AnyWindowHandle),
    UpdateFerrite(AnyWindowHandle),
    /// The close button closed it.
    Closed,
}

impl EventEmitter<SettingsEvent> for SettingsView {}

impl SettingsView {
    pub fn new(
        inputs: Source<SettingsInputs, ()>,
        connections: Source<ConnectionsInputs, ()>,
        cx: &mut Context<Self>,
    ) -> Self {
        let connections = cx.new(|_| ConnectionsView::new(connections));
        Self {
            inputs,
            page: PageKey::default(),
            search: None,
            copied: None,
            focus: cx.focus_handle(),
            connections,
            page_events: None,
        }
    }

    /// The page shown.
    pub fn page(&self) -> PageKey {
        self.page
    }

    /// The About path copied last, until the sheet closes.
    pub fn copied(&self) -> Option<&'static str> {
        self.copied
    }

    /// The Connections page, which the window answers itself.
    pub fn connections(&self) -> &Entity<ConnectionsView> {
        &self.connections
    }

    /// Show `page`, as the window opens Settings on it.
    pub fn show(&mut self, page: PageKey, cx: &mut Context<Self>) {
        self.show_page(page, cx);
    }

    /// The diagnostics are on the clipboard: About says so until the sheet
    /// closes.
    pub fn diagnostics_copied(&mut self, cx: &mut Context<Self>) {
        self.copied = Some("Diagnostics");
        cx.notify();
    }

    /// Search for `text`, as though it were typed in the sidebar's field.
    pub fn search(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let search = self.search_field(window, cx);
        let text = text.to_string();
        search.update(cx, |search, cx| search.set_value(text, window, cx));
    }

    fn search_field(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        self.search
            .get_or_insert_with(|| cx.new(|cx| InputState::new(window, cx).placeholder("Search")))
            .clone()
    }

    /// A listener that says `event` to the window.
    fn teller(cx: &Context<Self>) -> impl Fn(SettingsEvent, &mut App) + Clone + 'static {
        let view = cx.entity().downgrade();
        move |event, cx| {
            view.update(cx, |_, cx| cx.emit(event)).ok();
        }
    }

    /// A listener that says the window it is pressed in, as `event` asks.
    fn in_window(
        cx: &Context<Self>,
        event: fn(AnyWindowHandle) -> SettingsEvent,
    ) -> impl Fn(&mut Window, &mut App) + 'static {
        let tell = Self::teller(cx);
        move |window, cx| tell(event(window.window_handle()), cx)
    }

    /// A row's change, said to the window to save.
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

    /// A change to one of the machine's defaults, said to the window.
    fn default_change<T: Clone + 'static>(
        cx: &Context<Self>,
        write: impl Fn(&mut ServerDefaults, T) + 'static,
    ) -> impl Fn(T, &mut App) + 'static {
        let tell = Self::teller(cx);
        let write = Rc::new(write);
        move |value, cx| {
            let write = write.clone();
            tell(
                SettingsEvent::Defaults(Rc::new(move |defaults| write(defaults, value.clone()))),
                cx,
            );
        }
    }

    /// The searchable sheet's card, for the sheet host; `None` once the
    /// window has nothing to show.
    pub fn card(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<ferrite_ui::sheet::Drawn> {
        if self.page_events.is_none() {
            self.page_events = Some(cx.subscribe_in(
                &self.connections,
                window,
                |view, _, event: &PageEvent, window, cx| match event {
                    PageEvent::Editing => view.reveal(PageKey::Connections, window, cx),
                    PageEvent::Cancelled => view.refocus(window, cx),
                },
            ));
        }
        let search = self.search_field(window, cx);
        let inputs = self.inputs.read(&(), cx)?;
        let connections = self
            .connections
            .update(cx, |connections, cx| connections.groups(window, cx));
        let pages = self.pages(inputs, connections, cx);
        let select = {
            let view = cx.entity().downgrade();
            move |page: PageKey, window: &mut Window, cx: &mut App| {
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
        Some(ferrite_ui::frame::active().settings(
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

    fn show_page(&mut self, page: PageKey, cx: &mut Context<Self>) {
        self.page = page;
        if page != PageKey::Connections {
            self.connections
                .update(cx, |connections, _| connections.close_editor());
        }
        cx.notify();
    }

    /// Show `page` whole: choosing a page ends a search.
    fn reveal(&mut self, page: PageKey, window: &mut Window, cx: &mut Context<Self>) {
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
            defaults,
            unsaved,
            server,
            ferrite_update,
            version,
            dev,
        } = inputs;
        let tell = Self::teller(cx);
        let defaults = defaults.map_err(|unread| settings::fact("Defaults", unread));
        let unsaved = unsaved.map(|unsaved| settings::fact("Defaults", unsaved));
        let choose_provider = vec![settings::choices(
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
        let mut new_thread_groups = vec![settings::Group::new(None).rows(choose_provider)];
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
                    ferrite_core::models::menu(catalog.clone(), chosen.as_deref()),
                    chosen.as_deref(),
                ),
                Self::change(cx, move |settings, value| {
                    settings.set_model_for(provider, value)
                }),
            );
            let effort = settings.effort_for(provider).map(str::to_string);
            let mut ladder = ferrite_core::models::efforts_for(
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
                            .map(ferrite_ui::provider::effort_title)
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
            size > ReadingSize::SMALLEST,
            size < ReadingSize::LARGEST,
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
            .flat_map(|ProviderInputs { provider, update, signed_out, .. }| {
                let (id, sign_in, version) = match provider {
                    Provider::Claude => ("settings-update-claude", "settings-sign-in-claude", claude.clone()),
                    Provider::Codex => ("settings-update-codex", "settings-sign-in-codex", codex.clone()),
                };
                let (detail, button) = update;
                let updater = tell.clone();
                let cli = settings::action(
                    id,
                    ferrite_ui::provider::cli(provider),
                    detail,
                    version,
                    button,
                    move |_, cx| updater(SettingsEvent::UpdateCli(provider), cx),
                );
                let tell = tell.clone();
                let sign_in = signed_out.then(|| {
                    settings::action(
                        sign_in,
                        "Not signed in",
                        format!("Its Threads on {place} fail until it is signed in there"),
                        SharedString::default(),
                        Some(format!("Sign in to {provider} on {place}").into()),
                        move |_, cx| tell(SettingsEvent::SignIn(provider), cx),
                    )
                    .nested()
                });
                std::iter::once(cli).chain(sign_in)
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
            settings::fact("Version", version),
            settings::path_fact(
                "Threads",
                dir.join("threads").display().to_string(),
                copied == Some("Threads"),
                copy("Threads", cx),
            ),
            settings::path_fact(
                "Settings file",
                dir.join(Settings::FILE).display().to_string(),
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
        about.splice(1..1, Self::ferrite_update_rows(ferrite_update, &settings, cx));
        let mut about = vec![settings::Group::new(None).rows(about)];
        about.push(settings::Group::new(Some("Provider CLIs")).rows(clis));
        let machine = Self::machine_group(&defaults, &unsaved, place, cx);
        vec![
            settings::page(PageKey::NewThreads, new_thread_groups),
            settings::page(
                PageKey::Permissions,
                Self::permission_groups(&defaults, &unsaved, cx),
            )
            .scope(scope.clone()),
            settings::page(
                PageKey::Behaviour,
                vec![
                    settings::Group::new(Some("Reading")).rows(reading),
                    settings::Group::new(Some("Threads and navigation")).rows(behaviour),
                    machine,
                ],
            ),
            settings::page(
                PageKey::Notifications,
                vec![
                    settings::Group::new(Some("Sounds"))
                        .rows(sounds)
                        .rows(provider_sounds),
                    settings::Group::new(Some("System")).rows(system),
                ],
            ),
            settings::page(
                PageKey::Server,
                vec![settings::Group::new(None).rows(Self::server_rows(server, &settings, dev, cx))],
            ),
            // Where Open picks the machine, so where it is named first.
            settings::page(PageKey::Connections, connections).scope(scope.clone()),
            settings::page(PageKey::Labs, Self::labs_groups(&defaults, &unsaved, cx)).scope(scope),
            settings::page(PageKey::About, about),
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

    /// Settings › Permissions: how each provider's Sessions ask before
    /// acting, as the machine they run on says. The page says which
    /// machine atop it (`SettingsInputs::scope`).
    fn permission_groups(
        defaults: &Result<ServerDefaults, settings::Row>,
        unsaved: &Option<settings::Row>,
        cx: &Context<Self>,
    ) -> Vec<settings::Group> {
        let defaults = match defaults {
            Ok(defaults) => defaults,
            Err(unread) => return vec![settings::Group::new(None).rows([unread.clone()])],
        };
        let modes = |options: &[(&str, Option<&str>)], selected: Option<&str>| {
            options
                .iter()
                .map(|(label, value)| {
                    (
                        SharedString::from(label.to_string()),
                        selected == *value,
                        value.map(str::to_string),
                    )
                })
                .collect()
        };
        let claude = vec![settings::choices(
            "settings-claude-mode",
            "Permissions",
            "When Claude asks before acting",
            modes(
                &[
                    (settings::CLI_DEFAULT, None),
                    ("Ask", Some("default")),
                    ("Accept edits", Some("acceptEdits")),
                    ("Plan", Some("plan")),
                    ("Bypass permissions", Some("bypassPermissions")),
                ],
                defaults.claude_permission_mode.as_deref(),
            ),
            Self::default_change(cx, |d, v| d.claude_permission_mode = v),
        )];
        let codex = vec![
            settings::choices(
                "settings-codex-approval",
                "Approvals",
                "When Codex asks before acting",
                modes(
                    &[
                        ("On request", Some("on-request")),
                        ("Untrusted", Some("untrusted")),
                        ("Never", Some("never")),
                    ],
                    Some(&defaults.codex_approval_policy),
                ),
                Self::default_change(cx, |d, v: Option<String>| {
                    d.codex_approval_policy = v.unwrap_or_else(|| "on-request".into())
                }),
            ),
            settings::choices(
                "settings-codex-sandbox",
                "Sandbox",
                "What Codex may touch",
                modes(
                    &[
                        (settings::CLI_DEFAULT, None),
                        ("Read only", Some("read-only")),
                        ("Workspace write", Some("workspace-write")),
                        ("Full access", Some("danger-full-access")),
                    ],
                    defaults.codex_sandbox.as_deref(),
                ),
                Self::default_change(cx, |d, v| d.codex_sandbox = v),
            ),
        ];
        unsaved
            .clone()
            .map(|unsaved| settings::Group::new(None).rows([unsaved]))
            .into_iter()
            .chain([
                provider_group(Provider::Claude).rows(claude),
                provider_group(Provider::Codex).rows(codex),
            ])
            .collect()
    }

    /// Settings › Behaviour's group of what the machine does on its own
    /// account: titles, follow-up suggestions and its Factory's limit.
    fn machine_group(
        defaults: &Result<ServerDefaults, settings::Row>,
        unsaved: &Option<settings::Row>,
        place: SharedString,
        cx: &Context<Self>,
    ) -> settings::Group {
        let applies = settings::fact("Applies to", place);
        let group = settings::Group::new(Some("Machine defaults"));
        let defaults = match defaults {
            Ok(defaults) => defaults,
            Err(unread) => return group.rows([applies, unread.clone()]),
        };
        let at_once = defaults.factory_tasks_at_once;
        let tell = Self::teller(cx);
        group.rows(
            std::iter::once(applies)
                .chain(unsaved.clone())
                .chain([
                    settings::toggle(
                        "settings-auto-title",
                        "Name threads automatically",
                        "Titled from the first prompt until you rename it",
                        defaults.auto_title,
                        Self::default_change(cx, |d, v| d.auto_title = v),
                    ),
                    settings::toggle(
                        "settings-placeholder-suggestions",
                        "Suggest follow-up prompts",
                        "A likely next prompt in an empty composer · tab takes it",
                        defaults.placeholder_suggestions,
                        Self::default_change(cx, |d, v| d.placeholder_suggestions = v),
                    ),
                    settings::stepper(
                        "settings-factory-limit",
                        "Tasks working at once",
                        "In the Factory · the rest wait in Queued",
                        at_once.to_string().into(),
                        at_once > *ferrite_core::factory_board::LIMIT_RANGE.start(),
                        at_once < *ferrite_core::factory_board::LIMIT_RANGE.end(),
                        move |delta, cx| tell(SettingsEvent::StepFactoryLimit(delta), cx),
                    ),
                ]),
        )
    }

    /// Settings › Labs: what is still being tried on the machine, each off
    /// until turned on there (T3 Code's Labs › Beta, scoped to its server).
    /// The page says which machine atop it (`SettingsInputs::scope`).
    fn labs_groups(
        defaults: &Result<ServerDefaults, settings::Row>,
        unsaved: &Option<settings::Row>,
        cx: &Context<Self>,
    ) -> Vec<settings::Group> {
        let group = settings::Group::new(Some("Beta"));
        let defaults = match defaults {
            Ok(defaults) => defaults,
            Err(unread) => return vec![group.rows([unread.clone()])],
        };
        vec![group.rows(
            unsaved
                .clone()
                .into_iter()
                .chain([settings::toggle(
                    "settings-linked-machines",
                    "Linked machines",
                    "Let agents here work on your other machines, and theirs here. Turn it on \
                     for each machine, then link them in Connections. While off, nothing new \
                     starts over a link; running tasks finish.",
                    defaults.linked_machines,
                    Self::default_change(cx, |d, v| d.linked_machines = v),
                )]),
        )]
    }

    /// Settings › Server: this computer's, for a window attached to it.
    fn server_rows(
        server: ServerInputs,
        settings: &Settings,
        dev: bool,
        cx: &Context<Self>,
    ) -> Vec<settings::Row> {
        let status = match server {
            // A sentence, so the row's words (as About says it), not a
            // fact's machine-text value.
            ServerInputs::None(standing) => {
                return vec![settings::action(
                    "settings-server-none",
                    "Status",
                    standing,
                    SharedString::default(),
                    None,
                    |_, _| {},
                )];
            }
            ServerInputs::Home(status) => status,
        };
        let mut rows = vec![settings::fact("Status", status)];
        if !dev {
            let tell = Self::teller(cx);
            rows.push(settings::toggle(
                "settings-start-at-login",
                "Start at login",
                "Your agents keep running with no window open",
                settings.start_at_login,
                move |on, cx| tell(SettingsEvent::StartAtLogin(on), cx),
            ));
        }
        rows.extend([
            settings::action(
                "settings-server-restart",
                "Restart",
                "Restart the server and resume every open Thread",
                SharedString::default(),
                Some("Restart".into()),
                Self::in_window(cx, SettingsEvent::RestartServer),
            ),
            settings::action(
                "settings-server-stop",
                "Stop",
                "Stop the server and its agents; a window starts it again",
                SharedString::default(),
                Some("Stop".into()),
                Self::in_window(cx, SettingsEvent::StopServer),
            ),
        ]);
        rows
    }

    /// Settings › About's rows for Ferrite's own update: where it stands,
    /// with the one button that moves it on, and the automatic checks.
    /// None in a build that does not update itself.
    fn ferrite_update_rows(
        update: Option<(SharedString, Option<SharedString>)>,
        settings: &Settings,
        cx: &Context<Self>,
    ) -> Vec<settings::Row> {
        let Some((detail, button)) = update else {
            return Vec::new();
        };
        vec![
            settings::action(
                "settings-update-ferrite",
                "Updates",
                detail,
                SharedString::default(),
                button,
                Self::in_window(cx, SettingsEvent::UpdateFerrite),
            ),
            settings::toggle(
                "settings-check-for-updates",
                "Check for updates automatically",
                "Look for a newer Ferrite every few hours and download it. It installs \
                 when Ferrite restarts, which waits for your agents.",
                settings.check_for_updates,
                Self::change(cx, |s, v| s.check_for_updates = v),
            ),
        ]
    }
}

/// A Settings group for one Provider: its name, led by its logomark in
/// brand colour.
fn provider_group(provider: Provider) -> settings::Group {
    let (mark, ink) = ferrite_ui::provider::mark(provider);
    settings::Group::new(Some(ferrite_ui::provider::title(provider))).mark(mark, ink)
}

impl gpui::Focusable for SettingsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrite_ui::testing::{click, mount, record, shows, Heard};
    use gpui::{deferred, div, px, Focusable as _, TestAppContext, VisualTestContext};

    /// The sheet as the sheet host lays it: its card on the veil, and what
    /// it puts above it.
    struct Host {
        sheet: Entity<SettingsView>,
    }

    impl Render for Host {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let drawn = self.sheet.update(cx, |sheet, cx| sheet.card(window, cx));
            div().size_full().relative().when_some(drawn, |host, drawn| {
                host.child(ferrite_ui::sheet::veil().child(drawn.card))
                    .children(drawn.above.map(|above| deferred(above).with_priority(4)))
            })
        }
    }

    /// What the window has for the sheet, as a test keeps it: `None` once
    /// it has nothing to show.
    struct Kept(Option<SettingsInputs>);

    fn kept(kept: &Entity<Kept>, cx: &mut VisualTestContext, change: impl FnOnce(&mut SettingsInputs)) {
        kept.update(cx, |kept, cx| {
            change(kept.0.as_mut().unwrap());
            cx.notify();
        });
        cx.run_until_parked();
    }

    /// A window on `box`, its Server attached, each CLI with its own line.
    fn inputs() -> SettingsInputs {
        let update = |provider| match provider {
            Provider::Claude => ("Claude Code 2.4.0 is out".into(), Some("Update".into())),
            Provider::Codex => ("Codex is up to date".into(), Some("Check".into())),
        };
        SettingsInputs {
            settings: Settings::default(),
            providers: Provider::ALL
                .into_iter()
                .map(|provider| ProviderInputs {
                    provider,
                    catalog: Vec::new(),
                    announced: Vec::new(),
                    update: update(provider),
                    signed_out: false,
                })
                .collect(),
            cli_versions: Some(("2.3.1 (Claude Code)".into(), "codex-cli 0.61.0".into())),
            place: "box".into(),
            scope: Some("On box".into()),
            dir: PathBuf::from("/ferrite"),
            defaults: Ok(ServerDefaults::default()),
            unsaved: None,
            server: ServerInputs::Home("1.2.3 · up 5m · 2 live sessions".into()),
            ferrite_update: Some(("Ferrite 1.3.0 is ready".into(), Some("Restart to update".into()))),
            version: "1.2.3".into(),
            dev: false,
        }
    }

    /// The sheet in a window of its own, reading `inputs` as the window
    /// keeps them.
    fn open(
        inputs: SettingsInputs,
        cx: &mut TestAppContext,
    ) -> (Entity<SettingsView>, Entity<Kept>, &mut VisualTestContext) {
        // The app's frame; its key table names the window's actions, which
        // are not here.
        ferrite_ui::frame::install(ferrite_frame::Frame { menus: Vec::new });
        let kept = cx.new(|_| Kept(Some(inputs)));
        let source = Source::of(&kept, |kept: &Kept, _: &(), _| kept.0.clone());
        let connections = Source::of(&kept, |kept: &Kept, _: &(), _| {
            kept.0.as_ref().map(|_| ConnectionsInputs::default())
        });
        let (host, cx) = mount(cx, move |_, cx| Host {
            sheet: cx.new(|cx| SettingsView::new(source, connections, cx)),
        });
        cx.simulate_resize(gpui::size(px(1000.), px(700.)));
        cx.run_until_parked();
        let sheet = host.read_with(cx, |host, _| host.sheet.clone());
        (sheet, kept, cx)
    }

    fn heard(sheet: &Entity<SettingsView>, cx: &mut VisualTestContext) -> Heard<SettingsEvent> {
        record(cx, sheet, |event: &SettingsEvent| event.clone())
    }

    /// The card's words, as drawn.
    fn says(cx: &mut VisualTestContext) -> String {
        cx.update(|window, _| window.debug_text("settings-card"))
            .unwrap_or_default()
            .join(" ")
    }

    /// Search for `query`: the card is the sheet's height, so a row low on
    /// a long page is found, as the operator finds it, to be pressed.
    fn seek(sheet: &Entity<SettingsView>, query: &str, cx: &mut VisualTestContext) {
        sheet.update_in(cx, |sheet, window, cx| sheet.search(query, window, cx));
        cx.run_until_parked();
    }

    fn page(sheet: &Entity<SettingsView>, cx: &mut VisualTestContext) -> PageKey {
        sheet.read_with(cx, |sheet, _| sheet.page())
    }

    fn handle(cx: &mut VisualTestContext) -> AnyWindowHandle {
        cx.update(|window, _| window.window_handle())
    }

    /// What `selector`'s press asks of the window: one edit, which it
    /// makes of `settings`.
    fn edited(cx: &mut VisualTestContext, heard: &Heard<SettingsEvent>, selector: &str, settings: &Settings) -> Settings {
        click(cx, selector);
        let mut settings = settings.clone();
        let events = heard.take();
        assert_eq!(events.len(), 1, "{selector}: one edit");
        match &events[0] {
            SettingsEvent::Change(change) => change(&mut settings),
            _ => panic!("{selector}: not an edit of Settings"),
        }
        settings
    }

    /// `edited`, of the machine's defaults.
    fn defaults_edited(cx: &mut VisualTestContext, heard: &Heard<SettingsEvent>, selector: &str) -> ServerDefaults {
        click(cx, selector);
        let mut defaults = ServerDefaults::default();
        let events = heard.take();
        assert_eq!(events.len(), 1, "{selector}: one edit");
        match &events[0] {
            SettingsEvent::Defaults(change) => change(&mut defaults),
            _ => panic!("{selector}: not an edit of the defaults"),
        }
        defaults
    }

    /// With the sheet itself focused, as the sheet host focuses it, ↓ and
    /// ↑ step the sidebar's pages and hold at the ends.
    #[gpui::test]
    fn the_arrows_step_the_pages_and_hold_at_the_ends(cx: &mut TestAppContext) {
        let (sheet, _kept, cx) = open(inputs(), cx);
        cx.update(|window, cx| window.focus(&sheet.read(cx).focus_handle(cx), cx));
        assert_eq!(page(&sheet, cx), PageKey::NewThreads);
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
        assert_eq!(page(&sheet, cx), PageKey::Permissions);
        assert!(shows(cx, "settings-codex-sandbox"));
        assert!(!shows(cx, "settings-provider-0"));
        cx.simulate_keystrokes("down down down down down down");
        cx.run_until_parked();
        assert_eq!(page(&sheet, cx), PageKey::About, "held at the foot");
        cx.simulate_keystrokes("up");
        cx.run_until_parked();
        assert_eq!(page(&sheet, cx), PageKey::Labs);
        assert!(shows(cx, "settings-linked-machines"));
        cx.simulate_keystrokes("up up up up");
        cx.run_until_parked();
        assert_eq!(page(&sheet, cx), PageKey::Behaviour);
        cx.simulate_keystrokes("up up up up");
        cx.run_until_parked();
        assert_eq!(page(&sheet, cx), PageKey::NewThreads, "held at the head");
    }

    /// A path in About copies whole on a press, and shows its check until
    /// another is copied.
    #[gpui::test]
    fn an_about_path_copies_whole_and_shows_its_check(cx: &mut TestAppContext) {
        let (sheet, _kept, cx) = open(inputs(), cx);
        click(cx, "settings-page-about");
        click(cx, "settings-fact-Threads");
        assert_eq!(sheet.read_with(cx, |sheet, _| sheet.copied()), Some("Threads"));
        let threads = PathBuf::from("/ferrite").join("threads").display().to_string();
        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some(threads),
            "the whole path, never the ~ form"
        );
        click(cx, "settings-fact-Settings file");
        assert_eq!(sheet.read_with(cx, |sheet, _| sheet.copied()), Some("Settings file"));
    }

    /// Each of the window's own settings is said as an edit, which the
    /// window saves: the provider, a provider's effort, the switches and
    /// the update checks.
    #[gpui::test]
    fn each_setting_is_said_as_an_edit_of_settings(cx: &mut TestAppContext) {
        let mut given = inputs();
        given.settings.set_effort_for(Provider::Claude, Some("high".into()));
        let settings = given.settings.clone();
        let (sheet, _kept, cx) = open(given, cx);
        let heard = heard(&sheet, cx);
        assert_eq!(edited(cx, &heard, "settings-provider-1", &settings).default_provider, Provider::Codex);
        seek(&sheet, "Effort", cx);
        // A long ladder is a menu of its values.
        click(cx, "settings-claude-effort");
        let effort = edited(cx, &heard, "settings-claude-effort-option-0", &settings);
        assert_eq!(effort.effort_for(Provider::Claude), None, "the CLI's default");
        assert_eq!(effort.effort_for(Provider::Codex), settings.effort_for(Provider::Codex));
        seek(&sheet, "Confirm before deleting", cx);
        assert_eq!(edited(cx, &heard, "settings-confirm-delete", &settings).confirm_delete, !settings.confirm_delete);
        seek(&sheet, "Start with the sidebar collapsed", cx);
        assert_eq!(edited(cx, &heard, "settings-nav-collapsed", &settings).nav_collapsed, !settings.nav_collapsed);
        seek(&sheet, "Play sounds", cx);
        assert_eq!(edited(cx, &heard, "settings-sounds", &settings).sounds, !settings.sounds);
        seek(&sheet, "Sounds for", cx);
        for provider in Provider::ALL {
            let id = match provider {
                Provider::Claude => "settings-claude-sounds",
                Provider::Codex => "settings-codex-sounds",
            };
            let after = edited(cx, &heard, id, &settings);
            assert_eq!(after.provider_sounds(provider), !settings.provider_sounds(provider), "{id}");
            for other in Provider::ALL.into_iter().filter(|other| *other != provider) {
                assert_eq!(after.provider_sounds(other), settings.provider_sounds(other), "{id} is its own");
            }
        }
        seek(&sheet, "System notifications", cx);
        assert_eq!(
            edited(cx, &heard, "settings-system-notifications", &settings).system_notifications,
            !settings.system_notifications
        );
        seek(&sheet, "Update the CLIs automatically", cx);
        assert_eq!(edited(cx, &heard, "settings-auto-update-clis", &settings).auto_update_clis, !settings.auto_update_clis);
        seek(&sheet, "Check for updates automatically", cx);
        assert_eq!(
            edited(cx, &heard, "settings-check-for-updates", &settings).check_for_updates,
            !settings.check_for_updates
        );
    }

    /// The machine's defaults are said as edits of them, which the window
    /// makes where they live.
    #[gpui::test]
    fn the_machines_defaults_are_said_as_edits_of_them(cx: &mut TestAppContext) {
        let (sheet, _kept, cx) = open(inputs(), cx);
        let heard = heard(&sheet, cx);
        let before = ServerDefaults::default();
        seek(&sheet, "When Claude asks before acting", cx);
        click(cx, "settings-claude-mode");
        assert_eq!(
            defaults_edited(cx, &heard, "settings-claude-mode-option-2").claude_permission_mode.as_deref(),
            Some("acceptEdits")
        );
        seek(&sheet, "When Codex asks before acting", cx);
        assert_eq!(defaults_edited(cx, &heard, "settings-codex-approval-1").codex_approval_policy, "untrusted");
        seek(&sheet, "What Codex may touch", cx);
        click(cx, "settings-codex-sandbox");
        assert_eq!(
            defaults_edited(cx, &heard, "settings-codex-sandbox-option-1").codex_sandbox.as_deref(),
            Some("read-only")
        );
        seek(&sheet, "Name threads automatically", cx);
        assert_eq!(defaults_edited(cx, &heard, "settings-auto-title").auto_title, !before.auto_title);
        seek(&sheet, "Suggest follow-up prompts", cx);
        assert_eq!(
            defaults_edited(cx, &heard, "settings-placeholder-suggestions").placeholder_suggestions,
            !before.placeholder_suggestions
        );
        seek(&sheet, "Linked machines", cx);
        assert_eq!(defaults_edited(cx, &heard, "settings-linked-machines").linked_machines, !before.linked_machines);
    }

    /// The text size and the Factory's limit are stepped by the window, a
    /// step at a time, and not past either end.
    #[gpui::test]
    fn the_steppers_say_their_steps_and_stop_at_the_ends(cx: &mut TestAppContext) {
        let (sheet, kept_by, cx) = open(inputs(), cx);
        let heard = heard(&sheet, cx);
        let steps = |cx: &mut VisualTestContext, selector: &str| -> Vec<(&'static str, i32)> {
            let query = if selector.starts_with("settings-text-size") { "Text size" } else { "Tasks working at once" };
            seek(&sheet, query, cx);
            click(cx, selector);
            heard
                .take()
                .into_iter()
                .map(|event| match event {
                    SettingsEvent::StepTextSize(delta) => ("text", delta),
                    SettingsEvent::StepFactoryLimit(delta) => ("factory", delta),
                    _ => panic!("{selector}: not a step"),
                })
                .collect()
        };
        assert_eq!(steps(cx, "settings-text-size-1"), [("text", 1)]);
        assert_eq!(steps(cx, "settings-text-size-0"), [("text", -1)]);
        assert_eq!(steps(cx, "settings-factory-limit-1"), [("factory", 1)]);
        assert_eq!(steps(cx, "settings-factory-limit-0"), [("factory", -1)]);
        kept(&kept_by, cx, |inputs| {
            inputs.settings.reading_size = ReadingSize::LARGEST;
            inputs.defaults = Ok(ServerDefaults {
                factory_tasks_at_once: *ferrite_core::factory_board::LIMIT_RANGE.end(),
                ..ServerDefaults::default()
            });
        });
        assert_eq!(steps(cx, "settings-text-size-1"), [], "at the largest");
        assert_eq!(steps(cx, "settings-factory-limit-1"), [], "at the limit's top");
        kept(&kept_by, cx, |inputs| {
            inputs.settings.reading_size = ReadingSize::SMALLEST;
            inputs.defaults = Ok(ServerDefaults {
                factory_tasks_at_once: *ferrite_core::factory_board::LIMIT_RANGE.start(),
                ..ServerDefaults::default()
            });
        });
        assert_eq!(steps(cx, "settings-text-size-0"), [], "at the smallest");
        assert_eq!(steps(cx, "settings-factory-limit-0"), [], "at the limit's foot");
    }

    /// Until the machine has said its defaults, what stands in for them
    /// is said on each page that shows them, with none of their rows; a
    /// refusal of an edit is said there beside the values kept.
    #[gpui::test]
    fn the_defaults_unread_or_refused_are_said_where_they_are_shown(cx: &mut TestAppContext) {
        let unread = SettingsInputs { defaults: Err("Reading from box…".into()), ..inputs() };
        let (_sheet, kept_by, cx) = open(unread, cx);
        let pages = [
            ("settings-page-permissions", "settings-codex-sandbox"),
            ("settings-page-behaviour", "settings-factory-limit-1"),
            ("settings-page-labs", "settings-linked-machines"),
        ];
        for (page, row) in pages {
            click(cx, page);
            assert!(says(cx).contains("Reading from box…"), "{page}: {}", says(cx));
            assert!(!shows(cx, row), "{page}");
        }
        click(cx, "settings-page-behaviour");
        assert!(says(cx).contains("Applies to box"), "{}", says(cx));
        kept(&kept_by, cx, |inputs| {
            inputs.defaults = Ok(ServerDefaults::default());
            inputs.unsaved = Some("Not saved on box: read-only".into());
        });
        for (page, row) in pages {
            click(cx, page);
            assert!(says(cx).contains("Not saved on box: read-only"), "{page}: {}", says(cx));
            assert!(!says(cx).contains("Reading from box"), "{page}");
            assert!(shows(cx, row), "{page}: beside the values kept");
        }
    }

    /// Settings › Server says where this computer's Server stands; Restart
    /// and Stop ask in the window they were pressed in; Start at login is
    /// the window's to set, and never a development build's.
    #[gpui::test]
    fn the_server_page_says_where_it_stands_and_asks_in_its_window(cx: &mut TestAppContext) {
        let none = ServerInputs::None("This window runs without a separate server".into());
        let (sheet, kept_by, cx) = open(SettingsInputs { server: none, ..inputs() }, cx);
        let heard = heard(&sheet, cx);
        click(cx, "settings-page-server");
        assert!(says(cx).contains("This window runs without a separate server"));
        for row in ["settings-server-restart", "settings-server-stop", "settings-start-at-login"] {
            assert!(!shows(cx, row), "{row}: no server to act on");
        }
        kept(&kept_by, cx, |inputs| inputs.server = ServerInputs::Home("1.2.3 · up 5m · 2 live sessions".into()));
        assert!(says(cx).contains("1.2.3 · up 5m · 2 live sessions"), "{}", says(cx));
        assert!(!shows(cx, "settings-server-none"));
        let here = handle(cx);
        click(cx, "settings-server-restart");
        assert!(matches!(heard.take()[..], [SettingsEvent::RestartServer(window)] if window == here));
        click(cx, "settings-server-stop");
        assert!(matches!(heard.take()[..], [SettingsEvent::StopServer(window)] if window == here));
        let on = !Settings::default().start_at_login;
        click(cx, "settings-start-at-login");
        assert!(matches!(heard.take()[..], [SettingsEvent::StartAtLogin(asked)] if asked == on));
        kept(&kept_by, cx, |inputs| inputs.dev = true);
        assert!(!shows(cx, "settings-start-at-login"), "a development build");
        assert!(shows(cx, "settings-server-restart"));
    }

    /// About offers Ferrite's own update, asked in the window it was
    /// pressed in, only in a build that updates itself.
    #[gpui::test]
    fn ferrites_update_is_offered_where_it_updates_itself(cx: &mut TestAppContext) {
        let (sheet, kept_by, cx) = open(inputs(), cx);
        let heard = heard(&sheet, cx);
        click(cx, "settings-page-about");
        assert!(says(cx).contains("Ferrite 1.3.0 is ready"), "{}", says(cx));
        let here = handle(cx);
        click(cx, "settings-update-ferrite");
        assert!(matches!(heard.take()[..], [SettingsEvent::UpdateFerrite(window)] if window == here));
        kept(&kept_by, cx, |inputs| inputs.ferrite_update = None);
        assert!(!shows(cx, "settings-update-ferrite"));
        assert!(!shows(cx, "settings-check-for-updates"));
        assert!(!says(cx).contains("Ferrite 1.3.0 is ready"));
    }

    /// The CLIs say their versions once known, each its own update line,
    /// and a press updates that CLI; the automatic updates name the
    /// machine they install on. A row with an update to offer has its
    /// button where the version goes, which search still finds it by.
    #[gpui::test]
    fn the_clis_say_their_versions_and_update_on_a_press(cx: &mut TestAppContext) {
        let (sheet, kept_by, cx) = open(SettingsInputs { cli_versions: None, ..inputs() }, cx);
        let heard = heard(&sheet, cx);
        kept(&kept_by, cx, |inputs| inputs.providers[1].update = ("Codex is up to date".into(), None));
        seek(&sheet, "Codex CLI", cx);
        assert!(says(cx).contains("checking…"), "{}", says(cx));
        kept(&kept_by, cx, |inputs| {
            inputs.cli_versions = Some(("2.3.1 (Claude Code)".into(), "codex-cli 0.61.0".into()))
        });
        let said = says(cx);
        assert!(said.contains("codex-cli 0.61.0") && said.contains("Codex is up to date"), "{said}");
        assert!(!said.contains("checking…"));
        assert!(!shows(cx, "settings-update-codex"), "nothing to update");
        seek(&sheet, "2.3.1 (Claude Code)", cx);
        assert!(says(cx).contains("Claude Code 2.4.0 is out"), "{}", says(cx));
        click(cx, "settings-update-claude");
        assert!(matches!(heard.take()[..], [SettingsEvent::UpdateCli(Provider::Claude)]));
        kept(&kept_by, cx, |inputs| inputs.providers[1].update = ("Codex 0.62.0 is out".into(), Some("Update".into())));
        seek(&sheet, "Codex CLI", cx);
        click(cx, "settings-update-codex");
        assert!(matches!(heard.take()[..], [SettingsEvent::UpdateCli(Provider::Codex)]));
        seek(&sheet, "Update the CLIs automatically", cx);
        assert!(says(cx).contains("installed on box"), "{}", says(cx));
    }

    /// A CLI the machine says is not signed in has a row under it, whose
    /// press signs it in there; one signed in has none.
    #[gpui::test]
    fn a_cli_not_signed_in_there_offers_to_sign_it_in(cx: &mut TestAppContext) {
        let (sheet, kept_by, cx) = open(inputs(), cx);
        let heard = heard(&sheet, cx);
        seek(&sheet, "Not signed in", cx);
        assert!(!shows(cx, "settings-sign-in-claude") && !shows(cx, "settings-sign-in-codex"));
        kept(&kept_by, cx, |inputs| inputs.providers[1].signed_out = true);
        assert!(says(cx).contains("Sign in to Codex on box"), "{}", says(cx));
        assert!(!shows(cx, "settings-sign-in-claude"));
        click(cx, "settings-sign-in-codex");
        assert!(matches!(heard.take()[..], [SettingsEvent::SignIn(Provider::Codex)]));
    }

    /// Each chime plays and is chosen by the window; one of the operator's
    /// own goes back to the built-in, as an edit.
    #[gpui::test]
    fn a_chime_plays_is_chosen_and_goes_back_to_the_built_in(cx: &mut TestAppContext) {
        let (sheet, kept_by, cx) = open(inputs(), cx);
        let heard = heard(&sheet, cx);
        click(cx, "settings-page-notifications");
        let asked = |cx: &mut VisualTestContext, selector: &str| -> Vec<(&'static str, Sound)> {
            click(cx, selector);
            heard
                .take()
                .into_iter()
                .map(|event| match event {
                    SettingsEvent::PlaySound(sound) => ("play", sound),
                    SettingsEvent::ChooseSound(sound) => ("choose", sound),
                    _ => panic!("{selector}"),
                })
                .collect()
        };
        assert_eq!(asked(cx, "settings-needs-you-sound-0"), [("play", Sound::NeedsYou)]);
        assert_eq!(asked(cx, "settings-needs-you-sound-1"), [("choose", Sound::NeedsYou)]);
        assert_eq!(asked(cx, "settings-done-sound-0"), [("play", Sound::Done)]);
        assert_eq!(asked(cx, "settings-done-sound-1"), [("choose", Sound::Done)]);
        assert!(!shows(cx, "settings-done-sound-2"), "the built-in already");
        kept(&kept_by, cx, |inputs| {
            inputs.settings.set_sound_file(Sound::Done, Some("/sounds/bell.wav".into()))
        });
        let settings = kept_by.read_with(cx, |kept, _| kept.0.clone().unwrap().settings);
        let after = edited(cx, &heard, "settings-done-sound-2", &settings);
        assert_eq!(after.sound_file(Sound::Done), None, "built-in again");
        assert!(!shows(cx, "settings-needs-you-sound-2"));
    }

    /// About says the version the window gives; Copy asks the window for
    /// the diagnostics, and says Copied once it has them.
    #[gpui::test]
    fn about_says_the_version_and_copies_the_diagnostics(cx: &mut TestAppContext) {
        let (sheet, _kept, cx) = open(SettingsInputs { version: "1.2.3 · dev".into(), ..inputs() }, cx);
        let heard = heard(&sheet, cx);
        click(cx, "settings-page-about");
        assert!(says(cx).contains("1.2.3 · dev"), "{}", says(cx));
        assert!(!says(cx).contains("Copied"));
        click(cx, "settings-copy-diagnostics");
        assert!(matches!(heard.take()[..], [SettingsEvent::CopyDiagnostics]));
        sheet.update(cx, |sheet, cx| sheet.diagnostics_copied(cx));
        cx.run_until_parked();
        assert!(says(cx).contains("Copied"), "{}", says(cx));
        assert_eq!(sheet.read_with(cx, |sheet, _| sheet.copied()), Some("Diagnostics"));
    }

    /// A search shows every row that matches it, of every page, and only
    /// those; choosing a page ends it.
    #[gpui::test]
    fn a_search_finds_its_rows_and_choosing_a_page_ends_it(cx: &mut TestAppContext) {
        let (sheet, _kept, cx) = open(inputs(), cx);
        click(cx, "settings-search");
        cx.simulate_input("Suggest follow-up prompts");
        cx.run_until_parked();
        assert!(shows(cx, "settings-placeholder-suggestions"));
        assert!(!shows(cx, "settings-provider-0"));
        assert!(!shows(cx, "settings-confirm-delete"));
        click(cx, "settings-page-about");
        assert!(shows(cx, "settings-copy-diagnostics"), "About whole");
        assert!(!shows(cx, "settings-placeholder-suggestions"));
        let query = |cx: &mut VisualTestContext| {
            let search = sheet.read_with(cx, |sheet, _| sheet.search.clone().unwrap());
            search.read_with(cx, |search, _| search.value().to_string())
        };
        assert_eq!(query(cx), "");
        sheet.update_in(cx, |sheet, window, cx| sheet.search("sandbox", window, cx));
        cx.run_until_parked();
        assert_eq!(query(cx), "sandbox");
        assert!(shows(cx, "settings-codex-sandbox"));
        assert!(!shows(cx, "settings-copy-diagnostics"));
    }

    #[gpui::test]
    fn the_close_button_says_it_closed(cx: &mut TestAppContext) {
        let (sheet, _kept, cx) = open(inputs(), cx);
        let heard = heard(&sheet, cx);
        click(cx, "settings-close");
        assert!(matches!(heard.take()[..], [SettingsEvent::Closed]));
    }

    /// Another page drops the connection editor; the editor opened shows
    /// Connections whole; and cancelling it gives the sheet the keyboard
    /// back.
    #[gpui::test]
    fn connections_shows_its_editor_only_on_its_own_page(cx: &mut TestAppContext) {
        let (sheet, _kept, cx) = open(inputs(), cx);
        click(cx, "settings-page-connections");
        click(cx, "connection-add");
        assert!(shows(cx, "connection-dialog"));
        click(cx, "settings-page-about");
        click(cx, "settings-page-connections");
        assert!(!shows(cx, "connection-dialog"), "another page dropped the editor");
        assert!(shows(cx, "connection-add"));
        click(cx, "settings-page-about");
        click(cx, "settings-search");
        cx.simulate_input("Diagnostics");
        cx.run_until_parked();
        let connections = sheet.read_with(cx, |sheet, _| sheet.connections().clone());
        connections.update_in(cx, |connections, window, cx| connections.edit(None, std::path::Path::new(""), window, cx));
        cx.run_until_parked();
        assert_eq!(page(&sheet, cx), PageKey::Connections, "the editor shows its page");
        assert!(shows(cx, "connection-dialog"));
        let search = sheet.read_with(cx, |sheet, _| sheet.search.clone().unwrap());
        assert_eq!(search.read_with(cx, |search, _| search.value().to_string()), "", "and ends the search");
        click(cx, "connection-close");
        let focus = sheet.read_with(cx, |sheet, cx| sheet.focus_handle(cx));
        assert!(cx.update(|window, _| focus.is_focused(window)), "the sheet has the keyboard");
    }

    /// The window opens Settings on a page; one that is not Connections
    /// drops the connection editor.
    #[gpui::test]
    fn the_window_opens_it_on_a_page(cx: &mut TestAppContext) {
        let (sheet, _kept, cx) = open(inputs(), cx);
        sheet.update(cx, |sheet, cx| sheet.show(PageKey::Connections, cx));
        cx.run_until_parked();
        assert_eq!(page(&sheet, cx), PageKey::Connections);
        click(cx, "connection-add");
        assert!(shows(cx, "connection-dialog"));
        sheet.update(cx, |sheet, cx| sheet.show(PageKey::Server, cx));
        cx.run_until_parked();
        assert_eq!(page(&sheet, cx), PageKey::Server);
        assert!(!shows(cx, "connection-dialog"));
    }

    /// What the sheet shows it reads from the window, which redraws it;
    /// with nothing from the window it draws nothing.
    #[gpui::test]
    fn the_sheet_reads_the_window(cx: &mut TestAppContext) {
        let (_sheet, kept_by, cx) = open(inputs(), cx);
        click(cx, "settings-page-behaviour");
        assert!(says(cx).contains("Applies to box"));
        kept(&kept_by, cx, |inputs| inputs.place = "spare".into());
        assert!(says(cx).contains("Applies to spare"), "the window's notify redraws it");
        kept_by.update(cx, |kept, cx| {
            kept.0 = None;
            cx.notify();
        });
        cx.run_until_parked();
        assert!(!shows(cx, "settings-card"));
    }
}
