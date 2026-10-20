//! How Ferrite starts Sessions: the production `Spawner` adapter over the
//! two provider CLIs, and the process-memory sampler the watchdog reads.
//! The app's scripted demo Sessions are their own adapter, in its `demo` —
//! both hand the pump the same `Receiver<SessionEvent>`.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::engine::{Preparation, RssSampler, SpawnRequest, Spawner};
use crate::providers::{
    ClaudeConfig, ClaudeSession, CodexConfig, CodexSession, Session, SettingAck,
};
use crate::session::SessionLifecycle;
use crate::settings::Settings;
use crate::store::Provider;
use crate::ThreadId;

/// One Session may hold this much before the watchdog replaces it. Generous:
/// a busy agent legitimately grows, and a restart costs the operator context.
pub const RSS_LIMIT: u64 = 4 * 1024 * 1024 * 1024;

/// What every new Session is started with beyond the Thread's own choice:
/// the operator's permission, sandbox and effort defaults, read from
/// Settings and shared between the window (which edits them) and the
/// spawner (which reads them at each spawn — so a change applies to the
/// next Session). A Thread's own effort, once chosen, wins over the
/// default.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SessionDefaults {
    pub claude_permission_mode: Option<String>,
    pub codex_approval_policy: Option<String>,
    pub codex_sandbox: Option<String>,
    pub claude_effort: Option<String>,
    pub codex_effort: Option<String>,
    pub placeholder_suggestions: bool,
    /// How readily agents are offered `show_visual` (Settings).
    pub visuals: crate::visual::Level,
}

impl SessionDefaults {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            claude_permission_mode: settings.claude_permission_mode.clone(),
            codex_approval_policy: Some(settings.codex_approval_policy.clone())
                .filter(|policy| !policy.is_empty()),
            codex_sandbox: settings.codex_sandbox.clone(),
            claude_effort: settings.claude_effort.clone(),
            codex_effort: settings.codex_effort.clone(),
            placeholder_suggestions: settings.placeholder_suggestions,
            visuals: settings.visuals,
        }
    }

    /// The effort a spawn on `provider` gets when the Thread chose none.
    fn effort_for(&self, provider: Provider) -> Option<String> {
        match provider {
            Provider::Claude => self.claude_effort.clone(),
            Provider::Codex => self.codex_effort.clone(),
        }
    }
}

/// How the app starts Sessions: the Thread's stored choice becomes a
/// provider CLI process working in its binding.
pub struct Spawn {
    pub defaults: Arc<Mutex<SessionDefaults>>,
    /// What answers an agent's `show_visual` with a screenshot (the app's
    /// off-screen browser, `visual::renderer`). `None`: the tool isn't offered.
    pub renderer: Option<Arc<dyn crate::visual::Renderer>>,
    /// The Server's API socket, told to each agent (`FERRITE_SOCKET`) so it
    /// can drive other Threads (ADR 0027). `None` where there is no API.
    pub api: Option<PathBuf>,
    /// The shipped `ferrite-projects` while Projects run (ADR 0031): each
    /// agent is told it with herdr's socket, as herdr tells every pane.
    pub projects: Option<PathBuf>,
}

impl Spawn {
    pub fn new(
        defaults: Arc<Mutex<SessionDefaults>>,
        renderer: Option<Arc<dyn crate::visual::Renderer>>,
    ) -> Self {
        Self {
            defaults,
            renderer,
            api: None,
            projects: None,
        }
    }

    /// The tool as this spawn offers it: the operator's level and the
    /// renderer (none without one).
    fn visuals(&self, level: crate::visual::Level) -> Option<crate::visual::Visuals> {
        self.renderer
            .clone()
            .map(|renderer| crate::visual::Visuals { level, renderer })
    }
}

