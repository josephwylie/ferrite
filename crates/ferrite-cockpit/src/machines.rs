//! The machines a window holds (ADR 0026, ADR 0045): this computer, each
//! machine saved in Settings, and the one `--remote` named for this run.
//! The window holds every enabled one at once, each its own machine in
//! the one Cockpit: it attaches to each as it starts or is turned on, and
//! lets each go as it is turned off or removed. A machine's bridge lives
//! for the run, so turning it back on finds its ssh master still up.

use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ferrite_client::cockpit::{Attach, Cockpit, Endpoint, ServerState};
use ferrite_core::machine::{self, Bridge, Machine};
use ferrite_core::protocol::Peer;
use ferrite_core::settings::SessionDefaults;
use ferrite_core::spawner;
use ferrite_core::visual::Renderer;
use ferrite_core::{MachineId, MachineKey};

/// What the nav calls this computer.
pub const LOCAL: &str = "local";

/// What the window needs to attach to any of its machines.
pub struct Reach {
    /// `~/.ferrite`: where each machine keeps its bridge.
    pub settings_dir: PathBuf,
    /// This computer's Store, and its Server's executable.
    pub store: PathBuf,
    pub executable: PathBuf,
    pub client: Peer,
    pub renderer: Option<Arc<dyn Renderer>>,
    /// The window takes the API's `window.*` actions: a `ferrite
    /// --headless` one (ADR 0038).
    pub driven: bool,
    pub defaults: Arc<Mutex<SessionDefaults>>,
    /// How a machine is reached: `Bridge::ssh`, or a test's fake.
    pub bridge: fn(&Machine, &std::path::Path) -> Bridge,
}

pub struct Machines {
    reach: Reach,
    /// This computer first, then the machines in Settings' order, then an
    /// unsaved `--remote` one.
    entries: Vec<Entry>,
    local_enabled: bool,
}

struct Entry {
    /// `None`: this computer.
    machine: Option<Machine>,
    /// Made the first time the machine is reached.
    bridge: Option<Arc<Bridge>>,
    /// The machine as the Cockpit holds it, while it is enabled.
    held: Option<MachineId>,
    /// The `--remote` machine, not in Settings.
    unsaved: bool,
}

/// A machine's connection, as Settings › Connections says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    Online,
    Connecting,
    /// Trying again will not help until the operator acts.
    Attention,
    Disabled,
}

impl Connection {
    fn of(state: Option<&ServerState>) -> Self {
        match state {
            // An Engine in this process is always there.
            None | Some(ServerState::Attached { .. }) => Self::Online,
            Some(ServerState::Detached {
                attention: Some(_), ..
            }) => Self::Attention,
            Some(ServerState::Detached { .. }) => Self::Connecting,
        }
    }
}

/// One machine, as Settings › Connections lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineRow {
    pub index: usize,
    /// The machine as the Cockpit holds it; `None` while it is off.
    pub held: Option<MachineId>,
    pub name: String,
    pub connection: Connection,
}

impl Machines {
    /// This computer — unless it is turned off in Settings — `saved`, and
    /// `remote` (a target, or a saved machine's name or target), which
    /// the window holds for this run though it is not saved. Nothing is
    /// attached until `cockpit`.
    pub fn new(reach: Reach, saved: &[Machine], remote: Option<&str>, local_enabled: bool) -> Self {
        let mut machines = Self {
            reach,
            entries: vec![Entry::new(None, false)],
            local_enabled,
        };
        machines.follow(saved);
        if let Some(remote) = remote {
            if machines.named(remote).is_none() {
                machines
                    .entries
                    .push(Entry::new(Some(Machine::new(remote)), true));
            }
        }
        machines
    }

    /// The window's Cockpit, holding every enabled machine. This
    /// computer's Store that cannot be read is an error; a machine is
    /// attached to in the background, so one that is not there holds only
    /// its own rows.
    pub fn cockpit(&mut self) -> io::Result<Cockpit> {
        let mut cockpit = Cockpit::empty();
        if !self.entries[0].disabled(self.local_enabled) {
            self.entries[0].held = Some(cockpit.hold(self.attach(0))?);
        }
        self.sync(&mut cockpit);
        Ok(cockpit)
    }

