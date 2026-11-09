//! Private CDP adapter for explicitly registered external views. CEF's global
//! remote-debugging server is never enabled. Browser-level target discovery is
//! synthesized from our registry; page commands go to that browser's host.

use std::collections::{HashMap, HashSet};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use cef::*;
use serde_json::{json, Value};
use tungstenite::handshake::server::{Request, Response};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{accept_hdr_with_config, Error, Message};

use super::super::ControllerEndpoint;

const LIMIT: usize = 8 * 1024 * 1024;
struct Output {
    send: mpsc::SyncSender<String>,
    stop: Arc<AtomicBool>,
    bytes: Arc<std::sync::atomic::AtomicUsize>,
}

enum Incoming {
    Open(u64, Output),
    Command(u64, Value),
    Closed(u64),
    Page(String, Value),
}

struct Connection {
    output: Output,
    sessions: HashSet<String>,
    discover: bool,
}

struct Target {
    browser: Browser,
    // Keeping the registration alive is what keeps CEF delivering messages.
    _registration: Registration,
}

struct Pending {
    peer: u64,
    id: Value,
    session: String,
    since: Instant,
}

pub(super) struct Control {
    endpoint: ControllerEndpoint,
    stop: Arc<AtomicBool>,
    receive: mpsc::Receiver<Incoming>,
    send: mpsc::SyncSender<Incoming>,
    targets: HashMap<String, Target>,
    connections: HashMap<u64, Connection>,
    next_message: i32,
    pending: HashMap<i32, Pending>,
    overflow: Arc<AtomicBool>,
}