impl Spawner for Spawn {
    fn discover_sessions(
        &mut self,
        roots: Vec<(Provider, std::path::PathBuf)>,
        cap: usize,
    ) -> Option<Receiver<io::Result<Vec<crate::import::Candidate>>>> {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("ferrite-session-discovery".into())
            .spawn(move || {
                let mut rows = Vec::new();
                for (provider, root) in roots {
                    let discovered = match provider {
                        Provider::Claude => crate::providers::claude_sessions(&root, cap),
                        Provider::Codex => {
                            let program = crate::providers::discover::program(provider);
                            crate::providers::codex_sessions(&program, cap)
                        }
                    };
                    match discovered {
                        Ok(found) => rows.extend(found),
                        Err(error) => {
                            crate::log!("native session discovery failed: {error}");
                            rows.extend(crate::import::candidates(&[(provider, root)], cap));
                        }
                    }
                }
                rows.sort_by_key(|row| std::cmp::Reverse(row.modified));
                rows.truncate(cap);
                let _ = tx.send(Ok(rows));
            })
            .ok()?;
        Some(rx)
    }

    fn suggest(
        &mut self,
        request: crate::suggest::Request,
        replies: std::sync::mpsc::Sender<crate::suggest::Suggestion>,
    ) {
        crate::suggest::spawn(request, replies);
    }

    fn discover_commands(
        &mut self,
        provider: Provider,
        cwd: &std::path::Path,
    ) -> Option<crate::providers::commands::Discovery> {
        let (tx, rx) = mpsc::channel();
        let failures = tx.clone();
        let cwd = cwd.to_path_buf();
        if let Err(error) = std::thread::Builder::new()
            .name("ferrite-command-discovery".into())
            .spawn(move || {
                let program = crate::providers::discover::program(provider);
                let _ = tx.send(crate::providers::commands::list(&program, provider, &cwd));
            })
        {
            let _ = failures.send(Err(error));
        }
        Some(rx)
    }

    fn discover_models(
        &mut self,
    ) -> Option<std::sync::mpsc::Receiver<(Provider, Vec<crate::ModelInfo>)>> {
        let (tx, rx) = std::sync::mpsc::channel();
        // Claude announces its menu only at initialize, so without this a
        // new Claude model reached the pickers only once some Thread's
        // Session had started on the CLI that knows it.
        let claude = tx.clone();
        std::thread::Builder::new()
            .name("ferrite-claude-model-discovery".into())
            .spawn(move || {
                let program = crate::providers::discover::program(Provider::Claude);
                let cwd = std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir);
                match crate::providers::commands::claude_models(&program, &cwd) {
                    Ok(models) => {
                        let _ = claude.send((Provider::Claude, models));
                    }
                    Err(error) => crate::log!("could not discover Claude models: {error}"),
                }
            })
            .ok()?;
        std::thread::Builder::new()
            .name("ferrite-model-discovery".into())
            .spawn(move || {
                let program = crate::providers::discover::program(Provider::Codex);
                match crate::providers::codex_models(&program) {
                    Ok(models) => {
                        let _ = tx.send((Provider::Codex, models));
                    }
                    Err(error) => crate::log!("could not discover Codex models: {error}"),
                }
            })
            .ok()?;
        Some(rx)
    }

    fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>> {
        self.config(request)
            .spawn(self.defaults.clone())
            .map(|session| session as Box<dyn Session>)
    }

    fn start(&mut self, request: SpawnRequest) -> io::Result<SessionLifecycle> {
        self.start_after(Box::new(|| Ok(())), request)
    }

    /// The preparation runs on the spawning thread, ahead of the CLI: a
    /// revived Thread's stagger and git calls never hold the window.
    fn start_after(
        &mut self,
        prepare: Preparation,
        request: SpawnRequest,
    ) -> io::Result<SessionLifecycle> {
        let config = self.config(request);
        let defaults = self.defaults.clone();
        SessionLifecycle::background(move || {
            prepare()?;
            config.spawn(defaults)
        })
    }
}

