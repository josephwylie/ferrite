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
    // The LaunchAgent (the systemd user service on Linux) serves the
    // operator's own Store; a dev or test Store gets a Server of its own.
    // launchd runs the job it loaded, which can name a bundle that has
    // since moved until the next login: when its Server never answers,
    // start this one.
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
    // A session of its own: the hangup when the session that started it
    // ends (`ferrited bridge`'s ssh session, ADR 0026) never reaches it.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid is async-signal-safe, and nothing else runs
        // between fork and exec.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
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

/// Start the installed LaunchAgent's Server (the systemd user service's on
/// Linux); false when there is none.
fn kickstart() -> bool {
    platform::kickstart()
}

#[cfg(target_os = "linux")]
pub use platform::service;

/// The Server as a `systemd --user` service: the LaunchAgent's counterpart
/// on a Linux machine reached over ssh (ADR 0026), so it outlives the ssh
/// session and comes back after a crash. Every command goes through `run`
/// — a program and its arguments, to its stdout or why it failed — the
/// seam the tests script.
#[cfg(any(target_os = "linux", test))]
mod systemd {
    use std::io;
    use std::path::Path;

    pub(super) const UNIT: &str = "ferrited.service";

    pub(super) type Run<'a> = &'a mut dyn FnMut(&str, &[&str]) -> io::Result<String>;

    /// The unit: `ferrited server` at login (or at boot, for a user who
    /// lingers), and again after a crash — not after a clean stop, which
    /// includes a Server that found the Store taken. The default
    /// `KillMode` stops the agents with the Server, as on macOS.
    pub(super) fn unit(executable: &Path) -> String {
        let executable = executable
            .display()
            .to_string()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
            .replace('$', "$$");
        format!(
            "[Unit]
Description=Ferrite server

[Service]
ExecStart=\"{executable}\" server
Restart=on-failure

[Install]
WantedBy=default.target
"
        )
    }

