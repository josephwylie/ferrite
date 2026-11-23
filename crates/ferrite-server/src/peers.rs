//! Peers (ADR 0027): this Server's client of a saved machine's API, as T3
//! Code's `PeerMcpClient` is of a linked environment's. It dials a machine
//! in `settings.machines` through its saved transport (`machine::api_dial`:
//! `ferrited api-bridge` over ssh, or a paired link) and asks one request
//! for one answer. A call is the Server's own, never a Thread's: it sends no
//! `caller`, since a Thread id here names another Thread there.
//!
//! A peer is named by its [`ServerId`], not by a saved name or target, which
//! the operator may edit or point elsewhere. Each call asks `identity.get`
//! first and sends its request only to the Server it expects, as T3 gives a
//! link's token only to an address that answers as the linked environment.
//! How a call failed is typed ([`PeerFailure`]) for a caller to decide
//! whether to ask again; Peers itself retries nothing.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;

use ferrite_core::api::schema::{AgentWaitParams, Empty};
use ferrite_core::api::{ApiError, Code, Method, Request};
use ferrite_core::link;
use ferrite_core::machine::{self, Dial, Machine, Tunnel};
use ferrite_core::server::ServerId;
use ferrite_core::settings::Settings;

/// The longest one call to a peer waits: a link's edge drops a request idle
/// for about 100 s, so a longer wait is asked as several (T3's
/// `docs/internals/remote.md`).
pub const WAIT_PIECE: Duration = Duration::from_secs(50);

/// A saved machine's Server, as it answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub server: ServerId,
    /// What this computer calls the machine: its saved name.
    pub label: String,
}

/// Why a call to a peer has no answer.
#[derive(Debug, Clone, PartialEq)]
pub enum PeerFailure {
    /// Not reached, or the connection broke before an answer, which may
    /// still have been carried out: the machine asleep, no route, in ssh's
    /// or the link's words.
    Unreachable(String),
    /// ssh needs the operator to sign in, from a window.
    SignIn,
    /// A paired link's address did not answer, at that address (T3's
    /// "did not answer at").
    Silent(String),
    /// The machine no longer takes this paired link: its session expired
    /// or was revoked there, and only a new pairing undoes it (T3's
    /// `expired`).
    Expired,
    /// The machine's shell has no `ferrited`.
    NoFerrited,
    /// The machine's `ferrited` is too old for what was asked.
    Unsupported(String),
    /// Asking again cannot change it: no saved machine here is that Server,
    /// it is turned off, or the Thread asked about is gone there.
    Final(String),
    /// The machine answered as another Server: it was pointed elsewhere.
    OtherServer {
        expected: ServerId,
        answered: ServerId,
    },
    /// The peer heard the request and refused it, in its own code and words.
    Refused(ApiError),
}

/// Opens a Tunnel to a saved machine's API.
pub type DialMachine = Box<dyn Fn(&Machine) -> Dial + Send + Sync>;

pub struct Peers {
    settings_dir: PathBuf,
    dial: DialMachine,
    clock: Box<dyn Fn() -> Instant + Send + Sync>,
    /// The Server each saved target last answered as.
    heard: Mutex<BTreeMap<String, ServerId>>,
}

impl Peers {
    /// The machines saved beside `settings_dir`, through their saved
    /// transports.
    pub fn new(settings_dir: PathBuf) -> Self {
        let dir = settings_dir.clone();
        Self::with(
            settings_dir,
            Box::new(move |machine| machine::api_dial(machine, &dir)),
            Box::new(ferrite_core::clock::instant),
        )
    }

    /// The same, through `dial`, its waits timed by `clock`.
    pub fn with(
        settings_dir: PathBuf,
        dial: DialMachine,
        clock: Box<dyn Fn() -> Instant + Send + Sync>,
    ) -> Self {
        Self {
            settings_dir,
            dial,
            clock,
            heard: Mutex::default(),
        }
    }

    /// The Server the saved machine `name` (by its name, else its target)
    /// answers as.
    pub fn identify(&self, name: &str) -> Result<Peer, PeerFailure> {
        let saved = self.saved()?;
        let machine = saved_machine(&saved, name)
            .ok_or_else(|| PeerFailure::Final(format!("no saved machine `{name}`")))?;
        if !machine.enabled {
            return Err(turned_off(machine));
        }
        self.answering(machine)
    }

