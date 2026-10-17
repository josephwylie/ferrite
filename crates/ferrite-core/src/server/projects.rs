//! Projects under the Server (ADR 0031): what herdr does for
//! herdr-projects 1.0 by its plugin manifest, the Server does with the
//! shipped `ferrite-projects`.
//!
//! - At start, off the pump's thread: herdr-projects 1.0's take-over of
//!   older installs (the 0.2 Projects set aside, its hooks and skill
//!   links taken out), as `ferrite-projects take-over`; then the
//!   program's `hook` as herdr's `[[startup]]` runs it, which starts the
//!   keeper unless one beats; then what the take-over did, as one toast
//!   once a window is there to show it. Every command takes over at its
//!   start, and two at once would race to set the 0.2 folders aside: so
//!   the startup hook waits for the take-over, and no other hook runs
//!   until the startup hook has returned. A failure is logged and never
//!   holds the Server up. The keeper holds its own lock, one per machine,
//!   and ends once this Server's socket is gone. One that beats may be
//!   the Server's before, about to end: the startup hook runs once more
//!   past it, so a fresh keeper serves this Server.
//! - On each Thread event herdr's manifest names, it runs `hook` again,
//!   the event in herdr's words (`HERDR_PLUGIN_EVENT`, `…_EVENT_JSON`):
//!   a Thread opened is an agent detected, a change of Attention an agent
//!   status changed, a Thread closed a pane closed, and a worktree the
//!   Engine removed (`worktree.remove`, a Thread deleted with its tree) a
//!   worktree removed. The hook decides, starts what it needs detached,
//!   restarts a keeper that is gone, and keeps its housekeeping to once
//!   in two seconds itself. The events before the startup hook returned
//!   run no hook: its housekeeping, and the keeper's ticks, look at every
//!   task.
//!
//! Every Session's agent is told herdr's pane environment
//! (`pane_environment`), as herdr tells every pane, so a task's own agent
//! is known as its agent and what it delegates links this Server. The
//! Server never waits for an event's hook: each is reaped once it ends.
//! Every process here inherits the Server's environment, which a launch
//! from the Finder took from the login shell (`login_shell`), so `gh`,
//! `git`, `claude` and `codex` resolve and gh's login is there; nothing
//! of it is written down. The keeper raises no notification of its own.
//!
//! While the shipped program is still 0.2.34's, which has none of this,
//! the Server runs 0.2's coordinators' migration and ticker instead
//! (`legacy`), and tells no Session of the program.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::api::schema::{Method, NotificationShowParams};
use crate::api::{Event, Request, ThreadInfo};
use crate::engine::Engine;
use crate::{Attention, ThreadId};

/// How often, and how many times, the take-over's notes look for a window:
/// the Server starts before its first window says hello.
const TELL_EVERY: Duration = Duration::from_secs(2);
const TELL_TRIES: u32 = 150;

/// How long after the startup hook the Server runs it once more: past a
/// keeper of the Server before it (three failed checks of the socket,
/// about a second apart), which the first one found still beating.
const SECOND_LOOK: Duration = Duration::from_secs(10);

mod legacy;

/// The shipped `ferrite-projects`: `FERRITE_PROJECTS_BIN`, else beside
/// `exe`, the running executable — as `ferrite-projects` names itself.
fn program(given: Option<OsString>, exe: &Path) -> PathBuf {
    given
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| exe.with_file_name("ferrite-projects"))
}

