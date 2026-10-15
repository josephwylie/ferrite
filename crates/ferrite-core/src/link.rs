//! The Server's transport (ADR 0025): one JSON message per line over a
//! per-user socket — a Unix socket beside the Store, owner-only, or on
//! Windows an owner-only named pipe — with blocking I/O and a thread per
//! connection direction, like the rest of core.
//!
//! What a connection is sent waits in its [`Outbox`]: one ordered queue
//! that never drops. Adjacent deltas of one Thread merge while they wait;
//! a state message (a Thread's state, the Roster, the Groups …) supersedes
//! the one of its kind still waiting and joins the back; and a client that
//! falls more than [`LAGGED_BYTES`] behind is closed `lagged` and
//! reconnects. What is too large to queue — a Thread's history — waits as a
//! [`Deferred`] that the writer thread produces as it sends.
//! A reconnecting client resumes each Thread from the [`Ring`], the last
//! [`RING_BYTES`] of its inputs, or rebuilds it from history.

use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use serde::Serialize;

use crate::protocol::{self, ClientMessage, ErrorCode, ServerMessage, WireInput, WireRecord};
use crate::ThreadId;

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{connect, listen, Listener, Stream};
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{connect, listen, Listener, Stream};

/// A line longer than this is not a message: the peer is not speaking
/// the protocol, and the connection closes.
pub const MAX_LINE: usize = 64 << 20;

/// How far a client may fall behind before it is closed `lagged`.
pub const LAGGED_BYTES: usize = 32 << 20;

/// How much of each open Thread's recent inputs a resume can replay.
pub const RING_BYTES: usize = 8 << 20;

/// Where the Server listens for the Store in `store_dir`: beside it, so a
/// test's or a second install's Store never meets the operator's Server.
pub fn address(store_dir: &Path) -> PathBuf {
    store_dir.join("server.sock")
}

// ------------------------------------------------------------- framing

/// Messages, one per line, off a byte stream.
pub struct Lines<R> {
    reader: BufReader<R>,
}

impl<R: Read> Lines<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
        }
    }

    /// The next line without its newline, or `None` at the end of the
    /// stream.
    pub fn next_line(&mut self) -> io::Result<Option<Vec<u8>>> {
        let mut line = Vec::new();
        let read = (&mut self.reader)
            .take(MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if read == 0 {
            return Ok(None);
        }
        if line.pop() != Some(b'\n') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                if line.len() >= MAX_LINE {
                    "a line past the protocol's limit"
                } else {
                    "the stream ended inside a line"
                },
            ));
        }
        Ok(Some(line))
    }

    /// The next message, skipping none: a line that is not one is an error.
    pub fn next_message<T: serde::de::DeserializeOwned>(&mut self) -> io::Result<Option<T>> {
        match self.next_line()? {
            Some(line) => protocol::decode(&line)
                .map(Some)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            None => Ok(None),
        }
    }
}

/// Write one message as its line.
pub fn send<W: Write, T: Serialize>(writer: &mut W, message: &T) -> io::Result<()> {
    writer.write_all(&protocol::encode(message))?;
    writer.flush()
}

/// Copy a byte stream as it arrives, until it ends: the protocol is lines,
/// and a line must not wait for a buffer to fill. What bridges carry.
pub fn relay(from: &mut dyn Read, to: &mut dyn Write) -> io::Result<()> {
    let mut buffer = vec![0u8; 64 << 10];
    loop {
        let read = from.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        to.write_all(&buffer[..read])?;
        to.flush()?;
    }
}

// -------------------------------------------------------------- outbox

/// What a connection has yet to be sent.
#[derive(Default)]
pub struct Outbox {
    queue: VecDeque<Waiting>,
    bytes: usize,
    lagged: bool,
    /// The Server let the connection go: the writer stops.
    closed: bool,
    /// The writer thread has ended.
    finished: bool,
}

/// Messages made only as they are sent, on the connection's writer thread:
/// a Thread's whole history, read from its log and sent in pieces. What it
/// sends never counts against [`LAGGED_BYTES`]; it is handed each message
/// to send in turn.
pub type Deferred =
    Box<dyn FnOnce(&mut dyn FnMut(ServerMessage) -> io::Result<()>) -> io::Result<()> + Send>;

/// What waits to be sent.
pub enum Next {
    Message(ServerMessage),
    Deferred(Deferred),
}

struct Waiting {
    next: Next,
    bytes: usize,
}