impl Control {
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
                let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                while !thread_stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if active.load(Ordering::Relaxed) >= 8 {
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
            targets: HashMap::new(),
            connections: HashMap::new(),
            next_message: 1,
            pending: HashMap::new(),
            overflow: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(super) fn endpoint(&self) -> ControllerEndpoint {
        self.endpoint.clone()
    }

    pub(super) fn register(&mut self, id: String, browser: Browser) {
        let Some(host) = browser.host() else { return };
        let mut observer = Observer::new(id.clone(), self.send.clone(), self.overflow.clone());
        let Some(registration) = host.add_dev_tools_message_observer(Some(&mut observer)) else {
            return;
        };
        self.targets.insert(
            id.clone(),
            Target {
                browser,
                _registration: registration,
            },
        );
        let info = self.info(&id);
        for connection in self.connections.values() {
            if connection.discover {
                emit(
                    &connection.output,
                    json!({"method":"Target.targetCreated","params":{"targetInfo":info}}),
                );
            }
        }
    }

    pub(super) fn retain(&mut self, ids: &HashSet<String>) {
        let removed: Vec<_> = self
            .targets
            .keys()
            .filter(|id| !ids.contains(*id))
            .cloned()
            .collect();
        for id in removed {
            self.targets.remove(&id);
            for connection in self.connections.values_mut() {
                if connection.sessions.remove(&id) {
                    emit(
                        &connection.output,
                        json!({"method":"Target.detachedFromTarget","params":{"sessionId":id,"targetId":id}}),
                    );
                }
                if connection.discover {
                    emit(
                        &connection.output,
                        json!({"method":"Target.targetDestroyed","params":{"targetId":id}}),
                    );
                }
            }
            let abandoned: Vec<_> = self
                .pending
                .iter()
                .filter(|(_, pending)| pending.session == id)
                .map(|(key, _)| *key)
                .collect();
            for key in abandoned {
                if let Some(pending) = self.pending.remove(&key) {
                    self.uncertain(pending, "Embedded target closed after dispatch; completion is uncertain. Inspect fresh page state before any retry.");
                }
            }
        }
    }

    fn info(&self, id: &str) -> Value {
        let url = self
            .targets
            .get(id)
            .and_then(|t| t.browser.main_frame())
            .map(|f| CefString::from(&f.url()).to_string())
            .unwrap_or_default();
        json!({"targetId":id,"type":"page","title":"Ferrite conversation browser","url":url,"attached":false,"canAccessOpener":false,"browserContextId":"ferrite-external"})
    }

    pub(super) fn pump(&mut self) {
        // Any lost observation/result invalidates the controller's picture of
        // the page. Disconnect; never continue using partially delivered state.
        let overflowed = self.overflow.swap(false, Ordering::Relaxed);
        if overflowed {
            for connection in self.connections.values() {
                connection.output.stop.store(true, Ordering::Relaxed);
            }
        }
        self.connections
            .retain(|_, connection| !connection.output.stop.load(Ordering::Relaxed));
        self.pending
            .retain(|_, pending| self.connections.contains_key(&pending.peer));
        let expired: Vec<_> = self
            .pending
            .iter()
            // Return before the pinned native CDP client's 30-second timeout,
            // preserving an explicit uncertain-completion result for callers.
            .filter(|(_, pending)| pending.since.elapsed() >= Duration::from_secs(25))
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            if let Some(pending) = self.pending.remove(&id) {
                self.uncertain(pending, "Embedded page command timed out after dispatch; completion is uncertain. Inspect fresh page state before any retry.");
            }
        }
        // Bound controller work so a client cannot starve rendering or input.
        for _ in 0..64 {
            let Ok(incoming) = self.receive.try_recv() else {
                break;
            };
            match incoming {
                Incoming::Open(id, output) => {
                    if overflowed || output.stop.load(Ordering::Relaxed) {
                        output.stop.store(true, Ordering::Relaxed);
                        continue;
                    }
                    self.connections.insert(
                        id,
                        Connection {
                            output,
                            sessions: HashSet::new(),
                            discover: false,
                        },
                    );
                }
                Incoming::Closed(id) => {
                    self.connections.remove(&id);
                    self.pending.retain(|_, pending| pending.peer != id);
                }
                Incoming::Command(id, value) => self.command(id, value),
                Incoming::Page(target, mut value) => {
                    if let Some(id) = value.get("id").and_then(Value::as_i64) {
                        let key = i32::try_from(id).ok();
                        if let Some(key) = key.filter(|key| {
                            self.pending
                                .get(key)
                                .is_some_and(|pending| pending.session == target)
                        }) {
                            if let Some(pending) = self.pending.remove(&key) {
                                value["id"] = pending.id;
                                value["sessionId"] = pending.session.into();
                                self.output(pending.peer, value);
                            }
                        }
                    } else {
                        value["sessionId"] = target.clone().into();
                        for connection in self.connections.values() {
                            if connection.sessions.contains(&target) {
                                emit(&connection.output, value.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    fn output(&self, peer: u64, value: Value) {
        if let Some(c) = self.connections.get(&peer) {
            emit(&c.output, value);
        }
    }
    fn error(&self, peer: u64, id: Value, message: &str) {
        self.output(
            peer,
            json!({"id":id,"error":{"code":-32000,"message":message}}),
        );
    }

    fn uncertain(&self, pending: Pending, message: &str) {
        self.output(pending.peer, json!({"id":pending.id,"sessionId":pending.session,
            "error":{"code":-32000,"message":message,"data":{"completion":"uncertain","retryable":false}}}));
    }

    fn command(&mut self, peer: u64, mut value: Value) {
        let id = value.get("id").cloned().unwrap_or(Value::Null);
        let Some(method) = value
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            return;
        };
        if !id.is_i64()
            || self.overflow.load(Ordering::Relaxed)
            || !self
                .connections
                .get(&peer)
                .is_some_and(|connection| !connection.output.stop.load(Ordering::Relaxed))
        {
            // A queued command must not run after this peer disconnected or
            // after an observation was lost earlier in the same pump.
            return;
        }
        let params = value.get("params").cloned().unwrap_or(json!({}));
        let session = value
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(session) = session {
            if !self.connections[&peer].sessions.contains(&session)
                || !self.targets.contains_key(&session)
            {
                self.error(peer, id, "Unknown embedded target session");
                return;
            }
            // Never forward browser-wide/target-management commands. CEF's page
            // observer otherwise permits discovery outside this view's scope.
            let domain = method.split('.').next().unwrap_or("");
            if !matches!(
                domain,
                "Page"
                    | "Runtime"
                    | "DOM"
                    | "DOMSnapshot"
                    | "CSS"
                    | "Accessibility"
                    | "Input"
                    | "Network"
                    | "Log"
                    | "Console"
                    | "Emulation"
                    | "Performance"
                    | "Debugger"
                    | "Profiler"
                    | "Security"
            ) {
                self.error(
                    peer,
                    id,
                    "CDP domain is outside the embedded page capability",
                );
                return;
            }
            if self.pending.len() >= 256 {
                self.error(peer, id, "Too many pending browser commands");
                return;
            }
            let number = self.next_message;
            self.next_message = self.next_message.checked_add(1).unwrap_or(1);
            if self.pending.contains_key(&number) {
                self.error(peer, id, "Controller message space exhausted");
                return;
            }
            value["id"] = number.into();
            value.as_object_mut().unwrap().remove("sessionId");
            self.pending.insert(
                number,
                Pending {
                    peer,
                    id,
                    session: session.clone(),
                    since: Instant::now(),
                },
            );
            let bytes = value.to_string().into_bytes();
            let dispatched = self.targets[&session]
                .browser
                .host()
                .is_some_and(|host| host.send_dev_tools_message(Some(&bytes)) == 1);
            if !dispatched {
                if let Some(pending) = self.pending.remove(&number) {
                    self.error(
                        pending.peer,
                        pending.id,
                        "CEF rejected the embedded page command before dispatch",
                    );
                }
            }
            return;
        }
        let result = match method.as_str() {
            "Target.setDiscoverTargets" => {
                self.connections.get_mut(&peer).unwrap().discover = params["discover"] == true;
                json!({})
            }
            "Target.getTargets" => {
                json!({"targetInfos":self.targets.keys().map(|id| self.info(id)).collect::<Vec<_>>()})
            }
            "Target.attachToTarget" => {
                let target = params["targetId"].as_str().unwrap_or("");
                if !self.targets.contains_key(target) || params["flatten"] != true {
                    self.error(peer, id, "Unknown embedded target or non-flat session");
                    return;
                }
                self.connections
                    .get_mut(&peer)
                    .unwrap()
                    .sessions
                    .insert(target.into());
                json!({"sessionId":target})
            }
            "Target.detachFromTarget" => {
                if let Some(target) = params["sessionId"].as_str() {
                    self.connections
                        .get_mut(&peer)
                        .unwrap()
                        .sessions
                        .remove(target);
                }
                json!({})
            }
            "Target.getTargetInfo" => {
                let target = params["targetId"].as_str().unwrap_or("");
                if !self.targets.contains_key(target) {
                    self.error(peer, id, "Unknown embedded target");
                    return;
                }
                json!({"targetInfo":self.info(target)})
            }
            "Target.activateTarget" => {
                let Some(host) = self
                    .targets
                    .get(params["targetId"].as_str().unwrap_or(""))
                    .and_then(|target| target.browser.host())
                else {
                    self.error(peer, id, "Unknown embedded target");
                    return;
                };
                host.set_focus(1);
                json!({})
            }
            "Browser.getVersion" => {
                json!({"protocolVersion":"1.3","product":"Ferrite/CEF","revision":"","userAgent":"Ferrite","jsVersion":""})
            }
            _ => {
                self.error(
                    peer,
                    id,
                    "Browser command is outside the embedded target capability",
                );
                return;
            }
        };
        self.output(peer, json!({"id":id,"result":result}));
    }
}

impl Drop for Control {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn emit(output: &Output, value: Value) {
    // Slow readers lose the connection rather than blocking the CEF UI thread.
    let text = value.to_string();
    let length = text.len();
    if output.bytes.fetch_add(length, Ordering::Relaxed) + length > 32 * 1024 * 1024 {
        output.bytes.fetch_sub(length, Ordering::Relaxed);
        output.stop.store(true, Ordering::Relaxed);
    } else if output.send.try_send(text).is_err() {
        output.bytes.fetch_sub(length, Ordering::Relaxed);
        output.stop.store(true, Ordering::Relaxed);
    }
}

wrap_dev_tools_message_observer! {
    struct Observer { target: String, send: mpsc::SyncSender<Incoming>, overflow: Arc<AtomicBool> }
    impl DevToolsMessageObserver {
        fn on_dev_tools_message(&self, _browser: Option<&mut Browser>, message: Option<&[u8]>) -> ::std::os::raw::c_int {
            let value = message.filter(|m| m.len() <= LIMIT).and_then(|m| serde_json::from_slice(m).ok());
            if value.is_none_or(|value| self.send.try_send(Incoming::Page(self.target.clone(), value)).is_err()) {
                self.overflow.store(true, Ordering::Relaxed);
            }
            1
        }
    }
}

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
    let Ok(mut socket) = socket else { return };
    let _ = socket
        .get_mut()
        .set_read_timeout(Some(Duration::from_millis(10)));
    let (output, outgoing) = mpsc::sync_channel(32);
    let peer_stop = Arc::new(AtomicBool::new(false));
    let queued_bytes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let output = Output {
        send: output,
        stop: peer_stop.clone(),
        bytes: queued_bytes.clone(),
    };
    if send.try_send(Incoming::Open(id, output)).is_err() {
        return;
    }
    while !stop.load(Ordering::Relaxed) && !peer_stop.load(Ordering::Relaxed) {
        for _ in 0..64 {
            let Ok(value) = outgoing.try_recv() else {
                break;
            };
            queued_bytes.fetch_sub(value.len(), Ordering::Relaxed);
            if socket.send(Message::Text(value)).is_err() {
                peer_stop.store(true, Ordering::Relaxed);
                let _ = send.try_send(Incoming::Closed(id));
                return;
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                let Ok(value) = serde_json::from_str(&text) else {
                    break;
                };
                if send.try_send(Incoming::Command(id, value)).is_err() {
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
            Err(_) => break,
        }
    }
    peer_stop.store(true, Ordering::Relaxed);
    let _ = socket.close(None);
    let _ = send.try_send(Incoming::Closed(id));
}