/// What herdr's pane environment and everything Projects start share:
/// herdr's socket, here the Server's API `socket`, which every task's
/// link names (`current_socket`), and the `program`, which
/// `store/files.rs`'s `bin()` names so the launcher and turn-end hooks
/// run it.
fn herdr_environment(socket: &Path, program: &Path) -> [(&'static str, OsString); 2] {
    [
        ("HERDR_SOCKET_PATH", socket.into()),
        ("FERRITE_PROJECTS_BIN", program.into()),
    ]
}

/// What herdr sets in a pane, for the agent of `thread` while Projects
/// run their `program` (`api::caller_environment`): herdr's environment,
/// and the pane's id, the Thread's, as the hook's events name it
/// (`Hook::agent`). With both, `cli::caller()` knows a task's own agent:
/// its `report --state blocked` is its own, and what it adds is from it.
pub fn pane_environment(
    thread: ThreadId,
    socket: &Path,
    program: &Path,
) -> Vec<(&'static str, OsString)> {
    let mut env = vec![("HERDR_PANE_ID", thread.to_string().into())];
    env.extend(herdr_environment(socket, program));
    env
}

/// What a process running herdr-projects 1.0's code for the Server of the
/// Store in `store` needs: herdr's environment, and the `ferrite` that
/// speaks to the Server. It is in no pane.
pub(super) fn environment(
    store: &Path,
    exe: &Path,
    program: &Path,
) -> Vec<(&'static str, OsString)> {
    let socket = crate::api::address(store);
    let mut env = herdr_environment(&socket, program).to_vec();
    env.push(("FERRITE_SOCKET", socket.into_os_string()));
    env.push(("FERRITE_BIN", exe.into()));
    env
}

/// The shipped `ferrite-projects`, found once at Server start.
pub(super) struct Shipped {
    program: PathBuf,
    /// This executable, links resolved.
    exe: PathBuf,
    /// Still 0.2.34's (`legacy`), which runs no hooks and wants none of
    /// herdr's environment.
    legacy: bool,
}

/// This executable, links resolved, so a `ferrite` linked onto `PATH`
/// finds its sibling; and the shipped program it finds (`program`).
fn located() -> std::io::Result<(PathBuf, PathBuf)> {
    let exe = std::env::current_exe().map(|exe| std::fs::canonicalize(&exe).unwrap_or(exe))?;
    let program = program(std::env::var_os("FERRITE_PROJECTS_BIN"), &exe);
    Ok((exe, program))
}

/// The shipped program; none, logged, when it is not there.
pub(super) fn find() -> Option<Shipped> {
    let Ok((exe, program)) = located() else {
        crate::log!("server: Projects are off: this executable's path is unknown");
        return None;
    };
    if !program.is_file() {
        crate::log!("server: Projects are off: no {}", program.display());
        return None;
    }
    let legacy = is_0_2(&program);
    Some(Shipped {
        program,
        exe,
        legacy,
    })
}

/// `ferrited projects <args…>`: the program the Server runs, with the
/// environment it gives it, for another machine's board (ADR 0031, "Other
/// machines"): `ferrited` is the one program a remote's `PATH` is sure to
/// have (ADR 0026). Its exit code, or 127 when there is none.
pub fn passthrough(store: &Path, args: &[String]) -> i32 {
    match located() {
        Ok((exe, program)) => run_program(&program, &exe, store, args),
        Err(error) => {
            eprintln!("ferrited: this executable's path is unknown: {error}");
            127
        }
    }
}

fn run_program(program: &Path, exe: &Path, store: &Path, args: &[String]) -> i32 {
    use std::os::unix::process::CommandExt;
    if !program.is_file() {
        eprintln!("{}", missing(program));
        return 127;
    }
    let error = Command::new(program)
        .args(args)
        .envs(environment(store, exe, program))
        .exec();
    eprintln!("ferrited: cannot run {}: {error}", program.display());
    126
}

/// What `ferrited projects` says without the program, in the words
/// `remote/why.rs` reads.
fn missing(program: &Path) -> String {
    format!(
        "ferrited: no projects program at {}: put ferrite-projects beside ferrited, or name it in FERRITE_PROJECTS_BIN",
        program.display()
    )
}

/// Whether `program` is still 0.2.34's, which says so to `--version`;
/// 1.0's refuses that flag, having done nothing. Asked until the 0.2
/// crate leaves (ADR 0031), which takes this and `legacy` with it.
fn is_0_2(program: &Path) -> bool {
    Command::new(program)
        .arg("--version")
        .env_remove("FERRITE_THREAD_ID")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|output| output.status.success() && says_0_2(&output.stdout))
}

fn says_0_2(version: &[u8]) -> bool {
    version.starts_with(b"ferrite-projects 0.")
}

impl Shipped {
    /// The program every Session is told of: 1.0's, whose hooks run.
    pub(super) fn hooked(&self) -> Option<PathBuf> {
        (!self.legacy).then(|| self.program.clone())
    }