    /// Take as its own each machine `cockpit` already holds, by the name
    /// it is saved as, and attach every other enabled one beside them: a
    /// view test's window around an Engine of its own.
    #[cfg(test)]
    pub fn beside(&mut self, cockpit: &mut Cockpit) {
        for entry in &mut self.entries {
            entry.held = cockpit.machine_saved_as(&entry.key());
        }
        self.sync(cockpit);
    }

    /// Whether there is more than this computer to list.
    pub fn listed(&self) -> bool {
        self.entries.len() > 1
    }

    /// Machine `index`; `None` for this computer.
    pub fn machine(&self, index: usize) -> Option<&Machine> {
        self.entries.get(index)?.machine.as_ref()
    }

    /// Machine `index` as the Cockpit holds it, while it is enabled.
    pub fn held(&self, index: usize) -> Option<MachineId> {
        self.entries.get(index)?.held
    }

    /// Where the machine the Cockpit holds as `machine` is listed.
    pub fn listed_as(&self, machine: MachineId) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.held == Some(machine))
    }

    /// What the nav and the Inbox call the machine the Cockpit holds as
    /// `machine`.
    pub fn name(&self, machine: MachineId) -> Option<String> {
        Some(self.entries[self.listed_as(machine)?].name())
    }

    pub fn server_state(&self, index: usize, cockpit: &Cockpit) -> Option<ServerState> {
        cockpit.server(self.held(index)?)
    }

    pub fn retry(&self, index: usize, cockpit: &mut Cockpit) {
        if let Some(machine) = self.held(index) {
            cockpit.retry_server(machine);
        }
    }

    /// Turn this computer on or off, attaching to it or letting it go.
    pub fn set_local_enabled(&mut self, enabled: bool, cockpit: &mut Cockpit) {
        self.local_enabled = enabled;
        self.sync(cockpit);
    }

    /// Where the machine `name` (its name or its target) is listed now;
    /// the empty name is this computer's.
    pub fn named(&self, name: &str) -> Option<usize> {
        if name.is_empty() {
            return Some(0);
        }
        self.entries.iter().position(|entry| {
            entry
                .machine
                .as_ref()
                .is_some_and(|machine| machine.target == name || machine.name == name)
        })
    }

    /// Where the machine `key` names is listed now.
    pub fn index(&self, key: &MachineKey) -> Option<usize> {
        self.entries.iter().position(|entry| entry.key() == *key)
    }

    /// The `--remote` machine, while it is not saved.
    pub fn unsaved(&self) -> Option<&Machine> {
        self.entries
            .iter()
            .find(|entry| entry.unsaved)
            .and_then(|entry| entry.machine.as_ref())
    }

    /// Forget the unsaved `--remote` machine `target`: the window lets it
    /// go. False when there is none.
    pub fn forget_unsaved(&mut self, target: &str, cockpit: &mut Cockpit) -> bool {
        let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.unsaved && entry.target() == Some(target))
        else {
            return false;
        };
        if let Some(machine) = self.entries.remove(index).held {
            cockpit.let_go(machine);
        }
        true
    }

    /// Every machine for Settings: this computer only while it is on.
    pub fn rows(&self, cockpit: &Cockpit) -> Vec<MachineRow> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != 0 || self.local_enabled)
            .map(|(index, entry)| {
                let connection = match entry.held {
                    None => Connection::Disabled,
                    Some(machine) => Connection::of(cockpit.server(machine).as_ref()),
                };
                MachineRow {
                    index,
                    held: entry.held,
                    name: entry.name(),
                    connection,
                }
            })
            .collect()
    }

    /// Sign in to machine `index` in a terminal; its link attaches once
    /// the master is up.
    pub fn sign_in(&mut self, index: usize) -> io::Result<()> {
        match self.bridge(index) {
            Some(bridge) => bridge.sign_in(),
            None => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "this computer needs no sign-in",
            )),
        }
    }

    /// Follow Settings' machines — added, removed, renamed, enabled or
    /// not — attaching to each one turned on and letting go of each one
    /// turned off or removed. An unsaved command-line machine stays
    /// unsaved.
    pub fn settings_changed(&mut self, saved: &[Machine], cockpit: &mut Cockpit) {
        let gone = self.follow(saved);
        for machine in gone {
            cockpit.let_go(machine);
        }
        self.sync(cockpit);
    }

    /// Settings' machines in their order, keeping what each entry already
    /// had (its bridge, its hold, its fold): what the entries removed
    /// held, for the Cockpit to let go of.
    fn follow(&mut self, saved: &[Machine]) -> Vec<MachineId> {
        let mut old: Vec<Entry> = self.entries.drain(1..).collect();
        let mut take = |target: &str| {
            old.iter()
                .position(|entry| entry.target() == Some(target))
                .map(|at| old.remove(at))
        };
        let mut entries: Vec<Entry> = saved
            .iter()
            .filter(|machine| machine::check_target(&machine.target).is_ok())
            .map(|machine| {
                let mut entry = take(&machine.target).unwrap_or_else(|| Entry::new(None, false));
                entry.machine = Some(machine.clone());
                entry.unsaved = false;
                entry
            })
            .collect();
        let (unsaved, removed): (Vec<Entry>, Vec<Entry>) =
            old.into_iter().partition(|entry| entry.unsaved);
        entries.extend(unsaved);
        self.entries.extend(entries);
        removed.into_iter().filter_map(|entry| entry.held).collect()
    }

    /// Hold every enabled machine and none other.
    fn sync(&mut self, cockpit: &mut Cockpit) {
        for index in 0..self.entries.len() {
            let wanted = !self.entries[index].disabled(self.local_enabled);
            match (wanted, self.entries[index].held) {
                (true, None) => {
                    let attach = self.attach(index);
                    match cockpit.hold(attach) {
                        Ok(machine) => self.entries[index].held = Some(machine),
                        Err(error) => ferrite_core::log!(
                            "the window cannot hold {}: {error}",
                            self.entries[index].name()
                        ),
                    }
                }
                (false, Some(machine)) => {
                    cockpit.let_go(machine);
                    self.entries[index].held = None;
                }
                _ => {}
            }
        }
    }

    fn attach(&mut self, index: usize) -> Attach {
        let (store, server) = self.endpoint(index);
        Attach {
            browser: cfg!(feature = "cef"),
            store,
            server,
            saved_as: self.entries[index].key(),
            client: self.reach.client.clone(),
            renderer: self.reach.renderer.clone(),
            driven: self.drives(index),
            discovery: Box::new(spawner::Spawn::new(self.reach.defaults.clone(), None)),
        }
    }

    /// Whether machine `index`'s Server hands this window its `window.*`
    /// actions: only the first one enabled, so a paired machine's API
    /// never reaches a headless window on another computer.
    fn drives(&self, index: usize) -> bool {
        self.reach.driven
            && self
                .entries
                .iter()
                .position(|entry| !entry.disabled(self.local_enabled))
                == Some(index)
    }

    /// Where machine `index`'s Server is, and the Store path its holder is
    /// given (`Attach::store`).
    fn endpoint(&mut self, index: usize) -> (PathBuf, Endpoint) {
        match self.bridge(index) {
            Some(bridge) => {
                let target = &self.entries[index]
                    .machine
                    .as_ref()
                    .expect("a machine")
                    .target;
                let store = machine::dir(&self.reach.settings_dir, target).join("threads");
                (store, Endpoint::Machine(bridge))
            }
            None => (
                self.reach.store.clone(),
                Endpoint::Local {
                    executable: self.reach.executable.clone(),
                },
            ),
        }
    }

    fn bridge(&mut self, index: usize) -> Option<Arc<Bridge>> {
        let (settings_dir, reach) = (&self.reach.settings_dir, self.reach.bridge);
        let entry = self.entries.get_mut(index)?;
        let machine = entry.machine.as_ref()?;
        Some(Arc::clone(entry.bridge.get_or_insert_with(|| {
            Arc::new(reach(machine, settings_dir))
        })))
    }
}

impl Entry {
    fn new(machine: Option<Machine>, unsaved: bool) -> Self {
        Self {
            machine,
            bridge: None,
            held: None,
            unsaved,
        }
    }

    fn key(&self) -> MachineKey {
        MachineKey(self.target().map(str::to_string))
    }

    fn target(&self) -> Option<&str> {
        self.machine.as_ref().map(|machine| machine.target.as_str())
    }

    fn disabled(&self, local_enabled: bool) -> bool {
        self.machine
            .as_ref()
            .map_or(!local_enabled, |machine| !machine.enabled)
    }

    fn name(&self) -> String {
        match &self.machine {
            Some(machine) => machine.name.clone(),
            None => LOCAL.into(),
        }
    }
}

#[cfg(test)]
mod tests;
