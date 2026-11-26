//! Connections in the window (ADR 0041). The page is
//! `ferrite_settings::connections::ConnectionsView`, which the Settings
//! sheet holds. It reads the environments through a `Source` the root
//! makes over itself (`connections_inputs`); what would change the saved
//! environments it says as a `ConnectionsEvent`, which the root answers
//! here: saving them, letting a machine go or taking one up, and what the
//! drafts on it do.

use super::*;
use crate::machines::Connection;
use ferrite_client::cockpit::ServerState;
use ferrite_core::machine::{self, remote, Machine};
use ferrite_settings::connections::{
    connection_exists, ConnectionsEvent, ConnectionsInputs, ConnectionsView, Listed, Reached, ALREADY_SAVED,
};
use gpui::App;

impl CockpitView {
    /// What Connections draws from. It never reads the view: the view
    /// reads it, from its own listeners and as the sheet draws.
    pub(super) fn connections_inputs(&self, cx: &App) -> ConnectionsInputs {
        let cockpit = self.cockpit.read(cx);
        let editing = self.settings_machine(cx);
        let listed = self
            .machines
            .as_ref()
            .map(|machines| {
                machines
                    .rows(cockpit)
                    .into_iter()
                    .filter_map(|row| {
                        let machine = machines.machine(row.index)?.clone();
                        let state = machines.server_state(row.index, cockpit);
                        let unsaved = machines
                            .unsaved()
                            .is_some_and(|unsaved| unsaved.target == machine.target);
                        let sync = self.setup_sync.read(cx).status(&machine.target).map(|status| status.row(&machine.name));
                        Some(Listed {
                            editing: row.held.is_some() && row.held == editing,
                            index: row.index,
                            reached: match (row.connection, state) {
                                (Connection::Disabled, _) => Reached::Off,
                                (_, Some(ServerState::Detached { reason, attention })) => {
                                    Reached::Detached { reason, attention }
                                }
                                _ => Reached::Online,
                            },
                            machine,
                            unsaved,
                            sync,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        ConnectionsInputs {
            dir: self.prefs.dir.clone(),
            saved: self.prefs.settings.machines.clone(),
            local_enabled: self.prefs.settings.local_environment_enabled,
            switchable: self.machines.is_some(),
            listed,
        }
    }

    /// Connections, while Settings is up.
    pub(crate) fn connections(&self, cx: &App) -> Option<Entity<ConnectionsView>> {
        Some(self.settings_sheet(cx)?.read(cx).connections().clone())
    }

    pub(super) fn on_connections_event(
        &mut self,
        view: Entity<ConnectionsView>,
        event: &ConnectionsEvent,
        cx: &mut Context<Self>,
    ) {
        match event.clone() {
            ConnectionsEvent::Open(index) => {
                let held = self.machines.as_ref().and_then(|machines| machines.held(index));
                if let Some(machine) = held {
                    self.edit_machine_settings(machine, cx);
                }
            }
            ConnectionsEvent::ToggleLocal => {
                let error = self.toggle_local_environment(cx).err();
                view.update(cx, |view, cx| view.answered(error, cx));
            }
            ConnectionsEvent::Toggle(target) => {
                let away = self.drafts_on(&target, cx);
                let saved = self.save_connections(
                    |saved| {
                        for machine in saved.iter_mut().filter(|machine| machine.target == target) {
                            machine.enabled = !machine.enabled;
                        }
                    },
                    cx,
                );
                let put_away = saved.is_ok() && away > 0 && self.drafts_on(&target, cx) == 0;
                view.update(cx, |view, cx| view.answered(saved.err(), cx));
                if let Some(name) = put_away.then(|| self.saved_name(&target)).flatten() {
                    let drafts = if away == 1 { "Your draft" } else { "Your drafts" };
                    let are = if away == 1 { "is" } else { "are" };
                    let warning = format!("{drafts} on {name} {are} put away until {name} is on again.");
                    view.update(cx, |view, cx| view.warn(warning, cx));
                }
            }
            ConnectionsEvent::ToggleSync(target) => {
                let saved = self.save_connections(
                    |saved| {
                        for machine in saved.iter_mut().filter(|machine| machine.target == target) {
                            machine.sync_setup = !machine.sync_setup;
                        }
                    },
                    cx,
                );
                self.tick_setup_sync(cx);
                view.update(cx, |view, cx| view.answered(saved.err(), cx));
            }
            ConnectionsEvent::SyncAnyway(target) => self.sync_setup_anyway(&target, cx),
            ConnectionsEvent::UseSignIns(target, side) => self.sync_setup_sign_ins(&target, side, cx),
            ConnectionsEvent::Remove { machine, unsaved } => {
                self.remove_connection(&view, machine, unsaved, cx)
            }
            ConnectionsEvent::ProjectsDir { target, dir } => {
                let saved = self.save_connections(
                    |saved| {
                        for machine in saved.iter_mut().filter(|machine| machine.target == target) {
                            machine.projects_dir = dir.clone();
                        }
                    },
                    cx,
                );
                view.update(cx, |view, cx| view.answered(saved.err(), cx));
            }
            ConnectionsEvent::Ready { machine, replacing } => {
                self.add_connection(&view, machine, replacing, cx)
            }
        }
        cx.notify();
    }

    /// How many drafts are open on the environment `target` names.
    fn drafts_on(&self, target: &str, cx: &gpui::App) -> usize {
        let Some(machine) = self
            .machines
            .as_ref()
            .and_then(|machines| machines.held(machines.named(target)?))
        else {
            return 0;
        };
        self.panes
            .iter()
            .filter_map(|pane| pane.read(cx).draft())
            .filter(|draft| draft.binding.project().machine() == machine)
            .count()
    }

    /// What Settings calls the environment `target` names.
    fn saved_name(&self, target: &str) -> Option<String> {
        self.prefs
            .settings
            .machines
            .iter()
            .find(|machine| machine.target == target)
            .map(|machine| machine.name.clone())
    }

    /// Why the window cannot let the environment `target` names go now
    /// (`files_leave_error`); `None` when it is not held.
    fn leave_error(&self, target: &str, cx: &App) -> Option<&'static str> {
        let machines = self.machines.as_ref()?;
        let held = machines.held(machines.named(target)?)?;
        self.files_leave_error(held, cx)
    }

    /// Connections' own save: unlike `change_settings`, it saves nothing
    /// in memory that it could not write, and refuses to let go of an
    /// environment that has files left unsaved.
    fn save_connections(
        &mut self,
        change: impl FnOnce(&mut Vec<Machine>),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(error) = &self.prefs.unreadable {
            return Err(format!("Settings cannot be saved: {error}"));
        }
        let mut settings = self.prefs.settings.clone();
        change(&mut settings.machines);
        let leaving = self.prefs.settings.machines.iter().filter(|machine| {
            machine.enabled
                && !settings
                    .machines
                    .iter()
                    .any(|kept| kept.target == machine.target && kept.enabled)
        });
        for machine in leaving {
            if let Some(error) = self.leave_error(&machine.target, cx) {
                return Err(error.into());
            }
        }
        settings
            .save(&self.prefs.dir)
            .map_err(|error| format!("Environment not saved: {error}. Try again."))?;
        self.prefs.settings = settings;
        self.cockpit.update(cx, |cockpit, _| cockpit.settings_changed(&self.prefs.settings));
        self.machines_changed(cx);
        cx.notify();
        Ok(())
    }

    fn add_connection(
        &mut self,
        view: &Entity<ConnectionsView>,
        machine: Machine,
        replacing: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if connection_exists(&machine, replacing.as_deref(), &self.prefs.settings.machines) {
            view.update(cx, |view, cx| view.refused(ALREADY_SAVED.into(), cx));
            return;
        }
        let saved = self.save_connections(
            |saved| {
                if let Some(existing) = saved
                    .iter_mut()
                    .find(|saved| replacing.as_ref() == Some(&saved.target))
                {
                    // Set up again, it is the same environment: what the
                    // setup does not ask stays as it was.
                    *existing = Machine {
                        projects_dir: existing.projects_dir.clone(),
                        ..machine.clone()
                    };
                } else {
                    saved.push(machine.clone());
                }
            },
            cx,
        );
        let added = saved.is_ok();
        view.update(cx, |view, cx| view.added(saved, cx));
        if !added {
            return;
        }
        if let Some(old) = replacing.filter(|old| *old != machine.target) {
            let (from, to) = (
                machine::dir(&self.prefs.dir, &old),
                machine::dir(&self.prefs.dir, &machine.target),
            );
            if let Err(error) = ferrite_core::placement::follow(&from, &to) {
                let warning = format!("Where Projects were copied to {} was not kept: {error}", machine.name);
                view.update(cx, |view, cx| view.warn(warning, cx));
            }
        }
        if let Some(machines) = &self.machines {
            if let Some(index) = machines.named(&machine.target) {
                self.cockpit
                    .update(cx, |cockpit, _| machines.retry(index, cockpit));
            }
        }
    }

    fn remove_connection(
        &mut self,
        view: &Entity<ConnectionsView>,
        machine: Machine,
        unsaved: bool,
        cx: &mut Context<Self>,
    ) {
        let target = machine.target.clone();
        if let Some(error) = self.leave_error(&target, cx) {
            view.update(cx, |view, cx| view.answered(Some(error.into()), cx));
            return;
        }
        let saved = self.save_connections(|saved| saved.retain(|saved| saved.target != target), cx);
        let removed = saved.is_ok();
        view.update(cx, |view, cx| view.removed(saved, cx));
        if !removed {
            return;
        }
        // Where the Machine's copies went goes with it: a Machine saved
        // again under that name is another.
        let kept = ferrite_core::machine::dir(&self.prefs.dir, &target);
        if let Err(error) = ferrite_core::placement::forget(&kept) {
            let warning = format!("Environment removed, but where its copies went could not be forgotten: {error}");
            view.update(cx, |view, cx| view.warn(warning, cx));
        }
        if machine.is_link() {
            let directory = self.prefs.dir.clone();
            let view = view.downgrade();
            cx.spawn(async move |_, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { remote::forget(&machine, &directory) })
                    .await;
                if let Err(error) = result {
                    let warning = format!(
                        "Environment removed, but its saved credential could not be deleted: {error}"
                    );
                    let _ = view.update(cx, |view, cx| view.warn(warning, cx));
                }
            })
            .detach();
        }
        if unsaved {
            let before: Vec<MachineId> = self.cockpit.read(cx).machines().collect();
            if let Some(machines) = &mut self.machines {
                self.cockpit
                    .update(cx, |cockpit, _| machines.forget_unsaved(&target, cockpit));
            }
            self.machines_let_go(&before, cx);
        }
    }

    /// Like `save_connections`, it keeps nothing it could not write.
    fn toggle_local_environment(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if let Some(error) = &self.prefs.unreadable {
            return Err(format!("Settings cannot be saved: {error}"));
        }
        let enabled = !self.prefs.settings.local_environment_enabled;
        if !enabled {
            if let Some(error) = self.leave_error("", cx) {
                return Err(error.into());
            }
        }
        let mut settings = self.prefs.settings.clone();
        settings.local_environment_enabled = enabled;
        settings
            .save(&self.prefs.dir)
            .map_err(|error| format!("Local environment not saved: {error}. Try again."))?;
        self.prefs.settings = settings;
        self.machines_changed(cx);
        Ok(())
    }
}
