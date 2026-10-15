//! `ferrited bridge` (ADR 0026): what a window's `ssh` runs on a machine.
//! It makes sure the machine's Server runs, then carries stdin to the
//! Server's socket and the socket to stdout until either ends — one
//! window's connection, spoken over ssh's stdio.

use std::io;
use std::path::Path;

use crate::link::{self, TryClone};

/// Bridge until the window or the Server goes. What fails is said on
/// stderr, which ssh hands back to the window.
pub fn run(store: &Path, executable: &Path) -> i32 {
    let bridged = super::agent::ensure_running(store, executable)
        .and_then(|()| link::connect(&link::address(store)))
        .and_then(|mut stream| {
            let mut upload = stream.try_clone()?;
            std::thread::Builder::new()
                .name("ferrited-bridge-up".into())
                .spawn(move || {
                    let _ = io::copy(&mut io::stdin(), &mut upload);
                    // The window went: so does the download below.
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
