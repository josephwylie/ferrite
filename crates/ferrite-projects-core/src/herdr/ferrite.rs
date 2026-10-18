//! Ferrite: where herdr's command line and socket meet Ferrite's Server (ADR 0031). Every herdr command this crate
//! runs (`cli::launch`) and every request it makes on herdr's socket (`socket::ask`) arrives here in herdr's own
//! words, after a test's stand-in has had its turn, and is answered from Ferrite's API (ADR 0027) in herdr's own
//! shapes, so `api.rs` reads the answers as it reads herdr's. One Server per computer: herdr's one session, `default`,
//! at the Server's API socket.
//!
//! A herdr Space is a Ferrite Project, named by its id, or a Space the board made in one (`workspace create`, by an id
//! of its own, `panes.rs`), or a linked worktree, named by its path, as herdr's worktree Spaces are. A tab, a pane and
//! an agent are one Thread. A pane the board places is kept
//! by an id of its own until its Thread starts there and after its agent ends (`panes.rs`); any other Thread is a pane
//! named by its Thread's id. Ferrite has no shells, screens or keys: a pane whose Thread is not open is at its shell,
//! a screen is the tail of its Thread's transcript, and a key or typed text is what it meant (`answers.rs`). What
//! Ferrite has no counterpart for, the terminal runtime's and herdr's own setup's calls, is refused `not_wired` before
//! anything runs, which callers treat as any refusal of herdr's.
use super::cli::{HRes, HerdrError, Ran};
use crate::json::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

mod answers;
mod panes;
mod server;
#[cfg(test)]
mod tests;

use panes::{Pane, Panes, Space};
use server::{Refused, Server};

/// The words a start of an agent kind Ferrite has no Provider for is refused with (`agents/start_failed.rs`).
pub const NO_PROVIDER: &str = "Ferrite has no Provider for";
/// A process id no system gives (above every `pid_max`): what `pane process-info` names where no process runs, so
/// nothing is ever signalled and it has no children.
const NO_PID: i64 = i32::MAX as i64;
/// What a wait on the Server leaves its answer of the request's deadline: the most, and the share.
const ANSWER_MARGIN: Duration = Duration::from_secs(5);
const ANSWER_SHARE: u32 = 5;

/// The Server's API socket: `FERRITE_SOCKET`, else `api.sock` in the Store (`FERRITE_STORE`, else
/// `~/.ferrite/threads`), as the `ferrite` CLI finds it.
pub fn socket() -> String {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    if let Some(socket) = env("FERRITE_SOCKET") { return socket; }
    let store = env("FERRITE_STORE").map(PathBuf::from).unwrap_or_else(|| crate::store::files::user_home().join(".ferrite").join("threads"));
    store.join("api.sock").display().to_string()
}

pub fn launch(socket: Option<&str>, args: &[String], _input: &str, timeout_ms: u64) -> HRes<Ran> {
    let stdout = Ferrite::at(socket, timeout_ms).answer(args)?;
    Ok(Ran { status: 0, stdout, stderr: String::new() })
}

/// herdr's socket is spoken to only for its popup and its tab events, both the terminal runtime's.
pub fn ask(method: &str, _params: &Value, _socket: Option<&str>) -> HRes<Value> {
    Err(not_wired(&format!("herdr's `{method}`")))
}

/// herdr's `pane quit`: the agent in the pane ended, leaving the pane at its shell. Its Thread is parked
/// (`thread.close`), never signalled, and its pane kept, for its agent to be started there again (`thread.revive`).
pub fn pane_quit(socket: Option<&str>, pane: &str) -> bool { Ferrite::at(socket, 10000).quit(pane).is_ok() }

/// herdr's `Herdr::pane_focus`, which zooms a pane to put the keyboard on it: every window lands on its Thread
/// (`thread.focus`). A pane at its shell has none to land on.
pub fn pane_focus(socket: Option<&str>, pane: &str) -> HRes<()> {
    Ferrite::at(socket, 5000).focus(pane, "pane").map(|_| ()).map_err(|r| herdr_error(r, "pane"))
}

fn not_wired(what: &str) -> HerdrError {
    HerdrError { code: "not_wired".into(), message: format!("Ferrite has not wired {what} to its Server yet.") }
}

/// A command's leading words ("pane get"), never its ids or the text it carries.
fn command(args: &[String]) -> String {
    let is_word = |a: &&String| !a.starts_with('-') && a.chars().all(|c| c.is_ascii_lowercase() || c == '-');
    let words: Vec<&str> = args.iter().take_while(is_word).take(3).map(String::as_str).collect();
    match words.is_empty() {
        true => args.first().cloned().unwrap_or_default(),
        false => words.join(" "),
    }
}

/// A Ferrite refusal in herdr's words: no open Thread is herdr's missing pane, tab or agent, no Project its missing
/// Space.
fn herdr_error(r: Refused, area: &str) -> HerdrError {
    let code = match (r.code.as_str(), area) {
        ("unknown_thread", "agent") => "agent_not_found",
        ("unknown_thread", "tab") => "tab_not_found",
        ("unknown_thread", _) => "pane_not_found",
        ("project_not_found", _) => "workspace_not_found",
        (code, _) => code,
    };
    HerdrError { code: code.into(), message: r.message }
}

