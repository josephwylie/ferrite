//! Typed calls to the herdr CLI. Differences between herdr versions stay here.
//!
//! Ferrite: this is the adapter seam (ADR 0030). Upstream's names, types and
//! signatures are kept so every caller merges unchanged; the calls go to
//! Ferrite's Server through the `ferrite` CLI (ADR 0027) instead: `bin` is
//! `$FERRITE_BIN`, `socket` its `$FERRITE_SOCKET`, and a pane, tab or agent
//! is a Thread, named by its id (never reused, ADR 0029). A Herdr Space is a
//! Ferrite Project (`project.*`), except that a Thread working in a linked
//! worktree is in that worktree's own Space, named by its path, as Herdr's
//! worktree Spaces are. Herdr's agent states and Ferrite's Attention are the
//! same four words.
//!
//! Ferrite has no shells, keys or screens. A Thread is born with its agent,
//! so a placement (`tab create`, `workspace create`, `worktree create` and
//! `open`) only stages where the Thread will start, as a [`Staged`] pane, and
//! `agent start` starts it there. What Herdr reads off a screen, Ferrite
//! knows exactly ([`PaneState`]); a key press becomes what it meant. A call
//! with no Ferrite meaning is refused with [`NOT_WIRED`] before anything
//! runs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::runner::{Cmd, Runner};

/// Ferrite: Ferrite's own minimum, the first release whose API answers every
/// call below and prints `ferrite --version`.
pub const MIN_VERSION: Version = Version(0, 4, 0);
pub const CALL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u64, pub u64, pub u64);

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// Parses `herdr 0.9.1` and `herdr 0.9.2-preview.3`; a pre-release suffix is ignored.
pub fn parse_version(text: &str) -> Option<Version> {
    let token = text
        .split_whitespace()
        .find(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit()))?;
    let core = token.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    Some(Version(parts.next()??, parts.next()??, parts.next()??))
}

/// A herdr command that does not talk to a server (`--version`, `session list`).
fn bare(bin: &str) -> Cmd {
    Cmd::new(bin, CALL_TIMEOUT)
}

/// Ferrite: `ferrite --version` prints `ferrite X.Y.Z`, as herdr's does.
pub fn version(bin: &str, runner: &dyn Runner) -> Result<Version> {
    let out = runner.run(&bare(bin).arg("--version"))?;
    if !out.success() {
        bail!("`{bin} --version` failed: {}", out.error_text());
    }
    parse_version(&out.stdout)
        .with_context(|| format!("could not read a version from `{}`", out.stdout.trim()))
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SessionInfo {
    pub name: String,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub running: bool,
    pub socket_path: PathBuf,
}

/// Ferrite: where the Server takes API connections when nothing names
/// another: `api.sock` in the Store (`~/.ferrite/threads`, ADR 0025).
pub fn default_socket(home: &Path) -> PathBuf {
    home.join(".ferrite").join("threads").join("api.sock")
}

/// Ferrite: one Server per computer, so one session, `default`: the Server
/// at `$FERRITE_SOCKET` (else its default socket), running when it answers.
pub fn session_list(bin: &str, runner: &dyn Runner) -> Result<Vec<SessionInfo>> {
    let socket = match std::env::var_os("FERRITE_SOCKET").filter(|s| !s.is_empty()) {
        Some(socket) => PathBuf::from(socket),
        None => default_socket(&std::env::var_os("HOME").map(PathBuf::from).context("HOME is not set")?),
    };
    let running = Herdr::new(bin, &socket, runner).reachable();
    Ok(vec![SessionInfo { name: "default".into(), default: true, running, socket_path: socket }])
}

/// herdr bound to one session's socket. Every call a project makes goes through
/// this, so a project always talks to the session it was opened in.
pub struct Herdr<'a> {
    bin: String,
    socket: PathBuf,
    /// A saved SSH machine: every call is forwarded as `herdr --machine M ...`.
    machine: Option<String>,
    runner: &'a dyn Runner,
}

impl<'a> Herdr<'a> {
    pub fn new(bin: impl Into<String>, socket: impl Into<PathBuf>, runner: &'a dyn Runner) -> Self {
        Herdr {
            bin: bin.into(),
            socket: socket.into(),
            machine: None,
            runner,
        }
    }

    /// The same session, with calls forwarded to a saved machine.
    pub fn on_machine(&self, machine: &str) -> Herdr<'a> {
        Herdr {
            bin: self.bin.clone(),
            socket: self.socket.clone(),
            machine: (!machine.is_empty()).then(|| machine.to_string()),
            runner: self.runner,
        }
    }

    /// Ferrite: the socket this project recorded is set over any inherited
    /// one.
    pub fn cmd(&self, timeout: Duration) -> Cmd {
        let cmd = Cmd::new(&self.bin, timeout).env("FERRITE_SOCKET", self.socket.to_string_lossy());
        match &self.machine {
            Some(machine) => cmd.args(["--machine", machine]),
            None => cmd,
        }
    }

    /// True when the socket file exists and the server answers.
    pub fn reachable(&self) -> bool {
        if !self.socket.exists() {
            return false;
        }
        self.runner
            .run(&self.cmd(CALL_TIMEOUT).args(["thread", "list"]))
            .map(|out| out.success())
            .unwrap_or(false)
    }
}

/// A herdr call that failed. `code` is herdr's own error code (for example
/// `agent_blocked` or `pane_not_found`), or `timeout` / `unreachable` / `failed`
/// when herdr never answered with one.
#[derive(Debug, Clone, PartialEq)]
pub struct HerdrError {
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for HerdrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "herdr: {} ({})", self.message, self.code)
    }
}

impl std::error::Error for HerdrError {}

/// Ferrite: the code a herdr call with no Ferrite meaning is refused with.
pub const NOT_WIRED: &str = "not_wired";

fn refused<T>(code: &str, message: impl Into<String>) -> Result<T, HerdrError> {
    Err(HerdrError { code: code.into(), message: message.into() })
}

/// Ferrite: refuses a herdr call Ferrite has no meaning for.
fn not_wired<T>(call: &str, why: &str) -> Result<T, HerdrError> {
    refused(NOT_WIRED, format!("`herdr {call}` has no Ferrite counterpart: {why}"))
}

/// Ferrite: Ferrite's error code under the Herdr name callers match on.
fn herdr_code(code: &str) -> &str {
    match code {
        "thread_not_found" => "pane_not_found",
        "project_not_found" => "workspace_not_found",
        "server_unavailable" => "unreachable",
        other => other,
    }
}

pub const AGENT_START_TIMEOUT: Duration = Duration::from_secs(20);
/// How long `agent_prompt_confirmed` lets herdr wait for the agent to start.
pub const PROMPT_WAIT: Duration = Duration::from_secs(8);

/// herdr refused the prompt before typing anything.
///
/// Ferrite: `agent_starting` (a starting Session already holds a prompt) is a
/// refusal before sending too.
pub fn refused_before_typing(error: &HerdrError) -> bool {
    matches!(error.code.as_str(), "agent_blocked" | "pane_not_found" | "unreachable" | "agent_starting")
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct Pane {
    pub pane_id: String,
    pub tab_id: String,
    pub workspace_id: String,
    #[serde(default)]
    pub cwd: String,
    /// The working directory of the pane's foreground process; differs from
    /// `cwd` (the shell's) while `open` runs a coordinator in the pane.
    #[serde(default)]
    pub foreground_cwd: String,
    /// Stable across pane moves; restarts with the server.
    #[serde(default)]
    pub terminal_id: String,
    #[serde(default)]
    pub focused: bool,
}

/// A Space as `workspace list` shows it.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct Workspace {
    pub workspace_id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub focused: bool,
    #[serde(default)]
    pub pane_count: usize,
    #[serde(default)]
    pub worktree: Option<WorkspaceWorktree>,
}

/// The git checkout herdr groups a Space under. A repository's primary Space
/// (the "parent" herdr makes or reuses for its worktrees) is not linked.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct WorkspaceWorktree {
    #[serde(default)]
    pub repo_key: String,
    #[serde(default)]
    pub checkout_path: String,
    #[serde(default)]
    pub is_linked_worktree: bool,
}

