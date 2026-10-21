//! The UI refresh's world (`docs/design/ui-refresh/mockups/`): the one
//! roster behind the `refresh-*` scenes of the visual reference and behind
//! `--demo parity`. Seven Projects under a disposable HOME, the Threads,
//! Groups and states the mockups draw, and a timeline written in the
//! mockups' own wall-clock times, so every stamp, age and duration a
//! capture shows falls out of the fixture clock rather than being painted.
//!
//! What the mockups show that has a source today is fed through it: the
//! transcripts, states, Subagents, the queued prompt, the mode, model and
//! effort, the context, the account limits, the checkouts' branches and
//! drift, the agent's edits. The pull request the panel's PR tab reads
//! (SPEC §5.6.2) comes from the forge's fixture (`Cockpit::fixture_forge`):
//! the world answers both PR reads of the crash fix's worktree with one of
//! `Pr`'s snapshots. Free disk (`7.5 GB free`) has no source yet (SPEC §8):
//! nothing here feeds it.
//!
//! Where the mockups disagree with themselves, the world takes the reading
//! most of them share and says so where it happens.

#![cfg_attr(not(feature = "visual-reference"), allow(dead_code))]

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use ferrite_core::activity::{ActivityEvent, AgentInfo, AgentKey, AgentStatus, Subject};
use ferrite_core::cockpit::Cockpit;
use ferrite_core::store::{Provider, Store};
use ferrite_core::workspace::registry::ProjectId;
use ferrite_core::workspace::{BranchStatus, WorkspaceChoice};
use ferrite_core::{Hunk, RunState, SessionEvent, ThreadId, ToolResult, TurnOutcome};
use ferrite_reviewr::forge::{
    Check, CheckStatus, Comment, CommentKind, FindingPlace, Merge, PrSnapshot, PrState, PrView,
    Sync,
};

use super::world::{
    apply, approval, bash_approval, claude_menu, filler, git, git_command, group, hms, init,
    last_used, limits, mode, phase, play, question, quiet, started, text, tokens, write_lines,
    Clock, Run, Script, Secs, Sessions, DAY, HOUR, MINUTE,
};

// ------------------------------------------------------------------ clock

/// The capture: the bar reads `12:31 pm`. The crash fix's turn has run
/// `5m22s` since its `12:26 pm` prompt, so the clock stands at least 22
/// seconds into the minute; thirty keeps every other stamp on its minute.
pub(crate) const CAPTURE: Secs = hms(12, 31, 30);

// ------------------------------------------------------------------ world

/// Which view the world is built for. The roster never changes; what
/// differs is the Codex request the Group view is waiting on: the Solo
/// mockups count `3 need you` and roll WORK up as `1 failed`, the Group
/// mockup counts `4`, with Find all calls asking. So in Solo that Thread
/// is still searching, and in the Group it has asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    /// Ferrite idle memory usage in Solo (solo.png, solo-changes.png).
    Solo,
    /// The WORK Group, Supplier Price Review focused (group.png).
    Group,
}

/// The crash fix's pull request, as the forge reads it (pr-states.png and
/// the PR tab of solo.png): one per column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pr {
    /// #146 open: checks running, no conflicts, one commit not pushed.
    Open,
    /// #146 open: a check failed, it conflicts with main, in sync.
    Failing,
    /// #146 merged, every check passed, every comment resolved.
    Merged,
    /// No pull request for the branch.
    None,
}

/// Every Thread of the world, by role.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cast {
    /// Ferrite idle memory usage: the Solo Thread.
    pub memory: ThreadId,
    pub hedr: ThreadId,
    pub chess: ThreadId,
    pub recipes: ThreadId,
    pub customers: ThreadId,
    /// InvoiceMate Business's two.
    pub business: ThreadId,
    pub redesign: ThreadId,
    pub permission: ThreadId,
    pub workmate: ThreadId,
    pub tempo: ThreadId,
    pub parked: [ThreadId; 3],
}

/// The built world: a Cockpit over a disposable store, and what a scene
/// needs to aim at it.
pub(crate) struct World {
    pub core: Cockpit,
    pub sessions: Sessions,
    pub home: PathBuf,
    /// `~/Desktop/Projects`, where every Project lives and Ferrite was
    /// launched.
    pub launch: PathBuf,
    pub cast: Cast,
    /// The Projects the mockups draw folded: workmate (`workmate 1`) and
    /// tempo (its heading alone).
    pub folded: Vec<ProjectId>,
    /// What each open Thread's checkout says (branch, upstream drift,
    /// dirt): what git reads there, for a scene to seed the nav with
    /// before its own reads land.
    pub branches: Vec<(ThreadId, BranchStatus)>,
}

/// A checkout's branch, and its drift against `origin/main`.
fn status(branch: &str, ahead: u32, behind: u32, dirty: u32) -> BranchStatus {
    BranchStatus {
        branch: Some(branch.into()),
        upstream: (ahead + behind > 0).then(|| "origin/main".into()),
        ahead,
        behind,
        dirty,
    }
}

