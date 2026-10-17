//! A stand-in for ssh, for the tests of other machines: `HERDR_PROJECTS_SSH` names a script that ends each call as
//! the test told that machine to (its stdout, stderr and exit code for each request, kept in a folder of files), and
//! writes down what it was asked. Also: a web board's answers from other machines, waited for.
#![allow(dead_code)]
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use sw::json::*;

pub struct Far { pub dir: PathBuf }
const SCRIPT: &str = r#"#!/bin/sh
D='@DIR@'
h="${1##*@}"
cmd="$2"
set -- $2
# Ferrite: the board asks `ferrited projects remote <what> <arg>`.
while [ $# -gt 0 ] && [ "$1" != "ferrited" ]; do shift; done
what="$4"; arg="$5"; act=""
log="$D/asked"
case "$cmd" in *"herdr machine list"*) what="herdr-machines"; log="$D/asked-herdr" ;; esac
[ -n "$arg" ] && act=$(printf %s "$arg" | base64 -d 2>/dev/null | sed -n 's/.*"action":"\([^"]*\)".*/\1/p')
printf '%s %s %s\n' "$h" "$what" "$arg" >> "$log"
f="$D/$h.$what"
[ -n "$act" ] && [ -e "$D/$h.$what.$act.code" ] && f="$D/$h.$what.$act"
[ -e "$f.sleep" ] && sleep "$(cat "$f.sleep")"
[ -e "$f.err" ] && cat "$f.err" >&2
[ -e "$f.out" ] && cat "$f.out"
[ -e "$f.code" ] && exit "$(cat "$f.code")"
exit 0
"#;
impl Far {
    pub fn new(base: &Path) -> Far {
        let dir = base.join("far");
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("ssh");
        std::fs::write(&script, SCRIPT.replace("@DIR@", &dir.display().to_string())).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        unsafe { std::env::set_var("HERDR_PROJECTS_SSH", &script) };
        Far { dir }
    }
    /// How `label` ends a call of `what` ("list", "dispatch", "do", or "do.<action>" for one action only; "herdr-machines"
/// for its herdr's own list of machines).
    pub fn ends(&self, label: &str, what: &str, out: &str, err: &str, code: i32) {
        let f = |ext: &str| self.dir.join(format!("{label}.{what}.{ext}"));
        std::fs::write(f("out"), out).unwrap(); std::fs::write(f("err"), err).unwrap(); std::fs::write(f("code"), code.to_string()).unwrap();
        let _ = std::fs::remove_file(f("sleep"));
    }
    pub fn answers(&self, label: &str, what: &str, j: &Value) { self.ends(label, what, &format!("{j}\n"), "", 0); }
    /// Its tasks, as its door lists them (`version` "": a machine on an older plugin, which sends none).
    pub fn tasks(&self, label: &str, version: &str, tasks: Value) {
        let mut j = json!({ "host": label, "tasks": tasks });
        if !version.is_empty() { j["version"] = json!(version); }
        self.answers(label, "list", &j);
    }
    pub fn hangs(&self, label: &str, what: &str, secs: u32) { self.ends(label, what, "", "", 0); std::fs::write(self.dir.join(format!("{label}.{what}.sleep")), secs.to_string()).unwrap(); }
    /// What it was asked, in order: the machine, the request and what came with it.
    pub fn asked(&self) -> Vec<(String, String, Value)> {
        std::fs::read_to_string(self.dir.join("asked")).unwrap_or_default().lines().map(|l| {
            let p: Vec<&str> = l.splitn(3, ' ').collect();
            let arg = p.get(2).and_then(|a| sw::util::unbase64(a)).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
            (p[0].to_string(), p.get(1).unwrap_or(&"").to_string(), arg)
        }).collect()
    }
}
impl Drop for Far { fn drop(&mut self) { unsafe { std::env::remove_var("HERDR_PROJECTS_SSH") }; } }

/// A task of another machine, as its door lists it.
pub fn theirs(n: i64) -> Value { let now = sw::clock::now(); json!({ "id": format!("far/{n}"), "title": format!("Theirs {n}"), "stage": "todo", "state": "idle", "agent": "", "app": "", "msg": "", "note": "", "at": now, "created": now, "snap": { "folder": "~/far" } }) }
// Ferrite: `answered` and `ask_again`, the web board's machines, are not taken: crates/web is not (ADR 0031).
pub fn obj_of(v: &Value) -> Obj { v.as_object().cloned().unwrap_or_default() }