/// Everything a spawn needs, resolved on the UI thread (the defaults are
/// read at the moment of the request, as before) and carried to the
/// spawning thread.
enum SessionConfig {
    Claude(ClaudeConfig),
    Codex(CodexConfig),
}

impl SessionConfig {
    fn spawn(self, defaults: Arc<Mutex<SessionDefaults>>) -> io::Result<Box<dyn Session + Send>> {
        let (provider, inner): (Provider, Box<dyn Session + Send>) = match self {
            SessionConfig::Claude(config) => (
                Provider::Claude,
                Box::new(
                    ClaudeSession::spawn(config).map_err(|e| io::Error::other(e.to_string()))?,
                ),
            ),
            SessionConfig::Codex(config) => (
                Provider::Codex,
                Box::new(CodexSession::spawn(config).map_err(|e| io::Error::other(e.to_string()))?),
            ),
        };
        Ok(Box::new(SessionWithDefaults {
            inner,
            provider,
            defaults,
        }))
    }
}

/// A ready production Session resolves a cleared Thread effort against the
/// current Settings. Startup and delivery remain owned by the headless core;
/// this adapter adds only the app's provider-specific defaults.
struct SessionWithDefaults {
    inner: Box<dyn Session + Send>,
    provider: Provider,
    defaults: Arc<Mutex<SessionDefaults>>,
}

impl Session for SessionWithDefaults {
    fn supports_control(&self, kind: crate::ControlKind) -> bool {
        self.inner.supports_control(kind)
    }

    fn control(&mut self, action: crate::SessionControl) -> io::Result<()> {
        self.inner.control(action)
    }

    fn permission_modes(&self) -> Vec<crate::PermissionModeChoice> {
        self.inner.permission_modes()
    }

    fn set_model(&mut self, model: Option<&str>) -> io::Result<SettingAck> {
        self.inner.set_model(model)
    }

    fn search_files(
        &mut self,
        query: &str,
        reply: crate::providers::FileSearchReply,
    ) -> io::Result<()> {
        self.inner.search_files(query, reply)
    }
    fn enqueue(&mut self, client_id: &str, text: &str) -> io::Result<()> {
        self.inner.enqueue(client_id, text)
    }
    fn cancel_queued(&mut self, id: &str) -> io::Result<()> {
        self.inner.cancel_queued(id)
    }

    fn set_suggestions_enabled(&mut self, enabled: bool) -> io::Result<()> {
        self.inner.set_suggestions_enabled(enabled)
    }
    fn take_suggestion(&mut self) -> Option<String> {
        self.inner.take_suggestion()
    }

    fn events(&self) -> &std::sync::mpsc::Receiver<crate::SessionEvent> {
        self.inner.events()
    }

    fn send(&mut self, text: &str) -> io::Result<()> {
        self.inner.send(text)
    }

    fn set_effort(&mut self, effort: Option<&str>) -> io::Result<SettingAck> {
        let effort = match effort {
            Some(effort) => Some(effort.to_string()),
            None => self
                .defaults
                .lock()
                .map_err(|_| io::Error::other("Session defaults are unavailable"))?
                .effort_for(self.provider),
        };
        self.inner.set_effort(effort.as_deref())
    }

    fn interrupt(&mut self) -> io::Result<()> {
        self.inner.interrupt()
    }

    fn respond_to_decision(&mut self, id: &str, answer: crate::DecisionAnswer) -> io::Result<()> {
        self.inner.respond_to_decision(id, answer)
    }

    fn set_name(&mut self, name: &str) -> io::Result<()> {
        self.inner.set_name(name)
    }

    fn pid(&self) -> Option<u32> {
        self.inner.pid()
    }
}