/// Build the world for `look` under `root` (which must exist; it is made
/// canonical so the registry's roots and HOME agree), its crash fix's pull
/// request reading `pr`. Nothing global is touched: the caller points HOME
/// at `World::home` and owns the clock.
pub(crate) fn build(root: &Path, look: Look, pr: Pr, clock: Clock) -> World {
    let root = std::fs::canonicalize(root).expect("a canonical disposable root");
    let home = root.join("home");
    let launch = home.join("Desktop").join("Projects");
    let project = |name: &str| launch.join(name);
    clock.set(hms(8, 0, 0));
    let worktree = lay_out_ferrite(&project("ferrite"), &home);
    // agenticos' main is 715 commits ahead of its upstream; invoicemate's
    // one ahead and five behind (the headings' `↑715`, `↑1 ↓5`).
    lay_out_tracking(&project("agenticos"), &home, "agenticos", 715, 0);
    lay_out_tracking(&project("invoicemate"), &home, "invoicemate", 1, 5);
    for name in ["production-scheduling", "strat", "workmate", "tempo"] {
        super::world::lay_out_plain(&project(name), &home, name);
    }
    git(
        &project("tempo"),
        &home,
        &["checkout", "-q", "-b", "overnight/coach"],
    );

    let sessions = Sessions::default();
    let mut core = Cockpit::new(
        Store::open(root.join("store")).expect("open the disposable store"),
        Box::new(sessions.spawner()),
    );
    core.set_suggestions_enabled(false);

    // Created in the nav's order: rows go in creation order, and each
    // Project heading in the order its first Thread came.
    let main = |name: &str| WorkspaceChoice::Main {
        checkout: project(name),
    };
    let open = |core: &mut Cockpit, provider, workspace, title: &str| {
        sessions.open(core, provider, workspace, title)
    };
    let claude = Provider::Claude;
    let hedr = open(
        &mut core,
        claude,
        main("ferrite"),
        "Hedr: Rust agent management",
    );
    let memory = open(
        &mut core,
        claude,
        WorkspaceChoice::ExistingWorktree {
            repo: project("ferrite"),
            path: worktree.clone(),
        },
        "Ferrite idle memory usage",
    );
    let chess = open(
        &mut core,
        claude,
        main("agenticos"),
        "Chess Matches: Principled play",
    );
    let supplier = open(
        &mut core,
        claude,
        main("agenticos"),
        "Supplier Price Review",
    );
    let calls = open(
        &mut core,
        Provider::Codex,
        main("agenticos"),
        "Find all calls to this API",
    );
    let epicurean = open(
        &mut core,
        claude,
        main("agenticos"),
        "Epicurean chocolate range",
    );
    let bb = open(
        &mut core,
        claude,
        main("agenticos"),
        "Complete BB supplier list",
    );
    let recipes = open(
        &mut core,
        claude,
        main("production-scheduling"),
        "Implement recipes feature",
    );
    let access = open(
        &mut core,
        claude,
        main("production-scheduling"),
        "Redesign Production Mate access",
    );
    let viability = open(
        &mut core,
        claude,
        main("production-scheduling"),
        "Redesign viability overview",
    );
    let customers = open(
        &mut core,
        claude,
        main("invoicemate"),
        "Calling customers to move",
    );
    let business = open(
        &mut core,
        claude,
        main("invoicemate"),
        "InvoiceMate Business",
    );
    let redesign = open(
        &mut core,
        claude,
        main("invoicemate"),
        "Redesign InvoiceMate invoices",
    );
    let permission = open(
        &mut core,
        claude,
        main("strat"),
        "I don't have permission to deploy the strategy site",
    );
    let country = open(
        &mut core,
        claude,
        main("strat"),
        "Country strategy game metadata",
    );
    let workmate = open(
        &mut core,
        claude,
        main("workmate"),
        "Autonomous background maintenance",
    );
    let tempo = open(
        &mut core,
        claude,
        main("tempo"),
        "Build the autonomous chess coach",
    );
    // The crash fix runs Opus 5.5 at high effort, chosen before its first
    // prompt: the effort lands on the Session, then the model respawns it
    // fresh, which also clears the effort's notice.
    core.set_effort(memory, Some("high".into()))
        .expect("choose the crash fix's effort");
    core.set_model(memory, Some("opus".into()))
        .expect("choose the crash fix's model");
    let cast = Cast {
        memory,
        hedr,
        chess,
        recipes,
        customers,
        business,
        redesign,
        permission,
        workmate,
        tempo,
        parked: [access, viability, country],
    };

    let mut script = Script::default();
    crash_fix(&mut script, memory);
    supplier_review(&mut script, supplier);
    find_calls(&mut script, calls, look);
    epicurean_range(&mut script, epicurean);
    bb_list(&mut script, bb);
    asks(&mut script, &cast);
    quiet_threads(&mut script, &cast);

    // The day is played with the crash fix focused: the requests arrive
    // unread, and only the crash fix is looked at.
    core.focus_thread(memory);
    let last = play(&mut core, script, clock, |thread| sessions.feed(thread));
    clock.set(CAPTURE);
    core.pump();
    // The operator's follow-up, held until the crash fix's turn ends.
    core.queue(memory, QUEUED.into());
    core.pump();

    // The Groups, in creation order: the bar's `2 Work`, `3 InvoiceMate
    // business`. WORK sits under agenticos, its first member's Project.
    let work = group(&mut core, supplier, calls, &[epicurean, bb], "WORK");
    group(&mut core, business, redesign, &[], "InvoiceMate Business");

    // The idle Threads were read when they finished: only Complete BB
    // supplier list is done and unseen, and the notices hold only what
    // still matters — the done, the failure and the requests.
    for thread in [hedr, chess, business, redesign, workmate, tempo] {
        core.focus_thread(thread);
    }
    let keep = [bb, epicurean, calls, recipes, customers, permission];
    let dismissed: Vec<_> = core
        .notifications()
        .notices()
        .filter(|notice| !keep.contains(&notice.thread))
        .map(|notice| notice.id)
        .collect();
    for id in dismissed {
        core.dismiss_notice(id);
    }

    match look {
        Look::Solo => {
            core.focus_thread(memory);
        }
        Look::Group => {
            core.focus_thread(supplier);
            core.enter_group(work).expect("enter WORK");
        }
    }
    core.pump();

    // Recency is the log's mtime: each Thread was last written when it
    // last acted on the fixture clock, not when the world was built.
    let store = root.join("store");
    for (thread, at) in last {
        last_used(&store, thread, clock.instant(at));
    }

    // Both PR reads of the crash fix's worktree answer the chosen snapshot.
    core.fixture_forge(worktree, pr.view(clock));

    let agenticos = status("main", 715, 0, 0);
    let invoicemate = status("main", 1, 5, 0);
    let mut branches = vec![
        (hedr, status("main", 0, 0, 0)),
        // The fix is not committed: its one file is the dirt.
        (memory, status(BRANCH, 0, 0, 1)),
        (recipes, status("main", 0, 0, 0)),
        (permission, status("main", 0, 0, 0)),
        (workmate, status("main", 0, 0, 0)),
        (tempo, status("overnight/coach", 0, 0, 0)),
    ];
    branches.extend([chess, supplier, calls, epicurean, bb].map(|t| (t, agenticos.clone())));
    branches.extend([customers, business, redesign].map(|t| (t, invoicemate.clone())));

    let folded = core
        .registry()
        .projects()
        .iter()
        .filter(|project| ["workmate", "tempo"].contains(&project.title.as_str()))
        .map(|project| project.id)
        .collect();

    World {
        core,
        sessions,
        home,
        launch,
        cast,
        folded,
        branches,
    }
}

