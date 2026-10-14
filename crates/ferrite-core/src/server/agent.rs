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
    // The LaunchAgent serves the operator's own Store; a dev or test Store
    // gets a Server of its own. launchd runs the job it loaded, which can
    // name a bundle that has since moved until the next login: when its
    // Server never answers, start this one.
    if operators_store(store) && kickstart() && answers(&address).is_ok() {
        return Ok(());
    }
    start_detached(executable, &[])?;
    answers(&address)
}

/// Wait up to [`START`] for a Server to answer at `address`.
fn answers(address: &Path) -> io::Result<()> {
    let deadline = Instant::now() + START;
    loop {
        match link::connect(address) {
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

/// Whether `store` is the operator's own, `~/.ferrite/threads`: the one
/// the LaunchAgent's Server serves.
fn operators_store(store: &Path) -> bool {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .is_some_and(|home| Path::new(&home).join(".ferrite").join("threads") == store)
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

/// The LaunchAgent's label, and the Run key's value name on Windows.
pub const LABEL: &str = "com.github.josephwylie.ferrite.server";

/// The LaunchAgent: the Server at login, and again after a crash — not
/// after a clean stop (`SuccessfulExit = false`). No `ProcessType`: a
/// Background agent is throttled, and the agents' builds run under it.
pub fn plist(executable: &Path) -> String {
    let executable = executable
        .display()
        .to_string()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{executable}</string>
		<string>server</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<dict>
		<key>SuccessfulExit</key>
		<false/>
	</dict>
</dict>
</plist>
"#
    )
}

/// Settings › Server › "Start at login". On: the LaunchAgent is written
/// (rewritten when the bundle moved) and loaded, a label already loaded
/// counting as done. Off: only the plist goes — `bootout` would SIGTERM
/// the running Server, so the job stays loaded until logout and nothing
/// starts it at the next login. On Windows the Run key, the same way.
pub fn start_at_login(executable: &Path, on: bool) -> io::Result<()> {
    platform::start_at_login(executable, on)
}

/// Start the installed LaunchAgent's Server; false when there is none.
fn kickstart() -> bool {
    platform::kickstart()
}

#[cfg(target_os = "macos")]
mod platform {
    use std::io;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use super::{plist, LABEL};

    fn plist_path() -> io::Result<PathBuf> {
        let home = std::env::var_os("HOME")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no HOME"))?;
        Ok(PathBuf::from(home)
            .join("Library/LaunchAgents")
            .join(format!("{LABEL}.plist")))
    }

    fn domain() -> String {
        // SAFETY: getuid has no preconditions and cannot fail.
        format!("gui/{}", unsafe { libc::getuid() })
    }

    fn launchctl(args: &[&str]) -> io::Result<bool> {
        Ok(Command::new("/bin/launchctl")
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?
            .success())
    }

    pub(super) fn start_at_login(executable: &Path, on: bool) -> io::Result<()> {
        let path = plist_path()?;
        if !on {
            return match std::fs::remove_file(&path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            };
        }
        let wanted = plist(executable);
        if std::fs::read_to_string(&path).ok().as_deref() != Some(wanted.as_str()) {
            std::fs::create_dir_all(path.parent().expect("under LaunchAgents"))?;
            crate::store::replace_file(&path, wanted.as_bytes())?;
        }
        let service = format!("{}/{LABEL}", domain());
        if launchctl(&["print", &service])? {
            return Ok(());
        }
        let path = path.to_string_lossy();
        if launchctl(&["bootstrap", &domain(), &path])? {
            Ok(())
        } else {
            Err(io::Error::other(
                "launchctl could not load the Ferrite server",
            ))
        }
    }

    pub(super) fn kickstart() -> bool {
        plist_path().is_ok_and(|path| path.exists())
            && launchctl(&["kickstart", &format!("{}/{LABEL}", domain())]).unwrap_or(false)
    }
}

#[cfg(windows)]
mod platform {
    use std::io;
    use std::path::Path;
    use std::process::Command;

    use super::LABEL;
    use crate::spawn::NoConsoleWindow;

    const RUN: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    pub(super) fn start_at_login(executable: &Path, on: bool) -> io::Result<()> {
        let mut reg = Command::new("reg");
        if on {
            let value = format!("\"{}\" server", executable.display());
            reg.args(["add", RUN, "/v", LABEL, "/t", "REG_SZ", "/d", &value, "/f"]);
        } else {
            reg.args(["delete", RUN, "/v", LABEL, "/f"]);
        }
        let status = reg.no_console_window().status()?;
        // Deleting a value that is not there is the state asked for.
        if status.success() || !on {
            Ok(())
        } else {
            Err(io::Error::other("the Run key could not be written"))
        }
    }

    /// Windows has no supervisor to ask: the window starts the Server.
    pub(super) fn kickstart() -> bool {
        false
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use std::io;
    use std::path::Path;

    pub(super) fn start_at_login(_: &Path, _: bool) -> io::Result<()> {
        Ok(())
    }

    pub(super) fn kickstart() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plist launchd reads, byte for byte: what it runs, when, and that
    /// a clean stop stays stopped.
    #[test]
    fn the_launch_agent_plist_is_as_launchd_reads_it() {
        let written = plist(Path::new(
            "/Applications/Ferrite & Co.app/Contents/MacOS/ferrite",
        ));
        assert_eq!(written, include_str!("fixtures/launch_agent.plist"));
    }
}
