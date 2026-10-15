//! `doctor`: what is installed, where things resolve, and whether it fits.

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use crate::herdr::{self, Herdr};
use crate::paths::{self, Ctx, Env, SessionFlags};
use crate::project;
use crate::runner::{Cmd, Runner};

const TOOL_TIMEOUT: Duration = Duration::from_secs(10);

/// GitHub hosts of the `origin` remotes of this directory's repository and
/// of each project's local repositories, with one `owner/repo` on each.
fn github_hosts(root: &Path, runner: &dyn Runner) -> std::collections::BTreeMap<String, String> {
    let mut dirs: Vec<String> = std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).into_iter().collect();
    for slug in project::list_slugs(root) {
        if let Ok((settings, _)) = project::Project::load(root, &slug).and_then(|p| p.read_project_md()) {
            dirs.extend(settings.repos.into_iter().filter(|r| r.machine.is_none()).map(|r| r.path));
        }
    }
    let mut hosts = std::collections::BTreeMap::new();
    for dir in dirs {
        let Ok(out) = runner.run(&Cmd::new("git", TOOL_TIMEOUT).args(["-C", &dir, "remote", "get-url", "origin"])) else {
            continue;
        };
        if let Some(remote) = out.success().then(|| crate::pr::parse_remote(&out.stdout)).flatten() {
            hosts.entry(remote.host).or_insert(remote.repo);
        }
    }
    hosts
}

/// The report, and whether every required check passed. With `fix`, repairs
/// what the binary owns: priming files, `uploads/`, the absolute binary path
/// they carry, and the skill link for a configured harness. Never edits
/// another plugin's entries.
/// Ferrite: returns the text instead of printing it, for the Projects view;
/// the command link, Herdr update and sidebar checks are gone (the binary
/// ships with Ferrite, Ferrite updates itself, and Projects are drawn natively).
pub fn run(ctx: &Ctx, session: &SessionFlags, fix: bool) -> (String, bool) {
    let skill = crate::setup::skill_source(&ctx.config_dir);
    report(ctx.env, &ctx.root, &ctx.config_dir, session, ctx.runner, fix, Some(&skill))
}

/// Ferrite: the shipped `ferrite-projects` ([`paths::binary`]) and its
/// version: this build's when it is the running executable, else its own
/// `--version`, since the Projects view runs this inside `ferrite`.
fn shipped(runner: &dyn Runner) -> (String, String) {
    let binary = match paths::binary() {
        Ok(binary) => binary,
        Err(error) => return (format!("unknown ({error:#})"), crate::VERSION.into()),
    };
    let running = std::env::current_exe().and_then(std::fs::canonicalize).is_ok_and(|exe| exe == binary);
    let version = if running {
        crate::VERSION.to_string()
    } else {
        match runner.run(&Cmd::new(binary.to_string_lossy(), TOOL_TIMEOUT).arg("--version")) {
            Ok(out) if out.success() => out.stdout.split_whitespace().last().unwrap_or("").to_string(),
            Ok(out) => format!("unknown ({})", out.error_text()),
            Err(error) => format!("unknown ({error:#})"),
        }
    };
    (binary.display().to_string(), version)
}

