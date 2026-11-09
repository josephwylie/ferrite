//! One owned, authenticated native process. The worker serializes whole
//! select/observe/action transactions; no command can choose a caller's target.
//! Screenshots and annotation evidence share that reservation but read the
//! exact CEF target directly, so capture never needs to unpause input or run
//! the native controller's script-based bootstrap probes.
use super::super::web::ControllerEndpoint;
use base64::Engine as _;
use ferrite_core::browser::{Control, Document, Outcome};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

const LIMIT: u64 = 24 * 1024 * 1024;
const TEXT: usize = 128 * 1024;

pub enum Event {
    Ready(mpsc::SyncSender<bool>),
    Finished(Outcome),
}

pub struct Worker {
    send: mpsc::Sender<Job>,
    busy: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}
struct Job {
    endpoint: ControllerEndpoint,
    target: String,
    document: Document,
    revision: u64,
    epoch: u64,
    control: Control,
    point: Option<ferrite_core::browser_annotations::Point>,
    reply: mpsc::SyncSender<Event>,
}
thread_local! {
    static SHARED: std::cell::RefCell<std::sync::Weak<Worker>> = const { std::cell::RefCell::new(std::sync::Weak::new()) };
}
impl Worker {
    pub fn shared() -> Arc<Self> {
        SHARED.with(|slot| {
            if let Some(worker) = slot.borrow().upgrade() {
                return worker;
            }
            let (send, receive) = mpsc::channel::<Job>();
            let busy = Arc::new(AtomicBool::new(false));
            let running = busy.clone();
            let stop = Arc::new(AtomicBool::new(false));
            let stopping = stop.clone();
            let child = Arc::new(Mutex::new(None));
            let owned_child = child.clone();
            let thread = std::thread::Builder::new()
                .name("conversation-browser".into())
                .spawn(move || {
                    let mut controller: Option<Controller> = None;
                    while !stopping.load(Ordering::Acquire) {
                        let job = match receive.recv_timeout(Duration::from_millis(50)) {
                            Ok(job) => job,
                            Err(mpsc::RecvTimeoutError::Timeout) => continue,
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        };
                        let restart = controller.as_ref().is_some_and(|controller| {
                            controller.endpoint != job.endpoint.url
                                || controller
                                    .epochs
                                    .get(&job.target)
                                    .map_or(job.epoch != 0, |epoch| *epoch != job.epoch)
                        });
                        if restart {
                            controller.take();
                        }
                        let deadline = Instant::now() + Duration::from_secs(30);
                        let result = (|| {
                            if !job.control.supported() {
                                return Err(failure("Unsupported browser control or wait state"));
                            }
                            if matches!(job.control, Control::Screenshot) {
                                let mut page = ReadOnlyPage::open(
                                    &job.endpoint,
                                    &job.target,
                                    deadline,
                                    stopping.clone(),
                                )
                                .map_err(failure)?;
                                authorize(&job, &stopping)?;
                                return page.capture(job.point.as_ref()).map_err(failure);
                            }
                            if controller.is_none() {
                                controller = Some(Controller::start(
                                    &job.endpoint,
                                    deadline,
                                    owned_child.clone(),
                                    stopping.clone(),
                                )?);
                            }
                            let controller = controller.as_mut().unwrap();
                            controller.deadline = deadline;
                            let switched = controller.select(&job.target)?;
                            authorize(&job, &stopping)?;
                            // No launch, reconnect, or target selection after
                            // the GUI authorizes this established attachment.
                            controller.perform_selected(&job, switched)
                        })()
                        .map_err(|mut error: Error| {
                            error.message = error
                                .message
                                .replace(&job.endpoint.authorization, "[private]")
                                .replace(&job.endpoint.url, "[private]");
                            error
                        });
                        #[cfg(feature = "browser-smoke")]
                        if result.is_ok() && !job.control.observation() {
                            super::completion_gate::after_write(&stopping);
                        }
                        if result.is_ok() {
                            if let Some(controller) = controller.as_mut() {
                                controller.epochs.insert(job.target.clone(), job.epoch);
                            }
                        }
                        let outcome = match result {
                            Ok(value) => Outcome::Success { value },
                            Err(error) if error.uncertain && !job.control.observation() => {
                                Outcome::Uncertain {
                                    error: error.message,
                                }
                            }
                            Err(error) => Outcome::Failed {
                                error: error.message,
                            },
                        };
                        // A timed-out command may still execute. Reap this owned
                        // process before admitting another job, including Observe.
                        if controller.as_ref().is_some_and(|c| c.broken) {
                            controller.take();
                        }
                        let _ = job.reply.try_send(Event::Finished(outcome));
                        running.store(false, Ordering::Release);
                    }
                })
                .expect("start conversation browser worker");
            let worker = Arc::new(Self {
                send,
                busy,
                stop,
                child,
                thread: Mutex::new(Some(thread)),
            });
            *slot.borrow_mut() = Arc::downgrade(&worker);
            worker
        })
    }
    /// Run on the application's background executor, and await it during quit.
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Release);
        if let Some(mut child) = self.child.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(thread) = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = thread.join();
        }
    }

    /// Call only after the owning GUI has revalidated document and intervention
    /// state. Busy jobs remain in that window's queue, without native dispatch.
    pub fn try_start(
        &self,
        endpoint: ControllerEndpoint,
        target: String,
        document: Document,
        revision: u64,
        epoch: u64,
        control: Control,
        point: Option<ferrite_core::browser_annotations::Point>,
    ) -> Option<mpsc::Receiver<Event>> {
        if self.stop.load(Ordering::Acquire) {
            return None;
        }
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return None;
        }
        let (reply, answer) = mpsc::sync_channel(1);
        if self
            .send
            .send(Job {
                endpoint,
                target,
                document,
                revision,
                epoch,
                control,
                point,
                reply,
            })
            .is_err()
        {
            self.busy.store(false, Ordering::Release);
        }
        Some(answer)
    }
}
fn authorize(job: &Job, stopping: &AtomicBool) -> Result<(), Error> {
    let (authorize, permission) = mpsc::sync_channel(1);
    job.reply
        .send(Event::Ready(authorize))
        .map_err(|_| failure("owning window disappeared before dispatch"))?;
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if stopping.load(Ordering::Acquire) || Instant::now() >= until {
            return Err(failure("Browser owner shut down before dispatch"));
        }
        match permission.recv_timeout(Duration::from_millis(50)) {
            Ok(true) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            _ => {
                return Err(failure(
                    "The owning page changed before dispatch; observe again",
                ))
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let child = self.child.clone();
        let thread = self
            .thread
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        // Last-window disposal stays responsive. Normal app quit explicitly
        // awaits shutdown before the process is allowed to exit.
        let _ = std::thread::Builder::new()
            .name("browser-reaper".into())
            .spawn(move || {
                if let Some(mut child) = child.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                if let Some(thread) = thread {
                    let _ = thread.join();
                }
            });
    }
}

struct Error {
    message: String,
    uncertain: bool,
}
impl From<String> for Error {
    fn from(message: String) -> Self {
        Self {
            message,
            uncertain: false,
        }
    }
}
fn failure(error: impl std::fmt::Display) -> Error {
    Error {
        message: format!("Browser controller: {error}"),
        uncertain: false,
    }
}
fn secret() -> Result<String, Error> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(failure)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
struct Binding {
    native: String,
    target: String,
    document: Document,
    revision: u64,
    epoch: u64,
}
struct Controller {
    child: Arc<Mutex<Option<Child>>>,
    directory: PathBuf,
    token: String,
    endpoint: String,
    authorization: String,
    stopping: Arc<AtomicBool>,
    selected: Option<String>,
    refs: HashMap<String, Binding>,
    observed: HashMap<String, (Document, u64, u64)>,
    epochs: HashMap<String, u64>,
    broken: bool,
    deadline: Instant,
}
impl Drop for Controller {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
impl Controller {
    fn start(
        endpoint: &ControllerEndpoint,
        deadline: Instant,
        child: Arc<Mutex<Option<Child>>>,
        stop: Arc<AtomicBool>,
    ) -> Result<Self, Error> {
        let executable = std::env::current_exe()
            .map_err(failure)?
            .parent()
            .ok_or_else(|| failure("missing packaged application directory"))?
            .join(if cfg!(windows) {
                "agent-browser.exe"
            } else {
                "agent-browser"
            });
        if !executable.is_file() {
            return Err(failure(
                "The packaged browser controller is missing; reinstall Ferrite",
            ));
        }
        let token = secret()?;
        let directory = std::env::temp_dir().join(format!("ferrite-control-{}", &token[..24]));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory).map_err(failure)?;
        let mut command = Command::new(executable);
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy();
            if name.starts_with("AGENT_BROWSER_") || name.starts_with("FERRITE_AGENT_BROWSER") {
                command.env_remove(key);
            }
        }
        command
            .env("AGENT_BROWSER_DAEMON", "1")
            .env("AGENT_BROWSER_SESSION", "ferrite")
            .env("AGENT_BROWSER_SOCKET_DIR", &directory)
            .env("AGENT_BROWSER_IDLE_TIMEOUT_MS", "0")
            .env("FERRITE_AGENT_BROWSER", "1")
            .env("FERRITE_AGENT_BROWSER_TOKEN", &token)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        {
            let mut owned = child.lock().unwrap_or_else(|e| e.into_inner());
            if stop.load(Ordering::Acquire) {
                let _ = std::fs::remove_dir_all(&directory);
                return Err(failure("browser owner is shutting down"));
            }
            match command.spawn() {
                Ok(process) => *owned = Some(process),
                Err(error) => {
                    let _ = std::fs::remove_dir_all(&directory);
                    return Err(failure(error));
                }
            }
        }
        let mut controller = Self {
            child,
            directory,
            token,
            endpoint: endpoint.url.clone(),
            authorization: endpoint.authorization.clone(),
            stopping: stop.clone(),
            selected: None,
            refs: HashMap::new(),
            observed: HashMap::new(),
            epochs: HashMap::new(),
            broken: false,
            deadline,
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if stop.load(Ordering::Acquire) {
                return Err(failure("browser owner is shutting down"));
            }
            let exited = controller
                .child
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_mut()
                .map(|child| child.try_wait())
                .transpose()
                .map_err(failure)?
                .flatten()
                .is_some();
            if exited {
                return Err(failure("exited before becoming ready"));
            }
            if controller.connect().is_ok() {
                break;
            }
            if Instant::now() >= deadline {
                return Err(failure("startup timed out"));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        controller.command(json!({"action":"launch", "cdpUrl":endpoint.url, "cdpHeaders":{"Authorization":endpoint.authorization}}))?;
        Ok(controller)
    }
    #[cfg(unix)]
    fn connect(&self) -> std::io::Result<std::os::unix::net::UnixStream> {
        let stream = std::os::unix::net::UnixStream::connect(self.directory.join("ferrite.sock"))?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        Ok(stream)
    }
    #[cfg(windows)]
    fn connect(&self) -> std::io::Result<std::net::TcpStream> {
        let port: u16 = std::fs::read_to_string(self.directory.join("ferrite.port"))?
            .trim()
            .parse()
            .map_err(std::io::Error::other)?;
        let stream = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_secs(1),
        )?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        Ok(stream)
    }
    fn command(&mut self, mut value: Value) -> Result<Value, Error> {
        if self.stopping.load(Ordering::Acquire) {
            return Err(failure("browser owner is shutting down"));
        }
        if Instant::now() >= self.deadline {
            return Err(failure(
                "operation deadline expired before dispatch; observe again",
            ));
        }
        let writes = matches!(
            value["action"].as_str(),
            Some(
                "click"
                    | "type"
                    | "fill"
                    | "select"
                    | "check"
                    | "uncheck"
                    | "scroll"
                    | "scrollintoview"
            )
        );
        value["ferriteToken"] = json!(self.token);
        let mut connection = self.connect().map_err(|error| {
            self.broken = true;
            failure(error)
        })?;
        let bytes = serde_json::to_vec(&value).map_err(failure)?;
        let result = (|| -> Result<Value, String> {
            connection.write_all(&bytes).map_err(|e| e.to_string())?;
            connection.write_all(b"\n").map_err(|e| e.to_string())?;
            let mut line = String::new();
            BufReader::new(connection.take(LIMIT + 1))
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            if line.len() as u64 > LIMIT || !line.ends_with('\n') {
                return Err("response exceeded its bound or disconnected".into());
            }
            serde_json::from_str(&line).map_err(|e| e.to_string())
        })();
        let response = result.map_err(|message| {
            self.broken = true;
            Error {
                message: format!(
                    "Browser controller transport failed; completion may be uncertain: {message}"
                ),
                uncertain: writes,
            }
        })?;
        if response["success"] != true {
            let message = response["error"]
                .as_str()
                .unwrap_or("native operation failed");
            let mut message: String = message.chars().take(4096).collect();
            // Never return credentials even if an upstream diagnostic includes
            // transport configuration.
            message = message
                .replace(&self.token, "[private]")
                .replace(&self.authorization, "[private]")
                .replace(&self.endpoint, "[private]")
                .replace(self.directory.to_string_lossy().as_ref(), "[private]");
            let selector = value["selector"].as_str().unwrap_or("");
            let known_before_dispatch = message
                == format!(
                    "Element '{}' is outside the viewport, so the click would not reach it",
                    selector
                )
                || message == format!("Element ref {} is stale", selector.trim_start_matches('@'));
            return Err(Error {
                message,
                uncertain: writes && !known_before_dispatch,
            });
        }
        let mut data = response["data"].clone();
        if let Some(data) = data.as_object_mut() {
            // Upstream's generic launch claims do not describe the embedded
            // CEF lifetime and may include private attachment configuration.
            data.remove("lifecycle");
        }
        Ok(data)
    }
    fn select(&mut self, target: &str) -> Result<bool, Error> {
        let listed = self.command(json!({"action":"tab_list"}))?;
        let alias = listed["tabs"]
            .as_array()
            .and_then(|tabs| tabs.iter().find(|tab| tab["targetId"] == target))
            .and_then(|tab| tab["tabId"].as_str())
            .ok_or_else(|| failure("The owned live page is not attached; observe it again"))?
            .to_string();
        let active = listed["tabs"].as_array().is_some_and(|tabs| {
            tabs.iter()
                .any(|tab| tab["targetId"] == target && tab["active"] == true)
        });
        if active && self.selected.as_deref() == Some(target) {
            return Ok(false);
        }
        // A native switch can commit before a later setup failure. Quarantine
        // that process instead of trusting our old cached selection.
        // Upstream startup pin mode intentionally creates a fresh blank tab.
        // Attach read-only first; pin and explicitly bind our trusted alias in
        // the same serialized command before any provider action is admitted.
        let switched = self.command(json!({"action":"tab_switch", "tabId":alias, "pinTab":true}));
        if switched
            .as_ref()
            .is_ok_and(|value| value["targetId"] != target)
        {
            self.selected = None;
            self.broken = true;
            return Err(failure(
                "Native selection did not acknowledge the exact owned target",
            ));
        }
        if let Err(error) = switched {
            self.selected = None;
            self.broken = true;
            return Err(error);
        }
        self.selected = Some(target.to_owned());
        Ok(true)
    }
    fn perform_selected(&mut self, job: &Job, switched: bool) -> Result<Value, Error> {
        if !job.control.observation()
            && self.observed.get(&job.target) != Some(&(job.document, job.revision, job.epoch))
        {
            return Err(failure("The controller attachment changed; obtain a fresh readable observation before input"));
        }
        let reference = match &job.control {
            Control::Click { reference }
            | Control::Type { reference, .. }
            | Control::Fill { reference, .. }
            | Control::Select { reference, .. }
            | Control::Check { reference, .. } => Some(reference),
            _ => None,
        };
        let selector = if let Some(reference) = reference {
            let binding = self
                .refs
                .get(reference.trim_start_matches('@'))
                .ok_or_else(|| failure("Stale element reference; observe the page again"))?;
            if binding.target != job.target
                || binding.document != job.document
                || binding.revision != job.revision
                || binding.epoch != job.epoch
            {
                return Err(failure(
                    "Stale or unrelated element reference; observe the page again",
                ));
            }
            let native = binding.native.clone();
            if switched {
                let snapshot = self.command(json!({"action":"snapshot","interactive":true}))?;
                if snapshot["refs"].get(&native).is_none() {
                    return Err(failure(
                        "The exact element is no longer present; observe the page again",
                    ));
                }
            }
            Some(format!("@{native}"))
        } else {
            None
        };
        let mut value = match &job.control {
            Control::Unknown => Err(failure("Unsupported browser control")),
            Control::Observe => {
                let mut value = self.command(json!({"action":"snapshot", "interactive":true}))?;
                self.refs.retain(|_, binding| binding.target != job.target);
                // Closed pages cannot leave an unbounded reference history.
                if self.refs.len() > 16384 {
                    self.refs.clear();
                }
                let tree = value["snapshot"].as_str().unwrap_or("");
                let native_refs = value["refs"].as_object().cloned().unwrap_or_default();
                let nonce = secret()?;
                let mut refs = serde_json::Map::new();
                let mut native_to_opaque = HashMap::new();
                let mut ref_bytes = 0usize;
                let mut refs_truncated = false;
                for (index, (native, metadata)) in native_refs.into_iter().enumerate() {
                    let bounded_field = |field: &str, limit: usize| {
                        metadata[field]
                            .as_str()
                            .map(|text| text.chars().take(limit).collect::<String>())
                    };
                    let metadata =
                        json!({"role":bounded_field("role",128),"name":bounded_field("name",1024)});
                    let bytes = metadata.to_string().len() + 64;
                    if index >= 4096 || ref_bytes + bytes > 64 * 1024 {
                        refs_truncated = true;
                        continue;
                    }
                    ref_bytes += bytes;
                    let opaque = format!("f{}_{index}", &nonce[..24]);
                    native_to_opaque.insert(native.clone(), opaque.clone());
                    self.refs.insert(
                        opaque.clone(),
                        Binding {
                            native,
                            target: job.target.clone(),
                            document: job.document,
                            revision: job.revision,
                            epoch: job.epoch,
                        },
                    );
                    refs.insert(opaque, metadata);
                }
                let (tree, tree_truncated) = rewrite_references(tree, &native_to_opaque);
                let truncated = tree_truncated || refs_truncated;
                value = json!({"snapshot":tree,"refs":refs,"truncated":truncated});
                self.observed
                    .insert(job.target.clone(), (job.document, job.revision, job.epoch));
                Ok(value)
            }
            Control::Screenshot => Err(failure(
                "Screenshot must use the reserved readonly capture transaction",
            )),
            Control::Console => self.command(json!({"action":"console"})).map(bounded),
            Control::Network => {
                let value = self.command(json!({"action":"requests"}))?;
                // Raw headers, bodies and credentials never cross this seam.
                let requests: Vec<Value> = value["requests"].as_array().into_iter().flatten().take(100).map(|item| json!({"url":item["url"].as_str().and_then(|s| url::Url::parse(s).ok()).map(|mut url| {url.set_query(None);url.set_fragment(None);let _=url.set_username("");let _=url.set_password(None);url.to_string()}),"method":item["method"],"status":item["status"]})).collect();
                Ok(bounded(json!({"requests":requests})))
            }
            Control::Wait { selector, state } => self.command(
                json!({"action":"wait","selector":selector,"state":state,"timeout":10000}),
            ),
            Control::Click { .. } => {
                let command = json!({"action":"click","selector":selector});
                match self.command(command.clone()) {
                    Err(error) if error.message == format!("Element '{}' is outside the viewport, so the click would not reach it", selector.as_deref().unwrap_or("")) => {
                        let recovery = (|| {
                            self.command(json!({"action":"scrollintoview","selector":selector}))?;
                            let snapshot = self.command(json!({"action":"snapshot","interactive":true}))?;
                            let native = selector.as_ref().unwrap().trim_start_matches('@');
                            if snapshot["refs"].get(native).is_none() {
                                return Err(failure("The original click target disappeared during viewport recovery"));
                            }
                            self.command(command)
                        })();
                        match recovery {
                            Ok(mut value) => {
                                value["initial_failures"] = json!(1);
                                value["recovery_attempts"] = json!(1);
                                Ok(value)
                            }
                            Err(mut error) => {
                                error.message.push_str(" [initial_failures=1 recovery_attempts=1]");
                                Err(error)
                            }
                        }
                    }
                    result => result,
                }
            }
            Control::Type { text, .. } => {
                self.command(json!({"action":"type","selector":selector,"text":text}))
            }
            Control::Fill { value, .. } => {
                self.command(json!({"action":"fill","selector":selector,"value":value}))
            }
            Control::Select { values, .. } => {
                self.command(json!({"action":"select","selector":selector,"values":values}))
            }
            Control::Check { checked, .. } => self.command(
                json!({"action":if *checked {"check"} else {"uncheck"},"selector":selector}),
            ),
            Control::Scroll { x, y } => self.command(json!({"action":"scroll","x":x,"y":y})),
        }?;
        if !job.control.observation() {
            value["acknowledged"] = json!(true);
        }
        if let (Some(reference), Some(selector)) = (reference, selector.as_ref()) {
            for field in [
                "clicked",
                "filled",
                "typed",
                "selected",
                "checked",
                "unchecked",
            ] {
                if value[field].as_str() == Some(selector.as_str()) {
                    value[field] = json!(reference.trim_start_matches('@'));
                }
            }
        }
        Ok(value)
    }
}
// References share a bracket with flags (`[required, ref=e12]`) and are
// not necessarily the only attribute. Rewrite whole ref tokens outside quoted
// page text; preserve required/selected/checked metadata verbatim.
fn rewrite_references(tree: &str, refs: &HashMap<String, String>) -> (String, bool) {
    let mut output = String::new();
    let (mut index, mut bytes) = (0usize, 0usize);
    let (mut quoted, mut escaped, mut attributes, mut truncated) = (false, false, false, false);
    while index < tree.len() {
        let remaining = &tree[index..];
        let character = remaining.chars().next().unwrap();
        if !quoted && attributes && remaining.starts_with("ref=") {
            let suffix = &remaining[4..];
            let length = suffix.bytes().take_while(u8::is_ascii_alphanumeric).count();
            let native = &suffix[..length];
            let boundary = suffix[length..].chars().next().is_none_or(|character| {
                character == ']' || character == ',' || character.is_whitespace()
            });
            let preceding = index == 0
                || tree.as_bytes()[index - 1] == b'['
                || tree.as_bytes()[index - 1] == b','
                || tree.as_bytes()[index - 1].is_ascii_whitespace();
            if length > 0 && boundary && preceding {
                let reference = match refs.get(native) {
                    Some(reference) => reference.as_str(),
                    None => {
                        truncated = true;
                        "unavailable"
                    }
                };
                let token = format!("ref={reference}");
                if bytes + token.len() > TEXT {
                    return (output, true);
                }
                bytes += token.len();
                output.push_str(&token);
                index += 4 + length;
                continue;
            }
        }
        let encoded = match character {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
            character if character <= '\u{1f}' => 6,
            character => character.len_utf8(),
        };
        if bytes + encoded > TEXT {
            return (output, true);
        }
        output.push(character);
        bytes += encoded;
        index += character.len_utf8();
        if quoted {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            }
        } else {
            match character {
                '"' => quoted = true,
                '[' => attributes = true,
                ']' => attributes = false,
                _ => {}
            }
        }
    }
    (output, truncated)
}

