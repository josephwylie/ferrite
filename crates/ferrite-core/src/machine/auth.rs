use std::fmt;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::link::{self, TryClone};

const HELPER_ENV: &str = "FERRITE_SSH_ASKPASS";
const WAIT: Duration = Duration::from_secs(120);
const MAX_PROMPT: usize = 16 * 1024;
const MAX_RESPONSE: usize = 1023;
const MAX_FRAME: usize = 128 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptKind {
    ConfirmHost,
    Secret,
}

/// Dropping an unanswered question cancels it. Responses are never logged or saved.
pub struct Question {
    pub prompt: String,
    pub kind: PromptKind,
    pub expires_at: Instant,
    answer: Option<Sender<Option<String>>>,
}

impl fmt::Debug for Question {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Question")
            .field("kind", &self.kind)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

impl Question {
    /// A host confirmation accepts only `yes` or `no`; `None` cancels any prompt.
    pub fn respond(mut self, response: Option<String>) -> io::Result<()> {
        if Instant::now() >= self.expires_at {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "SSH prompt expired",
            ));
        }
        validate_response(self.kind, response.as_deref())?;
        self.answer
            .take()
            .expect("a question is answered once")
            .send(response)
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "SSH prompt ended"))
    }
}

impl Drop for Question {
    fn drop(&mut self) {
        if let Some(answer) = self.answer.take() {
            let _ = answer.send(None);
        }
    }
}

pub struct Broker {
    address: PathBuf,
    executable: PathBuf,
    state: Arc<Mutex<State>>,
    worker: Option<JoinHandle<()>>,
    temporary: Option<Temporary>,
}

#[derive(Default)]
struct State {
    stopped: bool,
    active: Option<Active>,
}

struct Active {
    stream: link::Stream,
    answer: Sender<Option<String>>,
}

impl Broker {
    /// The directory belongs to this connection. Only a socket is created in it.
    pub fn new(directory: &Path, executable: &Path) -> io::Result<(Self, Receiver<Question>)> {
        Self::with_timeout(directory, executable, WAIT)
    }

    fn with_timeout(
        directory: &Path,
        executable: &Path,
        timeout: Duration,
    ) -> io::Result<(Self, Receiver<Question>)> {
        let address = directory.join(format!(
            "ask-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let (address, temporary) = socket_address(address)?;
        let listener = link::listen(&address)?;
        let state = Arc::new(Mutex::new(State::default()));
        let shared = state.clone();
        let (questions, receiver) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("ferrite-askpass".into())
            .spawn(move || {
                while !shared.lock().unwrap_or_else(|e| e.into_inner()).stopped {
                    let Ok(stream) = listener.accept() else {
                        break;
                    };
                    let _ = ask(stream, &questions, &shared, timeout);
                    shared.lock().unwrap_or_else(|e| e.into_inner()).active = None;
                }
            })?;
        Ok((
            Self {
                address,
                executable: executable.to_owned(),
                state,
                worker: Some(worker),
                temporary,
            },
            receiver,
        ))
    }

    /// Apply only to the SSH process owned by the connection attempt.
    pub fn configure(&self, command: &mut Command) {
        command
            .env("SSH_ASKPASS", &self.executable)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env_remove("SSH_ASKPASS_PROMPT")
            .env(HELPER_ENV, &self.address);
    }
}

impl Drop for Broker {
    fn drop(&mut self) {
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.stopped = true;
            if let Some(active) = state.active.take() {
                let _ = active.answer.send(None);
                let _ = active.stream.shutdown();
            }
        }
        // A Windows accept retries a client that closes too early. Keep the
        // wake connection alive until the listener has observed cancellation.
        let wake = link::connect(&self.address).ok();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        drop(wake);
        drop(self.temporary.take());
    }
}

struct Temporary(PathBuf);

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn socket_address(address: PathBuf) -> io::Result<(PathBuf, Option<Temporary>)> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::DirBuilderExt;

        // macOS allows only 104 bytes, including the terminator. Custom
        // settings paths can exceed that before the socket name is added.
        if address.as_os_str().as_bytes().len() >= 104 {
            loop {
                let path = PathBuf::from("/tmp").join(format!(
                    "ferrite-ask-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match std::fs::DirBuilder::new().mode(0o700).create(&path) {
                    Ok(()) => return Ok((path.join("s"), Some(Temporary(path)))),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error),
                }
            }
        }
    }
    Ok((address, None))
}