/// The native session reference an official integration reported.
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct AgentSession {
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct Agent {
    pub pane_id: String,
    pub tab_id: String,
    pub workspace_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub agent: String,
    #[serde(default)]
    pub agent_status: String,
    #[serde(default)]
    pub cwd: String,
    /// See `Pane::foreground_cwd`.
    #[serde(default)]
    pub foreground_cwd: String,
    #[serde(default)]
    pub terminal_id: String,
    #[serde(default)]
    pub state_change_seq: u64,
    #[serde(default)]
    pub agent_session: Option<AgentSession>,
}

impl Agent {
    /// The one "ready for a prompt" predicate: state `idle` or `done`.
    pub fn ready(&self) -> bool {
        ready_state(&self.agent_status)
    }

    /// True when the agent works in `dir`: its shell's directory, or its own
    /// when it runs as a child of `open` in a shell elsewhere.
    pub fn works_in(&self, dir: &str) -> bool {
        !dir.is_empty() && (self.cwd == dir || self.foreground_cwd == dir)
    }

    pub fn session_id(&self) -> &str {
        self.agent_session.as_ref().map(|s| s.value.as_str()).unwrap_or("")
    }
}

pub fn ready_state(state: &str) -> bool {
    matches!(state, "idle" | "done")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Created {
    pub workspace_id: String,
    pub tab_id: String,
    pub pane_id: String,
}

/// Ferrite: an id as the API writes it: a Thread's is a number, a Project's
/// text.
fn id_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Ferrite: one open Thread as the API answers it (`thread.list`,
/// `thread.get`, `thread.start`; ADR 0027).
#[derive(Debug, Clone, Default, Deserialize)]
struct ThreadInfo {
    #[serde(default)]
    thread: Value,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    attention: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    focused: bool,
    #[serde(default)]
    project: Value,
    #[serde(default)]
    provider_session: Option<String>,
    #[serde(default)]
    attention_seq: u64,
    #[serde(default)]
    held_prompt: bool,
    #[serde(default)]
    queued_prompts: u64,
    #[serde(default)]
    draft: bool,
    /// The linked worktree it works in.
    #[serde(default)]
    worktree: Option<String>,
}

impl ThreadInfo {
    fn id(&self) -> String {
        id_text(&self.thread)
    }

    /// Its Space: its linked worktree's, else its Project.
    fn space(&self) -> String {
        self.worktree.clone().filter(|path| !path.is_empty()).unwrap_or_else(|| id_text(&self.project))
    }

    fn pane(&self) -> Pane {
        let id = self.id();
        Pane {
            pane_id: id.clone(),
            tab_id: id.clone(),
            workspace_id: self.space(),
            cwd: self.cwd.clone().unwrap_or_default(),
            foreground_cwd: String::new(),
            terminal_id: id,
            focused: self.focused,
        }
    }

    fn agent(&self) -> Agent {
        let id = self.id();
        Agent {
            pane_id: id.clone(),
            tab_id: id.clone(),
            workspace_id: self.space(),
            name: self.title.clone().unwrap_or_default(),
            agent: self.provider.clone(),
            agent_status: self.attention.clone(),
            cwd: self.cwd.clone().unwrap_or_default(),
            foreground_cwd: String::new(),
            terminal_id: id,
            state_change_seq: self.attention_seq,
            agent_session: self.provider_session.clone().filter(|s| !s.is_empty()).map(|value| AgentSession { value }),
        }
    }
}

/// Ferrite: a Project as `project.list` answers it.
#[derive(Debug, Clone, Default, Deserialize)]
struct ProjectInfo {
    #[serde(default)]
    id: Value,
    #[serde(default)]
    label: String,
    /// It holds the Thread the operator is looking at.
    #[serde(default)]
    focused: bool,
    #[serde(default)]
    worktree: Option<WorkspaceWorktree>,
}

/// Ferrite: a pane at a shell prompt, which Ferrite has not: where a Thread
/// will start once `agent_start` names its agent. Its id says everything, so
/// it survives the process that placed it (`thread start` places, the
/// ticker launches): `new:<focus 0|1>:<space>:<cwd>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Staged {
    /// The Project it joins, or the linked worktree's Space (its path).
    pub space: String,
    pub focus: bool,
    pub cwd: String,
}

const STAGED: &str = "new:";

impl Staged {
    pub fn id(&self) -> String {
        format!("{STAGED}{}:{}:{}", u8::from(self.focus), self.space, self.cwd)
    }

    pub fn parse(pane: &str) -> Option<Staged> {
        let rest = pane.strip_prefix(STAGED)?;
        let (focus, rest) = rest.split_once(':')?;
        let (space, cwd) = rest.split_once(':')?;
        Some(Staged { space: space.into(), focus: focus == "1", cwd: cwd.into() })
    }

    /// A linked worktree's Space is its path; a Project's is its id.
    fn in_worktree(&self) -> bool {
        Path::new(&self.space).is_absolute()
    }

    fn created(&self) -> Created {
        let id = self.id();
        Created { workspace_id: self.space.clone(), tab_id: id.clone(), pane_id: id }
    }
}

/// Ferrite: true for a pane staged for a Thread not yet started. It stands
/// for a shell pane: there, until its agent starts.
pub fn staged(pane: &str) -> bool {
    Staged::parse(pane).is_some()
}

/// Ferrite: the Provider that runs an agent kind. Ferrite runs two; a kind
/// with none fails as a missing binary does in Herdr.
fn provider(kind: &str) -> Result<&'static str, HerdrError> {
    match kind {
        "claude" => Ok("claude"),
        "codex" => Ok("codex"),
        _ => refused("no_provider", format!("Ferrite has no Provider for `{kind}`; it runs claude and codex")),
    }
}

/// Ferrite: where a new worktree goes, below the directory of the Server's
/// socket (the Store) as Ferrite's own do, Herdr's layout within:
/// `worktrees/<repo>/<branch with hyphens>`.
fn worktree_dir(store: &Path, repo: &str, branch: &str) -> PathBuf {
    let name = Path::new(repo).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "repo".into());
    store.join("worktrees").join(name).join(branch.replace('/', "-"))
}

/// Ferrite: a git command's output, or its failure as herdr reports one.
fn git_ran(out: Result<crate::runner::Output>, what: &str) -> Result<crate::runner::Output, HerdrError> {
    let out = out.map_err(|e| HerdrError { code: "unreachable".into(), message: format!("{e:#}") })?;
    if out.timed_out {
        return refused("timeout", format!("`git {what}` timed out"));
    }
    if !out.success() {
        return refused("failed", format!("`git {what}`: {}", out.error_text()));
    }
    Ok(out)
}

/// Ferrite: what a pane's screen shows, as Ferrite knows it exactly: whether
/// the operator has a draft, a prompt held or queued for the agent, the
/// Decisions waiting on it, and the tail of its transcript. Read through
/// [`crate::prompt_box::Screen`] where upstream reads the screen.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaneState {
    /// A window's Composer holds text not yet sent.
    pub draft: bool,
    /// A prompt sent but not yet taken: held for a starting Session, or
    /// queued behind the agent's work.
    pub waiting_prompts: bool,
    /// Each pending Decision's text: what it asks.
    pub decisions: Vec<String>,
    pub transcript: String,
}

/// Ferrite: the input box's text while the operator drafts: only the
/// window has the words; that some are there is what counts.
pub const DRAFT: &str = "…";

impl crate::prompt_box::Screen for PaneState {
    /// The draft, or empty when nothing waits; `None` while a prompt is
    /// held or queued, which is neither typed text nor an empty box.
    fn box_text(&self, _kind: &str) -> Option<String> {
        match (self.draft, self.waiting_prompts) {
            (true, _) => Some(DRAFT.into()),
            (false, true) => None,
            (false, false) => Some(String::new()),
        }
    }

    fn plain(&self) -> String {
        self.transcript.clone()
    }

    /// A trust question shows only as a Decision; the transcript quoting
    /// one never is.
    fn trust_text(&self, _kind: &str) -> Option<String> {
        (!self.decisions.is_empty()).then(|| self.decisions.join("\n"))
    }
}

/// Ferrite: the text of a pending Decision as `decision.list` gives it.
fn decision_text(decision: &Value) -> String {
    let mut parts: Vec<&str> = ["description", "tool_name"].iter().filter_map(|k| decision[*k].as_str()).filter(|t| !t.is_empty()).collect();
    for question in decision["kind"]["questions"].as_array().into_iter().flatten() {
        parts.extend(question["question"].as_str());
    }
    parts.join("\n")
}