    /// Start Projects for the Store in `store`: the take-over, the startup
    /// hook and the toast in the background (`startup`), and the hooks
    /// told each Thread event through the answer once that hook returned;
    /// for 0.2.34's program, its ticker (`legacy`), which hears none.
    pub(super) fn start(self, store: &Path) -> Option<Projects> {
        if self.legacy {
            legacy::start(self.program, &self.exe, store);
            return None;
        }
        let env = environment(store, &self.exe, &self.program);
        let projects = Projects::new(self.program, env);
        let (program, env) = (projects.program.clone(), projects.env.clone());
        let started = projects.started.clone();
        let socket = crate::api::address(store);
        let spawned = std::thread::Builder::new()
            .name("projects".into())
            .spawn(move || {
                startup(
                    &program,
                    &env,
                    || {
                        let _ = started.set(Instant::now());
                    },
                    |body| toast(&socket, TOOK_OVER, body),
                )
            });
        if let Err(error) = spawned {
            crate::log!("server: Projects are off: {error}");
        }
        Some(projects)
    }
}

/// The take-over, herdr's `[[startup]]` hook, then the take-over's toast.
/// Whatever starts the keeper goes here, between `take-over`'s return and
/// the toast, never beside it: the program takes over at the start of
/// every command (`keeper` and `hook` too), and two at once would race to
/// set the 0.2 folders aside. So the startup hook is waited for, and only
/// then are the events' hooks `started`.
fn startup(
    program: &Path,
    env: &[(&str, OsString)],
    started: impl FnOnce(),
    show: impl FnMut(&str) -> bool,
) {
    let notes = run(program, env, &["take-over".as_ref()]);
    match Hook::startup().command(program, env).status() {
        Ok(status) if !status.success() => {
            crate::log!("server: Projects: the startup hook ended {status}")
        }
        Ok(_) => {}
        Err(error) => crate::log!("server: Projects: the startup hook did not run: {error}"),
    }
    started();
    tell(&notes, show, TELL_EVERY, TELL_TRIES);
}

/// The notes as one toast, asked for until a window shows it.
fn tell(notes: &[String], mut show: impl FnMut(&str) -> bool, every: Duration, tries: u32) {
    if notes.is_empty() {
        return;
    }
    let body = notes.join("\n");
    for _ in 0..tries {
        if show(&body) {
            return;
        }
        std::thread::sleep(every);
    }
    crate::log!("server: Projects: no window showed their notes");
}

/// The take-over's toast's title.
const TOOK_OVER: &str = "Projects took over the older version";

/// A toast in every window through the Server's own API; true when a
/// window was there to show it.
fn toast(socket: &Path, title: &str, body: &str) -> bool {
    let request = Request {
        id: "projects".into(),
        method: Method::NotificationShow(NotificationShowParams {
            title: title.into(),
            body: Some(body.into()),
            sound: crate::protocol::WireSound::None,
            important: false,
        }),
    };
    let mut answer = Vec::new();
    if crate::api::cli::call(&request, socket, &mut answer, &mut std::io::sink()) != 0 {
        return false;
    }
    serde_json::from_slice::<serde_json::Value>(&answer)
        .ok()
        .and_then(|answer| answer["result"]["windows"].as_u64())
        .is_some_and(|windows| windows > 0)
}

/// What `ferrite-projects` said on success, line by line; a failure is logged.
fn run(program: &Path, env: &[(&str, OsString)], args: &[&OsStr]) -> Vec<String> {
    let output = Command::new(program)
        .args(args)
        .envs(env.iter().map(|(key, value)| (key, value)))
        // The Server is no Thread, whatever started it.
        .env_remove("FERRITE_THREAD_ID")
        .stdin(Stdio::null())
        .output();
    let said = args.join(OsStr::new(" "));
    let said = said.to_string_lossy();
    match output {
        // What it says on success is for the user: the take-over's notes,
        // or 0.2's migration's.
        Ok(output) if output.status.success() => {
            let notes: Vec<String> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::to_owned)
                .collect();
            for note in &notes {
                crate::log!("server: Projects: {note}");
            }
            return notes;
        }
        Ok(output) => crate::log!(
            "server: `ferrite-projects {said}` failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(error) => crate::log!("server: `ferrite-projects {said}` did not run: {error}"),
    }
    Vec::new()
}

