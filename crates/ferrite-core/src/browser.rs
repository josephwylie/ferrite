//! Thread-owned website tabs. Windows realize these identities as exact live
//! engine pages; neither providers nor previews own an independent browser.
//! Only URL/order/selection survive restart. Capabilities and live state do not.

mod control;
pub(crate) use control::{channel, Requests};
pub use control::{Client, Control, Operation, Outcome, Response, ThreadClient};

use crate::ThreadId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    manual: BTreeMap<ThreadId, u64>,
}

type Result<T> = std::result::Result<T, String>;

impl Service {
    pub fn state(&self, thread: ThreadId) -> State {
        let mut state = self.threads.get(&thread).cloned().unwrap_or_default();
        for tab in &mut state.tabs {
            tab.protected = self.protections.values().any(|(id, _, document, kind)| {
                *id == thread && document.tab == tab.id && *kind != Protection::Reclaim
            });
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
            tab.loading = false;
            tab.generation += 1;
            tab.dirty = Dirty::Unknown;
        }
        self.protections.retain(|_, (id, _, _, _)| *id != thread);
        self.manual.remove(&thread);
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
                self.next += 1;
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
                    selected.live = true;
                    selected.loading = true;
                    selected.generation += 1;
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
                            next.live = true;
                            begin_navigation(next);
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
                tab.url = url;
                begin_navigation(tab);
            }
            Action::Back { document }
            | Action::Forward { document }
            | Action::Reload { document } => {
                let tab = state
                    .tabs
                    .iter_mut()
                    .find(|tab| tab.id == document.tab)
                    .expect("checked document");
                begin_navigation(tab);
            }
        }
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
            tab.generation += 1;
        }
        tab.url = page.url;
        tab.loading = page.loading;
        tab.can_back = page.can_back;
        tab.can_forward = page.can_forward;
        tab.error = page.error;
        tab.dirty = page.dirty;
        Ok(state.clone())
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
        if self.protections.values().any(|(id, _, _, _)| *id == thread) {
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

    pub fn release(&mut self, thread: ThreadId, owner: Owner, lease: Lease) -> Result<()> {
        self.check_owner(thread, owner)?;
        match self.protections.get(&lease.0) {
            Some((t, o, _, _)) if *t == thread && *o == owner => {
                self.protections.remove(&lease.0);
                Ok(())
            }
            _ => Err("This browser protection is no longer valid".into()),
        }
    }

    /// Call before forwarding manual input. Every observation from before this
    /// revision is stale, so queued agent input must obtain fresh state.
    pub fn operator_input(&mut self, thread: ThreadId, owner: Owner) -> Result<u64> {
        self.check_owner(thread, owner)?;
        let revision = self.manual.entry(thread).or_default();
        *revision += 1;
        Ok(*revision)
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
            || state.active == Some(tab)
            || page.dirty != Dirty::Clean
            || self
                .protections
                .values()
                .any(|(t, _, d, _)| *t == thread && d.tab == tab)
        {
            return Ok(false);
        }
        page.live = false;
        page.loading = false;
        page.generation += 1;
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
        if state.active == Some(document.tab)
            || page.loading
            || page.dirty != Dirty::Clean
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
                    generation: generation + 1,
                    loading: false,
                    can_back: false,
                    can_forward: false,
                    error: None,
                    live: false,
                    dirty: Dirty::Unknown,
                    protected: false,
                });
            }
            state.active = saved
                .active
                .filter(|id| state.tabs.iter().any(|tab| tab.id == *id));
            service.threads.insert(thread, state);
        }
        Ok(service)
    }
}

fn begin_navigation(tab: &mut Tab) {
    tab.generation += 1;
    tab.loading = true;
    tab.error = None;
    tab.dirty = Dirty::Unknown;
}

fn validate_url(url: &str) -> Result<()> {
    // Do not allow app/file/javascript schemes into the external page surface.
    if (url.starts_with("http://") || url.starts_with("https://"))
        && !url.chars().any(|ch| ch.is_control())
        && url
            .split_once("://")
            .is_some_and(|(_, rest)| !rest.is_empty() && !rest.starts_with('/'))
    {
        Ok(())
    } else {
        Err("Enter an http:// or https:// website address".into())
    }
}