impl Spawn {
    fn config(&self, request: SpawnRequest) -> SessionConfig {
        // The Thread's workspace binding decides where the Session works; a
        // Thread from before bindings falls back to where Ferrite started.
        let cwd = request
            .cwd
            .map(|dir| dir.to_path_buf())
            .or_else(|| std::env::current_dir().ok());
        let model = request.model.map(|model| model.to_string());
        let resume = request.resume.map(|target| target.to_string());
        let name = request.name.map(|name| name.to_string());
        let additional_directories = request.additional_directories;
        let args = request.launch.args.to_vec();
        let launcher = request.launch.launcher.to_vec();
        // The Thread's own environment, then Ferrite's over it.
        let env: Vec<(String, std::ffi::OsString)> = request
            .launch
            .env
            .iter()
            .map(|(name, value)| (name.clone(), value.into()))
            .chain(
                crate::api::caller_environment(
                    request.thread,
                    self.api.as_deref(),
                    self.projects.as_deref(),
                )
                .into_iter()
                .map(|(name, value)| (name.to_string(), value)),
            )
            .collect();
        let defaults = self
            .defaults
            .lock()
            .map(|defaults| defaults.clone())
            .unwrap_or_default();
        // The Thread's own effort, else its launch arguments', else the
        // operator's default for the provider, else nothing — the
        // provider's own.
        let own_effort = request.effort.map(|effort| effort.to_string());
        let default_effort = || defaults.effort_for(request.provider);
        // The newest installed copy of the CLI, wherever it is — not
        // whichever a bare name happens to hit on this launch's PATH.
        let program = crate::providers::discover::program(request.provider);
        match request.provider {
            Provider::Claude => SessionConfig::Claude(ClaudeConfig {
                program: program.clone(),
                cwd,
                model,
                effort: own_effort.or_else(default_effort),
                name,
                permission_mode: request
                    .permission_mode
                    .map(str::to_string)
                    .or(defaults.claude_permission_mode),
                prompt_suggestions: defaults.placeholder_suggestions,
                resume,
                additional_directories,
                visuals: self.visuals(defaults.visuals),
                env,
                launcher,
                args,
            }),
            Provider::Codex => {
                // Written for Codex's terminal UI (ADR 0030); what they set
                // gives way only to the Thread's own choice.
                let launch = crate::providers::codex_launch::from_tui_args(&args);
                SessionConfig::Codex(CodexConfig {
                    program: program.clone(),
                    cwd,
                    model: model.or(launch.model),
                    effort: own_effort.or(launch.effort).or_else(default_effort),
                    approval_policy: request
                        .permission_mode
                        .map(str::to_string)
                        .or(launch.approval_policy)
                        .or(defaults.codex_approval_policy.clone())
                        .or_else(|| Some("on-request".into())),
                    sandbox: launch.sandbox.or(defaults.codex_sandbox.clone()),
                    resume,
                    additional_directories,
                    visuals: self.visuals(defaults.visuals),
                    env,
                    launcher,
                    args: launch.args,
                })
            }
        }
    }
}

/// Resident memory per Session, read the way the panes24 spike reads it.
pub struct ProcessRss {
    requests: Sender<u32>,
    samples: Receiver<(u32, Option<u64>)>,
    cached: HashMap<u32, u64>,
    pending: HashSet<u32>,
}

impl Default for ProcessRss {
    fn default() -> Self {
        Self::with_sampler(rss_bytes)
    }
}