fn failed(e: crate::err::Error) -> Refused { Refused::new("failed", e.msg) }
fn missing(what: &str, id: &str) -> Refused { Refused::new(&format!("{what}_not_found"), format!("{what} {id} not found")) }
fn result(v: Value) -> Result<String, Refused> { Ok(json!({ "id": "ferrite", "result": v }).to_string()) }
fn done() -> Result<String, Refused> { result(json!({ "type": "ok" })) }
fn is_path(space: &str) -> bool { Path::new(space).is_absolute() }
/// What herdr labels a Space after: its folder's name, the home folder `~`.
fn folder_label(cwd: &str) -> String {
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let folder = real(Path::new(cwd));
    if folder == real(&crate::store::files::user_home()) { return "~".into(); }
    folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| cwd.into())
}

/// A flag's value, before `--`.
fn flag<'a>(args: &[&'a str], name: &str) -> Option<&'a str> {
    let flags = &args[..args.iter().position(|a| *a == "--").unwrap_or(args.len())];
    flags.windows(2).find(|w| w[0] == name).map(|w| w[1])
}
fn has(args: &[&str], name: &str) -> bool { args.iter().take_while(|a| **a != "--").any(|a| *a == name) }
/// Every `--env NAME=value`.
fn envs(args: &[&str]) -> BTreeMap<String, String> {
    let flags = &args[..args.iter().position(|a| *a == "--").unwrap_or(args.len())];
    flags.windows(2).filter(|w| w[0] == "--env").filter_map(|w| w[1].split_once('=')).map(|(k, v)| (k.into(), v.into())).collect()
}

/// The Provider that runs an agent kind: Ferrite runs two. Any other kind is refused before anything starts, as a
/// program that is not there.
fn provider(kind: &str) -> Result<&'static str, Refused> {
    match kind {
        "claude" => Ok("claude"),
        "codex" => Ok("codex"),
        _ => Err(Refused::new("no_provider", format!("{NO_PROVIDER} {kind}: it runs claude and codex agents only."))),
    }
}

/// The arguments without the resume upstream adds to them (`adapter::resume_args`), and the conversation that resume
/// names: Ferrite resumes a Thread's conversation itself (`resume`).
fn split_resume(kind: &str, args: &[String]) -> (Vec<String>, Option<String>) {
    // A conversation's id is never a flag.
    let resumes = |id: &str, words: &[String]| !id.starts_with('-') && crate::agents::adapter::resume_args(kind, id) == words;
    let (mut rest, mut resume, mut i) = (Vec::new(), None, 0);
    while i < args.len() {
        if let Some(id) = args.get(i + 1).filter(|id| resumes(id, &args[i..i + 2])) {
            resume = Some(id.clone());
            i += 2;
        } else if let Some((_, id)) = args[i].split_once('=').filter(|(_, id)| resumes(id, &args[i..=i])) {
            resume = Some(id.to_string());
            i += 1;
        } else {
            rest.push(args[i].clone());
            i += 1;
        }
    }
    (rest, resume)
}

/// The words of a shell line as `util::shq` quotes them ('it'\''s') or bare.
fn shell_words(line: &str) -> Vec<String> {
    let re = regex::Regex::new(r"'((?:[^']|'\\'')*)'|(\S+)").expect("a valid pattern");
    re.captures_iter(line).map(|c| match c.get(1) { Some(q) => q.as_str().replace("'\\''", "'"), None => c[2].to_string() }).collect()
}

/// One herdr pane as Ferrite has it: an open Thread, or a pane the board placed whose Thread is not open (not started
/// yet, or its agent ended): a pane at its shell.
struct Place { id: String, space: String, cwd: String, label: String, thread: Option<Value>, pane: Option<Pane> }