    /// The saved machine that is the Server `server`: the one last heard as
    /// it, else the first of the others to answer as it.
    pub fn machine(&self, server: &ServerId) -> Result<Machine, PeerFailure> {
        let saved = self.saved()?;
        let heard = lock(&self.heard).clone();
        if let Some(machine) = saved
            .iter()
            .find(|machine| heard.get(&machine.target) == Some(server))
        {
            return match machine.enabled {
                true => Ok(machine.clone()),
                false => Err(turned_off(machine)),
            };
        }
        // One that did not answer may be it: that is no final answer.
        let mut unanswered = None;
        for machine in saved
            .iter()
            .filter(|machine| machine.enabled && !heard.contains_key(&machine.target))
        {
            match self.answering(machine) {
                Ok(peer) if peer.server == *server => return Ok(machine.clone()),
                Ok(_) => {}
                Err(failure) => {
                    unanswered.get_or_insert(failure);
                }
            }
        }
        Err(unanswered.unwrap_or_else(|| {
            PeerFailure::Final(format!(
                "no saved machine is the Server {}",
                server.as_str()
            ))
        }))
    }

    /// Ask the Server `server` one request: its answer's `result`.
    pub fn call(&self, server: &ServerId, method: Method) -> Result<Value, PeerFailure> {
        let machine = self.machine(server)?;
        let mut connection = self.connect(&machine)?;
        let answered = connection.server_id()?;
        self.heard(&machine, &answered);
        if answered != *server {
            return Err(PeerFailure::OtherServer {
                expected: server.clone(),
                answered,
            });
        }
        connection.ask(method)
    }

    /// `agent.wait` on the Server `server`, in calls of at most
    /// [`WAIT_PIECE`]: a piece that times out is asked again until the
    /// whole `timeout_ms` has passed, when its `timeout` refusal is the
    /// answer. With none it waits until the Thread answers.
    pub fn wait(&self, server: &ServerId, params: AgentWaitParams) -> Result<Value, PeerFailure> {
        let deadline = params
            .timeout_ms
            .map(|ms| (self.clock)() + Duration::from_millis(ms));
        loop {
            let left = deadline.map(|deadline| deadline.saturating_duration_since((self.clock)()));
            let piece = left.map_or(WAIT_PIECE, |left| left.min(WAIT_PIECE));
            let asked = AgentWaitParams {
                timeout_ms: Some(piece.as_millis() as u64),
                ..params.clone()
            };
            match self.call(server, Method::AgentWait(asked)) {
                Err(PeerFailure::Refused(refusal))
                    if refusal.code == Code::Timeout && left.is_none_or(|left| left > piece) => {}
                answer => return answer,
            }
        }
    }

    fn saved(&self) -> Result<Vec<Machine>, PeerFailure> {
        Settings::load(&self.settings_dir)
            .map(|settings| settings.machines)
            .map_err(|error| PeerFailure::Unreachable(format!("settings.json: {error}")))
    }

    fn answering(&self, machine: &Machine) -> Result<Peer, PeerFailure> {
        let server = self.connect(machine)?.server_id()?;
        self.heard(machine, &server);
        Ok(Peer {
            server,
            label: machine.name.clone(),
        })
    }

    fn heard(&self, machine: &Machine, server: &ServerId) {
        lock(&self.heard).insert(machine.target.clone(), server.clone());
    }

    fn connect(&self, machine: &Machine) -> Result<Connection, PeerFailure> {
        let now = || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        };
        if machine
            .expires(&self.settings_dir)
            .is_some_and(|expires| expires <= now())
        {
            return Err(PeerFailure::Expired);
        }
        let tunnel = (self.dial)(machine)().map_err(|error| match machine::failure(&error) {
            Some(failure) => unreached(failure.clone()),
            None => match error.kind() {
                io::ErrorKind::PermissionDenied if machine.is_link() => PeerFailure::Expired,
                io::ErrorKind::NotConnected if machine.is_link() => {
                    PeerFailure::Silent(machine.display_target(&self.settings_dir))
                }
                _ => PeerFailure::Unreachable(error.to_string()),
            },
        })?;
        Ok(Connection::new(tunnel))
    }
}

