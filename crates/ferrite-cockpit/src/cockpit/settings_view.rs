//! The Settings sheet in the window (ADR 0041). The sheet is
//! `ferrite_settings::sheet::SettingsView`, made when it opens and dropped
//! once it has gone (ADR 0042). It reads what it draws through a `Source`
//! the root makes over itself (`settings_inputs`); what the operator asks
//! of the window it says as a `SettingsEvent`, which the root answers here.

use super::*;
use ferrite_settings::sheet::{ProviderInputs, SettingsEvent, SettingsInputs, SettingsView};
use ferrite_ui::source::Source;
use gpui::{AnyWindowHandle, App};

impl CockpitView {
    /// Everything the sheet draws from. It never reads the sheet: the
    /// sheet reads it, from its own listeners and as it draws.
    pub(super) fn settings_inputs(&self, cx: &App) -> SettingsInputs {
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
                signed_out: self.cli_signed_out(provider),
            })
            .collect();
        SettingsInputs {
            settings: self.prefs.settings.clone(),
            providers,
            cli_versions: self.environment.cli_versions.clone(),
            scope: self.settings_scope(cx),
            dir: self.prefs.dir.clone(),
            defaults: self.shown_defaults(cx).ok_or_else(|| self.defaults_unread(cx)),
            unsaved: self.defaults_unsaved(cx),
            server: self.server_inputs(cx),
            ferrite_update: self.prefs.ferrite_updates.then(|| {
                let busy = self.in_flight_here(cx);
                self.ferrite_update.describe(busy.as_deref())
            }),
            version: version_label(env!("CARGO_PKG_VERSION"), crate::DEV).into(),
            dev: crate::DEV,
            place,
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
            SettingsEvent::Defaults(change) => self.change_defaults(|defaults| change(defaults), cx),
            SettingsEvent::StepTextSize(delta) => self.step_text_size(delta, cx),
            SettingsEvent::StepFactoryLimit(delta) => self.step_factory_limit(delta, cx),
            SettingsEvent::StartAtLogin(on) => self.set_start_at_login(on, cx),
            SettingsEvent::PlaySound(sound) => self.chime(sound),
            SettingsEvent::ChooseSound(sound) => self.choose_sound_file(sound, cx),
            SettingsEvent::UpdateCli(provider) => self.update_cli(provider, cx),
            SettingsEvent::SignIn(provider) => {
                if let Some(machine) = self.settings_machine(cx) {
                    self.sign_in_cli(machine, provider, cx);
                }
            }
            SettingsEvent::CopyDiagnostics => self.copy_diagnostics(cx),
            SettingsEvent::RestartServer(window) => self.in_window(window, Self::restart_server, cx),
            SettingsEvent::StopServer(window) => self.in_window(window, Self::stop_server, cx),
            SettingsEvent::UpdateFerrite(window) => {
                self.in_window(window, Self::press_ferrite_update, cx)
            }
            SettingsEvent::Closed => {
                self.hide_sheet(cx);
            }
        }
    }

    /// `act` with `window`, once the press that asked it has ended: a
    /// prompt it puts up needs the window.
    fn in_window(
        &mut self,
        window: AnyWindowHandle,
        act: fn(&mut Self, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) {
        let root = cx.entity();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                root.update(cx, |root, cx| act(root, window, cx));
            });
        });
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
        let inputs = Source::of(&cx.entity(), |root: &Self, _: &(), cx| Some(root.settings_inputs(cx)));
        let connections = Source::of(&cx.entity(), |root: &Self, _: &(), cx| Some(root.connections_inputs(cx)));
        let view = cx.new(|cx| SettingsView::new(inputs, connections, cx));
        cx.subscribe(&view, Self::on_settings_event).detach();
        let connections = view.read(cx).connections().clone();
        cx.subscribe(&connections, Self::on_connections_event).detach();
        self.show_sheet(sheets::Sheet::Settings(view.clone()), cx);
        if self.environment.cli_versions.is_none() {
            self.probe_cli_versions(cx);
        }
        self.ask_server_defaults(cx);
        view
    }
}

impl CockpitView {
    /// cmd-, and the gear: the Settings panel, toggled. Opening it probes
    /// the CLIs' versions once, off the UI thread.
    pub(super) fn open_settings(&mut self, _: &OpenSettings, _window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_settings(cx);
    }

