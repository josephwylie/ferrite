//! The machines a window can be on (ADR 0026): this computer, each machine
//! saved in Settings, and the one `--remote` named for this run. The window
//! is on one at a time — its Cockpit watches that machine's Threads — and
//! every other enabled machine streams only status (`ServerStatus`) for the
//! nav. A machine's bridge lives for the run, so going back to it finds its
//! ssh master still up.

use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use ferrite_core::cockpit::{Attach, Cockpit, Endpoint, ServerState, ServerStatus, StatusRow};
use ferrite_core::machine::{self, Bridge, Machine};
use ferrite_core::protocol::Peer;
use ferrite_core::spawner::{self, SessionDefaults};
use ferrite_core::visual::Renderer;

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
    pub defaults: Arc<Mutex<SessionDefaults>>,
    /// How a machine is reached: `Bridge::ssh`, or a test's fake.
    pub bridge: fn(&Machine, &std::path::Path) -> Bridge,
}

pub struct Machines {
    reach: Reach,
    /// This computer first, then the machines in Settings' order, then an
    /// unsaved `--remote` one.
    entries: Vec<Entry>,
    /// The one the window is on.
    selected: usize,
}

struct Entry {
    /// `None`: this computer.
    machine: Option<Machine>,
    /// Made the first time the machine is reached.
    bridge: Option<Arc<Bridge>>,
    /// Off screen and enabled: what the nav lists.
    status: Option<ServerStatus>,
    /// The `--remote` machine, not in Settings.
    unsaved: bool,
    /// The nav shows its heading alone.
    folded: bool,
}

/// A machine's connection, as its glyph in the nav says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    Online,
    Connecting,
    /// Trying again will not help until the operator acts.
    Attention,
    Disabled,
}

impl Connection {
    fn of(state: &ServerState) -> Self {
        match state {
            ServerState::Attached { .. } => Self::Online,
            ServerState::Detached {
                attention: Some(_), ..
            } => Self::Attention,
            ServerState::Detached { .. } => Self::Connecting,
        }
    }
}

/// One machine, as the nav draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineRow {
    pub index: usize,
    pub name: String,
    pub connection: Connection,
    pub selected: bool,
    pub folded: bool,
    /// What it last said it was doing: only for a machine off screen (the
    /// one on screen is the tree below its heading). A machine that went
    /// keeps its rows, drawn dimmed.
    pub threads: Vec<StatusRow>,
}

impl Machines {
    /// This computer, `saved`, and `remote` (a target, or a saved machine's
    /// name or target), which the window starts on — unless it is turned
    /// off in Settings: then on this computer, the machine listed as off.
    pub fn new(reach: Reach, saved: &[Machine], remote: Option<&str>) -> Self {
        let mut machines = Self {
            reach,
            entries: vec![Entry::new(None, false)],
            selected: 0,
        };
        machines.settings_changed(saved);
        if let Some(remote) = remote {
            let known = machines.entries.iter().position(|entry| {
                entry
                    .machine
                    .as_ref()
                    .is_some_and(|machine| machine.target == remote || machine.name == remote)
            });
            let index = known.unwrap_or_else(|| {
                machines
                    .entries
                    .push(Entry::new(Some(Machine::new(remote)), true));
                machines.entries.len() - 1
            });
            machines.select(index);
        }
        machines
    }

    /// Whether there is more than this computer to list.
    pub fn listed(&self) -> bool {
        self.entries.len() > 1
    }

    /// The machine the window is on; `None` for this computer.
    #[cfg(test)]
    pub fn selected(&self) -> Option<&Machine> {
        self.entries[self.selected].machine.as_ref()
    }

    /// Machine `index`; `None` for this computer.
    pub fn machine(&self, index: usize) -> Option<&Machine> {
        self.entries.get(index)?.machine.as_ref()
    }

    /// The `--remote` machine, while it is not saved.
    pub fn unsaved(&self) -> Option<&Machine> {
        self.entries
            .iter()
            .find(|entry| entry.unsaved)
            .and_then(|entry| entry.machine.as_ref())
    }

    /// A window's Cockpit on the selected machine.
    pub fn cockpit(&mut self) -> io::Result<Cockpit> {
        self.attach(self.selected)
    }

    /// Put the window on another machine: a Cockpit attached there. `None`
    /// when it already is, or the machine is disabled; the window stays
    /// where it is when the Cockpit cannot be built.
    pub fn go_to(&mut self, index: usize) -> Option<io::Result<Cockpit>> {
        if !self.selectable(index) {
            return None;
        }
        let cockpit = self.attach(index);
        if cockpit.is_ok() {
            self.select(index);
        }
        Some(cockpit)
    }

