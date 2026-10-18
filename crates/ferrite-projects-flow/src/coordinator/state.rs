//! The Coordinator's saved state (one file per herdr session) and the folder it chats in.
use crate::err::Res;
use crate::herdr::cli::session_name;
use crate::json::*;
use crate::store::files::{data_dir, marker_alive, read_obj, with_lock_wait, write_json};
use std::path::PathBuf;

pub fn dir_of(socket: &str) -> PathBuf { data_dir().join("coordinator").join(session_name(socket)) }
fn file_of(socket: &str) -> PathBuf { dir_of(socket).join("state.json") }
pub fn get(socket: &str) -> Option<Obj> { read_obj(&file_of(socket)) }
/// Ferrite: the operator can hand the native chat to another Provider. Follow that choice before reading its
/// conversation or restarting it; a model, effort and transcript from the old Provider do not cross the handover.
pub fn follow_agent(socket: &str, before: &Obj, agent: Option<&Value>) -> Obj {
    let Some(agent) = agent.filter(|a| a.b("agent") && a.s("agent") != before.at("run").s("kind")) else { return before.clone() };
    change(socket, |c| {
        if c.s("pane") != before.s("pane") || c.at("run").s("kind") != before.at("run").s("kind") { return false; }
        let mut run = c.o("run").cloned().unwrap_or_default();
        run.put("kind", agent.at("agent").clone()); run.put("model", agent.t("model")); run.put("effort", agent.t("effort"));
        c.put("run", run); c.put("transcript", Value::Null);
        c.put("session", agent.at("agent_session").at("value").clone());
        true
    }).unwrap_or_else(|_| before.clone())
}
/// A pump (the detached job that starts it and types into it) is at work for this herdr session.
pub fn pumping(c: &Obj) -> bool { c.at("pump").is_object() && marker_alive(c.at("pump")) }
/// The one way its state changes: under its lock. `f` returning false writes nothing.
pub fn change(socket: &str, f: impl FnOnce(&mut Obj) -> bool) -> Res<Obj> {
    with_lock_wait(&format!("coordinator:{}", session_name(socket)), 5000, || {
        let mut c = get(socket).unwrap_or_else(|| obj(json!({ "queue": [] })));
        if !f(&mut c) { return Ok(c); }
        write_json(&file_of(socket), &Value::Object(c.clone()))?;
        Ok(c)
    })
}
/// Where it runs: a folder of its own (HERDR_PROJECTS_COORDINATOR_DIR names another, used for nothing else). A folder
/// nothing else chats in is how its conversation is told from any other.
pub fn home(socket: &str) -> PathBuf {
    std::env::var("HERDR_PROJECTS_COORDINATOR_DIR").ok().filter(|s| !s.is_empty()).map(PathBuf::from).unwrap_or_else(|| dir_of(socket).join("chat"))
}