#[derive(Serialize, Deserialize)]
struct Request {
    prompt: String,
    kind: PromptKind,
}

fn ask(
    mut stream: link::Stream,
    questions: &SyncSender<Question>,
    state: &Mutex<State>,
    timeout: Duration,
) -> io::Result<()> {
    let expires_at = Instant::now() + timeout;
    let (answer, response) = mpsc::channel();
    {
        let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopped {
            return Ok(());
        }
        state.active = Some(Active {
            stream: stream.try_clone()?,
            answer: answer.clone(),
        });
    }
    let _deadline = Deadline::new(&stream, expires_at, Some(answer.clone()))?;
    let request: Request = receive(&mut stream)?;
    validate_prompt(&request.prompt)?;
    questions
        .try_send(Question {
            prompt: request.prompt,
            kind: request.kind,
            expires_at,
            answer: Some(answer),
        })
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "SSH prompt unavailable"))?;
    let response = response
        .recv_timeout(expires_at.saturating_duration_since(Instant::now()))
        .unwrap_or(None);
    validate_response(request.kind, response.as_deref())?;
    send(&mut stream, &response)
}

/// Called before any CLI, logging, or window initialization in the desktop executable.
pub fn dispatch() -> Option<i32> {
    let address = std::env::var_os(HELPER_ENV)?;
    let result = (|| {
        let prompt = std::env::args_os()
            .nth(1)
            .and_then(|value| value.into_string().ok())
            .ok_or_else(invalid)?;
        let confirm = std::env::var("SSH_ASKPASS_PROMPT").as_deref() == Ok("confirm");
        let kind = if confirm || prompt.contains("(yes/no") {
            PromptKind::ConfirmHost
        } else {
            PromptKind::Secret
        };
        let response = request(&PathBuf::from(address), Request { prompt, kind })?;
        let Some(response) = response else {
            return Ok(false);
        };
        let mut stdout = io::stdout().lock();
        stdout.write_all(response.as_bytes())?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        Ok::<_, io::Error>(true)
    })();
    Some(if matches!(result, Ok(true)) { 0 } else { 1 })
}

fn request(address: &Path, request: Request) -> io::Result<Option<String>> {
    let expires_at = Instant::now() + WAIT;
    validate_prompt(&request.prompt)?;
    let mut stream = link::connect(address)?;
    let _deadline = Deadline::new(&stream, expires_at, None)?;
    send(&mut stream, &request)?;
    let response: Option<String> = receive(&mut stream)?;
    validate_response(request.kind, response.as_deref())?;
    Ok(response)
}

struct Deadline {
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl Deadline {
    fn new(
        stream: &link::Stream,
        expires_at: Instant,
        answer: Option<Sender<Option<String>>>,
    ) -> io::Result<Self> {
        let stream = stream.try_clone()?;
        let (stop, receiver) = mpsc::channel();
        let worker = std::thread::Builder::new()
            .name("ferrite-askpass-deadline".into())
            .spawn(move || {
                if matches!(
                    receiver.recv_timeout(expires_at.saturating_duration_since(Instant::now())),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    if let Some(answer) = answer {
                        let _ = answer.send(None);
                    }
                    let _ = stream.shutdown();
                }
            })?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for Deadline {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn send(writer: &mut impl Write, message: &impl Serialize) -> io::Result<()> {
    let bytes = serde_json::to_vec(message).map_err(|_| invalid())?;
    if bytes.len() > MAX_FRAME {
        return Err(invalid());
    }
    writer.write_all(&(bytes.len() as u32).to_be_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}

fn receive<T: DeserializeOwned>(reader: &mut impl Read) -> io::Result<T> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_FRAME {
        return Err(invalid());
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(|_| invalid())
}

fn validate_prompt(prompt: &str) -> io::Result<()> {
    if prompt.is_empty() || prompt.len() > MAX_PROMPT || prompt.contains('\0') {
        return Err(invalid());
    }
    Ok(())
}

fn validate_response(kind: PromptKind, response: Option<&str>) -> io::Result<()> {
    if let Some(response) = response {
        if response.len() > MAX_RESPONSE
            || response.contains(['\0', '\n', '\r'])
            || kind == PromptKind::ConfirmHost && !matches!(response, "yes" | "no")
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid SSH verification message",
    )
}

#[cfg(test)]
mod tests;