impl Place {
    fn focused(&self) -> bool { self.thread.as_ref().map(|t| t.b("focused")).unwrap_or(false) }
    fn thread_id(&self) -> Option<u64> { self.thread.as_ref().and_then(|t| t.at("thread").as_u64()) }
    /// herdr's terminal id, which holds while its server runs and changes when it restarts, its `state_change_seq`
    /// starting over: its Thread's, in the Server's boot its Attention is counted in (`thread@boot`).
    fn terminal(&self) -> String {
        match &self.thread { Some(t) => format!("{}@{}", text(t.at("thread")), t.t("boot")), None => self.id.clone() }
    }
    fn pane_json(&self) -> Value {
        let t = self.thread.as_ref();
        json!({ "pane_id": self.id, "tab_id": self.id, "workspace_id": self.space, "terminal_id": self.terminal(), "cwd": self.cwd, "foreground_cwd": self.cwd,
            "focused": self.focused(), "agent": t.map(|t| t.at("provider").clone()).unwrap_or(Value::Null), "agent_status": t.map(|t| t.t("attention")).unwrap_or_else(|| "unknown".into()) })
    }
    fn tab_json(&self) -> Value {
        json!({ "tab_id": self.id, "workspace_id": self.space, "label": self.label, "pane_count": 1, "focused": self.focused() })
    }
    /// Its Thread as herdr's agent: Attention is herdr's agent status, word for word.
    fn agent_json(&self) -> Option<Value> {
        let t = self.thread.as_ref()?;
        let mut a = json!({ "pane_id": self.id, "tab_id": self.id, "workspace_id": self.space, "terminal_id": self.terminal(), "name": t.t("title"), "agent": t.at("provider"),
            "agent_status": t.at("attention"), "cwd": self.cwd, "foreground_cwd": self.cwd, "state_change_seq": t.at("attention_seq") });
        if let Some(session) = t.so("provider_session") { a["agent_session"] = json!({ "kind": "id", "value": session }); }
        // Absent until its first finished turn, as herdr's.
        if let Some(seq) = t.at("completion_seq").as_u64() { a["completion_seq"] = json!(seq); }
        Some(a)
    }
    /// herdr's `pane process-info`: a shell alone where no Thread is open, else a program in front of it.
    fn process_info(&self) -> Value {
        let front = if self.thread.is_some() { NO_PID - 1 } else { NO_PID };
        json!({ "pane_id": self.id, "shell_pid": NO_PID, "foreground_process_group_id": front, "foreground_processes": [{ "pid": front, "name": if self.thread.is_some() { "agent" } else { "shell" } }] })
    }
}

/// Threads placed by the board, including closed and replaced agents.
pub fn owned_threads() -> Vec<u64> {
    Panes { path: crate::store::files::data_dir().join("ferrite-panes.json") }
        .owned_threads().unwrap_or_default()
}

/// The Server one herdr call is answered from, and the panes the board placed.
struct Ferrite { server: Server, panes: Panes }

impl Ferrite {
    fn at(socket: Option<&str>, timeout_ms: u64) -> Self {
        let socket = socket.map(String::from).unwrap_or_else(super::cli::current_socket);
        Ferrite {
            server: Server { socket: socket.into(), timeout: Duration::from_millis(timeout_ms) },
            panes: Panes { path: crate::store::files::data_dir().join("ferrite-panes.json") },
        }
    }

    /// What herdr would have printed.
    fn answer(&self, args: &[String]) -> HRes<String> {
        let words: Vec<&str> = args.iter().map(String::as_str).collect();
        self.dispatch(&words).map_err(|r| match r.code.as_str() {
            "not_wired" => not_wired(&format!("`herdr {}`", command(args))),
            _ => herdr_error(r, words.first().copied().unwrap_or("")),
        })
    }

    fn dispatch(&self, a: &[&str]) -> Result<String, Refused> {
        match a {
            // The nav draws its rows from the board's data itself: nothing is pushed.
            [_, "report-metadata", ..] => done(),
            ["workspace", "list"] => result(json!({ "type": "workspace_list", "workspaces": self.spaces()? })),
            ["workspace", "get", id] => result(json!({ "type": "workspace_info", "workspace": self.space(id)? })),
            ["workspace", "create", ..] => self.workspace_create(a),
            ["workspace", "focus", id] => self.focus_space(id),
            ["workspace", "rename", id, label @ ..] => self.rename_space(id, &label.join(" ")),
            ["workspace", "close", id] => self.close_space(id),
            ["tab", "create", ..] => self.tab_create(a),
            ["tab", "list", ..] => {
                let tabs: Vec<Value> = self.places()?.iter().filter(|p| flag(a, "--workspace").is_none_or(|w| p.space == w)).map(Place::tab_json).collect();
                result(json!({ "type": "tab_list", "tabs": tabs }))
            }
            ["tab", "get", id] => result(json!({ "type": "tab_info", "tab": self.place_or(id, "tab")?.tab_json() })),
            ["tab", "rename", id, label @ ..] => self.rename_tab(id, &label.join(" ")),
            ["tab" | "pane", "close", id] => self.close(id, a[0]),
            ["tab", "focus", id] | ["agent", "focus" | "attach", id] => self.focus(id, a[0]),
            ["worktree", "create", ..] => self.worktree_create(a),
            ["worktree", "open", ..] => self.worktree_open(a),
            ["worktree", "remove", ..] => self.worktree_remove(a),
            ["pane", "list", ..] => {
                let panes: Vec<Value> = self.places()?.iter().filter(|p| flag(a, "--workspace").is_none_or(|w| p.space == w)).map(Place::pane_json).collect();
                result(json!({ "type": "pane_list", "panes": panes }))
            }
            ["pane", "get", id] => result(json!({ "type": "pane_info", "pane": self.place_or(id, "pane")?.pane_json() })),
            ["pane", "split", id, ..] => self.pane_split(id, a),
            ["pane", "process-info", ..] => {
                let id = flag(a, "--pane").unwrap_or("");
                result(json!({ "type": "pane_process_info", "process_info": self.place_or(id, "pane")?.process_info() }))
            }
            ["pane", "run", id, line] => self.pane_run(id, line),
            ["pane", "read", id, ..] => self.read(id, a, false),
            ["agent", "read", id, ..] => self.read(id, a, true),
            ["pane", "send-text", id, text] => self.send_text(id, text),
            ["agent", "list"] => result(json!({ "type": "agent_list", "agents": self.places()?.iter().filter_map(Place::agent_json).collect::<Vec<_>>() })),
            ["agent", "get", target] => {
                let agent = self.agent_place(target)?.and_then(|p| p.agent_json()).ok_or_else(|| missing("agent", target))?;
                result(json!({ "type": "agent_info", "agent": agent }))
            }
            ["agent", "start", name, ..] => {
                let extra: Vec<String> = a.iter().skip_while(|w| **w != "--").skip(1).map(|w| w.to_string()).collect();
                let timeout = flag(a, "--timeout").and_then(|t| t.parse().ok()).unwrap_or(60000).min(self.wait_ms());
                let agent = self.start(flag(a, "--pane").unwrap_or(""), name, flag(a, "--kind").unwrap_or(""), &extra, &[], timeout)?;
                result(json!({ "type": "agent_started", "agent": agent }))
            }
            ["agent", "prompt", target, text] => self.prompt(target, text),
            ["agent", "send-keys", target, keys @ ..] => self.send_keys(target, keys),
            // Ferrite's Providers report their Attention themselves; a kind without one fails at its start.
            ["integration", "status"] => Ok(String::new()),
            ["machine", "list", ..] => self.machines(),
            _ => Err(Refused::new("not_wired", "")),
        }
    }