    pub(super) fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        if self.settings_open(cx) {
            self.hide_sheet(cx);
            return;
        }
        self.show_settings(cx);
    }

    /// Settings › About's Diagnostics: what a bug report needs, on the
    /// clipboard (`ferrite_core::diagnostics::report`). The report reads
    /// up to the whole log, so it is gathered in the background.
    pub(super) fn copy_diagnostics(&mut self, cx: &mut Context<Self>) {
        let (claude, codex) = self
            .environment
            .cli_versions
            .clone()
            .unwrap_or_else(|| ("unknown".into(), "unknown".into()));
        let facts = [
            (
                "Version",
                version_label(env!("CARGO_PKG_VERSION"), crate::DEV),
            ),
            (
                "Platform",
                format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            ),
            ("Claude CLI", claude.to_string()),
            ("Codex CLI", codex.to_string()),
            ("Open Threads", self.cockpit.read(cx).threads().len().to_string()),
        ];
        let dir = self.prefs.dir.clone();
        cx.spawn(async move |this, cx| {
            let report = cx
                .background_executor()
                .spawn(async move { ferrite_core::diagnostics::report(&dir, &facts) })
                .await;
            this.update(cx, |view, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(report));
                if let Some(settings) = view.settings_sheet(cx) {
                    settings.update(cx, |settings, cx| settings.diagnostics_copied(cx));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A `settings.json` that could not be used is said once and kept on
    /// screen: the window runs on the defaults and saves nothing over the
    /// file, so a change made now lasts only until Ferrite quits.
    pub(super) fn present_unreadable_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use gpui::component::notification::{Notification, NotificationType};
        use gpui::component::WindowExt as _;
        if self.unreadable_told {
            return;
        }
        let Some(reason) = &self.prefs.unreadable else {
            return;
        };
        self.unreadable_told = true;
        let path = self.prefs.dir.join(ferrite_core::settings::Settings::FILE);
        window.push_notification(
            Notification::new()
                .title("Settings could not be read")
                .message(format!(
                    "{}: {reason}. Ferrite is using the defaults and will not save over \
                     the file; fix or remove it, then relaunch.",
                    path.display()
                ))
                .with_type(NotificationType::Warning)
                .autohide(false),
            cx,
        );
    }

    /// Every change to the settings goes through here: saved at once
    /// (unless the file on disk could not be read, which is kept as it
    /// is), the Session defaults the spawner reads refreshed, and the facts
    /// that depend on a setting re-derived.
    pub(super) fn change_settings(
        &mut self,
        change: impl FnOnce(&mut ferrite_core::settings::Settings),
        cx: &mut Context<Self>,
    ) {
        change(&mut self.prefs.settings);
        if self.prefs.unreadable.is_none() {
            if let Err(e) = self.prefs.settings.save(&self.prefs.dir) {
                self.nav_notice.problem(format!("settings not saved: {e}"));
            }
        }
        // Saved first: a Server rereads the file.
        self.cockpit.update(cx, |cockpit, _| cockpit.settings_changed(&self.prefs.settings));
        self.machines_changed(cx);
        if let Ok(mut defaults) = self.prefs.defaults.lock() {
            *defaults = ferrite_core::settings::SessionDefaults::from_settings(&self.prefs.settings);
        }
        ferrite_pane::visual::diagram::set_enabled(
            self.prefs.settings.visuals != ferrite_core::visual::Level::Off,
            cx,
        );
        if self.facts.set_auto_title(self.prefs.settings.auto_title) {
            self.parked_changed(cx);
            for thread in self.cockpit.read(cx).threads() {
                self.facts.renamed(self.cockpit.read(cx), thread);
            }
            self.refresh_names(cx);
        }
        let reading = self.prefs.settings.reading_size;
        for review in self.panes.iter().filter_map(|pane| pane.review.review()) {
            review.update(cx, |review, cx| review.set_reading(reading, cx));
        }
        cx.notify();
    }

    /// Ask the system for a sound file, and play it from now on.
    pub(super) fn choose_sound_file(&mut self, sound: Sound, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose sound".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(file) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |view, cx| {
                view.change_settings(|s| s.set_sound_file(sound, Some(file)), cx);
            });
        })
        .detach();
    }

    /// cmd-= / cmd-- step every transcript's text size through
    /// `ReadingSize::STEPS` (12–24px), stopping at either end, and cmd-0
    /// returns it to 14: the same saved setting as Settings › Behaviour ›
    /// Text size. `delta` 0 resets.
    pub(super) fn step_text_size(&mut self, delta: i32, cx: &mut Context<Self>) {
        use ferrite_core::settings::ReadingSize;
        let now = self.prefs.settings.reading_size;
        let next = if delta == 0 {
            ReadingSize::STANDARD
        } else {
            now.step(delta)
        };
        if next != now {
            self.change_settings(|settings| settings.reading_size = next, cx);
            cx.notify();
        }
    }
}