fn report(
    env: &Env,
    root: &Path,
    config_dir: &Path,
    session: &SessionFlags,
    runner: &dyn Runner,
    fix: bool,
    skill: Option<&Path>,
) -> (String, bool) {
    let mut out = String::new();
    let mut healthy = true;
    let mut check = |out: &mut String, ok: Option<bool>, label: &str, detail: String| {
        let mark = match ok {
            Some(true) => "ok  ",
            Some(false) => {
                healthy = false;
                "FAIL"
            }
            None => "warn",
        };
        let _ = writeln!(out, "[{mark}] {label}: {detail}");
    };

    // Ferrite: the shipped binary, also when this runs inside `ferrite`.
    let (binary, version) = shipped(runner);
    let _ = writeln!(out, "binary:     {binary}");
    let _ = writeln!(out, "version:    {version}");
    let _ = writeln!(out, "root:       {}", root.display());
    let _ = writeln!(out, "config dir: {}", config_dir.display());
    let _ = writeln!(out);

    let bin = env.herdr_bin();
    match herdr::version(&bin, runner) {
        Ok(version) if version >= herdr::MIN_VERSION => {
            check(&mut out, Some(true), "ferrite", format!("{version} ({bin})"))
        }
        Ok(version) => check(
            &mut out,
            Some(false),
            "ferrite",
            format!("{version} ({bin}); {} or later is required", herdr::MIN_VERSION),
        ),
        Err(error) => check(&mut out, Some(false), "ferrite", format!("{error:#}")),
    }

    // Ferrite: the Server at `$FERRITE_SOCKET`, where Herdr had sessions.
    match paths::resolve_session(session, env, runner) {
        Ok(found) => {
            let reachable = Herdr::new(&bin, &found.socket, runner).reachable();
            check(
                &mut out,
                if reachable { Some(true) } else { None },
                "server",
                format!("{}{}", found.socket.display(), if reachable { "" } else { "; not reachable" }),
            );
        }
        Err(error) => check(&mut out, Some(false), "server", format!("{error:#}")),
    }

    for (tool, args, required) in [
        ("git", vec!["--version"], true),
        ("ssh", vec!["-V"], true),
        ("rsync", vec!["--version"], false),
        ("gh", vec!["--version"], false),
    ] {
        let result = runner.run(&Cmd::new(tool, TOOL_TIMEOUT).args(args));
        match result {
            Ok(o) if o.success() => {
                let text = if o.stdout.trim().is_empty() { &o.stderr } else { &o.stdout };
                let line = text.lines().next().unwrap_or("").trim().to_string();
                check(&mut out, Some(true), tool, line);
            }
            Ok(o) => check(&mut out, required.then_some(false), tool, o.error_text()),
            Err(error) => check(&mut out, required.then_some(false), tool, format!("{error:#}")),
        }
    }
    // `gh` per GitHub host the repositories use: the one in this directory
    // and each project's local ones. Off `github.com` the check runs as the
    // pull request follow-up does, with `GH_HOST` set.
    let hosts = github_hosts(root, runner);
    if hosts.is_empty() || hosts.contains_key("github.com") {
        match runner.run(&Cmd::new("gh", TOOL_TIMEOUT).args(["auth", "status"])) {
            Ok(o) if o.success() => check(&mut out, Some(true), "gh auth", "logged in".into()),
            Ok(o) => check(
                &mut out,
                None,
                "gh auth",
                format!(
                    "{}; pull request follow-up will not work",
                    o.error_text().lines().next().unwrap_or("not logged in")
                ),
            ),
            Err(_) => check(&mut out, None, "gh auth", "gh is not installed".into()),
        }
    }
    for (host, repo) in hosts.iter().filter(|(host, _)| host.as_str() != "github.com") {
        let label = format!("github {host}");
        let result = runner.run(&crate::pr::gh(host).args(["api", "user", "--jq", ".login"]));
        match result {
            Ok(o) if o.success() => check(&mut out, Some(true), &label, format!("reachable as {} (GH_HOST={host}, used for {repo})", crate::pr::sanitize(o.stdout.trim()))),
            Ok(o) => check(
                &mut out,
                None,
                &label,
                format!(
                    "gh cannot reach {host} ({}); pull requests of {repo} are not followed. Try `GH_HOST={host} gh auth status`",
                    o.error_text().lines().next().unwrap_or("failed")
                ),
            ),
            Err(error) => check(&mut out, None, &label, format!("gh cannot reach {host} ({error:#}); pull requests of {repo} are not followed")),
        }
    }

    if root.is_dir() {
        let count = project::list_slugs(root).len();
        check(&mut out, Some(true), "root", format!("{count} project(s)"));
    } else {
        check(
            &mut out,
            None,
            "root",
            "does not exist yet; `new` creates it".into(),
        );
    }

    match crate::ticker::lock_state(root) {
        crate::ticker::LockState::Free => check(&mut out, None, "ticker", "not running".into()),
        crate::ticker::LockState::Held(info) => check(
            &mut out,
            Some(true),
            "ticker",
            format!(
                "running, version {} (this binary: {}), root {}",
                info.version,
                version,
                info.root
            ),
        ),
    }

    // Every project's priming files, and the binary path they carry: a
    // `plugin link` from another checkout or a moved plugin root breaks them
    // silently, and `--fix` rewrites them.
    let prefix = crate::coordinator::current_prefix(root).unwrap_or_default();
    let slugs = project::list_slugs(root);
    for slug in &slugs {
        let Ok(project) = project::Project::load(root, slug) else {
            continue;
        };
        let label = format!("files {slug}");
        let problems = project::priming_problems(&project, &prefix);
        if problems.is_empty() {
            check(&mut out, Some(true), &label, "AGENTS.md, CLAUDE.md link and uploads/ are in place".into());
        } else if fix {
            match project::write_priming(&project, &prefix) {
                Ok(()) => check(&mut out, Some(true), &label, format!("fixed: {}", problems.join("; "))),
                Err(error) => check(&mut out, Some(false), &label, format!("could not fix ({error:#}): {}", problems.join("; "))),
            }
        } else {
            check(&mut out, None, &label, format!("{}; `doctor --fix` repairs this", problems.join("; ")));
        }
        for other in &slugs {
            if other > slug && crate::names::collide(slug, other) {
                check(&mut out, Some(false), &format!("names {slug}"), format!("its agent names collide with `{other}` after truncation to 32 characters; rename one project"));
            }
        }
    }

    // Profiles: config.toml loads, and each project's defaults exist and are
    // allowed, or its threads and coordinator do not start.
    match crate::profiles::load(config_dir) {
        Err(error) => check(&mut out, Some(false), "profiles", format!("{error:#}")),
        Ok(config) => {
            let detected = crate::profiles::detect(env);
            check(&mut out, Some(true), "profiles", format!("{} of yours; built-ins (installed and signed in): {}", config.profiles.len(), if detected.is_empty() { "none".to_string() } else { detected.join(", ") }));
            for slug in &slugs {
                let Ok(project) = project::Project::load(root, slug) else {
                    continue;
                };
                let (Ok((settings, _)), Ok(safety)) = (project.read_project_md(), project.safety(config_dir)) else {
                    continue;
                };
                for role in [crate::profiles::Role::Thread, crate::profiles::Role::Coordinator] {
                    if let Err(error) = crate::profiles::resolve(&config, &safety, &settings, role, None, slug) {
                        check(&mut out, Some(false), &format!("profiles {slug}"), format!("{} {error:#}", role.default_key()));
                    }
                }
            }
        }
    }

    // Orphans, as `sweep --dry-run` would list them.
    {
        let ctx = Ctx { env, root: root.to_path_buf(), config_dir: config_dir.to_path_buf(), runner, detached_ticker: false };
        for slug in &slugs {
            let Ok(project) = project::Project::load(root, slug) else {
                continue;
            };
            let orphans = crate::sweep::find(&ctx, &project);
            if !orphans.is_empty() {
                let list: Vec<String> = orphans.iter().map(|o| o.describe()).collect();
                check(&mut out, None, &format!("sweep {slug}"), format!("{}; `sweep {slug}` removes them", list.join("; ")));
            }
        }
    }

    for slug in &slugs {
        let Ok(project) = project::Project::load(root, slug) else {
            continue;
        };
        let label = format!("project {slug}");
        let none = format!("no coordinator running (start any agent in {}, or `open {slug}`)", project.dir().display());
        // With no usable record, the agents the ticker would discover: the
        // ones working in the project folder in this session.
        let (record, mut notes) = match project.coordinator() {
            Some(record) if Path::new(&record.socket).exists() => (Some(record), Vec::new()),
            Some(record) => (None, vec![format!("recorded socket {} no longer exists", record.socket)]),
            None => (None, Vec::new()),
        };
        let socket = match (&record, paths::resolve_session(session, env, runner)) {
            (Some(record), _) => record.socket.clone(),
            (None, Ok(found)) => found.socket.to_string_lossy().into_owned(),
            (None, Err(_)) => {
                notes.push(none);
                check(&mut out, Some(true), &label, format!("{}; {}", project.status(), notes.join("; ")));
                continue;
            }
        };
        let herdr = Herdr::new(&bin, &socket, runner);
        match (herdr.pane_list(), herdr.agent_list()) {
            (Ok(panes), Ok(agents)) => {
                let dir = project.canonical_dir().to_string_lossy().into_owned();
                let coordinators: Vec<String> = agents
                    .iter()
                    .filter(|a| a.works_in(&dir))
                    .map(|a| if a.name.starts_with("hpc-") { format!("{} ({})", a.pane_id, a.agent) } else { format!("{} ({}, started by hand)", a.pane_id, a.agent) })
                    .collect();
                if let Some(record) = &record {
                    let workspace = crate::coordinator::workspace_open(record, &panes);
                    notes.push(format!("socket {}; workspace {} {}", record.socket, record.workspace_id, if workspace { "open" } else { "closed" }));
                }
                if coordinators.is_empty() {
                    notes.push(none);
                } else {
                    notes.push(format!("coordinator: {}", coordinators.join(", ")));
                    if record.is_none() {
                        notes.push("the ticker records it on its next tick".into());
                    }
                }
                check(
                    &mut out,
                    if coordinators.is_empty() && record.is_some() { None } else { Some(true) },
                    &label,
                    format!("{}; {}", project.status(), notes.join("; ")),
                );
            }
            (Err(error), _) | (_, Err(error)) => check(&mut out, None, &label, format!("session at {socket} unreachable: {error}")),
        }
    }

    // Scheduled routines whose last due run did nothing: no coordinator ran.
    for slug in &slugs {
        let Ok(project) = project::Project::load(root, slug) else {
            continue;
        };
        let states = crate::steps::load_state(&project).routines;
        let now = jiff::Zoned::now();
        let skipped: Vec<String> = crate::routine::load_all(&project)
            .0
            .iter()
            .filter(|r| r.enabled && states.get(&r.name).is_some_and(|s| s.no_coordinator > 0))
            .map(|r| format!("{}: {}", r.name, crate::routine::when_text(r, states.get(&r.name), &now)))
            .collect();
        if !skipped.is_empty() {
            check(&mut out, None, &format!("routines {slug}"), format!("{}; routines run only while a coordinator runs", skipped.join("; ")));
        }
    }

    // Hooks: ours in place and pointing at this binary; the standalone
    // agent-progress plugin's hooks gone (never edited by this plugin).
    let journal = crate::setup::load_journal(config_dir);
    for agent in crate::setup::AGENTS {
        let file = crate::setup::hook_file(env, agent, None, None);
        let Ok(Some(text)) = crate::setup::read(&file) else {
            continue;
        };
        let label = format!("hooks {agent}");
        if crate::setup::has_agent_progress_hooks(&text) {
            let launcher = text
                .split('"')
                .find(|s| s.contains("herdr-progress") && s.contains(" hook --agent "))
                .and_then(|s| s.split(" hook --agent ").next())
                .unwrap_or("herdr-progress")
                .to_string();
            check(&mut out, None, &label, format!("{} still runs the standalone agent-progress hooks; run `{launcher} unconfigure`, then `herdr plugin disable agent-progress`", file.display()));
        }
        let key = file.to_string_lossy().into_owned();
        let binary = crate::paths::binary().unwrap_or_default();
        let expected = crate::setup::hook_command(&binary, root, agent);
        match journal.get(&key) {
            None => check(&mut out, None, &label, "not configured; `configure` installs the progress hooks".into()),
            Some(_) if text.contains(&expected) => check(&mut out, Some(true), &label, format!("{} runs this binary", file.display())),
            Some(_) if fix => {
                let options = crate::setup::ConfigureOptions { clients: vec![agent.to_string()], claude_home: None, codex_home: None, dry_run: false, hooks: true, skill: skill.map(Path::to_path_buf) };
                let ctx = Ctx { env, root: root.to_path_buf(), config_dir: config_dir.to_path_buf(), runner, detached_ticker: false };
                match crate::setup::configure(&ctx, &options) {
                    Ok(_) => check(&mut out, Some(true), &label, format!("fixed: {} now runs this binary", file.display())),
                    Err(error) => check(&mut out, Some(false), &label, format!("could not fix: {error:#}")),
                }
            }
            Some(_) => check(&mut out, None, &label, format!("{} runs another binary or root; `doctor --fix` rewrites it", file.display())),
        }
    }

    // The bundled skill, linked where each installed harness looks for skills.
    // `--fix` links it only for a harness the user already ran `configure`
    // for (its hooks or the link are journaled), so an update alone brings a
    // newly bundled skill to existing users without a new opt-in.
    // Ferrite: the bundled skill is built in and written out under the config
    // dir by `configure`; an update of Ferrite leaves an older copy there.
    let bundled = crate::setup::skill_source(config_dir);
    if skill == Some(bundled.as_path()) && bundled.exists() && !crate::setup::skill_current(&bundled) {
        match fix {
            true => match crate::setup::install_skill(config_dir) {
                Ok(_) => check(&mut out, Some(true), "skill files", format!("fixed: {} has this binary's `{}` skill", bundled.display(), crate::setup::SKILL)),
                Err(error) => check(&mut out, Some(false), "skill files", format!("could not fix: {error:#}")),
            },
            false => check(&mut out, None, "skill files", format!("{} is not this binary's `{}` skill; `doctor --fix` rewrites it", bundled.display(), crate::setup::SKILL)),
        }
    }
    if let Some(source) = skill.map(Path::to_path_buf).filter(|s| s.join("SKILL.md").is_file() || *s == bundled) {
        for agent in ["claude", "codex"] {
            if !crate::setup::hook_file(env, agent, None, None).parent().is_some_and(Path::is_dir) {
                continue;
            }
            let link = crate::setup::skill_link(env, agent, None);
            let label = format!("skill {agent}");
            let journaled = journal.contains_key(&*link.to_string_lossy());
            let opted_in = journaled || journal.get(&*crate::setup::hook_file(env, agent, None, None).to_string_lossy()).is_some_and(|o| o.kind == "hooks");
            let state = crate::setup::skill_state(&link, &source);
            let repairable = matches!(state, crate::setup::SkillState::Missing) || matches!(state, crate::setup::SkillState::Elsewhere(_) if journaled);
            if fix && opted_in && repairable {
                let options = crate::setup::ConfigureOptions { clients: vec![agent.to_string()], claude_home: None, codex_home: None, dry_run: false, hooks: false, skill: Some(source.clone()) };
                let ctx = Ctx { env, root: root.to_path_buf(), config_dir: config_dir.to_path_buf(), runner, detached_ticker: false };
                // Ferrite: the bundled skill is written out before it is linked.
                let installed = if source == bundled { crate::setup::install_skill(config_dir).map(drop) } else { Ok(()) };
                match installed.and_then(|()| crate::setup::configure(&ctx, &options)) {
                    Ok(_) => check(&mut out, Some(true), &label, format!("fixed: {} now links the bundled `{}` skill", link.display(), crate::setup::SKILL)),
                    Err(error) => check(&mut out, Some(false), &label, format!("could not fix: {error:#}")),
                }
                continue;
            }
            let repair = if opted_in { "`doctor --fix`" } else { "`configure`" };
            match state {
                crate::setup::SkillState::Ours => check(&mut out, Some(true), &label, format!("{} links the bundled `{}` skill", link.display(), crate::setup::SKILL)),
                crate::setup::SkillState::Missing => check(&mut out, None, &label, format!("{} is missing; {repair} links the bundled skill", link.display())),
                crate::setup::SkillState::Elsewhere(old) if journaled => check(&mut out, None, &label, format!("{} links {}, another checkout; {repair} relinks it", link.display(), old.display())),
                _ => check(&mut out, None, &label, format!("{} is not this plugin's link, so the bundled skill is not installed; move it away and run {repair}", link.display())),
            }
        }
    }

    // Machines that projects use need an SSH target for report and library copies.
    let mut machines = std::collections::BTreeSet::new();
    for slug in project::list_slugs(root) {
        let Ok(project) = project::Project::load(root, &slug) else {
            continue;
        };
        if let Ok((settings, _)) = project.read_project_md() {
            machines.extend(settings.repos.into_iter().filter_map(|r| r.machine));
        }
        machines.extend(crate::thread::list(&project).into_iter().filter(|t| t.is_remote() && t.status != crate::thread::Status::Resolved).map(|t| t.machine));
    }
    for machine in machines {
        match crate::remote::ssh_target(runner, &bin, config_dir, &machine) {
            Ok(target) => check(&mut out, Some(true), &format!("machine {machine}"), format!("ssh target {target}")),
            Err(error) => check(&mut out, Some(false), &format!("machine {machine}"), format!("{error:#}")),
        }
    }

    (out, healthy)
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::runner::fake::{FakeRunner, fail, ok};

    /// Ferrite: `ferrite --version` (`version` as Ferrite prints it) at the
    /// default binary, and the tools.
    fn runner_with_ferrite(version: herdr::Version) -> FakeRunner {
        let runner = FakeRunner::new();
        runner.on(&format!("{} --version", Env::for_test(Path::new("/"), &[]).herdr_bin()), ok(&format!("ferrite {version}\n")));
        runner.on("git --version", ok("git version 2.50.0\n"));
        runner.on("ssh -V", ok(""));
        runner.on("rsync --version", ok("rsync 3\n"));
        runner.on("gh --version", ok("gh version 2\n"));
        runner.on("gh auth status", fail(1, "not logged in"));
        runner
    }

    #[test]
    fn old_ferrite_fails_and_names_the_minimum() {
        let home = tempfile::tempdir().unwrap();
        let env = Env::for_test(home.path(), &[]);
        let runner = runner_with_ferrite(herdr::Version(0, 0, 0));
        let (text, healthy) = report(
            &env,
            &home.path().join("root"),
            &home.path().join("cfg"),
            &SessionFlags::default(),
            &runner,
            false,
            None,
        );
        assert!(!healthy);
        assert!(text.contains("[FAIL] ferrite: 0.0.0"), "{text}");
        assert!(text.contains(&format!("{} or later", herdr::MIN_VERSION)));
    }

    #[test]
    fn new_ferrite_passes_and_warnings_do_not_fail() {
        let home = tempfile::tempdir().unwrap();
        let env = Env::for_test(home.path(), &[]);
        let runner = runner_with_ferrite(herdr::MIN_VERSION);
        let root = home.path().join("root");
        let (text, healthy) = report(
            &env,
            &root,
            &home.path().join("cfg"),
            &SessionFlags::default(),
            &runner,
            false,
            None,
        );
        assert!(healthy, "{text}");
        assert!(text.contains("[warn] gh auth"));
        assert!(text.contains("[warn] server: ") && text.contains("; not reachable"), "{text}");
        for gone in ["] update:", "] command:", "] sidebar:", "] session:"] {
            assert!(!text.contains(gone), "{gone} is not checked in Ferrite: {text}");
        }
        assert!(text.contains("[warn] root"));
        assert!(text.contains(&format!("root:       {}", root.display())));
        assert!(!root.exists(), "doctor must not create the root");
    }

    #[test]
    fn missing_priming_files_are_reported_and_fixed() {
        let home = tempfile::tempdir().unwrap();
        let env = Env::for_test(home.path(), &[]);
        let runner = runner_with_ferrite(herdr::MIN_VERSION);
        let root = home.path().join("root");
        // A project made before AGENTS.md existed: no priming files, no uploads/.
        let project = project::create(&root, "demo", "", vec![]).unwrap();
        std::fs::remove_dir(project.dir().join("uploads")).unwrap();
        let flags = SessionFlags::default();
        let (text, _) = report(&env, &root, &home.path().join("cfg"), &flags, &runner, false, None);
        assert!(text.contains("[warn] files demo: AGENTS.md is missing; CLAUDE.md is not a link to AGENTS.md; uploads/ is missing; `doctor --fix` repairs this"), "{text}");
        assert!(!project.dir().join("AGENTS.md").exists());

        let (text, _) = report(&env, &root, &home.path().join("cfg"), &flags, &runner, true, None);
        assert!(text.contains("[ok  ] files demo: fixed: AGENTS.md is missing"), "{text}");
        assert!(project.dir().join("AGENTS.md").is_file());
        assert!(project.dir().join("uploads").is_dir());
        let (text, _) = report(&env, &root, &home.path().join("cfg"), &flags, &runner, false, None);
        assert!(text.contains("[ok  ] files demo: AGENTS.md, CLAUDE.md link and uploads/ are in place"), "{text}");
    }

    /// exe.dev VMs: `origin` is on `github.localhost` and plain `gh` is logged
    /// in nowhere. Doctor asks that host, not `gh auth status`.
    #[test]
    fn a_repository_off_github_com_is_checked_on_its_own_host() {
        let home = tempfile::tempdir().unwrap();
        let env = Env::for_test(home.path(), &[]);
        let root = home.path().join("root");
        let repo = home.path().join("app");
        std::fs::create_dir_all(&repo).unwrap();
        project::create(&root, "demo", "", vec![project::Repo { path: repo.to_string_lossy().into_owned(), machine: None }]).unwrap();
        let on_host = |cmd: &Cmd| cmd.program == "gh" && cmd.env.contains(&("GH_HOST".into(), "github.localhost".into()));
        for reachable in [true, false] {
            let runner = FakeRunner::new();
            runner.on_fn(|cmd| cmd.display().ends_with("/app remote get-url origin"), |_| Ok(ok("http://github.localhost/eliasstravik/app.git\n")));
            if reachable {
                runner.on_fn(on_host, |_| Ok(ok("eliasstravik\n")));
            }
            runner.on("gh api user", fail(1, "HTTP 401: Requires authentication"));
            let ferrite = format!("{} --version", env.herdr_bin());
            let version = format!("ferrite {}\n", herdr::MIN_VERSION);
            for (needle, output) in [(ferrite.as_str(), version.as_str()), ("git --version", "git 2\n"), ("gh --version", "gh 2\n")] {
                runner.on(needle, ok(output));
            }
            let (text, _) = report(&env, &root, &home.path().join("cfg"), &SessionFlags::default(), &runner, false, None);
            if reachable {
                assert!(text.contains("[ok  ] github github.localhost: reachable as eliasstravik (GH_HOST=github.localhost, used for eliasstravik/app)"), "{text}");
            } else {
                assert!(text.contains("[warn] github github.localhost: gh cannot reach github.localhost (HTTP 401: Requires authentication); pull requests of eliasstravik/app are not followed. Try `GH_HOST=github.localhost gh auth status`"), "{text}");
            }
            assert!(!text.contains("] gh auth:"), "github.com is not in use: {text}");
            assert_eq!(runner.count("gh auth status"), 0);
        }
    }

    #[test]
    fn routines_skipped_for_want_of_a_coordinator_are_named() {
        let home = tempfile::tempdir().unwrap();
        let env = Env::for_test(home.path(), &[]);
        let runner = runner_with_ferrite(herdr::MIN_VERSION);
        let root = home.path().join("root");
        let project = project::create(&root, "demo", "", vec![]).unwrap();
        std::fs::write(project.dir().join("routines/standup.md"), "+++\nschedule = \"every 5m\"\n+++\nGo.\n").unwrap();
        let (text, _) = report(&env, &root, &home.path().join("cfg"), &SessionFlags::default(), &runner, false, None);
        assert!(!text.contains("routines demo"), "{text}");
        let mut state = crate::steps::State::default();
        state.routines.insert("standup".into(), crate::routine::State { last_run: "2026-09-24T09:00:00Z".into(), no_coordinator: 2, ..Default::default() });
        crate::steps::save_state(&project, &state).unwrap();
        let (text, _) = report(&env, &root, &home.path().join("cfg"), &SessionFlags::default(), &runner, false, None);
        assert!(text.contains("[warn] routines demo: standup: last "), "{text}");
        assert!(text.contains("skipped: no coordinator (2 run(s)); routines run only while a coordinator runs"), "{text}");
    }

    #[test]
    fn fix_links_the_skill_only_for_a_configured_harness_and_never_over_a_foreign_one() {
        let home = tempfile::tempdir().unwrap();
        let claude = home.path().join("claude-config");
        let env = Env::for_test(home.path(), &[("CLAUDE_CONFIG_DIR", claude.to_str().unwrap())]);
        let runner = runner_with_ferrite(herdr::MIN_VERSION);
        let root = home.path().join("root");
        let cfg = home.path().join("cfg");
        let flags = SessionFlags::default();
        std::fs::create_dir_all(&claude).unwrap();
        let source = home.path().join("plugin/skill/autoproject");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: autoproject\n---\n").unwrap();
        let link = claude.join("skills").join(crate::setup::SKILL);

        // Never configured: `--fix` only points to `configure`.
        let (text, _) = report(&env, &root, &cfg, &flags, &runner, true, Some(&source));
        assert!(text.contains("[warn] skill claude:") && text.contains("`configure` links the bundled skill"), "{text}");
        assert_eq!(crate::setup::skill_state(&link, &source), crate::setup::SkillState::Missing);

        // Configured before the skill shipped: hooks journaled, no link yet.
        let hooks = crate::setup::Owned { before: None, after: "{}".into(), kind: "hooks".into(), command: Some("x".into()) };
        crate::setup::save_journal(&cfg, &[(claude.join("settings.json").to_string_lossy().into_owned(), hooks)].into()).unwrap();
        let (text, _) = report(&env, &root, &cfg, &flags, &runner, false, Some(&source));
        assert!(text.contains("`doctor --fix` links the bundled skill"), "{text}");
        let (text, healthy) = report(&env, &root, &cfg, &flags, &runner, true, Some(&source));
        assert!(healthy && text.contains("[ok  ] skill claude: fixed:"), "{text}");
        assert_eq!(crate::setup::skill_state(&link, &source), crate::setup::SkillState::Ours);
        assert!(!claude.join("settings.json").exists(), "the hooks were touched");

        // A moved checkout: our journaled link is relinked.
        let moved = home.path().join("moved/skill/autoproject");
        std::fs::create_dir_all(&moved).unwrap();
        std::fs::write(moved.join("SKILL.md"), "x").unwrap();
        report(&env, &root, &cfg, &flags, &runner, true, Some(&moved));
        assert_eq!(crate::setup::skill_state(&link, &moved), crate::setup::SkillState::Ours);

        // A directory of the same name is never touched.
        std::fs::remove_file(&link).unwrap();
        std::fs::create_dir(&link).unwrap();
        let (text, _) = report(&env, &root, &cfg, &flags, &runner, true, Some(&moved));
        assert!(text.contains("is not this plugin's link"), "{text}");
        assert_eq!(crate::setup::skill_state(&link, &moved), crate::setup::SkillState::Foreign);
    }

    #[test]
    fn the_bundled_skill_is_written_by_fix_only() {
        let home = tempfile::tempdir().unwrap();
        let env = Env::for_test(home.path(), &[]);
        let runner = runner_with_ferrite(herdr::MIN_VERSION);
        let root = home.path().join("root");
        let cfg = home.path().join("cfg");
        let flags = SessionFlags::default();
        std::fs::create_dir_all(home.path().join(".claude")).unwrap();
        let bundled = crate::setup::skill_source(&cfg);

        // Never configured: the missing link is named, nothing is written.
        let (text, _) = report(&env, &root, &cfg, &flags, &runner, false, Some(&bundled));
        assert!(text.contains("`configure` links the bundled skill"), "{text}");
        assert!(!bundled.exists(), "doctor without --fix wrote the skill");

        // An older Ferrite's copy: named without `--fix`, rewritten with it.
        crate::setup::install_skill(&cfg).unwrap();
        std::fs::write(bundled.join("SKILL.md"), "older").unwrap();
        let (text, _) = report(&env, &root, &cfg, &flags, &runner, false, Some(&bundled));
        assert!(text.contains("[warn] skill files:") && text.contains("`doctor --fix` rewrites it"), "{text}");
        assert!(!crate::setup::skill_current(&bundled));
        let (text, _) = report(&env, &root, &cfg, &flags, &runner, true, Some(&bundled));
        assert!(text.contains("[ok  ] skill files: fixed:"), "{text}");
        assert!(crate::setup::skill_current(&bundled));
    }

    #[test]
    fn colliding_agent_names_fail_the_report() {
        let home = tempfile::tempdir().unwrap();
        let env = Env::for_test(home.path(), &[]);
        let runner = runner_with_ferrite(herdr::MIN_VERSION);
        let root = home.path().join("root");
        let long = "x".repeat(30);
        for suffix in ["a", "b"] {
            let project = project::create(&root, &format!("{long}-{suffix}"), "", vec![]).unwrap();
            project::write_priming(&project, &crate::coordinator::current_prefix(&root).unwrap()).unwrap();
        }
        let (text, healthy) = report(&env, &root, &home.path().join("cfg"), &SessionFlags::default(), &runner, false, None);
        assert!(!healthy);
        assert!(text.contains("[FAIL] names"), "{text}");
    }
}
