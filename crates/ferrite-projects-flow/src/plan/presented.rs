//! A program in its own plan mode presents its plan: the board takes it there, and the tool that would ask in the
//! terminal is refused. `herdr-projects plan-written` is run by the hook the board passes at launch (Claude Code's
//! PreToolUse on ExitPlanMode), inside the agent's turn. It saves the plan as the task's plan file, tells the agent on
//! stderr to stop, and exits 2, which is how such a hook says no. Anything else is let through (exit 0).
use super::{open, write_plan};
use crate::json::*;
use crate::store::marks::token_ok;
use crate::store::tasks::get;
use std::io::Read;

pub const SAVED: &str = "The work board has your plan: it is saved, and it now waits for approval there. Stop here, with one line that says so. Do not present the plan again, and change nothing, until you are sent a message.";
pub fn presented(id: &str, token: &str, input: Option<Value>) -> i32 {
    let Some(t) = get(id) else { return 0 };
    if !open(&t) || !token_ok(t.s("id"), "agent", token, 0) { return 0; }
    let j = input.unwrap_or_else(|| { let mut s = String::new(); let _ = std::io::stdin().read_to_string(&mut s); serde_json::from_str(if s.trim().is_empty() { "{}" } else { &s }).unwrap_or(json!({})) });
    let tool = j.at("tool_input");
    let mut plan = tool.t("plan").trim().to_string();
    if plan.is_empty() && tool.b("planFilePath") { plan = std::fs::read_to_string(tool.s("planFilePath")).map(|s| s.trim().to_string()).unwrap_or_default(); }
    if plan.is_empty() { return 0; }
    if write_plan(&t, &plan).is_err() { return 0; }
    eprintln!("{SAVED}");
    2
}