impl ProcessRss {
    fn with_sampler(sample: impl Fn(u32) -> Option<u64> + Send + 'static) -> Self {
        let (request_tx, request_rx) = mpsc::channel();
        let (sample_tx, sample_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("ferrite-rss".into())
            .spawn(move || {
                while let Ok(pid) = request_rx.recv() {
                    if sample_tx.send((pid, sample(pid))).is_err() {
                        break;
                    }
                }
            })
            .expect("RSS sampler thread starts");
        Self {
            requests: request_tx,
            samples: sample_rx,
            cached: HashMap::new(),
            pending: HashSet::new(),
        }
    }
}

impl RssSampler for ProcessRss {
    fn sample(&mut self, _thread: ThreadId, pid: Option<u32>) -> Option<u64> {
        while let Ok((pid, rss)) = self.samples.try_recv() {
            self.pending.remove(&pid);
            if let Some(rss) = rss {
                self.cached.insert(pid, rss);
            }
        }
        let pid = pid?;
        if self.pending.insert(pid) && self.requests.send(pid).is_err() {
            self.pending.remove(&pid);
        }
        self.cached.get(&pid).copied()
    }
}

/// One process's resident bytes, the way the panes24 spike reads it.
#[cfg(not(windows))]
pub fn rss_bytes(pid: u32) -> Option<u64> {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let kilobytes: u64 = String::from_utf8(out.stdout).ok()?.trim().parse().ok()?;
    Some(kilobytes * 1024)
}

/// One process's resident bytes through the native Windows process API.
#[cfg(windows)]
pub fn rss_bytes(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    unsafe {
        let process = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let ok = K32GetProcessMemoryInfo(process, &mut counters, counters.cb);
        CloseHandle(process);
        (ok != 0).then_some(counters.WorkingSetSize as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::applied;

    /// Every agent knows its own Thread; one the Server runs also knows
    /// how to reach the API, and with which binary (ADR 0027).
    #[test]
    fn every_agent_is_told_its_thread_and_where_the_api_listens() {
        let request = |provider| SpawnRequest {
            thread: ThreadId::new(12),
            provider,
            model: None,
            effort: None,
            resume: None,
            permission_mode: None,
            name: None,
            cwd: None,
            additional_directories: Vec::new(),
            launch: Default::default(),
        };
        let served = Spawn {
            api: Some(PathBuf::from("/store/api.sock")),
            ..Spawn::new(Arc::default(), None)
        };
        let SessionConfig::Codex(codex) = served.config(request(Provider::Codex)) else {
            panic!("a Codex request makes a Codex config");
        };
        let names: Vec<&str> = codex.env.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            ["FERRITE_THREAD_ID", "FERRITE_SOCKET", "FERRITE_BIN"]
        );
        assert_eq!(codex.env[0].1, "12");
        assert_eq!(codex.env[1].1, "/store/api.sock");

        let alone = Spawn::new(Arc::default(), None);
        let SessionConfig::Claude(claude) = alone.config(request(Provider::Claude)) else {
            panic!("a Claude request makes a Claude config");
        };
        assert_eq!(claude.env, [("FERRITE_THREAD_ID".to_string(), "12".into())]);
    }

    /// A Thread's own environment goes under Ferrite's, and its launcher
    /// to the CLI's command (`thread.start --env --launcher`, ADR 0031).
    #[test]
    fn a_threads_env_and_launcher_reach_its_cli() {
        let env = [
            ("HERDR_PROJECTS_DATA_DIR".to_string(), "/data".to_string()),
            ("FERRITE_THREAD_ID".to_string(), "99".to_string()),
        ];
        let launcher = ["bwrap".to_string(), "--".to_string()];
        let request = SpawnRequest {
            thread: ThreadId::new(12),
            provider: Provider::Claude,
            model: None,
            effort: None,
            resume: None,
            permission_mode: None,
            name: None,
            cwd: None,
            additional_directories: Vec::new(),
            launch: crate::engine::AgentLaunch {
                args: &[],
                env: &env,
                launcher: &launcher,
            },
        };
        let SessionConfig::Claude(claude) = Spawn::new(Arc::default(), None).config(request) else {
            panic!("a Claude request makes a Claude config");
        };
        assert_eq!(
            claude.env,
            [
                ("HERDR_PROJECTS_DATA_DIR".to_string(), "/data".into()),
                ("FERRITE_THREAD_ID".to_string(), "99".into()),
                ("FERRITE_THREAD_ID".to_string(), "12".into()),
            ],
            "Ferrite's own is set last, over the Thread's"
        );
        assert_eq!(claude.launcher, launcher);
    }

    /// While Projects run, every agent is told its pane — its Thread —
    /// herdr's socket — the API's — and the program, as herdr tells every
    /// pane: it is known as its task's agent, and a task it delegates
    /// links this Server (ADR 0031).
    #[test]
    fn while_projects_run_every_agent_is_in_herdrs_pane_environment() {
        let projects = Spawn {
            api: Some(PathBuf::from("/store/api.sock")),
            projects: Some(PathBuf::from("/f/ferrite-projects")),
            ..Spawn::new(Arc::default(), None)
        };
        let SessionConfig::Claude(claude) = projects.config(SpawnRequest {
            thread: ThreadId::new(12),
            provider: Provider::Claude,
            model: None,
            effort: None,
            resume: None,
            permission_mode: None,
            name: None,
            cwd: None,
            additional_directories: Vec::new(),
            launch: crate::engine::AgentLaunch::default(),
        }) else {
            panic!("a Claude request makes a Claude config");
        };
        let told = |name: &str| {
            claude
                .env
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(told("HERDR_PANE_ID"), Some("12".into()));
        assert_eq!(told("HERDR_SOCKET_PATH"), Some("/store/api.sock".into()));
        assert_eq!(
            told("FERRITE_PROJECTS_BIN"),
            Some("/f/ferrite-projects".into())
        );
    }

    #[derive(Default)]
    struct NativeCalls {
        spawns: usize,
        model: Option<String>,
        controls: Vec<crate::SessionControl>,
    }

    struct NativeProvider {
        calls: Arc<Mutex<NativeCalls>>,
        events: Receiver<crate::SessionEvent>,
        sender: Sender<crate::SessionEvent>,
    }

    impl Session for NativeProvider {
        fn events(&self) -> &Receiver<crate::SessionEvent> {
            &self.events
        }
        fn send(&mut self, _: &str) -> io::Result<()> {
            let model = self
                .calls
                .lock()
                .unwrap()
                .model
                .clone()
                .unwrap_or("default".into());
            self.sender
                .send(crate::SessionEvent::TextDelta { text: model })
                .unwrap();
            self.sender
                .send(crate::SessionEvent::TurnEnded {
                    outcome: crate::TurnOutcome::Completed,
                    cost_usd: None,
                })
                .unwrap();
            Ok(())
        }
        fn interrupt(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn respond_to_decision(&mut self, _: &str, _: crate::DecisionAnswer) -> io::Result<()> {
            Ok(())
        }
        fn set_model(&mut self, model: Option<&str>) -> io::Result<SettingAck> {
            if model == Some("unavailable") {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "native model unavailable",
                ));
            }
            self.calls.lock().unwrap().model = model.map(str::to_owned);
            Ok(applied())
        }
        fn supports_control(&self, kind: crate::ControlKind) -> bool {
            kind != crate::ControlKind::LoginMcp
        }
        fn permission_modes(&self) -> Vec<crate::PermissionModeChoice> {
            vec![crate::PermissionModeChoice {
                value: "native-mode".into(),
                label: "Provider mode".into(),
            }]
        }
        fn control(&mut self, action: crate::SessionControl) -> io::Result<()> {
            if matches!(action, crate::SessionControl::LoginMcp { .. }) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "native login refused",
                ));
            }
            self.calls.lock().unwrap().controls.push(action);
            Ok(())
        }
    }

    struct WrappedNative(Arc<Mutex<NativeCalls>>);
    impl Spawner for WrappedNative {
        fn spawn(&mut self, request: SpawnRequest) -> io::Result<Box<dyn Session>> {
            let (sender, events) = mpsc::channel();
            self.0.lock().unwrap().spawns += 1;
            sender
                .send(crate::SessionEvent::Init {
                    session_id: "same-native-conversation".into(),
                    model: "initial".into(),
                })
                .unwrap();
            Ok(Box::new(SessionWithDefaults {
                inner: Box::new(NativeProvider {
                    calls: self.0.clone(),
                    events,
                    sender,
                }),
                provider: request.provider,
                defaults: Arc::new(Mutex::new(SessionDefaults::default())),
            }))
        }
    }

    #[test]
    fn production_wrapper_changes_model_without_replacing_the_conversation() {
        use crate::{cockpit::Cockpit, store::Store, workspace::WorkspaceChoice};
        for provider in [Provider::Claude, Provider::Codex] {
            let path = std::env::temp_dir().join(format!(
                "ferrite-wrapped-model-{}-{provider:?}",
                std::process::id()
            ));
            let calls = Arc::new(Mutex::new(NativeCalls::default()));
            let mut cockpit = Cockpit::new(
                Store::open(path.join("store")).unwrap(),
                Box::new(WrappedNative(calls.clone())),
            );
            let thread = cockpit
                .open(
                    provider,
                    WorkspaceChoice::Main {
                        checkout: path.clone(),
                    },
                )
                .unwrap();
            cockpit.pump();
            cockpit.send(thread, "first turn".into());
            cockpit.pump();
            cockpit.set_model(thread, Some("selected".into())).unwrap();
            assert_eq!(
                calls.lock().unwrap().spawns,
                1,
                "a model choice must not spawn a replacement"
            );
            assert_eq!(calls.lock().unwrap().model.as_deref(), Some("selected"));
            assert_eq!(
                cockpit.peek(thread).unwrap().model.as_deref(),
                Some("selected")
            );
            assert_eq!(
                cockpit.thread(thread).unwrap().transcript().session_id(),
                Some("same-native-conversation")
            );
            let error = cockpit
                .set_model(thread, Some("unavailable".into()))
                .unwrap_err();
            assert!(error.to_string().contains("native model unavailable"));
            assert_eq!(calls.lock().unwrap().spawns, 1);
            assert_eq!(
                cockpit.peek(thread).unwrap().model.as_deref(),
                Some("selected")
            );
            cockpit.send(thread, "after rejected model change".into());
            cockpit.pump();
            assert_eq!(
                cockpit.thread(thread).unwrap().transcript().session_id(),
                Some("same-native-conversation")
            );
            cockpit.set_model(thread, None).unwrap();
            assert_eq!(calls.lock().unwrap().model, None);
            assert_eq!(calls.lock().unwrap().spawns, 1);
            drop(cockpit);
            std::fs::remove_dir_all(path).unwrap();
        }
    }

    #[test]
    fn production_wrapper_exposes_native_modes_and_routes_controls() {
        use crate::{ControlKind as K, SessionControl as C};
        let calls = Arc::new(Mutex::new(NativeCalls::default()));
        let (sender, events) = mpsc::channel();
        let mut session: Box<dyn Session> = Box::new(SessionWithDefaults {
            inner: Box::new(NativeProvider {
                calls: calls.clone(),
                events,
                sender,
            }),
            provider: Provider::Claude,
            defaults: Arc::new(Mutex::new(SessionDefaults::default())),
        });
        for kind in [
            K::RefreshContext,
            K::RefreshMcp,
            K::ReconnectMcp,
            K::ReloadMcp,
            K::StopTask,
            K::BackgroundTasks,
            K::SetPermissionMode,
        ] {
            assert!(
                session.supports_control(kind),
                "native capability {kind:?} was lost"
            );
        }
        assert!(!session.supports_control(K::LoginMcp));
        assert_eq!(
            session.permission_modes(),
            vec![crate::PermissionModeChoice {
                value: "native-mode".into(),
                label: "Provider mode".into()
            }]
        );
        let actions = vec![
            C::RefreshContext,
            C::RefreshMcp,
            C::ReconnectMcp {
                server: "server:exact/id".into(),
            },
            C::ReloadMcp,
            C::StopTask {
                id: "task:exact/id".into(),
            },
            C::BackgroundTasks,
            C::SetPermissionMode {
                mode: "native-mode".into(),
            },
        ];
        for action in &actions {
            session.control(action.clone()).unwrap();
        }
        assert_eq!(calls.lock().unwrap().controls, actions);
        let error = session
            .control(C::LoginMcp {
                server: "refused".into(),
            })
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "native login refused");
    }

    #[test]
    fn default_effort_reads_current_settings_for_the_ready_sessions_provider() {
        use std::sync::mpsc::{channel, Receiver};
        struct Recording {
            events: Receiver<crate::SessionEvent>,
            efforts: Arc<Mutex<Vec<Option<String>>>>,
        }
        impl Session for Recording {
            fn events(&self) -> &Receiver<crate::SessionEvent> {
                &self.events
            }
            fn send(&mut self, _: &str) -> io::Result<()> {
                Ok(())
            }
            fn interrupt(&mut self) -> io::Result<()> {
                Ok(())
            }
            fn respond_to_decision(&mut self, _: &str, _: crate::DecisionAnswer) -> io::Result<()> {
                Ok(())
            }
            fn set_effort(&mut self, effort: Option<&str>) -> io::Result<SettingAck> {
                self.efforts.lock().unwrap().push(effort.map(str::to_owned));
                Ok(applied())
            }
        }
        for provider in [Provider::Claude, Provider::Codex] {
            let (_, events) = channel();
            let efforts = Arc::new(Mutex::new(Vec::new()));
            let defaults = Arc::new(Mutex::new(SessionDefaults {
                claude_effort: Some("high".into()),
                codex_effort: Some("low".into()),
                ..Default::default()
            }));
            let mut session = SessionWithDefaults {
                inner: Box::new(Recording {
                    events,
                    efforts: efforts.clone(),
                }),
                provider,
                defaults: defaults.clone(),
            };
            session.set_effort(Some("max")).unwrap();
            session.set_effort(None).unwrap();
            let expected = if provider == Provider::Claude {
                "high"
            } else {
                "low"
            };
            assert_eq!(
                *efforts.lock().unwrap(),
                [Some("max".into()), Some(expected.into())]
            );
            *defaults.lock().unwrap() = SessionDefaults::default();
            session.set_effort(None).unwrap();
            assert_eq!(efforts.lock().unwrap().last(), Some(&None));
        }
    }

    /// The Thread's effort wins; the Settings default fills in only when
    /// the Thread chose none; and each provider reads its own.
    #[test]
    fn the_settings_effort_is_the_default_when_the_thread_chose_none() {
        let mut settings = Settings::default();
        settings.claude_effort = Some("high".into());
        let defaults = SessionDefaults::from_settings(&settings);
        assert_eq!(
            defaults.effort_for(Provider::Claude).as_deref(),
            Some("high")
        );
        assert_eq!(defaults.effort_for(Provider::Codex), None);
        assert_eq!(
            Some("max".to_string()).or_else(|| defaults.effort_for(Provider::Claude)),
            Some("max".into())
        );
        assert_eq!(
            None.or_else(|| defaults.effort_for(Provider::Claude)),
            Some("high".into())
        );
    }

    /// Runs against whichever process table this platform has — `ps` here,
    /// `tasklist` when the suite runs on Windows.
    #[test]
    fn the_rss_sampler_reads_this_very_process() {
        let rss = rss_bytes(std::process::id()).expect("own pid must sample");
        assert!(rss > 1024 * 1024, "resident bytes were {rss}");
    }

    #[test]
    fn the_watchdog_never_waits_for_process_measurement() {
        use std::time::{Duration, Instant};

        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut sampler = ProcessRss::with_sampler(move |_| {
            let _ = started_tx.send(());
            let _ = release_rx.recv();
            Some(42)
        });

        let before = Instant::now();
        assert_eq!(sampler.sample(ThreadId::new(1), Some(7)), None);
        assert!(before.elapsed() < Duration::from_millis(50));
        started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        release_tx.send(()).unwrap();

        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if sampler.sample(ThreadId::new(1), Some(7)) == Some(42) {
                return;
            }
            std::thread::yield_now();
        }
        panic!("background sample was not adopted");
    }
}
