//! Machines (ADR 0026): another computer's Server, reached over SSH. The
//! window attaches to it exactly as to its own Server, through a local
//! socket that bridges each connection to `ssh <target> ferrited bridge`
//! (after Herdr's `SshStdioBridge`). Ferrite owns the SSH connection's
//! master (`ControlMaster`), so one sign-in carries later connections.

use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::link::{self, TryClone};
use crate::protocol::ClientMessage;
use crate::spawn::NoConsoleWindow;

pub mod auth;
pub mod hosts;
pub mod remote;
pub mod setup;

/// A saved machine, as `settings.json` keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    /// What the nav calls it.
    pub name: String,
    /// What `ssh` is given: a host, `user@host`, or an alias from the
    /// operator's `~/.ssh/config`.
    pub target: String,
    /// A disabled machine stays listed and is never connected to.
    #[serde(default = "enabled")]
    pub enabled: bool,
    /// This computer's agent setup is kept the same as the machine's, both
    /// ways (ADR 0048).
    #[serde(default = "enabled")]
    pub sync_setup: bool,
}

fn enabled() -> bool {
    true
}

impl Machine {
    /// A machine named after its target.
    pub fn new(target: &str) -> Self {
        Self {
            name: target.to_string(),
            target: target.to_string(),
            enabled: true,
            sync_setup: true,
        }
    }

    pub fn is_link(&self) -> bool {
        self.target.starts_with("ferrite-link:")
    }

    pub fn display_target(&self, settings_dir: &Path) -> String {
        remote::display_target(self, settings_dir)
    }

    /// When the machine stops taking this paired link, in seconds since
    /// the epoch; `None` over ssh, or for a pairing that was not told.
    pub fn expires(&self, settings_dir: &Path) -> Option<u64> {
        remote::expires(self, settings_dir)
    }

    pub fn from_ssh_fields(host: &str, username: &str, port: &str) -> Result<Self, String> {
        let mut destination = Destination::parse(host.trim())?;
        let username = username.trim();
        if !username.is_empty() {
            check_username(username)?;
            destination.username = Some(username.into());
        }
        let port = port.trim();
        if !port.is_empty() {
            let port = parse_port(port)?;
            destination.port = Some(port);
        }
        Ok(Self::new(&destination.target()))
    }

    pub fn ssh_fields(&self) -> (String, String, String) {
        match Destination::parse(&self.target) {
            Ok(destination) => (
                destination.host,
                destination.username.unwrap_or_default(),
                destination
                    .port
                    .map(|port| port.to_string())
                    .unwrap_or_default(),
            ),
            Err(_) => (self.target.clone(), String::new(), String::new()),
        }
    }
}

/// Whether `target` can be handed to `ssh` as its destination: never an
/// option (`-oProxyCommand=…`), and one word.
pub fn check_target(target: &str) -> Result<(), String> {
    if let Some(id) = target.strip_prefix("ferrite-link:") {
        return remote::check_identity(id);
    }
    Destination::parse(target).map(|_| ())
}

struct Destination {
    host: String,
    username: Option<String>,
    port: Option<u16>,
}

impl Destination {
    fn parse(target: &str) -> Result<Self, String> {
        let target = target.strip_prefix("ssh://").unwrap_or(target);
        if target.is_empty() {
            return Err("Enter an SSH host or alias.".into());
        }
        let (username, address) = match target.split_once('@') {
            Some((username, host)) => {
                check_username(username)?;
                (Some(username.to_string()), host)
            }
            None => (None, target),
        };
        let (host, port) = if let Some(bracketed) = address.strip_prefix('[') {
            let (host, suffix) = bracketed
                .split_once(']')
                .ok_or("Close the IPv6 address with ].")?;
            if host.parse::<std::net::Ipv6Addr>().is_err() {
                return Err("Enter a valid IPv6 address inside brackets.".into());
            }
            let port = if suffix.is_empty() {
                None
            } else {
                Some(parse_port(
                    suffix
                        .strip_prefix(':')
                        .ok_or("Enter a port after the IPv6 address.")?,
                )?)
            };
            (host, port)
        } else if address.matches(':').count() == 1 {
            let (host, port) = address.split_once(':').unwrap();
            (host, Some(parse_port(port)?))
        } else {
            (address, None)
        };
        let valid = if host.contains(':') {
            host.parse::<std::net::Ipv6Addr>().is_ok()
        } else {
            !host.is_empty()
                && !host.starts_with(['-', '.'])
                && host
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        };
        if !valid {
            return Err("Enter a host name, IP address, or SSH config alias without spaces or shell characters.".into());
        }
        Ok(Self {
            host: host.into(),
            username,
            port,
        })
    }

