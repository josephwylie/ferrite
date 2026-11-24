//! What every window shares, as a window holds it (ADR 0025): read-only
//! copies of the Server's registry, Notices and providers, and the open
//! Threads, each replaced whole by its latest message. An older Server's
//! Groups are not read.

use std::path::{Path, PathBuf};

use ferrite_core::notifications::Notifications;
use ferrite_core::protocol::{ProviderState, ServerMessage, Snapshot};
use ferrite_core::provider::Provider;
use ferrite_core::transcript::RateLimits;
use ferrite_core::workspace::registry::Registry;
use ferrite_core::{ModelInfo, ThreadId};

pub(crate) struct Shared {
    /// The Store these are of.
    dir: PathBuf,
    pub(crate) registry: Registry,
    pub(crate) notifications: Notifications,
    providers: Vec<ProviderState>,
    /// The open Threads, in the order they opened.
    pub(crate) open: Vec<ThreadId>,
    /// A provider's models changed since the window last asked.
    models_changed: bool,
    /// Every Factory's Tasks (ADR 0037), and whether they changed since
    /// the window last asked.
    pub(crate) factory: ferrite_core::factory::Board,
    factory_changed: bool,
}

impl Shared {
    /// Nothing yet: the shared state of the Store in `dir` before the
    /// Engine's first messages.
    pub(crate) fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            registry: Registry::empty(dir),
            notifications: Notifications::default(),
            providers: Vec::new(),
            open: Vec::new(),
            models_changed: false,
            factory: Default::default(),
            factory_changed: false,
        }
    }

    /// All of it afresh, as a Server's snapshot says it.
    pub(crate) fn attached(&mut self, snapshot: &Snapshot) {
        *self = Self {
            open: snapshot.roster.open.clone(),
            ..Self::new(&self.dir)
        };
        if let Some(registry) = &snapshot.registry {
            self.set_registry(registry);
        }
        if let Some(notices) = &snapshot.notices {
            self.notifications = Notifications::from_wire(notices);
        }
        if let Some(providers) = &snapshot.providers {
            self.providers = providers.clone();
            self.models_changed = true;
        }
        self.factory = snapshot.factory.clone().unwrap_or_default();
        self.factory_changed = true;
    }

    pub(crate) fn receive(&mut self, message: &ServerMessage) {
        match message {
            ServerMessage::Roster { roster, .. } => {
                self.open = roster.open.clone();
            }
            ServerMessage::Registry { registry, .. } => self.set_registry(registry),
            ServerMessage::Notices { notices, .. } => {
                self.notifications = Notifications::from_wire(notices);
            }
            ServerMessage::Factory { board, .. } => {
                // Told only the board, the Archive is the one it has.
                let archive = match &board.archive {
                    Some(_) => None,
                    None => self.factory.archive.take(),
                };
                self.factory = board.clone();
                if archive.is_some() {
                    self.factory.archive = archive;
                }
                self.factory_changed = true;
            }
            ServerMessage::Providers { providers, .. } => {
                let models = |providers: &[ProviderState]| -> Vec<Vec<ModelInfo>> {
                    providers.iter().map(|state| state.models.clone()).collect()
                };
                self.models_changed |= models(&self.providers) != models(providers);
                self.providers = providers.clone();
            }
            _ => {}
        }
    }

    fn set_registry(&mut self, registry: &serde_json::Value) {
        match Registry::from_wire(registry.clone(), &self.dir) {
            Ok(registry) => self.registry = registry,
            Err(error) => ferrite_core::log!("the server's registry could not be read: {error}"),
        }
    }

    fn provider(&self, provider: Provider) -> Option<&ProviderState> {
        self.providers
            .iter()
            .find(|state| state.provider == provider)
    }

    pub(crate) fn live_sessions(&self, provider: Provider) -> usize {
        self.provider(provider)
            .map_or(0, |state| state.live_sessions)
    }

    pub(crate) fn announced_models(&self, provider: Provider) -> Vec<ModelInfo> {
        self.provider(provider)
            .map(|state| state.models.clone())
            .unwrap_or_default()
    }

    pub(crate) fn account_limits(&self, provider: Provider) -> RateLimits {
        self.provider(provider)
            .and_then(|state| state.limits)
            .map(Into::into)
            .unwrap_or_default()
    }

    pub(crate) fn take_models_changed(&mut self) -> bool {
        std::mem::take(&mut self.models_changed)
    }

    pub(crate) fn take_factory_changed(&mut self) -> bool {
        std::mem::take(&mut self.factory_changed)
    }
}
