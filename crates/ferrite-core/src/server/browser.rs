//! Window routing for the one Thread browser service. Capabilities alone do
//! not grant ownership: the connection must also be attached to the Thread.
use super::*;
use crate::browser::{Action, Service};

pub(super) fn load(store: &crate::store::Store) -> Service {
    match store.load_browser().and_then(|saved| {
        saved
            .map(Service::restore)
            .transpose()
            .map_err(io::Error::other)
    }) {
        Ok(service) => service.unwrap_or_default(),
        Err(error) => {
            crate::log!("browser metadata unavailable: {error}");
            Service::default()
        }
    }
}

impl Server {
    pub(super) fn browser_window(&self, id: ClientId, thread: ThreadId) -> Result<(), WireError> {
        if self.engine.thread(thread).is_none() {
            return Err(failed("No such open Thread"));
        }
        // Transcript streaming follows attention; an existing browser lease
        // survives its Unwatch, so unfinished forms are not silently retired.
        let owns = self
            .browser
            .state(thread)
            .owner
            .is_some_and(|owner| owner.window == id);
        self.clients.get(&id)
            .filter(|client| client.welcomed && client.browser_capable && (owns || client.watching.contains(&thread)))
            .map(|_| ())
            .ok_or_else(|| failed("Conversation browser unavailable: open this Thread in a browser-capable Ferrite window"))
    }

    pub(super) fn browser_owned(
        &self,
        id: ClientId,
        thread: ThreadId,
        owner: crate::browser::Owner,
    ) -> Result<(), WireError> {
        if owner.window != id {
            return Err(failed("This browser belongs to another window"));
        }
        self.browser_window(id, thread)?;
        self.browser.check_owner(thread, owner).map_err(failed)
    }

    pub(super) fn attach_browser(
        &mut self,
        id: ClientId,
        thread: ThreadId,
    ) -> Result<Option<Done>, WireError> {
        self.browser_window(id, thread)?;
        self.browser.attach(thread, id).map_err(failed)?;
        self.publish_browser(thread, None);
        Ok(Some(Done::Browser {
            state: self.browser.state(thread),
        }))
    }

    pub(super) fn browser_action(
        &mut self,
        id: ClientId,
        thread: ThreadId,
        action: Action,
    ) -> Result<Option<Done>, WireError> {
        self.browser_window(id, thread)?;
        let state = self.browser.state(thread);
        let owner = state
            .owner
            .filter(|owner| owner.window == id)
            .ok_or_else(|| {
                failed("Attach this Thread's browser in this Ferrite window before using it")
            })?;
        let recovery = match &action {
            Action::Reload { document } => {
                self.browser
                    .check_document(thread, *document)
                    .map_err(failed)?;
                Some(document.tab)
            }
            Action::Close { tab } => Some(*tab),
            _ => None,
        };
        if let Some(tab) = recovery {
            let retired = self
                .browser
                .retire_commands(thread, owner, tab)
                .map_err(failed)?;
            let requests: Vec<_> = self
                .browser_pending
                .iter()
                .filter(|(_, pending)| pending.thread == thread && retired.contains(&pending.lease))
                .map(|(request, _)| *request)
                .collect();
            for request in requests {
                let pending = self
                    .browser_pending
                    .remove(&request)
                    .expect("selected pending command");
                let _ = pending.reply.send(Err("The operator closed or reloaded this tab. The previous action's completion is uncertain and may already have applied; observe the page before continuing and do not retry writes automatically".into()));
            }
            if self
                .browser_uncertain
                .get(&thread)
                .is_some_and(|lease| retired.contains(lease))
            {
                self.browser_uncertain.remove(&thread);
            }
            self.browser_observed.remove(&thread);
        }
        self.apply_browser_action(id, thread, action)
    }

    fn apply_browser_action(
        &mut self,
        id: ClientId,
        thread: ThreadId,
        action: Action,
    ) -> Result<Option<Done>, WireError> {
        self.browser_window(id, thread)?;
        let mut next = self.browser.clone();
        let state = next.apply(thread, id, action.clone()).map_err(failed)?;
        // Metadata is durable before realizing a page. A failed disk write is
        // reported, never silently described as a successful persisted open.
        self.engine
            .browser_store()
            .save_browser(&next.save())
            .map_err(failed)?;
        self.browser = next;
        self.publish_browser(thread, Some(action));
        Ok(Some(Done::Browser { state }))
    }