/// Ferrite: the answer a key press gives a pending Decision, as the menu
/// Herdr's terminal showed would have taken it: `1`, `y` or Enter allows (or
/// picks the first choice), `2` allows always, `3` or `n` denies, a digit
/// picks that choice. `None` when the keys mean nothing to it.
fn key_answer(decision: &Value, keys: &[String]) -> Option<Value> {
    let key = match keys {
        [key] => key.as_str(),
        [key, enter] if enter == "enter" => key.as_str(),
        _ => return None,
    };
    match decision["kind"]["kind"].as_str()? {
        "approval" => match key {
            "1" | "y" | "enter" => Some(json!({ "kind": "allow", "input": decision["input"] })),
            "2" => match decision["suggestions"].as_array().and_then(|all| all.first()) {
                Some(suggestion) => Some(json!({ "kind": "allow_always", "input": decision["input"], "suggestion": suggestion["value"] })),
                None => Some(json!({ "kind": "allow", "input": decision["input"] })),
            },
            "3" | "n" => Some(json!({ "kind": "deny", "message": "" })),
            _ => None,
        },
        "questions" => {
            let [question] = decision["kind"]["questions"].as_array()?.as_slice() else {
                return None;
            };
            let pick = match key {
                "enter" => 0,
                digit => digit.parse::<usize>().ok()?.checked_sub(1)?,
            };
            (pick < question["options"].as_array()?.len()).then(|| json!({ "kind": "questions", "answers": [{ "picks": [pick] }] }))
        }
        _ => None,
    }
}

impl<'a> Herdr<'a> {
    /// Runs one herdr command and returns the `result` object of its JSON reply.
    ///
    /// Ferrite: the herdr command lines callers build themselves, answered by
    /// Ferrite in Herdr's reply shapes.
    pub fn call(&self, args: &[&str], timeout: Duration) -> Result<Value, HerdrError> {
        match args {
            // The nav is Ferrite's own: its rows, groups and order are drawn
            // from the crate's data (the Projects view), never pushed.
            [_, "report-metadata", ..] => Ok(Value::Null),
            ["pane", "current", "--current"] => {
                let pane = self.thread_reply(&["thread", "get", "--current"], timeout)?.agent();
                Ok(json!({ "pane": { "pane_id": pane.pane_id, "terminal_id": pane.terminal_id, "agent": pane.agent, "agent_session": pane.agent_session.map(|s| json!({ "value": s.value })) } }))
            }
            ["tab" | "pane", "close", pane] => match staged(pane) {
                true => Ok(Value::Null),
                false => self.ferrite(&["thread", "close", "--thread", pane], timeout),
            },
            ["tab", "rename", pane, title] => match staged(pane) {
                // Its title is the agent's name, given at its start.
                true => Ok(Value::Null),
                false => self.ferrite(&["thread", "rename", "--thread", pane, "--title", title], timeout),
            },
            ["workspace", "close", space] => self.space_close(space, timeout),
            ["notification", "show", title, "--body", body, "--sound", sound] => {
                let mut ferrite = vec!["notification", "show", "--title", title, "--body", body, "--sound", sound];
                // A sound that asks for the user marks an important one: it
                // stays until dismissed.
                if *sound == "request" {
                    ferrite.push("--important");
                }
                self.ferrite(&ferrite, timeout)
            }
            _ => not_wired(&args.join(" "), "Ferrite's API has no such operation"),
        }
    }

    /// Ferrite: runs one `ferrite` command and returns the `result` of its
    /// reply: one JSON line, a refusal on stderr with exit status 1.
    fn ferrite(&self, args: &[&str], timeout: Duration) -> Result<Value, HerdrError> {
        let cmd = self.cmd(timeout).args(args.iter().copied());
        let out = self.runner.run(&cmd).map_err(|e| HerdrError {
            code: "unreachable".into(),
            message: format!("{e:#}"),
        })?;
        if out.timed_out {
            return Err(HerdrError {
                code: "timeout".into(),
                message: format!("`ferrite {}` timed out", args.join(" ")),
            });
        }
        // Which stream the reply lands on is not something to depend on.
        let reply = [&out.stdout, &out.stderr]
            .into_iter()
            .find_map(|text| serde_json::from_str::<Value>(text.trim()).ok());
        if let Some(reply) = reply {
            if let Some(error) = reply.get("error") {
                return Err(HerdrError {
                    code: herdr_code(error["code"].as_str().unwrap_or("failed")).to_string(),
                    message: error["message"].as_str().unwrap_or("").to_string(),
                });
            }
            if out.success() {
                return Ok(reply.get("result").cloned().unwrap_or(Value::Null));
            }
        }
        Err(HerdrError {
            code: "failed".into(),
            message: format!("`ferrite {}`: {}", args.join(" "), out.error_text()),
        })
    }

    fn ferrite_as<T: serde::de::DeserializeOwned>(&self, args: &[&str], field: &str, timeout: Duration) -> Result<T, HerdrError> {
        let result = self.ferrite(args, timeout)?;
        serde_json::from_value(result[field].clone()).map_err(|e| HerdrError {
            code: "failed".into(),
            message: format!("`ferrite {}` reply changed: {e}", args.join(" ")),
        })
    }

    fn thread_reply(&self, args: &[&str], timeout: Duration) -> Result<ThreadInfo, HerdrError> {
        self.ferrite_as(args, "thread", timeout)
    }

    fn threads(&self) -> Result<Vec<ThreadInfo>, HerdrError> {
        self.ferrite_as(&["thread", "list"], "threads", CALL_TIMEOUT)
    }

    /// Ferrite: every Thread is a pane with its agent in it.
    pub fn pane_list(&self) -> Result<Vec<Pane>, HerdrError> {
        Ok(self.threads()?.iter().map(ThreadInfo::pane).collect())
    }

    /// Ferrite: every Thread, its Attention as the agent's status.
    pub fn agent_list(&self) -> Result<Vec<Agent>, HerdrError> {
        Ok(self.threads()?.iter().map(ThreadInfo::agent).collect())
    }

    /// Ferrite: the Projects, each its repository's primary Space, then a
    /// Space for each linked worktree an open Thread works in, under its
    /// Project's repository.
    pub fn workspace_list(&self) -> Result<Vec<Workspace>, HerdrError> {
        let projects: Vec<ProjectInfo> = self.ferrite_as(&["project", "list"], "projects", CALL_TIMEOUT)?;
        let threads = self.threads()?;
        let space = |id: String, label: String, focused: bool, worktree: Option<WorkspaceWorktree>| Workspace {
            focused,
            pane_count: threads.iter().filter(|t| t.space() == id).count(),
            workspace_id: id,
            label,
            worktree,
        };
        let mut spaces: Vec<Workspace> = projects.iter().map(|p| space(id_text(&p.id), p.label.clone(), p.focused, p.worktree.clone())).collect();
        for thread in &threads {
            let Some(path) = thread.worktree.clone().filter(|path| !path.is_empty()) else {
                continue;
            };
            if spaces.iter().any(|w| w.workspace_id == path) {
                continue;
            }
            let repo_key = projects.iter().find(|p| id_text(&p.id) == id_text(&thread.project)).and_then(|p| p.worktree.as_ref()).map(|w| w.repo_key.clone()).unwrap_or_default();
            let worktree = WorkspaceWorktree { repo_key, checkout_path: path.clone(), is_linked_worktree: true };
            let focused = threads.iter().any(|t| t.focused && t.worktree.as_deref() == Some(path.as_str()));
            spaces.push(space(path, thread.title.clone().unwrap_or_default(), focused, Some(worktree)));
        }
        Ok(spaces)
    }

    /// True when the pane's shell is its foreground process: nothing runs in
    /// it. Unknown (no shell pid, an error) counts as busy.
    ///
    /// Ferrite: a Thread is never a shell; only a staged pane, where nothing
    /// has started yet, is one at its prompt. Nothing is asked.
    pub fn pane_idle_shell(&self, pane: &str) -> bool {
        staged(pane)
    }

    /// Ferrite: opens the Project at `cwd` (registered when there is none)
    /// and stages its first Thread there; `label` is the Project's name in
    /// the nav.
    pub fn workspace_create(&self, cwd: &Path, label: &str, focus: bool) -> Result<Created, HerdrError> {
        let dir = cwd.to_string_lossy();
        let mut args = vec!["project", "open", "--cwd", &dir, "--label", label];
        if focus {
            args.push("--focus");
        }
        let result = self.ferrite(&args, CALL_TIMEOUT)?;
        let project = id_text(&result["project"]["id"]);
        if project.is_empty() {
            return refused("failed", "Ferrite's project reply has no id");
        }
        Ok(Staged { space: project, focus, cwd: dir.into_owned() }.created())
    }

