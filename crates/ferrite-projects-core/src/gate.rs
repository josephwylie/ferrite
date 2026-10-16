//! The gate. A reviewer describes what it found, each fault tagged by what it does; the board decides from the fixed
//! rules here what goes back to the agent and what may merge. The reviewer's own wish to merge decides nothing. Pure:
//! no files, no herdr. A review from before the gate (a 1-to-5 score and a merge advice) still reads correctly.
use crate::json::*;
use std::collections::BTreeMap;

/// Mildest first: the place in the list is the rank.
pub const SEVERITIES: [&str; 4] = ["trivial", "minor", "major", "critical"];
pub const DEFAULT_GATE: &str = "major";
pub const QUESTION: &str = "Does the change do what the task asked?";
const MAX_FINDINGS: usize = 100;
fn rank(s: &str) -> i64 { SEVERITIES.iter().position(|x| *x == s).map(|i| i as i64).unwrap_or(-1) }
fn line(v: &Value, max: usize) -> String { let s = crate::text::squash(&text(v)); s.chars().take(max).collect() }
/// The bar a review is judged at: `review.mustFix`, or the default for a value the board does not know.
pub fn bar_of(gate: &str) -> &str { if SEVERITIES.contains(&gate) { gate } else { DEFAULT_GATE } }

// ---- what the reviewer wrote -----------------------------------------------------------------------------------------
/// A severity the board does not know is read as major: it goes back at the default gate rather than slipping through.
pub fn read_findings(list: &Value) -> Vec<Obj> {
    list.as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter().filter(|f| f.is_object() && !line(f.at("subject"), 1).is_empty()).map(|f| {
        let sev = text(f.at("severity")).trim().to_lowercase();
        obj(json!({ "id": line(f.at("id"), 16), "severity": if SEVERITIES.contains(&sev.as_str()) { sev } else { "major".into() },
            "subject": line(f.at("subject"), 400), "where": line(f.at("where"), 200), "proof": line(f.at("proof"), 600), "preexisting": f.at("preexisting") == &json!(true) }))
    }).collect()
}
/// The board's rules, applied to what came back. `earlier`: the findings of the round before, on a later round: an
/// earlier finding still open comes back under its id, and anything new stays only if it is critical with proof, or was
/// there before the task. `used`: every finding id this task has had, so a new one never takes an old number.
pub fn settle(meets: Option<bool>, found: &[Obj], earlier: Option<&[Value]>, used: &[String]) -> (Vec<Obj>, i64) {
    let prior: BTreeMap<String, &Value> = earlier.unwrap_or(&[]).iter().filter(|f| !f.b("board")).map(|f| (f.t("id"), f)).collect();
    let mut out: Vec<Obj> = vec![];
    let mut left = 0;
    for f in found {
        // A critical finding nobody showed is a claim, not a proven fault.
        let mut severity = if f.s("severity") == "critical" && f.s("proof").is_empty() { "major".to_string() } else { f.s("severity").to_string() };
        let was = if f.s("id").is_empty() { None } else { prior.get(f.s("id")) };
        if let Some(was) = was {
            if out.iter().any(|o| o.s("id") == was.s("id")) { continue; }
            // An earlier finding may not grow on the way, except into a proven critical one. What it was tagged as stays.
            if severity != "critical" && rank(&severity) > rank(was.s("severity")) { severity = was.s("severity").to_string(); }
            let mut n = f.clone();
            n.put("id", was.s("id")); n.put("severity", severity); n.put("preexisting", was.at("preexisting").clone());
            if was.b("task") { n.put("task", was.at("task").clone()); }
            out.push(n);
        } else if earlier.is_some() && severity != "critical" && !f.b("preexisting") { left += 1; }
        else { let mut n = f.clone(); n.put("id", ""); n.put("severity", severity); out.push(n); }
    }
    // The fixed question answered "no" with nothing listed: that answer is all there is, so it is the one finding.
    if meets == Some(false) && found.is_empty() {
        out.insert(0, obj(json!({ "id": "meets", "severity": "critical", "subject": "The change does not do what the task asked.", "where": "", "proof": "", "preexisting": false, "board": true })));
    }
    // A very long list of the task's own findings is cut only now, from the mildest up. Notes from before stay.
    let own: Vec<usize> = (0..out.len()).filter(|i| !out[*i].b("preexisting")).collect();
    if own.len() > MAX_FINDINGS {
        let mut sorted = own.clone();
        sorted.sort_by_key(|i| -rank(out[*i].s("severity")));
        let gone: Vec<usize> = sorted[MAX_FINDINGS..].to_vec();
        left += gone.len() as i64;
        out = out.into_iter().enumerate().filter(|(i, _)| !gone.contains(i)).map(|(_, f)| f).collect();
    }
    let mut n = used.iter().chain(prior.keys()).map(|id| id.trim_start_matches('F').parse::<i64>().unwrap_or(0)).max().unwrap_or(0).max(0);
    for f in out.iter_mut() { if f.s("id").is_empty() { n += 1; f.put("id", format!("F{n}")); } }
    (out, left)
}