impl Waiting {
    fn message(&self) -> Option<&ServerMessage> {
        match &self.next {
            Next::Message(message) => Some(message),
            Next::Deferred(_) => None,
        }
    }
}

/// The connection fell more than [`LAGGED_BYTES`] behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lagged;

/// What a state message replaces: the waiting one of the same kind.
#[derive(PartialEq)]
enum Slot {
    Thread(ThreadId),
    ToolDraft(ThreadId, String),
    Roster,
    Groups,
    Registry,
    Notices,
    Providers,
}

fn slot(message: &ServerMessage) -> Option<Slot> {
    Some(match message {
        ServerMessage::Thread { state, .. } => Slot::Thread(state.thread),
        ServerMessage::ToolDraft { thread, id, .. } => Slot::ToolDraft(*thread, id.clone()),
        ServerMessage::Roster { .. } => Slot::Roster,
        ServerMessage::Groups { .. } => Slot::Groups,
        ServerMessage::Registry { .. } => Slot::Registry,
        ServerMessage::Notices { .. } => Slot::Notices,
        ServerMessage::Providers { .. } => Slot::Providers,
        _ => return None,
    })
}

impl Outbox {
    /// Queue a message: a state message drops its waiting kind and joins
    /// the back, so it never overtakes what was queued before it; a delta
    /// merges into the one before it; anything else waits its turn. `Err(Lagged)` once the client is too far behind — the
    /// connection then closes, and nothing more is queued.
    pub fn push(&mut self, message: ServerMessage) -> Result<(), Lagged> {
        if self.lagged {
            return Err(Lagged);
        }
        let bytes = protocol::encode(&message).len();
        if let Some(slot) = slot(&message) {
            if let Some(stale) = self
                .queue
                .iter()
                .position(|waiting| waiting.message().and_then(self::slot).as_ref() == Some(&slot))
            {
                let stale = self.queue.remove(stale).expect("found just now");
                self.bytes -= stale.bytes;
            }
        }
        if let Some(Waiting {
            next: Next::Message(last),
            bytes: last_bytes,
        }) = self.queue.back_mut()
        {
            if merge(last, &message) {
                let merged = protocol::encode(last).len();
                self.bytes = self.bytes - *last_bytes + merged;
                *last_bytes = merged;
                return self.check();
            }
        }
        self.bytes += bytes;
        self.queue.push_back(Waiting {
            next: Next::Message(message),
            bytes,
        });
        self.check()
    }

    /// Queue what the writer thread will make as it sends: in order with
    /// everything else, but outside the lag limit.
    pub fn defer(&mut self, deferred: Deferred) -> Result<(), Lagged> {
        if self.lagged {
            return Err(Lagged);
        }
        self.queue.push_back(Waiting {
            next: Next::Deferred(deferred),
            bytes: 0,
        });
        Ok(())
    }

    fn check(&mut self) -> Result<(), Lagged> {
        if self.bytes > LAGGED_BYTES {
            self.lagged = true;
            self.queue.clear();
            self.bytes = 0;
            return Err(Lagged);
        }
        Ok(())
    }

    pub fn pop(&mut self) -> Option<Next> {
        let waiting = self.queue.pop_front()?;
        self.bytes -= waiting.bytes;
        Some(waiting.next)
    }

    pub fn lagged(&self) -> bool {
        self.lagged
    }
}

/// Merge `next` into `last` when both are deltas of one Thread the log
/// would keep as one line. The merged input carries the later `seq`.
fn merge(last: &mut ServerMessage, next: &ServerMessage) -> bool {
    let (
        ServerMessage::Input {
            thread,
            seq,
            at_ms,
            input: WireInput::Record(WireRecord::Known(record)),
        },
        ServerMessage::Input {
            thread: next_thread,
            seq: next_seq,
            at_ms: next_at,
            input: WireInput::Record(WireRecord::Known(next_record)),
        },
    ) = (last, next)
    else {
        return false;
    };
    if thread != next_thread || !record.coalesce(next_record) {
        return false;
    }
    *seq = *next_seq;
    *at_ms = *next_at;
    true
}

// ---------------------------------------------------------------- ring

/// The last [`RING_BYTES`] of one Thread's inputs, by `seq`: what a
/// client reconnecting within this boot resumes from.
#[derive(Default)]
pub struct Ring {
    /// The `seq` the Thread's current Activity began at: a client behind
    /// it holds the Activity before, and must rebuild, not resume.
    floor: u64,
    entries: VecDeque<(u64, ServerMessage, usize)>,
    bytes: usize,
}