// ------------------------------------------------------------ the checkouts

/// The crash fix's branch, checked out in its own worktree.
const BRANCH: &str = "scheduled-work-crash";
const SCHEDULED_WORK: &str = "crates/ferrite-core/src/scheduled_work.rs";

/// `~/Desktop/Projects/ferrite` on `main`, with the crash fix's worktree
/// beside it on `scheduled-work-crash`, the fix applied and not committed.
/// Answers the worktree.
fn lay_out_ferrite(ferrite: &Path, home: &Path) -> PathBuf {
    std::fs::create_dir_all(ferrite).expect("create ferrite");
    let before = scheduled_work_before();
    write_lines(&ferrite.join(SCHEDULED_WORK), &before);
    std::fs::write(ferrite.join("README.md"), "# ferrite\n").expect("write README");
    git(ferrite, home, &["init", "-q", "-b", "main"]);
    git(ferrite, home, &["add", "-A"]);
    git(ferrite, home, &["commit", "-q", "-m", "ferrite 0.5.0"]);
    // Worktrees live inside the checkout, out of its status.
    std::fs::write(ferrite.join(".git/info/exclude"), ".worktrees/\n")
        .expect("exclude the worktrees");
    let worktree = ferrite.join(".worktrees").join(BRANCH);
    let path = worktree.to_string_lossy().into_owned();
    git(
        ferrite,
        home,
        &["worktree", "add", "-q", "-b", BRANCH, &path],
    );
    write_lines(
        &worktree.join(SCHEDULED_WORK),
        &apply(&before, &[crash_fix_hunk()]),
    );
    worktree
}

