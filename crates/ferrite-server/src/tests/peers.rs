//! Peers, the Server's client of a saved machine's API: against a real
//! Server's API (where `ssh … ferrited api-bridge` would reach it), and
//! against scripted Tunnels for what a real one rarely does.

use super::*;
use crate::peers::{Peer, PeerFailure, Peers, WAIT_PIECE};
use ferrite_core::api::schema::{AgentWaitParams, Empty, ProjectParams, ThreadParams};
use ferrite_core::api::{Code, Method};
use ferrite_core::machine::{Dial, Failure, Machine, Tunnel};
use ferrite_core::server::ServerId;

/// `box` is saved and on; `off` is saved and turned off.
fn save_machines(dir: &Path) {
    std::fs::write(
        dir.join("settings.json"),
        r#"{"machines":[{"name":"box","target":"me@box"},{"name":"off","target":"me@off","enabled":false}]}"#,
    )
    .unwrap();
}

/// Peers whose `box` is whatever `dial` opens.
fn peers(dir: &Path, dial: impl Fn() -> Dial + Send + Sync + 'static) -> Peers {
    save_machines(dir);
    Peers::with(
        dir.to_path_buf(),
        Box::new(move |machine: &Machine| {
            assert_eq!(machine.target, "me@box", "only box is ever dialled");
            dial()
        }),
        Box::new(Instant::now),
    )
}

/// Each request sent, as JSON.
fn requests(sent: &Mutex<Vec<u8>>) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&sent.lock().unwrap())
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn methods(sent: &Mutex<Vec<u8>>) -> Vec<String> {
    requests(sent)
        .iter()
        .map(|request| request["method"].as_str().unwrap().to_string())
        .collect()
}

fn server_id(harness: &Harness) -> ServerId {
    crate::server_id::open(&harness.dir.join("threads")).unwrap()
}

/// A saved machine is known by the Server it answers as, under the name
/// this computer gives it, and a ServerId leads back to it. Each call asks
/// who answers before its request, and none carries a `caller`: a Thread
/// id here would name another Thread there.
#[test]
fn a_peer_is_asked_as_the_server_it_answers_as() {
    let harness = start("peers-asked");
    let sent = Arc::new(Mutex::new(Vec::new()));
    let (api, kept) = (harness.api.clone(), sent.clone());
    let peers = peers(&harness.dir, move || fake_ssh(api.clone(), kept.clone()));

    let peer = peers.identify("box").unwrap();
    assert_eq!(
        peer,
        Peer {
            server: server_id(&harness),
            label: "box".into(),
        }
    );
    assert_eq!(peers.identify("me@box").unwrap(), peer, "by its target too");
    assert_eq!(peers.machine(&peer.server).unwrap().name, "box");
    let listed = peers
        .call(&peer.server, Method::ThreadList(Empty {}))
        .unwrap();
    assert_eq!(listed, serde_json::json!({ "threads": [] }));

    assert_eq!(
        methods(&sent),
        ["identity.get", "identity.get", "identity.get", "thread.list"]
    );
    for request in requests(&sent) {
        assert!(request.get("caller").is_none(), "{request}");
    }
}

/// A machine pointed at another Server is never sent the request, and is
/// then no longer that Server: asking for it again is final.
#[test]
fn a_machine_answering_as_another_server_is_never_asked() {
    let (first, other) = (start("peers-first"), start("peers-other"));
    let sent = Arc::new(Mutex::new(Vec::new()));
    let api = Arc::new(Mutex::new(first.api.clone()));
    let (at, kept) = (api.clone(), sent.clone());
    let peers = peers(&first.dir, move || {
        fake_ssh(at.lock().unwrap().clone(), kept.clone())
    });
    let expected = peers.identify("box").unwrap().server;

    *api.lock().unwrap() = other.api.clone();
    sent.lock().unwrap().clear();
    let launch = Method::ThreadStart(
        serde_json::from_value(serde_json::json!({ "cwd": "/srv/app" })).unwrap(),
    );
    assert_eq!(
        peers.call(&expected, launch.clone()),
        Err(PeerFailure::OtherServer {
            expected: expected.clone(),
            answered: server_id(&other),
        })
    );
    assert_eq!(methods(&sent), ["identity.get"], "the start was never sent");
    assert!(matches!(
        peers.call(&expected, launch),
        Err(PeerFailure::Final(_))
    ));
}

