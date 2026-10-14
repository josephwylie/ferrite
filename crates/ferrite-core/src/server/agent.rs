//! Keeping a Server running, from the window's side (ADR 0025): find the
//! one that answers for a Store, or start one that outlives the window.

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::link;

/// How long a starting Server has to answer.
const START: Duration = Duration::from_secs(5);

/// Make sure a Server answers for the Store in `store`: when nothing does,
/// start `executable server` detached — its own process group, no stdio,
/// so it lives on when the window goes — and wait for it to listen.
pub fn ensure_running(store: &Path, executable: &Path) -> io::Result<()> {
    let address = link::address(store);
    if link::connect(&address).is_ok() {
        return Ok(());
    }
    start_detached(executable, &[])?;
    let deadline = Instant::now() + START;
    loop {
        match link::connect(&address) {
            Ok(_) => return Ok(()),
            Err(error) if Instant::now() >= deadline => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("the Ferrite server did not start: {error}"),
                ))
            }
            Err(_) => std::thread::sleep(Duration::from_millis(25)),
        }
    }
}

/// Start `executable server …` so it outlives this process.
pub(super) fn start_detached(executable: &Path, args: &[&str]) -> io::Result<()> {
    let mut command = Command::new(executable);
    command
        .arg("server")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        command.creation_flags(
            DETACHED_PROCESS
                | CREATE_NEW_PROCESS_GROUP
                | CREATE_NO_WINDOW
                | CREATE_BREAKAWAY_FROM_JOB,
        );
    }
    let mut child = command.spawn()?;
    // Reaped whenever it ends, which is meant to be after this process.
    std::thread::spawn(move || child.wait());
    Ok(())
}
