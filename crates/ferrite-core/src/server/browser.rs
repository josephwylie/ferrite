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
        self.clients.get(&id)
            .filter(|client| client.welcomed && client.browser_capable && client.watching.contains(&thread))
            .map(|_| ())
            .ok_or_else(|| failed("Conversation browser unavailable: open this Thread in a browser-capable Ferrite window"))
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
            .filter(|(_, client)| client.welcomed && client.watching.contains(&thread))
            .map(|(id, _)| *id)
            .collect();
        for receiver in receivers {
            self.send(
                receiver,
                ServerMessage::Browser {
                    thread,
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
        }
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
                Operation::Action(action) => self
                    .browser_action(owner.window, thread, action)
                    .map(|_| self.browser_response(thread, None))
                    .map_err(|error| error.message),
                Operation::Control {
                    document,
                    revision,
                    control,
                } => {
                    let prepare = (|| {
                        self.browser.check_owner(thread, owner)?;
                        self.browser.check_document(thread, document)?;
                        if self
                            .browser_pending
                            .values()
                            .any(|pending| pending.thread == thread)
                        {
                            return Err("Another agent is using this Thread's browser; wait for its command to finish".into());
                        }
                        if !control.observation()
                            && revision != self.browser.operator_revision(thread)
                        {
                            return Err("The operator interacted with this page; observe fresh state before continuing".into());
                        }
                        let recovering = self.browser_uncertain.contains_key(&thread);
                        if recovering && !control.observation() {
                            return Err("The previous action's completion is uncertain; observe the current page before continuing, and do not retry the write automatically".into());
                        }
                        let lease = if let Some(lease) = self.browser_uncertain.get(&thread) {
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
                                    recovering,
                                    deadline: Instant::now() + Duration::from_secs(45),
                                    reply: request.reply,
                                },
                            );
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
            .and_then(|_| if revision == self.browser.operator_revision(thread) { Ok(()) }
                else { Err("The operator interacted with this page; observe fresh state before continuing".into()) });
        let (result, uncertain) = match (current, outcome) {
            (Err(error), _) => (Err(error), !pending.observation || pending.recovering),
            (Ok(()), crate::browser::Outcome::Success { value }) => (Ok(self.browser_response(thread, Some(value))), false),
            (Ok(()), crate::browser::Outcome::Failed { error }) => (Err(error), pending.recovering),
            (Ok(()), crate::browser::Outcome::Uncertain { error }) => (Err(format!("Action completion is uncertain: {error}. Observe current page state; do not retry writes automatically")), true),
        };
        if uncertain {
            self.browser_uncertain.insert(thread, pending.lease);
        } else {
            self.browser_uncertain.remove(&thread);
            let _ = self.browser.release(thread, owner, pending.lease);
        }
        let _ = pending.reply.send(result);
    }

    pub(super) fn browser_owner_gone(&mut self, window: ClientId, thread: Option<ThreadId>) {
        let gone: Vec<_> = self
            .browser_pending
            .iter()
            .filter(|(_, pending)| {
                pending.owner.window == window
                    && thread.is_none_or(|thread| pending.thread == thread)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            let pending = self.browser_pending.remove(&id).expect("pending request");
            self.browser_uncertain.remove(&pending.thread);
            let _ = pending.reply.send(Err("Conversation browser owner disconnected; action completion may be uncertain. Reopen this Thread and observe its current page before continuing".into()));
        }
        let detached = match thread {
            Some(thread) => self
                .browser
                .detach(thread, window)
                .then_some(thread)
                .into_iter()
                .collect(),
            None => self.browser.detach_window(window),
        };
        for thread in detached {
            self.browser_uncertain.remove(&thread);
            self.publish_browser(thread, None);
        }
    }
}
