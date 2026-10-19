//! The board's own check as a task shows it: the count its command reports, the wait for a turn, the minutes left, and
//! saying so when it stands still. A check command tells the board where it is by writing one line to the file named
//! in HERDR_PROJECTS_PROGRESS (docs/check-progress.md). Nothing here is a guess: with no count there is no percent,
//! and with no earlier run there are no minutes.
use crate::json::*;

const MIN: i64 = 60000;
pub const KNOWN_LINE: &str = "The check already passed for this exact code";
/// How long nothing may move before the row says so: three times a usual run, never under three minutes; ten where none was timed.
pub fn still_after(usual: i64) -> i64 { if usual > 0 { (3 * MIN).max(3 * usual) } else { 10 * MIN } }
const MIN_SHARE: f64 = 0.05;
fn count(v: &Value) -> Option<i64> { let n = number(v); (v.is_number() || v.as_str().map(|s| s.trim().parse::<f64>().is_ok()).unwrap_or(false)).then_some(n).filter(|n| n.is_finite() && *n >= 0.0).map(|n| n.floor() as i64) }
/// What a check wrote: JSON ({"done":31,"total":47,"unit":"tests","before":0}, {"state":"waiting","ahead":2},
/// {"state":"known"}) or plain words ("31/47", "31 of 47 tests"). None: nothing the board can read.
pub fn read_report(text: &str) -> Option<Value> {
    let s = text.trim();
    if s.is_empty() { return None; }
    static PLAIN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"^(\d+)\s*(?:/|of)\s*(\d+)(?:\s+([A-Za-z][A-Za-z ]{0,19}))?$").unwrap());
    let j = match PLAIN.captures(s) {
        Some(c) => json!({ "done": &c[1], "total": &c[2], "unit": c.get(3).map(|m| m.as_str()) }),
        None => serde_json::from_str::<Value>(s).ok()?,
    };
    if !j.is_object() { return None; }
    let state = j.at("state");
    if state == &json!("known") { return Some(json!({ "state": "known" })); }
    if state == &json!("waiting") { return Some(json!({ "state": "waiting", "ahead": count(j.at("ahead")) })); }
    if !state.is_null() && state != &json!("running") { return None; }
    let (done, total) = (count(j.at("done")), count(j.at("total")).filter(|t| *t > 0));
    let (Some(done), Some(total)) = (done, total) else { return if state == &json!("running") { Some(json!({ "state": "running" })) } else { None } };
    let unit: String = j.at("unit").as_str().map(|u| u.chars().filter(|c| c.is_ascii_alphabetic() || *c == ' ').collect::<String>().trim().chars().take(20).collect()).unwrap_or_default();
    let done = done.min(total);
    Some(json!({ "state": "running", "done": done, "total": total, "before": count(j.at("before")).unwrap_or(0).min(done).min(total), "unit": unit }))
}
/// A check that just started: nothing read yet. `usual`: how long its last full runs took here, 0 when never timed.
pub fn start_check(now: i64, usual: i64) -> Value { json!({ "at": now, "runAt": now, "movedAt": now, "usual": usual.max(0) }) }
/// The check after one more reading.
pub fn step_check(c: &Value, r: Option<&Value>, now: i64) -> Value {
    let Some(r) = r else { return c.clone() };
    let mut n = obj(json!({ "at": c.at("at"), "usual": c.at("usual"), "runAt": c.at("runAt"), "movedAt": c.at("movedAt"), "state": r.at("state") }));
    if r.s("state") == "waiting" {
        n.put("ahead", r.at("ahead").clone());
        if c.s("state") != "waiting" || c.at("ahead") != r.at("ahead") { n.put("movedAt", now); }
    } else if r.s("state") == "running" {
        for k in ["done", "total", "unit"] { n.put(k, r.at(k).clone()); }
        n.put("from", r.i("before"));
        if c.s("state") != "running" && c.has("state") { n.put("runAt", now); n.put("movedAt", now); }
        if r.at("done") != c.at("done") { n.put("movedAt", now); }
    }
    Value::Object(n)
}
/// How long the run itself took, for a run worth remembering: one that did all of its work. None otherwise.
pub fn ran_for(c: &Value, now: i64) -> Option<i64> {
    if c.s("state") == "known" || c.s("state") == "waiting" || (c.i("total") > 0 && c.i("from") > 0) { None } else { Some((now - c.i("runAt")).max(0)) }
}
fn mins(ms: i64) -> String { format!("{}m", ((ms as f64 / MIN as f64).round() as i64).max(1)) }
fn spent_words(ms: i64) -> String { if ms < MIN { "under 1m".into() } else { format!("{}m", ms / MIN) } }
fn left_of(ms: f64) -> i64 { ((ms / MIN as f64).ceil() as i64).clamp(1, 99999) }
pub struct Shown { pub msg: String, pub progress: Option<i64>, pub eta: Option<i64> }
fn shown(msg: String, progress: Option<i64>, eta: Option<i64>) -> Shown { Shown { msg, progress, eta } }
/// What the task shows: the line, the percent (only from a true count) and the minutes left (only from earlier runs
/// or from the pace of this one).
pub fn check_shown(c: &Value, now: i64) -> Shown {
    let usual = c.i("usual").max(0);
    let moved = if c.b("movedAt") { c.i("movedAt") } else { c.i("at") };
    let idle = (now - moved).max(0);
    let still = idle > still_after(usual);
    if c.s("state") == "known" { return shown(KNOWN_LINE.into(), None, None); }
    if c.s("state") == "waiting" {
        let ahead = c.i("ahead").max(0);
        let aw = if ahead > 0 { format!(" · {ahead} ahead") } else { String::new() };
        if still { return shown(format!("Waiting for its turn unusually long · nothing moved for {}{aw}", mins(idle)), None, None); }
        let eta = (usual > 0).then(|| left_of(((ahead.max(1) * usual - idle).max(0) + usual) as f64));
        return shown(format!("Waiting for its turn{aw}"), None, eta);
    }
    let spent = (now - if c.b("runAt") { c.i("runAt") } else { c.i("at") }).max(0);
    let total = c.i("total");
    if total > 0 {
        let done = c.i("done").min(total);
        let from = c.i("from").min(done);
        let unit = c.s("unit");
        let place = format!("{}{done} of {total}", if unit.is_empty() { String::new() } else { format!("{unit} ") });
        let progress = if done >= total { 100 } else { ((done as f64 / total as f64) * 100.0).floor().min(99.0) as i64 };
        if done >= total { return if still { shown(format!("The check has not ended · all finished {} ago · {place}", mins(idle)), Some(progress), None) } else { shown(format!("Running the check · {place}"), Some(progress), Some(0)) }; }
        if still { return shown(format!("The check stands still · nothing finished for {} · {place}", mins(idle)), Some(progress), None); }
        let whole = usual as f64 * ((total - from) as f64 / total as f64);
        let eta = if whole > spent as f64 { Some(left_of(whole - spent as f64)) }
            else if (done - from) as f64 / (total - from) as f64 >= MIN_SHARE { Some(left_of(spent as f64 * (total - done) as f64 / (done - from) as f64)) } else { None };
        return shown(format!("Running the check · {place}"), Some(progress), eta);
    }
    if usual == 0 { return shown(format!("Running the check · {}", spent_words(spent)), None, None); }
    if spent > (3 * usual).max(usual + 10 * MIN) { return shown(format!("The check runs far longer than usual · {}, usually {}", spent_words(spent), mins(usual)), None, None); }
    if spent >= usual { return shown(format!("Running the check · {}, usually {}", spent_words(spent), mins(usual)), None, None); }
    shown(format!("Running the check · {} of about {}", spent_words(spent), mins(usual)), None, Some(left_of((usual - spent) as f64)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reports() {
        assert_eq!(read_report("31 of 47 tests").unwrap(), json!({ "state": "running", "done": 31, "total": 47, "before": 0, "unit": "tests" }));
        assert_eq!(read_report(r#"{"state":"waiting","ahead":2}"#).unwrap().i("ahead"), 2);
        assert!(read_report("nonsense").is_none());
        let c = step_check(&start_check(0, 600000), read_report("10/40").as_ref(), 1000);
        let s = check_shown(&c, 60000);
        assert_eq!(s.msg, "Running the check · 10 of 40");
        assert_eq!(s.progress, Some(25));
        assert_eq!(s.eta, Some(9));
    }
}
