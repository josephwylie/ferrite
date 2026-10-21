//! An attempt in a few words: where it stands (passed, out, needs you, waits for a slot, at work), its mark on the
//! task's row (`A claude ✓ · B codex working 60% · C pi ✗ check`), and what the task says while its attempts run or
//! wait for a pick. The row, the task panel, herdr's sidebar, the web page and `show <id>` all say it with these.
use crate::json::*;
use crate::store::attempts::attempts_of;
use crate::store::files::marker_alive;
use crate::store::retries::waiting_of;
use crate::store::settings::values::agent_of;

const GONE_MS: i64 = 15000;
/// Where one attempt stands, from its own saved task: passed, out, needs, waits or works.
pub fn standing_of(c: &Obj, now: i64) -> &'static str {
    match c.s("stage") {
        "tried" => "passed",
        "nosummary" | "stopped" | "todo" | "done" => "out",
        "failed" => if waiting_of(c).is_some() { "works" } else if c.at("failure").b("auth") { "needs" } else { "out" },
        "asks" | "reviewneeds" => "needs",
        "queued" => "waits",
        // Its terminal was closed, or its program ended, and nothing is bringing it back.
        "working" if !marker_alive(c.at("runner")) && (c.at("last").b("gone") || (c.at("last").at("status").is_null() && c.at("last").as_object().map(|l| l.contains_key("status")).unwrap_or(false) && c.b("exitedAt") && now - c.i("exitedAt") > GONE_MS)) => "out",
        _ => "works",
    }
}
/// Why an attempt is out, in a word or two: what stands after its ✗.
pub fn out_why(c: &Obj) -> &'static str {
    match c.s("stage") {
        "failed" => match c.at("failure").s("stage") { "check" => "check", "setup" => "setup", "start" => "could not start", "research" => "changed the repo", "agent" => "agent stopped", _ => "failed" },
        "nosummary" => "no summary",
        "stopped" => "stopped",
        _ => "agent gone",
    }
}
pub fn agent_word(c: &Obj) -> String {
    let w = c.at("settings").t("worker.agent");
    let k = agent_of(if w.is_empty() { c.s("agent") } else { &w }).kind;
    if !k.is_empty() { k } else if c.b("agent") { c.t("agent") } else { "agent".into() }
}
/// One attempt's mark. `c` is the attempt as the board shows it, or as saved.
pub fn mark_of(c: &Obj, now: i64) -> String {
    let who = format!("{} {}", c.s("letter"), agent_word(c));
    match standing_of(c, now) {
        "passed" => format!("{who} ✓"),
        "out" => format!("{who} ✗ {}", out_why(c)),
        "needs" => format!("{who} ? {}", if c.s("stage") == "failed" { "needs a login" } else { "asks you" }),
        "waits" => format!("{who} waits for a slot"),
        _ => {
            let step = match c.s("stage") { "starting" => "starting", "setup" => "setting up", "checking" => "checking", "failed" => "cut off, trying again", _ => "" };
            if !step.is_empty() { return format!("{who} {step}"); }
            if c.s("state") == "blocked" { return format!("{who} ? needs you"); }
            let p = c.f("progress");
            format!("{who} working{}", if c.at("progress").is_number() && p > 0.0 { format!(" {}%", p as i64) } else { String::new() })
        }
    }
}
pub fn marks_line(kids: &[Obj], now: i64) -> String { kids.iter().map(|c| mark_of(c, now)).collect::<Vec<_>>().join(" · ") }
pub struct AttemptsShown { pub state: &'static str, pub kind: Option<&'static str>, pub msg: String, pub progress: Option<i64>, pub eta: Option<i64> }
/// What the task shows while its attempts are under way.
pub fn attempts_shown(kids: &[Obj], now: i64) -> AttemptsShown {
    let st: Vec<&str> = kids.iter().map(|c| standing_of(c, now)).collect();
    let needs = kids.iter().zip(&st).any(|(c, s)| *s == "needs" || (*s == "works" && c.s("state") == "blocked"));
    let pct: Vec<f64> = kids.iter().zip(&st).map(|(c, s)| if *s == "passed" || *s == "out" { 100.0 } else if c.at("progress").is_number() { c.f("progress") } else { 0.0 }).collect();
    let etas: Vec<i64> = kids.iter().zip(&st).filter(|(_, s)| **s == "works" || **s == "waits").filter_map(|(c, _)| c.at("eta").is_number().then(|| c.i("eta"))).collect();
    AttemptsShown { state: if needs { "blocked" } else { "working" }, kind: needs.then_some("question"), msg: marks_line(kids, now),
        progress: (!kids.is_empty()).then(|| (pct.iter().sum::<f64>() / kids.len() as f64).round() as i64), eta: etas.into_iter().max() }
}
/// What the task says while it waits for your pick: "Pick one of 3 · the reviewer recommends B".
pub fn pick_line(t: &Obj, kids: Option<&[Obj]>) -> String {
    let owned;
    let kids = match kids { Some(k) => k, None => { owned = attempts_of(t); &owned } };
    let now = crate::clock::now();
    let n = kids.iter().filter(|c| standing_of(c, now) == "passed").count();
    let c = t.at("comparison");
    format!("Pick one of {n}{}", if c.b("pick") { format!(" · the reviewer recommends {}", c.s("pick")) } else if c.b("error") { " · no comparison".into() } else { String::new() })
}