/// A refusal says whether asking again can change it: a Thread gone there,
/// or a machine not saved or turned off here, is final; any other refusal
/// comes back in the peer's own code.
#[test]
fn a_refusal_says_whether_asking_again_can_change_it() {
    let harness = start("peers-refused");
    let api = harness.api.clone();
    let peers = peers(&harness.dir, move || fake_ssh(api.clone(), Arc::default()));
    let server = peers.identify("box").unwrap().server;

    let gone = peers.call(
        &server,
        Method::ThreadGet(ThreadParams {
            thread: ThreadId::new(999),
        }),
    );
    assert!(matches!(gone, Err(PeerFailure::Final(_))), "{gone:?}");
    match peers.call(
        &server,
        Method::ProjectGet(ProjectParams {
            project: "404".into(),
        }),
    ) {
        Err(PeerFailure::Refused(refusal)) => assert_eq!(refusal.code, Code::ProjectNotFound),
        other => panic!("{other:?}"),
    }
    for name in ["off", "nobody"] {
        let refused = peers.identify(name);
        assert!(
            matches!(refused, Err(PeerFailure::Final(_))),
            "{name}: {refused:?}"
        );
    }
    let unknown = ServerId::new("0".repeat(32));
    assert!(matches!(
        peers.machine(&unknown),
        Err(PeerFailure::Final(_))
    ));
}

/// A peer played by a script: each request sent is answered with what
/// `answer` makes of it, from the Server `s-1`.
fn scripted(
    answer: impl Fn(&serde_json::Value) -> serde_json::Value + Send + Sync + 'static,
) -> impl Fn() -> Dial + Send + Sync + 'static {
    let answer = Arc::new(answer);
    move || {
        let answer = answer.clone();
        Box::new(move || {
            let (says, said) = mpsc::channel::<Vec<u8>>();
            Ok(Tunnel {
                from: Box::new(Heard(said, io::Cursor::new(Vec::new()))),
                to: Box::new(Asked(says, answer.clone())),
                ended: Box::new(|| None),
            })
        })
    }
}

type Answer = Arc<dyn Fn(&serde_json::Value) -> serde_json::Value + Send + Sync>;

/// The script's end of a Tunnel: every line written is answered.
struct Asked(Sender<Vec<u8>>, Answer);

impl io::Write for Asked {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let request: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        let answer = match request["method"].as_str() {
            Some("identity.get") => serde_json::json!({ "result": { "server_id": "s-1" } }),
            _ => (self.1)(&request),
        };
        let _ = self.0.send(protocol::encode(&answer));
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Heard(Receiver<Vec<u8>>, io::Cursor<Vec<u8>>);

impl io::Read for Heard {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.1.position() as usize == self.1.get_ref().len() {
            match self.0.recv() {
                Ok(line) => self.1 = io::Cursor::new(line),
                Err(_) => return Ok(0),
            }
        }
        self.1.read(buf)
    }
}

/// A Tunnel that ends at once, as an ssh that failed ends.
fn ended(failure: Failure) -> impl Fn() -> Dial + Send + Sync + 'static {
    move || {
        let failure = failure.clone();
        Box::new(move || {
            let failure = failure.clone();
            Ok(Tunnel {
                from: Box::new(io::empty()),
                to: Box::new(io::sink()),
                ended: Box::new(move || Some(failure)),
            })
        })
    }
}

/// A peer whose `ferrited` is from before `identity.get`.
fn older() -> Dial {
    Box::new(|| {
        let line =
            br#"{"id":"1","error":{"code":"unknown_method","message":"no method `identity.get`"}}"#;
        Ok(Tunnel {
            from: Box::new(io::Cursor::new(
                [
                    &line[..],
                    b"
",
                ]
                .concat(),
            )),
            to: Box::new(io::sink()),
            ended: Box::new(|| None),
        })
    })
}