    /// Ferrite: stages a Thread in the Project; nothing runs until its agent
    /// starts. `label` is its tab's: the Thread's title is its agent's name.
    pub fn tab_create(&self, workspace: &str, cwd: &Path, _label: &str, focus: bool) -> Result<Created, HerdrError> {
        Ok(Staged { space: workspace.into(), focus, cwd: cwd.to_string_lossy().into_owned() }.created())
    }

    /// Creates a worktree-backed workspace. Returns the ids and the checkout
    /// path as herdr reports it (on the machine the call ran on).
    ///
    /// Ferrite: the worktree is made now, before its agent, so the brief can
    /// be written into it (`git worktree add`, over ssh on a machine); its
    /// Thread is staged there and opens it as it is at `agent_start`.
    pub fn worktree_create(&self, repo: &str, branch: &str, base: &str, _label: &str) -> Result<(Created, String, String), HerdrError> {
        let timeout = Duration::from_secs(20);
        let path = match &self.machine {
            None => {
                let store = self.socket.parent().unwrap_or(Path::new("/"));
                let path = worktree_dir(store, repo, branch).to_string_lossy().into_owned();
                let cmd = Cmd::new("git", timeout).args(["-C", repo, "worktree", "add", "-b", branch, &path, base]);
                git_ran(self.runner.run(&cmd), "worktree add")?;
                path
            }
            Some(machine) => {
                let target = crate::remote::saved_machines(self.runner, &self.bin)
                    .into_iter()
                    .find(|(label, _)| label == machine)
                    .map(|(_, target)| target)
                    .unwrap_or_else(|| machine.clone());
                let relative = worktree_dir(Path::new(".ferrite/threads"), repo, branch).to_string_lossy().into_owned();
                let q = crate::remote::quote;
                let script = format!(
                    "cd {repo} && p=\"$HOME\"/{relative} && git worktree add -b {branch} \"$p\" {base} >&2 && printf '%s' \"$p\"",
                    repo = q(repo),
                    relative = q(&relative),
                    branch = q(branch),
                    base = q(base),
                );
                let out = git_ran(crate::remote::ssh(self.runner, &target, &script, None, timeout), "worktree add")?;
                out.stdout.trim().to_string()
            }
        };
        if path.is_empty() {
            return refused("failed", "`git worktree add` named no path");
        }
        let created = Staged { space: path.clone(), focus: false, cwd: path.clone() }.created();
        Ok((created, path.clone(), path))
    }

    /// `--cwd <repo>` is required: without it herdr answers `worktree_not_found`
    /// even for a path git lists (checked on 0.9.1).
    ///
    /// Ferrite: an existing worktree needs nothing made: its Thread is staged
    /// in it.
    pub fn worktree_open(&self, _repo: &str, path: &str, _label: &str) -> Result<(Created, String, String), HerdrError> {
        let created = Staged { space: path.into(), focus: false, cwd: path.into() }.created();
        Ok((created, path.into(), path.into()))
    }

    /// Never passes `--force`: herdr refuses a dirty worktree and that refusal
    /// is reported unchanged.
    ///
    /// Ferrite: a worktree's Space is its path; the Server removes the
    /// worktree of the Thread working in it (`worktree.remove`, never
    /// forced: `dirty_worktree_requires_force`).
    pub fn worktree_remove(&self, workspace: &str) -> Result<(), HerdrError> {
        let threads = self.threads()?;
        let Some(thread) = threads.iter().find(|t| t.worktree.as_deref() == Some(workspace)) else {
            return refused("worktree_not_found", format!("no open Thread works in the worktree {workspace}"));
        };
        self.ferrite(&["worktree", "remove", "--thread", &thread.id()], Duration::from_secs(20)).map(|_| ())
    }

    /// Ferrite: a Project's Space closes with the Project (its Threads
    /// parked); a linked worktree's Space with the Threads in it.
    fn space_close(&self, space: &str, timeout: Duration) -> Result<Value, HerdrError> {
        if !Path::new(space).is_absolute() {
            return self.ferrite(&["project", "close", "--project", space], timeout);
        }
        for thread in self.threads()?.iter().filter(|t| t.worktree.as_deref() == Some(space)) {
            self.ferrite(&["thread", "close", "--thread", &thread.id()], timeout)?;
        }
        Ok(Value::Null)
    }

    /// A workspace's label, as the sidebar shows it.
    ///
    /// Ferrite: a worktree's Space is labelled by its Thread's title.
    pub fn workspace_label(&self, workspace: &str) -> Result<String, HerdrError> {
        if Path::new(workspace).is_absolute() {
            let threads = self.threads()?;
            return Ok(threads.iter().find(|t| t.worktree.as_deref() == Some(workspace)).and_then(|t| t.title.clone()).unwrap_or_default());
        }
        let result = self.ferrite(&["project", "get", "--project", workspace], CALL_TIMEOUT)?;
        Ok(result["project"]["label"].as_str().unwrap_or_default().to_string())
    }

    /// Ferrite: a worktree's Space has no label of its own to rename.
    pub fn workspace_rename(&self, workspace: &str, label: &str) -> Result<(), HerdrError> {
        if Path::new(workspace).is_absolute() {
            return Ok(());
        }
        self.ferrite(&["project", "rename", "--project", workspace, "--label", label], CALL_TIMEOUT).map(|_| ())
    }

    /// The working directory herdr reports for a new tab's pane.
    pub fn pane_cwd(&self, pane: &str) -> Result<String, HerdrError> {
        if let Some(staged) = Staged::parse(pane) {
            return Ok(staged.cwd);
        }
        Ok(self.thread_reply(&["thread", "get", "--thread", pane], CALL_TIMEOUT)?.cwd.unwrap_or_default())
    }

    /// Starts an agent in a pane that is at a shell prompt. Success means herdr
    /// detected the agent and it is ready for input.
    ///
    /// Ferrite: starts the Thread staged in `pane`, titled with the agent's
    /// name, its CLI given `agent_args` (a resume among them), and answers
    /// once it is ready (`agent_not_ready` otherwise; the Thread stays). A
    /// start whose answer was lost left its Thread running: that one is the
    /// answer, never a second.
    pub fn agent_start(&self, name: &str, kind: &str, pane: &str, agent_args: &[String]) -> Result<Agent, HerdrError> {
        let Some(staged) = Staged::parse(pane) else {
            return refused("agent_pane_occupied", format!("Thread {pane} already runs its agent: a Thread is born with it"));
        };
        let provider = provider(kind)?;
        if let Some(running) = self.threads()?.iter().find(|t| t.title.as_deref() == Some(name) && t.cwd.as_deref() == Some(staged.cwd.as_str())) {
            return Ok(running.agent());
        }
        let timeout_ms = AGENT_START_TIMEOUT.as_millis().to_string();
        let mut args = vec!["thread", "start", "--cwd", &staged.cwd, "--provider", provider, "--title", name, "--ready", "--timeout-ms", &timeout_ms];
        // The Server finds the Thread's Project by its directory: `--project`
        // would move a Thread staged in a Project's subdirectory (a tab
        // thread's folder) to the Project's own directory.
        if staged.in_worktree() {
            args.push("--worktree");
        }
        if staged.focus {
            args.push("--focus");
        }
        for arg in agent_args {
            args.extend(["--args", arg.as_str()]);
        }
        // The Server enforces the timeout itself; the outer deadline only guards a hang.
        Ok(self.thread_reply(&args, AGENT_START_TIMEOUT + Duration::from_secs(5))?.agent())
    }

    /// Submits a prompt. herdr's parser takes positionals first and options
    /// after them, and has no `--` separator here; text in the second
    /// position is accepted even when it starts with a dash (checked on 0.9.1).
    ///
    /// Ferrite: `--text` takes the next word whole, a dash first or not.
    pub fn agent_prompt(&self, target: &str, text: &str) -> Result<(), HerdrError> {
        self.ferrite(&["agent", "prompt", "--thread", target, "--text", text], CALL_TIMEOUT).map(|_| ())
    }