fn bounded(value: Value) -> Value {
    if value.to_string().len() <= TEXT {
        value
    } else {
        json!({"truncated":true,"message":"Diagnostics exceeded the response limit"})
    }
}

/// Decode only the bounded PNG produced by the owned screenshot command.
pub fn screenshot_bytes(value: &Value) -> Result<Vec<u8>, String> {
    let encoded = value["png_base64"]
        .as_str()
        .ok_or("Browser screenshot did not contain PNG data")?;
    if encoded.len() > 16 * 1024 * 1024 * 4 / 3 + 4 {
        return Err("Browser screenshot exceeded its bound".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "Browser screenshot contained invalid image data")?;
    if bytes.len() > 16 * 1024 * 1024 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err("Browser screenshot was not a bounded PNG".into());
    }
    Ok(bytes)
}

// Read-only capture context at the existing private CEF seam. No page script,
// raw CDP method, target identity or authentication is accepted from callers.
struct ReadOnlyPage {
    socket: tungstenite::WebSocket<std::net::TcpStream>,
    session: String,
    frame: String,
    loader: String,
    url: String,
    next: u64,
    deadline: Instant,
    stopping: Arc<AtomicBool>,
}
impl ReadOnlyPage {
    fn open(
        endpoint: &ControllerEndpoint,
        target: &str,
        deadline: Instant,
        stopping: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        use tungstenite::client::IntoClientRequest;
        let url = url::Url::parse(&endpoint.url).map_err(|e| e.to_string())?;
        let port = url.port().ok_or("Private browser endpoint has no port")?;
        let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let stream = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(1))
            .map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        let mut request = endpoint
            .url
            .as_str()
            .into_client_request()
            .map_err(|e| e.to_string())?;
        request.headers_mut().insert(
            "Authorization",
            endpoint
                .authorization
                .parse()
                .map_err(|_| "Invalid private controller authorization")?,
        );
        let config = tungstenite::protocol::WebSocketConfig {
            max_message_size: Some(LIMIT as usize),
            max_frame_size: Some(LIMIT as usize),
            ..Default::default()
        };
        let (socket, _) = tungstenite::client::client_with_config(request, stream, Some(config))
            .map_err(|e| e.to_string())?;
        let mut page = Self {
            socket,
            session: String::new(),
            frame: String::new(),
            loader: String::new(),
            url: String::new(),
            next: 0,
            deadline,
            stopping,
        };
        let attached = page.call(
            "Target.attachToTarget",
            json!({"targetId":target,"flatten":true}),
        )?;
        page.session = attached["sessionId"]
            .as_str()
            .ok_or("Owned capture target did not attach")?
            .to_string();
        page.call("DOM.enable", json!({}))?;
        let (frame, loader, url) = page.identity()?;
        page.frame = frame;
        page.loader = loader;
        page.url = url;
        Ok(page)
    }
    fn identity(&mut self) -> Result<(String, String, String), String> {
        let tree = self.call("Page.getFrameTree", json!({}))?;
        let frame = &tree["frameTree"]["frame"];
        let id = frame["id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or("Capture frame is unavailable")?
            .to_string();
        let loader = frame["loaderId"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or("Wait for the browser document to finish loading before capture")?
            .to_string();
        let mut url = frame["url"]
            .as_str()
            .ok_or("Capture URL is unavailable")?
            .to_string();
        if let Some(fragment) = frame["urlFragment"]
            .as_str()
            .filter(|value| !value.is_empty())
        {
            if !url.contains('#') {
                if !fragment.starts_with('#') {
                    url.push('#');
                }
                url.push_str(fragment);
            }
        }
        Ok((id, loader, url))
    }
    fn capture(
        &mut self,
        point: Option<&ferrite_core::browser_annotations::Point>,
    ) -> Result<Value, String> {
        let before = (self.frame.clone(), self.loader.clone(), self.url.clone());
        let element_before = point.and_then(|point| self.element(point).ok());
        let image = self.call(
            "Page.captureScreenshot",
            json!({"format":"png","fromSurface":true,"captureBeyondViewport":false}),
        )?;
        let mut value = json!({"png_base64":image["data"],"url":&before.2});
        // Use the same PNG bound and signature validation as Provider codecs
        // and retained annotation assets; no private output path is exposed.
        let _ = screenshot_bytes(&value)?;
        let element_after = point.and_then(|point| self.element(point).ok());
        let after = self.identity()?;
        if before != after {
            return Err("The browser document changed during capture; capture it again".into());
        }
        if let (Some(before), Some(after)) = (element_before, element_after) {
            if before == after {
                value["element"] =
                    serde_json::to_value(before.1).map_err(|error| error.to_string())?;
            }
        }
        Ok(value)
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        if self.stopping.load(Ordering::Acquire) {
            return Err("Browser capture is shutting down".into());
        }
        if Instant::now() >= self.deadline {
            return Err("Capture context deadline expired".into());
        }
        self.next += 1;
        let mut command = json!({"id":self.next,"method":method,"params":params});
        if !self.session.is_empty() {
            command["sessionId"] = json!(self.session);
        }
        self.socket
            .send(tungstenite::Message::Text(command.to_string()))
            .map_err(|e| e.to_string())?;
        for _ in 0..64 {
            if Instant::now() >= self.deadline {
                return Err("Capture context deadline expired".into());
            }
            let message = self.socket.read().map_err(|e| e.to_string())?;
            if let tungstenite::Message::Text(text) = message {
                let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                if value["id"] == self.next {
                    if value.get("error").is_some() {
                        return Err("Exact element context was unavailable".into());
                    }
                    return Ok(value["result"].clone());
                }
            }
        }
        Err("Capture context event budget exceeded".into())
    }
    fn element(
        &mut self,
        point: &ferrite_core::browser_annotations::Point,
    ) -> Result<(i64, ferrite_core::browser_annotations::ElementContext), String> {
        use ferrite_core::browser_annotations::{ElementContext, Rect};
        if !point.x.is_finite()
            || !point.y.is_finite()
            || point.x < 0.0
            || point.y < 0.0
            || point.x > i32::MAX as f64
            || point.y > i32::MAX as f64
        {
            return Err("Invalid capture point".into());
        }
        // Blink getNodeForLocation consumes document CSS coordinates; its
        // getBoxModel border quad is already viewport CSS coordinates. See
        // InspectorDOMAgent::getNodeForLocation and InspectorHighlight's
        // BuildNodeQuads/FrameQuadToViewport in Chromium's primary source.
        let metrics = self.call("Page.getLayoutMetrics", json!({}))?;
        let viewport = &metrics["cssVisualViewport"];
        let page_x = viewport["pageX"]
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or("Capture scroll position is unavailable")?;
        let page_y = viewport["pageY"]
            .as_f64()
            .filter(|value| value.is_finite())
            .ok_or("Capture scroll position is unavailable")?;
        if viewport["scale"].as_f64() != Some(1.0) {
            return Err("Use region feedback while the page is pinch zoomed".into());
        }
        let document_x = point.x + page_x;
        let document_y = point.y + page_y;
        if document_x < i32::MIN as f64
            || document_y < i32::MIN as f64
            || document_x > i32::MAX as f64
            || document_y > i32::MAX as f64
        {
            return Err("Capture coordinate is out of range".into());
        }
        let location = self.call("DOM.getNodeForLocation", json!({"x":document_x.round() as i32,"y":document_y.round() as i32,"includeUserAgentShadowDOM":false}))?;
        if location["frameId"].as_str() != Some(self.frame.as_str()) {
            return Err("Use region feedback for embedded frames".into());
        }
        let backend = location["backendNodeId"]
            .as_i64()
            .ok_or("No exact node at capture point")?;
        let described = self.call(
            "DOM.describeNode",
            json!({"backendNodeId":backend,"depth":1,"pierce":false}),
        )?;
        let node = &described["node"];
        let tag = node["localName"]
            .as_str()
            .filter(|tag| !tag.is_empty())
            .ok_or("No element at capture point")?
            .to_string();
        if matches!(
            tag.as_str(),
            "canvas" | "iframe" | "object" | "embed" | "video"
        ) || node["shadowRoots"]
            .as_array()
            .is_some_and(|roots| roots.iter().any(|root| root["shadowRootType"] == "closed"))
        {
            return Err("Use region feedback for this content".into());
        }
        let boxed = self.call("DOM.getBoxModel", json!({"backendNodeId":backend}))?;
        let border = boxed["model"]["border"]
            .as_array()
            .filter(|border| border.len() == 8)
            .ok_or("Exact element bounds are unavailable")?;
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for (index, number) in border.iter().enumerate() {
            let number = number
                .as_f64()
                .filter(|number| number.is_finite())
                .ok_or("Invalid element geometry")?;
            if index % 2 == 0 {
                xs.push(number);
            } else {
                ys.push(number);
            }
        }
        let x = xs.iter().copied().fold(f64::INFINITY, f64::min);
        let y = ys.iter().copied().fold(f64::INFINITY, f64::min);
        let width = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max) - x;
        let height = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max) - y;
        if width <= 0.0
            || height <= 0.0
            || point.x < x
            || point.y < y
            || point.x > x + width
            || point.y > y + height
        {
            return Err("Element has no visible bounds".into());
        }
        let ax = self
            .call(
                "Accessibility.getPartialAXTree",
                json!({"backendNodeId":backend,"fetchRelatives":false}),
            )
            .ok();
        let accessible = ax
            .as_ref()
            .and_then(|ax| ax["nodes"].as_array())
            .and_then(|nodes| {
                nodes
                    .iter()
                    .find(|node| node["backendDOMNodeId"] == backend)
            });
        let read = |field: &str| {
            accessible
                .and_then(|node| node[field]["value"].as_str())
                .filter(|text| !text.is_empty())
                .map(|text| text.chars().take(2048).collect::<String>())
        };
        let role = read("role");
        let name = read("name");
        let text = read("value").or_else(|| {
            let text = node["children"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|child| child["nodeValue"].as_str())
                .collect::<Vec<_>>()
                .join("");
            (!text.is_empty()).then(|| text.chars().take(2048).collect())
        });
        Ok((
            backend,
            ElementContext {
                tag,
                role,
                name,
                text,
                bounds_css: Rect {
                    x,
                    y,
                    width,
                    height,
                },
                selector_hint: None,
            },
        ))
    }
}
impl Drop for ReadOnlyPage {
    fn drop(&mut self) {
        let _ = self.socket.close(None);
    }
}