/// How a Tunnel failed is typed, so a caller can tell waiting for the
/// operator from waiting for the machine from giving up.
#[test]
fn a_failed_tunnel_says_why() {
    let dir = scratch("peers-failed");
    let usage = "usage: ferrited server | ferrited bridge".to_string();
    type Dials = Box<dyn Fn() -> Dial + Send + Sync>;
    let cases: Vec<(Dials, fn(&PeerFailure) -> bool)> = vec![
        (
            Box::new(|| Box::new(|| Err(io::Error::other("ssh: not found")))),
            |failure| matches!(failure, PeerFailure::Unreachable(said) if said.contains("not found")),
        ),
        (
            Box::new(ended(Failure::Unreachable("No route to host".into()))),
            |failure| matches!(failure, PeerFailure::Unreachable(said) if said == "No route to host"),
        ),
        (Box::new(ended(Failure::SignIn)), |failure| {
            *failure == PeerFailure::SignIn
        }),
        (Box::new(ended(Failure::NotInstalled)), |failure| {
            *failure == PeerFailure::NoFerrited
        }),
        (Box::new(ended(Failure::Unreachable(usage))), |failure| {
            matches!(failure, PeerFailure::Unsupported(_))
        }),
        (
            Box::new(older),
            |failure| matches!(failure, PeerFailure::Unsupported(said) if said.contains("older")),
        ),
    ];
    for (index, (dial, expected)) in cases.into_iter().enumerate() {
        let failure = peers(&dir, dial).identify("box").unwrap_err();
        assert!(expected(&failure), "case {index}: {failure:?}");
    }
}

/// A wait longer than [`WAIT_PIECE`] is asked as pieces of at most that,
/// each timed out one asked again, until the Thread answers or the whole
/// wait has passed on the clock.
#[test]
fn a_long_wait_is_asked_in_pieces() {
    let dir = scratch("peers-wait");
    save_machines(&dir);
    let now = Arc::new(Mutex::new(Instant::now()));
    let asked = Arc::new(Mutex::new(Vec::new()));
    let settles_on = Arc::new(Mutex::new(usize::MAX));
    let (clock, kept, settles) = (now.clone(), asked.clone(), settles_on.clone());
    let dial = scripted(move |request| {
        let ms = request["params"]["timeout_ms"].as_u64().unwrap();
        let mut asked = kept.lock().unwrap();
        asked.push(ms);
        *clock.lock().unwrap() += Duration::from_millis(ms);
        match asked.len() == *settles.lock().unwrap() {
            true => serde_json::json!({ "result": { "thread": { "attention": "done" } } }),
            false => serde_json::json!({ "error": { "code": "timeout", "message": "timed out" } }),
        }
    });
    let at = now.clone();
    let peers = Peers::with(
        dir.clone(),
        Box::new(move |_: &Machine| dial()),
        Box::new(move || *at.lock().unwrap()),
    );
    let server = ServerId::new("s-1");
    let wait = |ms| AgentWaitParams {
        thread: ThreadId::new(3),
        until: None,
        turn_ended: false,
        timeout_ms: Some(ms),
    };
    let piece = WAIT_PIECE.as_millis() as u64;

    match peers.wait(&server, wait(120_000)) {
        Err(PeerFailure::Refused(refusal)) => assert_eq!(refusal.code, Code::Timeout),
        other => panic!("{other:?}"),
    }
    assert_eq!(*asked.lock().unwrap(), [piece, piece, 20_000]);

    asked.lock().unwrap().clear();
    *settles_on.lock().unwrap() = 2;
    let settled = peers.wait(&server, wait(600_000)).unwrap();
    assert_eq!(settled["thread"]["attention"], "done");
    assert_eq!(*asked.lock().unwrap(), [piece, piece]);
}
