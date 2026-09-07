//! Ordinary host requests and Main's interrupt target. Server-originated
//! Decisions, history reads and catalog requests keep their own correlation.
use std::{collections::HashMap, io};

use serde_json::{json, Value};

use crate::{
    activity::{ActivityEvent, ExecutionEvent},
    SessionEvent, TurnOutcome,
};

const MAX_PENDING: usize = 128;

enum Purpose {
    Start { settled: bool },
    Interrupt,
    Rename,
}

#[derive(Default)]
pub(super) struct Requests {
    pending: HashMap<u64, Purpose>,
    starting: Option<u64>,
    pub current_turn: Option<String>,
    deferred_interrupt: Option<u64>,
}

impl Requests {
    fn register(&mut self, id: u64, purpose: Purpose) -> io::Result<()> {
        if self.pending.len() >= MAX_PENDING {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Codex has too many unanswered requests",
            ));
        }
        self.pending.insert(id, purpose);
        Ok(())
    }

    pub fn start(&mut self, id: u64) -> io::Result<()> {
        self.register(id, Purpose::Start { settled: false })?;
        self.starting = Some(id);
        Ok(())
    }

    pub fn rename(&mut self, id: u64) -> io::Result<()> {
        self.register(id, Purpose::Rename)
    }

    pub fn interrupt(&mut self, id: u64, thread: &str) -> io::Result<Option<Value>> {
        if let Some(turn) = self.current_turn.clone() {
            self.register(id, Purpose::Interrupt)?;
            return Ok(Some(interrupt_request(id, thread, &turn)));
        }
        if self.starting.is_some() && self.deferred_interrupt.is_none() {
            self.register(id, Purpose::Interrupt)?;
            self.deferred_interrupt = Some(id);
        }
        Ok(None)
    }

    pub fn started(&mut self, turn: &str) {
        self.current_turn = Some(turn.into());
    }

    pub fn completed(&mut self, turn: &str) {
        if self.current_turn.as_deref() == Some(turn) {
            self.current_turn = None;
            if let Some(Purpose::Start { settled }) =
                self.starting.and_then(|id| self.pending.get_mut(&id))
            {
                *settled = true;
            }
            self.cancel_deferred();
        }
    }

    pub fn discard(&mut self, id: u64) {
        self.pending.remove(&id);
        if self.starting == Some(id) {
            self.starting = None;
            self.cancel_deferred();
        }
        if self.deferred_interrupt == Some(id) {
            self.deferred_interrupt = None;
        }
    }

    fn cancel_deferred(&mut self) {
        if let Some(id) = self.deferred_interrupt.take() {
            self.pending.remove(&id);
        }
    }

    pub fn take_interrupt(&mut self, thread: &str) -> Option<Value> {
        let turn = self.current_turn.as_deref()?;
        let id = self.deferred_interrupt.take()?;
        Some(interrupt_request(id, thread, turn))
    }

    /// Only a response in our own ID space can settle one of these requests.
    /// The native message remains verbatim; protocol codes are not UI prose.
    pub fn response(&mut self, frame: &Value) -> Option<Vec<SessionEvent>> {
        if frame.get("method").is_some()
            || (frame.get("result").is_none() && frame.get("error").is_none())
        {
            return None;
        }
        let id = frame["id"].as_u64()?;
        let purpose = self.pending.remove(&id)?;
        let error = frame.get("error").map(|error| {
            error["message"]
                .as_str()
                .unwrap_or("Codex rejected the request")
                .to_owned()
        });
        match purpose {
            Purpose::Start { settled } => {
                if self.starting == Some(id) {
                    self.starting = None;
                    if error.is_some() {
                        self.current_turn = None;
                        self.cancel_deferred();
                    } else if !settled {
                        let turn = &frame["result"]["turn"];
                        if !matches!(
                            turn["status"].as_str(),
                            Some("completed" | "failed" | "interrupted")
                        ) {
                            if let Some(id) = turn["id"].as_str().filter(|id| !id.is_empty()) {
                                self.started(id);
                            } else {
                                self.cancel_deferred();
                            }
                        } else {
                            self.current_turn = None;
                            self.cancel_deferred();
                        }
                    }
                }
                Some(
                    error
                        .into_iter()
                        .map(|error| SessionEvent::TurnEnded {
                            outcome: TurnOutcome::Error(error),
                            cost_usd: None,
                        })
                        .collect(),
                )
            }
            Purpose::Interrupt | Purpose::Rename => Some(error.into_iter().map(notice).collect()),
        }
    }
}

fn interrupt_request(id: u64, thread: &str, turn: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "method":"turn/interrupt",
        "params":{"threadId":thread,"turnId":turn}})
}

pub(super) fn notice(text: String) -> SessionEvent {
    SessionEvent::Activity(ActivityEvent::MainContent {
        id: None,
        event: ExecutionEvent::Notice { text },
    })
}
