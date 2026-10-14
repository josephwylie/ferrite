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

/// A Server that welcomes the window, takes `commands` commands without
/// answering any, then goes.
fn server_answering_nothing(store: &std::path::Path, commands: usize) {
    std::fs::create_dir_all(store).unwrap();
    let listener = link::listen(&link::address(store)).unwrap();
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
        let mut taken = 0;
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
                Heard::Message(id, ClientMessage::Command(_)) => {
                    taken += 1;
                    if taken == commands {
                        connections.remove(&id);
                        return;
                    }
                }
                _ => {}
            }
        }
    });
}

/// A first send in flight when an act finds the Server gone is settled
/// at the next poll: its Draft hears it may not have started, and is no
/// longer starting, so it can be sent again.
#[test]
fn a_first_send_the_connection_took_with_it_is_settled() {
    let store = std::env::temp_dir().join(format!("fr-{}-gone", std::process::id()));
    let _ = std::fs::remove_dir_all(&store);
    server_answering_nothing(&store, 2);
    let (mut remote, _) = Remote::new(Attach {
        store: store.clone(),
        executable: "/bin/false".into(),
        client: Peer {
            name: "test".into(),
            version: "0".into(),
        },
        renderer: None,
        discovery: Box::new(NoDiscovery),
    });
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
