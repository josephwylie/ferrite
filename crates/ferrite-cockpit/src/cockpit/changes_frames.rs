//! What a frame costs the main thread with the Changes Surface open: the
//! production window off-screen with real text (`headless`), on a
//! throwaway checkout of N edited files. macOS makes its platform only on
//! the main thread, so this runs as an example, not a test:
//!
//! `cargo run -p ferrite-cockpit --features headless --example changes_frames -- shut 3 14 35 168 long`
//!
//! One line per case: files listed and drawn, hunk lines drawn, and the
//! time of each of `FRAMES` forced frames.

use std::path::Path;
use std::time::Instant;

use ferrite_client::cockpit::{Cockpit, SpawnRequest, Spawner};
use ferrite_core::checkout::WorkspaceChoice;
use ferrite_core::provider::Provider;
use ferrite_engine::providers::Session;
use ferrite_engine::store::Store;

use super::panel::Opener;
use ferrite_tools::changes::Diff;
use super::CockpitView;
use crate::headless::Driver;

/// Forced frames per case, `FERRITE_FRAMES` or 20.
fn frames() -> usize {
    std::env::var("FERRITE_FRAMES").ok().and_then(|n| n.parse().ok()).unwrap_or(20)
}

struct Quiet;

impl Spawner for Quiet {
    fn spawn(&mut self, _: SpawnRequest) -> std::io::Result<Box<dyn Session>> {
        Ok(Box::new(crate::demo::DemoSession::quiet()))
    }
}

fn git(repo: &Path, args: &[&str]) {
    let out = std::process::Command::new("git").args(args).current_dir(repo).output().expect("git");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// `files` committed files of 120 lines, every tenth then changed: twelve
/// hunks each, of a removed and an added line with a line of context
/// either side.
fn edited(repo: &Path, files: usize) {
    let body = |file: usize, changed: bool| -> String {
        (0..120)
            .map(|n| {
                let edit = if changed && n % 10 == 5 { "_edited" } else { "" };
                format!("    let value_{n} = compute_{file}_{n}{edit}(input, {n}); // line {n} of {file}\n")
            })
            .collect()
    };
    for file in 0..files {
        let dir = repo.join(format!("src/m{}", file / 20));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("f{file}.rs")), body(file, false)).unwrap();
    }
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "base"]);
    for file in 0..files {
        std::fs::write(repo.join(format!("src/m{}/f{file}.rs", file / 20)), body(file, true)).unwrap();
    }
}

/// One new file of `lines` lines.
fn long(repo: &Path, lines: usize) {
    let body: String = (0..lines)
        .map(|n| format!("    let value_{n} = compute_{n}(input, {n}); // line {n} of the long file\n"))
        .collect();
    std::fs::write(repo.join("long.rs"), body).unwrap();
}

/// This workspace's own history: checked out at `base`, its files then
/// as `head` has them, uncommitted.
fn real(repo: &Path, range: &str) {
    let (base, head) = range.split_once("..").expect("real:<base>..<head>");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = source.to_str().expect("a UTF-8 path");
    git(Path::new(source), &["clone", "-q", "--shared", "--no-checkout", source, repo.to_str().unwrap()]);
    git(repo, &["checkout", "-q", "--detach", base]);
    git(repo, &["restore", "-q", "--source", head, "--worktree", "--", "."]);
}