// ---- the result ------------------------------------------------------------------------------------------------------
pub fn is_tagged(rv: &Value) -> bool { rv.at("findings").is_array() && rv.at("meets").is_boolean() }
pub struct Split { pub required: Vec<Value>, pub optional: Vec<Value>, pub preexisting: Vec<Value> }
/// What holds the work back (at or above the gate), what does not, and what was there before the task.
pub fn split(rv: &Value, gate: Option<&str>) -> Split {
    let bar = rank(bar_of(gate.unwrap_or(rv.s("gate"))));
    let all: Vec<Value> = if is_tagged(rv) { rv.arr("findings").to_vec() } else { vec![] };
    let open: Vec<Value> = all.iter().filter(|f| !f.b("preexisting")).cloned().collect();
    Split { required: open.iter().filter(|f| rank(f.s("severity")) >= bar).cloned().collect(),
        optional: open.iter().filter(|f| rank(f.s("severity")) < bar).cloned().collect(),
        preexisting: all.into_iter().filter(|f| f.b("preexisting")).collect() }
}
/// Written on the review when it is made: the gate it was judged at and how it came out.
pub fn stamp(rv: &mut Obj, gate: &str) { rv.put("gate", bar_of(gate)); let pass = split(&Value::Object(rv.clone()), None).required.is_empty(); rv.put("pass", pass); }
/// "Merge", everywhere the board asks whether work may: the gate passed. An old review: 4 or 5, and not "don't merge".
pub fn passes(rv: &Value) -> bool {
    if is_tagged(rv) { return rv.at("pass") == &json!(true); }
    is_int(rv.at("score")) && rv.f("score") >= 4.0 && rv.at("recommend") != &json!(false)
}
pub fn is_int(v: &Value) -> bool { v.as_f64().map(|f| f.fract() == 0.0).unwrap_or(false) }
pub fn is_legacy(rv: &Value) -> bool { !is_tagged(rv) && is_int(rv.at("score")) }
/// Is there anything to hand to the agent?
pub fn sendable(rv: &Value) -> bool { if is_tagged(rv) { rv.arr("findings").iter().any(|f| !f.b("preexisting")) } else { is_legacy(rv) && rv.f("score") < 5.0 } }
/// "1 critical, 2 minor": the task's own faults, worst first, then how many were there before it.
pub fn tally(rv: &Value) -> String {
    if !is_tagged(rv) { return String::new(); }
    let pre = split(rv, None).preexisting.len();
    let open: Vec<&Value> = rv.arr("findings").iter().filter(|f| !f.b("preexisting")).collect();
    let mut parts: Vec<String> = SEVERITIES.iter().rev().filter_map(|s| { let n = open.iter().filter(|f| f.s("severity") == *s).count(); (n > 0).then(|| format!("{n} {s}")) }).collect();
    if pre > 0 { parts.push(format!("{pre} pre-existing")); }
    parts.join(", ")
}
pub fn word(rv: &Value) -> &'static str { if passes(rv) { "Pass" } else { "Fail" } }
/// "Pass · 2 minor", "Fail · 1 critical, 1 major". An old review keeps its own words: "4/5, says merge".
pub fn result(rv: &Value, verb: &str) -> String {
    if is_tagged(rv) { let t = tally(rv); return format!("{}{}", word(rv), if t.is_empty() { String::new() } else { format!(" · {t}") }); }
    let advice = match rv.at("recommend").as_bool() { Some(true) => format!(", says {verb}"), Some(false) => format!(", says don't {verb}"), None => String::new() };
    format!("{}/5{advice}", rv.t("score"))
}
pub fn sentence(rv: &Value, verb: &str) -> String { if is_tagged(rv) { format!("Review: {}.", result(rv, verb)) } else { format!("Reviewed {}.", result(rv, verb)) } }
/// The board's own review words at the start of a task's message, for a screen that shows them elsewhere.
pub fn strip_sentence(msg: &str) -> String {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"^(Approve to merge\?\s*)?(?:Reviewed [1-5]/5(?:, says (?:don't )?(?:merge|accept))?|Review: (?:Pass|Fail)(?: · [a-z0-9, -]+)?)\.\s*").unwrap());
    RE.replace(msg, "").into_owned()
}
pub fn has_sentence(msg: &str) -> bool { strip_sentence(msg) != msg }

