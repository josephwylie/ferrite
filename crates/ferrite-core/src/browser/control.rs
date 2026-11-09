//! Provider-neutral requests to the owning window. Native target identities
//! stay in that window; a provider receives only Thread/tab/document state.
use super::{Action, Document, State};
use crate::ThreadId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{mpsc, Arc, Mutex};

/// The operation itself determines arbitration; callers cannot relabel an
/// input as an observation to bypass manual intervention or command protection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Control {
    Observe,
    Screenshot,
    Console,
    Network,
    Wait {
        selector: String,
        state: WaitState,
    },
    Click {
        reference: String,
    },
    Type {
        reference: String,
        text: String,
    },
    Fill {
        reference: String,
        value: String,
    },
    Select {
        reference: String,
        values: Vec<String>,
    },
    Check {
        reference: String,
        checked: bool,
    },
    Scroll {
        x: i32,
        y: i32,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitState {
    Visible,
    Hidden,
    Attached,
    Detached,
}

impl Control {
    pub fn observation(&self) -> bool {
        matches!(
            self,
            Self::Observe | Self::Screenshot | Self::Console | Self::Network | Self::Wait { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operation {
    State,
    Action(Action),
    Control {
        document: Document,
        revision: u64,
        control: Control,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    Success {
        value: Value,
    },
    Failed {
        error: String,
    },
    /// The controller may have performed a write. Never retry automatically.
    Uncertain {
        error: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub state: State,
    pub revision: u64,
    pub value: Option<Value>,
}

pub(crate) struct Request {
    pub thread: ThreadId,
    pub operation: Operation,
    pub reply: mpsc::SyncSender<Result<Response, String>>,
}

#[derive(Clone)]
pub struct Client(Arc<Mutex<mpsc::Sender<Request>>>);

/// A provider gets this bound handle, rather than accepting a Thread id from
/// page content or a tool argument. #194 supplies it to provider adapters.
#[derive(Clone)]
pub struct ThreadClient {
    client: Client,
    thread: ThreadId,
}
impl Client {
    pub fn for_thread(&self, thread: ThreadId) -> ThreadClient {
        ThreadClient {
            client: self.clone(),
            thread,
        }
    }
}
impl ThreadClient {
    /// Nonblocking, like the visual renderer seam. The consumer must use a
    /// bounded wait; a disconnected server drops the reply immediately.
    pub fn submit(&self, operation: Operation) -> mpsc::Receiver<Result<Response, String>> {
        let (reply, answer) = mpsc::sync_channel(1);
        let _ = self
            .client
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .send(Request {
                thread: self.thread,
                operation,
                reply,
            });
        answer
    }
}

pub(crate) struct Requests(pub mpsc::Receiver<Request>);
impl Default for Requests {
    fn default() -> Self {
        channel().1
    }
}
pub(crate) fn channel() -> (Client, Requests) {
    let (send, receive) = mpsc::channel();
    (Client(Arc::new(Mutex::new(send))), Requests(receive))
}
