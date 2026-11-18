//! The private browser controller's sockets: a loopback WebSocket listener
//! that admits up to eight authorized peers and turns their traffic into
//! [`Incoming`] messages for the engine to take on its own thread.

use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use serde_json::Value;
use tungstenite::handshake::server::{Request, Response};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{accept_hdr_with_config, Error, Message};

use super::ControllerEndpoint;

pub(super) const LIMIT: usize = 8 * 1024 * 1024;

/// One peer's way back out: messages queued here are written to its socket.
pub(super) struct Output {
    send: mpsc::SyncSender<String>,
    stop: Arc<AtomicBool>,
    bytes: Arc<AtomicUsize>,
}

impl Output {
    pub(super) fn emit(&self, value: Value) {
        // Slow readers lose the connection rather than blocking the CEF UI thread.
        let text = value.to_string();
        let length = text.len();
        if self.bytes.fetch_add(length, Ordering::Relaxed) + length > 32 * 1024 * 1024 {
            self.bytes.fetch_sub(length, Ordering::Relaxed);
            self.close();
        } else if self.send.try_send(text).is_err() {
            self.bytes.fetch_sub(length, Ordering::Relaxed);
            self.close();
        }
    }

    pub(super) fn close(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub(super) fn is_closed(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

/// What reaches the engine. Peers send the first three; `Page` is the engine's
/// own, queued through [`Peers::sender`] so it shares the same bound.
pub(super) enum Incoming {
    Open(u64, Output),
    Command(u64, Value),
    Closed(u64),
    Page(String, Value),
}

/// The listening controller. Dropping it stops the listener and every peer.
pub(super) struct Peers {
    endpoint: ControllerEndpoint,
    stop: Arc<AtomicBool>,
    receive: mpsc::Receiver<Incoming>,
    send: mpsc::SyncSender<Incoming>,
}

impl Peers {
    pub(super) fn start() -> Result<Self, String> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .map_err(|e| format!("Private browser controller: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).map_err(|e| format!("Controller entropy: {e}"))?;
        let token: String = secret.iter().map(|byte| format!("{byte:02x}")).collect();
        let endpoint = ControllerEndpoint {
            url: format!("ws://{address}/ferrite"),
            authorization: format!("Bearer {token}"),
        };
        let authorization = endpoint.authorization.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let (send, receive) = mpsc::sync_channel(32);
        let thread_stop = stop.clone();
        let thread_send = send.clone();
        std::thread::Builder::new()
            .name("browser-controller".into())
            .spawn(move || {
                let mut next = 0u64;
                // Bound peers, including unauthenticated clients holding an upgrade.
                let active = Arc::new(AtomicUsize::new(0));
                while !thread_stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if active.load(Ordering::Relaxed) >= 8 {
                                eprintln!("ferrite browser controller: refused a peer, eight are connected");
                                continue;
                            }
                            // An accepted socket inherits the listener's non-blocking mode on
                            // macOS and Windows, and `serve`'s timeouts only hold on a blocking one.
                            if let Err(error) = stream.set_nonblocking(false) {
                                eprintln!("ferrite browser controller: dropped a peer, {error}");
                                continue;
                            }
                            active.fetch_add(1, Ordering::Relaxed);
                            next += 1;
                            let id = next;
                            let send = thread_send.clone();
                            let stop = thread_stop.clone();
                            let authorization = authorization.clone();
                            let active = active.clone();
                            std::thread::spawn(move || {
                                serve(stream, id, authorization, send, stop);
                                active.fetch_sub(1, Ordering::Relaxed);
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            endpoint,
            stop,
            receive,
            send,
        })
    }

    pub(super) fn endpoint(&self) -> ControllerEndpoint {
        self.endpoint.clone()
    }

    pub(super) fn sender(&self) -> mpsc::SyncSender<Incoming> {
        self.send.clone()
    }

    pub(super) fn try_recv(&self) -> Option<Incoming> {
        self.receive.try_recv().ok()
    }
}

impl Drop for Peers {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
static READ_TURNS: AtomicUsize = AtomicUsize::new(0);

fn serve(
    stream: TcpStream,
    id: u64,
    authorization: String,
    send: mpsc::SyncSender<Incoming>,
    stop: Arc<AtomicBool>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let config = WebSocketConfig {
        max_message_size: Some(LIMIT),
        max_frame_size: Some(LIMIT),
        ..Default::default()
    };
    let socket = accept_hdr_with_config(
        stream,
        |request: &Request, response: Response| {
            // Browsers must never be able to use cookies, query-string credentials,
            // a forged local Origin or a websocket from page JS to authorize control.
            let authenticated = request.uri().path() == "/ferrite"
                && request.uri().query().is_none()
                && request.headers().get("authorization").is_some_and(|h| {
                    let actual = h.as_bytes();
                    let expected = authorization.as_bytes();
                    actual.len() == expected.len()
                        && actual
                            .iter()
                            .zip(expected)
                            .fold(0u8, |different, (a, b)| different | (a ^ b))
                            == 0
                })
                && !request.headers().contains_key("origin");
            if authenticated {
                Ok(response)
            } else {
                Err(tungstenite::http::Response::builder()
                    .status(403)
                    .body(Some("Forbidden".into()))
                    .unwrap())
            }
        },
        Some(config),
    );
    let mut socket = match socket {
        Ok(socket) => socket,
        Err(error) => {
            eprintln!("ferrite browser controller: peer {id} failed its upgrade, {error}");
            return;
        }
    };
    let _ = socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(10)));
    let (output, outgoing) = mpsc::sync_channel(32);
    let peer_stop = Arc::new(AtomicBool::new(false));
    let queued_bytes = Arc::new(AtomicUsize::new(0));
    let output = Output {
        send: output,
        stop: peer_stop.clone(),
        bytes: queued_bytes.clone(),
    };
    if send.try_send(Incoming::Open(id, output)).is_err() {
        eprintln!("ferrite browser controller: dropped peer {id}, the window is behind");
        return;
    }
    while !stop.load(Ordering::Relaxed) && !peer_stop.load(Ordering::Relaxed) {
        #[cfg(test)]
        READ_TURNS.fetch_add(1, Ordering::Relaxed);
        for _ in 0..64 {
            let Ok(value) = outgoing.try_recv() else {
                break;
            };
            queued_bytes.fetch_sub(value.len(), Ordering::Relaxed);
            if let Err(error) = socket.send(Message::Text(value)) {
                eprintln!("ferrite browser controller: closed peer {id}, send failed, {error}");
                peer_stop.store(true, Ordering::Relaxed);
                let _ = send.try_send(Incoming::Closed(id));
                return;
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                let Ok(value) = serde_json::from_str(&text) else {
                    eprintln!("ferrite browser controller: closed peer {id}, its message is not JSON");
                    break;
                };
                if send.try_send(Incoming::Command(id, value)).is_err() {
                    eprintln!("ferrite browser controller: closed peer {id}, the window is behind");
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(Error::ConnectionClosed | Error::AlreadyClosed) => break,
            Err(error) => {
                eprintln!("ferrite browser controller: closed peer {id}, read failed, {error}");
                break;
            }
        }
    }
    peer_stop.store(true, Ordering::Relaxed);
    let _ = socket.close(None);
    let _ = send.try_send(Incoming::Closed(id));
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::time::Instant;

    use serde_json::json;
    use tungstenite::protocol::Role;
    use tungstenite::WebSocket;

    use super::*;

    fn connect(peers: &Peers, delay: Duration) -> WebSocket<TcpStream> {
        let endpoint = peers.endpoint();
        let address = endpoint
            .url
            .strip_prefix("ws://")
            .and_then(|rest| rest.strip_suffix("/ferrite"))
            .unwrap()
            .to_owned();
        let mut stream = TcpStream::connect(&address).unwrap();
        std::thread::sleep(delay);
        write!(
            stream,
            "GET /ferrite HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\
             Authorization: {}\r\n\r\n",
            endpoint.authorization
        )
        .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut response = Vec::new();
        let mut byte = [0u8; 1];
        while !response.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(1) => response.push(byte[0]),
                other => panic!(
                    "upgrade response ended early with {other:?} after {:?}",
                    String::from_utf8_lossy(&response)
                ),
            }
        }
        let response = String::from_utf8_lossy(&response);
        assert!(
            response.starts_with("HTTP/1.1 101"),
            "expected a 101 upgrade, got {response:?}"
        );
        stream
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        WebSocket::from_raw_socket(stream, Role::Client, None)
    }

    fn read_text(socket: &mut WebSocket<TcpStream>) -> String {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match socket.read() {
                Ok(Message::Text(text)) => return text,
                Ok(_) => {}
                Err(Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => panic!("controller socket failed: {e}"),
            }
        }
        panic!("no message from the controller within 5 s");
    }

    fn next_incoming(peers: &Peers) -> Incoming {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(incoming) = peers.try_recv() {
                return incoming;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("nothing reached the engine within 5 s");
    }

    fn connected_peer(peers: &Peers) -> (u64, Output) {
        match next_incoming(peers) {
            Incoming::Open(id, output) => (id, output),
            _ => panic!("the first message from a new peer was not its opening"),
        }
    }

    #[test]
    fn a_client_that_sends_its_upgrade_late_still_connects() {
        let peers = Peers::start().unwrap();
        let mut socket = connect(&peers, Duration::from_millis(150));
        let (peer, output) = connected_peer(&peers);
        let command = json!({"id":1,"method":"Browser.getVersion"});
        socket.send(Message::Text(command.to_string())).unwrap();
        match next_incoming(&peers) {
            Incoming::Command(from, value) => {
                assert_eq!(from, peer);
                assert_eq!(value, command);
            }
            _ => panic!("the peer's command did not reach the engine"),
        }
        output.emit(json!({"id":1,"result":{}}));
        let reply: Value = serde_json::from_str(&read_text(&mut socket)).unwrap();
        assert_eq!(reply["id"], 1);
    }

    #[test]
    fn a_message_larger_than_the_socket_buffer_reaches_a_slow_reader_whole() {
        let peers = Peers::start().unwrap();
        let mut socket = connect(&peers, Duration::ZERO);
        let (_, output) = connected_peer(&peers);
        let blob = "x".repeat(4 * 1024 * 1024);
        output.emit(json!({"blob": blob}));
        std::thread::sleep(Duration::from_millis(300));
        let message: Value = serde_json::from_str(&read_text(&mut socket)).unwrap();
        assert_eq!(message["blob"].as_str().map(str::len), Some(blob.len()));
    }

    #[test]
    fn an_idle_peer_waits_for_input_instead_of_spinning() {
        let peers = Peers::start().unwrap();
        let _socket = connect(&peers, Duration::ZERO);
        let _peer = connected_peer(&peers);
        std::thread::sleep(Duration::from_millis(50));
        let before = READ_TURNS.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(200));
        let turns = READ_TURNS.load(Ordering::Relaxed) - before;
        // A 10 ms read timeout allows about 20 turns in 200 ms.
        assert!(turns <= 40, "the idle peer's read loop turned {turns} times in 200 ms");
    }
}
