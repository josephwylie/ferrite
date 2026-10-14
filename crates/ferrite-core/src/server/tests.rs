//! The Server over its socket, with scripted Sessions: Decisions answered
//! by two windows, and a window that reconnects resuming from its ring or
//! rebuilding from history.

use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Instant;

use super::*;
use crate::cockpit::mirror::Mirrors;
use crate::engine::{SpawnRequest, Spawner};
use crate::link::{Attached, Client};
use crate::protocol::{Peer, Resume, WireAnswer, WireChoice};
use crate::providers::Session;
use crate::store::Provider;
use crate::transcript::Body;
use crate::{Decision, DecisionAnswer, SessionEvent};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fs-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

type Answers = Arc<Mutex<Vec<(String, DecisionAnswer)>>>;

struct Scripted {
    events: Receiver<SessionEvent>,
    answers: Answers,
}

impl Session for Scripted {
    fn events(&self) -> &Receiver<SessionEvent> {
        &self.events
    }
    fn send(&mut self, _: &str) -> io::Result<()> {
        Ok(())
    }
    fn interrupt(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn respond_to_decision(&mut self, id: &str, answer: DecisionAnswer) -> io::Result<()> {
        self.answers.lock().unwrap().push((id.into(), answer));
        Ok(())
    }
}

/// Each Session it starts hands the test the Sender that plays its provider.
struct Spawn {
    sessions: Sender<Sender<SessionEvent>>,
    answers: Answers,
}

impl Spawner for Spawn {
    fn spawn(&mut self, _: SpawnRequest) -> io::Result<Box<dyn crate::providers::Session>> {
        let (provider, events) = mpsc::channel();
        self.sessions.send(provider).unwrap();
        Ok(Box::new(Scripted {
            events,
            answers: self.answers.clone(),
        }))
    }
}

struct Harness {
    server: std::thread::JoinHandle<i32>,
    /// Every executable the Server restarted into, in place of `exec`.
    relaunched: Receiver<PathBuf>,
    dir: PathBuf,
    address: PathBuf,
    sessions: Receiver<Sender<SessionEvent>>,
    answers: Answers,
}

fn start(name: &str) -> Harness {
    let dir = scratch(name);
    let store = dir.join("threads");
    let address = link::address(&store);
    let listener = link::listen(&address).unwrap();
    let (sessions, started) = mpsc::channel();
    let answers = Answers::default();
    let spawn = Spawn {
        sessions,
        answers: answers.clone(),
    };
    let settings_dir = dir.clone();
    let (relaunches, relaunched) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let engine = Engine::try_new(Store::open(&store).unwrap(), Box::new(spawn)).unwrap();
        let info = ServerInfo {
            version: "0.9.0".into(),
            executable: "/bin/ferrite".into(),
        };
        let (_, renders) = render::remote();
        let defaults = Arc::new(Mutex::new(SessionDefaults::default()));
        let mut server = Server::new(engine, info, renders, defaults, settings_dir);
        server.relaunch = Box::new(move |executable: &Path| {
            relaunches.send(executable.to_owned()).unwrap();
            Ok(())
        });
        let (heard, hear) = mpsc::channel();
        link::accept_all(listener, heard).unwrap();
        server.serve(hear)
    });
    Harness {
        server,
        relaunched,
        dir,
        address,
        sessions: started,
        answers,
    }
}

/// A window's end: the link and the mirrors it builds.
struct Window {
    client: Client,
    boot_id: String,
    mirrors: Mirrors,
    replies: Vec<ServerMessage>,
}

fn attach(address: &Path) -> Window {
    let Attached {
        client, snapshot, ..
    } = Client::attach(
        address,
        ClientMessage::Hello {
            generation: 1,
            client: Peer {
                name: "test".into(),
                version: "0".into(),
            },
            capabilities: Vec::new(),
        },
    )
    .unwrap();
    let mut window = Window {
        client,
        boot_id: snapshot.boot_id.clone(),
        mirrors: Mirrors::default(),
        replies: Vec::new(),
    };
    for state in snapshot.threads {
        window.receive(ServerMessage::Thread { revision: 0, state });
    }
    window
}

impl Window {
    fn receive(&mut self, message: ServerMessage) {
        if matches!(message, ServerMessage::Reply { .. }) {
            self.replies.push(message);
        } else {
            self.mirrors.receive(message, Instant::now());
        }
    }

