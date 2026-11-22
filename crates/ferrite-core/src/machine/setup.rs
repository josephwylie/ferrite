use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::{auth, bridge_args, check_target, dir, lock, quoted, ssh_options, Machine, WINDOW};
use crate::link;
use crate::protocol::{self, ClientMessage, Peer, ServerMessage};
use crate::spawn::NoConsoleWindow;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const BOOTSTRAP: &str = include_str!("bootstrap.sh");
const WAIT: Duration = Duration::from_secs(300);
const DIAGNOSTIC_LIMIT: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Connecting,
    Inspecting,
    Installing,
    Starting,
    Verifying,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Connecting => "Connecting over SSH…",
            Self::Inspecting => "Checking the remote server…",
            Self::Installing => "Installing Ferrite Server…",
            Self::Starting => "Starting Ferrite Server…",
            Self::Verifying => "Verifying the connection…",
        }
    }
}

#[derive(Debug)]
pub enum Event {
    Progress(Phase),
    Verify(auth::Question),
    Warning(String),
    Ready(Machine),
    Failed(String),
}

pub struct Attempt {
    events: Receiver<Event>,
    questions: Receiver<auth::Question>,
    cancelled: Arc<AtomicBool>,
    broker: Option<auth::Broker>,
}

impl Attempt {
    pub fn start(machine: Machine, settings_dir: PathBuf, helper: PathBuf) -> io::Result<Self> {
        check_target(&machine.target).map_err(io::Error::other)?;
        let directory = dir(&settings_dir, &machine.target);
        let (broker, questions) = auth::Broker::new(&directory, &helper)?;
        let mut command = Command::new("ssh");
        command.args([
            "-T",
            "-o",
            "BatchMode=no",
            "-o",
            "NumberOfPasswordPrompts=1",
            "-o",
            "StrictHostKeyChecking=ask",
        ]);
        command.args(ssh_options(&directory.join("c")));
        command.args([
            "--",
            &machine.target,
            &shell_command(&BOOTSTRAP.replace("@VERSION@", VERSION)),
        ]);
        broker.configure(&mut command);
        let reconnect = cfg!(windows).then(|| {
            let mut reconnect = Command::new("ssh");
            reconnect
                .args(bridge_args(&machine.target, &directory.join("c"), WINDOW))
                .no_console_window();
            reconnect
        });
        Self::launch(machine, command, reconnect, Some(broker), questions, WAIT)
    }

    fn launch(
        machine: Machine,
        command: Command,
        reconnect: Option<Command>,
        broker: Option<auth::Broker>,
        questions: Receiver<auth::Question>,
        timeout: Duration,
    ) -> io::Result<Self> {
        let child = OwnedChild::spawn(command)?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&cancelled);
        let (send, events) = crate::doorbell::channel();
        send.send(Event::Progress(Phase::Connecting)).ok();
        std::thread::Builder::new().name("ferrite-setup".into()).spawn(move || {
            let deadline = Instant::now() + timeout;
            let result = prepare(child, &send, &stopped, deadline).and_then(|version| {
                let Some(command) = reconnect else {
                    return Ok(version);
                };
                OwnedChild::spawn(command)
                    .and_then(|child| prepare(child, &send, &stopped, deadline))
                    .map_err(|error| io::Error::other(format!(
                        "Could not verify a connection without another sign-in. On Windows, configure an SSH key or unlock it in ssh-agent, then retry.\n{error}"
                    )))
            });
            if !stopped.load(Ordering::Acquire) {
                let terminal = match result {
                    Ok(server_version) => {
                        if server_version != VERSION {
                            let _ = send.send(Event::Warning(format!(
                                "Connected to Ferrite Server {server_version}. This app is {VERSION}. Existing agents keep running; restart the remote server when they finish to update it."
                            )));
                        }
                        Event::Ready(machine)
                    }
                    Err(error) => Event::Failed(error.to_string()),
                };
                let _ = send.send(terminal);
            }
        })?;
        Ok(Self {
            events,
            questions,
            cancelled,
            broker,
        })
    }

    pub fn poll(&mut self) -> Option<Event> {
        if let Ok(question) = self.questions.try_recv() {
            return Some(Event::Verify(question));
        }
        self.events.try_recv().ok()
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.broker.take();
    }
}

struct OwnedChild(Child);