/// One case: `shut` (the 168-file checkout, the Panel shut), a file count,
/// `long` (one new file of 6,500 lines), `long<lines>`, or
/// `real:<base>..<head>`.
fn case(name: &str) -> String {
    let base = std::env::temp_dir().join(format!("ferrite-changes-frames-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let repo = base.join("repo");
    if let Some(range) = name.strip_prefix("real:") {
        real(&repo, range);
    } else {
        std::fs::create_dir_all(&repo).unwrap();
        for args in [
            &["init", "-q"][..],
            &["config", "user.email", "operator@example.invalid"],
            &["config", "user.name", "operator"],
            &["commit", "-q", "--allow-empty", "-m", "root"],
        ] {
            git(&repo, args);
        }
        match name {
            "long" => long(&repo, 6_500),
            name if name.starts_with("long") => long(&repo, name["long".len()..].parse().expect("long<lines>")),
            "shut" => edited(&repo, 168),
            files => edited(&repo, files.parse().expect("shut, long, real:<base>..<head> or a file count")),
        }
    }
    let open = name != "shut";
    let store = Store::open(base.join("store")).unwrap();
    let mut core = Cockpit::new(store, Box::new(Quiet));
    let here = core.this_computer().expect("this computer");
    core.open(here, Provider::Claude, WorkspaceChoice::Main { checkout: repo.clone() }).unwrap();
    let cx = crate::headless::app(ferrite_ui::keymap::bindings(ferrite_ui::keymap::PLATFORM));
    let mut driver = Driver::open(cx, |cx| CockpitView::new_with_provider(core, Provider::Claude, cx));
    driver.tick(16);
    let thread = driver.read(|view, _| view.panes[0].thread().expect("the opened Thread"));
    driver.update(|view, _, cx| {
        view.prefs.settings.panel_shown = open;
        cx.notify();
    });
    let mut listed = 0;
    if open {
        driver.update(|view, _, cx| view.run_opener(thread, Opener::Changes, cx));
        let started = Instant::now();
        loop {
            driver.tick(5);
            if let Some(files) = driver.read(|view, cx| view.changes.read(cx).files_read(Diff::Changes(thread))) {
                listed = files;
                break;
            }
            assert!(started.elapsed().as_secs() < 60, "the read never landed");
        }
    }
    for _ in 0..10 {
        driver.tick(16);
    }
    let count = frames();
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        driver.update(|_, _, cx| cx.notify());
        let started = Instant::now();
        driver.draw();
        frames.push(started.elapsed().as_secs_f64() * 1_000.);
    }
    let timed = |driver: &mut Driver, act: &dyn Fn(&mut CockpitView, &mut gpui::Context<CockpitView>)| {
        driver.update(|view, _, cx| act(view, cx));
        let started = Instant::now();
        driver.draw();
        let ms = started.elapsed().as_secs_f64() * 1_000.;
        for _ in 0..3 {
            driver.tick(16);
        }
        ms
    };
    let diff = Diff::Changes(thread);
    let (wrap, fold, expand) = if open {
        let wrap = timed(&mut driver, &|view, cx| view.changes.update(cx, |changes, cx| changes.run_verb(diff, ferrite_tools::changes::Verb::ToggleWrap, cx)));
        timed(&mut driver, &|view, cx| view.changes.update(cx, |changes, cx| changes.run_verb(diff, ferrite_tools::changes::Verb::ToggleWrap, cx)));
        let fold = timed(&mut driver, &|view, cx| view.changes.update(cx, |changes, cx| changes.run_verb(diff, ferrite_tools::changes::Verb::CollapseAll, cx)));
        let expand = timed(&mut driver, &|view, cx| view.changes.update(cx, |changes, cx| changes.run_verb(diff, ferrite_tools::changes::Verb::ExpandAll, cx)));
        (wrap, fold, expand)
    } else {
        (0., 0., 0.)
    };
    let drawn = (0..listed)
        .filter(|at| driver.bounds(&format!("changes-file-{at}")).is_some())
        .count();
    let lines: usize = (0..listed)
        .map(|at| (0..).take_while(|n| driver.bounds(&format!("panel-hunk-{at}-{n}")).is_some()).count())
        .sum();
    if let Ok(dir) = std::env::var("FERRITE_FRAMES_SHOTS") {
        driver.shot(&Path::new(&dir).join(format!("changes-frames-{name}.png"))).expect("a shot");
    }
    driver.close();
    let _ = std::fs::remove_dir_all(&base);
    let total: f64 = frames.iter().sum();
    let mut sorted = frames.clone();
    sorted.sort_by(f64::total_cmp);
    format!(
        "FRAMES case={name} listed={listed} files_drawn={drawn} lines_drawn={lines} ms_per_frame={:.2} median={:.2} min={:.2} max={:.2} wrap_off_frame={wrap:.2} collapse_all_frame={fold:.2} expand_all_frame={expand:.2}",
        total / count as f64,
        sorted[count / 2],
        sorted[0],
        sorted[count - 1],
    )
}

/// Each case named, in order, on its own window.
pub fn run(cases: &[String]) {
    for name in cases {
        println!("{}", case(name));
    }
}
