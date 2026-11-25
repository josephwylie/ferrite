//! One connection's tree ops (ADR 0048), run on a thread of their own in
//! the order they were asked: a sender asks ahead of the replies, and a
//! write's chunks must land in order.

use std::io;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};

use ferrite_core::chunked::{Cancellation, EXPIRY};
use ferrite_engine::connection::{ClientId, Heard};
use ferrite_core::protocol::{Done, ErrorCode, WireError};
use ferrite_core::tree::{Request, Session};

use super::failed;

/// At most this many connections send trees at once.
pub(super) const MOST: usize = 16;

/// Dropped as its connection goes: what it was asked and has not begun is
/// never run, and the files it was writing are dropped with their
/// temporary files.
pub(super) struct Lane {
    asked: Sender<(u64, Request)>,
    gone: Cancellation,
}

impl Lane {
    /// Each answer goes to the pump on `heard`.
    pub(super) fn open(client: ClientId, heard: Sender<Heard>) -> Self {
        let (asked, asks) = mpsc::channel::<(u64, Request)>();
        let gone = Cancellation::default();
        let going = gone.clone();
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
            let mut session = Session::installing(ferrite_engine::providers::claude_plugin_installs());
            loop {
                match asks.recv_timeout(EXPIRY) {
                    Ok((request, operation)) => {
                        if going.check().is_err() {
                            return;
                        }
                        let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            session.execute(operation)
                        }))
                        .unwrap_or_else(|_| Err(io::Error::other("the tree op ended")))
                        .map(|reply| Some(Done::Tree { reply }))
                        .map_err(refused);
                        if heard.send(Heard::Answered(client, request, answer)).is_err() {
                            return;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => session.expire(),
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        });
        Self { asked, gone }
    }

    pub(super) fn ask(&self, request: u64, operation: Request) {
        let _ = self.asked.send((request, operation));
    }
}

impl Drop for Lane {
    fn drop(&mut self) {
        self.gone.cancel();
    }
}

/// `failed`, but what this Server does not do says so: `unsupported`.
fn refused(error: io::Error) -> WireError {
    match error.kind() {
        io::ErrorKind::Unsupported => WireError {
            code: ErrorCode::Unsupported,
            message: error.to_string(),
        },
        _ => failed(error),
    }
}
