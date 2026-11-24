//! The PR watcher through the Server (ADR 0046): watches started over the
//! API, read from a scripted GitHub on a scripted clock, and the wakes
//! that reach the agent's transcript.

use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use super::*;
use ferrite_core::factory_board::{GitHubQuery, GitHubRead};
use ferrite_core::pr_watch::{
    Check, CheckStatus, Fingerprint, Mergeable, ReadError, Reading, Remark,
};
use ferrite_core::pull_request::PrState;
use ferrite_core::server::github::GitHub;
use ferrite_core::wake::WakeNote;

const URL: &str = "https://github.com/acme/app/pull/7";
const MINUTE: i64 = 60_000;
/// Wednesday 2026-11-11, noon UTC.
const NOON: i64 = 1_794_398_400_000;

/// GitHub as the test scripts it: PR #7 as it stands, or what every read
/// of it meets.
#[derive(Default)]
struct Forge {
    pr: Mutex<Option<Result<Reading, ReadError>>>,
    /// Fingerprints asked for: one each time a pass looks at the PR.
    looks: AtomicUsize,
}

impl Forge {
    fn is(&self, reading: Reading) {
        *self.pr.lock().unwrap() = Some(Ok(reading));
    }

    fn fails(&self, error: ReadError) {
        *self.pr.lock().unwrap() = Some(Err(error));
    }

    fn looks(&self) -> usize {
        self.looks.load(Ordering::SeqCst)
    }

    fn now(&self) -> Result<Reading, ReadError> {
        self.pr
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(Err(ReadError::Failed("not scripted".into())))
    }
}

impl GitHub for Forge {
    fn matches(&self, _: &GitHubQuery) -> Result<GitHubRead, String> {
        Ok(GitHubRead::Unchanged)
    }

    fn repositories(&self) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    fn labels(&self, _: &str) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    fn head(&self, repository: &str, number: u64) -> Result<String, String> {
        Err(format!("{repository}#{number}: not found"))
    }

    /// Moves whenever the PR does.
    fn pr_fingerprint(&self, _: &str, _: u64) -> Result<Fingerprint, ReadError> {
        self.looks.fetch_add(1, Ordering::SeqCst);
        let pr = self.now()?;
        Ok(Fingerprint {
            status: format!("{:?} {:?} {:?} {:?}", pr.state, pr.head, pr.checks, pr.mergeable),
            remarks: format!("{:?}", pr.remarks),
        })
    }

    fn pr_reading(&self, _: &str, _: u64, remarks: bool) -> Result<Reading, ReadError> {
        let mut pr = self.now()?;
        if !remarks {
            pr.remarks = None;
        }
        Ok(pr)
    }
}

fn check(status: CheckStatus) -> Check {
    Check {
        name: "ci".into(),
        status,
        url: Some("https://github.com/acme/app/actions/runs/1/job/2".into()),
        required: false,
        placeholder: false,
    }
}

/// PR #7, open on head `abc1234`, with one check.
fn open_pr(status: CheckStatus) -> Reading {
    Reading {
        state: PrState::Open,
        head: Some("abc1234def".into()),
        base_branch: "main".into(),
        checks: vec![check(status)],
        mergeable: Mergeable::Mergeable,
        viewer: Some("agent-bot".into()),
        author: Some("agent-bot".into()),
        remarks: Some(Vec::new()),
        merge_queue_removal: None,
        merged_by: None,
        merge_commit: None,
    }
}

fn ended_pr(state: PrState) -> Reading {
    Reading {
        state,
        merged_by: (state == PrState::Merged).then(|| "octo".into()),
        merge_commit: (state == PrState::Merged).then(|| "feedface00".into()),
        ..open_pr(CheckStatus::Success)
    }
}

/// A Server on a clock and a GitHub the test holds, with one Thread whose
/// agent is up, watched by a window.
struct World {
    harness: Harness,
    window: Window,
    thread: ThreadId,
    agent: Sender<SessionEvent>,
    clock: Arc<AtomicI64>,
    forge: Arc<Forge>,
}

fn configured(clock: &Arc<AtomicI64>, forge: &Arc<Forge>) -> Configure {
    let (time, reader) = (Arc::clone(clock), Arc::clone(forge));
    Box::new(move |server: &mut Server| {
        server.clock = Arc::new(move || time.load(Ordering::SeqCst));
        server.github = reader;
    })
}

fn world(name: &str) -> World {
    let clock = Arc::new(AtomicI64::new(NOON));
    let forge = Arc::new(Forge::default());
    forge.is(open_pr(CheckStatus::Pending));
    let harness = serve_configured(
        scratch(name),
        None,
        Tuning::default(),
        configured(&clock, &forge),
    );
    let mut window = attach(&harness.address);
    let (thread, agent) = open(&mut window, &harness);
    window.watch(thread, None);
    World {
        harness,
        window,
        thread,
        agent,
        clock,
        forge,
    }
}

