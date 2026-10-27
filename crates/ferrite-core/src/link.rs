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
//! reconnects. What is too large to queue — a Thread's history — is read on
//! a thread of its own ([`Deferred`], a few at once) and waits in the queue
//! as its pieces arrive: what that Thread is sent after it waits for it,
//! every other Thread's messages go past, and histories take turns.
//! A reconnecting client resumes each Thread from the [`Ring`], the last
//! [`RING_BYTES`] of its inputs, or rebuilds it from history.

use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
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
    /// The last thing sent was a history's piece: a waiting message goes
    /// next, so neither starves the other.
    piece_last: bool,
    lagged: bool,
    /// The Server let the connection go: the writer stops.
    closed: bool,
    /// The writer thread has ended.
    finished: bool,
}

/// A Thread's history, made on a thread of its own: read from its log and
/// handed over in pieces, each message in turn. What it sends never counts
/// against [`LAGGED_BYTES`].
pub type Deferred =
    Box<dyn FnOnce(&mut dyn FnMut(ServerMessage) -> io::Result<()>) -> io::Result<()> + Send>;

/// What waits to be sent.
enum Entry {
    Message(ServerMessage),
    History(History),
}

/// A history being read: its pieces as they arrive, the one taken but not
/// yet sent, and how many it has sent (histories take turns by it).
struct History {
    thread: ThreadId,
    pieces: Receiver<ServerMessage>,
    ready: Option<ServerMessage>,
    sent: u64,
}

/// Where a history being read stands.
enum Reading {
    /// A piece has arrived to send.
    Ready,
    /// Its next piece is still being read.
    Waiting,
    /// It has sent its last.
    Done,
}

impl History {
    fn reading(&mut self) -> Reading {
        if self.ready.is_none() {
            match self.pieces.try_recv() {
                Ok(piece) => self.ready = Some(piece),
                Err(TryRecvError::Empty) => return Reading::Waiting,
                Err(TryRecvError::Disconnected) => return Reading::Done,
            }
        }
        Reading::Ready
    }
}

struct Waiting {
    entry: Entry,
    bytes: usize,
}

impl Waiting {
    fn message(&self) -> Option<&ServerMessage> {
        match &self.entry {
            Entry::Message(message) => Some(message),
            Entry::History(_) => None,
        }
    }
}

/// The Thread a message is of, when it must follow that Thread's history:
/// what a mirror applies in order.
fn after_history(message: &ServerMessage) -> Option<ThreadId> {
    match message {
        ServerMessage::Input { thread, .. }
        | ServerMessage::ToolDraft { thread, .. }
        | ServerMessage::History { thread, .. }
        | ServerMessage::HistoryEnd { thread, .. } => Some(*thread),
        _ => None,
    }
}

/// How many pieces of a history wait to be sent at most, read ahead.
const HISTORY_PIECES_AHEAD: usize = 2;

/// How many histories are read at once, across every connection: each
/// holds its log in memory while it reads.
const HISTORY_READERS: usize = 4;

/// The history readers at work.
static READERS: Readers = Readers {
    reading: Mutex::new(0),
    freed: Condvar::new(),
};

struct Readers {
    reading: Mutex<usize>,
    freed: Condvar,
}

impl Readers {
    /// A reader's place, waited for while `HISTORY_READERS` read.
    fn take(&'static self) -> Reader {
        let mut reading = self.reading.lock().unwrap_or_else(|e| e.into_inner());
        while *reading >= HISTORY_READERS {
            reading = self.freed.wait(reading).unwrap_or_else(|e| e.into_inner());
        }
        *reading += 1;
        Reader(self)
    }
}

