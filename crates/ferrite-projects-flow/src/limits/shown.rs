//! What a task shows of its limits: what this run and the task have used against each one, the words the row says
//! when one is near, and the same for `details`.
use super::run::*;
use crate::json::*;
use crate::store::runs::{run_role, run_time};
use crate::store::settings::lookup::setting_text;
use crate::store::settings::values::{ms, tokens_of, tokens_say, NONE};
use crate::store::tasks::Task;

/// From this share of a limit on, the row says how much is used.
pub const NEAR: f64 = 0.8;
/// A task's limits while an agent runs on it: { time: { limit, used }, tokens: { limit, used, kind }, near }.
pub fn limits_of(t: &Task, now: i64) -> Option<Value> {
    let role = run_role(t);
    if role.is_empty() || t.b("archivedAt") || !limit_said(t) { return None; }
    let (time, tokens) = (run_time(t), setting_text("limit.tokens", Some(t)));
    if time == NONE && tokens == NONE { return None; }
    let mut out = Obj::new();
    let mut near = vec![];
    if time != NONE {
        let used = ran_for(t, now);
        out.put("time", json!({ "limit": time, "used": used }));
        if used as f64 >= NEAR * ms(&time) as f64 { near.push(format!("{} of its {time}", time_say(used))); }
    }
    if tokens != NONE {
        let kind = if role == "reviewer" { t.at("link").at("reviewer").t("kind") } else { t.t("agent") };
        let used = used_of(t);
        out.put("tokens", json!({ "limit": tokens, "used": used, "kind": kind }));
        if used >= NEAR * tokens_of(&tokens) as f64 { near.push(format!("{} of {tokens} tokens", tokens_say(used))); }
    }
    if !near.is_empty() { out.put("near", near.join(" · ")); }
    Some(Value::Object(out))
}
/// One limit as the panel says it: "1h12m of 2h, this run", "1.2m of 2m".
pub fn time_line(x: &Value) -> String { format!("{} of {}, this run", time_say(x.i("used")), x.s("limit")) }
pub fn tokens_line(x: &Value, counted: bool) -> String { if counted { format!("{} of {}", tokens_say(x.f("used")), x.s("limit")) } else { format!("not counted for this program · limit {}", x.s("limit")) } }
/// For `details`: the limits as they stand, what has been used, and the stop when one was reached.
pub fn limits_detail(t: &Task, now: i64) -> Value {
    let running = !run_role(t).is_empty();
    let time = if running { run_time(t) } else { setting_text("limit.time", Some(t)) };
    let mut out = obj(json!({ "time": { "limit": time }, "tokens": { "limit": setting_text("limit.tokens", Some(t)), "used": used_of(t), "total": t.at("spent").f("total") } }));
    if running { let r = ran_for(t, now); out.obj_mut("time").put("usedMs", r); out.obj_mut("time").put("used", time_say(r)); }
    if stopped_at_limit(t) { let mut s = t.o("limitStop").cloned().unwrap_or_default(); s.put("line", t.at("msg").clone()); out.put("stopped", s); }
    Value::Object(out)
}