impl World {
    fn api(&self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let mut params = params;
        params["thread"] = serde_json::json!(self.thread);
        call(&self.harness).answer("1", method, params)
    }

    /// Watch PR #7, and wait for its baseline read.
    fn watch(&self) -> serde_json::Value {
        let looked = self.forge.looks();
        let answer = self.api("pr.watch", serde_json::json!({ "url": URL }));
        self.looked_since(looked);
        answer["result"].clone()
    }

    /// Let a sweep come due, and wait for it to look at the PR.
    fn sweep(&self) {
        let looked = self.forge.looks();
        self.clock.fetch_add(3 * MINUTE, Ordering::SeqCst);
        self.looked_since(looked);
    }

    /// Let a sweep come due when no watch is left to look at.
    fn idle_sweep(&self) {
        self.clock.fetch_add(3 * MINUTE, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(400));
    }

    /// Wait until a pass looked at the PR after `looked`, and let the pump
    /// weigh what it read.
    fn looked_since(&self, looked: usize) {
        let deadline = Instant::now() + PATIENCE;
        while self.forge.looks() <= looked {
            assert!(Instant::now() < deadline, "no pass looked at the PR");
            std::thread::sleep(Duration::from_millis(5));
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    /// The wakes the window's transcript shows: each one's notice and text.
    fn wakes(&mut self) -> Vec<(WakeNote, String)> {
        // Take what has arrived.
        while let Ok(message) = self.window.client.messages.try_recv() {
            self.window.receive(message);
        }
        wakes_in(&self.window, self.thread)
    }

    /// Wait for the `n`th wake, and answer it.
    fn woken(&mut self, n: usize) -> (WakeNote, String) {
        let thread = self.thread;
        self.window
            .until(|window| wakes_in(window, thread).len() >= n);
        let wakes = wakes_in(&self.window, thread);
        assert_eq!(wakes.len(), n, "{wakes:?}");
        wakes[n - 1].clone()
    }

    fn watching(&self) -> bool {
        let answer = self.api("pr.unwatch", serde_json::json!({ "url": URL }));
        answer["result"]["was_watching"] == true
    }
}

pub(super) fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else if std::fs::copy(entry.path(), &target).is_err() {
            // A socket or a lock is the old Server's, not the store.
        }
    }
}

pub(super) fn wakes_in(window: &Window, thread: ThreadId) -> Vec<(WakeNote, String)> {
    window.mirrors.view(thread).map_or(Vec::new(), |view| {
        view.transcript()
            .blocks()
            .iter()
            .filter_map(|block| match (&block.wake, &block.body) {
                (Some(note), Body::Prompt(text)) => Some((note.clone(), text.clone())),
                _ => None,
            })
            .collect()
    })
}

/// AC: the first read is the baseline and wakes nothing; a check that then
/// fails wakes the agent once, in T3's words, and the next sweep repeats
/// nothing.
#[test]
fn a_failed_check_wakes_the_agent_once_after_a_silent_baseline() {
    let mut world = world("prw-failed");
    let answer = world.watch();
    assert_eq!(
        (&answer["watching"], &answer["was_watching"], &answer["url"]),
        (&serde_json::json!(true), &serde_json::json!(false), &serde_json::json!(URL)),
        "{answer}"
    );
    world.sweep();
    assert!(world.wakes().is_empty(), "the baseline wakes nothing");

    world.forge.is(open_pr(CheckStatus::Failure));
    world.sweep();
    let (note, text) = world.woken(1);
    assert_eq!(note.summary, "#7: checks failed");
    assert_eq!(note.outcome, ferrite_core::wake::Outcome::Failed);
    assert!(
        text.starts_with(&format!(
            "Update on pull request #7 ({URL}), which Ferrite is watching for you:\n- Checks failed on abc1234:\n  - ci https://github.com/acme/app/actions/runs/1/job/2"
        )),
        "{text}"
    );
    turn_ends(&world.agent);
    world.sweep();
    world.sweep();
    assert_eq!(world.wakes().len(), 1, "told once");
}

