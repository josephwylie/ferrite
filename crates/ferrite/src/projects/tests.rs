//! The snapshot the nav reads, on a scratch root, and the configure
//! action.

use std::path::PathBuf;

use ferrite_core::ThreadId;
use ferrite_projects::paths::{Ctx, Env};
use ferrite_projects::project;
use ferrite_projects::runner::{Cmd, Output, Runner};
use ferrite_projects::thread::{self, Group};

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

#[test]
fn the_snapshot_lists_open_threads_by_need_with_their_sub_lines() {
    let scratch = Scratch::new("model");
    let place = scratch.place(&["Alpha"]);
    let alpha = project::Project::load(&place.root, "alpha").unwrap();
    thread::allocate(&alpha, |t| {
        t.title = "First".into();
        t.status = thread::Status::Open;
        t.last_group = "working".into();
        t.pane_id = "11".into();
        t.state_line = "working · ~40%".into();
        t.activity = "Writing tests".into();
    })
    .unwrap();
    thread::allocate(&alpha, |t| {
        t.title = "Second".into();
        t.status = thread::Status::Open;
        t.last_group = "waiting-on-you".into();
        t.pane_id = "12".into();
        t.state_line = "needs you · blocked".into();
    })
    .unwrap();
    thread::allocate(&alpha, |t| {
        t.title = "Done".into();
        t.status = thread::Status::Resolved;
        t.pane_id = "13".into();
    })
    .unwrap();
    thread::allocate(&alpha, |t| {
        t.title = "Elsewhere".into();
        t.status = thread::Status::Open;
        t.machine = "box".into();
        t.pane_id = "14".into();
    })
    .unwrap();

    let model = Model::load(&place);
    assert!(!model.ticker, "no ticker runs on a scratch root");
    assert_eq!(model.needs_you, None, "no stale count without the ticker");
    let project = &model.projects[0];
    assert_eq!(
        (project.slug.as_str(), project.name.as_str()),
        ("alpha", "Alpha")
    );
    assert_eq!(
        project.summary, "1 need you · 1 working",
        "the line counts the groups the ticker recorded, as herdr-projects' sidebar does"
    );
    let rows: Vec<(&str, Option<ThreadId>, Group, &str, &str)> = project
        .threads
        .iter()
        .map(|row| {
            (
                row.id.as_str(),
                row.thread,
                row.group,
                row.display.as_str(),
                row.sub.as_str(),
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            (
                "t-0002",
                Some(ThreadId::new(12)),
                Group::WaitingOnYou,
                "t-0002 · Second",
                "needs you · blocked"
            ),
            (
                "t-0001",
                Some(ThreadId::new(11)),
                Group::Working,
                "t-0001 · First",
                "~40% · Writing tests"
            ),
            ("t-0004", None, Group::Working, "t-0004 · Elsewhere", ""),
        ],
        "needs you first; another machine's pane is no Thread here"
    );
    assert_eq!(
        model.project_of(ThreadId::new(12)).map(|p| p.slug.as_str()),
        Some("alpha")
    );
    assert!(
        model.project_of(ThreadId::new(13)).is_none(),
        "a resolved thread is no one's"
    );
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