impl Ring {
    /// The ring of an Activity that began at `seq`.
    pub fn after(seq: u64) -> Self {
        Self {
            floor: seq,
            ..Self::default()
        }
    }

    pub fn push(&mut self, seq: u64, message: ServerMessage) {
        let bytes = protocol::encode(&message).len();
        self.entries.push_back((seq, message, bytes));
        self.bytes += bytes;
        while self.bytes > RING_BYTES {
            let Some((_, _, bytes)) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= bytes;
        }
    }

    /// Every input after `after`, or `None` when the ring no longer holds
    /// the one right after it, or `after` is from before the Activity began.
    pub fn since(&self, after: u64) -> Option<Vec<ServerMessage>> {
        if after < self.floor {
            return None;
        }
        let newest = self.entries.back().map_or(self.floor, |(seq, _, _)| *seq);
        if after >= newest {
            return (after == newest).then(Vec::new);
        }
        let oldest = self.entries.front().map(|(seq, _, _)| *seq)?;
        if oldest > after + 1 {
            return None;
        }
        Some(
            self.entries
                .iter()
                .filter(|(seq, _, _)| *seq > after)
                .map(|(_, message, _)| message.clone())
                .collect(),
        )
    }
}

// --------------------------------------------------------------- server

/// A client connection's number, for the replies and broadcasts that
/// name it.
pub type ClientId = u64;

/// What the Server hears from its connections, in arrival order.
pub enum Heard {
    /// A client connected: what to queue its messages on. Always heard
    /// before anything it says.
    Joined(ClientId, Connection),
    Message(ClientId, ClientMessage),
    /// The connection ended, or broke the protocol.
    Gone(ClientId),
}

/// One client's end on the Server: its outbox, drained by a writer
/// thread, and a reader thread that forwards what it says.
pub struct Connection {
    shared: Arc<(Mutex<Outbox>, Condvar)>,
}

impl Connection {
    /// Serve a stream: the Connection is handed to `heard` (the pump's
    /// channel), then its reader thread hands over each message the client
    /// sends, and its writer thread sends what is queued on it.
    pub fn serve<S>(id: ClientId, stream: S, heard: Sender<Heard>) -> io::Result<()>
    where
        S: Read + Write + Send + 'static + TryClone,
    {
        let shared = Arc::new((Mutex::new(Outbox::default()), Condvar::new()));
        let mut writer = stream.try_clone()?;
        let outbox = Arc::clone(&shared);
        std::thread::Builder::new()
            .name(format!("ferrite-client-{id}-out"))
            .spawn(move || {
                let (lock, ready) = &*outbox;
                loop {
                    let next = {
                        let mut waiting = lock.lock().unwrap_or_else(|e| e.into_inner());
                        loop {
                            if let Some(next) = waiting.pop() {
                                break Some(next);
                            }
                            if waiting.lagged() {
                                break Some(Next::Message(lagged()));
                            }
                            if waiting.closed {
                                break None;
                            }
                            waiting = ready.wait(waiting).unwrap_or_else(|e| e.into_inner());
                        }
                    };
                    // Whether to keep sending.
                    let sent = match next {
                        None => Ok(false),
                        Some(Next::Message(message)) => {
                            let closing = matches!(
                                message,
                                ServerMessage::Welcome { error: Some(_), .. }
                            ) || matches!(&message, ServerMessage::Reply { error: Some(error), .. }
                                        if error.code == ErrorCode::Lagged);
                            send(&mut writer, &message).map(|()| !closing)
                        }
                        Some(Next::Deferred(deferred)) => {
                            deferred(&mut |message| send(&mut writer, &message)).map(|()| true)
                        }
                    };
                    if !matches!(sent, Ok(true)) {
                        break;
                    }
                }
                let _ = writer.shutdown();
                lock.lock().unwrap_or_else(|e| e.into_inner()).finished = true;
                ready.notify_all();
            })?;
        if heard.send(Heard::Joined(id, Self { shared })).is_err() {
            return Ok(());
        }
        std::thread::Builder::new()
            .name(format!("ferrite-client-{id}-in"))
            .spawn(move || {
                let mut lines = Lines::new(stream);
                while let Ok(Some(message)) = lines.next_message::<ClientMessage>() {
                    if heard.send(Heard::Message(id, message)).is_err() {
                        return;
                    }
                }
                let _ = heard.send(Heard::Gone(id));
            })?;
        Ok(())
    }

    /// Queue a message for this client. `Err(Lagged)`: it is too far
    /// behind, and its connection is closing.
    pub fn send(&self, message: ServerMessage) -> Result<(), Lagged> {
        self.queue(|outbox| outbox.push(message))
    }