// ---- words for the agent, the pull request and the board ----------------------------------------------------------------
pub fn finding_line(f: &Value) -> String { format!("[{}] {}{}", f.s("severity"), f.s("subject"), if f.s("where").is_empty() { String::new() } else { format!(" ({})", f.s("where")) }) }
fn with_proof(f: &Value) -> String { format!("- {}{}", finding_line(f), if f.s("proof").is_empty() { String::new() } else { format!("\n  Proof: {}", f.s("proof")) }) }
/// A review an older version saved may name the task that version added for a fault from before (`task`).
pub fn went_to(f: &Value) -> String { if f.b("task") { format!("task {}", f.t("task")) } else { String::new() } }
fn filed_as(f: &Value) -> String { let w = went_to(f); format!("- {}{}", finding_line(f), if w.is_empty() { w } else { format!(" → {w}") }) }
/// The review as plain text: its summary and what it found. `all`: the human sends it by hand, so nothing is optional.
pub fn findings_text(rv: &Value, all: bool) -> String {
    if !is_tagged(rv) {
        let probs = rv.arr("problems");
        return format!("{}{}", rv.t("text"), if probs.is_empty() { String::new() } else { format!("\n\nProblems it found:\n{}", probs.iter().map(|p| format!("- {}", text(p))).collect::<Vec<_>>().join("\n")) });
    }
    let s = split(rv, None);
    let (required, optional) = if all && s.required.is_empty() { ([s.required.clone(), s.optional.clone()].concat(), vec![]) } else { (s.required.clone(), s.optional.clone()) };
    let join = |xs: &[Value], f: fn(&Value) -> String| xs.iter().map(f).collect::<Vec<_>>().join("\n");
    [rv.t("text"),
        if required.is_empty() { String::new() } else { format!("Required: fix each of these.\n{}", join(&required, with_proof)) },
        if optional.is_empty() { String::new() } else { format!("Optional: these do not hold the work back. Fix one only if it is quick and safe; otherwise leave it.\n{}", join(&optional, with_proof)) },
        if s.preexisting.is_empty() { String::new() } else { format!("Not yours to fix: these were there before your work. They are notes for the human. Leave them.\n{}", join(&s.preexisting, filed_as)) },
    ].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n\n")
}
pub fn review_message(rv: &Value, all: bool) -> String {
    if !is_tagged(rv) { return format!("A reviewer went through your change and rated it {} out of 5.\n\n{}\n\nAddress these, then finish again.", rv.t("score"), findings_text(rv, false)); }
    format!("A reviewer went through your change. Result: {}.\n\n{}\n\nFix what is required, then finish again. The next review checks that each of these is fixed and that the fixes broke nothing.", result(rv, "merge"), findings_text(rv, all))
}
/// What a pull request says about a review: every finding, those below the gate marked as not required.
pub fn pr_comment(rv: &Value, round: i64) -> String {
    let s = split(rv, None);
    let item = |f: &Value| {
        let w = went_to(f);
        format!("- **{}**: {}{}{}{}", f.s("severity"), f.s("subject"), if f.s("where").is_empty() { String::new() } else { format!(" (`{}`)", f.s("where")) },
            if f.s("proof").is_empty() { String::new() } else { format!("\n  Proof: {}", f.s("proof")) },
            if f.b("preexisting") && !w.is_empty() { format!("\n  {}{}.", w[..1].to_uppercase(), &w[1..]) } else { String::new() })
    };
    let group = |title: &str, xs: &[Value]| if xs.is_empty() { String::new() } else { format!("**{title}**\n{}", xs.iter().map(item).collect::<Vec<_>>().join("\n")) };
    let left = rv.i("left");
    [format!("**Review{} · {}**", if round > 0 { format!(" {round}") } else { String::new() }, result(rv, "merge")),
        format!("Does the change do what the task asked? {} Gate: {} and worse must be fixed.", if rv.b("meets") { "Yes." } else { "No." }, rv.s("gate")),
        rv.t("text"), group("Required", &s.required), group("Not required (below the gate)", &s.optional), group("Not required (there before this change)", &s.preexisting),
        if left > 0 { format!("{left} smaller finding{} noticed in this later round {} left out.", if left == 1 { "" } else { "s" }, if left == 1 { "was" } else { "were" }) } else { String::new() },
        if rv.arr("findings").is_empty() { "No findings.".into() } else { String::new() },
    ].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n\n")
}
/// Is this the same finding, word for word? The same thing said about the same place.
pub fn same_fault(a: &Value, b: &Value) -> bool {
    let plain = |x: &str| x.to_lowercase().split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    if crate::text::squash(a.s("where")) != crate::text::squash(b.s("where")) { return false; }
    let p = plain(a.s("subject"));
    !p.is_empty() && p == plain(b.s("subject"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gate_rules() {
        let found = read_findings(&json!([{ "severity": "critical", "subject": "No proof" }, { "severity": "minor", "subject": "Nit" }, { "severity": "odd", "subject": "Odd" }]));
        let (f, left) = settle(Some(true), &found, None, &[]);
        assert_eq!(left, 0);
        assert_eq!(f[0].s("severity"), "major");
        assert_eq!(f[2].s("severity"), "major");
        assert_eq!(f[2].s("id"), "F3");
        let mut rv = obj(json!({ "meets": true, "findings": f, "text": "ok" }));
        stamp(&mut rv, "critical");
        let rv = Value::Object(rv);
        assert!(passes(&rv));
        assert_eq!(result(&rv, "merge"), "Pass · 2 major, 1 minor");
        assert_eq!(strip_sentence("Review: Pass · 2 minor. Approve?"), "Approve?");
        let old = json!({ "score": 4, "recommend": true });
        assert_eq!(result(&old, "merge"), "4/5, says merge");
        assert!(passes(&old));
    }
}