    pub(super) fn browser_page(
        &mut self,
        id: ClientId,
        thread: ThreadId,
        owner: crate::browser::Owner,
        page: crate::browser::PageState,
    ) -> Result<Option<Done>, WireError> {
        if owner.window != id {
            return Err(failed("This browser belongs to another window"));
        }
        self.browser_window(id, thread)?;
        self.browser.update(thread, owner, page).map_err(failed)?;
        // The page already changed in the real engine: acknowledge its new
        // identity even if the drive failed, retaining the live operator work.
        if let Err(error) = self.save_browser() {
            crate::log!("browser metadata could not be saved: {:?}", error);
            self.browser.persistence_error(thread, &error.message);
        }
        self.publish_browser(thread, None);
        Ok(Some(Done::Browser {
            state: self.browser.state(thread),
        }))
    }

    pub(super) fn save_browser(&self) -> Result<(), WireError> {
        self.engine
            .browser_store()
            .save_browser(&self.browser.save())
            .map_err(failed)
    }

    pub(super) fn publish_browser(&mut self, thread: ThreadId, action: Option<Action>) {
        let state = self.browser.state(thread);
        let owner = state.owner;
        let receivers: Vec<_> = self
            .clients
            .iter()
            .filter(|(id, client)| {
                client.welcomed
                    && (client.watching.contains(&thread)
                        || owner.is_some_and(|owner| owner.window == **id))
            })
            .map(|(id, _)| *id)
            .collect();
        for receiver in receivers {
            self.send(
                receiver,
                ServerMessage::Browser {
                    thread,
                    owned: owner.is_some_and(|owner| owner.window == receiver),
                    owner,
                    state: state.clone(),
                    action: action
                        .clone()
                        .filter(|_| owner.is_some_and(|owner| owner.window == receiver)),
                },
            );
        }
    }
}

pub(super) struct Pending {
    thread: ThreadId,
    owner: crate::browser::Owner,
    document: crate::browser::Document,
    revision: u64,
    lease: crate::browser::Lease,
    observation: bool,
    refreshes: bool,
    recovering: bool,
    deadline: Instant,
    reply: std::sync::mpsc::SyncSender<Result<crate::browser::Response, String>>,
}

impl Server {
    /// Use only windows actually attached to this Thread. An agent's first
    /// request can claim an eligible window; no invented window/client id and
    /// no unrelated hidden browser are involved.
    fn browser_owner(&mut self, thread: ThreadId) -> Result<crate::browser::Owner, String> {
        if let Some(owner) = self.browser.state(thread).owner {
            self.browser_window(owner.window, thread)
                .map_err(|e| e.message)?;
            return Ok(owner);
        }
        let window = self.clients.keys().copied()
            .find(|window| self.browser_window(*window, thread).is_ok())
            .ok_or("Conversation browser unavailable: open this Thread in a browser-capable Ferrite window")?;
        let owner = self.browser.attach(thread, window)?;
        self.publish_browser(thread, None);
        Ok(owner)
    }

    fn browser_response(
        &self,
        thread: ThreadId,
        value: Option<serde_json::Value>,
    ) -> crate::browser::Response {
        crate::browser::Response {
            state: self.browser.state(thread),
            revision: self.browser.operator_revision(thread),
            value,
            observation: None,
        }
    }

    fn browser_observation_current(
        &self,
        thread: ThreadId,
        document: crate::browser::Document,
        revision: u64,
        receipt: Option<&crate::browser::Observation>,
    ) -> bool {
        self.browser_observed
            .get(&thread)
            .is_some_and(|(observed, version, expected)| {
                *observed == document && *version == revision && receipt == Some(expected)
            })
    }

    fn browser_agent_action(
        &mut self,
        thread: ThreadId,
        owner: crate::browser::Owner,
        action: Action,
        observation: Option<crate::browser::Observation>,
    ) -> Result<crate::browser::Response, String> {
        if self.browser.operator_active(thread) {
            return Err("The operator is interacting with this page; wait for that gesture to settle, then observe fresh state".into());
        }
        if self
            .browser_pending
            .values()
            .any(|pending| pending.thread == thread)
            || self.browser_uncertain.contains_key(&thread)
        {
            return Err("This Thread's browser has an active or uncertain command; observe its result before navigating, switching or closing pages".into());
        }
        let state = self.browser.state(thread);
        if state.tabs.iter().any(|tab| tab.protected) {
            return Err("This page is in use; finish its command or annotation capture before changing tabs".into());
        }
        let revision = self.browser.operator_revision(thread);
        let target = match &action {
            Action::Navigate { document, .. }
            | Action::Back { document }
            | Action::Forward { document }
            | Action::Reload { document } => Some(*document),
            Action::Close { tab } => state
                .tabs
                .iter()
                .find(|page| page.id == *tab)
                .map(|page| page.document()),
            Action::Open { .. } | Action::Select { .. } if revision > 0 => state
                .tabs
                .iter()
                .find(|page| Some(page.id) == state.active)
                .map(|page| page.document()),
            _ => None,
        };
        if let Some(document) = target {
            if !self.browser_observation_current(thread, document, revision, observation.as_ref()) {
                return Err("Observe the current page before navigating, switching or closing it; operator input invalidates earlier observations".into());
            }
        }
        self.apply_browser_action(owner.window, thread, action)
            .map_err(|error| error.message)?;
        Ok(self.browser_response(thread, None))
    }

