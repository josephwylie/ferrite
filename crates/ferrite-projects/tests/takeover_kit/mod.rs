//! What the tests of taking over older installs share (take_over.rs, take_over_more.rs): a machine's home folder as
//! the old herdr-projects 0.2 left it, as herdr-superwork left it, or both; a stand-in for what is done beyond files;
//! and the take-over run on it.
#![allow(dead_code)]
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use sw::store::move_home::{swap_now, Env};
use sw::store::old_installs::{places, Places};
use sw::take_over::{take_over, Report, Sys};

pub fn put(file: &Path, text: &str) { std::fs::create_dir_all(file.parent().unwrap()).unwrap(); std::fs::write(file, text).unwrap(); }
pub fn read(f: &Path) -> String { std::fs::read_to_string(f).unwrap_or_default() }
pub fn is_link(p: &Path) -> bool { std::fs::symlink_metadata(p).map(|m| m.file_type().is_symlink()).unwrap_or(false) }
pub fn exists(p: &Path) -> bool { std::fs::symlink_metadata(p).is_ok() }
/// Every file under a folder with its text, links followed: what "nothing lost, nothing changed" is checked against.
pub fn all(dir: &Path) -> BTreeMap<String, String> {
    fn walk(d: &Path, base: &Path, out: &mut BTreeMap<String, String>) {
        for e in std::fs::read_dir(d).unwrap() {
            let f = e.unwrap().path();
            if std::fs::symlink_metadata(&f).unwrap().file_type().is_symlink() { out.insert(f.strip_prefix(base).unwrap().display().to_string(), format!("-> {}", std::fs::read_link(&f).unwrap().display())); continue; }
            if f.is_dir() { walk(&f, base, out) } else { out.insert(f.strip_prefix(base).unwrap().display().to_string(), read(&f)); }
        }
    }
    let mut out = BTreeMap::new(); walk(dir, dir, &mut out); out
}

/// What the stand-in was asked to do.
#[derive(Default)]
pub struct Fake { pub calls: RefCell<Vec<String>>, pub reject: bool, pub old_running: bool, pub plugin_on: RefCell<bool> }
pub const LAUNCHER: &str = "#!/bin/sh\nexec '/new/herdr-projects' \"$@\"\n";
impl Sys for Fake {
    fn launcher(&self) -> Option<String> { Some(LAUNCHER.into()) }
    fn stop_old(&self, data: &Path) -> Vec<String> { self.calls.borrow_mut().push(format!("stop {}", data.display())); if self.old_running { vec!["keeper".into(), "page server".into()] } else { vec![] } }
    fn start_new(&self, stopped: &[String]) { self.calls.borrow_mut().push(format!("start {}", stopped.join(","))); }
    fn write_config(&self, file: &Path, next: &str) -> Result<(), String> {
        if self.reject { return Err("herdr would reject the change: no".into()); }
        self.calls.borrow_mut().push("config".into());
        std::fs::write(file, next).map_err(|e| e.to_string())
    }
    fn old_plugin_off(&self) -> bool { let on = *self.plugin_on.borrow(); *self.plugin_on.borrow_mut() = false; on }
}