    fn target(&self) -> String {
        let user = self
            .username
            .as_ref()
            .map(|user| format!("{user}@"))
            .unwrap_or_default();
        match self.port {
            Some(port) => {
                let host = if self.host.contains(':') {
                    format!("[{}]", self.host)
                } else {
                    self.host.clone()
                };
                format!("ssh://{user}{host}:{port}")
            }
            None => format!("{user}{}", self.host),
        }
    }
}

fn check_username(username: &str) -> Result<(), String> {
    if username.is_empty()
        || username.starts_with('-')
        || !username
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return Err(
            "Enter an SSH username using letters, numbers, periods, underscores, or hyphens."
                .into(),
        );
    }
    Ok(())
}

fn parse_port(port: &str) -> Result<u16, String> {
    port.bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| port.parse::<u16>().ok())
        .flatten()
        .filter(|&port| port != 0)
        .ok_or_else(|| "Enter a port between 1 and 65535.".into())
}

/// Where Ferrite keeps what it needs for `target`: the bridge's socket and
/// the SSH master's socket. Owner-only, and short, since a socket's path
/// is.
pub fn dir(settings_dir: &Path, target: &str) -> PathBuf {
    let hash = target
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    settings_dir
        .join("machines")
        .join(&format!("{hash:016x}")[..12])
}

/// Why a machine's Server could not be reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The remote shell has no `ferrited`.
    NotInstalled,
    /// `ssh` could not sign in on its own: the operator signs in.
    SignIn,
    /// The machine no longer takes this paired link's session: it expired
    /// or was revoked there, and only a new pairing undoes it.
    Revoked,
    /// Anything else, in ssh's (or the remote bridge's) last words.
    Unreachable(String),
}

impl Failure {
    /// How an `ssh … ferrited bridge` ended, from its exit status and what
    /// it said on stderr. `None`: it ended cleanly.
    pub fn of(status: Option<i32>, stderr: &str) -> Option<Self> {
        const SIGN_IN: [&str; 4] = [
            "Permission denied",
            "Host key verification failed",
            "Too many authentication failures",
            "REMOTE HOST IDENTIFICATION HAS CHANGED",
        ];
        let said = stderr
            .lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .map(str::to_string);
        match status {
            Some(0) => None,
            Some(127) => Some(Self::NotInstalled),
            Some(255) if SIGN_IN.iter().any(|sign| stderr.contains(sign)) => Some(Self::SignIn),
            Some(code) => Some(Self::Unreachable(
                said.unwrap_or_else(|| format!("ssh exited with {code}")),
            )),
            None => Some(Self::Unreachable(
                said.unwrap_or_else(|| "ssh was stopped".into()),
            )),
        }
    }

    /// Retrying will not help until the operator does something.
    pub fn needs_operator(&self) -> bool {
        matches!(self, Self::NotInstalled | Self::SignIn | Self::Revoked)
    }

    /// The operator must sign in before asking again helps: each ask is a
    /// refused login on the machine.
    pub fn needs_sign_in(&self) -> bool {
        matches!(self, Self::SignIn | Self::Revoked)
    }

    /// The machine's `ferrited` is older than `api-bridge`: it refuses the
    /// word with its usage.
    pub fn older_ferrited(&self) -> bool {
        matches!(self, Self::Unreachable(said) if said.starts_with("usage: ferrited"))
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled => f.write_str("`ferrited` is not on the machine's PATH"),
            Self::SignIn => f.write_str("ssh needs you to sign in"),
            Self::Revoked => {
                f.write_str("This machine no longer takes this session. Pair it again with a new code.")
            }
            Self::Unreachable(said) => f.write_str(said),
        }
    }
}

impl std::error::Error for Failure {}

/// The Failure an attach error carries, when the bridge knew why.
pub fn failure(error: &io::Error) -> Option<&Failure> {
    error.get_ref()?.downcast_ref()
}

/// One run of the remote bridge: its stdio, and how it ended.
pub struct Tunnel {
    pub from: Box<dyn Read + Send>,
    pub to: Box<dyn Write + Send>,
    /// Called once `from` has ended: how the run ended.
    pub ended: Box<dyn FnOnce() -> Option<Failure> + Send>,
}

/// Opens a Tunnel: `ssh <target> ferrited bridge`, or a test's fake.
pub type Dial = Box<dyn Fn() -> io::Result<Tunnel> + Send + Sync>;

