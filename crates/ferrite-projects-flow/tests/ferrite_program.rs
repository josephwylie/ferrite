//! Ferrite: what agents and hooks are handed runs the bundled `ferrite-projects`, whichever process wrote it: the
//! board inside Ferrite's app as much as the program itself (ADR 0031). One test, as it sets this process's variables.
use std::path::Path;
use sw_flow::agents::adapter::{hook_args, Hook};
use sw_flow::flow::send::shim;
use sw_flow::json::*;
use sw_flow::store::files::bin;

const BUNDLED: &str = "/Applications/Ferrite.app/Contents/MacOS/ferrite-projects";

#[test]
fn launcher_and_turn_end_hooks_run_the_bundled_program() {
    let home = std::env::temp_dir().join(format!("sw-ferrite-program-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    unsafe {
        std::env::set_var("HERDR_PROJECTS_HOME", &home);
        std::env::set_var("HERDR_PROJECTS_CODEX_TOML", home.join("no-codex-config.toml"));
        std::env::set_var("FERRITE_PROJECTS_BIN", BUNDLED);
    }
    assert_eq!(bin(), Path::new(BUNDLED));

    let launcher = shim();
    let text = std::fs::read_to_string(&launcher).unwrap();
    assert!(text.ends_with(&format!("exec '{BUNDLED}' \"$@\"\n")), "{text}");

    let dir = home.join("task");
    std::fs::create_dir_all(&dir).unwrap();
    let hook = Hook { shim: launcher.display().to_string(), id: "app/1".into(), role: "agent".into(), token: "tok".into(), round: 0, dir, plan: true };
    let ends = format!("'{}' 'turn-ended' '--id' 'app/1' '--role' 'agent' '--token' 'tok'", launcher.display());

    // Claude: a settings file of the task's own (`thread.start --args` hands it to Ferrite's Claude as given).
    let claude = hook_args("claude", &hook).unwrap().unwrap();
    assert_eq!(claude[0], "--settings");
    let settings: Value = serde_json::from_str(&std::fs::read_to_string(&claude[1]).unwrap()).unwrap();
    for event in ["Stop", "StopFailure"] { assert_eq!(settings["hooks"][event][0]["hooks"][0]["command"], ends.as_str()); }
    let plan = settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"].as_str().unwrap();
    assert!(plan.starts_with(&format!("'{}' 'plan-written'", launcher.display())), "{plan}");

    // Codex: one `notify`, which Ferrite's Codex passes to its app-server as a `-c` setting.
    let codex = hook_args("codex", &hook).unwrap().unwrap();
    assert_eq!(codex[0], "-c");
    let notify: Vec<String> = serde_json::from_str(codex[1].strip_prefix("notify=").unwrap()).unwrap();
    assert_eq!(notify[..2], [launcher.display().to_string(), "turn-ended".to_string()]);
    let _ = std::fs::remove_dir_all(&home);
}