    /// Submits a prompt and waits until the agent is seen `working` (or
    /// `blocked`, a question it asked): herdr's own proof that the text was
    /// taken, not only typed. `agent_prompt_stalled` (herdr saw neither within
    /// its 5 s), a timeout or a reply without a result mean the text may or may
    /// not sit in the input box: callers never type it again blindly.
    pub fn agent_prompt_confirmed(&self, target: &str, text: &str) -> Result<(), HerdrError> {
        let timeout_ms = PROMPT_WAIT.as_millis().to_string();
        let args = ["agent", "prompt", "--thread", target, "--text", text, "--until", "working", "--until", "blocked", "--timeout-ms", &timeout_ms];
        match self.ferrite(&args, PROMPT_WAIT + Duration::from_secs(5))? {
            Value::Null => Err(HerdrError { code: "no_reply".into(), message: "`ferrite agent prompt --until` answered without a result".into() }),
            _ => Ok(()),
        }
    }

    /// The agent's screen as plain text: what is visible now, or the last
    /// `lines` lines of scrollback.
    ///
    /// Ferrite: the tail of the Thread's transcript; "visible" is the API's
    /// default length.
    pub fn agent_read(&self, target: &str, lines: Option<usize>) -> Result<String, HerdrError> {
        let lines = lines.map(|n| n.to_string());
        let mut args = vec!["agent", "read", "--thread", target];
        if let Some(n) = &lines {
            args.extend(["--lines", n.as_str()]);
        }
        Ok(self.ferrite(&args, CALL_TIMEOUT)?["text"].as_str().unwrap_or_default().to_string())
    }

    /// The visible screen with its styling (SGR escapes), for telling typed
    /// text from a dim placeholder (`prompt_box`).
    ///
    /// Ferrite: what that screen is read for, exactly: the Thread's draft and
    /// waiting prompts, its pending Decisions, its transcript.
    pub fn agent_screen(&self, target: &str) -> Result<PaneState, HerdrError> {
        let thread = self.thread_reply(&["thread", "get", "--thread", target], CALL_TIMEOUT)?;
        let decisions: Vec<Value> = self.ferrite_as(&["decision", "list", "--thread", target], "decisions", CALL_TIMEOUT)?;
        Ok(PaneState {
            draft: thread.draft,
            waiting_prompts: thread.held_prompt || thread.queued_prompts > 0,
            decisions: decisions.iter().map(|pending| decision_text(&pending["decision"])).collect(),
            transcript: self.agent_read(target, None)?,
        })
    }

    /// Key presses in herdr's key syntax (`enter`, `esc`, `up`, `tab`,
    /// `ctrl+c`, a single character). herdr checks every key before it sends
    /// any, and refuses a pane that no longer hosts the agent.
    ///
    /// Ferrite: there is no terminal to press keys in, so each press becomes
    /// what it meant: Escape or Ctrl+C interrupts the agent, and a menu key
    /// answers the Decision pending on the Thread. Anything else is refused
    /// (`no_decision`, `invalid_answer`) before anything is sent.
    pub fn agent_send_keys(&self, target: &str, keys: &[String]) -> Result<(), HerdrError> {
        if matches!(keys, [key] if key == "esc" || key == "ctrl+c") {
            return self.ferrite(&["agent", "interrupt", "--thread", target], CALL_TIMEOUT).map(|_| ());
        }
        let pending: Vec<Value> = self.ferrite_as(&["decision", "list", "--thread", target], "decisions", CALL_TIMEOUT)?;
        let Some(decision) = pending.first().map(|p| &p["decision"]) else {
            return refused("no_decision", format!("nothing on Thread {target} waits for keys ({}): Ferrite sends a prompt whole, and only a Decision is answered with a menu key", keys.join(" ")));
        };
        let Some(answer) = key_answer(decision, keys) else {
            return refused("invalid_answer", format!("`{}` answers nothing in Thread {target}'s Decision: {}", keys.join(" "), decision_text(decision)));
        };
        let id = decision["id"].as_str().unwrap_or_default();
        self.ferrite(&["decision", "answer", "--thread", target, "--decision", id, "--answer", &answer.to_string()], CALL_TIMEOUT).map(|_| ())
    }

    /// Literal text typed into a pane, with no Enter (herdr has no agent-level
    /// form of this in 0.9.1).
    ///
    /// Ferrite: there is no input box to type into: a prompt is sent whole
    /// (`thread prompt`), a Decision answered with keys.
    pub fn pane_send_text(&self, pane: &str, _text: &str) -> Result<(), HerdrError> {
        not_wired(&format!("pane send-text {pane}"), "a Thread has no input box to type into; send a prompt, or answer its Decision with keys")
    }

    pub fn agent_focus(&self, target: &str) -> Result<(), HerdrError> {
        self.ferrite(&["thread", "focus", "--thread", target], CALL_TIMEOUT).map(|_| ())
    }

    /// Names an already detected agent (after Herdr's native resume leaves a
    /// restored pane unnamed).
    pub fn agent_rename(&self, pane: &str, name: &str) -> Result<(), HerdrError> {
        self.ferrite(&["thread", "rename", "--thread", pane, "--title", name], CALL_TIMEOUT).map(|_| ())
    }

    pub fn notification_show(&self, title: &str, body: &str) -> Result<(), HerdrError> {
        self.ferrite(&["notification", "show", "--title", title, "--body", body], CALL_TIMEOUT).map(|_| ())
    }

    /// Ferrite: the Ferrite Project a herdr-projects project lives in: the
    /// one its coordinator works in, by the coordinator's name.
    fn project_of(&self, slug: &str) -> Result<String, HerdrError> {
        let names: Vec<String> = (1..=9).map(|n| crate::names::coordinator(slug, n)).collect();
        self.threads()?
            .iter()
            .find(|t| t.title.as_ref().is_some_and(|title| names.contains(title)))
            .map(|t| id_text(&t.project))
            .filter(|project| !project.is_empty())
            .map_or_else(|| refused("workspace_not_found", format!("`{slug}` has no coordinator open in Ferrite; `open {slug}` starts one")), Ok)
    }
}

pub const SOURCE: &str = "herdr-projects";

impl<'a> Herdr<'a> {
    /// Installs an agent view (socket only in 0.9.1). One view is active
    /// globally, so this replaces any other tool's view.
    ///
    /// Ferrite: the view `focus` and `unfocus` install is the nav's focus
    /// (`nav.focus`): one project's Threads, or every Thread. Ferrite's nav
    /// sorts by need itself.
    pub fn agent_view_set(&self, params: Value) -> Result<(), HerdrError> {
        let project = match params["filter"]["value"].as_str() {
            Some(slug) => Some(self.project_of(slug)?),
            None => None,
        };
        let mut args = vec!["nav", "focus"];
        if let Some(project) = &project {
            args.extend(["--project", project.as_str()]);
        }
        self.ferrite(&args, CALL_TIMEOUT).map(|_| ())
    }