/// A plain Project on `main` tracking an `origin/main` it is `ahead` of and
/// `behind`, the commits empty. The remote points nowhere, so nothing ever
/// fetches.
fn lay_out_tracking(dir: &Path, home: &Path, name: &str, ahead: usize, behind: usize) {
    super::world::lay_out_plain(dir, home, name);
    git(
        dir,
        home,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    commits(dir, home, "refs/remotes/origin/main", behind);
    commits(dir, home, "refs/heads/main", ahead);
    let nowhere = dir.with_extension("origin.git");
    for (key, value) in [
        ("remote.origin.url", nowhere.to_string_lossy().as_ref()),
        ("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"),
        ("branch.main.remote", "origin"),
        ("branch.main.merge", "refs/heads/main"),
    ] {
        git(dir, home, &["config", key, value]);
    }
}

/// `count` empty commits on top of `branch`, written in one `git
/// fast-import` (715 `git commit`s would take seconds).
fn commits(dir: &Path, home: &Path, branch: &str, count: usize) {
    use std::io::Write as _;
    if count == 0 {
        return;
    }
    let mut stream = String::new();
    for at in 0..count {
        let message = format!("work {at}\n");
        stream.push_str(&format!(
            "commit {branch}\ncommitter Ferrite <fixture@ferrite.invalid> 1791100800 +0000\n\
             data {}\n{message}",
            message.len()
        ));
        if at == 0 {
            stream.push_str(&format!("from {branch}^0\n"));
        }
        stream.push('\n');
    }
    let child = git_command(dir, home)
        .args(["fast-import", "--quiet"])
        .stdin(Stdio::piped())
        .spawn();
    let done = child.and_then(|mut child| {
        child
            .stdin
            .take()
            .expect("a piped stdin")
            .write_all(stream.as_bytes())?;
        child.wait()
    });
    if !matches!(done, Ok(status) if status.success()) {
        eprintln!(
            "ferrite: mockup world: fast-import onto {branch} failed in {}",
            dir.display()
        );
    }
}

const SCHEDULED_NAMES: [&str; 8] = [
    "next_due",
    "lease",
    "release",
    "backoff",
    "record_run",
    "worktree_for",
    "prune",
    "snapshot",
];

/// `scheduled_work.rs` before the fix: the pop and the spawn at 88–89, and
/// the requeue the finding at 120 is about.
fn scheduled_work_before() -> Vec<String> {
    let mut lines: Vec<String> = [
        "//! Scheduled work: jobs queued for a Thread's worktree, run when due.",
        "",
        "use crate::workspace::Worktree;",
        "",
        "impl Scheduler {",
    ]
    .map(String::from)
    .to_vec();
    filler(&mut lines, 87, &SCHEDULED_NAMES, "Option<Job>");
    lines.extend(
        [
            "let job = queue.pop_front()?;",
            "let handle = job.spawn().unwrap();",
        ]
        .map(String::from),
    );
    filler(&mut lines, 119, &SCHEDULED_NAMES[4..], "Option<Job>");
    lines.push("fn requeue(&mut self, job: Job) -> Option<Handle> {".into());
    filler(&mut lines, 139, &SCHEDULED_NAMES[..4], "Option<Job>");
    lines.push("}".into());
    lines
}

/// The fix, `+4 −1`: the spawn's failure goes back on the queue. The
/// mockups draw the hunk's first four rows.
fn crash_fix_hunk() -> Hunk {
    Hunk {
        old_start: 88,
        old_lines: 2,
        new_start: 88,
        new_lines: 5,
        section: None,
        lines: [
            " let job = queue.pop_front()?;",
            "-let handle = job.spawn().unwrap();",
            "+let Ok(handle) = job.spawn() else {",
            "+    return self.requeue(job);",
            "+};",
            "+",
        ]
        .map(String::from)
        .to_vec(),
    }
}

// ---------------------------------------------------------------- scripts

/// Ferrite idle memory usage (solo.png): one turn, 5m22s in at capture —
/// seven commands, the panic found, the fix, the core suite, two reviews
/// by Subagent (one still running) — the working line `Pondering…`.
fn crash_fix(s: &mut Script, who: ThreadId) {
    let start = CAPTURE - (5 * MINUTE + 22);
    s.prompt(start, who, "Oamy ylou killed it again? it keeps dying");
    s.ev(start, who, init("mockup-memory", "claude-opus-5-5"));
    s.ev(start, who, mode("bypassPermissions"));
    s.ev(
        start,
        who,
        SessionEvent::Models {
            models: claude_menu(),
        },
    );
    // The account's windows, as Claude last reported them: the week is the
    // fuller one, `14% this week`.
    s.ev(start, who, limits(0.05, 0.14));
    // `Ran 7 commands · 1 failed`, back to back over 34 seconds.
    let mut at = start + 2;
    for (at_id, (command, secs, exit, stdout)) in INVESTIGATION.iter().enumerate() {
        s.bash(
            at,
            who,
            &format!("toolu_memory_look_{at_id}"),
            command,
            Run {
                stdout: (*stdout).into(),
                exit: *exit,
                ms: *secs * 1000,
            },
        );
        at += *secs as Secs;
    }
    s.ev(at + 2, who, text(RED));
    s.edit(
        at + 6,
        who,
        "toolu_memory_fix",
        SCHEDULED_WORK,
        vec![crash_fix_hunk()],
    );
    let suite = at + 10;
    s.bash(
        suite,
        who,
        "toolu_memory_suite",
        "cargo test -p ferrite-core",
        Run {
            stdout: "running 954 tests\ntest result: ok. 954 passed; 0 failed; 0 ignored; \
                     0 measured; 0 filtered out; finished in 57.84s"
                .into(),
            exit: 0,
            ms: 59_000,
        },
    );
    s.ev(suite + 61, who, text(FIXED));
    // Two reviews by Subagent: the standards one took 56s; the spec one
    // has run two minutes at capture.
    let review = |id: &str, description: &str| {
        started(
            id,
            "Agent",
            serde_json::json!({
                "description": description,
                "subagent_type": "general-purpose",
                "prompt": format!("{description}: read the diff in {SCHEDULED_WORK}."),
            }),
        )
    };
    let standards = suite + 64;
    s.ev(
        standards,
        who,
        review("toolu_memory_standards", "Standards review of crash fix"),
    );
    s.ev(
        standards + 56,
        who,
        super::world::completed(
            "toolu_memory_standards",
            "No findings: the guard reads as the module's other early returns.",
            false,
            ToolResult::Opaque,
        ),
    );
    s.ev(
        CAPTURE - 2 * MINUTE,
        who,
        review("toolu_memory_spec", "Spec review of crash fix"),
    );
    // The Thread's Subagents: 23 finished, the spec review live.
    for (at_id, description) in FINISHED_SUBAGENTS.iter().enumerate() {
        subagent(
            s,
            start,
            who,
            &format!("done-{at_id}"),
            description,
            AgentStatus::Idle,
        );
    }
    subagent(
        s,
        CAPTURE - 2 * MINUTE,
        who,
        "spec-review",
        "Spec review of crash fix",
        AgentStatus::Working,
    );
    // 84k of 200k in context; the turn has written 130 tokens.
    for event in tokens(84_000, 2_400, 130, 200_000) {
        s.ev(CAPTURE - 1, who, event);
    }
    s.ev(CAPTURE, who, phase("Pondering"));
}

/// A Subagent of the crash fix's Main, discovered at `at` in `state`.
fn subagent(
    s: &mut Script,
    at: Secs,
    who: ThreadId,
    id: &str,
    description: &str,
    state: AgentStatus,
) {
    let key = AgentKey::new(Provider::Claude, "mockup-memory", id);
    let mut info = AgentInfo::new(key.clone());
    info.description = Some(description.into());
    info.kind = Some("general-purpose".into());
    info.parent = Some(Subject::Main);
    s.ev(
        at,
        who,
        SessionEvent::Activity(ActivityEvent::Discovered(info)),
    );
    s.ev(
        at,
        who,
        SessionEvent::Activity(ActivityEvent::Status { key, state }),
    );
}

/// Supplier Price Review (group.png, Pane 1): its prompt at 12:20 pm, two
/// reads and a line of prose, then a wait; it resumed 1m04s before capture
/// (the working line's clock starts there) and its script is 14s in.
fn supplier_review(s: &mut Script, who: ThreadId) {
    let start = hms(12, 20, 0);
    s.prompt(
        start,
        who,
        "Compare this quarter's supplier quotes with last quarter's",
    );
    s.ev(start, who, init("mockup-supplier", "claude-opus-5-5"));
    s.ev(start, who, mode("acceptEdits"));
    for (at, path) in [(2, "quotes/2026-q3.csv"), (4, "quotes/2026-q4.csv")] {
        s.tool(
            (start + at, start + at + 1),
            who,
            &format!("toolu_supplier_read_{at}"),
            "Read",
            serde_json::json!({ "file_path": path }),
            quiet(),
        );
    }
    s.ev(
        start + 7,
        who,
        text(
            "**The two files name their columns differently,** so I'm lining them up before \
             comparing.\n\n",
        ),
    );
    s.ev(
        start + 8,
        who,
        SessionEvent::RunState {
            state: RunState::Idle,
        },
    );
    s.ev(
        CAPTURE - 64,
        who,
        SessionEvent::RunState {
            state: RunState::Running,
        },
    );
    s.ev(
        CAPTURE - 14,
        who,
        started(
            "toolu_supplier_compare",
            "Bash",
            serde_json::json!({ "command": "python3 compare_quotes.py" }),
        ),
    );
    s.ev(CAPTURE, who, phase("Comparing"));
}

/// Find all calls to this API (group.png, Pane 2): a Codex search, then,
/// in the Group's look, its request to write the list out.
fn find_calls(s: &mut Script, who: ThreadId, look: Look) {
    let start = hms(12, 24, 0);
    s.prompt(
        start,
        who,
        "Find every call to client.request and list them by file",
    );
    s.ev(start, who, init("mockup-calls", "gpt-6-sol"));
    s.ev(start, who, mode("on-request"));
    // The account's windows, as Codex last reported them: `6% this week`.
    s.ev(start, who, limits(0.02, 0.06));
    let matches: Vec<String> = (0..38)
        .map(|at| {
            format!(
                "src/api/{}.ts:{}:  await client.request(",
                CALL_FILES[at % 11],
                12 + at
            )
        })
        .collect();
    s.tool(
        (start + 1, start + 3),
        who,
        "call_calls_search",
        "Grep",
        serde_json::json!({
            "pattern": "client.request(",
            "path": "src",
            "output_mode": "content",
        }),
        (
            matches.join("\n"),
            false,
            ToolResult::Structured {
                value: serde_json::json!({
                    "mode": "content",
                    "numFiles": 11,
                    "filenames": [],
                    "content": matches.join("\n"),
                    "numLines": 38,
                }),
                duration_ms: None,
            },
        ),
    );
    if look == Look::Group {
        let command = "rg -n \"client.request\\(\" src --json > calls.json";
        s.ev(
            start + 5,
            who,
            started(
                "call_calls_rg",
                "Bash",
                serde_json::json!({ "command": command }),
            ),
        );
        s.ev(
            start + 6,
            who,
            approval("call_calls_rg", command, &["rg", "-n"]),
        );
    }
}

/// Epicurean chocolate range (group.png, Pane 3): its prompt at 12:02 pm
/// and a read, a wait, then the build script failing and the turn with it,
/// three minutes before capture (`Stopped 3m ago`).
fn epicurean_range(s: &mut Script, who: ThreadId) {
    let start = hms(12, 2, 0);
    let stopped = CAPTURE - 3 * MINUTE;
    s.prompt(
        start,
        who,
        "Build the range sheet from the new chocolate prices",
    );
    s.ev(start, who, init("mockup-epicurean", "claude-opus-5-5"));
    s.ev(start, who, mode("acceptEdits"));
    s.tool(
        (start + 2, start + 3),
        who,
        "toolu_epicurean_read",
        "Read",
        serde_json::json!({ "file_path": "prices/epicurean.csv" }),
        quiet(),
    );
    s.ev(
        start + 4,
        who,
        SessionEvent::RunState {
            state: RunState::Idle,
        },
    );
    s.ev(
        stopped - 5,
        who,
        SessionEvent::RunState {
            state: RunState::Running,
        },
    );
    s.bash(
        stopped - 3,
        who,
        "toolu_epicurean_build",
        "python3 build_range.py",
        Run {
            stdout: "FileNotFoundError: range.xlsx".into(),
            exit: 1,
            ms: 3_000,
        },
    );
    s.ev(
        stopped,
        who,
        SessionEvent::TurnEnded {
            outcome: TurnOutcome::Error(String::new()),
            cost_usd: None,
        },
    );
}

/// Complete BB supplier list (group.png, Pane 4): asked at 11:58 am,
/// started 12:00:48 pm, done at 12:09 pm after 8m12s, not yet seen.
fn bb_list(s: &mut Script, who: ThreadId) {
    let start = hms(11, 58, 0);
    s.prompt(start, who, "Finish the BB supplier list");
    s.ev(start, who, init("mockup-bb", "claude-opus-5-5"));
    s.ev(start, who, mode("acceptEdits"));
    s.ev(
        start + 1,
        who,
        SessionEvent::RunState {
            state: RunState::Idle,
        },
    );
    let ran = hms(12, 0, 48);
    s.ev(
        ran,
        who,
        SessionEvent::RunState {
            state: RunState::Running,
        },
    );
    let rows: Vec<String> = (0..64)
        .map(|at| {
            format!(
                "+Supplier {at},orders@supplier{at}.example,{} days,{}",
                3 + at % 9,
                20 + at
            )
        })
        .collect();
    s.tool(
        (ran + 2, ran + 3),
        who,
        "toolu_bb_write",
        "Write",
        serde_json::json!({
            "file_path": "suppliers/bb.csv",
            "content": rows.iter().map(|row| &row[1..]).collect::<Vec<_>>().join("\n"),
        }),
        (
            String::new(),
            false,
            ToolResult::FileEdit {
                path: "suppliers/bb.csv".into(),
                hunks: vec![Hunk {
                    old_start: 0,
                    old_lines: 0,
                    new_start: 1,
                    new_lines: 64,
                    section: None,
                    lines: rows,
                }],
            },
        ),
    );
    let done = hms(12, 9, 0);
    s.ev(
        done - 5,
        who,
        text(
            "**The list is complete and saved.** Each supplier has a contact, lead time and \
             minimum order.",
        ),
    );
    s.end(done, who);
}

/// The three that need you outside WORK: two approvals and a question.
fn asks(s: &mut Script, cast: &Cast) {
    let start = hms(12, 12, 0);
    let who = cast.recipes;
    s.prompt(start, who, "Implement the recipes feature from the spec");
    s.ev(start, who, init("mockup-recipes", "claude-opus-5-5"));
    s.ev(start, who, mode("default"));
    s.ev(
        start + 40,
        who,
        text("The schema and the API are in. The migration needs to run before the tests.\n\n"),
    );
    for event in bash_approval("toolu_recipes_migrate", "npm run db:migrate") {
        s.ev(start + 42, who, event);
    }

    let start = hms(12, 15, 0);
    let who = cast.customers;
    s.prompt(
        start,
        who,
        "Draft the call list for moving customers to the new billing plan",
    );
    s.ev(start, who, init("mockup-customers", "claude-opus-5-5"));
    s.ev(start, who, mode("default"));
    s.ev(
        start + 20,
        who,
        started(
            "toolu_customers_ask",
            "AskUserQuestion",
            serde_json::json!({ "questions": [{ "question": "Who should be called first?" }] }),
        ),
    );
    s.ev(
        start + 21,
        who,
        question(
            "toolu_customers_ask",
            "Who should be called first?",
            &[
                ("largest accounts", "Start with the ten biggest invoices"),
                (
                    "oldest accounts",
                    "Start with the customers on the plan longest",
                ),
            ],
        ),
    );

    let start = hms(12, 18, 0);
    let who = cast.permission;
    s.prompt(start, who, "Deploy the strategy site");
    s.ev(start, who, init("mockup-permission", "claude-opus-5-5"));
    s.ev(start, who, mode("default"));
    for event in bash_approval("toolu_permission_deploy", "gh workflow run deploy.yml") {
        s.ev(start + 6, who, event);
    }
}

/// The Threads that only sit there: one finished turn each, earlier in the
/// day, and the three parked.
fn quiet_threads(s: &mut Script, cast: &Cast) {
    let [access, viability, country] = cast.parked;
    for (at, (who, ask, answer)) in [
        (
            cast.hedr,
            "Write the overview of Hedr: how it manages Rust agents.",
            "Drafted `docs/hedr.md`: the supervisor, the agent registry and the restart policy.",
        ),
        (
            cast.chess,
            "Sketch the principled-play evaluator for chess matches.",
            "The evaluator scores centre control, development and king safety before material.",
        ),
        (
            cast.business,
            "List what InvoiceMate Business adds over the free plan.",
            "Multi-entity books, approval chains and the audit export.",
        ),
        (
            cast.redesign,
            "Redesign the InvoiceMate invoice template.",
            "The new template leads with the amount due and the due date.",
        ),
        (
            cast.workmate,
            "Run the background maintenance pass.",
            "Pruned 14 stale branches and refreshed the lockfile.",
        ),
        (
            cast.tempo,
            "Build the overnight chess coach.",
            "The coach replays the day's games overnight and leaves three lessons.",
        ),
        (
            access,
            "Redesign the access screen.",
            "Roles now read as one table.",
        ),
        (
            viability,
            "Redesign the viability overview.",
            "The overview leads with margin per batch.",
        ),
        (
            country,
            "Fill in the country metadata for the strategy game.",
            "All 48 countries carry a capital, a region and a starting budget.",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let start = hms(9, 0, 0) + at as Secs * 20 * MINUTE;
        s.prompt(start, who, ask);
        s.ev(
            start,
            who,
            init(&format!("mockup-quiet-{at}"), "claude-opus-5-5"),
        );
        s.ev(start + 30, who, text(answer));
        s.end(start + 40, who);
    }
    for (at, who) in cast.parked.into_iter().enumerate() {
        s.park(hms(11, 50, 0) + at as Secs * MINUTE, who);
    }
}

// --------------------------------------------------------- the pull request

impl Pr {
    /// What the forge reads for the crash fix's branch.
    fn view(self, clock: Clock) -> PrView {
        let ago = |secs: Secs| rfc3339(clock.instant(CAPTURE - secs));
        let finding = |anchor: &str, body: &str, age: Secs, resolved: bool| Comment {
            kind: CommentKind::Finding,
            author: "claude".into(),
            author_is_bot: true,
            anchor: format!("{SCHEDULED_WORK}:{anchor}"),
            place: Some(FindingPlace::from_anchor(
                &format!("{SCHEDULED_WORK}:{anchor}"),
                None,
            )),
            body: body.into(),
            snippet: None,
            created_at: ago(age),
            is_resolved: resolved,
            is_outdated: false,
            replies: Vec::new(),
        };
        let retries = |age, resolved| finding("89-91", RETRIES, age, resolved);
        let untested = |resolved| finding("120", UNTESTED, 12 * MINUTE, resolved);
        // Resolved in every state: the first review's catch, long fixed.
        let popped = finding("88", POPPED, 40 * MINUTE, true);
        let check = |name: &str, status| Check {
            name: name.into(),
            status,
        };
        let checks = |macos, windows, review| {
            vec![
                check("macOS (Metal)", macos),
                check("Windows (DirectX 11)", windows),
                check("Claude Code Review", review),
            ]
        };
        use CheckStatus::{Failure, Running, Success};
        let (state, merge, sync, checks, comments) = match self {
            // The mockup heads these three `2 · 1 unresolved`; they are
            // three, the resolved one folded.
            Pr::Open => (
                PrState::Open,
                Merge::Clean,
                Sync::Unpushed(1),
                checks(Running, Success, Success),
                vec![
                    retries(6 * MINUTE, false),
                    Comment {
                        kind: CommentKind::Comment,
                        author: "josephwylie".into(),
                        author_is_bot: false,
                        anchor: "comment".into(),
                        place: None,
                        body: "Tried it locally. Closing the window mid-run no longer takes the \
                               server down."
                            .into(),
                        snippet: None,
                        created_at: ago(4 * MINUTE),
                        is_resolved: false,
                        is_outdated: false,
                        replies: Vec::new(),
                    },
                    popped,
                ],
            ),
            Pr::Failing => (
                PrState::Open,
                Merge::Conflicting,
                Sync::InSync,
                checks(Failure, Success, Success),
                vec![retries(12 * MINUTE, false), untested(false), popped],
            ),
            Pr::Merged => (
                PrState::Merged,
                Merge::Clean,
                Sync::InSync,
                checks(Success, Success, Success),
                vec![retries(12 * MINUTE, true), untested(true), popped],
            ),
            Pr::None => return PrView::NoPr,
        };
        PrView::Pr(Box::new(PrSnapshot {
            number: 146,
            title: "Requeue scheduled work whose worktree is gone".into(),
            url: "https://github.com/josephwylie/ferrite/pull/146".into(),
            body: "A job whose worktree was removed while it waited no longer panics the \
                   server: it goes back on the queue."
                .into(),
            state,
            is_draft: false,
            head_ref: BRANCH.into(),
            head_is_fork: false,
            head_oid: "5be1c0d4a1f2e3b4c5d6e7f8091a2b3c4d5e6f70".into(),
            base_ref: "main".into(),
            merge,
            sync,
            checks,
            comments,
            comments_truncated: false,
            checks_truncated: false,
        }))
    }
}

/// `at` as the forge stamps it: RFC 3339, UTC, whole seconds.
fn rfc3339(at: SystemTime) -> String {
    let secs = at
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or_default();
    let (days, rest) = (secs.div_euclid(DAY), secs.rem_euclid(DAY));
    // Days since the epoch to the civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / HOUR,
        rest % HOUR / MINUTE,
        rest % MINUTE
    )
}

// ---------------------------------------------------------------- the copy

/// The follow-up queued behind the crash fix's turn, as the operator typed
/// it.
const QUEUED: &str = "Also once this is done, i wnat to make terminal ui overhaul the main of \
this repo please,e and remove the other oberhaul branch and the codex full access branch remove \
as well";

const RED: &str = "**Red, with the exact panic from your crash reports.** `job.spawn().unwrap()` \
panics when the worktree it was queued for has been removed. Fixing it.\n\n";

const FIXED: &str = "**Fixed, and the full core suite passes.** A job whose worktree is gone now \
goes back on the queue instead of taking the server down.\n\n\
- Free disk dropped to **7.5 GB**, so I'm checking what grew before building anything else.\n\
- Clearing the **5 GB** incremental cache my test runs left in `target/`. Cargo rebuilds it on \
demand.\n\n";

/// The seven commands before the panic was found: what each ran, for how
/// many seconds, how it exited and what it printed. The system log refused.
const INVESTIGATION: [(&str, u64, i64, &str); 7] = [
    (
        "ps -axo pid,lstart,etime,command | grep -i ferrite | grep -v grep",
        2,
        0,
        "27945 Wed 21 Oct 12:08:56 2026     16:52 /Applications/Ferrite.app/Contents/MacOS/ferrite",
    ),
    (
        "ls -t ~/Library/Logs/Ferrite | head -5",
        1,
        0,
        "server.log\nwindow.log",
    ),
    (
        "tail -n 40 ~/Library/Logs/Ferrite/server.log",
        2,
        0,
        "12:21:50 scheduled work: spawning job 41 in .worktrees/chess-coach\n\
         12:21:50 thread 'scheduler' panicked at crates/ferrite-core/src/scheduled_work.rs:89:28",
    ),
    (
        "log show --last 30m --predicate 'process == \"ferrite\"' --style compact",
        20,
        1,
        "log: Cannot run while sandboxed",
    ),
    (
        "ls ~/Library/Logs/DiagnosticReports | grep -i ferrite",
        3,
        0,
        "ferrite-2026-10-21-122150.ips",
    ),
    (
        "head -40 ~/Library/Logs/DiagnosticReports/ferrite-2026-10-21-122150.ips",
        4,
        0,
        "\"exception\" : {\"type\":\"EXC_CRASH\",\"signal\":\"SIGABRT\"}",
    ),
    (
        "git worktree list",
        2,
        0,
        "~/Desktop/Projects/ferrite  e37884d [main]",
    ),
];

/// The crash fix's 23 finished Subagents, oldest first.
const FINISHED_SUBAGENTS: [&str; 23] = [
    "Standards review of crash fix",
    "Find where scheduled work spawns",
    "Read the crash reports",
    "Trace the server's restart path",
    "Check the worktree prune timing",
    "Map the queue's owners",
    "Audit unwrap calls in core",
    "Reproduce the panic in a test",
    "Read the server log rotation",
    "List the worktrees on disk",
    "Measure idle memory at launch",
    "Measure idle memory after an hour",
    "Profile the transcript cache",
    "Compare the 0.4 and 0.5 heaps",
    "Find what holds the old Sessions",
    "Check the watchdog's tick",
    "Review the requeue backoff",
    "Read the scheduler's tests",
    "Check disk use under target",
    "Summarise the incremental cache",
    "Look for other spawn unwraps",
    "Draft the PR description",
    "Check the CI matrix",
];

/// Where Find all calls' 38 matches are.
const CALL_FILES: [&str; 11] = [
    "accounts",
    "billing",
    "customers",
    "invoices",
    "ledger",
    "orders",
    "payments",
    "products",
    "reports",
    "suppliers",
    "users",
];

const RETRIES: &str = "`requeue` retries forever if the worktree never comes back. Cap the \
attempts or drop the job after a few.";

const UNTESTED: &str = "The new branch has no test. A job whose worktree is removed between \
queue and spawn should requeue once and then be dropped.";

const POPPED: &str = "`pop_front` takes the job off the queue before the spawn can fail, so a \
failure loses it.";

#[cfg(test)]
mod tests {
    use super::super::world::midnight;
    use super::super::world::tests::assert_applies;
    use super::*;

    #[test]
    fn the_fix_applies_where_the_mockup_draws_it() {
        let before = scheduled_work_before();
        assert_eq!(before.len(), 140);
        assert_eq!(before[87], "let job = queue.pop_front()?;");
        assert_eq!(
            before[119],
            "fn requeue(&mut self, job: Job) -> Option<Handle> {"
        );
        let hunk = crash_fix_hunk();
        assert_applies(&before, std::slice::from_ref(&hunk));
        let count = |marker| hunk.lines.iter().filter(|l| l.starts_with(marker)).count();
        assert_eq!((count('+'), count('-')), (4, 1), "the mockups' `+4 −1`");
    }

    #[test]
    fn a_forge_stamp_is_rfc3339_in_utc() {
        let at = UNIX_EPOCH + std::time::Duration::from_secs(1_792_067_490);
        assert_eq!(rfc3339(at), "2026-10-15T12:31:30Z");
        assert_eq!(rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn the_pull_request_reads_as_each_column_of_the_mockup() {
        let clock = Clock {
            midnight: midnight(),
            fixture: None,
        };
        let PrView::Pr(open) = Pr::Open.view(clock) else {
            panic!("an open PR");
        };
        assert_eq!(open.number, 146);
        assert_eq!(open.sync, Sync::Unpushed(1));
        let unresolved = open.comments.iter().filter(|c| !c.is_resolved).count();
        assert_eq!((open.comments.len(), unresolved), (3, 2));
        let PrView::Pr(failing) = Pr::Failing.view(clock) else {
            panic!("a failing PR");
        };
        assert_eq!(failing.merge, Merge::Conflicting);
        assert_eq!(failing.checks[0].status, CheckStatus::Failure);
        let PrView::Pr(merged) = Pr::Merged.view(clock) else {
            panic!("a merged PR");
        };
        assert!(merged.comments.iter().all(|c| c.is_resolved));
        assert_eq!(Pr::None.view(clock), PrView::NoPr);
    }

    #[test]
    fn the_world_holds_the_mockups_roster() {
        let root =
            std::env::temp_dir().join(format!("ferrite-mockup-world-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let clock = Clock {
            midnight: midnight(),
            fixture: None,
        };
        let world = build(&root, Look::Group, Pr::Open, clock);
        let core = &world.core;
        assert_eq!(core.threads().len(), 14, "fourteen Threads are open");
        assert_eq!(core.parked().unwrap().len(), 3, "three are parked");
        assert_eq!(core.registry().projects().len(), 7);
        let titles: Vec<String> = core.groups().iter().map(|g| g.display_title()).collect();
        assert_eq!(titles, ["WORK", "InvoiceMate Business"]);
        let asking = core.needs_you();
        assert_eq!(asking.len(), 4, "four need you on the Group");
        for thread in [
            world.cast.recipes,
            world.cast.customers,
            world.cast.permission,
        ] {
            assert!(asking.contains(&thread));
        }
        let focused = core.roster().focused_thread().expect("a focused Thread");
        assert_eq!(
            core.groups().of(focused).map(|group| group.display_title()),
            Some("WORK".to_string()),
            "Supplier Price Review, on WORK"
        );
        let memory = core.thread(world.cast.memory).unwrap();
        assert_eq!(memory.model(), Some("opus"));
        assert_eq!(memory.effort(), Some("high"));
        drop(world);
        let _ = std::fs::remove_dir_all(&root);
    }
}