/// The program and the events' hooks it is running.
pub(super) struct Projects {
    program: PathBuf,
    env: Vec<(&'static str, OsString)>,
    running: Vec<Child>,
    /// When the startup hook returned (`startup`); until then no hook runs.
    started: Arc<OnceLock<Instant>>,
    /// Whether the startup hook ran once more (`SECOND_LOOK`).
    looked_again: bool,
}

impl Projects {
    pub(super) fn new(program: PathBuf, env: Vec<(&'static str, OsString)>) -> Self {
        Self {
            program,
            env,
            running: Vec::new(),
            started: Arc::default(),
            looked_again: false,
        }
    }

    /// Projects whose startup hook already returned, as a test's Server
    /// wants them.
    #[cfg(test)]
    pub(super) fn started(program: PathBuf, env: Vec<(&'static str, OsString)>) -> Self {
        let projects = Self::new(program, env);
        let _ = projects.started.set(Instant::now());
        projects
    }

    /// What the API's subscribers heard this pass, for the events herdr's
    /// manifest names, and the worktrees `removed` from disk. `engine`
    /// says where a Thread is and what runs it.
    pub(super) fn heard(&mut self, engine: &Engine, events: &[Event], removed: &[PathBuf]) {
        self.reap();
        let Some(&started) = self.started.get() else {
            return;
        };
        if !self.looked_again && started.elapsed() >= SECOND_LOOK {
            self.looked_again = true;
            self.hook(&Hook::startup());
        }
        for event in events {
            let hook = match event {
                Event::ThreadOpened { thread } => Hook::agent("pane.agent_detected", thread),
                Event::Attention { thread, attention } => {
                    let Some(info) = crate::api::thread_info(engine, *thread) else {
                        continue;
                    };
                    Hook::agent(
                        "pane.agent_status_changed",
                        &ThreadInfo {
                            attention: *attention,
                            ..info
                        },
                    )
                }
                Event::ThreadClosed { thread } => Hook::pane_closed(*thread),
                Event::DecisionRequested { .. } | Event::DecisionClosed { .. } => continue,
            };
            self.hook(&hook);
        }
        for path in removed {
            self.hook(&Hook::worktree_removed(path));
        }
    }

    fn hook(&mut self, hook: &Hook) {
        match hook.command(&self.program, &self.env).spawn() {
            Ok(child) => self.running.push(child),
            Err(error) => crate::log!(
                "server: Projects: the `{}` hook did not run: {error}",
                hook.event
            ),
        }
    }

    /// Let go of the hooks that ended, saying which failed.
    fn reap(&mut self) {
        self.running.retain_mut(|child| match child.try_wait() {
            Ok(None) => true,
            Ok(Some(status)) => {
                if !status.success() {
                    crate::log!("server: Projects: a hook ended {status}");
                }
                false
            }
            Err(_) => false,
        });
    }
}

/// One of herdr's plugin events, as herdr hands it to a hook: a pane is a
/// Thread (by its id), a workspace a Project (by `project.list`'s id) and
/// a worktree's workspace its path, as the herdr adapter names them.
#[derive(Debug, PartialEq)]
struct Hook {
    event: &'static str,
    data: Value,
}

impl Hook {
    fn startup() -> Self {
        Self {
            event: "startup",
            data: Value::Null,
        }
    }

    fn agent(event: &'static str, thread: &ThreadInfo) -> Self {
        Self {
            event,
            data: json!({
                "pane_id": thread.thread.to_string(),
                "workspace_id": workspace_id(thread),
                "agent_status": agent_status(thread.attention),
                "agent": thread.provider,
            }),
        }
    }

    fn pane_closed(thread: ThreadId) -> Self {
        Self {
            event: "pane.closed",
            data: json!({ "pane_id": thread.to_string() }),
        }
    }

    fn worktree_removed(path: &Path) -> Self {
        Self {
            event: "worktree.removed",
            data: json!({
                "workspace_id": path,
                "worktree": { "path": path },
            }),
        }
    }

    /// `HERDR_PLUGIN_EVENT_JSON`.
    fn json(&self) -> Value {
        json!({ "event": self.event, "data": self.data })
    }

    /// `program hook`, told this event as herdr tells it.
    fn command(&self, program: &Path, env: &[(&str, OsString)]) -> Command {
        let mut command = Command::new(program);
        command
            .arg("hook")
            .envs(env.iter().map(|(key, value)| (key, value)))
            .env("HERDR_PLUGIN_EVENT", self.event)
            .env_remove("HERDR_PLUGIN_EVENT_JSON")
            // The Server is no Thread, whatever started it.
            .env_remove("FERRITE_THREAD_ID")
            // What the hook starts detached, the keeper, holds no pipe.
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if !self.data.is_null() {
            command.env("HERDR_PLUGIN_EVENT_JSON", self.json().to_string());
        }
        command
    }
}

/// The workspace a Thread is in, in herdr's words: its worktree's path,
/// as the herdr adapter names a worktree's Space, else its Project's id.
fn workspace_id(thread: &ThreadInfo) -> Value {
    match &thread.worktree {
        Some(path) => json!(path),
        None => json!(thread.project),
    }
}

/// An agent's status in herdr's words: Attention's own.
fn agent_status(attention: Attention) -> Value {
    serde_json::to_value(attention).unwrap_or_default()
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::store::Provider;

    /// What herdr-projects' own hook is handed, for a contract both sides
    /// test against (`sw-flow`'s `tests/ferrite_hook_events.rs`).
    fn fixture(name: &str) -> Value {
        let text = match name {
            "pane.agent_status_changed" => {
                include_str!("fixtures/projects-hook/pane.agent_status_changed.json")
            }
            "pane.closed" => include_str!("fixtures/projects-hook/pane.closed.json"),
            "worktree.removed" => include_str!("fixtures/projects-hook/worktree.removed.json"),
            _ => unreachable!("no fixture {name}"),
        };
        serde_json::from_str(text).unwrap()
    }

    fn info(attention: Attention) -> ThreadInfo {
        ThreadInfo {
            thread: ThreadId::new(7),
            title: None,
            provider: Provider::Claude,
            attention,
            cwd: None,
            focused: false,
            project: Some("2".into()),
            provider_session: None,
            attention_seq: 0,
            completion_seq: None,
            boot: String::new(),
            held_prompt: false,
            queued_prompts: 0,
            draft: false,
            worktree: None,
            branch: None,
        }
    }

    #[test]
    fn thread_events_are_herdrs_plugin_events() {
        assert_eq!(
            Hook::agent("pane.agent_status_changed", &info(Attention::Idle)).json(),
            fixture("pane.agent_status_changed")
        );
        assert_eq!(
            Hook::pane_closed(ThreadId::new(7)).json(),
            fixture("pane.closed")
        );
        assert_eq!(
            Hook::worktree_removed(Path::new("/work/app-wt-1")).json(),
            fixture("worktree.removed")
        );
    }

    #[test]
    fn every_attention_is_a_status_herdr_has() {
        let statuses: Vec<Value> = [
            Attention::Idle,
            Attention::Working,
            Attention::Done,
            Attention::Blocked,
        ]
        .into_iter()
        .map(agent_status)
        .collect();
        assert_eq!(statuses, ["idle", "working", "done", "blocked"]);
        let detected = Hook::agent("pane.agent_detected", &info(Attention::Working));
        assert_eq!(detected.data["agent"], "claude");
    }

    #[test]
    fn a_worktrees_thread_is_in_its_worktrees_workspace() {
        let main = Hook::agent("pane.agent_detected", &info(Attention::Idle));
        assert_eq!(main.data["workspace_id"], "2");
        let worktree = ThreadInfo {
            worktree: Some("/work/app-wt-1".into()),
            ..info(Attention::Idle)
        };
        let hook = Hook::agent("pane.agent_detected", &worktree);
        assert_eq!(hook.data["workspace_id"], "/work/app-wt-1");
    }

    #[test]
    fn the_program_is_the_override_else_beside_the_executable() {
        let exe = Path::new("/Applications/Ferrite.app/Contents/MacOS/ferrite");
        assert_eq!(
            program(None, exe),
            Path::new("/Applications/Ferrite.app/Contents/MacOS/ferrite-projects")
        );
        assert_eq!(program(Some("".into()), exe), program(None, exe));
        assert_eq!(program(Some("/opt/fp".into()), exe), Path::new("/opt/fp"));
    }

    #[test]
    fn ferrited_projects_without_the_program_says_how_to_ship_it() {
        let program = Path::new("/opt/ferrite/ferrite-projects-missing");
        let said = missing(program);
        assert!(said.starts_with(
            "ferrited: no projects program at /opt/ferrite/ferrite-projects-missing:"
        ));
        assert!(said.contains("beside ferrited") && said.contains("FERRITE_PROJECTS_BIN"));
        let exe = Path::new("/opt/ferrite/ferrited");
        let args = ["remote".to_string(), "list".to_string()];
        assert_eq!(
            run_program(program, exe, Path::new("/tmp/store"), &args),
            127
        );
    }

    #[test]
    fn its_processes_reach_this_server_by_its_api_socket() {
        let store = Path::new("/home/me/.ferrite/threads");
        let env = environment(
            store,
            Path::new("/f/ferrite"),
            Path::new("/f/ferrite-projects"),
        );
        let api = crate::api::address(store).into_os_string();
        assert_eq!(
            env,
            [
                ("HERDR_SOCKET_PATH", api.clone()),
                ("FERRITE_PROJECTS_BIN", "/f/ferrite-projects".into()),
                ("FERRITE_SOCKET", api),
                ("FERRITE_BIN", "/f/ferrite".into()),
            ]
        );
    }

    /// An agent's pane is its Thread, by the id its events' hooks name it
    /// by: what `cli::caller()` matches a task's link against
    /// (`crates/ferrite-projects`' `tests/ferrite.rs`).
    #[test]
    fn an_agents_pane_is_its_thread_as_its_hooks_name_it() {
        let env = pane_environment(
            ThreadId::new(7),
            Path::new("/s/api.sock"),
            Path::new("/f/ferrite-projects"),
        );
        assert_eq!(
            env,
            [
                ("HERDR_PANE_ID", "7".into()),
                ("HERDR_SOCKET_PATH", "/s/api.sock".into()),
                ("FERRITE_PROJECTS_BIN", "/f/ferrite-projects".into()),
            ]
        );
        assert_eq!(fixture("pane.closed")["data"]["pane_id"], "7");
    }

    /// A fresh per-test scratch directory.
    fn scratch(name: &str) -> PathBuf {
        super::super::tests::scratch(&format!("projects-{name}"))
    }

    /// A stand-in `ferrite-projects` that writes down each call: its
    /// arguments, its event, and the socket it reaches the Server by;
    /// then ends with `status`.
    #[cfg(unix)]
    pub(in crate::server) fn recorder(dir: &Path, status: i32) -> (PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let calls = dir.join("calls");
        let script = dir.join("ferrite-projects");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$*${{HERDR_PLUGIN_EVENT:+ $HERDR_PLUGIN_EVENT}} ${{HERDR_PLUGIN_EVENT_JSON:-none}} | ${{HERDR_SOCKET_PATH:-none}} ${{FERRITE_THREAD_ID:-none}}\" >> '{}'\necho refused >&2\nexit {status}\n",
                calls.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        (script, calls)
    }

    /// The calls written down, once `enough` holds of them or five
    /// seconds have gone.
    #[cfg(unix)]
    pub(in crate::server) fn calls(file: &Path, enough: impl Fn(&[String]) -> bool) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let lines: Vec<String> = std::fs::read_to_string(file)
                .unwrap_or_default()
                .lines()
                .map(str::to_owned)
                .collect();
            if enough(&lines) || Instant::now() >= deadline {
                return lines;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The take-over, then herdr's `[[startup]]` once it returned, and
    /// only then the events' hooks; whatever either says or ends with.
    #[cfg(unix)]
    #[test]
    fn startup_takes_over_then_runs_the_startup_hook_then_the_events() {
        for status in [0, 1] {
            let dir = scratch(&format!("startup-{status}"));
            let (script, file) = recorder(&dir, status);
            let env = [("HERDR_SOCKET_PATH", OsString::from("/s/api.sock"))];
            let mut ran_before = None;
            startup(
                &script,
                &env,
                || ran_before = Some(std::fs::read_to_string(&file).unwrap()),
                |_| unreachable!("nothing to say"),
            );
            assert_eq!(
                ran_before.unwrap(),
                "take-over none | /s/api.sock none\nhook startup none | /s/api.sock none\n",
                "the startup hook after the take-over, both before the events'"
            );
        }
    }

    /// No event's hook runs until the startup hook returned; then the
    /// startup hook once more, after `SECOND_LOOK`, and only once.
    #[cfg(unix)]
    #[test]
    fn the_events_wait_for_the_startup_hook_which_looks_again_once() {
        let dir = scratch("second-look");
        let (script, file) = recorder(&dir, 0);
        let (spawner, _providers) = super::super::tests::spawn();
        let engine = super::super::tests::engine(&dir.join("store"), spawner);
        let closed = [Event::ThreadClosed {
            thread: ThreadId::new(7),
        }];
        let env = vec![("HERDR_SOCKET_PATH", OsString::from("/s/api.sock"))];
        let mut projects = Projects::new(script, env);
        projects.heard(&engine, &closed, &[]);
        assert!(projects.running.is_empty(), "not before the startup hook");

        let _ = projects.started.set(Instant::now() - SECOND_LOOK);
        projects.heard(&engine, &closed, &[]);
        projects.heard(&engine, &[], &[]);
        let said = calls(&file, |calls| calls.len() == 2);
        assert_eq!(said.len(), 2, "{said:?}");
        assert!(
            said.contains(&"hook startup none | /s/api.sock none".to_string()),
            "{said:?}"
        );
        assert!(
            said.iter()
                .any(|call| call.starts_with("hook pane.closed ")),
            "{said:?}"
        );
    }

    #[test]
    fn only_0_2_says_its_version() {
        assert!(says_0_2(b"ferrite-projects 0.2.34\n"));
        assert!(!says_0_2(b"herdr-projects 1.0.1 (/f/ferrite-projects)\n"));
        assert!(!says_0_2(b""));
    }

    /// 0.2's program keeps 0.2's ticker; 1.0's, refusing `--version`, and
    /// one that does not run get 1.0's hooks.
    #[cfg(unix)]
    #[test]
    fn the_program_is_asked_whether_it_is_still_0_2() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch("version");
        let program = |name: &str, body: &str| {
            let path = dir.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        assert!(is_0_2(&program("old", "echo 'ferrite-projects 0.2.34'")));
        assert!(!is_0_2(&program(
            "new",
            "echo 'herdr-projects does not know --version' >&2; exit 1"
        )));
        assert!(!is_0_2(&dir.join("missing")));
    }

    /// 0.2's program is not 1.0's: no Session is told of it.
    #[test]
    fn only_1_0s_program_is_told_to_every_session() {
        let shipped = |legacy| Shipped {
            program: "/f/ferrite-projects".into(),
            exe: "/f/ferrite".into(),
            legacy,
        };
        assert_eq!(
            shipped(false).hooked(),
            Some(PathBuf::from("/f/ferrite-projects"))
        );
        assert_eq!(shipped(true).hooked(), None);
    }

    #[test]
    fn the_notes_are_one_toast_once_a_window_shows_it() {
        let mut asked = Vec::new();
        let notes = [
            "The old herdr-projects (0.2) was set aside.".to_string(),
            "Taken out of the old herdr-projects: its hooks.".to_string(),
        ];
        tell(
            &notes,
            |body| {
                asked.push(body.to_string());
                asked.len() == 3
            },
            Duration::ZERO,
            10,
        );
        assert_eq!(
            asked,
            vec![notes.join("\n"); 3],
            "asked until a window was there"
        );

        asked.clear();
        tell(
            &notes,
            |body| {
                asked.push(body.to_string());
                false
            },
            Duration::ZERO,
            4,
        );
        assert_eq!(asked.len(), 4, "then given up on");
        tell(&[], |_| unreachable!("nothing to say"), Duration::ZERO, 4);
    }

    #[cfg(unix)]
    #[test]
    fn a_program_that_fails_or_cannot_run_is_logged_not_a_panic() {
        let dir = scratch("failing");
        let (script, _) = recorder(&dir, 1);
        assert!(run(&script, &[], &["take-over".as_ref()]).is_empty());
        let mut started = false;
        startup(&dir.join("missing"), &[], || started = true, |_| true);
        assert!(started, "the events' hooks run all the same");
        let mut projects = Projects::started(dir.join("missing"), Vec::new());
        projects.hook(&Hook::startup());
        assert!(projects.running.is_empty());
    }
}
