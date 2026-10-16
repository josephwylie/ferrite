//! The configure action, on a scratch root.

use std::path::PathBuf;

use ferrite_projects::paths::{Ctx, Env};
use ferrite_projects::project;
use ferrite_projects::runner::{Cmd, Output, Runner};

use super::*;

/// A projects root of its own, removed with it.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("ferrite-projects-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("config")).unwrap();
        Scratch(dir)
    }

    /// The place, with a project made for each name.
    fn place(&self, names: &[&str]) -> Place {
        let root = self.0.join("root");
        for name in names {
            project::create(&root, name, "", Vec::new()).unwrap();
        }
        Place {
            root,
            config_dir: self.0.join("config"),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The CLI as the adapter runs it: `thread list` answers one Thread, 7,
/// working in `/work`; nothing else answers.
struct OneThread;

impl Runner for OneThread {
    fn run(&self, cmd: &Cmd) -> anyhow::Result<Output> {
        let listed = cmd.args.windows(2).any(|pair| pair == ["thread", "list"]);
        Ok(if listed {
            Output {
                code: Some(0),
                stdout: r#"{"result":{"threads":[{"thread":7,"provider":"claude","attention":"idle","cwd":"/work"}]}}"#.into(),
                ..Output::default()
            }
        } else {
            Output {
                code: Some(1),
                ..Output::default()
            }
        })
    }

    fn socket_request(
        &self,
        _: &std::path::Path,
        _: &str,
        _: std::time::Duration,
    ) -> anyhow::Result<String> {
        anyhow::bail!("no socket here")
    }

    fn run_foreground(&self, _: &Cmd, _: &mut dyn FnMut() -> bool) -> anyhow::Result<Option<i32>> {
        anyhow::bail!("no terminal here")
    }
}

/// The configure action is `setup::configure_ferrite`, run in process —
/// no CLI flags to fall out of step with the parser — and says so in
/// herdr-projects' words.
#[test]
fn configure_says_what_it_set_or_why_not() {
    assert_eq!(
        data::configured(Ok(vec!["claude: hooks added".into()])),
        (
            "Projects configured".into(),
            "Progress hooks and the autoproject skill are set.".into(),
            false
        )
    );
    assert_eq!(
        data::configured(Err("no harness is installed".into())),
        (
            "Projects: configure failed".into(),
            "no harness is installed".into(),
            true
        )
    );
}

/// The configure action itself, in a scratch home where Claude is
/// installed: it succeeds, writes the progress hooks and the skill, and
/// says so — where `configure --hooks-only` failed on an unknown flag.
#[test]
fn the_configure_action_sets_the_hooks_and_the_skill() {
    let scratch = Scratch::new("configure");
    let place = scratch.place(&[]);
    let home = scratch.0.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    let env = Env::for_test(&home, &[]);
    let ctx = Ctx {
        env: &env,
        root: place.root.clone(),
        config_dir: place.config_dir.clone(),
        runner: &OneThread,
        detached_ticker: false,
    };
    let (title, body, failed) = data::configured(data::configure_in(&ctx));
    assert_eq!(
        (title.as_str(), body.as_str(), failed),
        (
            "Projects configured",
            "Progress hooks and the autoproject skill are set.",
            false
        )
    );
    let settings = std::fs::read_to_string(home.join(".claude/settings.json")).unwrap();
    assert!(settings.contains("hook --agent claude"), "{settings}");
    assert!(ferrite_projects::setup::skill_source(&place.config_dir)
        .join("SKILL.md")
        .is_file());
}
