//! A read answered on another thread (a checkout's git, a forge's `gh`),
//! awaited on the window's executor without holding the UI thread.

use std::io;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

/// How often a read on another thread is looked at for its answer.
pub const POLL: Duration = Duration::from_millis(30);

/// The answer to a read on another thread (`Cockpit::review_diff`,
/// `Cockpit::pull_request`), awaited without holding the UI thread.
pub async fn answered<T>(answer: io::Result<Receiver<io::Result<T>>>, cx: &gpui::AsyncApp) -> io::Result<T> {
    let answer = answer?;
    loop {
        cx.background_executor().timer(POLL).await;
        match answer.try_recv() {
            Ok(read) => return read,
            Err(TryRecvError::Empty) => continue,
            Err(TryRecvError::Disconnected) => return Err(io::Error::other("the read ended")),
        }
    }
}
