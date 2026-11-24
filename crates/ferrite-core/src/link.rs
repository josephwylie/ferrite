//! The Server's transport (ADR 0025): one JSON message per line over a
//! per-user socket — a Unix socket beside the Store, owner-only, or on
//! Windows an owner-only named pipe — with blocking I/O and a thread per
//! connection direction, like the rest of core. Here are the framing, the
//! socket and a window's end. The Server's end of each connection is the
//! engine's `connection` module.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::protocol::{self, ClientMessage, Command, Done, Op, ServerMessage};

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
    capture_out: Option<mpsc::Sender<ClientMessage>>,
    pub messages: Receiver<ServerMessage>,
}

/// What attaching found: the Server's welcome, its clock as the welcome
/// read against this side's, and its snapshot.
pub struct Attached {
    pub client: Client,
    pub welcome: ServerMessage,
    pub clock: crate::clock::ServerClock,
    pub snapshot: protocol::Snapshot,
}

impl Client {
    /// Connect, say hello, and read the welcome and snapshot. A refused
    /// welcome is an error carrying the Server's reason.
    pub fn attach(address: &Path, hello: ClientMessage) -> io::Result<Attached> {
        let wall_ms = || crate::clock::now().timestamp_millis();
        // Before connecting: a Server that refuses the generation closes
        // as soon as it has welcomed, so the hello follows the connect at once.
        let requested_ms = wall_ms();
        let mut stream = connect(address)?;
        send(&mut stream, &hello)?;
        let mut lines = Lines::new(stream.try_clone()?);
        let welcome: ServerMessage = lines
            .next_message()?
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "no welcome"))?;
        let received_ms = wall_ms();
        let server_ms = match &welcome {
            ServerMessage::Welcome {
                error: Some(error), ..
            } => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    error.message.clone(),
                ))
            }
            ServerMessage::Welcome { now_ms, .. } => *now_ms,
            _ => None,
        };
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
                capture_out: None,
                messages,
            },
            welcome,
            clock: crate::clock::ServerClock::sampled(server_ms, requested_ms, received_ms),
            snapshot,
        })
    }

    pub fn send(&mut self, message: &ClientMessage) -> io::Result<()> {
        if self.capture_out.is_some() {
            return self.enqueue_capture_stream(message.clone());
        }
        send(&mut self.writer, message)
    }

    /// Send `op` as command `id` and wait up to `patience` for its reply:
    /// for a connection of its own that carries one kind of act, such as a
    /// transfer's.
    pub fn ask(&mut self, id: u64, op: Op, patience: Duration) -> io::Result<Option<Done>> {
        self.send(&ClientMessage::Command(Command { id, op }))?;
        self.answer(id, patience)
    }

    /// Wait up to `patience` for the reply to command `id`, passing over
    /// whatever else the Server says.
    pub fn answer(&mut self, id: u64, patience: Duration) -> io::Result<Option<Done>> {
        let deadline = Instant::now() + patience;
        loop {
            match self
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(ServerMessage::Reply { id: answered, ok, error }) if answered == id => {
                    return match error {
                        Some(error) => Err(error.into()),
                        None => Ok(ok),
                    };
                }
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "The server did not answer in time",
                    ))
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::new(
                        io::ErrorKind::NotConnected,
                        "The connection to the server closed",
                    ))
                }
            }
        }
    }

    /// Start an ordered writer for large captures. Once started, all later
    /// messages use it: enqueueing them never waits for a socket upload or
    /// overtakes an earlier capture. It has no bound: asks do not wait for
    /// their replies (ADR 0041), so a beat can queue one for every input of
    /// a gesture, and an ask the Server never reads times out by itself.
    /// Ownership remains this connection's.
    pub fn send_capture(&mut self, message: ClientMessage) -> io::Result<()> {
        if !matches!(
            &message,
            ClientMessage::Command(protocol::Command {
                op: protocol::Op::BrowserCapture { .. },
                ..
            })
        ) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected a retained browser capture",
            ));
        }
        if self.capture_out.is_none() {
            let mut writer = self.writer.try_clone()?;
            let (queue, pending) = mpsc::channel::<ClientMessage>();
            std::thread::Builder::new()
                .name("ferrite-capture-out".into())
                .spawn(move || {
                    while let Ok(message) = pending.recv() {
                        if send(&mut writer, &message).is_err() {
                            // Wake the normal incoming reader/detach path. That
                            // drops outstanding replies rather than stranding a
                            // capture lease while its window waits for an ACK.
                            let _ = writer.shutdown();
                            return;
                        }
                    }
                })?;
            self.capture_out = Some(queue);
        }
        self.enqueue_capture_stream(message)
    }

    fn enqueue_capture_stream(&self, message: ClientMessage) -> io::Result<()> {
        self.capture_out
            .as_ref()
            .unwrap()
            .send(message)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "the browser capture upload connection is unavailable",
                )
            })
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.writer.shutdown();
    }
}

#[cfg(test)]
mod tests;
