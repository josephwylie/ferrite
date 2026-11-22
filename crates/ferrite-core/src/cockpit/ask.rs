//! A Server ask on its way (ADR 0041). An ask returns at once; its reply
//! comes back the way every Server message does: the socket's reader rings
//! the window's bell, the pump reads the reply and routes it by its
//! command's id to the ask's channel, and the model says `Arrived`. An ask
//! the Server has not answered when its time is up is answered that it did
//! not answer in time, by the same pump; one whose link went is answered
//! so. An Engine in this process, a fixture or a send that could not go
//! answers at once, through the same `Ask`, so a caller has one path.

use std::io;
use std::sync::mpsc::{Receiver, TryRecvError};

/// A reply to come, taken once with `answer`.
#[must_use = "an ask's answer is the only word of how it went"]
pub struct Ask<T> {
    answer: Receiver<io::Result<T>>,
}

impl<T> Ask<T> {
    /// The answer `answer` will carry.
    pub(crate) fn of(answer: Receiver<io::Result<T>>) -> Self {
        Self { answer }
    }

    /// Answered already. It rings the bell, so the pump that holds it looks.
    pub(crate) fn answered(result: io::Result<T>) -> Self {
        let (answer, answered) = crate::doorbell::channel();
        let _ = answer.send(result);
        Self { answer: answered }
    }

    /// The answer, once it came; `None` while it is on its way.
    pub fn answer(&mut self) -> Option<io::Result<T>> {
        match self.answer.try_recv() {
            Ok(answer) => Some(answer),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "the Ferrite server went away; this may not have arrived",
            ))),
        }
    }
}