pub struct Machine { pub home: PathBuf, pub env: Env, pub p: Places }
static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// An empty machine: its home folder, and the variables that put the agent programs' folders in it.
pub fn machine(base: &Path) -> Machine {
    let home = base.join(format!("machine-{}", N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    std::fs::create_dir_all(&home).unwrap();
    let home = std::fs::canonicalize(&home).unwrap();
    let env: Env = [("CLAUDE_CONFIG_DIR", home.join(".claude")), ("CODEX_HOME", home.join(".codex"))].into_iter().map(|(k, v)| (k.to_string(), v.display().to_string())).collect();
    let p = places(&home, &env);
    Machine { home, env, p }
}
pub const OLD_CONFIG: &str = r#"onboarding = false

[ui]
accent = "blue"
tab_bar_right = [{ type = "command", command = "/h/.config/herdr/plugins/github/herdr-projects-b1278ffb803c/target/release/herdr-projects --root /h/.herdr-projects needs-you --line", interval_seconds = 15, timeout_seconds = 5 }]

[ui.sidebar.agents]
rows = [["state_icon", { token = "agent", bold = false, dim = false, rules = [{ contains = "​", bold = true }] }, "state_text"], [{ token = "$hp_sub", dim = true }]]

[ui.sidebar.spaces]
rows = [["state_icon", { token = "workspace", rules = [{ contains = "⠀", bold = true }] }, { token = "branch", dim = true }, "git_status"]]

[[keys.command]]
key = "prefix+a"
type = "plugin_action"
command = "herdr-projects.open-popup"
description = "Projects"

[[keys.command]]
key = "prefix+g"
type = "plugin_action"
command = "other.thing"
"#;
pub const OLD_HOOK: &str = "/h/.config/herdr/plugins/github/herdr-projects-b1278ffb803c/target/release/herdr-projects --root /h/.herdr-projects hook --agent claude 2>/dev/null || true";
impl Machine {
    pub fn run(&self, sys: &Fake) -> Report { take_over(&self.p, &self.env, sys, false, 200, &swap_now) }
    pub fn doctor(&self, sys: &Fake) -> Vec<String> { sw::take_over::look::look(&self.p, &self.env, sys) }
    pub fn at(&self, rest: &str) -> PathBuf { self.home.join(rest) }
    /// What the old herdr-projects 0.2 left: two projects and its own files in ~/.herdr-projects, its settings, herdr's
    /// state folder for it, its entries in herdr's config, its hooks, its skill and the link on the PATH.
    pub fn old_rust(&self) {
        let r = self.at(".herdr-projects");
        put(&r.join("gtm-ai/PROJECT.md"), "+++\nname = \"gtm-ai\"\n+++\n# Instructions\n");
        put(&r.join("gtm-ai/.state/project.json"), "{}");
        put(&r.join("home/PROJECT.md"), "+++\n+++\n# Instructions\n");
        put(&r.join(".ticker.log"), "2026-09-30T18:33:00Z ticker 0.2.34 started (pid 1)\n");
        put(&r.join(".ticker.lock"), r#"{"version":"0.2.34","pid":1}"#);
        put(&r.join(".machines.json"), r#"{"checked":1,"machines":[]}"#);
        put(&r.join("._gtm-ai"), "apple double");
        put(&self.at(".config/herdr-projects/config.toml"), "root = \"~/.herdr-projects\"\n");
        put(&self.at(".config/herdr-projects/owned.json"), "{}");
        put(&self.p.plugin_state.join("herdr-projects/handoff.json"), "{}");
        put(&self.p.herdr_config, OLD_CONFIG);
        let hook = serde_json::json!({ "matcher": "*", "hooks": [{ "type": "command", "command": OLD_HOOK, "timeout": 10 }] });
        put(&self.at(".claude/settings.json"), &serde_json::to_string_pretty(&serde_json::json!({ "theme": "dark", "hooks": { "SessionStart": [hook.clone()], "PostToolUse": [hook.clone(), { "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-own-hook" }] }] } })).unwrap());
        std::fs::create_dir_all(self.at(".claude/skills")).unwrap();
        std::os::unix::fs::symlink(self.at(".config/herdr/plugins/github/herdr-projects-b1278ffb803c/skill/autoproject"), self.at(".claude/skills/autoproject")).unwrap();
        std::fs::create_dir_all(self.at(".local/bin")).unwrap();
        std::os::unix::fs::symlink(self.at(".config/herdr/plugins/github/herdr-projects-b1278ffb803c/target/release/herdr-projects"), self.at(".local/bin/herdr-projects")).unwrap();
    }
    /// What herdr-superwork left: its home folder (a task, its agent's hook settings with the old launcher, the
    /// launcher, its how-to written down, its keeper's file, an artifact, an older copy, its settings and a report,
    /// the record of its own earlier move), its blocks in herdr's config, its how-to folders and herdr's folders for it.
    pub fn superwork(&self) {
        let s = self.at(".herdr-superwork");
        let old = s.display().to_string();
        let launcher = format!("{old}/data/bin/herdr-superwork");
        put(&s.join("data/tasks/herdr-superwork/7/task.json"), &format!(r#"{{"id":"herdr-superwork/7","repo":"herdr-superwork","note":"{old}/data/tasks/herdr-superwork/7/closing-note.md","report":"~/.herdr-superwork/reports/herdr-superwork/7/","other":"{old}-elsewhere/x"}}"#));
        put(&s.join("data/tasks/herdr-superwork/7/hook-agent.json"), &format!(r#"{{"hooks":{{"Stop":[{{"hooks":[{{"type":"command","command":"'{launcher}' turn-ended --id herdr-superwork/7"}}]}}]}}}}"#));
        put(&s.join("data/tasks/herdr-superwork/7/artifacts/check.txt"), &format!("ran {launcher}\n"));
        put(&s.join("data/bin/herdr-superwork"), &format!("#!/bin/sh\nexport HERDR_SUPERWORK_DATA_DIR='{old}/data'\nexec '/old/herdr-superwork' \"$@\"\n"));
        put(&s.join("data/extras.json"), &format!(r#"{{"skillPaths":["{h}/.claude/skills/herdr-superwork","{h}/elsewhere/mine"],"sidebar":true,"asked":true,"webUi":"yes","webUiLinks":[{{"https":8751,"now":"http://127.0.0.1:7327"}}]}}"#, h = self.home.display()));
        put(&s.join("data/keeper.json"), r#"{"pid":1,"version":"0.3.0"}"#);
        put(&s.join("data/coordinator/default/chat/notes.md"), &format!("saved in {old}/data/coordinator/default/chat\n"));
        put(&s.join("data-0.2.0/keeper.json"), &format!(r#"{{"was":"{old}"}}"#));
        put(&s.join("settings/config.json"), r#"{"repos":{"herdr-superwork":{"key":"HSW"}}}"#);
        put(&s.join("reports/herdr-superwork/7/report.md"), &format!("see {old}/reports\n"));
        put(&s.join("moved.json"), &format!(r#"{{"data":{{"from":"{h}/.local/state/herdr-superwork","to":"{old}/data","state":"done","how":"moved"}},"told":true}}"#, h = self.home.display()));
        let cfg = read(&self.p.herdr_config);
        put(&self.p.herdr_config, &format!("{cfg}\n[[keys.command]]\nkey = \"prefix+w\"\ntype = \"plugin_action\"\ncommand = \"herdr-superwork.coordinator\"\n\n# >>> herdr-superwork sidebar (added by the herdr-superwork plugin; `herdr-superwork remove-extras` removes it)\n[ui.sidebar.agents]\nrows = [[\"agent\"]]\n# <<< herdr-superwork sidebar\n\n# >>> herdr-superwork key (added by the herdr-superwork plugin)\n[[keys.command]]\nkey = \"prefix+a\"\ntype = \"plugin_action\"\ncommand = \"herdr-superwork.open\"\n# <<< herdr-superwork key\n"));
        for d in [".claude/skills", ".codex/skills"] { put(&self.at(d).join("herdr-superwork/SKILL.md"), "---\nname: herdr-superwork\n---\nthe board\n"); }
        put(&self.p.plugin_config.join("herdr-superwork/settings.toml"), "x = 1\n");
        std::fs::create_dir_all(self.p.plugin_state.join("herdr-superwork")).unwrap();
    }
}
