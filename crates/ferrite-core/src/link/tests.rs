//! The transport: framing, a window's uploads, and the socket's ownership
//! rules.

use super::*;
use crate::protocol::{Peer, ServerInfo, SharedRoster, Snapshot};
use crate::server::ServerId;
use crate::ThreadId;

fn scratch(name: &str) -> PathBuf {
    // Short: a socket path has a hard length limit.
    let dir = std::env::temp_dir().join(format!("fl-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn roster(open: &[u64], revision: u64) -> ServerMessage {
    ServerMessage::Roster {
        revision,
        roster: SharedRoster {
            open: open.iter().map(|id| ThreadId::new(*id)).collect(),
            parked: Vec::new(),
        },
    }
}

#[test]
fn lines_frame_messages_and_refuse_a_cut_line() {
    let bytes = [
        protocol::encode(&roster(&[1], 1)),
        protocol::encode(&roster(&[2], 2)),
    ]
    .concat();
    let mut lines = Lines::new(&bytes[..]);
    assert_eq!(
        lines.next_message::<ServerMessage>().unwrap(),
        Some(roster(&[1], 1))
    );
    assert_eq!(
        lines.next_message::<ServerMessage>().unwrap(),
        Some(roster(&[2], 2))
    );
    assert_eq!(lines.next_message::<ServerMessage>().unwrap(), None);

    let cut = &bytes[..bytes.len() - 3];
    let mut lines = Lines::new(cut);
    lines.next_line().unwrap();
    assert_eq!(
        lines.next_line().unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}

#[cfg(unix)]
#[test]
fn the_socket_is_the_owners_alone_and_a_stale_one_is_replaced() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("perm");
    let path = address(&dir);
    let listener = listen(&path).unwrap();
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&path), 0o600);

    // A second Server for the same Store is refused while the first answers.
    assert_eq!(
        listen(&path).err().map(|error| error.kind()),
        Some(io::ErrorKind::AddrInUse)
    );

    drop(listener);
    assert!(
        !path.exists(),
        "a Server that stops takes its socket with it"
    );

    // A crashed Server leaves its socket behind: nothing answers it, so the
    // next one removes it and listens.
    let stale = dir.join("stale.sock");
    drop(std::os::unix::net::UnixListener::bind(&stale).unwrap());
    assert!(stale.exists());
    let _replaced = listen(&stale).unwrap();
    assert!(connect(&stale).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_connection_is_accepted_from_this_user() {
    let dir = scratch("peer");
    let path = address(&dir);
    let listener = listen(&path).unwrap();
    let _client = connect(&path).unwrap();
    let accepted = listener.accept().unwrap();
    assert!(unix::peer_is_us(&accepted));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A Server that takes the connection and its hello and never welcomes it
/// does not keep a connection given patience waiting past it: the attach
/// ends, timed out.
#[test]
fn an_attach_given_patience_ends_when_the_server_never_welcomes() {
    let dir = scratch("never-welcomed");
    let path = address(&dir);
    let listener = listen(&path).unwrap();
    let (taken, hello_read) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let stream = listener.accept().unwrap();
        let mut input = Lines::new(stream.try_clone().unwrap());
        let hello = input.next_message::<ClientMessage>().unwrap();
        assert!(matches!(hello, Some(ClientMessage::Hello { .. })));
        let _ = taken.send(());
        // Held open, never answered, until the window's end goes.
        while let Ok(Some(_)) = input.next_message::<ClientMessage>() {}
        drop(stream);
    });
    let hello = ClientMessage::Hello {
        generation: 1,
        client: Peer {
            name: "never-welcomed".into(),
            version: "test".into(),
        },
        capabilities: vec![],
    };
    let (ended, attach) = mpsc::channel();
    std::thread::spawn(move || {
        let attached = Client::attach_within(&path, hello, Duration::from_millis(200));
        let _ = ended.send(attached.map(drop));
    });
    hello_read.recv_timeout(Duration::from_secs(10)).expect("the Server took the hello");
    let attached = attach
        .recv_timeout(Duration::from_secs(10))
        .expect("the attach is still waiting for a welcome");
    let error = attached.expect_err("never welcomed");
    assert_eq!(error.kind(), io::ErrorKind::TimedOut, "{error}");
    server.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn browser_annotation_upload_does_not_wait_for_socket_and_keeps_following_messages_ordered() {
    use crate::browser::{Document, Lease, Owner, TabId};
    use crate::browser_annotations::{Capture, Selection, Viewport};
    let dir = scratch("capture-out");
    let path = address(&dir);
    let listener = listen(&path).unwrap();
    let (read, reading) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let mut stream = listener.accept().unwrap();
        let mut input = Lines::new(stream.try_clone().unwrap());
        assert!(matches!(
            input.next_message::<ClientMessage>().unwrap(),
            Some(ClientMessage::Hello { .. })
        ));
        send(
            &mut stream,
            &protocol::welcome(
                1,
                ServerInfo {
                    version: "test".into(),
                    executable: "/fixture/ferrite".into(),
                },
                "capture".into(),
                0,
                ServerId::new("s-1"),
                0,
            ),
        )
        .unwrap();
        send(
            &mut stream,
            &ServerMessage::Snapshot(Snapshot {
                boot_id: "capture".into(),
                revision: 1,
                threads: vec![],
                roster: SharedRoster::default(),
                decisions: vec![],
                groups: None,
                registry: None,
                notices: None,
                providers: None,
                parked: None,
                digests: None,
                factory: None,
            }),
        )
        .unwrap();
        reading
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let first = input.next_message::<ClientMessage>().unwrap().unwrap();
        assert!(matches!(
            first,
            ClientMessage::Command(protocol::Command {
                id: 7,
                op: protocol::Op::BrowserCapture { .. }
            })
        ));
        let second = input.next_message::<ClientMessage>().unwrap().unwrap();
        assert!(
            matches!(second, ClientMessage::Watch { thread, .. } if thread == ThreadId::new(3))
        );
    });
    let mut attached = Client::attach(
        &path,
        ClientMessage::Hello {
            generation: 1,
            client: Peer {
                name: "capture".into(),
                version: "test".into(),
            },
            capabilities: vec![],
        },
    )
    .unwrap();
    let upload = ClientMessage::Command(protocol::Command {
        id: 7,
        op: protocol::Op::BrowserCapture {
            thread: ThreadId::new(3),
            owner: Owner {
                window: 1,
                epoch: 1,
            },
            lease: Lease(9),
            capture: Capture {
                document: Document {
                    tab: TabId(1),
                    generation: 1,
                },
                native_generation: 1,
                url: "https://example.test/".into(),
                viewport: Viewport {
                    width_css: 1.,
                    height_css: 1.,
                    device_scale: 1.,
                },
                selection: Selection::Region {
                    x: 0.,
                    y: 0.,
                    width: 1.,
                    height: 1.,
                },
            },
            png: Some(protocol::WirePng {
                width: 1,
                height: 1,
                base64: "A".repeat(8 * 1024 * 1024),
            }),
        },
    });
    let (queued, returned) = mpsc::channel();
    let (finish, finished) = mpsc::channel();
    let client = std::thread::spawn(move || {
        attached.client.send_capture(upload).unwrap();
        // More than one gesture's worth: asks do not wait for their
        // replies, so a beat queues one for every input.
        for _ in 0..64 {
            attached
                .client
                .send(&ClientMessage::Watch {
                    thread: ThreadId::new(3),
                    resume: None,
                })
                .unwrap();
        }
        queued.send(()).unwrap();
        finished
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
    });
    let nonblocking = returned.recv_timeout(std::time::Duration::from_secs(3));
    read.send(()).unwrap();
    server.join().unwrap();
    finish.send(()).unwrap();
    client.join().unwrap();
    assert!(
        nonblocking.is_ok(),
        "capture and later commands must enqueue before the peer drains screenshot bytes"
    );
    let _ = std::fs::remove_dir_all(dir);
}