/// How long ssh's last words are waited for once it has exited.
const STDERR_WAIT: Duration = Duration::from_secs(1);

/// How long a sign-in is waited for before Ferrite stops looking.
const SIGN_IN_WAIT: Duration = Duration::from_secs(600);

/// A machine's local socket: each connection a window makes to it is one
/// Tunnel to the machine's Server. The socket is this process's own, so
/// how a Tunnel failed is heard by the window that asked.
pub struct Bridge {
    name: String,
    socket: PathBuf,
    dial: Arc<Dial>,
    /// The SSH master's socket, for an ssh bridge: what signing in opens.
    ssh: Option<(String, PathBuf)>,
    remote: Option<(String, PathBuf)>,
    /// The socket is being served; false again if its accepting stops.
    listening: Arc<Mutex<bool>>,
    /// How the last Tunnel that failed ended.
    failure: Arc<Mutex<Option<Failure>>>,
    signed_in: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    listener: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl Bridge {
    pub fn for_machine(machine: &Machine, settings_dir: &Path) -> Self {
        if machine.is_link() {
            remote::bridge(machine, settings_dir)
        } else {
            Self::ssh(machine, settings_dir)
        }
    }

    pub fn terminal_command(&self, cwd: &Path) -> io::Result<(String, Vec<String>)> {
        if let Some((target, settings)) = &self.remote {
            let program = std::env::current_exe()?.to_string_lossy().into_owned();
            return Ok((program, vec![
                "--remote-terminal".into(), settings.to_string_lossy().into_owned(),
                target.clone(), cwd.to_string_lossy().into_owned(),
            ]));
        }
        self.terminal_args(cwd).map(|args| ("ssh".into(), args))
    }

    /// The bridge to `machine` over the operator's own `ssh`.
    pub fn ssh(machine: &Machine, settings_dir: &Path) -> Self {
        let dir = dir(settings_dir, &machine.target);
        #[cfg(unix)]
        sweep(&dir);
        let control = dir.join("c");
        let mut bridge = Self::new(
            &machine.name,
            dir.join(format!("bridge-{}.sock", std::process::id())),
            ssh_dial(machine.target.clone(), control.clone(), WINDOW),
        );
        bridge.ssh = Some((machine.target.clone(), control));
        bridge
    }

    /// A bridge at `socket` whose Tunnels `dial` opens.
    pub fn new(name: &str, socket: PathBuf, dial: Dial) -> Self {
        Self {
            name: name.to_string(),
            socket,
            dial: Arc::new(dial),
            ssh: None,
            remote: None,
            listening: Arc::default(),
            failure: Arc::default(),
            signed_in: Arc::default(),
            stopping: Arc::default(),
            listener: Mutex::default(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// SSH arguments for a login shell in an absolute Unix directory on this machine.
    pub fn terminal_args(&self, cwd: &Path) -> io::Result<Vec<String>> {
        let Some((target, control)) = &self.ssh else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "This machine is not reached over SSH.",
            ));
        };
        let cwd = cwd
            .to_str()
            .filter(|cwd| cwd.starts_with('/') && !cwd.contains('\0'))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Choose an absolute remote directory for the terminal.",
                )
            })?;
        let script = format!("cd {} && exec \"${{SHELL:-/bin/sh}}\" -l", quoted(cwd));
        let mut args = vec!["-tt".to_string(), "-o".into(), "BatchMode=yes".into()];
        args.extend(ssh_options(control));
        args.extend([
            "--".into(),
            target.clone(),
            format!("sh -c {}", quoted(&script)),
        ]);
        Ok(args)
    }

    /// Attach to the machine's Server: the window's hello through a fresh
    /// Tunnel. An error carries the [`Failure`] when the Tunnel said why.
    pub fn attach(&self, hello: ClientMessage) -> io::Result<link::Attached> {
        self.listen()?;
        *lock(&self.failure) = None;
        link::Client::attach(&self.socket, hello).map_err(|error| {
            match lock(&self.failure).take() {
                Some(failure) => io::Error::other(failure),
                None => error,
            }
        })
    }

    /// Sign in to the machine in a terminal, where `ssh` asks for whatever
    /// it needs (a password, a code, a new host key). The master it leaves
    /// carries the bridge's connections; [`Bridge::take_signed_in`] says
    /// when it is up.
    pub fn sign_in(&self) -> io::Result<()> {
        let Some((target, control)) = &self.ssh else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "this machine is not reached over ssh",
            ));
        };
        open_terminal(&self.name, target, control)?;
        let (target, control) = (target.clone(), control.clone());
        let signed_in = Arc::clone(&self.signed_in);
        std::thread::Builder::new()
            .name("ferrite-sign-in".into())
            .spawn(move || {
                let deadline = std::time::Instant::now() + SIGN_IN_WAIT;
                while std::time::Instant::now() < deadline {
                    if master_up(&target, &control) {
                        signed_in.store(true, Ordering::Release);
                        return;
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
            })?;
        Ok(())
    }

    /// Whether a sign-in finished since the last call.
    pub fn take_signed_in(&self) -> bool {
        self.signed_in.swap(false, Ordering::AcqRel)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn signed_in(&self) {
        self.signed_in.store(true, Ordering::Release);
    }

    /// Serve the socket, unless it already is.
    fn listen(&self) -> io::Result<()> {
        let mut listening = lock(&self.listening);
        if *listening {
            return Ok(());
        }
        let listener = link::listen(&self.socket)?;
        let dial = Arc::clone(&self.dial);
        let failure = Arc::clone(&self.failure);
        let stopped = Arc::clone(&self.listening);
        let stopping = Arc::clone(&self.stopping);
        let worker = std::thread::Builder::new()
            .name("ferrite-bridge-accept".into())
            .spawn(move || loop {
                match listener.accept() {
                    Ok(stream) => {
                        if stopping.load(Ordering::Acquire) {
                            return;
                        }
                        let (dial, failure) = (Arc::clone(&dial), Arc::clone(&failure));
                        let served = std::thread::Builder::new()
                            .name("ferrite-bridge".into())
                            .spawn(move || carry(stream, &dial, &failure));
                        if let Err(error) = served {
                            crate::log!("machine: a connection could not be bridged: {error}");
                        }
                    }
                    Err(error) => {
                        crate::log!("machine: the bridge stopped accepting: {error}");
                        // The next attach listens again.
                        *lock(&stopped) = false;
                        return;
                    }
                }
            })?;
        *lock(&self.listener) = Some(worker);
        *listening = true;
        Ok(())
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        if let Some(worker) = lock(&self.listener).take() {
            // Keep the wake connection open until Windows' accept observes it.
            let wake = link::connect(&self.socket).ok();
            let _ = worker.join();
            drop(wake);
        }
    }
}

/// Carry one window's connection through a Tunnel, both ways. How the
/// Tunnel ended is recorded before the window's end is closed, so a window
/// that finds its connection gone can read why.
fn carry(mut stream: link::Stream, dial: &Dial, failure: &Mutex<Option<Failure>>) {
    let ended = match dial().and_then(|tunnel| Ok((tunnel, stream.try_clone()?))) {
        Ok((
            Tunnel {
                mut from,
                mut to,
                ended,
            },
            mut upload,
        )) => {
            let uploading = std::thread::Builder::new()
                .name("ferrite-bridge-up".into())
                .spawn(move || {
                    let _ = io::copy(&mut upload, &mut to);
                });
            if uploading.is_ok() {
                let _ = link::relay(&mut from, &mut stream);
            }
            ended()
        }
        Err(error) => Some(self::failure(&error).cloned().unwrap_or_else(|| {
            Failure::Unreachable(format!("The remote connection could not start: {error}"))
        })),
    };
    if let Some(ended) = ended {
        crate::log!("machine: the tunnel ended: {ended}");
        *lock(failure) = Some(ended);
    }
    let _ = stream.shutdown();
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Remove the bridge sockets left in `dir` by Ferrites that are gone.
#[cfg(unix)]
fn sweep(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let pid = name
            .to_str()
            .and_then(|name| name.strip_prefix("bridge-")?.strip_suffix(".sock"))
            .and_then(|pid| pid.parse::<libc::pid_t>().ok());
        // SAFETY: signal 0 only asks whether the process exists.
        if pid.is_some_and(|pid| unsafe { libc::kill(pid, 0) } != 0) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

// ----------------------------------------------------------------- ssh

/// What every `ssh` Ferrite runs is told: keep the connection alive, and
/// (where OpenSSH multiplexes) share Ferrite's own master, so one sign-in
/// carries every connection for ten idle minutes after the last.
fn ssh_options(control: &Path) -> Vec<String> {
    let mut options = vec![
        "-C".to_string(),
        "-o".into(),
        "ServerAliveInterval=15".into(),
        "-o".into(),
        "ServerAliveCountMax=4".into(),
        "-o".into(),
        "ConnectTimeout=15".into(),
        "-o".into(),
        "ConnectionAttempts=1".into(),
        "-o".into(),
        "RemoteCommand=none".into(),
    ];
    // Windows' OpenSSH has no multiplexing: every connection signs in.
    if cfg!(unix) {
        options.extend([
            "-o".into(),
            "ControlMaster=auto".into(),
            "-o".into(),
            format!("ControlPath={}", control.display()),
            "-o".into(),
            "ControlPersist=600".into(),
        ]);
    }
    options
}

/// What a window's ssh runs on the machine: the bridge to its Server.
pub const WINDOW: &[&str] = &["ferrited", "bridge"];

/// What the CLI's ssh runs (ADR 0027): the bridge to the Server's API,
/// which never starts a Server. A word of its own, so a `ferrited` older
/// than it refuses it rather than bridging a window.
pub const API: &[&str] = &["ferrited", "api-bridge"];

/// `ssh … <target> <remote>`, never asking anything: a connection that
/// needs the operator fails, and they sign in (`Bridge::sign_in`).
pub fn bridge_args(target: &str, control: &Path, remote: &[&str]) -> Vec<String> {
    let mut args = vec!["-T".to_string(), "-o".into(), "BatchMode=yes".into()];
    args.extend(ssh_options(control));
    args.extend(["--".into(), target.into()]);
    if remote == WINDOW || remote == API {
        args.push(setup::bridge_command(remote[1]));
    } else {
        args.extend(remote.iter().map(|word| word.to_string()));
    }
    args
}

/// `ssh … <target> true`, in a terminal: it asks what it must, and leaves
/// the master behind.
pub fn sign_in_args(target: &str, control: &Path) -> Vec<String> {
    let mut args = ssh_options(control);
    args.extend(["--".into(), target.into(), "true".into()]);
    args
}

/// The CLI's Tunnels to `machine`'s API through its saved transport.
pub fn api_dial(machine: &Machine, settings_dir: &Path) -> Dial {
    if machine.is_link() {
        return remote::dial(machine, settings_dir, "/v1/api");
    }
    let control = dir(settings_dir, &machine.target).join("c");
    ssh_dial(machine.target.clone(), control, API)
}

fn ssh_dial(target: String, control: PathBuf, remote: &'static [&'static str]) -> Dial {
    Box::new(move || {
        if let Some(dir) = control.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut child = Command::new("ssh")
            .args(bridge_args(&target, &control, remote))
            .no_console_window()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let (Some(to), Some(from), Some(mut stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(io::Error::other("ssh's stdio was not piped"));
        };
        let (say, said) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("ferrite-ssh-stderr".into())
            .spawn(move || {
                let mut words = String::new();
                let _ = stderr.read_to_string(&mut words);
                let _ = say.send(words);
            })?;
        Ok(Tunnel {
            from: Box::new(from),
            to: Box::new(to),
            ended: Box::new(move || {
                let status = child.wait().ok().and_then(|status| status.code());
                // A master ssh left running may hold stderr open: what was
                // said by the time ssh exited is all there is to read.
                let said = said.recv_timeout(STDERR_WAIT).unwrap_or_default();
                Failure::of(status, &said)
            }),
        })
    })
}

/// Whether Ferrite's master for `target` is up.
fn master_up(target: &str, control: &Path) -> bool {
    Command::new("ssh")
        .args(["-o", &format!("ControlPath={}", control.display())])
        .args(["-O", "check", "--", target])
        .no_console_window()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// A word the shell reads back as itself.
fn quoted(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

/// The script a sign-in terminal runs.
fn sign_in_script(name: &str, target: &str, control: &Path) -> String {
    let ssh: Vec<String> = sign_in_args(target, control)
        .iter()
        .map(|arg| quoted(arg))
        .collect();
    format!(
        "#!/bin/sh\nclear\necho {}\nssh {} && echo {}\n",
        quoted(&format!("Signing in to {name} for Ferrite…")),
        ssh.join(" "),
        quoted("Signed in. Ferrite reconnects now; you can close this window."),
    )
}

#[cfg(unix)]
fn open_terminal(name: &str, target: &str, control: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = control.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let script = dir.join("sign-in.command");
    std::fs::write(&script, sign_in_script(name, target, control))?;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))?;
    let mut terminal = if cfg!(target_os = "macos") {
        let mut open = Command::new("open");
        open.arg(&script);
        open
    } else {
        let mut emulator = Command::new("x-terminal-emulator");
        emulator.arg("-e").arg(&script);
        emulator
    };
    terminal
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

#[cfg(windows)]
fn open_terminal(_: &str, target: &str, _: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!("Windows' ssh cannot share a sign-in: set up a key for {target}"),
    ))
}

#[cfg(test)]
mod tests;
