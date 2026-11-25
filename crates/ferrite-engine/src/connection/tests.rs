//! The Server's connections: the outbox's merging, slots and lag cutoff,
//! histories, the replay ring, and a window attaching end to end.

use super::*;
use ferrite_core::link::{address, connect, listen, Client};
use ferrite_core::protocol::{Live, Peer, ServerInfo, SharedRoster, Snapshot};
use ferrite_core::server::ServerId;
use ferrite_core::thread_log::LogRecord;
use ferrite_core::transcript::Input;
use ferrite_core::SessionEvent;
use std::path::PathBuf;

fn scratch(name: &str) -> PathBuf {
    // Short: a socket path has a hard length limit.
    let dir = std::env::temp_dir().join(format!("fl-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn text(thread: u64, seq: u64, text: &str) -> ServerMessage {
    ServerMessage::Input {
        thread: ThreadId::new(thread),
        seq,
        at_ms: seq,
        input: WireInput::Record(WireRecord::Known(Box::new(
            LogRecord::of(
                &Input::Event(SessionEvent::TextDelta { text: text.into() }),
                None,
            )
            .unwrap(),
        ))),
    }
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

/// Deltas waiting together become one input that says all of them, and
/// carries the last `seq`: nothing is lost, only re-chunked.
#[test]
fn waiting_deltas_of_one_thread_merge_without_loss() {
    let mut outbox = Outbox::default();
    outbox.push(text(1, 1, "Rea")).unwrap();
    outbox.push(text(1, 2, "ding ")).unwrap();
    outbox.push(text(2, 1, "other")).unwrap();
    outbox.push(text(2, 2, " thread")).unwrap();
    outbox.push(text(1, 3, "the parser")).unwrap();

    let sent: Vec<_> = std::iter::from_fn(|| outbox.pop()).collect();
    assert_eq!(
        sent,
        vec![
            text(1, 2, "Reading "),
            text(2, 2, "other thread"),
            text(1, 3, "the parser")
        ]
    );
}

/// A newer state supersedes the waiting one, but never overtakes what was
/// queued after that one: a roster that closes a Thread arrives after the
/// Thread's last inputs.
#[test]
fn a_state_message_supersedes_its_waiting_kind_without_overtaking() {
    let mut outbox = Outbox::default();
    outbox.push(roster(&[1], 1)).unwrap();
    outbox.push(text(1, 9, "last words")).unwrap();
    outbox.push(roster(&[], 2)).unwrap();

    assert_eq!(outbox.pop(), Some(text(1, 9, "last words")));
    assert_eq!(outbox.pop(), Some(roster(&[], 2)));
    assert_eq!(outbox.pop(), None);
}

#[test]
fn a_client_too_far_behind_is_lagged_and_nothing_more_is_queued() {
    let limit = 1 << 20;
    let mut outbox = Outbox::new(limit);
    let chunk = "x".repeat(32 << 10);
    let mut seq = 0;
    let lagged = loop {
        seq += 1;
        // Alternating Threads, so nothing merges.
        if let Err(lagged) = outbox.push(text(seq % 2, seq, &chunk)) {
            break lagged;
        }
        assert!(seq < 64, "never lagged");
    };
    assert_eq!(lagged, Lagged);
    assert!(seq as usize * chunk.len() >= limit);
    assert!(outbox.lagged());
    assert_eq!(outbox.pop(), None);
    assert_eq!(outbox.push(roster(&[1], 1)), Err(Lagged));
}

#[test]
fn the_ring_resumes_only_a_gap_it_still_holds() {
    let mut ring = Ring::default();
    for seq in 1..=3 {
        ring.push(seq, text(1, seq, "a"));
    }
    assert_eq!(ring.since(1), Some(vec![text(1, 2, "a"), text(1, 3, "a")]));
    assert_eq!(ring.since(3), Some(Vec::new()));
    assert_eq!(
        ring.since(0),
        Some(vec![text(1, 1, "a"), text(1, 2, "a"), text(1, 3, "a")])
    );
    assert_eq!(
        ring.since(7),
        None,
        "a seq from the future is another boot's"
    );

    let big = "y".repeat(RING_BYTES / 4);
    for seq in 4..=9 {
        ring.push(seq, text(1, seq, &big));
    }
    assert_eq!(
        ring.since(1),
        None,
        "evicted: the client rebuilds from history"
    );
    assert!(ring.since(8).is_some());
}

/// A client that last saw the Activity before a reset holds a Thread that
/// no longer exists: it rebuilds, however close its `seq`.
#[test]
fn the_ring_never_resumes_across_a_reset() {
    let mut ring = Ring::after(5);
    assert_eq!(ring.since(4), None, "saw only the old Activity");
    assert_eq!(ring.since(5), Some(Vec::new()), "saw the reset");
    ring.push(6, text(1, 6, "new"));
    assert_eq!(ring.since(4), None);
    assert_eq!(ring.since(5), Some(vec![text(1, 6, "new")]));
}

/// The handshake end to end over the socket: hello, welcome, snapshot,
/// then each side's messages in order.
#[test]
fn a_client_attaches_and_both_sides_hear_each_other() {
    let dir = scratch("attach");
    let path = address(&dir);
    let listener = listen(&path).unwrap();
    let (heard_tx, hear) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let stream = listener.accept().unwrap();
        let (joined, joining) = mpsc::channel();
        Connection::serve(7, stream, joined, LAGGED_BYTES).unwrap();
        let Ok(Heard::Joined(7, connection)) = joining.recv() else {
            panic!("joined first");
        };
        std::thread::spawn(move || {
            for heard in joining {
                let _ = heard_tx.send(heard);
            }
        });
        connection
            .send(protocol::welcome(
                1,
                ServerInfo {
                    version: "0.9.0".into(),
                    executable: "/bin/ferrite".into(),
                },
                "b-1".into(),
                0,
                ServerId::new("s-1"),
                0,
            ))
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
                factory: None,
            }))
            .unwrap();
        connection
    });
    let hello = ClientMessage::Hello {
        generation: 1,
        client: Peer {
            name: "test".into(),
            version: "0".into(),
        },
        capabilities: Vec::new(),
    };
    let mut attached = Client::attach(&path, hello.clone()).unwrap();
    assert_eq!(attached.snapshot.boot_id, "b-1");
    let connection = server.join().unwrap();
    assert!(matches!(hear.recv().unwrap(), Heard::Message(7, message) if message == hello));

    attached
        .client
        .send(&ClientMessage::Watch {
            thread: ThreadId::new(4),
            resume: Some(protocol::Resume {
                boot_id: "b-1".into(),
                after: 41,
            }),
        })
        .unwrap();
    assert!(matches!(
        hear.recv().unwrap(),
        Heard::Message(7, ClientMessage::Watch { resume: Some(resume), .. }) if resume.after == 41
    ));
    connection.send(text(4, 42, "again")).unwrap();
    assert_eq!(
        attached.client.messages.recv().unwrap(),
        text(4, 42, "again")
    );

    drop(attached);
    assert!(matches!(
        hear.recv_timeout(std::time::Duration::from_secs(10))
            .expect("dropping the client must disconnect its reader"),
        Heard::Gone(7)
    ));
    drop(connection);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_refused_generation_is_the_attach_error() {
    let dir = scratch("refused");
    let path = address(&dir);
    let listener = listen(&path).unwrap();
    let server = std::thread::spawn(move || {
        let stream = listener.accept().unwrap();
        let (heard, hear) = mpsc::channel();
        Connection::serve(1, stream, heard, LAGGED_BYTES).unwrap();
        let Ok(Heard::Joined(_, connection)) = hear.recv() else {
            panic!("joined first");
        };
        connection
            .send(protocol::welcome(
                2,
                ServerInfo {
                    version: "0.9.0".into(),
                    executable: "/bin/ferrite".into(),
                },
                "b".into(),
                0,
                ServerId::new("s-1"),
                0,
            ))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
    });
    let error = Client::attach(
        &path,
        ClientMessage::Hello {
            generation: 2,
            client: Peer {
                name: "t".into(),
                version: "0".into(),
            },
            capabilities: Vec::new(),
        },
    )
    .err()
    .unwrap();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    server.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_live_input_never_merges() {
    let mut outbox = Outbox::default();
    let live = ServerMessage::Input {
        thread: ThreadId::new(1),
        seq: 2,
        at_ms: 2,
        input: WireInput::Live(Live::Disconnect),
    };
    outbox.push(text(1, 1, "a")).unwrap();
    outbox.push(live.clone()).unwrap();
    outbox.push(text(1, 3, "b")).unwrap();
    let sent: Vec<_> = std::iter::from_fn(|| outbox.pop()).collect();
    assert_eq!(sent, vec![text(1, 1, "a"), live, text(1, 3, "b")]);
}

/// A history keeps its place for its Thread: nothing merges across it,
/// what that Thread is sent after it waits for it, and the bytes it sends
/// never count toward the lag limit.
#[test]
fn a_history_waits_its_turn_outside_the_lag_limit() {
    let mut outbox = Outbox::default();
    let (pieces, history) = mpsc::sync_channel(4);
    outbox.push(text(1, 1, "a")).unwrap();
    outbox.history(ThreadId::new(1), history).unwrap();
    outbox.push(text(1, 2, "b")).unwrap();
    assert_eq!(outbox.pop(), Some(text(1, 1, "a")));
    assert_eq!(outbox.pop(), None, "the history is still being read");
    pieces.send(text(1, 9, "the history")).unwrap();
    drop(pieces);
    assert_eq!(outbox.pop(), Some(text(1, 9, "the history")));
    assert_eq!(outbox.pop(), Some(text(1, 2, "b")));
    assert_eq!(outbox.pop(), None);
}

/// No other Thread waits on a history: what another Thread is sent goes
/// past one still being read.
#[test]
fn another_threads_messages_pass_a_history_still_being_read() {
    let mut outbox = Outbox::default();
    let (_pieces, history) = mpsc::sync_channel::<ServerMessage>(4);
    outbox.history(ThreadId::new(1), history).unwrap();
    outbox.push(text(1, 1, "after its history")).unwrap();
    outbox.push(text(2, 1, "another Thread")).unwrap();
    assert_eq!(outbox.pop(), Some(text(2, 1, "another Thread")));
    assert_eq!(outbox.pop(), None, "Thread 1 waits on its history");
}

/// A roster that closes a Thread is the last its client hears of it: the
/// Thread's history still being read stops, and what waited on it goes
/// unsent, or the client would hold again the Thread the roster closed. A
/// Thread the roster keeps open keeps its history.
#[test]
fn a_roster_that_closes_a_thread_ends_its_history_still_being_read() {
    let mut outbox = Outbox::default();
    let (closed, closed_history) = mpsc::sync_channel(4);
    let (kept, kept_history) = mpsc::sync_channel(4);
    outbox.history(ThreadId::new(1), closed_history).unwrap();
    outbox.push(text(1, 1, "after its history")).unwrap();
    outbox.history(ThreadId::new(2), kept_history).unwrap();
    outbox.push(roster(&[2], 1)).unwrap();
    assert_eq!(outbox.pop(), Some(roster(&[2], 1)));
    assert!(closed.send(text(1, 9, "its history")).is_err(), "its reading stops");
    kept.send(text(2, 9, "still open")).unwrap();
    drop(kept);
    assert_eq!(outbox.pop(), Some(text(2, 9, "still open")));
    assert_eq!(outbox.pop(), None, "nothing of the closed Thread follows");
}

/// Histories read at once take turns: a small one never waits for a big
/// one to finish.
#[test]
fn histories_take_turns() {
    let mut outbox = Outbox::default();
    let (big, big_history) = mpsc::sync_channel(8);
    let (small, small_history) = mpsc::sync_channel(8);
    outbox.history(ThreadId::new(1), big_history).unwrap();
    outbox.history(ThreadId::new(2), small_history).unwrap();
    for seq in 1..=6 {
        big.send(text(1, seq, "big")).unwrap();
    }
    small.send(text(2, 1, "small")).unwrap();
    drop(small);
    assert_eq!(outbox.pop(), Some(text(1, 1, "big")));
    assert_eq!(outbox.pop(), Some(text(2, 1, "small")), "its turn");
}

/// A history's pieces and other messages take turns: neither waits for
/// the other to run out.
#[test]
fn pieces_and_messages_take_turns() {
    let mut outbox = Outbox::default();
    let (pieces, history) = mpsc::sync_channel(8);
    outbox.history(ThreadId::new(1), history).unwrap();
    for seq in 1..=3 {
        pieces.send(text(1, seq, "piece")).unwrap();
    }
    outbox.push(text(2, 1, "a")).unwrap();
    outbox.push(text(3, 1, "b")).unwrap();
    assert_eq!(outbox.pop(), Some(text(1, 1, "piece")));
    assert_eq!(outbox.pop(), Some(text(2, 1, "a")));
    assert_eq!(outbox.pop(), Some(text(1, 2, "piece")));
    assert_eq!(outbox.pop(), Some(text(3, 1, "b")));
}

/// A Thread's histories go one at a time, in order: one asked for again
/// before the first is sent waits for it, never mixes its pieces in.
#[test]
fn a_threads_second_history_waits_for_its_first() {
    let mut outbox = Outbox::default();
    let (first, first_history) = mpsc::sync_channel(8);
    let (second, second_history) = mpsc::sync_channel(8);
    outbox.history(ThreadId::new(1), first_history).unwrap();
    outbox.history(ThreadId::new(1), second_history).unwrap();
    second.send(text(1, 1, "second")).unwrap();
    drop(second);
    first.send(text(1, 2, "first")).unwrap();
    assert_eq!(outbox.pop(), Some(text(1, 2, "first")));
    assert_eq!(outbox.pop(), None, "the first is still being read");
    drop(first);
    assert_eq!(outbox.pop(), Some(text(1, 1, "second")));
}

/// A connection the Server lets go ends its threads and closes its
/// stream, even while the client stays connected: a long-running Server
/// leaks neither a thread nor a descriptor per window.
#[cfg(unix)]
#[test]
fn a_connection_let_go_closes_its_stream() {
    let dir = scratch("dropped");
    let path = address(&dir);
    let listener = listen(&path).unwrap();
    let mut client = connect(&path).unwrap();
    let (heard, hear) = mpsc::channel();
    Connection::serve(1, listener.accept().unwrap(), heard, LAGGED_BYTES).unwrap();
    let Ok(Heard::Joined(1, connection)) = hear.recv() else {
        panic!("joined first");
    };
    connection.send(roster(&[1], 1)).unwrap();
    drop(connection);
    let mut received = Vec::new();
    client.read_to_end(&mut received).unwrap();
    assert_eq!(received, protocol::encode(&roster(&[1], 1)));
    assert!(matches!(
        hear.recv_timeout(std::time::Duration::from_secs(5)),
        Ok(Heard::Gone(1))
    ));
    let _ = std::fs::remove_dir_all(&dir);
}