/// A reader's place, given back as it drops.
struct Reader(&'static Readers);

impl Drop for Reader {
    fn drop(&mut self) {
        *self.0.reading.lock().unwrap_or_else(|e| e.into_inner()) -= 1;
        self.0.freed.notify_one();
    }
}

/// Wakes a connection's writer: on each piece of a history, and as the
/// history ends however it ends.
struct Wake(Arc<(Mutex<Outbox>, Condvar)>);

impl Wake {
    fn wake(&self) {
        let (lock, ready) = &*self.0;
        drop(lock.lock().unwrap_or_else(|e| e.into_inner()));
        ready.notify_all();
    }
}

impl Drop for Wake {
    fn drop(&mut self) {
        self.wake();
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
    Digest(ThreadId),
    Roster,
    Groups,
    Registry,
    Notices,
    Providers,
    Parked,
}

fn slot(message: &ServerMessage) -> Option<Slot> {
    Some(match message {
        ServerMessage::Thread { state, .. } => Slot::Thread(state.thread),
        ServerMessage::ToolDraft { thread, id, .. } => Slot::ToolDraft(*thread, id.clone()),
        ServerMessage::Digest { thread, .. } => Slot::Digest(*thread),
        ServerMessage::Roster { .. } => Slot::Roster,
        ServerMessage::Groups { .. } => Slot::Groups,
        ServerMessage::Registry { .. } => Slot::Registry,
        ServerMessage::Notices { .. } => Slot::Notices,
        ServerMessage::Providers { .. } => Slot::Providers,
        ServerMessage::Parked { .. } => Slot::Parked,
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
            entry: Entry::Message(last),
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
            entry: Entry::Message(message),
            bytes,
        });
        self.check()
    }

    /// Queue `thread`'s history, its pieces arriving on `pieces` until it
    /// closes: in order for that Thread, but outside the lag limit.
    pub fn history(
        &mut self,
        thread: ThreadId,
        pieces: Receiver<ServerMessage>,
    ) -> Result<(), Lagged> {
        if self.lagged {
            return Err(Lagged);
        }
        self.queue.push_back(Waiting {
            entry: Entry::History(History {
                thread,
                pieces,
                ready: None,
                sent: 0,
            }),
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

    /// The next message to send, if one can go now: the first waiting
    /// message no unfinished history of its Thread holds back, or a piece
    /// from the history that has sent the fewest, turn about. A Thread's
    /// histories go one at a time, in order. `None` while everything waits
    /// on a history still being read.
    pub fn pop(&mut self) -> Option<ServerMessage> {
        let mut held: Vec<ThreadId> = Vec::new();
        let mut message: Option<usize> = None;
        let mut turn: Option<(u64, usize)> = None;
        let mut index = 0;
        while index < self.queue.len() {
            match &mut self.queue[index].entry {
                Entry::History(history) if held.contains(&history.thread) => {}
                Entry::History(history) => {
                    held.push(history.thread);
                    match history.reading() {
                        Reading::Done => {
                            held.pop();
                            self.queue.remove(index);
                            continue;
                        }
                        Reading::Ready if turn.is_none_or(|(sent, _)| history.sent < sent) => {
                            turn = Some((history.sent, index));
                        }
                        Reading::Ready | Reading::Waiting => {}
                    }
                }
                Entry::Message(waiting) => {
                    if message.is_none()
                        && after_history(waiting).is_none_or(|thread| !held.contains(&thread))
                    {
                        message = Some(index);
                    }
                }
            }
            index += 1;
        }
        match (message, turn) {
            (Some(index), turn) if self.piece_last || turn.is_none() => {
                let waiting = self.queue.remove(index).expect("in the queue");
                self.bytes -= waiting.bytes;
                self.piece_last = false;
                match waiting.entry {
                    Entry::Message(message) => Some(message),
                    Entry::History(_) => unreachable!("a message was found here"),
                }
            }
            (_, Some((_, index))) => {
                let Entry::History(history) = &mut self.queue[index].entry else {
                    unreachable!("a history was found here")
                };
                history.sent += 1;
                self.piece_last = true;
                history.ready.take()
            }
            (None, None) => None,
            (Some(_), None) => unreachable!("matched above"),
        }
    }

    /// Whether nothing waits to be sent, history or message.
    fn is_empty(&self) -> bool {
        self.queue.is_empty()
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
    /// An automation API connection's (ADR 0027), on its own socket.
    Api(crate::api::Heard),
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
                crate::spawn::serve_the_operator();
                let (lock, ready) = &*outbox;
                loop {
                    let next = {
                        let mut waiting = lock.lock().unwrap_or_else(|e| e.into_inner());
                        loop {
                            if let Some(next) = waiting.pop() {
                                break Some(next);
                            }
                            if waiting.lagged() {
                                break Some(lagged());
                            }
                            if waiting.closed && waiting.is_empty() {
                                break None;
                            }
                            waiting = ready.wait(waiting).unwrap_or_else(|e| e.into_inner());
                        }
                    };
                    // Whether to keep sending.
                    let sent = match next {
                        None => Ok(false),
                        Some(message) => {
                            let closing = matches!(
                                message,
                                ServerMessage::Welcome { error: Some(_), .. }
                            ) || matches!(&message, ServerMessage::Reply { error: Some(error), .. }
                                        if error.code == ErrorCode::Lagged);
                            send(&mut writer, &message).map(|()| !closing)
                        }
                    };
                    if !matches!(sent, Ok(true)) {
                        break;
                    }
                }
                let _ = writer.shutdown();
                let mut waiting = lock.lock().unwrap_or_else(|e| e.into_inner());
                // Nothing more is sent: a history still being read stops at
                // its next piece.
                waiting.queue.clear();
                waiting.finished = true;
                drop(waiting);
                ready.notify_all();
            })?;
        if heard.send(Heard::Joined(id, Self { shared })).is_err() {
            return Ok(());
        }
        std::thread::Builder::new()
            .name(format!("ferrite-client-{id}-in"))
            .spawn(move || {
                crate::spawn::serve_the_operator();
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

    /// Read `thread`'s history on a thread of its own ([`Deferred`]),
    /// queueing its pieces as they come: a few at most, so a slow client
    /// holds the reading back. At most `HISTORY_READERS` read at once.
    pub fn history(&self, thread: ThreadId, read: Deferred) -> Result<(), Lagged> {
        let (pieces, history) = mpsc::sync_channel(HISTORY_PIECES_AHEAD);
        self.queue(|outbox| outbox.history(thread, history))?;
        let wake = Wake(self.shared.clone());
        let spawned = std::thread::Builder::new()
            .name(format!("ferrite-history-{thread}"))
            .spawn(move || {
                crate::spawn::serve_the_operator();
                let _reading = READERS.take();
                let _ = read(&mut |message| {
                    pieces
                        .send(message)
                        .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
                    wake.wake();
                    Ok(())
                });
                // `wake` wakes the writer as it drops, the history ended.
            });
        if let Err(error) = spawned {
            crate::log!("thread {thread}: its history could not be read: {error}");
        }
        Ok(())
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
        let (deliver, messages) = crate::doorbell::channel();
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
