//! What happens on the board that an automation can be fired by, and telling that it happened: a task was added,
//! finished and waits for approval, failed its review, went wrong, needs you, was merged. Each is read off the task as it
//! stands, and counted once: what was seen already is kept in the automations' folder.
use crate::json::*;
use crate::notify::event_of;
use crate::store::reviews::review_history;

/// id: its name on the command line. words: after "when a task …". what: what the new task is told happened.
pub struct Event { pub id: &'static str, pub words: &'static str, pub what: &'static str }
pub const EVENTS: [Event; 6] = [
    Event { id: "added", words: "is added", what: "was added" },
    Event { id: "approval", words: "waits for approval", what: "finished and waits for approval" },
    Event { id: "review-failed", words: "failed its review", what: "failed its review" },
    Event { id: "failed", words: "went wrong", what: "went wrong" },
    Event { id: "needs-you", words: "needs you", what: "needs you" },
    Event { id: "merged", words: "is merged", what: "was merged" },
];
fn flat(s: &str) -> String {
    let mut s = s.trim().to_lowercase();
    for p in ["when ", "a ", "task "] { if let Some(r) = s.strip_prefix(p) { s = r.to_string(); } }
    s.split(|c: char| c.is_whitespace() || c == '_').filter(|w| !w.is_empty()).collect::<Vec<_>>().join("-")
}
pub fn event_named(typed: &str) -> Option<&'static Event> { let f = flat(typed); EVENTS.iter().find(|e| e.id == f || flat(e.words) == f || flat(e.what) == f) }
/// What is so of a task right now, one mark per thing that happened: "added", "merged:<when>", "approval:<since>".
pub fn marks_of(t: &Obj) -> Vec<String> {
    let mut out = vec!["added".to_string()];
    match event_of(t) {
        "ready to approve" => out.push(format!("approval:{}", t.t("at"))),
        "needs you" => out.push(format!("needs-you:{}:{}", t.s("stage"), t.t("at"))),
        "failed" => out.push(format!("failed:{}", t.t("at"))),
        _ => {}
    }
    if let Some(rv) = review_history(t).last() { if rv.b("error") || rv.at("pass") == &json!(false) || (rv.at("pass").is_null() && rv.at("recommend") == &json!(false)) { out.push(format!("review-failed:{}:{}", rv.t("round"), rv.t("at"))); } }
    if t.b("mergedAt") { out.push(format!("merged:{}", t.t("mergedAt"))); }
    out
}
/// When the thing a mark stands for happened.
pub fn mark_at(t: &Obj, mark: &str) -> i64 { if mark == "added" { t.i("created") } else { mark.rsplit(':').next().and_then(|x| x.parse().ok()).unwrap_or(0) } }
pub struct Happened { pub event: String, pub task: Obj, pub at: i64 }
/// What happened since the last look, and what is seen now. A task that is gone is forgotten; an archived one is noted
/// and fires nothing.
pub fn new_events(tasks: &[Obj], seen: Option<&Obj>) -> (Vec<Happened>, Obj, bool) {
    let mut next = Obj::new();
    let mut events = vec![];
    for t in tasks {
        let marks = marks_of(t);
        let had = seen.and_then(|s| s.get(t.s("id"))).map(strings);
        next.put(t.s("id"), marks.clone());
        if t.b("archivedAt") { continue; }
        for m in &marks { if !had.as_ref().map(|h| h.contains(m)).unwrap_or(false) { events.push(Happened { event: m.split(':').next().unwrap_or("").into(), task: t.clone(), at: mark_at(t, m) }); } }
    }
    let changed = Value::Object(next.clone()) != seen.cloned().map(Value::Object).unwrap_or(Value::Null);
    (events, next, changed)
}
