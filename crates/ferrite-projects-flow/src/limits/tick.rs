//! The limits' own pass, run by whoever ticks (the keeper, or an open board): a run that has taken its `limit.time` is
//! stopped, a task a limit stopped carries on by itself once its limit was raised, and the tokens of a running agent
//! are counted for the panel. `limit.tokens` itself is only acted on at the end of a turn.
use super::reached::{lifted, stop_at, StopHow};
use super::run::{limit_said, ran_for, stopped_at_limit};
use super::spent::count;
use crate::json::*;
use crate::store::runs::{keep_run_time, run_role};
use crate::store::settings::lookup::setting_text;
use crate::store::settings::values::{ms, NONE};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

const LOOK: i64 = 2000;
const COUNT: i64 = 15000;
static LOOKED: LazyLock<Mutex<HashMap<String, i64>>> = LazyLock::new(Default::default);
static COUNTED_AT: LazyLock<Mutex<HashMap<String, i64>>> = LazyLock::new(Default::default);
/// A clock that went back is a new start.
fn within(last: Option<i64>, now: i64, span: i64) -> bool { matches!(last, Some(l) if now >= l && now - l < span) }
/// `rows`: the board's list as this herdr session shows it (flow/live.rs). `carry_on`: what continues a task.
pub fn limits_tick(rows: &[Obj], now: i64, carry_on: Option<&dyn Fn(&str)>) {
    for r in rows {
        let live = r.at("live");
        if r.b("archivedAt") || !crate::flow::task::is_agent_task(r) || !live.is_object() || live.b("away") || live.b("down") { continue; }
        let (stopped, role) = (stopped_at_limit(r), run_role(r));
        if !stopped && role.is_empty() { continue; }
        let id = r.s("id").to_string();
        if within(LOOKED.lock().unwrap().get(&id).copied(), now, LOOK) { continue; }
        LOOKED.lock().unwrap().insert(id.clone(), now);
        if !stopped && !limit_said(r) { continue; }
        if stopped { if let Some(f) = carry_on { if lifted(r) { f(&id); } } continue; }
        let (time, tokens) = (keep_run_time(r), setting_text("limit.tokens", Some(r)));
        // An agent that shows a dialog waits for you: that wait is not stopped.
        let at_work = r.s("stage") == "reviewing" || (r.s("stage") == "working" && r.s("state") == "working" && live.s("status") != "blocked");
        if time != NONE && at_work {
            let used = ran_for(r, now);
            if used >= ms(&time) { stop_at(&id, "time", &time, used as f64, StopHow { role: role.into(), ..Default::default() }); continue; }
        }
        if tokens != NONE && !within(COUNTED_AT.lock().unwrap().get(&id).copied(), now, COUNT) { COUNTED_AT.lock().unwrap().insert(id.clone(), now); count(&id, role, 300); }
    }
}
