//! Text on its way: an agent's inbox, and instructions for a Coordinator.
use super::files::*;
use crate::clock::now;
use crate::err::Res;
use crate::json::*;
use std::path::PathBuf;

// ---- the inbox: text for an agent that could not be delivered yet. Never dropped. ----------------------------------
fn inbox_file(id: &str) -> PathBuf { dir_of(id).join("inbox.jsonl") }
fn offset_file(id: &str) -> PathBuf { dir_of(id).join("inbox.offset") }
fn offset(id: &str) -> usize { read_json(&offset_file(id)).map(|v| number(&v) as usize).unwrap_or(0) }
pub fn queue_text(id: &str, text: &str, extra: Obj) -> Res<()> {
    let mut e = obj(json!({ "text": text, "at": now() }));
    for (k, v) in extra { e.insert(k, v); }
    append_line(&inbox_file(id), &Value::Object(e))
}
pub fn pending_text(id: &str) -> Vec<Value> { let all = read_lines(&inbox_file(id)); let off = offset(id).min(all.len()); all[off..].to_vec() }
pub fn take_text(id: &str) -> Res<Vec<Value>> {
    let all = read_lines(&inbox_file(id));
    let off = offset(id).min(all.len());
    write_json(&offset_file(id), &json!(all.len()))?;
    Ok(all[off..].to_vec())
}
/// Put taken text back (the delivery failed after all).
pub fn untake_text(id: &str, count: usize) -> Res<()> { write_json(&offset_file(id), &json!(offset(id).saturating_sub(count))) }

// ---- dispatcher instructions: siblings of the tasks on disk, so a running or failed one survives the board ---------
fn dispatch_dir() -> PathBuf { data_dir().join("dispatch") }
fn num_of(id: &str) -> String { id.rsplit('/').next().unwrap_or("").to_string() }
pub fn new_dispatch(text: &str, extra: Obj) -> Res<Obj> {
    with_lock("seq:dispatch", || {
        let seq = dispatch_dir().join("seq");
        let n = read_json(&seq).map(|v| number(&v) as i64).unwrap_or(0) + 1;
        write_json(&seq, &json!(n))?;
        let mut d = obj(json!({ "id": format!("dispatch/{n}"), "n": n, "text": text, "state": "working", "msg": "", "created": now() }));
        for (k, v) in extra { d.insert(k, v); }
        write_json(&dispatch_dir().join(format!("{n}.json")), &Value::Object(d.clone()))?;
        Ok(d)
    })
}
pub fn get_dispatch(id: &str) -> Option<Obj> { read_obj(&dispatch_dir().join(format!("{}.json", num_of(id)))) }
pub fn set_dispatch(id: &str, patch: Obj) -> Option<Obj> {
    let mut d = get_dispatch(id)?;
    for (k, v) in patch { d.insert(k, v); }
    write_json(&dispatch_dir().join(format!("{}.json", d.t("n"))), &Value::Object(d.clone())).ok()?;
    Some(d)
}
pub fn clear_dispatch(id: &str) { let _ = std::fs::remove_file(dispatch_dir().join(format!("{}.json", num_of(id)))); }
pub fn dispatches() -> Vec<Obj> {
    let names = super::tasks::read_dir_names(&dispatch_dir());
    let mut out: Vec<Obj> = names.iter().filter(|f| f.ends_with(".json") && f[..f.len() - 5].bytes().all(|b| b.is_ascii_digit()) && f.len() > 5)
        .filter_map(|f| read_obj(&dispatch_dir().join(f))).collect();
    out.sort_by_key(|d| d.i("n"));
    // A working one whose helper process is gone did not finish: say so instead of spinning forever.
    for d in out.iter_mut() {
        if d.s("state") == "working" && d.b("pid") && !marker_alive(&json!({ "pid": d.at("pid"), "start": d.at("pidStart") })) && now() - d.i("created") > 3000 {
            let msg = if d.s("msg").is_empty() { "Stopped before it finished".to_string() } else { d.t("msg") };
            if let Some(n) = set_dispatch(d.s("id"), obj(json!({ "state": "error", "msg": msg }))) { *d = n; }
        }
    }
    out
}
