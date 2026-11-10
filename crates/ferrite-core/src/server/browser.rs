//! Window routing for the one Thread browser service. Capabilities alone do
//! not grant ownership: the connection must also be attached to the Thread.
use super::*;
use crate::browser::{Action, Service};
mod disk;
#[cfg(test)]
pub(super) use disk::Control as DiskControl;
pub(super) use disk::Disk;

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
    pub(super) fn browser_capture(
        &mut self,
        id: ClientId,
        request: u64,
        thread: ThreadId,
        owner: crate::browser::Owner,
        lease: crate::browser::Lease,
        capture: crate::browser_annotations::Capture,
        png: &crate::protocol::WirePng,
    ) -> Result<Option<Done>, WireError> {
        use crate::browser_annotations::{self, MAX_CAPTURE_BYTES};
        use base64::Engine as _;
        self.browser_owned(id, thread, owner)?;
        self.browser
            .check_protection(
                thread,
                owner,
                capture.document,
                lease,
                crate::browser::Protection::Annotation,
            )
            .map_err(failed)?;
        if !self
            .browser
            .state(thread)
            .tabs
            .iter()
            .any(|tab| tab.id == capture.document.tab && tab.url == capture.url)
        {
            return Err(failed("The page changed; capture it again"));
        }
        if png.base64.len() > ((MAX_CAPTURE_BYTES + 2) / 3) * 4 {
            return Err(failed(
                "The browser capture is too large; use a smaller viewport and capture again",
            ));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&png.base64)
            .map_err(|_| failed("The browser capture could not be read; capture the page again"))?;
        if browser_annotations::png_size(&bytes).map_err(failed)? != (png.width, png.height) {
            return Err(failed("The page geometry changed; capture it again"));
        }
        if self
            .browser_captures
            .iter()
            .any(|pending| pending.thread == thread && pending.lease == lease)
        {
            return Err(failed("This browser capture is already being retained"));
        }
        let dir = self.engine.browser_store().dir().join(thread.to_string());
        let answer = self
            .browser_disk
            .submit(|answer| disk::Job::Capture(dir, capture, bytes, answer));
        self.browser_captures.push(CaptureWrite {
            id,
            request,
            thread,
            owner,
            lease,
            release: false,
            answer,
        });
        Ok(None)
    }

    pub(super) fn browser_capture_image(
        &mut self,
        id: ClientId,
        request: u64,
        thread: ThreadId,
        hash: &str,
    ) -> Result<Option<Done>, WireError> {
        if !self
            .clients
            .get(&id)
            .is_some_and(|client| client.welcomed && client.watching.contains(&thread))
        {
            return Err(failed(
                "Open this Thread before reviewing its browser captures",
            ));
        }
        let dir = self.engine.browser_store().dir().join(thread.to_string());
        let hash = hash.to_owned();
        let answer = self
            .browser_disk
            .submit(|answer| disk::Job::Image(dir, hash, answer));
        self.reads.push((id, request, answer));
        Ok(None)
    }

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
        let owner = self
            .browser
            .state(thread)
            .owner
            .ok_or_else(|| failed("This browser has no owner"))?;
        let mut next = self.browser.clone();
        let state = next.apply(thread, id, action.clone()).map_err(failed)?;
        let saved = next.save();
        let answer = self.browser_disk.save(saved);
        self.browser_write = Some(Write {
            thread,
            owner,
            action: Some(action),
            answer,
            reply: Reply::None,
            failure: None,
        });
        Ok(Some(Done::Browser { state }))
    }

    pub(super) fn browser_page(
        &mut self,
        id: ClientId,
        thread: ThreadId,
        owner: crate::browser::Owner,
        page: crate::browser::PageState,
    ) -> Result<Option<Done>, WireError> {
        self.browser_owned(id, thread, owner)?;
        self.browser.update(thread, owner, page).map_err(failed)?;
        let saved = self.browser.save();
        let answer = self.browser_disk.save(saved);
        self.browser_write = Some(Write {
            thread,
            owner,
            action: None,
            answer,
            reply: Reply::None,
            failure: None,
        });
        // The native page already changed; publish its identity immediately,
        // but its durable acknowledgement waits for the worker.
        self.publish_browser(thread, None);
        Ok(Some(Done::Browser {
            state: self.browser.state(thread),
        }))
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
        while self.browser_write.is_none() {
            let Ok(request) = self.browser_requests.0.try_recv() else {
                break;
            };
            let thread = request.thread;
            let owner = match self.browser_owner(thread) {
                Ok(owner) => owner,
                Err(error) => {
                    let _ = request.reply.send(Err(error));
                    continue;
                }
            };
            let result = match request.operation {
                Operation::Unknown => Err("Unsupported browser operation".into()),
                Operation::State => Ok(self.browser_response(thread, None)),
                Operation::Action {
                    action,
                    observation,
                } => match self.browser_agent_action(thread, owner, action, observation) {
                    Ok(_) => {
                        self.browser_write
                            .as_mut()
                            .expect("action persistence")
                            .reply = Reply::Agent {
                            reply: request.reply,
                            revision: self.browser.operator_revision(thread),
                        };
                        continue;
                    }
                    Err(error) => Err(error),
                },
                Operation::Control {
                    document,
                    revision,
                    observation,
                    control,
                } => {
                    let prepare = (|| {
                        if !control.supported() {
                            return Err("Unsupported browser control or wait state".into());
                        }
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
            (Ok(()), crate::browser::Outcome::Unknown) => (Err("Browser completion is unknown; observe current state and do not replay writes automatically".into()), true),
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

pub(super) enum Reply {
    None,
    Window(ClientId, u64),
    Agent {
        reply: std::sync::mpsc::SyncSender<Result<crate::browser::Response, String>>,
        revision: u64,
    },
}

pub(super) struct Write {
    thread: ThreadId,
    owner: crate::browser::Owner,
    action: Option<Action>,
    answer: Receiver<Result<Option<Done>, WireError>>,
    pub(super) reply: Reply,
    // A staged action invalidated by an owner change is repaired on disk
    // before reporting failure; it must not resurrect the departed owner.
    failure: Option<WireError>,
}

pub(super) struct CaptureWrite {
    id: ClientId,
    request: u64,
    pub(super) thread: ThreadId,
    pub(super) owner: crate::browser::Owner,
    pub(super) lease: crate::browser::Lease,
    pub(super) release: bool,
    answer: Receiver<Result<Option<Done>, WireError>>,
}

fn completed(
    answer: &Receiver<Result<Option<Done>, WireError>>,
) -> Option<Result<Option<Done>, WireError>> {
    match answer.try_recv() {
        Ok(result) => Some(result),
        Err(mpsc::TryRecvError::Empty) => None,
        Err(mpsc::TryRecvError::Disconnected) => Some(Err(failed("The browser disk worker ended"))),
    }
}

impl Server {
    pub(super) fn answer_browser_disk(&mut self) {
        let mut captures = std::mem::take(&mut self.browser_captures);
        for capture in captures.drain(..) {
            if let Some(result) = completed(&capture.answer) {
                if capture.release || result.is_err() {
                    let _ = self
                        .browser
                        .release(capture.thread, capture.owner, capture.lease);
                    self.publish_browser(capture.thread, None);
                }
                self.reply(capture.id, capture.request, result);
            } else {
                self.browser_captures.push(capture);
            }
        }
        let result = self
            .browser_write
            .as_ref()
            .and_then(|write| completed(&write.answer));
        if let Some(mut result) = result {
            let mut write = self.browser_write.take().expect("completed browser write");
            if result.is_ok() && write.failure.is_none() {
                if let Some(action) = &write.action {
                    // Reapply to the live service: revisions, leases and other
                    // Threads may have changed while the worker waited.
                    let applied = match &write.reply {
                        Reply::Agent { revision, .. }
                            if *revision != self.browser.operator_revision(write.thread)
                                || self.browser.operator_active(write.thread) =>
                        {
                            Err("The operator interacted with this page; observe fresh state before continuing".into())
                        }
                        _ => self.browser.apply_owned(write.thread, write.owner, action.clone()),
                    };
                    match applied {
                        Ok(_) => self.publish_browser(write.thread, Some(action.clone())),
                        Err(error) => {
                            let saved = self.browser.save();
                            write.answer = self.browser_disk.save(saved);
                            write.failure = Some(failed(error));
                            self.browser_write = Some(write);
                            return;
                        }
                    }
                }
            }
            if let Err(error) = &result {
                self.browser.persistence_error(write.thread, &error.message);
                self.publish_browser(write.thread, None);
            }
            if let Some(error) = write.failure.take() {
                // Even if the corrective write fails, never acknowledge the
                // invalidated action as a successful persisted navigation.
                result = Err(error);
            }
            if write.action.is_none() {
                // A native page has already changed. Acknowledge its current
                // identity even on a disk failure; the state carries the error.
                result = Ok(None);
            }
            let result = result.map(|_| {
                Some(Done::Browser {
                    state: self.browser.state(write.thread),
                })
            });
            match write.reply {
                Reply::None => {}
                Reply::Window(id, request) => self.reply(id, request, result),
                Reply::Agent { reply, .. } => {
                    let result = result
                        .map(|_| self.browser_response(write.thread, None))
                        .map_err(|error| error.message);
                    let _ = reply.send(result);
                }
            }
        }
        while self.browser_write.is_none() {
            let Some((id, request, op)) = self.browser_changes.pop_front() else {
                break;
            };
            if let Some(request) = request {
                self.command(id, Command { id: request, op });
            } else if let Op::BrowserPage {
                thread,
                owner,
                page,
            } = op
            {
                let _ = self.browser_page(id, thread, owner, page);
            }
        }
    }
}
