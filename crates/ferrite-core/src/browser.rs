//! Thread-owned website tabs. Windows realize these identities as exact live
//! engine pages; neither providers nor previews own an independent browser.
//! Only URL/order/selection survive restart. Capabilities and live state do not.

mod control;
pub mod tools;
pub(crate) use control::{channel, Requests};
pub use control::{
    Client, Control, Observation, Operation, Outcome, Response, ThreadClient, WaitState,
};

use crate::ThreadId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TabId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub tab: TabId,
    pub generation: u64,
}

/// A lease is valid only while this exact window connection owns the Thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    pub window: u64,
    pub epoch: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dirty {
    Clean,
    Dirty,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tab {
    pub id: TabId,
    pub url: String,
    pub generation: u64,
    pub loading: bool,
    pub can_back: bool,
    pub can_forward: bool,
    pub error: Option<String>,
    pub live: bool,
    pub visible: bool,
    pub dirty: Dirty,
    /// Includes active commands and annotation capture, never a renderer id.
    pub protected: bool,
}

impl Tab {
    pub fn document(&self) -> Document {
        Document {
            tab: self.id,
            generation: self.generation,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub tabs: Vec<Tab>,
    pub active: Option<TabId>,
    pub owner: Option<Owner>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Open { url: String },
    Select { tab: TabId },
    Close { tab: TabId },
    Navigate { document: Document, url: String },
    Back { document: Document },
    Forward { document: Document },
    Reload { document: Document },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protection {
    Command,
    Annotation,
    /// Internal two-phase unloading reservation; never an agent lease.
    Reclaim,
}

/// An opaque protection token. The service checks its owner/document before
/// accepting completion; dropping a connection invalidates every token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageState {
    /// Trusted owning preview's actual painted visibility, not tab selection.
    pub visible: bool,
    /// The native main-frame document changed since the last report.
    pub navigation: bool,
    pub document: Document,
    pub url: String,
    pub loading: bool,
    pub can_back: bool,
    pub can_forward: bool,
    pub error: Option<String>,
    pub dirty: Dirty,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Saved {
    version: u32,
    next: u64,
    threads: BTreeMap<ThreadId, SavedThread>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SavedThread {
    tabs: Vec<(TabId, String, u64)>,
    active: Option<TabId>,
}

#[derive(Clone, Default)]
pub struct Service {
    threads: BTreeMap<ThreadId, State>,
    next: u64,
    epoch: u64,
    next_lease: u64,
    protections: BTreeMap<u64, (ThreadId, Owner, Document, Protection)>,
    /// A terminal native reply proves this retained command no longer writes,
    /// although its effect remains uncertain to the agent.
    finished_uncertain: BTreeSet<u64>,
    manual: BTreeMap<ThreadId, u64>,
    interactions: BTreeMap<ThreadId, (Owner, Document, u64)>,
}

type Result<T> = std::result::Result<T, String>;

impl Service {
    pub fn state(&self, thread: ThreadId) -> State {
        let mut state = self.threads.get(&thread).cloned().unwrap_or_default();
        for tab in &mut state.tabs {
            tab.protected = self.protections.values().any(|(id, _, document, kind)| {
                *id == thread && document.tab == tab.id && *kind != Protection::Reclaim
            }) || self
                .interactions
                .get(&thread)
                .is_some_and(|(_, document, _)| document.tab == tab.id);
        }
        state
    }

    /// The caller verifies that `window` advertises browser capability and is
    /// actually watching this Thread. Ownership never follows recent focus.
    pub fn attach(&mut self, thread: ThreadId, window: u64) -> Result<Owner> {
        let state = self.threads.entry(thread).or_default();
        if let Some(owner) = state.owner {
            return if owner.window == window {
                Ok(owner)
            } else {
                Err("This Thread's browser is already open in another Ferrite window".into())
            };
        }
        self.epoch += 1;
        let owner = Owner {
            window,
            epoch: self.epoch,
        };
        state.owner = Some(owner);
        Ok(owner)
    }

    pub fn detach(&mut self, thread: ThreadId, window: u64) -> bool {
        let Some(state) = self
            .threads
            .get_mut(&thread)
            .filter(|state| state.owner.is_some_and(|owner| owner.window == window))
        else {
            return false;
        };
        state.owner = None;
        for tab in &mut state.tabs {
            tab.live = false;
            tab.visible = false;
            tab.loading = false;
            tab.generation = tab.generation.saturating_add(1);
            tab.dirty = Dirty::Unknown;
        }
        self.protections.retain(|_, (id, _, _, _)| *id != thread);
        self.finished_uncertain
            .retain(|lease| self.protections.contains_key(lease));
        self.manual.remove(&thread);
        self.interactions.remove(&thread);
        true
    }

    pub fn detach_window(&mut self, window: u64) -> Vec<ThreadId> {
        let threads: Vec<_> = self.threads.keys().copied().collect();
        threads
            .into_iter()
            .filter(|thread| self.detach(*thread, window))
            .collect()
    }

    pub fn check_owner(&self, thread: ThreadId, owner: Owner) -> Result<()> {
        if self.state(thread).owner == Some(owner) {
            Ok(())
        } else {
            Err("Conversation browser unavailable: open this Thread in a browser-capable Ferrite window".into())
        }
    }

    pub fn check_document(&self, thread: ThreadId, document: Document) -> Result<()> {
        self.threads
            .get(&thread)
            .and_then(|state| state.tabs.iter().find(|tab| tab.id == document.tab))
            .filter(|tab| tab.generation == document.generation && tab.live)
            .map(|_| ())
            .ok_or_else(|| {
                "The page changed; observe the current document before continuing".into()
            })
    }

    pub fn apply(&mut self, thread: ThreadId, window: u64, action: Action) -> Result<State> {
        let owner = self.state(thread).owner.filter(|owner| owner.window == window)
            .ok_or("Conversation browser unavailable: attach this Thread in a browser-capable Ferrite window")?;
        self.apply_owned(thread, owner, action)
    }

    pub fn apply_owned(&mut self, thread: ThreadId, owner: Owner, action: Action) -> Result<State> {
        self.check_owner(thread, owner)?;
        match &action {
            Action::Open { url } | Action::Navigate { url, .. } => validate_url(url)?,
            _ => {}
        }
        match &action {
            Action::Navigate { document, .. }
            | Action::Back { document }
            | Action::Forward { document }
            | Action::Reload { document } => {
                self.check_document(thread, *document)?;
                if self
                    .protections
                    .values()
                    .any(|(t, _, d, _)| *t == thread && d.tab == document.tab)
                {
                    return Err("This page is in use; finish its command or annotation capture before navigating".into());
                }
            }
            _ => {}
        }
        let state = self.threads.get_mut(&thread).expect("checked owner");
        match action {
            Action::Open { url } => {
                self.next = self.next.checked_add(1).ok_or(
                    "Browser tab identities are exhausted; no identity can safely be reused",
                )?;
                let id = TabId(self.next);
                state.tabs.push(Tab {
                    id,
                    url,
                    generation: 1,
                    loading: true,
                    can_back: false,
                    can_forward: false,
                    error: None,
                    live: true,
                    visible: false,
                    dirty: Dirty::Unknown,
                    protected: false,
                });
                state.active = Some(id);
            }
            Action::Select { tab } => {
                if self.protections.values().any(|(id, _, document, kind)| {
                    *id == thread && document.tab == tab && *kind == Protection::Reclaim
                }) {
                    return Err(
                        "This page is being unloaded; select it again once unloading completes"
                            .into(),
                    );
                }
                let selected = state
                    .tabs
                    .iter_mut()
                    .find(|item| item.id == tab)
                    .ok_or("No such tab in this Thread")?;
                if !selected.live {
                    begin_navigation(selected)?;
                    selected.live = true;
                }
                state.active = Some(tab);
            }
            Action::Close { tab } => {
                if self
                    .protections
                    .values()
                    .any(|(t, _, d, _)| *t == thread && d.tab == tab)
                {
                    return Err("This page is in use; finish its command or annotation capture before closing".into());
                }
                let at = state
                    .tabs
                    .iter()
                    .position(|item| item.id == tab)
                    .ok_or("No such tab in this Thread")?;
                if state.active == Some(tab) {
                    let successor = state
                        .tabs
                        .get(at + 1)
                        .or_else(|| at.checked_sub(1).and_then(|at| state.tabs.get(at)));
                    if let Some(next) = successor.filter(|next| !next.live) {
                        next_generation(next.generation)?;
                    }
                }
                state.tabs.remove(at);
                if state.active == Some(tab) {
                    state.active = state
                        .tabs
                        .get(at.min(state.tabs.len().saturating_sub(1)))
                        .map(|tab| tab.id);
                    if let Some(next) = state
                        .tabs
                        .iter_mut()
                        .find(|tab| Some(tab.id) == state.active)
                    {
                        if !next.live {
                            begin_navigation(next)?;
                            next.live = true;
                        }
                    }
                }
            }
            Action::Navigate { document, url } => {
                let tab = state
                    .tabs
                    .iter_mut()
                    .find(|tab| tab.id == document.tab)
                    .expect("checked document");
                begin_navigation(tab)?;
                tab.url = url;
            }
            Action::Back { document }
            | Action::Forward { document }
            | Action::Reload { document } => {
                let tab = state
                    .tabs
                    .iter_mut()
                    .find(|tab| tab.id == document.tab)
                    .expect("checked document");
                begin_navigation(tab)?;
            }
        }
        self.expire_interaction(thread);
        Ok(self.state(thread))
    }

    /// Navigation/dirty information from the exact window-owned page. An old
    /// callback cannot change a new document or resurrect a reclaimed page.
    pub fn update(&mut self, thread: ThreadId, owner: Owner, page: PageState) -> Result<State> {
        self.check_owner(thread, owner)?;
        self.check_document(thread, page.document)?;
        validate_url(&page.url)?;
        let state = self.threads.get_mut(&thread).expect("checked owner");
        let tab = state
            .tabs
            .iter_mut()
            .find(|tab| tab.id == page.document.tab)
            .expect("checked document");
        if page.navigation {
            tab.generation = next_generation(tab.generation)?;
        }
        tab.url = page.url;
        tab.loading = page.loading;
        tab.visible = page.visible;
        tab.can_back = page.can_back;
        tab.can_forward = page.can_forward;
        tab.error = page.error;
        tab.dirty = page.dirty;
        self.expire_interaction(thread);
        Ok(self.state(thread))
    }

    pub(crate) fn persistence_error(&mut self, thread: ThreadId, error: &str) {
        if let Some(state) = self.threads.get_mut(&thread) {
            if let Some(tab) = state
                .tabs
                .iter_mut()
                .find(|tab| Some(tab.id) == state.active)
            {
                let previous = tab
                    .error
                    .take()
                    .map(|error| format!("{error}. "))
                    .unwrap_or_default();
                tab.error = Some(format!("{previous}Tab URLs could not be saved: {error}. Check available disk space and the store permissions"));
            }
        }
    }

    pub fn protect(
        &mut self,
        thread: ThreadId,
        owner: Owner,
        document: Document,
        kind: Protection,
    ) -> Result<Lease> {
        self.check_owner(thread, owner)?;
        self.check_document(thread, document)?;
        if kind == Protection::Reclaim {
            return Err("Use the page reclamation handshake to reserve an unload".into());
        }
        if self
            .protections
            .iter()
            .any(|(lease, (id, _, _, existing))| {
                *id == thread
                    && !(kind == Protection::Annotation
                        && *existing == Protection::Command
                        && self.finished_uncertain.contains(lease))
            })
        {
            return Err(
                "This Thread's browser is in use; finish its command or annotation capture first"
                    .into(),
            );
        }
        self.next_lease += 1;
        self.protections
            .insert(self.next_lease, (thread, owner, document, kind));
        Ok(Lease(self.next_lease))
    }

    pub fn check_protection(
        &self,
        thread: ThreadId,
        owner: Owner,
        document: Document,
        lease: Lease,
        kind: Protection,
    ) -> Result<()> {
        self.check_owner(thread, owner)?;
        self.check_document(thread, document)?;
        if self.protections.get(&lease.0) == Some(&(thread, owner, document, kind)) {
            Ok(())
        } else {
            Err("This browser capture protection is no longer valid; select the page again".into())
        }
    }

    pub fn release(&mut self, thread: ThreadId, owner: Owner, lease: Lease) -> Result<()> {
        self.check_owner(thread, owner)?;
        match self.protections.get(&lease.0) {
            Some((t, o, _, _)) if *t == thread && *o == owner => {
                self.protections.remove(&lease.0);
                self.finished_uncertain.remove(&lease.0);
                Ok(())
            }
            _ => Err("This browser protection is no longer valid".into()),
        }
    }

    pub(crate) fn command_finished_uncertain(
        &mut self,
        thread: ThreadId,
        owner: Owner,
        lease: Lease,
    ) -> Result<()> {
        self.check_owner(thread, owner)?;
        match self.protections.get(&lease.0) {
            Some((id, candidate, _, Protection::Command))
                if *id == thread && *candidate == owner =>
            {
                self.finished_uncertain.insert(lease.0);
                Ok(())
            }
            _ => Err("This command protection is no longer valid".into()),
        }
    }

    pub(crate) fn check_command_recovery(
        &self,
        thread: ThreadId,
        owner: Owner,
        lease: Lease,
    ) -> Result<()> {
        self.check_owner(thread, owner)?;
        if !matches!(self.protections.get(&lease.0), Some((id, candidate, _, Protection::Command)) if *id == thread && *candidate == owner)
        {
            return Err("This command protection is no longer valid".into());
        }
        if self
            .protections
            .iter()
            .any(|(key, (id, _, _, _))| *id == thread && *key != lease.0)
        {
            return Err(
                "Finish the annotation capture before observing or controlling this browser".into(),
            );
        }
        Ok(())
    }

    /// A trusted operator's explicit Close/Reload may abandon only this tab's
    /// commands after the window fences its native writer. Capture protection
    /// is never discarded by that recovery path.
    pub(crate) fn retire_commands(
        &mut self,
        thread: ThreadId,
        owner: Owner,
        tab: TabId,
    ) -> Result<Vec<Lease>> {
        self.check_owner(thread, owner)?;
        if !self.state(thread).tabs.iter().any(|page| page.id == tab) {
            return Err("No such tab in this Thread".into());
        }
        if self.protections.values().any(|(id, _, document, kind)| {
            *id == thread && document.tab == tab && *kind == Protection::Annotation
        }) {
            return Err(
                "Finish or cancel the annotation capture before closing or reloading this page"
                    .into(),
            );
        }
        let leases: Vec<_> = self
            .protections
            .iter()
            .filter(|(_, (id, candidate, document, kind))| {
                *id == thread
                    && *candidate == owner
                    && document.tab == tab
                    && *kind == Protection::Command
            })
            .map(|(key, _)| Lease(*key))
            .collect();
        for lease in &leases {
            self.release(thread, owner, *lease)?;
        }
        Ok(leases)
    }

    /// Begin/refresh a trusted operator gesture before forwarding its native
    /// input. The ACK revision identifies the latest burst, not just its page.
    pub fn operator_input(
        &mut self,
        thread: ThreadId,
        owner: Owner,
        document: Document,
    ) -> Result<u64> {
        self.check_owner(thread, owner)?;
        self.check_document(thread, document)?;
        if self.state(thread).active != Some(document.tab) {
            return Err("The selected tab changed; this queued input was discarded".into());
        }
        let revision = self.manual.entry(thread).or_default();
        *revision = revision
            .checked_add(1)
            .ok_or("Operator input revision exhausted; reconnect this browser")?;
        let revision = *revision;
        self.interactions
            .insert(thread, (owner, document, revision));
        Ok(revision)
    }

    /// An old key-up, quiet-period callback or cancelled document cannot end a
    /// newer gesture. Stale settlement is harmless and deliberately idempotent.
    pub fn operator_settled(
        &mut self,
        thread: ThreadId,
        owner: Owner,
        document: Document,
        revision: u64,
    ) -> Result<bool> {
        self.check_owner(thread, owner)?;
        if self.interactions.get(&thread) == Some(&(owner, document, revision)) {
            self.interactions.remove(&thread);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn operator_active(&self, thread: ThreadId) -> bool {
        self.interactions.contains_key(&thread)
    }

    fn expire_interaction(&mut self, thread: ThreadId) {
        let invalid = self
            .interactions
            .get(&thread)
            .is_some_and(|(owner, document, _)| {
                self.check_owner(thread, *owner).is_err()
                    || self.check_document(thread, *document).is_err()
                    || self.state(thread).active != Some(document.tab)
            });
        if invalid {
            self.interactions.remove(&thread);
        }
    }

    pub fn operator_revision(&self, thread: ThreadId) -> u64 {
        self.manual.get(&thread).copied().unwrap_or(0)
    }

    /// Policy supplies candidates from measured budgets. Unknown dirty state
    /// is deliberately ineligible, as are visible work and protected captures.
    pub fn reclaim(&mut self, thread: ThreadId, owner: Owner, tab: TabId) -> Result<bool> {
        self.check_owner(thread, owner)?;
        let state = self.threads.get_mut(&thread).expect("checked owner");
        let page = state
            .tabs
            .iter_mut()
            .find(|page| page.id == tab)
            .ok_or("No such tab in this Thread")?;
        if !page.live
            || page.loading
            || page.visible
            || page.dirty != Dirty::Clean
            || self
                .interactions
                .get(&thread)
                .is_some_and(|(_, document, _)| document.tab == tab)
            || self
                .protections
                .values()
                .any(|(t, _, d, _)| *t == thread && d.tab == tab)
        {
            return Ok(false);
        }
        let generation = next_generation(page.generation)?;
        page.live = false;
        page.visible = false;
        page.loading = false;
        page.generation = generation;
        page.dirty = Dirty::Unknown;
        Ok(true)
    }

    /// Reserve an eligible inactive page while the owning window asks its
    /// native view to release. Commands cannot select or use it in between.
    pub fn begin_reclaim(
        &mut self,
        thread: ThreadId,
        owner: Owner,
        document: Document,
    ) -> Result<Option<Lease>> {
        self.check_owner(thread, owner)?;
        self.check_document(thread, document)?;
        let state = self.state(thread);
        let page = state
            .tabs
            .iter()
            .find(|page| page.id == document.tab)
            .expect("checked document");
        if page.visible
            || page.loading
            || page.dirty != Dirty::Clean
            || page.protected
            || self.protections.values().any(|(id, _, _, _)| *id == thread)
        {
            return Ok(None);
        }
        self.next_lease += 1;
        let lease = Lease(self.next_lease);
        self.protections
            .insert(lease.0, (thread, owner, document, Protection::Reclaim));
        Ok(Some(lease))
    }

    /// A failed native release keeps the exact document. Only a successful
    /// acknowledgement changes live identity and invalidates stale callbacks.
    pub fn finish_reclaim(
        &mut self,
        thread: ThreadId,
        owner: Owner,
        lease: Lease,
        released: bool,
    ) -> Result<State> {
        self.check_owner(thread, owner)?;
        let document = match self.protections.get(&lease.0) {
            Some((id, candidate, document, Protection::Reclaim))
                if *id == thread && *candidate == owner =>
            {
                *document
            }
            _ => return Err("This page reclamation reservation is no longer valid".into()),
        };
        self.release(thread, owner, lease)?;
        if released {
            self.check_document(thread, document)?;
        }
        if released && !self.reclaim(thread, owner, document.tab)? {
            return Err(
                "The page changed while unloading; reopen this tab to restore its URL".into(),
            );
        }
        Ok(self.state(thread))
    }

    pub fn save(&self) -> Saved {
        Saved {
            version: 1,
            next: self.next,
            threads: self
                .threads
                .iter()
                .map(|(id, state)| {
                    (
                        *id,
                        SavedThread {
                            tabs: state
                                .tabs
                                .iter()
                                .map(|tab| (tab.id, tab.url.clone(), tab.generation))
                                .collect(),
                            active: state.active,
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn restore(saved: Saved) -> Result<Self> {
        if saved.version != 1 {
            return Err("Browser tabs were saved by an unsupported Ferrite version".into());
        }
        let mut service = Self {
            next: saved.next,
            ..Self::default()
        };
        for (thread, saved) in saved.threads {
            let mut state = State::default();
            for (id, url, generation) in saved.tabs {
                validate_url(&url)?;
                if id.0 == 0
                    || id.0 > service.next
                    || service
                        .threads
                        .values()
                        .any(|state| state.tabs.iter().any(|tab| tab.id == id))
                    || state.tabs.iter().any(|tab| tab.id == id)
                {
                    return Err("Browser tab identities are inconsistent".into());
                }
                state.tabs.push(Tab {
                    id,
                    url,
                    generation: next_generation(generation)?,
                    loading: false,
                    can_back: false,
                    can_forward: false,
                    error: None,
                    live: false,
                    visible: false,
                    dirty: Dirty::Unknown,
                    protected: false,
                });
            }
            if saved
                .active
                .is_some_and(|id| !state.tabs.iter().any(|tab| tab.id == id))
            {
                return Err("Browser selection does not identify a saved tab".into());
            }
            state.active = saved.active;
            service.threads.insert(thread, state);
        }
        Ok(service)
    }
}

fn next_generation(generation: u64) -> Result<u64> {
    generation.checked_add(1).ok_or_else(|| {
        "Browser document identity is exhausted; close this tab and open its URL in a new tab"
            .into()
    })
}

fn begin_navigation(tab: &mut Tab) -> Result<()> {
    tab.generation = next_generation(tab.generation)?;
    tab.loading = true;
    tab.error = None;
    tab.dirty = Dirty::Unknown;
    Ok(())
}

fn validate_url(url: &str) -> Result<()> {
    let parsed = url::Url::parse(url)
        .map_err(|_| "Enter a valid http:// or https:// website address".to_string())?;
    if matches!(parsed.scheme(), "http" | "https")
        && parsed.host_str().is_some()
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && !url.chars().any(char::is_control)
        && url.trim() == url
    {
        Ok(())
    } else {
        Err("Enter a valid http:// or https:// website address".into())
    }
}
