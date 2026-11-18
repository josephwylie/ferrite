use super::*;
use crate::file_operations::{Reply, Request, fingerprint};

#[test]
fn file_operations_execute_on_the_server_and_preserve_conflicts() {
    let harness = start("file-operations");
    let root = harness.dir.join("workspace");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("remote.txt");
    std::fs::write(&path, "remote").unwrap();
    let mut window = attach(&harness.address);
    let reply = window.command(
        1,
        Op::FileOperation {
            request: Request::Read {
                root: root.clone(),
                path: path.clone(),
            },
        },
    );
    assert!(
        matches!(reply, ServerMessage::Reply { ok: Some(Done::FileOperation { reply: Reply::Text { ref text, .. } }), .. } if text == "remote"),
        "{reply:?}"
    );
    let reply = window.command(
        2,
        Op::FileOperation {
            request: Request::Write {
                root: root.clone(),
                path: path.clone(),
                text: "saved remotely".into(),
                expected: Some(fingerprint(b"remote")),
            },
        },
    );
    assert!(
        matches!(
            reply,
            ServerMessage::Reply {
                ok: Some(Done::FileOperation {
                    reply: Reply::Written { .. }
                }),
                ..
            }
        ),
        "{reply:?}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "saved remotely");
    let reply = window.command(
        3,
        Op::FileOperation {
            request: Request::Write {
                root: root.clone(),
                path: path.clone(),
                text: "stale".into(),
                expected: Some(fingerprint(b"remote")),
            },
        },
    );
    assert!(
        matches!(
            reply,
            ServerMessage::Reply {
                ok: Some(Done::FileOperation {
                    reply: Reply::Conflict
                }),
                ..
            }
        ),
        "{reply:?}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "saved remotely");
    let reply = window.command(
        4,
        Op::FileOperation {
            request: Request::Snapshot {
                root,
                directories: Vec::new(),
                search: true,
            },
        },
    );
    assert!(
        matches!(reply, ServerMessage::Reply { ok: Some(Done::FileOperation { reply: Reply::Snapshot(ref snapshot) }), .. } if snapshot.files.iter().any(|entry| entry.path == path)),
        "{reply:?}"
    );
}
