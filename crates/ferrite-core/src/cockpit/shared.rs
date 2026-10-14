//! What every window shares, as a window holds it (ADR 0025): read-only
//! copies of the Server's Groups, registry, Notices and providers, and the
//! open and parked Threads, each replaced whole by its latest message.

use crate::groups::Groups;
use crate::notifications::Notifications;
use crate::protocol::{ProviderState, ServerMessage};
use crate::store::Provider;
use crate::transcript::RateLimits;
use crate::workspace::registry::Registry;
use crate::{ModelInfo, ThreadId};

#[derive(Default)]
pub(crate) struct Shared {
    pub(crate) groups: Groups,
    pub(crate) registry: Registry,
    pub(crate) notifications: Notifications,
    providers: Vec<ProviderState>,
    /// The open Threads, in the order they opened.
    pub(crate) open: Vec<ThreadId>,
    /// The Threads that are not open.
    pub(crate) parked: Vec<ThreadId>,
    /// A provider's models changed since the window last asked.
    models_changed: bool,
}

impl Shared {
    pub(crate) fn receive(&mut self, message: &ServerMessage) {
        match message {
            ServerMessage::Roster { roster, .. } => {
                self.open = roster.open.clone();
                self.parked = roster.parked.clone();
            }
            ServerMessage::Groups { groups, .. } => self.set_groups(groups),
            ServerMessage::Registry { registry, .. } => self.set_registry(registry),
            ServerMessage::Notices { notices, .. } => {
                self.notifications = Notifications::from_wire(notices);
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

    /// A copy the Server sent that this build cannot read keeps the one
    /// before: a window older than its Server says so in its log.
    fn set_groups(&mut self, groups: &serde_json::Value) {
        match Groups::from_wire(groups.clone()) {
            Ok(groups) => self.groups = groups,
            Err(error) => crate::log!("the server's groups could not be read: {error}"),
        }
    }

    fn set_registry(&mut self, registry: &serde_json::Value) {
        match Registry::from_wire(registry.clone()) {
            Ok(registry) => self.registry = registry,
            Err(error) => crate::log!("the server's registry could not be read: {error}"),
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
}