impl OwnedChild {
    fn spawn(mut command: Command) -> io::Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        command.spawn().map(Self)
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            // The child starts its own process group, including an active askpass helper.
            unsafe {
                libc::kill(-(self.0.id() as libc::pid_t), libc::SIGTERM);
            }
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn prepare(
    mut child: OwnedChild,
    events: &crate::doorbell::Sender<Event>,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> io::Result<String> {
    let mut input = child
        .0
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("SSH has no input pipe"))?;
    let output = child
        .0
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("SSH has no output pipe"))?;
    let stderr = child
        .0
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("SSH has no diagnostic pipe"))?;
    let diagnostics = Arc::new(Mutex::new(String::new()));
    let words = Arc::clone(&diagnostics);
    let progress = events.clone();
    let (drained, stderr_done) = mpsc::channel();
    std::thread::Builder::new()
        .name("ferrite-setup-errors".into())
        .spawn(move || {
            read_diagnostics(stderr, &progress, &words);
            let _ = drained.send(());
        })?;
    let (reply, replies) = mpsc::channel();
    std::thread::Builder::new()
        .name("ferrite-setup-welcome".into())
        .spawn(move || {
            let _ = reply.send(verify(output));
        })?;
    link::send(
        &mut input,
        &ClientMessage::Hello {
            generation: protocol::GENERATION,
            client: Peer {
                name: "Ferrite connection setup".into(),
                version: VERSION.into(),
            },
            capabilities: Vec::new(),
        },
    )?;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Connection cancelled",
            ));
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Connection setup timed out. Check the host and network, then retry.",
            ));
        }
        match replies.recv_timeout(Duration::from_millis(25)) {
            Ok(Ok(version)) => return Ok(version),
            Ok(Err(error)) => {
                // A persistent SSH master may keep stderr open after the session closes.
                let _ = stderr_done.recv_timeout(Duration::from_secs(1));
                let said = lock(&diagnostics).trim().to_string();
                return Err(io::Error::other(if said.is_empty() {
                    error.to_string()
                } else {
                    format!("{error}\n{said}")
                }));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(io::Error::other(
                    "The connection verification stopped unexpectedly.",
                ))
            }
        }
    }
}

fn verify(output: impl Read) -> io::Result<String> {
    let mut lines = link::Lines::new(output);
    let version = match lines.next_message::<ServerMessage>()? {
        Some(ServerMessage::Welcome {
            generation,
            server,
            error,
            ..
        }) => {
            if let Some(error) = error {
                return Err(io::Error::other(format!("The remote server cannot connect to this app: {}. Update Ferrite on the remote machine after its agents finish.", error.message)));
            }
            if generation != protocol::GENERATION {
                return Err(io::Error::other("The remote server uses an incompatible protocol. Update Ferrite on the remote machine after its agents finish."));
            }
            server.version
        }
        _ => {
            return Err(io::Error::other(
                "The remote command did not send a Ferrite Server welcome.",
            ))
        }
    };
    match lines.next_message::<ServerMessage>()? {
        Some(ServerMessage::Snapshot(_)) => Ok(version),
        _ => Err(io::Error::other(
            "The remote server did not send its initial state.",
        )),
    }
}

fn read_diagnostics(mut stderr: impl Read, events: &crate::doorbell::Sender<Event>, diagnostics: &Mutex<String>) {
    let mut buffer = [0; 1024];
    let mut line = Vec::new();
    while let Ok(count) = stderr.read(&mut buffer) {
        if count == 0 {
            break;
        }
        for &byte in &buffer[..count] {
            if byte == b'\n' {
                diagnostic(&String::from_utf8_lossy(&line), events, diagnostics);
                line.clear();
            } else if line.len() < DIAGNOSTIC_LIMIT {
                line.push(byte);
            }
        }
    }
    if !line.is_empty() {
        diagnostic(&String::from_utf8_lossy(&line), events, diagnostics);
    }
}

fn diagnostic(line: &str, events: &crate::doorbell::Sender<Event>, diagnostics: &Mutex<String>) {
    let phase = match line.trim() {
        "FERRITE_SETUP inspecting" => Some(Phase::Inspecting),
        "FERRITE_SETUP installing" => Some(Phase::Installing),
        "FERRITE_SETUP starting" => Some(Phase::Starting),
        "FERRITE_SETUP verifying" => Some(Phase::Verifying),
        _ => None,
    };
    if let Some(phase) = phase {
        let _ = events.send(Event::Progress(phase));
    } else if let Some(warning) = line.strip_prefix("FERRITE_WARNING ") {
        let _ = events.send(Event::Warning(warning.to_string()));
    } else {
        let mut said = lock(diagnostics);
        said.push_str(line);
        said.push('\n');
        if said.len() > DIAGNOSTIC_LIMIT {
            let mut start = said.len() - DIAGNOSTIC_LIMIT;
            while !said.is_char_boundary(start) {
                start += 1;
            }
            said.drain(..start);
        }
    }
}

fn shell_command(script: &str) -> String {
    format!("sh -c {}", quoted(script))
}

pub(super) fn bridge_command(mode: &str) -> String {
    shell_command(&format!(
        "binary=\"$HOME/.ferrite/runtime/versions/{VERSION}/ferrited\"; if [ -x \"$binary\" ] && [ \"$(\"$binary\" --version 2>/dev/null)\" = \"ferrited {VERSION}\" ]; then exec \"$binary\" {mode}; elif command -v ferrited >/dev/null 2>&1; then exec ferrited {mode}; else printf '%s\\n' 'Ferrite Server is not installed. Open Settings > Connections and retry setup.' >&2; exit 127; fi"
    ))
}

#[cfg(test)]
mod tests;