    /// Put the window on another machine. False when it already is, or
    /// the machine is disabled.
    pub fn select(&mut self, index: usize) -> bool {
        if !self.selectable(index) {
            return false;
        }
        self.selected = index;
        self.sync_status();
        true
    }

    /// Take what the machines off screen said. True when a row changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        for status in self
            .entries
            .iter_mut()
            .filter_map(|entry| entry.status.as_mut())
        {
            changed |= status.poll();
        }
        changed
    }

    /// Every machine for the nav; `shown` is the window's own Server.
    pub fn rows(&self, shown: Option<ServerState>) -> Vec<MachineRow> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let selected = index == self.selected;
                let connection = if entry.disabled() {
                    Connection::Disabled
                } else if selected {
                    shown.as_ref().map_or(Connection::Online, Connection::of)
                } else {
                    entry
                        .status
                        .as_ref()
                        .map_or(Connection::Connecting, |status| {
                            Connection::of(&status.server())
                        })
                };
                MachineRow {
                    index,
                    name: entry.name(),
                    connection,
                    selected,
                    folded: entry.folded,
                    threads: entry
                        .status
                        .as_ref()
                        .map(ServerStatus::rows)
                        .unwrap_or_default(),
                }
            })
            .collect()
    }

    pub fn toggle_fold(&mut self, index: usize) {
        if let Some(entry) = self.entries.get_mut(index) {
            entry.folded = !entry.folded;
        }
    }

    /// Sign in to a machine in a terminal; its links attach once the
    /// master is up.
    pub fn sign_in(&mut self, index: usize) -> io::Result<()> {
        match self.bridge(index) {
            Some(bridge) => bridge.sign_in(),
            None => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "this computer needs no sign-in",
            )),
        }
    }

    /// Follow Settings' machines: added, removed, renamed, enabled or not.
    /// A machine keeps its bridge and status across the change; the one
    /// the window is on stays, unsaved, when Settings drops it. True when
    /// Settings turned off the one the window is on: the window leaves it
    /// for this computer.
    pub fn settings_changed(&mut self, saved: &[Machine]) -> bool {
        let selected = self.entries[self.selected].machine.clone();
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
        if let Some(machine) = &selected {
            if !entries
                .iter()
                .any(|entry| entry.target() == Some(&machine.target))
            {
                let mut entry = take(&machine.target).unwrap_or_else(|| Entry::new(None, true));
                entry.machine = Some(machine.clone());
                entry.unsaved = true;
                entries.push(entry);
            }
        }
        self.entries.extend(entries);
        self.selected = selected
            .and_then(|machine| {
                self.entries
                    .iter()
                    .position(|entry| entry.target() == Some(&machine.target))
            })
            .unwrap_or(0);
        self.sync_status();
        self.entries[self.selected].disabled()
    }

    /// Where machine `index`'s Server is, and the Store its window reads.
    fn selectable(&self, index: usize) -> bool {
        index != self.selected && !self.entries.get(index).is_none_or(Entry::disabled)
    }

    fn attach(&mut self, index: usize) -> io::Result<Cockpit> {
        let (store, server) = self.endpoint(index);
        Cockpit::attach(Attach {
            store,
            server,
            client: self.reach.client.clone(),
            renderer: self.reach.renderer.clone(),
            discovery: Box::new(spawner::Spawn::new(self.reach.defaults.clone(), None)),
        })
    }

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

    /// A status link for each enabled machine off screen, and none for the
    /// one on screen, which its Cockpit watches.
    fn sync_status(&mut self) {
        for index in 0..self.entries.len() {
            let wanted = index != self.selected && !self.entries[index].disabled();
            match (wanted, self.entries[index].status.is_some()) {
                (true, false) => {
                    let (store, server) = self.endpoint(index);
                    self.entries[index].status =
                        Some(ServerStatus::new(store, server, self.reach.client.clone()));
                }
                (false, true) => self.entries[index].status = None,
                _ => {}
            }
        }
    }
}

impl Entry {
    fn new(machine: Option<Machine>, unsaved: bool) -> Self {
        Self {
            machine,
            bridge: None,
            status: None,
            unsaved,
            folded: false,
        }
    }

    fn target(&self) -> Option<&str> {
        self.machine.as_ref().map(|machine| machine.target.as_str())
    }

    fn disabled(&self) -> bool {
        self.machine
            .as_ref()
            .is_some_and(|machine| !machine.enabled)
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
