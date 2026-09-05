//! Session ownership from nonblocking startup to cancellation. A starting
//! Session cannot accept prompts: only a ready provider adapter can deliver.
//! Dropping the owner cancels adoption; the worker drops any late process.

use std::io;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::providers::Session;

enum State {
    Starting(Receiver<io::Result<Box<dyn Session + Send>>>),
    Ready(Box<dyn Session>),
    Failed,
}

pub struct SessionLifecycle {
    state: State,
}

impl SessionLifecycle {
    pub fn ready(session: Box<dyn Session>) -> Self {
        Self {
            state: State::Ready(session),
        }
    }

    /// Start a provider off the caller's thread. This worker never sends a
    /// prompt, forwards events, or owns Thread policy. It exits as soon as
    /// startup returns, including when the owner has already been dropped.
    pub fn background(
        spawn: impl FnOnce() -> io::Result<Box<dyn Session + Send>> + Send + 'static,
    ) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("ferrite-spawn".into())
            .spawn(move || {
                let _ = tx.send(spawn());
            })?;
        Ok(Self {
            state: State::Starting(rx),
        })
    }

    /// Poll once, without blocking. Errors include a panicked startup worker.
    /// Success means a provider is ready; false means startup is still pending.
    pub fn poll(&mut self) -> io::Result<bool> {
        let State::Starting(rx) = &self.state else {
            return match self.state {
                State::Ready(_) => Ok(true),
                _ => Err(io::Error::other("Session startup failed")),
            };
        };
        let result = match rx.try_recv() {
            Ok(result) => result.map(|session| session as Box<dyn Session>),
            Err(TryRecvError::Empty) => return Ok(false),
            Err(TryRecvError::Disconnected) => {
                Err(io::Error::other("Session startup worker stopped"))
            }
        };
        self.state = State::Failed;
        self.state = State::Ready(result?);
        Ok(true)
    }

    pub fn is_starting(&self) -> bool {
        matches!(self.state, State::Starting(_))
    }

    pub fn session(&self) -> Option<&dyn Session> {
        match &self.state {
            State::Ready(session) => Some(session.as_ref()),
            _ => None,
        }
    }

    pub fn session_mut(&mut self) -> Option<&mut (dyn Session + 'static)> {
        match &mut self.state {
            State::Ready(session) => Some(session.as_mut()),
            _ => None,
        }
    }
}
