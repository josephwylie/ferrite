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
            cli_versions: self.cli_versions.clone(),
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
        if self.cli_versions.is_none() {
            self.probe_cli_versions(cx);
        }
        self.ask_server_defaults(cx);
        view
    }
}