/// AC: news saved before its wake went is not told again by the Server
/// that loads the store next.
#[test]
fn a_reloaded_store_repeats_nothing() {
    let mut world = world("prw-reload");
    world.watch();
    world.forge.is(open_pr(CheckStatus::Failure));
    world.sweep();
    world.woken(1);
    let World {
        harness,
        mut window,
        clock,
        forge,
        thread,
        ..
    } = world;
    window.command(90, Op::Stop { when: When::Now });
    assert_eq!(harness.server.join().unwrap(), 0);
    // The in-process harness keeps its listeners after shutdown, so the
    // successor loads a copy of the store.
    let dir = scratch("prw-reload-successor");
    copy_dir(&harness.dir, &dir);
    // The successor's first sweep runs at once.
    let looked = forge.looks();
    let successor = serve_configured(
        dir,
        None,
        Tuning::default(),
        configured(&clock, &forge),
    );
    let mut window = attach(&successor.address);
    window.watch(thread, None);
    let deadline = Instant::now() + PATIENCE;
    while forge.looks() <= looked {
        assert!(Instant::now() < deadline, "the successor never swept");
        std::thread::sleep(Duration::from_millis(5));
    }
    window.until(|window| !wakes_in(window, thread).is_empty());
    std::thread::sleep(Duration::from_millis(400));
    while let Ok(message) = window.client.messages.try_recv() {
        window.receive(message);
    }
    assert_eq!(wakes_in(&window, thread).len(), 1, "only the first Server's wake");
}

/// AC: while a Decision waits on the operator, the news is not saved and
/// no wake is sent; once the agent is free, the next sweep tells it.
#[test]
fn a_pending_decision_holds_the_news() {
    let mut world = world("prw-decision");
    world.watch();
    world.agent.send(decision()).unwrap();
    let thread = world.thread;
    let handle = pending_handle(&mut world.window, thread, None);
    world.forge.is(open_pr(CheckStatus::Failure));
    world.sweep();
    assert!(world.wakes().is_empty(), "held while the Decision waits");

    world.window.command(91, allow(thread, &handle));
    turn_ends(&world.agent);
    world.sweep();
    let (note, _) = world.woken(1);
    assert_eq!(note.summary, "#7: checks failed");
}

/// AC: news for an agent mid-turn waits for the turn to end, then goes on
/// its own, with no further read; a wake whose watch ended first is
/// dropped.
#[test]
fn a_busy_agent_gets_the_wake_after_its_turn() {
    let mut world = world("prw-busy");
    world.watch();
    world.agent.send(text("working")).unwrap();
    world.forge.is(open_pr(CheckStatus::Failure));
    world.sweep();
    assert!(world.wakes().is_empty(), "queued behind the turn");
    turn_ends(&world.agent);
    let (note, _) = world.woken(1);
    assert_eq!(note.summary, "#7: checks failed");

    // A wake waiting when its watch ends is cancelled.
    turn_ends(&world.agent);
    world.agent.send(text("more work")).unwrap();
    world.forge.is(open_pr(CheckStatus::Success));
    world.sweep();
    assert!(world.watching(), "unwatched with a wake waiting");
    turn_ends(&world.agent);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(world.wakes().len(), 1, "the stale wake was dropped");
}

/// AC: a merge or a close ends the watch with one wake.
#[test]
fn a_merge_or_a_close_ends_the_watch_with_one_wake() {
    for (state, summary, words) in [
        (
            PrState::Merged,
            "#7: merged, stopped watching",
            format!("Pull request #7 ({URL}) was merged into main by octo as feedfac, so Ferrite stopped watching it."),
        ),
        (
            PrState::Closed,
            "#7: closed, stopped watching",
            format!("Pull request #7 ({URL}) was closed without merging, so Ferrite stopped watching it. If it is reopened, call watch_pull_request to watch it again."),
        ),
    ] {
        let mut world = world(&format!("prw-end-{state:?}"));
        world.watch();
        world.forge.is(ended_pr(state));
        world.sweep();
        let (note, text) = world.woken(1);
        assert_eq!((note.summary.as_str(), text), (summary, words));
        turn_ends(&world.agent);
        world.idle_sweep();
        assert_eq!(world.wakes().len(), 1, "one wake");
        assert!(!world.watching(), "the watch ended");
    }
}