    /// Take what arrives until `done` holds, for at most five seconds.
    fn until(&mut self, done: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(self) {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "timed out");
            if let Ok(message) = self.client.messages.recv_timeout(left) {
                self.receive(message);
            }
        }
    }

    fn command(&mut self, id: u64, op: Op) -> ServerMessage {
        self.client
            .send(&ClientMessage::Command(Command { id, op }))
            .unwrap();
        self.until(|window| {
            window
                .replies
                .iter()
                .any(|reply| matches!(reply, ServerMessage::Reply { id: got, .. } if *got == id))
        });
        let at = self
            .replies
            .iter()
            .position(|reply| matches!(reply, ServerMessage::Reply { id: got, .. } if *got == id))
            .unwrap();
        self.replies.remove(at)
    }

    fn watch(&mut self, thread: ThreadId, resume: Option<Resume>) {
        self.client
            .send(&ClientMessage::Watch { thread, resume })
            .unwrap();
    }

    fn said(&self, thread: ThreadId) -> Vec<String> {
        self.mirrors.view(thread).map_or(Vec::new(), |view| {
            view.transcript()
                .blocks()
                .iter()
                .filter(|block| !matches!(block.body, Body::Meta(_)))
                .map(|block| format!("{:?}", block.body))
                .collect()
        })
    }
}

fn open(window: &mut Window, harness: &Harness) -> (ThreadId, Sender<SessionEvent>) {
    let reply = window.command(
        1,
        Op::Open {
            provider: Provider::Claude,
            workspace: WireChoice::Main {
                checkout: harness.dir.clone(),
            },
        },
    );
    let ServerMessage::Reply {
        ok: Some(Done::Thread { thread }),
        ..
    } = reply
    else {
        panic!("open failed: {reply:?}");
    };
    let provider = harness
        .sessions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    (thread, provider)
}

fn decision() -> SessionEvent {
    SessionEvent::DecisionRequested {
        decision: Decision {
            delivery: Default::default(),
            kind: Default::default(),
            policy: Default::default(),
            id: "perm_01".into(),
            tool_use_id: "toolu_01".into(),
            tool_name: "Write".into(),
            description: "ferrite-perm.txt".into(),
            input: serde_json::json!({}),
            suggestions: vec![],
        },
    }
}

fn text(text: &str) -> SessionEvent {
    SessionEvent::TextDelta { text: text.into() }
}

/// Two windows answer one Decision: the first answer reaches the provider,
/// the second is told it was already answered, and both see it close.
#[test]
fn the_first_answer_wins_and_the_second_hears_already_answered() {
    let harness = start("answers");
    let mut first = attach(&harness.address);
    let mut second = attach(&harness.address);
    let (thread, provider) = open(&mut first, &harness);
    first.watch(thread, None);
    second.watch(thread, None);
    provider.send(decision()).unwrap();
    let pending = |window: &Window| {
        window
            .mirrors
            .view(thread)
            .is_some_and(|view| view.pending().is_some())
    };
    first.until(pending);
    second.until(pending);
    let handle: protocol::WireHandle =
        (&first.mirrors.view(thread).unwrap().activity().decisions()[0].handle).into();
    let answer = |handle: &protocol::WireHandle| Op::Answer {
        thread,
        handle: handle.clone(),
        answer: WireAnswer::Allow {
            input: serde_json::json!({}),
        },
    };
    let won = first.command(10, answer(&handle));
    let lost = second.command(20, answer(&handle));

    assert!(
        matches!(won, ServerMessage::Reply { error: None, .. }),
        "{won:?}"
    );
    assert!(
        matches!(&lost, ServerMessage::Reply { error: Some(error), .. }
            if error.code == ErrorCode::AlreadyAnswered),
        "{lost:?}"
    );
    assert_eq!(
        harness.answers.lock().unwrap().len(),
        1,
        "never answered twice"
    );
    first.until(|window| !pending(window));
    second.until(|window| !pending(window));
}

/// A window that drops mid-turn and comes back within this boot resumes
/// from the ring: its transcript then reads as an uninterrupted window's.
#[test]
fn a_resume_within_the_ring_reads_as_if_never_gone() {
    let harness = start("resume");
    let mut steady = attach(&harness.address);
    let (thread, provider) = open(&mut steady, &harness);
    steady.watch(thread, None);
    let mut leaving = attach(&harness.address);
    leaving.watch(thread, None);
    provider.send(text("before ")).unwrap();
    let heard = |text: &'static str| {
        move |window: &Window| window.said(thread).iter().any(|said| said.contains(text))
    };
    leaving.until(heard("before"));

    let Window {
        mut mirrors,
        boot_id,
        ..
    } = leaving;
    provider.send(text("while away")).unwrap();
    steady.until(heard("while away"));
    let mut back = attach(&harness.address);
    std::mem::swap(&mut back.mirrors, &mut mirrors);
    let after = back.mirrors.seq(thread).unwrap();
    back.watch(thread, Some(Resume { boot_id, after }));
    back.until(heard("while away"));

    assert_eq!(back.said(thread), steady.said(thread));
}