    fn call(&self, method: &str, params: Value) -> Result<Value, Refused> { self.server.call(method, params) }
    /// The longest the Server may wait inside one request and still answer before its deadline.
    fn wait_ms(&self) -> u64 {
        let timeout = self.server.timeout;
        timeout.saturating_sub(ANSWER_MARGIN.min(timeout / ANSWER_SHARE)).as_millis() as u64
    }

    /// Every pane: each open Thread, then each placed pane whose Thread is not open.
    fn places(&self) -> Result<Vec<Place>, Refused> {
        let threads = self.call("thread.list", json!({}))?.arr("threads").to_vec();
        let mut panes = self.panes.all().map_err(failed)?;
        let mut out = Vec::new();
        for t in threads {
            let thread = t.at("thread").as_u64();
            let id = panes.iter().find(|(_, p)| p.thread.is_some() && p.thread == thread).map(|(id, _)| id.clone());
            let pane = id.as_ref().and_then(|id| panes.remove(id));
            // A Thread in a linked worktree is in that worktree's Space; any other in its Project's.
            let space = pane.as_ref().map(|p| p.space.clone()).unwrap_or_else(|| if t.b("worktree") { t.t("worktree") } else { t.t("project") });
            let label = pane.as_ref().map(|p| p.label.clone()).filter(|l| !l.is_empty()).unwrap_or_else(|| t.t("title"));
            out.push(Place { id: id.unwrap_or_else(|| text(t.at("thread"))), space, cwd: t.t("cwd"), label, thread: Some(t), pane });
        }
        out.extend(panes.into_iter().map(|(id, p)| Place { id, space: p.space.clone(), cwd: p.cwd.clone(), label: p.label.clone(), thread: None, pane: Some(p) }));
        Ok(out)
    }
    fn place(&self, id: &str) -> Result<Option<Place>, Refused> { Ok(self.places()?.into_iter().find(|p| p.id == id)) }
    fn place_or(&self, id: &str, what: &str) -> Result<Place, Refused> { self.place(id)?.ok_or_else(|| missing(what, id)) }
    /// An agent target: its pane, or its name.
    fn agent_place(&self, target: &str) -> Result<Option<Place>, Refused> {
        let places = self.places()?;
        let by_name = || places.iter().position(|p| p.thread.as_ref().is_some_and(|t| t.s("title") == target));
        Ok(places.iter().position(|p| p.id == target).or_else(by_name).map(|i| places.into_iter().nth(i).expect("found")))
    }
    fn open_thread(&self, target: &str, area: &str) -> Result<u64, Refused> {
        self.agent_place(target)?.and_then(|p| p.thread_id()).ok_or_else(|| missing(area, target))
    }

    // ---- spaces