    /// Queue what the writer thread makes as it sends ([`Deferred`]).
    pub fn defer(&self, deferred: Deferred) -> Result<(), Lagged> {
        self.queue(|outbox| outbox.defer(deferred))
    }

    fn queue(&self, push: impl FnOnce(&mut Outbox) -> Result<(), Lagged>) -> Result<(), Lagged> {
        let (lock, ready) = &*self.shared;
        let pushed = push(&mut lock.lock().unwrap_or_else(|e| e.into_inner()));
        ready.notify_all();
        pushed
    }

    /// Let the client go, as dropping does, and wait until its writer has
    /// sent what was queued or `deadline` passes: a Server about to exit
    /// says its last words (`stopping`, `restarting`) first.
    pub fn finish(self, deadline: Instant) {
        let (lock, ready) = &*self.shared;
        let mut outbox = lock.lock().unwrap_or_else(|e| e.into_inner());
        outbox.closed = true;
        ready.notify_all();
        while !outbox.finished {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            outbox = ready
                .wait_timeout(outbox, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

/// The Server let the client go: its writer thread sends what is already
/// queued (a lagged client's last word), then closes the stream and ends,
/// which ends the reader too.
impl Drop for Connection {
    fn drop(&mut self) {
        let (lock, ready) = &*self.shared;
        lock.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        ready.notify_all();
    }
}

/// Serve every connection `listener` accepts, numbered from 1, for as
/// long as the pump listens.
pub fn accept_all(listener: Listener, heard: Sender<Heard>) -> io::Result<()> {
    std::thread::Builder::new()
        .name("ferrite-server-accept".into())
        .spawn(move || {
            for id in 1.. {
                match listener.accept() {
                    Ok(stream) => {
                        if let Err(error) = Connection::serve(id, stream, heard.clone()) {
                            crate::log!("server: a connection could not be served: {error}");
                        }
                    }
                    Err(error) => {
                        crate::log!("server: stopped accepting: {error}");
                        return;
                    }
                }
            }
        })?;
    Ok(())
}

/// The last thing a lagged client hears: why it is being closed.
fn lagged() -> ServerMessage {
    ServerMessage::Reply {
        id: 0,
        ok: None,
        error: Some(protocol::WireError {
            code: ErrorCode::Lagged,
            message: "this window fell too far behind its server".into(),
        }),
    }
}

/// A stream whose other half can be had for the second thread, and that
/// can be closed from either.
pub trait TryClone: Sized {
    fn try_clone(&self) -> io::Result<Self>;
    fn shutdown(&self) -> io::Result<()>;
}

// --------------------------------------------------------------- client

/// A window's end of the link: what it sends goes out at once; what the
/// Server says arrives on `messages`, read by its own thread.
pub struct Client {
    writer: Stream,
    pub messages: Receiver<ServerMessage>,
}

/// What attaching found: the Server's welcome and its snapshot.
pub struct Attached {
    pub client: Client,
    pub welcome: ServerMessage,
    pub snapshot: protocol::Snapshot,
}

impl Client {
    /// Connect, say hello, and read the welcome and snapshot. A refused
    /// welcome is an error carrying the Server's reason.
    pub fn attach(address: &Path, hello: ClientMessage) -> io::Result<Attached> {
        let mut stream = connect(address)?;
        send(&mut stream, &hello)?;
        let mut lines = Lines::new(stream.try_clone()?);
        let welcome: ServerMessage = lines
            .next_message()?
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "no welcome"))?;
        if let ServerMessage::Welcome {
            error: Some(error), ..
        } = &welcome
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                error.message.clone(),
            ));
        }
        let snapshot = match lines.next_message()? {
            Some(ServerMessage::Snapshot(snapshot)) => snapshot,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "the server sent no snapshot",
                ))
            }
        };
        let (deliver, messages) = mpsc::channel();
        std::thread::Builder::new()
            .name("ferrite-server-in".into())
            .spawn(move || {
                while let Ok(Some(message)) = lines.next_message::<ServerMessage>() {
                    if deliver.send(message).is_err() {
                        return;
                    }
                }
            })?;
        Ok(Attached {
            client: Self {
                writer: stream,
                messages,
            },
            welcome,
            snapshot,
        })
    }

    pub fn send(&mut self, message: &ClientMessage) -> io::Result<()> {
        send(&mut self.writer, message)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.writer.shutdown();
    }
}

#[cfg(test)]
mod tests;
