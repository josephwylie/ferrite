//! `ferrite server`, the binary itself (ADR 0025): it starts on a Store of
//! its own, a window says hello and gets a snapshot, and `stop` ends it
//! cleanly. A second Server for the same Store steps aside with success.

use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use ferrite_core::link::{self, Client};
use ferrite_core::protocol::{self, ClientMessage, Op, Peer, ServerMessage, When};

fn server(store: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_ferrite"))
        .arg("server")
        .env("FERRITE_STORE", store)
        // A terminal's launch: the login shell is not asked.
        .env("TERM", "dumb")
        .spawn()
        .expect("the binary starts")
}

fn attach(address: &Path) -> link::Attached {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let hello = ClientMessage::Hello {
            generation: protocol::GENERATION,
            client: Peer {
                name: "smoke".into(),
                version: "0".into(),
            },
            capabilities: Vec::new(),
        };
        match Client::attach(address, hello) {
            Ok(attached) => return attached,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => panic!("the server never answered: {error}"),
        }
    }
}

fn exited(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < deadline, "the server did not exit");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_server_says_hello_and_stops_on_request() {
    let root = std::env::temp_dir().join(format!("ferrite-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let store = root.join("threads");
    std::fs::create_dir_all(&store).unwrap();
    let mut first = server(&store);
    let mut attached = attach(&link::address(&store));
    let ServerMessage::Welcome { server: info, .. } = &attached.welcome else {
        panic!("a welcome first");
    };
    assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    assert!(attached.snapshot.threads.is_empty());

    // The Store is taken: a second Server steps aside, and succeeds at it.
    let mut second = server(&store);
    assert!(exited(&mut second).success());

    attached
        .client
        .send(&ClientMessage::Command(protocol::Command {
            id: 1,
            op: Op::Stop { when: When::Now },
        }))
        .unwrap();
    assert!(exited(&mut first).success());
    assert!(root.join("logs").join("server.log").exists());
    let _ = std::fs::remove_dir_all(&root);
}
