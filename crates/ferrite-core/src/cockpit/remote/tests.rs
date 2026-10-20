//! A window's link against a Server played by the test.

use super::*;
use crate::engine::{SpawnRequest, Spawner};
use crate::link::{Connection, Heard};
use crate::protocol::{SharedRoster, WireChoice};
use crate::store::Provider;

struct NoDiscovery;

impl Spawner for NoDiscovery {
    fn spawn(&mut self, _: SpawnRequest) -> io::Result<Box<dyn crate::providers::Session>> {
        Err(io::Error::other("a window spawns no Session"))
    }
}

/// A Server that welcomes every window and answers no command: it hands
/// each one to the test, and lets the window go at the `drop_at`th.
fn server_answering_nothing(store: &std::path::Path, drop_at: usize) -> Receiver<Op> {
    server_answering(store, drop_at, |_| None)
}

/// A Server that welcomes every window and hands each command to the
/// test, replying what `reply` makes of it; it lets the window go at the
/// `drop_at`th.
fn server_answering(
    store: &std::path::Path,
    drop_at: usize,
    reply: fn(&Op) -> Option<Result<Done, WireError>>,
) -> Receiver<Op> {
    std::fs::create_dir_all(store).unwrap();
    let listener = link::listen(&link::address(store)).unwrap();
    let (took, taken) = mpsc::channel();
    std::thread::spawn(move || {
        let (heard, hear) = mpsc::channel();
        // The window probes the socket before it attaches: serve every
        // connection, and play the Server to the one that says hello.
        std::thread::spawn(move || {
            for id in 1.. {
                let Ok(stream) = listener.accept() else {
                    return;
                };
                if Connection::serve(id, stream, heard.clone()).is_err() {
                    return;
                }
            }
        });
        let mut connections = BTreeMap::new();
        let mut commands = 0;
        for heard in hear {
            match heard {
                Heard::Joined(id, connection) => {
                    connections.insert(id, connection);
                }
                Heard::Message(id, ClientMessage::Hello { .. }) => {
                    let connection = &connections[&id];
                    let info = ServerInfo {
                        version: "0.9.0".into(),
                        executable: "/bin/ferrite".into(),
                    };
                    connection
                        .send(protocol::welcome(1, info, "b-1".into(), 0))
                        .unwrap();
                    connection
                        .send(ServerMessage::Snapshot(Snapshot {
                            boot_id: "b-1".into(),
                            revision: 1,
                            threads: Vec::new(),
                            roster: SharedRoster::default(),
                            decisions: Vec::new(),
                            groups: None,
                            registry: None,
                            notices: None,
                            providers: None,
                            parked: None,
                            digests: None,
                        }))
                        .unwrap();
                }
                Heard::Message(id, ClientMessage::Command(command)) => {
                    commands += 1;
                    if commands == drop_at {
                        connections.remove(&id);
                    }
                    if let (Some(connection), Some(answer)) =
                        (connections.get(&id), reply(&command.op))
                    {
                        let (ok, error) = match answer {
                            Ok(done) => (Some(done), None),
                            Err(error) => (None, Some(error)),
                        };
                        let id = command.id;
                        connection
                            .send(ServerMessage::Reply { id, ok, error })
                            .unwrap();
                    }
                    if took.send(command.op).is_err() {
                        return;
                    }
                }
                _ => {}
            }
        }
    });
    taken
}

fn remote(store: &std::path::Path) -> Remote {
    let mut remote = Remote::new(Attach {
        store: store.to_path_buf(),
        server: Endpoint::Local {
            executable: "/bin/false".into(),
        },
        client: Peer {
            name: "test".into(),
            version: "0".into(),
        },
        renderer: None,
        discovery: Box::new(NoDiscovery),
    });
    remote.attach_now();
    remote
}