    /// Write the unit at `path` (rewritten when `ferrited` moved), then
    /// enable and start it. Returns what the operator should hear: when a
    /// Server already answers (`server_answers`) and it is not the unit's,
    /// the unit's Server finds the Store taken and stops; when logind says
    /// `uid` does not linger, the user manager — and the Server with it —
    /// stops at the last logout.
    pub(super) fn install(
        path: &Path,
        executable: &Path,
        uid: u32,
        server_answers: bool,
        run: Run,
    ) -> io::Result<Vec<&'static str>> {
        let wanted = unit(executable);
        if std::fs::read_to_string(path).ok().as_deref() != Some(wanted.as_str()) {
            std::fs::create_dir_all(path.parent().expect("under systemd/user"))?;
            crate::store::replace_file(path, wanted.as_bytes())?;
        }
        let outside =
            server_answers && run("systemctl", &["--user", "is-active", "--quiet", UNIT]).is_err();
        run("systemctl", &["--user", "daemon-reload"])?;
        run("systemctl", &["--user", "enable", "--now", UNIT])?;
        let mut hints = Vec::new();
        if outside {
            hints.push(
                "a Server started before the service still runs, outside it; \
                 the service takes over at the first connection after that Server stops",
            );
        }
        let linger = run(
            "loginctl",
            &["show-user", &uid.to_string(), "--property=Linger"],
        );
        if linger.is_ok_and(|said| said.trim() == "Linger=no") {
            hints.push(
                "the Server stops when your last session ends; \
                 to keep it and its agents running, run: loginctl enable-linger",
            );
        }
        Ok(hints)
    }

    /// Disable the unit and remove it, by hand — `systemctl disable` needs
    /// the user bus and the unit file both, and either may be gone — then
    /// tell the user manager, if it can be reached: one that cannot reads
    /// the directory afresh when it starts. As on macOS, the running
    /// Server is not stopped — that would end its agents — so it runs
    /// until it stops.
    pub(super) fn uninstall(path: &Path, run: Run) -> io::Result<()> {
        let wants = path
            .parent()
            .expect("under systemd/user")
            .join("default.target.wants")
            .join(UNIT);
        for file in [wants.as_path(), path] {
            match std::fs::remove_file(file) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        let _ = run("systemctl", &["--user", "daemon-reload"]);
        Ok(())
    }

    /// Start the installed unit's Server; false when there is none, or
    /// systemd would not start it.
    pub(super) fn kickstart(path: &Path, run: Run) -> bool {
        path.exists() && run("systemctl", &["--user", "start", UNIT]).is_ok()
    }
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

#[cfg(target_os = "linux")]
mod platform {
    use std::io;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    use super::systemd;
    use crate::link;

    /// Settings › Server › "Start at login" is the window's; on Linux the
    /// operator installs the service (`ferrited service install`).
    pub(super) fn start_at_login(_: &Path, _: bool) -> io::Result<()> {
        Ok(())
    }

    pub(super) fn kickstart() -> bool {
        unit_path().is_ok_and(|path| systemd::kickstart(&path, &mut run))
    }

    /// `ferrited service install|uninstall`: the process's exit code.
    pub fn service(executable: &Path, action: Option<&str>) -> i32 {
        let done = match action {
            Some("install") => install(executable),
            Some("uninstall") => uninstall(),
            _ => {
                eprintln!("usage: ferrited service install | ferrited service uninstall");
                return 2;
            }
        };
        match done {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("ferrited: {error}");
                1
            }
        }
    }

    fn install(executable: &Path) -> io::Result<()> {
        // SAFETY: getuid has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        let answers = link::connect(&link::address(&crate::server::store_dir())).is_ok();
        let hints = systemd::install(&unit_path()?, executable, uid, answers, &mut run)?;
        println!(
            "ferrited: the Server runs as the user service {}",
            systemd::UNIT
        );
        for hint in hints {
            println!("ferrited: {hint}");
        }
        Ok(())
    }

    fn uninstall() -> io::Result<()> {
        systemd::uninstall(&unit_path()?, &mut run)?;
        println!(
            "ferrited: {} removed; a running Server runs until it stops",
            systemd::UNIT
        );
        Ok(())
    }

    /// Where systemd looks for the user's own units.
    fn unit_path() -> io::Result<PathBuf> {
        let config = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(config) => PathBuf::from(config),
            None => std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(".config"))
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no HOME"))?,
        };
        Ok(config.join("systemd/user").join(systemd::UNIT))
    }

    fn run(program: &str, args: &[&str]) -> io::Result<String> {
        let output = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .output()?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }
        Err(io::Error::other(format!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
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

    /// The unit systemd reads, byte for byte: the executable quoted so a
    /// space, a quote, `%` (a specifier) or `$` (a variable) stays itself.
    #[test]
    fn the_systemd_unit_is_as_systemd_reads_it() {
        let written = systemd::unit(Path::new(r#"/home/me/my "tools"/50%/ferrited$"#));
        assert_eq!(written, include_str!("fixtures/ferrited.service"));
    }

    #[test]
    fn installing_writes_the_unit_then_enables_and_starts_it() {
        let path = scratch("install").join("systemd/user").join(systemd::UNIT);
        let executable = Path::new("/usr/local/bin/ferrited");
        let mut said = Vec::new();
        let hints = systemd::install(
            &path,
            executable,
            1000,
            false,
            &mut |program: &str, args: &[&str]| {
                said.push(spelled(program, args));
                Ok("Linger=no\n".into())
            },
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            systemd::unit(executable)
        );
        assert_eq!(
            said,
            [
                "systemctl --user daemon-reload",
                "systemctl --user enable --now ferrited.service",
                "loginctl show-user 1000 --property=Linger",
            ]
        );
        assert!(matches!(&hints[..], [hint] if hint.contains("loginctl enable-linger")));
    }

    #[test]
    fn a_user_who_lingers_hears_no_hint() {
        let path = scratch("lingers").join(systemd::UNIT);
        let hints = systemd::install(
            &path,
            Path::new("/bin/ferrited"),
            1000,
            false,
            &mut |_: &str, _: &[&str]| Ok("Linger=yes\n".into()),
        );
        assert!(hints.unwrap().is_empty());
    }

    /// A Server `ferrited bridge` started detached keeps the Store: the
    /// unit's Server finds it taken and stops, and the operator hears so.
    /// One that is the unit's own is said nothing about.
    #[test]
    fn a_server_running_outside_the_unit_is_told_of() {
        let path = scratch("outside").join(systemd::UNIT);
        let install = |active: bool| {
            systemd::install(
                &path,
                Path::new("/bin/ferrited"),
                1000,
                true,
                &mut |_: &str, args: &[&str]| match args {
                    [_, "is-active", ..] if !active => Err(io::Error::other("inactive")),
                    _ => Ok("Linger=yes\n".into()),
                },
            )
            .unwrap()
        };
        assert!(matches!(&install(false)[..], [hint] if hint.contains("outside")));
        assert!(install(true).is_empty());
    }

    #[test]
    fn installing_fails_when_systemd_will_not_enable_it() {
        let path = scratch("refused").join(systemd::UNIT);
        let installed = systemd::install(
            &path,
            Path::new("/bin/ferrited"),
            1000,
            false,
            &mut |_: &str, args: &[&str]| {
                if args.contains(&"enable") {
                    Err(io::Error::other("Failed to connect to bus"))
                } else {
                    Ok(String::new())
                }
            },
        );
        assert!(installed.is_err());
    }

    /// The unit and its `default.target.wants` link go even when the user
    /// bus cannot be reached, or the unit was already deleted by hand.
    #[test]
    fn uninstalling_removes_the_unit_and_its_link_but_never_stops_it() {
        let dir = scratch("uninstall");
        let path = dir.join(systemd::UNIT);
        let wants = dir.join("default.target.wants").join(systemd::UNIT);
        std::fs::create_dir_all(wants.parent().unwrap()).unwrap();
        std::fs::write(&wants, "").unwrap();
        let mut said = Vec::new();
        let mut run = |program: &str, args: &[&str]| {
            said.push(spelled(program, args));
            Err(io::Error::other("Failed to connect to bus"))
        };
        systemd::uninstall(&path, &mut run).unwrap();
        assert!(!wants.exists());

        std::fs::write(&path, systemd::unit(Path::new("/bin/ferrited"))).unwrap();
        systemd::uninstall(&path, &mut run).unwrap();
        assert!(!path.exists());
        assert_eq!(said, ["systemctl --user daemon-reload"; 2]);
    }

    /// `ensure_running`'s branch: the unit's Server when there is a unit
    /// systemd starts, else (false) a detached one.
    #[test]
    fn the_service_is_started_only_when_its_unit_is_installed() {
        let path = scratch("kickstart").join(systemd::UNIT);
        let mut said = Vec::new();
        let mut run = |program: &str, args: &[&str]| {
            said.push(spelled(program, args));
            Ok(String::new())
        };
        assert!(!systemd::kickstart(&path, &mut run));
        std::fs::write(&path, "").unwrap();
        assert!(systemd::kickstart(&path, &mut run));
        assert_eq!(said, ["systemctl --user start ferrited.service"]);

        let mut refused = |_: &str, _: &[&str]| Err(io::Error::other("no user manager"));
        assert!(!systemd::kickstart(&path, &mut refused));
    }

    fn spelled(program: &str, args: &[&str]) -> String {
        format!("{program} {}", args.join(" "))
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ferrite-agent-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