/// AC: a merge the agent's own `gh pr merge` made ends the watch with no
/// wake (T3's `agentEnds`).
#[test]
fn a_merge_the_agent_made_ends_the_watch_with_no_wake() {
    let mut world = world("prw-self-merge");
    world.watch();
    world
        .agent
        .send(SessionEvent::ToolStarted {
            id: "toolu_merge".into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": "gh pr merge 7 --squash" }),
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    world.forge.is(ended_pr(PrState::Merged));
    let looked = world.forge.looks();
    world
        .agent
        .send(SessionEvent::ToolCompleted {
            id: "toolu_merge".into(),
            output: "Merged".into(),
            is_error: false,
            result: Default::default(),
        })
        .unwrap();
    // The command's end reads the Thread's watches at once.
    world.looked_since(looked);
    turn_ends(&world.agent);
    world.idle_sweep();
    assert!(world.wakes().is_empty(), "{:?}", world.wakes());
    assert!(!world.watching(), "the watch ended");
}

/// AC: stopping the agent, or parking its Thread, ends its watches with no
/// wake.
#[test]
fn interrupting_or_parking_ends_the_watch_with_no_wake() {
    let mut world = world("prw-interrupt");
    world.watch();
    world.agent.send(text("working")).unwrap();
    let interrupted = world.api("agent.interrupt", serde_json::json!({}));
    assert!(interrupted["error"].is_null(), "{interrupted}");
    world.forge.is(ended_pr(PrState::Merged));
    world.idle_sweep();
    assert!(world.wakes().is_empty());
    assert!(!world.watching(), "interrupting ended the watch");

    let world = self::world("prw-park");
    world.watch();
    let parked = world.api("thread.close", serde_json::json!({}));
    assert!(parked["error"].is_null(), "{parked}");
    world.forge.is(ended_pr(PrState::Merged));
    world.idle_sweep();
    let revived = world.api("thread.revive", serde_json::json!({}));
    assert!(revived["error"].is_null(), "{revived}");
    assert!(!world.watching(), "parking ended the watch");
}

/// AC: a rate limit pauses the watch without counting; eight failed reads
/// in a row give up, with T3's give-up wake.
#[test]
fn a_rate_limit_pauses_and_eight_failed_reads_give_up() {
    let mut world = world("prw-give-up");
    world.watch();
    world.forge.fails(ReadError::RateLimited);
    for _ in 0..3 {
        world.sweep();
    }
    world.forge.fails(ReadError::Refused("Could not resolve to a PullRequest".into()));
    for _ in 0..7 {
        world.sweep();
    }
    assert!(world.wakes().is_empty(), "seven failures wake nothing");
    world.sweep();
    let (note, text) = world.woken(1);
    assert_eq!(note.summary, "#7: stopped watching, could not read it");
    assert_eq!(
        text,
        format!("Ferrite stopped watching pull request #7 ({URL}) because it failed to read it from the host 8 times in a row. Check it yourself, and call watch_pull_request only if it is still open.")
    );
    assert!(!world.watching());
}

/// AC: a PR already merged starts no watch, and says so.
#[test]
fn an_ended_pr_starts_no_watch() {
    let world = world("prw-ended");
    world.forge.is(ended_pr(PrState::Merged));
    let answer = world.api("pr.watch", serde_json::json!({ "url": URL }))["result"].clone();
    assert_eq!(answer["watching"], false, "{answer}");
    assert_eq!(answer["state"], "merged", "{answer}");
    assert_eq!(
        answer["next"],
        "This PR is already merged, so there is nothing to watch. Report that and end your turn."
    );
    assert!(!world.watching());
}

/// Remarks are news only once.
#[test]
fn a_new_comment_wakes_the_agent() {
    let mut world = world("prw-comment");
    world.watch();
    let mut pr = open_pr(CheckStatus::Pending);
    pr.remarks = Some(vec![Remark {
        id: "c1".into(),
        author: Some("reviewer".into()),
        body: "Please rename this.".into(),
        created_ms: NOON + MINUTE,
        edited_ms: None,
        url: Some(format!("{URL}#issuecomment-1")),
        path: None,
        review_state: None,
    }]);
    world.forge.is(pr);
    world.clock.store(NOON + 2 * MINUTE, Ordering::SeqCst);
    world.sweep();
    let (note, text) = world.woken(1);
    assert_eq!(note.summary, "#7: new comments");
    assert!(text.contains("Please rename this."), "{text}");
}

/// The agent's own `watch_pull_request` hangs up as soon as it asks, and
/// the answer, given after the fresh read, still reaches it. A
/// subagent's watch is refused before the Server hears it.
#[test]
fn the_agents_own_tool_watches_its_pr() {
    let world = world("prw-tool");
    let tools = world.harness.tools.for_thread(world.thread, None, false);
    let watch = |main: bool| {
        let (told, said) = mpsc::channel();
        let pr = serde_json::json!({ "url": URL });
        tools.call("watch_pull_request", &pr, main, move |answer| {
            told.send((answer.text, answer.is_error)).unwrap()
        });
        said.recv_timeout(PATIENCE).unwrap()
    };
    assert_eq!(
        watch(false),
        (ferrite_core::api::tools::SUBAGENT_WATCH.to_string(), true)
    );
    let (text, is_error) = watch(true);
    assert!(!is_error, "{text}");
    assert!(text.contains(r#""watching":true"#), "{text}");
    assert!(world.watching());
}