fn scratch(name: &str) -> std::path::PathBuf {
    let store = std::env::temp_dir().join(format!("fr-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&store);
    store
}

/// A first send in flight when an act finds the Server gone is settled
/// at the next poll: its Draft hears it may not have started, and is no
/// longer starting, so it can be sent again.
#[test]
fn a_first_send_the_connection_took_with_it_is_settled() {
    let store = scratch("gone");
    let _taken = server_answering_nothing(&store, 2);
    let mut remote = remote(&store);
    let draft = DraftId::new(1);
    remote
        .bootstrap(
            draft,
            "go",
            Op::Bootstrap {
                provider: Provider::Codex,
                model: None,
                effort: None,
                workspace: WireChoice::Main {
                    checkout: store.clone(),
                },
                prompt: "go".into(),
                group: None,
                new_group_with: None,
            },
        )
        .unwrap();
    assert!(remote.draft_starting(draft));

    assert!(remote.execute(Op::SettingsChanged).is_err());
    let settled: Vec<_> = remote
        .poll()
        .into_iter()
        .filter_map(|arrival| match arrival {
            Arrival::Started(started) => Some(started),
            _ => None,
        })
        .collect();

    assert_eq!(settled.len(), 1);
    assert_eq!(settled[0].draft, Some(draft));
    assert!(settled[0].result.is_err());
    assert!(!remote.draft_starting(draft));
    let _ = std::fs::remove_dir_all(&store);
}

/// An answer in flight when the connection goes may have arrived: the
/// window says so, attaches again, and never sends it again — the
/// snapshot says whether the Decision is still pending.
#[test]
fn an_answer_in_flight_across_a_disconnect_is_not_resent() {
    let store = scratch("answer");
    let taken = server_answering_nothing(&store, 1);
    let mut remote = remote(&store);

    let answered = remote.execute(Op::Answer {
        thread: ThreadId::new(1),
        handle: protocol::WireHandle {
            generation: 1,
            serial: 1,
            request_id: "perm_01".into(),
        },
        answer: protocol::WireAnswer::Allow {
            input: serde_json::json!({}),
        },
    });
    let error = answered.unwrap_err();
    assert!(error.message.contains("may not have arrived"), "{error:?}");
    assert!(matches!(
        taken.recv_timeout(Duration::from_secs(5)),
        Ok(Op::Answer { .. })
    ));

    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(remote.state(), ServerState::Attached { .. }) {
        assert!(Instant::now() < deadline, "never attached again");
        remote.poll();
        std::thread::sleep(Duration::from_millis(10));
    }
    remote.poll();
    assert!(
        taken.recv_timeout(Duration::from_millis(300)).is_err(),
        "the answer was sent again"
    );
    let _ = std::fs::remove_dir_all(&store);
}

/// A machine that needs a sign-in is not asked again on its own — each ask
/// is a refused login on it — and is asked as soon as the operator has
/// signed in.
#[test]
fn a_machine_waiting_for_a_sign_in_is_not_asked_again_until_then() {
    let store = scratch("sign-in");
    let dials = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = Arc::clone(&dials);
    let bridge = Arc::new(Bridge::new(
        "box",
        store.join("bridge.sock"),
        Box::new(move || {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(machine::Tunnel {
                from: Box::new(io::empty()),
                to: Box::new(io::sink()),
                ended: Box::new(|| Some(Failure::SignIn)),
            })
        }),
    ));
    let mut remote = Remote::new(Attach {
        store: store.clone(),
        server: Endpoint::Machine(Arc::clone(&bridge)),
        client: Peer {
            name: "test".into(),
            version: "0".into(),
        },
        renderer: None,
        discovery: Box::new(NoDiscovery),
    });
    let attention = |remote: &Remote| {
        matches!(
            remote.state(),
            ServerState::Detached {
                attention: Some(Failure::SignIn),
                ..
            }
        )
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while !attention(&remote) {
        assert!(Instant::now() < deadline, "timed out");
        remote.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(dials.load(std::sync::atomic::Ordering::SeqCst), 1);

    std::thread::sleep(RETRY_FIRST * 4);
    remote.poll();
    assert_eq!(
        dials.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "not asked again on its own"
    );

    bridge.signed_in();
    while dials.load(std::sync::atomic::Ordering::SeqCst) < 2 {
        assert!(Instant::now() < deadline, "timed out");
        remote.poll();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A Server from before `review_read` answers it `unsupported`: the window
/// asks its `review_diff` instead and reads the diff's hunks as the files'
/// sides. A last turn or a run of commits it cannot read is refused, never
/// asked as something less.
#[test]
fn a_review_falls_back_to_review_diff_on_an_older_server() {
    use ferrite_reviewr::ferrite::Read;
    use ferrite_reviewr::model::CommitPick;
    let store = scratch("older-review");
    let taken = server_answering(&store, 0, |op| match op {
        Op::ReviewDiff { .. } => Some(Ok(Done::Diff {
            text: "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -2 +2 @@\n-one\n+two\n"
                .into(),
        })),
        _ => Some(Err(WireError {
            code: ErrorCode::Unsupported,
            message: "this server does not know that".into(),
        })),
    });
    let mut remote = remote(&store);
    let read = |remote: &mut Remote, read: Read| {
        let answered = remote.review_diff(Path::new("/repo"), &read).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(Instant::now() < deadline, "never answered");
            remote.poll();
            if let Ok(files) = answered.try_recv() {
                return files;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    };

    let files = read(&mut remote, Read::Uncommitted).unwrap();
    assert!(matches!(taken.recv().unwrap(), Op::ReviewRead { .. }));
    assert!(matches!(
        taken.recv().unwrap(),
        Op::ReviewDiff {
            scope: protocol::WireScope::Uncommitted,
            ..
        }
    ));
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].file.path, "a.rs");
    assert_eq!(files[0].sides, Ok(("\none\n".into(), "\ntwo\n".into())));

    let run = Read::Commits(CommitPick {
        oldest: "1f2e3d4c".into(),
        newest: "9a31c76e".into(),
    });
    for refused in [run, Read::LastTurn] {
        let error = read(&mut remote, refused).unwrap_err();
        assert!(error.to_string().contains("too old"), "{error}");
        assert!(matches!(taken.recv().unwrap(), Op::ReviewRead { .. }));
    }
    assert!(
        taken.recv_timeout(Duration::from_millis(100)).is_err(),
        "a run or a last turn is never asked as a review_diff"
    );
    let _ = std::fs::remove_dir_all(&store);
}
