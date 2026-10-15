//! `ferrited bridge` (ADR 0026): what a window's `ssh` runs on a machine.
//! It makes sure the machine's Server runs, then carries stdin to the
//! Server's socket and the socket to stdout until either ends — one
//! window's connection, spoken over ssh's stdio. `ferrited bridge --api`
//! is the CLI's (ADR 0027): the same, to the Server's API, and like the
//! CLI it never starts a Server.

use std::io;
use std::path::Path;

use crate::link::{self, TryClone};

/// Bridge a window until it or the Server goes.
pub fn run(store: &Path, executable: &Path) -> i32 {
    carry(
        super::agent::ensure_running(store, executable)
            .and_then(|()| link::connect(&link::address(store))),
    )
}

/// Bridge one CLI call to the API, or say there is no Server to answer it.
pub fn api(store: &Path) -> i32 {
    let address = crate::api::address(store);
    carry(link::connect(&address).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("no Ferrite server at {}: {error}", address.display()),
        )
    }))
}

/// Carry stdio over `connected`. What fails is said on stderr, which ssh
/// hands back to the caller.
fn carry(connected: io::Result<link::Stream>) -> i32 {
    let bridged = connected.and_then(|mut stream| {
        let mut upload = stream.try_clone()?;
        std::thread::Builder::new()
            .name("ferrited-bridge-up".into())
            .spawn(move || {
                let _ = io::copy(&mut io::stdin(), &mut upload);
                // The caller went: so does the download below.
                let _ = upload.shutdown();
            })?;
        link::relay(&mut stream, &mut io::stdout())
    });
    match bridged {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("ferrited: {error}");
            1
        }
    }
}