/// The saved machine `name` names: by its name, else its target.
pub fn saved_machine<'a>(saved: &'a [Machine], name: &str) -> Option<&'a Machine> {
    saved
        .iter()
        .find(|machine| machine.name == name)
        .or_else(|| saved.iter().find(|machine| machine.target == name))
}

fn turned_off(machine: &Machine) -> PeerFailure {
    PeerFailure::Final(format!("`{}` is turned off", machine.name))
}

/// One Tunnel to a machine's API, asked one request at a time.
struct Connection {
    from: link::Lines<Box<dyn Read + Send>>,
    to: Option<Box<dyn Write + Send>>,
    ended: Option<Box<dyn FnOnce() -> Option<machine::Failure> + Send>>,
}

impl Connection {
    fn new(Tunnel { from, to, ended }: Tunnel) -> Self {
        Self {
            from: link::Lines::new(from),
            to: Some(to),
            ended: Some(ended),
        }
    }

    fn server_id(&mut self) -> Result<ServerId, PeerFailure> {
        let answer = self.ask(Method::IdentityGet(Empty {}))?;
        serde_json::from_value(answer["server_id"].clone()).map_err(|_| {
            PeerFailure::Unsupported(format!(
                "`identity.get` answered without a server_id: {answer}"
            ))
        })
    }

    fn ask(&mut self, method: Method) -> Result<Value, PeerFailure> {
        let request = Request {
            id: "1".into(),
            method,
            caller: None,
        };
        let to = self
            .to
            .as_mut()
            .expect("a connection is asked until it breaks");
        let broke = match link::send(to, &request) {
            Err(error) => error,
            Ok(()) => match self.from.next_line() {
                Ok(Some(line)) => return answer(&line),
                Ok(None) => io::ErrorKind::UnexpectedEof.into(),
                Err(error) => error,
            },
        };
        Err(match self.end() {
            Some(failure) => unreached(failure),
            None => PeerFailure::Unreachable(format!(
                "the connection ended before an answer ({broke}); the request may still have \
                 been carried out"
            )),
        })
    }

    /// Close the Tunnel, so an ssh still running ends, and say how it ended.
    fn end(&mut self) -> Option<machine::Failure> {
        drop(self.to.take());
        self.ended.take().and_then(|ended| ended())
    }
}

impl Drop for Connection {
    /// An ssh is waited for, never left behind.
    fn drop(&mut self) {
        self.end();
    }
}

/// An answer's `result`, or what its refusal means for asking again.
fn answer(line: &[u8]) -> Result<Value, PeerFailure> {
    let mut answer: Value = serde_json::from_slice(line).map_err(|error| {
        PeerFailure::Unsupported(format!("an answer this Server cannot read: {error}"))
    })?;
    if let Some(error) = answer.get("error") {
        return Err(refused(error));
    }
    match answer.get_mut("result") {
        Some(result) => Ok(result.take()),
        None => Err(PeerFailure::Unsupported(format!(
            "an answer with no result: {answer}"
        ))),
    }
}

fn refused(error: &Value) -> PeerFailure {
    let message = error["message"].as_str().unwrap_or_default().to_string();
    match serde_json::from_value::<Code>(error["code"].clone()) {
        Ok(Code::UnknownThread) => PeerFailure::Final(message),
        Ok(Code::UnknownMethod) => PeerFailure::Unsupported(format!(
            "the machine's `ferrited` is older than this Server ({message}): update it there"
        )),
        Ok(code) => PeerFailure::Refused(ApiError {
            thread: serde_json::from_value(error["thread"].clone()).ok(),
            ..ApiError::new(code, message)
        }),
        // A newer peer's code: its words are all there is to go on.
        Err(_) => PeerFailure::Refused(ApiError::new(Code::Failed, message)),
    }
}

/// How a Tunnel that ended without an answer failed.
fn unreached(failure: machine::Failure) -> PeerFailure {
    match failure {
        machine::Failure::SignIn => PeerFailure::SignIn,
        machine::Failure::Revoked => PeerFailure::Expired,
        machine::Failure::NotInstalled => PeerFailure::NoFerrited,
        failure if failure.older_ferrited() => PeerFailure::Unsupported(
            "the machine's `ferrited` is older than `api-bridge`: update it there".into(),
        ),
        machine::Failure::Unreachable(said) => PeerFailure::Unreachable(said),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
