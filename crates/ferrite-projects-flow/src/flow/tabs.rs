//! A task's tabs in its herdr space: the agent's tab is named `Agent`, and each reviewer has a tab of its own, named
//! after its review round. A task done several times at once has a tab per attempt: `Agent A`, `Agent B`. Once an
//! agent took over from another, its tab names the one that is on it now: `Agent (codex)`, `Reviewer #2 (claude)`. A
//! planner of its own (`plan.agent`) has the tab `Planner`. Checked on herdr 0.9.3: `tab create --no-focus` leaves the
//! focus where it is; closing a tab's last pane closes the tab; a tab's id is not given out again.
use crate::err::Res;
use crate::herdr::Herdr;
use crate::json::*;

pub const AGENT_TAB: &str = "Agent";
/// The number is the review round: the one in the row's `R#2`.
pub fn reviewer_tab(round: i64, by: &str) -> String { format!("Reviewer #{round}{}", if by.is_empty() { String::new() } else { format!(" ({by})") }) }
pub const PLANNER_TAB: &str = "Planner";
pub const PLAN_REVIEW_TAB: &str = "Plan review";
/// The tab of the reviewer that compares a task's attempts.
pub const PICKER_TAB: &str = "Picker";
/// A name is a convenience, as an agent's is: a herdr that will not rename a tab does not stop the task.
pub fn name_tab(api: &Herdr, tab: &str, label: &str) { if !tab.is_empty() { api.tab_rename(tab, label); } }
/// An attempt's tab says which attempt it is (`Agent B`), and keeps saying so once that attempt is the task.
pub fn agent_tab(t: Option<&Obj>) -> String {
    let Some(t) = t else { return AGENT_TAB.into() };
    let letter = if t.b("letter") { t.t("letter") } else { t.at("picked").t("letter") };
    format!("{AGENT_TAB}{}{}", if letter.is_empty() { String::new() } else { format!(" {letter}") }, if t.b("on") { format!(" ({})", t.s("agent")) } else { String::new() })
}
pub fn name_agent_tab(api: &Herdr, tab: &str, t: Option<&Obj>) { name_tab(api, tab, &agent_tab(t)); }
/// A tab's ids: { tab, pane, terminal }.
pub struct Opened { pub tab: String, pub pane: String, pub terminal: String }
/// A tab of its own in the task's space, with one pane at its shell.
pub fn open_tab(api: &Herdr, t: &Obj, label: &str, cwd: &str, env: &[(String, String)], who: &str) -> Res<Opened> {
    let workspace = if t.at("link").b("workspace") { t.at("link").t("workspace") } else { api.pane_get(t.at("link").s("pane")).map(|p| p.t("workspace_id")).unwrap_or_default() };
    if workspace.is_empty() { crate::fail!("the task's space is gone"); }
    let r = api.tab_create(&workspace, cwd, label, env)?;
    let tab = if r.at("tab").b("tab_id") { r.at("tab").t("tab_id") } else { r.at("root_pane").t("tab_id") };
    let pane = if r.at("root_pane").is_object() { Some(r.at("root_pane").clone()) } else { api.panes(Some(&workspace))?.into_iter().find(|p| p.s("tab_id") == tab) };
    let Some(pane) = pane else { if !tab.is_empty() { api.tab_close(&tab); } crate::fail!("herdr opened no terminal for {who}") };
    Ok(Opened { tab: if tab.is_empty() { pane.t("tab_id") } else { tab }, pane: pane.t("pane_id"), terminal: pane.t("terminal_id") })
}
/// The reviewer's own tab: `label` names another one than its round's (the look at a plan).
pub fn open_reviewer_tab(api: &Herdr, t: &Obj, round: i64, by: &str, cwd: &str, env: &[(String, String)], label: Option<&str>) -> Res<Opened> {
    let l = label.map(String::from).unwrap_or_else(|| reviewer_tab(round, by));
    open_tab(api, t, &l, cwd, env, "the reviewer")
}
/// `reviewer`: what the task keeps of it (link.reviewer). One from before reviewers had tabs has only a pane.
pub fn close_reviewer_tab(api: &Herdr, reviewer: &Value) {
    if !reviewer.is_object() { return; }
    if reviewer.b("tab") { api.tab_close(reviewer.s("tab")); }
    if reviewer.b("pane") { api.pane_close(reviewer.s("pane")); }
}