    /// Moves a Space to `insert_index` (a gap in the list before the move;
    /// socket only in 0.9.1).
    ///
    /// Ferrite: the nav orders Projects itself.
    pub fn workspace_move(&self, _workspace_id: &str, _insert_index: usize) -> Result<(), HerdrError> {
        Ok(())
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_box::{self, Draft};
    use crate::runner::fake::{FakeRunner, fail, ok};

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("herdr 0.9.0\n"), Some(Version(0, 9, 0)));
        assert_eq!(parse_version("herdr 0.9.2-preview.3"), Some(Version(0, 9, 2)));
        assert_eq!(parse_version("0.10.0"), Some(Version(0, 10, 0)));
        assert_eq!(parse_version("herdr"), None);
        // Ferrite: Ferrite's own version line and minimum.
        assert_eq!(parse_version("ferrite 0.4.1\n"), Some(Version(0, 4, 1)));
        assert!(Version(0, 3, 9) < MIN_VERSION);
        assert!(Version(0, 5, 0) > MIN_VERSION);
    }

    fn herdr(runner: &FakeRunner) -> Herdr<'_> {
        Herdr::new("ferrite", "/store/api.sock", runner)
    }

    fn last(runner: &FakeRunner, needle: &str) -> Vec<String> {
        runner.calls.borrow().iter().rev().find(|c| c.display().contains(needle)).map(|c| c.args.clone()).unwrap()
    }

    const THREADS: &str = r#"{"id":"1","result":{"threads":[
        {"thread":12,"title":"hp-demo-t-0001","provider":"claude","attention":"idle","cwd":"/wt","project":"3","provider_session":"s-1","attention_seq":7,"focused":true,"worktree":"/wt","held_prompt":false,"queued_prompts":0,"draft":false},
        {"thread":13,"title":"hpc-demo","provider":"codex","attention":"working","cwd":"/home/demo","project":"4","focused":false,"attention_seq":2,"held_prompt":false,"queued_prompts":0,"draft":false}]}}"#;

    #[test]
    fn threads_are_panes_and_agents_with_their_space() {
        let runner = FakeRunner::new();
        runner.on("thread list", ok(THREADS));
        let agents = herdr(&runner).agent_list().unwrap();
        assert_eq!((agents[0].pane_id.as_str(), agents[0].tab_id.as_str(), agents[0].terminal_id.as_str()), ("12", "12", "12"));
        assert_eq!(agents[0].workspace_id, "/wt", "a linked worktree is its own Space");
        assert_eq!(agents[0].name, "hp-demo-t-0001");
        assert_eq!((agents[0].agent.as_str(), agents[0].agent_status.as_str()), ("claude", "idle"));
        assert_eq!(agents[0].state_change_seq, 7);
        assert_eq!(agents[0].session_id(), "s-1");
        assert_eq!(agents[1].workspace_id, "4");
        assert_eq!(agents[1].session_id(), "");
        let panes = herdr(&runner).pane_list().unwrap();
        assert!(panes[0].focused && !panes[1].focused);
        assert_eq!(panes[1].cwd, "/home/demo");
        let cmd = &runner.calls.borrow()[0];
        assert_eq!(cmd.program, "ferrite");
        assert_eq!(cmd.env, [("FERRITE_SOCKET".to_string(), "/store/api.sock".to_string())]);
    }

    #[test]
    fn ferrite_refusals_carry_herdr_codes() {
        let runner = FakeRunner::new();
        runner.on("thread get", fail(1, r#"{"id":"1","error":{"code":"thread_not_found","message":"no open Thread has that id"}}"#));
        runner.on("thread list", fail(1, r#"{"id":"1","error":{"code":"server_unavailable","message":"no Ferrite server"}}"#));
        runner.on("project get", fail(1, r#"{"id":"1","error":{"code":"project_not_found","message":"no Project has that id"}}"#));
        assert_eq!(herdr(&runner).pane_cwd("9").unwrap_err().code, "pane_not_found");
        assert_eq!(herdr(&runner).agent_list().unwrap_err().code, "unreachable");
        assert_eq!(herdr(&runner).workspace_label("9").unwrap_err().code, "workspace_not_found");
        assert_eq!(herdr(&runner).on_machine("box").agent_list().unwrap_err().code, "unreachable");
        assert_eq!(last(&runner, "thread list")[..2], ["--machine", "box"]);
        // A call with no Ferrite meaning runs nothing.
        let before = runner.calls.borrow().len();
        assert_eq!(herdr(&runner).call(&["server", "reload-config"], CALL_TIMEOUT).unwrap_err().code, NOT_WIRED);
        assert_eq!(herdr(&runner).call(&["pane", "process-info", "--pane", "12"], CALL_TIMEOUT).unwrap_err().code, NOT_WIRED);
        assert!(!herdr(&runner).pane_idle_shell("12"), "a Thread is never a shell");
        assert_eq!(runner.calls.borrow().len(), before);
    }

    #[test]
    fn a_staged_tab_starts_its_thread_with_the_agent() {
        let runner = FakeRunner::new();
        runner.on("thread list", ok(r#"{"result":{"threads":[]}}"#));
        runner.on("thread start", ok(r#"{"result":{"thread":{"thread":21,"title":"hpc-demo","provider":"claude","attention":"idle","cwd":"/home/demo","project":"1"}}}"#));
        let created = herdr(&runner).tab_create("1", Path::new("/home/demo"), "coordinator", true).unwrap();
        assert_eq!(created.workspace_id, "1");
        assert!(staged(&created.pane_id) && created.tab_id == created.pane_id);
        assert!(herdr(&runner).pane_idle_shell(&created.pane_id), "a staged pane waits at its prompt");
        assert_eq!(herdr(&runner).pane_cwd(&created.pane_id).unwrap(), "/home/demo");
        assert!(runner.calls.borrow().is_empty(), "staging runs nothing");

        let args = vec!["--model".to_string(), "opus".to_string()];
        let agent = herdr(&runner).agent_start("hpc-demo", "claude", &created.pane_id, &args).unwrap();
        assert_eq!((agent.pane_id.as_str(), agent.workspace_id.as_str()), ("21", "1"));
        assert_eq!(
            last(&runner, "thread start"),
            ["thread", "start", "--cwd", "/home/demo", "--provider", "claude", "--title", "hpc-demo", "--ready", "--timeout-ms", "20000", "--focus", "--args", "--model", "--args", "opus"]
        );
        // Closing or renaming a staged pane has nothing to act on.
        herdr(&runner).call(&["tab", "close", &created.tab_id], CALL_TIMEOUT).unwrap();
        herdr(&runner).call(&["tab", "rename", &created.tab_id, "coordinator"], CALL_TIMEOUT).unwrap();
        assert_eq!(runner.count("thread close") + runner.count("thread rename"), 0);
        // A started one is a Thread.
        runner.on("thread close", ok(r#"{"result":{"thread":21}}"#));
        herdr(&runner).call(&["pane", "close", "21"], CALL_TIMEOUT).unwrap();
        assert_eq!(last(&runner, "thread close"), ["thread", "close", "--thread", "21"]);
    }

    #[test]
    fn a_start_whose_answer_was_lost_is_not_started_twice() {
        let runner = FakeRunner::new();
        runner.on("thread list", ok(r#"{"result":{"threads":[{"thread":21,"title":"hp-demo-t-0001","provider":"claude","attention":"working","cwd":"/wt","worktree":"/wt"}]}}"#));
        let pane = Staged { space: "/wt".into(), focus: false, cwd: "/wt".into() }.id();
        let agent = herdr(&runner).agent_start("hp-demo-t-0001", "claude", &pane, &[]).unwrap();
        assert_eq!(agent.pane_id, "21");
        assert_eq!(runner.count("thread start"), 0);
        // A kind Ferrite has no Provider for fails, as a missing binary does.
        assert_eq!(herdr(&runner).agent_start("x", "gemini", &pane, &[]).unwrap_err().code, "no_provider");
        // A Thread is born with its agent: there is no second start in it.
        assert_eq!(herdr(&runner).agent_start("x", "claude", "21", &[]).unwrap_err().code, "agent_pane_occupied");
        // A worktree's Thread opens the worktree as it is.
        runner.on("thread start", fail(1, r#"{"error":{"code":"agent_not_ready","message":"Thread 22 started, but its agent is blocked","thread":22}}"#));
        assert_eq!(herdr(&runner).agent_start("hp-demo-t-0002", "codex", &pane, &[]).unwrap_err().code, "agent_not_ready");
        assert_eq!(
            last(&runner, "thread start"),
            ["thread", "start", "--cwd", "/wt", "--provider", "codex", "--title", "hp-demo-t-0002", "--ready", "--timeout-ms", "20000", "--worktree"]
        );
    }

    #[test]
    fn a_worktree_is_made_before_its_thread_and_removed_through_it() {
        let runner = FakeRunner::new();
        runner.on("worktree add", ok(""));
        let (created, path, cwd) = herdr(&runner).worktree_create("/r/app", "hp/demo/t-0001", "origin/main", "Task").unwrap();
        assert_eq!(path, "/store/worktrees/app/hp-demo-t-0001");
        assert_eq!(cwd, path);
        assert_eq!(created.workspace_id, path);
        assert_eq!(Staged::parse(&created.pane_id).unwrap().cwd, path);
        let add = runner.calls.borrow()[0].clone();
        assert_eq!(add.program, "git");
        assert_eq!(add.args, ["-C", "/r/app", "worktree", "add", "-b", "hp/demo/t-0001", "/store/worktrees/app/hp-demo-t-0001", "origin/main"]);

        let broken = FakeRunner::new();
        broken.on("worktree add", fail(128, "fatal: a branch named 'hp/demo/t-0001' already exists"));
        let error = herdr(&broken).worktree_create("/r/app", "hp/demo/t-0001", "origin/main", "Task").unwrap_err();
        assert!(error.message.contains("already exists"), "{error}");

        runner.on("thread list", ok(THREADS));
        runner.on("worktree remove", fail(1, r#"{"error":{"code":"dirty_worktree_requires_force","message":"/wt has modified or untracked files; nothing was removed"}}"#));
        let error = herdr(&runner).worktree_remove("/wt").unwrap_err();
        assert_eq!(error.code, "dirty_worktree_requires_force");
        assert_eq!(last(&runner, "worktree remove"), ["worktree", "remove", "--thread", "12"]);
        assert!(!runner.calls.borrow().iter().any(|c| c.args.contains(&"--force".to_string())));
        assert_eq!(herdr(&runner).worktree_remove("/elsewhere").unwrap_err().code, "worktree_not_found");
    }

    #[test]
    fn a_worktree_on_a_machine_is_made_there() {
        let runner = FakeRunner::new();
        runner.on("machine list", ok(r#"{"id":"1","result":{"machines":[{"id":"box","label":"box","target":"me@box"}]}}"#));
        runner.on("ssh", ok("/home/me/.ferrite/threads/worktrees/app/hp-demo-t-0001"));
        let (created, path, _) = herdr(&runner).on_machine("box").worktree_create("/home/me/app", "hp/demo/t-0001", "main", "Task").unwrap();
        assert_eq!(path, "/home/me/.ferrite/threads/worktrees/app/hp-demo-t-0001");
        assert_eq!(created.workspace_id, path);
        let ssh = last(&runner, "ssh");
        assert!(ssh.contains(&"me@box".to_string()), "{ssh:?}");
        assert!(ssh.last().unwrap().contains("git worktree add -b hp/demo/t-0001"), "{ssh:?}");
    }

    #[test]
    fn a_worktree_space_closes_with_its_threads_and_a_project_space_with_the_project() {
        let runner = FakeRunner::new();
        runner.on("thread list", ok(THREADS));
        runner.on("thread close", ok(r#"{"result":{"thread":12}}"#));
        runner.on("project close", ok(r#"{"result":{"project":{"id":"4"},"parked":[13]}}"#));
        herdr(&runner).call(&["workspace", "close", "/wt"], CALL_TIMEOUT).unwrap();
        assert_eq!(last(&runner, "thread close"), ["thread", "close", "--thread", "12"]);
        assert_eq!(runner.count("thread close"), 1);
        herdr(&runner).call(&["workspace", "close", "4"], CALL_TIMEOUT).unwrap();
        assert_eq!(last(&runner, "project close"), ["project", "close", "--project", "4"]);
    }

    #[test]
    fn prompts_are_confirmed_by_the_server() {
        let runner = FakeRunner::new();
        runner.on("agent prompt", ok(r#"{"result":{"thread":{"thread":12}}}"#));
        herdr(&runner).agent_prompt_confirmed("12", "-x read the brief").unwrap();
        assert_eq!(
            last(&runner, "agent prompt"),
            ["agent", "prompt", "--thread", "12", "--text", "-x read the brief", "--until", "working", "--until", "blocked", "--timeout-ms", "8000"]
        );
        herdr(&runner).agent_prompt("12", "--help me").unwrap();
        assert_eq!(last(&runner, "agent prompt"), ["agent", "prompt", "--thread", "12", "--text", "--help me"]);
        let stalled = FakeRunner::new();
        stalled.on("agent prompt", fail(1, r#"{"error":{"code":"agent_prompt_stalled","message":"not seen working"}}"#));
        let error = herdr(&stalled).agent_prompt_confirmed("12", "x").unwrap_err();
        assert_eq!(error.code, "agent_prompt_stalled");
        assert!(!refused_before_typing(&error), "a stalled prompt may have arrived");
        for code in ["agent_blocked", "agent_starting", "pane_not_found", "unreachable"] {
            assert!(refused_before_typing(&HerdrError { code: code.into(), message: String::new() }), "{code}");
        }
        let blocked = FakeRunner::new();
        blocked.on("agent prompt", fail(1, r#"{"error":{"code":"agent_blocked","message":"a Decision waits on this Thread; nothing was sent"}}"#));
        assert!(refused_before_typing(&herdr(&blocked).agent_prompt_confirmed("12", "x").unwrap_err()));
    }

    #[test]
    fn reads_are_the_transcripts_tail() {
        let runner = FakeRunner::new();
        runner.on("agent read", ok(r#"{"result":{"thread":12,"text":"line one\nline two"}}"#));
        assert_eq!(herdr(&runner).agent_read("12", Some(200)).unwrap(), "line one\nline two");
        assert_eq!(last(&runner, "agent read"), ["agent", "read", "--thread", "12", "--lines", "200"]);
        herdr(&runner).agent_read("12", None).unwrap();
        assert_eq!(last(&runner, "agent read"), ["agent", "read", "--thread", "12"]);
    }

    fn state_runner(thread: &str, decisions: &str) -> FakeRunner {
        let runner = FakeRunner::new();
        runner.on("thread get", ok(&format!(r#"{{"result":{{"thread":{thread}}}}}"#)));
        runner.on("decision list", ok(&format!(r#"{{"result":{{"decisions":{decisions}}}}}"#)));
        runner.on("agent read", ok(r#"{"result":{"thread":12,"text":"Read .herdr-project/demo-t-0001/brief.md and do what it says."}}"#));
        runner.on("decision answer", ok(r#"{"result":{}}"#));
        runner.on("agent interrupt", ok(r#"{"result":{}}"#));
        runner
    }

    const IDLE: &str = r#"{"thread":12,"provider":"claude","attention":"idle"}"#;

    #[test]
    fn the_input_box_is_read_from_the_threads_exact_state() {
        let empty = state_runner(IDLE, "[]");
        let screen = herdr(&empty).agent_screen("12").unwrap();
        assert_eq!(prompt_box::check("claude", &screen), Draft::Empty);
        assert!(prompt_box::plain(&screen).contains("demo-t-0001/brief.md"));
        let draft = state_runner(r#"{"thread":12,"provider":"codex","attention":"idle","draft":true}"#, "[]");
        assert_eq!(prompt_box::check("codex", &herdr(&draft).agent_screen("12").unwrap()), Draft::Typed);
        for waiting in [r#""held_prompt":true"#, r#""queued_prompts":2"#] {
            let held = state_runner(&format!(r#"{{"thread":12,"provider":"claude","attention":"working",{waiting}}}"#), "[]");
            assert_eq!(prompt_box::check("claude", &herdr(&held).agent_screen("12").unwrap()), Draft::Unknown, "{waiting}");
        }
    }

    const TRUST: &str = r#"[{"thread":12,"handle":{},"decision":{"id":"d1","kind":{"kind":"approval"},"description":"Do you trust the files in this folder?","tool_name":"","input":{}}}]"#;
    const PERMISSION: &str = r#"[{"thread":12,"handle":{},"decision":{"id":"perm_01","kind":{"kind":"approval"},"description":"Run git push","tool_name":"Bash","input":{"command":"git push"},"suggestions":[{"label":"always","value":{"rule":"Bash(git push)"},"standing":true}]}}]"#;

    #[test]
    fn a_trust_screen_is_a_pending_decision_that_asks_for_trust() {
        let trust = state_runner(IDLE, TRUST);
        let screen = herdr(&trust).agent_screen("12").unwrap();
        assert_eq!(crate::trust_screen::detect("claude", &screen), Some("Do you trust the files in this folder?"));
        // An ordinary permission Decision is a question, not a trust screen,
        // and a transcript quoting the dialog is only text.
        let permission = state_runner(IDLE, PERMISSION);
        assert_eq!(crate::trust_screen::detect("claude", &herdr(&permission).agent_screen("12").unwrap()), None);
        let quoting = state_runner(IDLE, "[]");
        quoting.on("agent read", ok(r#"{"result":{"text":"it said Do you trust the files in this folder?"}}"#));
        assert_eq!(crate::trust_screen::detect("claude", &herdr(&quoting).agent_screen("12").unwrap()), None);
    }

    fn keys(text: &[&str]) -> Vec<String> {
        text.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn keys_become_what_they_meant() {
        let runner = state_runner(IDLE, PERMISSION);
        herdr(&runner).agent_send_keys("12", &keys(&["esc"])).unwrap();
        assert_eq!(last(&runner, "agent interrupt"), ["agent", "interrupt", "--thread", "12"]);
        herdr(&runner).agent_send_keys("12", &keys(&["ctrl+c"])).unwrap();
        assert_eq!(runner.count("agent interrupt"), 2);
        herdr(&runner).agent_send_keys("12", &keys(&["1"])).unwrap();
        let answer = last(&runner, "decision answer");
        assert_eq!(answer[..7], ["decision", "answer", "--thread", "12", "--decision", "perm_01", "--answer"]);
        assert_eq!(serde_json::from_str::<Value>(&answer[7]).unwrap(), json!({ "kind": "allow", "input": { "command": "git push" } }));
        herdr(&runner).agent_send_keys("12", &keys(&["2", "enter"])).unwrap();
        let always = serde_json::from_str::<Value>(&last(&runner, "decision answer")[7]).unwrap();
        assert_eq!((always["kind"].as_str(), &always["suggestion"]), (Some("allow_always"), &json!({ "rule": "Bash(git push)" })));
        herdr(&runner).agent_send_keys("12", &keys(&["3"])).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&last(&runner, "decision answer")[7]).unwrap()["kind"], "deny");
        let answered = runner.count("decision answer");
        assert_eq!(herdr(&runner).agent_send_keys("12", &keys(&["up", "up"])).unwrap_err().code, "invalid_answer");
        assert_eq!(runner.count("decision answer"), answered, "refused before anything is sent");

        let question = state_runner(IDLE, r#"[{"thread":12,"decision":{"id":"q1","kind":{"kind":"questions","questions":[{"question":"Which?","header":"","options":[{"label":"a","description":""},{"label":"b","description":""}],"multi_select":false,"secret":false,"allow_other":false}]},"input":{}}}]"#);
        herdr(&question).agent_send_keys("12", &keys(&["2"])).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&last(&question, "decision answer")[7]).unwrap(), json!({ "kind": "questions", "answers": [{ "picks": [1] }] }));
        assert_eq!(herdr(&question).agent_send_keys("12", &keys(&["3"])).unwrap_err().code, "invalid_answer");

        let none = state_runner(IDLE, "[]");
        assert_eq!(herdr(&none).agent_send_keys("12", &keys(&["enter"])).unwrap_err().code, "no_decision");
        assert_eq!(none.count("decision answer"), 0);
        let before = none.calls.borrow().len();
        assert_eq!(herdr(&none).pane_send_text("12", "hello").unwrap_err().code, NOT_WIRED);
        assert_eq!(none.calls.borrow().len(), before);
    }

    #[test]
    fn sidebar_pushes_and_space_moves_are_silent() {
        let runner = FakeRunner::new();
        herdr(&runner).call(&["pane", "report-metadata", "12", "--source", SOURCE, "--token", "hp_rank=1"], CALL_TIMEOUT).unwrap();
        herdr(&runner).call(&["workspace", "report-metadata", "4", "--source", SOURCE], CALL_TIMEOUT).unwrap();
        herdr(&runner).workspace_move("4", 0).unwrap();
        assert!(runner.calls.borrow().is_empty());
    }

    #[test]
    fn notifications_views_and_the_current_pane() {
        let runner = FakeRunner::new();
        runner.on("thread list", ok(THREADS));
        runner.on("notification show", ok(r#"{"result":{"windows":1}}"#));
        runner.on("nav focus", ok(r#"{"result":{"windows":1}}"#));
        runner.on("thread get --current", ok(r#"{"result":{"thread":{"thread":12,"provider":"claude","attention":"working","provider_session":"s-9"}}}"#));
        herdr(&runner).call(&["notification", "show", "Demo · t-0001", "--body", "needs you", "--sound", "request"], CALL_TIMEOUT).unwrap();
        assert_eq!(last(&runner, "notification"), ["notification", "show", "--title", "Demo · t-0001", "--body", "needs you", "--sound", "request", "--important"]);
        herdr(&runner).call(&["notification", "show", "Demo", "--body", "done", "--sound", "done"], CALL_TIMEOUT).unwrap();
        assert!(!last(&runner, "notification").contains(&"--important".to_string()));
        // `focus demo` shows the Project its coordinator works in.
        herdr(&runner).agent_view_set(crate::sidebar::project_view("demo")).unwrap();
        assert_eq!(last(&runner, "nav focus"), ["nav", "focus", "--project", "4"]);
        herdr(&runner).agent_view_set(crate::sidebar::default_view()).unwrap();
        assert_eq!(last(&runner, "nav focus"), ["nav", "focus"]);
        assert_eq!(herdr(&runner).agent_view_set(crate::sidebar::project_view("other")).unwrap_err().code, "workspace_not_found");
        let current = herdr(&runner).call(&["pane", "current", "--current"], CALL_TIMEOUT).unwrap();
        assert_eq!((current["pane"]["pane_id"].as_str(), current["pane"]["terminal_id"].as_str()), (Some("12"), Some("12")));
        assert_eq!((current["pane"]["agent"].as_str(), current["pane"]["agent_session"]["value"].as_str()), (Some("claude"), Some("s-9")));
    }

    #[test]
    fn projects_and_worktrees_are_spaces() {
        let runner = FakeRunner::new();
        runner.on("thread list", ok(THREADS));
        runner.on("project list", ok(r#"{"result":{"projects":[
            {"id":"3","label":"app","focused":true,"thread_count":1,"worktree":{"repo_key":"/r/app","checkout_path":"/r/app","is_linked_worktree":false}},
            {"id":"4","label":"Demo ⌂","focused":false,"thread_count":1,"worktree":null}]}}"#));
        runner.on("project open", ok(r#"{"result":{"project":{"id":"7","label":"Demo","focused":false,"thread_count":0,"worktree":null}}}"#));
        runner.on("project get", ok(r#"{"result":{"project":{"id":"4","label":"Demo ⌂"}}}"#));
        runner.on("project rename", ok(r#"{"result":{}}"#));
        let spaces = herdr(&runner).workspace_list().unwrap();
        let ids: Vec<&str> = spaces.iter().map(|w| w.workspace_id.as_str()).collect();
        assert_eq!(ids, ["3", "4", "/wt"]);
        assert_eq!(crate::spaces::primary(&spaces, "/r/app").map(|w| w.workspace_id.as_str()), Some("3"));
        let app = &spaces[0];
        assert_eq!((app.pane_count, app.focused), (0, true), "its one Thread, focused, is in its worktree's Space");
        let wt = &spaces[2];
        assert_eq!((wt.label.as_str(), wt.pane_count, wt.focused), ("hp-demo-t-0001", 1, true));
        assert_eq!(wt.worktree, Some(WorkspaceWorktree { repo_key: "/r/app".into(), checkout_path: "/wt".into(), is_linked_worktree: true }));
        assert_eq!(spaces[1].pane_count, 1);

        let created = herdr(&runner).workspace_create(Path::new("/home/demo"), "Demo", false).unwrap();
        assert_eq!(created.workspace_id, "7");
        assert_eq!(Staged::parse(&created.pane_id), Some(Staged { space: "7".into(), focus: false, cwd: "/home/demo".into() }));
        assert_eq!(last(&runner, "project open"), ["project", "open", "--cwd", "/home/demo", "--label", "Demo"]);
        assert_eq!(herdr(&runner).workspace_label("4").unwrap(), "Demo ⌂");
        assert_eq!(herdr(&runner).workspace_label("/wt").unwrap(), "hp-demo-t-0001");
        herdr(&runner).workspace_rename("4", "-Demo").unwrap();
        assert_eq!(last(&runner, "project rename"), ["project", "rename", "--project", "4", "--label", "-Demo"]);
        herdr(&runner).workspace_rename("/wt", "x").unwrap();
        assert_eq!(runner.count("project rename"), 1, "a worktree's Space has no label of its own");
    }

    #[test]
    fn the_one_session_is_the_server_at_its_socket() {
        let runner = FakeRunner::new();
        runner.on("--version", ok("ferrite 0.4.0\n"));
        assert_eq!(version("ferrite", &runner).unwrap(), Version(0, 4, 0));
        assert_eq!(last(&runner, "--version"), ["--version"]);
        assert_eq!(default_socket(Path::new("/h")), PathBuf::from("/h/.ferrite/threads/api.sock"));
        let old = FakeRunner::new();
        old.on("--version", fail(2, "ferrite: unknown command"));
        assert!(version("ferrite", &old).is_err());
    }
}