    pub(super) fn forward_browser_requests(&mut self) {
        use crate::browser::Operation;
        while let Ok(request) = self.browser_requests.0.try_recv() {
            let thread = request.thread;
            let owner = match self.browser_owner(thread) {
                Ok(owner) => owner,
                Err(error) => {
                    let _ = request.reply.send(Err(error));
                    continue;
                }
            };
            let result = match request.operation {
                Operation::State => Ok(self.browser_response(thread, None)),
                Operation::Action {
                    action,
                    observation,
                } => self.browser_agent_action(thread, owner, action, observation),
                Operation::Control {
                    document,
                    revision,
                    observation,
                    control,
                } => {
                    let prepare = (|| {
                        self.browser.check_owner(thread, owner)?;
                        self.browser.check_document(thread, document)?;
                        if self.browser.state(thread).active != Some(document.tab) {
                            return Err(
                                "Select this Thread's tab before observing or controlling its page"
                                    .into(),
                            );
                        }
                        if self
                            .browser_pending
                            .values()
                            .any(|pending| pending.thread == thread)
                        {
                            return Err("Another agent is using this Thread's browser; wait for its command to finish".into());
                        }
                        if !control.observation() && self.browser.operator_active(thread) {
                            return Err("The operator is interacting with this page; wait for that gesture to settle, then observe fresh state".into());
                        }
                        if !control.observation()
                            && (revision != self.browser.operator_revision(thread)
                                || !self.browser_observation_current(
                                    thread,
                                    document,
                                    revision,
                                    observation.as_ref(),
                                ))
                        {
                            return Err("Observe the current page before continuing; operator input and page changes invalidate earlier observations".into());
                        }
                        let recovering = self.browser_uncertain.contains_key(&thread);
                        if recovering && !control.observation() {
                            return Err("The previous action's completion is uncertain; observe the current page before continuing, and do not retry the write automatically".into());
                        }
                        let lease = if let Some(lease) = self.browser_uncertain.get(&thread) {
                            self.browser
                                .check_exclusive_command(thread, owner, *lease)?;
                            *lease
                        } else {
                            self.browser.protect(
                                thread,
                                owner,
                                document,
                                crate::browser::Protection::Command,
                            )?
                        };
                        Ok((lease, recovering))
                    })();
                    match prepare {
                        Err(error) => Err(error),
                        Ok((lease, recovering)) => {
                            self.browser_next += 1;
                            let number = self.browser_next;
                            let revision = self.browser.operator_revision(thread);
                            self.browser_pending.insert(
                                number,
                                Pending {
                                    thread,
                                    owner,
                                    document,
                                    revision,
                                    lease,
                                    observation: control.observation(),
                                    refreshes: matches!(control, crate::browser::Control::Observe)
                                        && !self.browser.operator_active(thread),
                                    recovering,
                                    deadline: Instant::now() + Duration::from_secs(45),
                                    reply: request.reply,
                                },
                            );
                            self.publish_browser(thread, None);
                            self.send(
                                owner.window,
                                ServerMessage::BrowserRequest {
                                    request: number,
                                    thread,
                                    owner,
                                    document,
                                    revision,
                                    control,
                                },
                            );
                            continue;
                        }
                    }
                }
            };
            let _ = request.reply.send(result);
        }
        let expired: Vec<_> = self
            .browser_pending
            .iter()
            .filter(|(_, pending)| Instant::now() >= pending.deadline)
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            let pending = self.browser_pending.remove(&id).expect("pending request");
            // A dispatched write may have happened even though its reply did
            // not arrive. Hold its page until a fresh observation resolves it.
            if !pending.observation || pending.recovering {
                self.browser_uncertain.insert(pending.thread, pending.lease);
            } else {
                let _ = self
                    .browser
                    .release(pending.thread, pending.owner, pending.lease);
            }
            self.publish_browser(pending.thread, None);
            let _ = pending.reply.send(Err("Browser request timed out; action completion may be uncertain. Observe the page before continuing; do not retry writes automatically".into()));
        }
    }

    pub(super) fn answer_browser(
        &mut self,
        client: ClientId,
        request: u64,
        thread: ThreadId,
        owner: crate::browser::Owner,
        document: crate::browser::Document,
        revision: u64,
        outcome: crate::browser::Outcome,
    ) {
        let Some(pending) = self.browser_pending.get(&request) else {
            return;
        };
        if pending.thread != thread
            || pending.owner != owner
            || owner.window != client
            || pending.document != document
            || pending.revision != revision
        {
            return;
        }
        let pending = self
            .browser_pending
            .remove(&request)
            .expect("checked request");
        let current = self.browser.check_owner(thread, owner)
            .and_then(|_| self.browser.check_document(thread, document))
            .and_then(|_| self.browser.check_exclusive_command(thread, owner, pending.lease))
            .and_then(|_| if self.browser.state(thread).active == Some(document.tab) { Ok(()) } else { Err("The selected tab changed; select and observe it before continuing".into()) })
            .and_then(|_| if revision == self.browser.operator_revision(thread) { Ok(()) }
                else { Err("The operator interacted with this page; observe fresh state before continuing".into()) });
        let refreshes = pending.refreshes && !self.browser.operator_active(thread);
        let (mut result, uncertain) = match (current, outcome) {
            (Err(error), _) => (Err(error), !pending.observation || pending.recovering),
            (Ok(()), crate::browser::Outcome::Success { value }) => {
                let mut response = self.browser_response(thread, Some(value));
                if refreshes {
                    let receipt = crate::browser::Observation::new(format!("{}:{request}", self.engine.boot_id()));
                    self.browser_observed.insert(thread, (document, revision, receipt.clone()));
                    response.observation = Some(receipt);
                }
                (Ok(response), pending.recovering && !refreshes)
            },
            (Ok(()), crate::browser::Outcome::Failed { error }) => (Err(error), pending.recovering),
            (Ok(()), crate::browser::Outcome::Uncertain { error }) => (Err(format!("Action completion is uncertain: {error}. Observe current page state; do not retry writes automatically")), true),
        };
        if uncertain {
            self.browser_observed.remove(&thread);
            self.browser_uncertain.insert(thread, pending.lease);
            if let Err(error) =
                self.browser
                    .command_finished_uncertain(thread, owner, pending.lease)
            {
                crate::log!(
                    "browser terminal command could not retain its completion state: {error}"
                );
            }
        } else {
            self.browser_uncertain.remove(&thread);
            let _ = self.browser.release(thread, owner, pending.lease);
        }
        if let Ok(response) = &mut result {
            response.state = self.browser.state(thread);
        }
        if pending.reply.send(result).is_err() && !pending.observation {
            // The native action finished, but its caller timed out or went
            // away before receiving completion. Keep that write uncertain
            // from the caller's perspective; an old receipt cannot retry it.
            self.browser_observed.remove(&thread);
            if !uncertain {
                match self.browser.protect(
                    thread,
                    owner,
                    document,
                    crate::browser::Protection::Command,
                ) {
                    Ok(lease) => {
                        self.browser_uncertain.insert(thread, lease);
                        if let Err(error) = self
                            .browser
                            .command_finished_uncertain(thread, owner, lease)
                        {
                            crate::log!("browser undelivered completion could not retain its terminal state: {error}");
                        }
                    }
                    Err(error) => crate::log!(
                        "browser undelivered completion could not retain protection: {error}"
                    ),
                }
            }
        }
        self.publish_browser(thread, None);
    }

    pub(super) fn browser_owner_gone(&mut self, window: ClientId) {
        let gone: Vec<_> = self
            .browser_pending
            .iter()
            .filter(|(_, pending)| pending.owner.window == window)
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            let pending = self.browser_pending.remove(&id).expect("pending request");
            self.browser_uncertain.remove(&pending.thread);
            let _ = pending.reply.send(Err("Conversation browser owner disconnected; action completion may be uncertain. Reopen this Thread and observe its current page before continuing".into()));
        }
        let detached = self.browser.detach_window(window);
        for thread in detached {
            self.browser_uncertain.remove(&thread);
            self.browser_observed.remove(&thread);
            self.publish_browser(thread, None);
        }
    }
}
