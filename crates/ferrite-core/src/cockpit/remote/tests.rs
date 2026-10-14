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
                        }))
                        .unwrap();
                }
                Heard::Message(id, ClientMessage::Command(command)) => {
                    commands += 1;
                    if commands == drop_at {
                        connections.remove(&id);
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
    Remote::new(Attach {
        store: store.to_path_buf(),
        executable: "/bin/false".into(),
        client: Peer {
            name: "test".into(),
            version: "0".into(),
        },
        renderer: None,
        discovery: Box::new(NoDiscovery),
    })
    .0
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