/// A resume into another boot (or past the ring) cannot be met: the
/// window rebuilds the Thread from its history and the live state.
#[test]
fn a_resume_the_server_cannot_meet_rebuilds_from_history() {
    let harness = start("rebuild");
    let mut steady = attach(&harness.address);
    let (thread, provider) = open(&mut steady, &harness);
    steady.watch(thread, None);
    provider.send(text("from the log")).unwrap();
    provider
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    provider.send(decision()).unwrap();
    steady.until(|window| {
        window
            .mirrors
            .view(thread)
            .is_some_and(|view| view.pending().is_some())
    });

    let mut back = attach(&harness.address);
    back.watch(
        thread,
        Some(Resume {
            boot_id: "another boot".into(),
            after: 3,
        }),
    );
    back.until(|window| {
        window
            .mirrors
            .view(thread)
            .is_some_and(|view| view.pending().is_some())
    });

    assert_eq!(back.said(thread), steady.said(thread));
    assert!(back
        .said(thread)
        .iter()
        .any(|said| said.contains("from the log")));
}

#[test]
fn a_command_this_server_does_not_know_is_answered_unsupported() {
    let harness = start("unknown");
    let mut window = attach(&harness.address);
    let reply = window.command(5, Op::Unknown);
    assert!(
        matches!(&reply, ServerMessage::Reply { error: Some(error), .. }
            if error.code == ErrorCode::Unsupported),
        "{reply:?}"
    );
}

#[test]
fn stop_now_ends_the_server_cleanly() {
    let harness = start("stop");
    let mut window = attach(&harness.address);
    let reply = window.command(9, Op::Stop { when: When::Now });
    assert!(matches!(reply, ServerMessage::Reply { error: None, .. }));
    window.until(|window| window.replies.is_empty());
    assert_eq!(harness.server.join().unwrap(), 0);
}

/// `restart {when: idle}` waits for the agents: not while a turn runs,
/// and then once — windows told first, the Sessions ended, the successor
/// started, and this Server done.
#[test]
fn a_restart_when_idle_waits_for_the_turn_and_happens_once() {
    let harness = start("restart");
    let mut window = attach(&harness.address);
    let (thread, provider) = open(&mut window, &harness);
    window.watch(thread, None);
    window.command(
        2,
        Op::Send {
            thread,
            text: "go".into(),
        },
    );
    provider.send(text("working")).unwrap();
    window.until(|window| {
        window
            .said(thread)
            .iter()
            .any(|said| said.contains("working"))
    });

    let reply = window.command(3, Op::Restart { when: When::Idle });
    assert!(
        matches!(reply, ServerMessage::Reply { error: None, .. }),
        "{reply:?}"
    );
    assert!(
        harness
            .relaunched
            .recv_timeout(Duration::from_millis(300))
            .is_err(),
        "not mid-turn"
    );

    provider
        .send(SessionEvent::TurnEnded {
            outcome: crate::TurnOutcome::Completed,
            cost_usd: None,
        })
        .unwrap();
    let successor = harness
        .relaunched
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(successor, std::env::current_exe().unwrap());
    let restarting = loop {
        match window.client.messages.recv_timeout(Duration::from_secs(5)) {
            Ok(ServerMessage::Restarting { when }) => break when,
            Ok(_) => continue,
            Err(error) => panic!("no restarting: {error}"),
        }
    };
    assert_eq!(restarting, When::Idle);
    assert_eq!(harness.server.join().unwrap(), 0);
    assert!(harness.relaunched.try_recv().is_err(), "restarted once");
}

#[test]
fn only_a_newer_window_running_the_same_executable_asks_for_a_restart() {
    let exe = std::env::current_exe().unwrap();
    let server = |version: &str, executable: &Path| ServerInfo {
        version: version.into(),
        executable: executable.to_owned(),
    };
    assert!(wants_restart("0.10.0", &exe, &server("0.9.3", &exe)));
    assert!(
        !wants_restart("0.9.3", &exe, &server("0.9.3", &exe)),
        "equal never"
    );
    assert!(
        !wants_restart("0.9.0", &exe, &server("0.10.0", &exe)),
        "older never"
    );
    assert!(
        !wants_restart(
            "0.10.0",
            &exe,
            &server(
                "0.9.3",
                Path::new("/Applications/Ferrite.app/Contents/MacOS/ferrite")
            )
        ),
        "another install's Server restarts into itself"
    );
    assert!(!wants_restart("garbage", &exe, &server("0.9.3", &exe)));
    assert!(wants_restart("1.0.0-beta.1", &exe, &server("0.9.9", &exe)));
}
