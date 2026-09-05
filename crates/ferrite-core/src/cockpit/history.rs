//! A single bounded disk worker restores evicted child projections. The log
//! prefix is frozen before reading; subsequent accepted facts are held by the
//! Cockpit and replayed once after that prefix, without writing them again.

use super::*;
use std::sync::mpsc::{self, SyncSender};

pub(super) struct Request {
    thread: ThreadId,
    key: AgentKey,
    generation: u64,
    serial: u64,
    through: u64,
}

pub(super) struct Completed {
    request: Request,
    inputs: Result<Vec<ActivityInput>, LoadError>,
}

pub(super) struct Loader {
    requests: SyncSender<Request>,
    results: Receiver<Completed>,
}

pub(super) struct Pending {
    serial: u64,
    events: Vec<ActivityEvent>,
    bytes: usize,
}

impl Loader {
    pub(super) fn start(dir: PathBuf) -> io::Result<Self> {
        let store = Store::open(dir)?;
        let (requests, receive) = mpsc::sync_channel::<Request>(8);
        let (send, results) = mpsc::sync_channel(8);
        std::thread::Builder::new()
            .name("ferrite-child-history".into())
            .spawn(move || {
                while let Ok(request) = receive.recv() {
                    let inputs =
                        store.agent_inputs_at(request.thread, &request.key, request.through);
                    if send.send(Completed { request, inputs }).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self { requests, results })
    }
}

impl Thread {
    pub(super) fn buffer_history(&mut self, event: &ActivityEvent) {
        let view = self.activity.view();
        for (key, pending) in &mut self.history {
            let target = view.canonical_subject(&Subject::Subagent(key.clone()));
            let belongs = match event {
                ActivityEvent::Content { key, .. }
                | ActivityEvent::HistoryContent { key, .. }
                | ActivityEvent::Status { key, .. }
                | ActivityEvent::Detached { key }
                | ActivityEvent::Coverage { key, .. } => {
                    view.canonical_subject(&Subject::Subagent(key.clone())) == target
                }
                ActivityEvent::Alias { from, to } => {
                    view.canonical_subject(&Subject::Subagent(from.clone())) == target
                        || view.canonical_subject(&Subject::Subagent(to.clone())) == target
                }
                _ => false,
            };
            if belongs {
                // Includes tool JSON and text; this bounds the wait buffer, not
                // the provider's already-bounded incoming channel.
                pending.bytes = pending.bytes.saturating_add(format!("{event:?}").len());
                pending.events.push(match event {
                    ActivityEvent::Detached { key } => ActivityEvent::Coverage {
                        key: key.clone(),
                        coverage: crate::activity::TranscriptCoverage::Unavailable,
                    },
                    event => event.clone(),
                });
            }
        }
    }

    pub(super) fn history_backpressure(&self) -> bool {
        self.history
            .values()
            .any(|pending| pending.events.len() >= 1024 || pending.bytes >= 4 * 1024 * 1024)
    }
}

impl Cockpit {
    /// Restore an evicted child from Ferrite's log. Viewing never starts or
    /// resumes a provider child. Returns true only when a read was queued.
    pub fn ensure_subject_history(
        &mut self,
        thread: ThreadId,
        subject: &Subject,
    ) -> io::Result<bool> {
        let Some(state) = self.threads.get_mut(&thread) else {
            return Ok(false);
        };
        let subject = state.activity.view().canonical_subject(subject);
        let Subject::Subagent(key) = subject else {
            return Ok(false);
        };
        let view = state.activity.view();
        if let Some(error) = state.history_errors.get(&key) {
            return Err(io::Error::other(error.clone()));
        }
        if view
            .subject(&Subject::Subagent(key.clone()))
            .is_none_or(|subject| subject.retained())
            || state.history.keys().any(|pending| {
                view.canonical_subject(&Subject::Subagent(pending.clone()))
                    == Subject::Subagent(key.clone())
            })
        {
            return Ok(false);
        }
        if self.history_loader.is_none() {
            self.history_loader = Some(Loader::start(self.store.dir().to_path_buf())?);
        }
        let through = state.writer.checkpoint().inspect_err(|error| {
            state.report_store_error(io::Error::new(error.kind(), error.to_string()));
        })?;
        let serial = next_generation();
        let request = Request {
            thread,
            key: key.clone(),
            generation: state.generation,
            serial,
            through,
        };
        self.history_loader
            .as_ref()
            .expect("initialized")
            .requests
            .try_send(request)
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    format!("history reader busy: {error}"),
                )
            })?;
        state.history.insert(
            key,
            Pending {
                serial,
                events: Vec::new(),
                bytes: 0,
            },
        );
        Ok(true)
    }

    pub fn subject_history_error(&self, thread: ThreadId, subject: &Subject) -> Option<&str> {
        let state = self.threads.get(&thread)?;
        let Subject::Subagent(key) = state.activity.view().canonical_subject(subject) else {
            return None;
        };
        state.history_errors.get(&key).map(String::as_str)
    }

    /// Explicit operator retry after a failed read; ordinary selection never
    /// discards a healthy projection or repeatedly retries a failed file.
    pub fn retry_subject_history(
        &mut self,
        thread: ThreadId,
        subject: &Subject,
    ) -> io::Result<bool> {
        if let Some(state) = self.threads.get_mut(&thread) {
            let subject = state.activity.view().canonical_subject(subject);
            if let Subject::Subagent(key) = &subject {
                if state.history_errors.remove(key).is_some() {
                    state.activity.apply(ActivityInput::Evict(subject));
                }
            }
        }
        self.ensure_subject_history(thread, subject)
    }

    pub(super) fn advance_history(&mut self) -> Vec<PaneUpdate> {
        let mut changes = Vec::new();
        let Some(loader) = &self.history_loader else {
            return changes;
        };
        for completed in loader.results.try_iter() {
            let Request {
                thread,
                key,
                generation,
                serial,
                ..
            } = completed.request;
            let Some(state) = self.threads.get_mut(&thread) else {
                continue;
            };
            if state.generation != generation
                || state
                    .history
                    .get(&key)
                    .is_none_or(|pending| pending.serial != serial)
            {
                continue;
            }
            let pending = state.history.remove(&key).expect("matched request");
            let subject = state
                .activity
                .view()
                .canonical_subject(&Subject::Subagent(key.clone()));
            if state
                .activity
                .view()
                .subject(&subject)
                .is_none_or(|view| view.retained())
            {
                continue;
            }
            let mut changed = PaneUpdate::new(thread);
            changed.absorb(
                state.activity.apply(ActivityInput::Retain(subject.clone())),
                true,
            );
            match completed.inputs {
                Ok(inputs) => {
                    for input in inputs {
                        // The running Activity already owns the newest graph
                        // and labels. A disk prefix cannot reattach a child or
                        // replace metadata received after the checkpoint.
                        if matches!(
                            &input,
                            ActivityInput::ReplayEvent(
                                ActivityEvent::Discovered(_)
                                    | ActivityEvent::Detached { .. }
                                    | ActivityEvent::Alias { .. }
                            )
                        ) {
                            continue;
                        }
                        changed.absorb(state.activity.apply(input), true);
                    }
                }
                Err(error) => {
                    if let Subject::Subagent(key) = &subject {
                        state.history_errors.insert(key.clone(), error.to_string());
                    }
                    changed.absorb(
                        state.activity.apply(ActivityInput::ReplayEvent(
                            ActivityEvent::HistoryContent {
                                key: key.clone(),
                                id: None,
                                event: crate::activity::ExecutionEvent::Notice {
                                    text: format!("Earlier activity could not be loaded: {error}"),
                                },
                            },
                        )),
                        true,
                    );
                    changed.absorb(
                        state
                            .activity
                            .apply(ActivityInput::ReplayEvent(ActivityEvent::Coverage {
                                key: key.clone(),
                                coverage: crate::activity::TranscriptCoverage::Partial,
                            })),
                        false,
                    );
                }
            }
            for event in pending.events {
                let event = match event {
                    ActivityEvent::Status { key, state } => {
                        use crate::activity::{AgentStatus, ExecutionEvent};
                        let outcome = match state {
                            AgentStatus::Idle => crate::TurnOutcome::Completed,
                            AgentStatus::Interrupted | AgentStatus::Shutdown => {
                                crate::TurnOutcome::Interrupted
                            }
                            AgentStatus::Failed => {
                                crate::TurnOutcome::Error("Subagent failed".into())
                            }
                            _ => continue,
                        };
                        ActivityEvent::HistoryContent {
                            key,
                            id: None,
                            event: ExecutionEvent::TurnEnded {
                                outcome,
                                cost_usd: None,
                            },
                        }
                    }
                    ActivityEvent::Alias { .. } => continue,
                    event => event,
                };
                changed.absorb(
                    state.activity.apply(ActivityInput::ReplayEvent(event)),
                    true,
                );
            }
            changes.push(changed);
        }
        changes
    }
}