    /// Every Project, then every Space the board made, then every linked worktree's Space a pane is in.
    fn spaces(&self) -> Result<Vec<Value>, Refused> {
        let projects = self.call("project.list", json!({}))?.arr("projects").to_vec();
        let made = self.panes.spaces().map_err(failed)?;
        let places = self.places()?;
        let space = |id: &str, label: String, worktree: Value| {
            let here = || places.iter().filter(|p| p.space == id);
            let mut w = json!({ "workspace_id": id, "label": label, "focused": here().any(Place::focused), "pane_count": here().count(), "tab_count": here().count() });
            if worktree.is_object() { w["worktree"] = worktree; }
            w
        };
        let project = |id: &str| projects.iter().find(|p| p.s("id") == id);
        // A Project's checkout, `repo_root` its `repo_key` as herdr names it.
        let checkout = |p: &Value| {
            let mut worktree = p.at("worktree").clone();
            if worktree.is_object() { worktree["repo_root"] = worktree.at("repo_key").clone(); }
            worktree
        };
        let mut out: Vec<Value> = projects.iter().map(|p| space(p.s("id"), p.t("label"), checkout(p))).collect();
        out.extend(made.iter().map(|(id, s)| space(id, s.label.clone(), project(&s.project).map(checkout).unwrap_or(Value::Null))));
        for p in places.iter().filter(|p| is_path(&p.space)) {
            if out.iter().any(|w| w.s("workspace_id") == p.space) { continue; }
            let placed = p.pane.as_ref();
            let repo_of = |t: &Value| project(t.s("project")).map(|p| p.at("worktree").t("repo_key")).unwrap_or_default();
            let repo = placed.map(|x| x.repo.clone()).filter(|r| !r.is_empty()).or_else(|| p.thread.as_ref().map(repo_of)).unwrap_or_default();
            let label = placed.map(|x| x.space_label.clone()).filter(|l| !l.is_empty())
                .unwrap_or_else(|| Path::new(&p.space).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            out.push(space(&p.space, label, json!({ "checkout_path": p.space, "repo_root": repo, "repo_key": repo, "is_linked_worktree": true })));
        }
        Ok(out)
    }
    fn space(&self, id: &str) -> Result<Value, Refused> {
        self.spaces()?.into_iter().find(|w| w.s("workspace_id") == id).ok_or_else(|| missing("workspace", id))
    }
    /// A Space the board made, by its id.
    fn made_space(&self, id: &str) -> Result<Option<Space>, Refused> { Ok(self.panes.spaces().map_err(failed)?.remove(id)) }
    /// The Project a Space is in: a Project's own, a made Space's; a worktree's Space is the Server's to place.
    fn project_of(&self, space: &str) -> Result<Option<String>, Refused> {
        if is_path(space) { return Ok(None); }
        Ok(Some(self.made_space(space)?.map(|s| s.project).unwrap_or_else(|| space.into())))
    }
    /// The Space is a Project's own, named by the Project's id.
    fn projects_own(&self, space: &str) -> Result<bool, Refused> { Ok(!is_path(space) && self.made_space(space)?.is_none()) }

    /// A new Space, as herdr's `workspace create` always makes one: of its own, inside the Project holding `--cwd`
    /// (registered when there is none), with a pane placed in it. Labelled `--label`, else after its folder, the home
    /// folder `~`, as herdr labels one.
    fn workspace_create(&self, a: &[&str]) -> Result<String, Refused> {
        let cwd = flag(a, "--cwd").unwrap_or(".");
        let project = self.call("project.open", json!({ "cwd": cwd }))?.at("project").t("id");
        let label = flag(a, "--label").map(String::from).unwrap_or_else(|| folder_label(cwd));
        let (id, pane) = self.panes.open_space(Space { project, cwd: cwd.into(), label }, Pane { cwd: cwd.into(), env: envs(a), ..Default::default() }).map_err(failed)?;
        let place = self.place_or(&pane, "pane")?;
        result(json!({ "type": "workspace_created", "workspace": self.space(&id)?, "tab": place.tab_json(), "root_pane": place.pane_json() }))
    }
    fn place_pane(&self, pane: Pane) -> Result<Place, Refused> {
        let id = self.panes.place(pane).map_err(failed)?;
        self.place_or(&id, "pane")
    }
    /// A Project's own Space lands every window on the Project; any other on its first open Thread. With no open
    /// Thread there is nothing to show, and nothing moves.
    fn focus_space(&self, id: &str) -> Result<String, Refused> {
        self.space(id)?;
        if self.projects_own(id)? {
            self.call("project.focus", json!({ "project": id }))?;
        } else if let Some(thread) = self.places()?.iter().filter(|p| p.space == id).find_map(Place::thread_id) {
            self.call("thread.focus", json!({ "thread": thread }))?;
        }
        done()
    }
    /// A made Space's label is its own and a worktree Space's is kept with its panes: the Project keeps its own. Only a
    /// Project's own Space renames it.
    fn rename_space(&self, id: &str, label: &str) -> Result<String, Refused> {
        self.space(id)?;
        if self.projects_own(id)? {
            self.call("project.rename", json!({ "project": id, "label": label }))?;
        } else if is_path(id) {
            let mine: Vec<String> = self.panes.all().map_err(failed)?.into_iter().filter(|(_, p)| p.space == id).map(|(k, _)| k).collect();
            for k in mine { self.panes.update(&k, |p| p.space_label = label.into()).map_err(failed)?; }
        } else {
            self.panes.rename_space(id, label).map_err(failed)?;
        }
        done()
    }
    /// The panes in a Space close: their Threads are parked, and nothing else. A Project's own Space keeps the
    /// Project, and a Thread of the operator's in it stays open: only what the board placed there was the board's.
    fn close_space(&self, id: &str) -> Result<String, Refused> {
        self.space(id)?;
        let projects_own = self.projects_own(id)?;
        for p in self.places()?.into_iter().filter(|p| p.space == id && (!projects_own || p.pane.is_some())) {
            if let Some(thread) = p.thread_id() { self.close_thread(thread)?; }
        }
        self.panes.retain(|_, p| p.space != id).map_err(failed)?;
        done()
    }
    fn close_thread(&self, thread: u64) -> Result<(), Refused> {
        match self.call("thread.close", json!({ "thread": thread })) { Err(r) if r.code != "unknown_thread" => Err(r), _ => Ok(()) }
    }

    // ---- tabs and panes

    /// A pane placed in a Space, at `--cwd` (else the Space's own folder).
    fn tab_create(&self, a: &[&str]) -> Result<String, Refused> {
        let workspace = flag(a, "--workspace").unwrap_or("");
        let space = self.space(workspace)?;
        let folder = || self.made_space(workspace).ok().flatten().map(|s| s.cwd).or_else(|| space.at("worktree").so("checkout_path").map(String::from));
        let cwd = flag(a, "--cwd").map(String::from).or_else(folder)
            .ok_or_else(|| Refused::new("invalid_request", format!("Space {workspace} has no folder of its own: give the tab --cwd")))?;
        let place = self.place_pane(Pane { space: workspace.into(), cwd, label: flag(a, "--label").unwrap_or("").into(), env: envs(a), ..Default::default() })?;
        result(json!({ "type": "tab_created", "tab": place.tab_json(), "root_pane": place.pane_json() }))
    }
    /// A pane beside another, in its Space.
    fn pane_split(&self, id: &str, a: &[&str]) -> Result<String, Refused> {
        let beside = self.place_or(id, "pane")?;
        let (repo, space_label) = beside.pane.map(|p| (p.repo, p.space_label)).unwrap_or_default();
        let cwd = flag(a, "--cwd").map(String::from).unwrap_or(beside.cwd);
        let place = self.place_pane(Pane { space: beside.space, cwd, env: envs(a), repo, space_label, ..Default::default() })?;
        result(json!({ "type": "pane_split", "pane": place.pane_json() }))
    }
    fn rename_tab(&self, id: &str, label: &str) -> Result<String, Refused> {
        let place = self.place_or(id, "tab")?;
        match (&place.pane, place.thread_id()) {
            (Some(_), _) => self.panes.update(id, |p| p.label = label.into()).map_err(failed)?,
            (None, Some(thread)) => { self.call("thread.rename", json!({ "thread": thread, "title": label }))?; }
            (None, None) => {}
        }
        result(json!({ "type": "tab_info", "tab": self.place_or(id, "tab")?.tab_json() }))
    }
    fn close(&self, id: &str, area: &str) -> Result<String, Refused> {
        let place = self.place_or(id, area)?;
        if let Some(thread) = place.thread_id() { self.close_thread(thread)?; }
        self.panes.retain(|k, _| k != id).map_err(failed)?;
        done()
    }
    /// Every window lands on the Thread; a pane at its shell has none to land on.
    fn focus(&self, id: &str, area: &str) -> Result<String, Refused> {
        let place = self.agent_place(id)?.ok_or_else(|| missing(area, id))?;
        if let Some(thread) = place.thread_id() { self.call("thread.focus", json!({ "thread": thread }))?; }
        done()
    }
    fn quit(&self, id: &str) -> Result<(), Refused> {
        match self.place(id)?.and_then(|p| p.thread_id()) { Some(thread) => self.close_thread(thread), None => Ok(()) }
    }

    // ---- worktrees

    /// The Server makes the worktree; a pane is placed in its Space.
    fn worktree_create(&self, a: &[&str]) -> Result<String, Refused> {
        let cwd = flag(a, "--cwd").unwrap_or(".");
        let branch = flag(a, "--branch").unwrap_or("");
        let mut params = json!({ "cwd": cwd, "branch": branch });
        if let Some(base) = flag(a, "--base") { params["base"] = json!(base); }
        let made = self.call("worktree.create", params)?;
        let path = made.at("worktree").t("path");
        let repo = self.call("project.get", json!({ "project": made.at("project") })).ok().map(|p| p.at("project").at("worktree").t("repo_key")).filter(|r| !r.is_empty()).unwrap_or_else(|| cwd.into());
        let place = self.place_pane(Pane { space: path.clone(), cwd: path.clone(), repo, space_label: flag(a, "--label").unwrap_or("").into(), ..Default::default() })?;
        result(json!({ "type": "worktree_created", "workspace": self.space(&path)?, "tab": place.tab_json(), "root_pane": place.pane_json(),
            "worktree": { "path": path, "branch": branch, "open_workspace_id": path } }))
    }
    /// A worktree that stands: its Space as it is, or a pane placed in it.
    fn worktree_open(&self, a: &[&str]) -> Result<String, Refused> {
        let path = flag(a, "--path").unwrap_or("");
        let open = self.places()?.into_iter().find(|p| p.space == path);
        let already = open.is_some();
        let place = match open {
            Some(p) => p,
            None => self.place_pane(Pane { space: path.into(), cwd: path.into(), repo: flag(a, "--cwd").unwrap_or("").into(), space_label: flag(a, "--label").unwrap_or("").into(), ..Default::default() })?,
        };
        result(json!({ "type": "worktree_opened", "workspace": self.space(path)?, "tab": place.tab_json(), "root_pane": place.pane_json(), "already_open": already }))
    }
    /// Never forced: a worktree with changes is refused and left whole (`dirty_worktree_requires_force`).
    fn worktree_remove(&self, a: &[&str]) -> Result<String, Refused> {
        if has(a, "--force") { return Err(Refused::new("invalid_request", "Ferrite never removes a worktree by force: one with changes is left whole.")); }
        let space = flag(a, "--workspace").unwrap_or("");
        if !is_path(space) { return Err(Refused::new("workspace_not_found", format!("Space {space} is a Ferrite Project, not a linked worktree"))); }
        self.call("worktree.remove", json!({ "path": space }))?;
        self.panes.retain(|_, p| p.space != space).map_err(failed)?;
        result(json!({ "type": "worktree_removed" }))
    }

    // ---- agents

    /// `agent start` in a pane at its shell: the Thread placed there starts (`thread.start --ready`), or, where an agent
    /// of the same kind ran before, starts again in place (`thread.revive --ready`). `launcher`: a program it runs
    /// through. Its Thread is the pane's from then on, also when the start is refused `agent_not_ready`.
    fn start(&self, pane_id: &str, name: &str, kind: &str, extra: &[String], launcher: &[String], timeout_ms: u64) -> Result<Value, Refused> {
        let provider = provider(kind)?;
        let place = self.place_or(pane_id, "pane")?;
        let (Some(pane), None) = (place.pane, &place.thread) else {
            return Err(Refused::new("pane_busy", "the pane is not at its shell prompt: its Thread's agent runs"));
        };
        let (args, resume) = split_resume(kind, extra);
        let mut how = json!({ "args": args, "env": pane.env, "launcher": launcher, "ready": true, "timeout_ms": timeout_ms });
        if let Some(resume) = resume { how["resume"] = json!(resume); }
        let again = pane.thread.filter(|_| pane.kind == kind).map(|thread| {
            let mut params = how.clone();
            params["thread"] = json!(thread);
            self.call("thread.revive", params).and_then(|r| {
                if r.at("thread").s("title") != name { self.call("thread.rename", json!({ "thread": thread, "title": name }))?; }
                Ok(r)
            })
        });
        let started = match again {
            // Deleted while it was parked: a new one starts.
            Some(Err(r)) if r.code == "unknown_thread" => self.start_thread(&pane, provider, name, how),
            Some(revived) => revived,
            None => self.start_thread(&pane, provider, name, how),
        };
        let thread = match &started { Ok(r) => r.at("thread").at("thread").as_u64(), Err(r) => r.thread };
        if let Some(thread) = thread { self.panes.update(pane_id, |p| { p.thread = Some(thread); p.kind = kind.into(); }).map_err(failed)?; }
        started?;
        self.place_or(pane_id, "pane")?.agent_json().ok_or_else(|| missing("agent", pane_id))
    }
    /// A Thread placed in a Project's Space joins that Project, working in its folder; a folder that is not the
    /// Project's starts in the Project that folder is.
    fn start_thread(&self, pane: &Pane, provider: &str, name: &str, how: Value) -> Result<Value, Refused> {
        let mut params = how;
        params["cwd"] = json!(pane.cwd);
        params["provider"] = json!(provider);
        params["title"] = json!(name);
        if let Some(project) = self.project_of(&pane.space)? {
            let mut in_project = params.clone();
            in_project["project"] = json!(project);
            match self.call("thread.start", in_project) { Err(r) if r.code == "invalid_request" => {} started => return started }
        }
        self.call("thread.start", params)
    }
    /// herdr's `pane run`, as the board uses it: a plan writer started in a read-only mount (plan/sandbox.rs), `env
    /// HERDR_PROJECTS_AGENT=<name> bwrap … -- <program> <args>`. The mount is the Thread's launcher. Its agent is up, or
    /// stays blocked, when this answers.
    fn pane_run(&self, id: &str, line: &str) -> Result<String, Refused> {
        let words = shell_words(line);
        let mut rest = words.as_slice();
        let mut name = String::new();
        if rest.first().map(String::as_str) == Some("env") { rest = &rest[1..]; }
        while let Some((assignment, after)) = rest.split_first().filter(|(w, _)| w.contains('=') && !w.starts_with('-')) {
            if let Some(agent) = assignment.strip_prefix("HERDR_PROJECTS_AGENT=") { name = agent.into(); }
            rest = after;
        }
        let refused = || Refused::new("not_wired", "");
        if rest.first().map(String::as_str) != Some("bwrap") { return Err(refused()); }
        let end = rest.iter().position(|w| w == "--").ok_or_else(refused)?;
        let (launcher, program) = (&rest[..=end], rest.get(end + 1).ok_or_else(refused)?);
        let kind = crate::agents::kinds::all().find(|k| crate::agents::kinds::program(k.name) == program.as_str()).map(|k| k.name).unwrap_or(program.as_str());
        match self.start(id, &name, kind, &rest[end + 2..], launcher, self.wait_ms()) {
            Err(r) if r.code != "agent_not_ready" => Err(r),
            _ => done(),
        }
    }
    /// Sent, and answered once the agent was seen taking it (working, or asking): herdr's `agent prompt`.
    fn prompt(&self, target: &str, text: &str) -> Result<String, Refused> {
        let thread = self.open_thread(target, "agent")?;
        self.call("agent.prompt", json!({ "thread": thread, "text": text, "until": ["working", "blocked"], "timeout_ms": self.wait_ms() }))?;
        let agent = self.agent_place(target)?.and_then(|p| p.agent_json()).unwrap_or(Value::Null);
        result(json!({ "type": "agent_prompted", "agent": agent }))
    }
    /// The tail of the Thread's transcript; a pane at its shell shows nothing.
    fn read(&self, id: &str, a: &[&str], agent: bool) -> Result<String, Refused> {
        let place = self.agent_place(id)?;
        let Some(thread) = place.as_ref().and_then(Place::thread_id) else {
            return match (place, agent) { (Some(_), false) => Ok(String::new()), _ => Err(missing(if agent { "agent" } else { "pane" }, id)) };
        };
        let mut params = json!({ "thread": thread });
        if let Some(lines) = flag(a, "--lines").and_then(|n| n.parse::<u64>().ok()) { params["lines"] = json!(lines); }
        Ok(self.call("agent.read", params)?.t("text"))
    }
    /// Escape or Ctrl+C interrupts the agent; a menu key answers the Decision pending on it; enter with nothing pending
    /// sends nothing, as in an empty input box. Anything else is refused before anything is sent.
    fn send_keys(&self, target: &str, keys: &[&str]) -> Result<String, Refused> {
        let keys: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
        let place = self.agent_place(target)?.ok_or_else(|| missing("agent", target))?;
        let one = |k: &[&str]| matches!(keys.as_slice(), [key] if k.contains(&key.as_str()));
        let Some(thread) = place.thread_id() else {
            // At a shell prompt these do nothing.
            return if one(&["esc", "ctrl+c", "enter"]) { done() } else { Err(missing("agent", target)) };
        };
        if one(&["esc", "ctrl+c"]) { self.call("agent.interrupt", json!({ "thread": thread }))?; return done(); }
        let Some(decision) = self.pending(thread)? else {
            if one(&["enter"]) { return done(); }
            return Err(Refused::new("no_decision", format!("nothing on Thread {thread} waits for keys ({}): Ferrite sends a prompt whole, and only a Decision is answered with a menu key", keys.join(" "))));
        };
        let answer = answers::key_answer(&decision, &keys)
            .ok_or_else(|| Refused::new("invalid_answer", format!("`{}` answers nothing in Thread {thread}'s Decision: {}", keys.join(" "), answers::decision_text(&decision))))?;
        self.answer_decision(thread, &decision, answer)
    }
    /// Text typed at a pending Decision answers it, where it takes typed text. At a shell prompt it does nothing.
    fn send_text(&self, id: &str, text: &str) -> Result<String, Refused> {
        let place = self.place_or(id, "pane")?;
        let Some(thread) = place.thread_id() else { return done() };
        let decision = self.pending(thread)?.ok_or_else(|| Refused::new("no_decision", format!("nothing on Thread {thread} takes typed text: Ferrite has no input box to type into")))?;
        let answer = answers::text_answer(&decision, text)
            .ok_or_else(|| Refused::new("invalid_answer", format!("Thread {thread}'s Decision takes no typed text, only its menu keys: {}", answers::decision_text(&decision))))?;
        self.answer_decision(thread, &decision, answer)
    }
    fn pending(&self, thread: u64) -> Result<Option<Value>, Refused> {
        Ok(self.call("decision.list", json!({ "thread": thread }))?.arr("decisions").first().map(|d| d.at("decision").clone()))
    }
    fn answer_decision(&self, thread: u64, decision: &Value, answer: Value) -> Result<String, Refused> {
        self.call("decision.answer", json!({ "thread": thread, "decision": decision.at("id"), "answer": answer }))?;
        done()
    }

    /// herdr's `machine list --json`: in their order (the machines sidebar's), each the one session of its Server.
    fn machines(&self) -> Result<String, Refused> {
        let machines: Vec<Value> = self.call("machine.list", json!({}))?.arr("machines").iter()
            .map(|m| json!({ "id": m.at("id"), "label": m.at("label"), "target": m.at("target"), "session": "default", "enabled": true })).collect();
        Ok(Value::Array(machines).to_string())
    }
}
