//! The Server's PR watcher (ADR 0046), T3 Code's
//! `PullRequestWatchReactor`. A pass reads the watched PRs off the pump,
//! one read per PR however many Threads watch it, at most
//! [`GROUP_READS`] at once. A sweep reads every one, [`SWEEP_MS`] after the
//! last sweep ended; a pass for one PR runs at once when its watch starts,
//! and when its agent's merge or close command ends. Each read is weighed
//! against each watch (`pr_watch::evaluate`): a cursor that only moved is
//! saved at once, and news is held until the pass ends, so each Thread
//! gets one wake for everything the pass found. News is saved before its
//! wake is held (persist, then act); a Thread with a Decision pending or a
//! wake already waiting has none saved, so the next read finds it again.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::{mpsc, Arc};

use serde_json::{json, Value};

use ferrite_core::api::schema::{PrParams, PrWatchParams};
use ferrite_core::api::{unknown_thread, ApiError, CallerId, Code};
use ferrite_core::pr_watch::{
    self, Change, CheckStatus, Closes, End, Fingerprint, Mergeable, ReadError, Reading, Report,
};
use ferrite_core::pull_request::{self, Cursor, PrState, Stamp, Target, Until, Watch};
use ferrite_core::server::github::GitHub;
use ferrite_core::transcript::{tools, Body, ToolState};
use ferrite_core::wake::Wake;
use ferrite_core::connection::ClientId;
use ferrite_core::protocol::{Done, ErrorCode, WireError};
use ferrite_core::ThreadId;

use super::wake::Section;
use super::Server;

/// Checks take minutes; faster sweeps waste the account's shared quota.
pub(super) const SWEEP_MS: i64 = 2 * 60_000;
/// Group reads in flight at once.
const GROUP_READS: usize = 4;
/// With no fingerprint, a quiet PR is read again this often, to catch
/// edits.
const QUIET_REREAD_MS: i64 = 10 * 60_000;
/// A PR is read again this often whatever its fingerprint says: it misses
/// edits to review-thread comments.
const FINGERPRINT_REREAD_MS: i64 = 30 * 60_000;

/// A PR as its watches are grouped: `owner/name` lowercased, and its
/// number. The Server reads every PR as its own `gh` account, so one read
/// serves every Thread watching it.
type Key = (String, u64);

fn key_of(watch: &Watch) -> Key {
    (watch.repository.to_ascii_lowercase(), watch.number)
}

#[derive(Default)]
pub(super) struct Watcher {
    /// When the next sweep is due: at once, on boot.
    sweep_due_ms: i64,
    /// Passes asked for, in order.
    asked: VecDeque<Ask>,
    pass: Option<Pass>,
    last_reads: HashMap<Key, LastRead>,
    /// Failed reads in a row.
    failures: HashMap<Key, u32>,
    /// Each open Thread's merge and close commands.
    commands: HashMap<ThreadId, Commands>,
    /// `pr.watch` calls waiting on their fresh read.
    starts: Vec<Start>,
}

/// A pass asked for.
struct Ask {
    /// These PRs; every watched one for a sweep.
    only: Option<BTreeSet<Key>>,
    /// Read them in full, whatever the last read saw.
    fresh: bool,
    /// The agent's commands whose read this is: once it is done, they no
    /// longer count as the agent's own end of a PR.
    release: Vec<(ThreadId, String)>,
}

struct Pass {
    sweep: bool,
    release: Vec<(ThreadId, String)>,
    waiting: VecDeque<Group>,
    reading: Vec<(Group, mpsc::Receiver<Read>)>,
    held: Vec<Held>,
}

/// The watches on one PR.
#[derive(Clone)]
struct Group {
    key: Key,
    watches: Vec<Watched>,
}

#[derive(Clone)]
struct Watched {
    thread: ThreadId,
    watch: Watch,
}

impl Watched {
    /// Which watch this is: a new watch on the PR needs a full read.
    fn id(&self) -> (ThreadId, i64) {
        (self.thread, self.watch.cursor.started_ms)
    }
}

/// What the last read of a PR saw.
#[derive(Clone)]
struct LastRead {
    at_ms: i64,
    /// Checks or mergeability were still settling.
    in_flight: bool,
    remarks_complete: bool,
    watches: HashSet<(ThreadId, i64)>,
    status: Option<String>,
    remarks: Option<String>,
}

/// A group read's outcome.
enum Read {
    /// Rate limited: the watch waits for the next pass.
    Paused,
    /// Nothing changed that a read would see.
    Quiet,
    Failed(ReadError),
    Done {
        reading: Reading,
        /// Remarks were asked for.
        activity: bool,
        fingerprint: Option<Fingerprint>,
        now_ms: i64,
    },
}

struct Plan {
    detail: bool,
    activity: bool,
}

/// News a pass holds for its end.
struct Held {
    key: Key,
    watched: Watched,
    wake: Wake,
    commit: Commit,
    remarks: bool,
}

/// What a wake's news saves.
enum Commit {
    Advance(Cursor),
    /// The watch ends: keeping what its final read saw, or as it stands.
    End(Option<Cursor>),
}

/// A Thread's merge and close commands (`pr_watch::closed_by`). One counts
/// as the agent's own end of the PRs it names while it runs and, if it
/// succeeded, until the read at its end is done.
#[derive(Default)]
struct Commands {
    /// The transcript revision and length last scanned; `None` before the
    /// first scan, which takes the commands already ended as old.
    revision: Option<u64>,
    scanned: usize,
    /// By call, with the block each is.
    running: BTreeMap<String, (usize, Closes)>,
    succeeded: BTreeMap<String, Closes>,
    ended: HashSet<String>,
}

impl Commands {
    fn closes(&self, number: u64) -> bool {
        self.running
            .values()
            .map(|(_, closes)| closes)
            .chain(self.succeeded.values())
            .any(|closes| closes.names(number))
    }
}

/// A `pr.watch` call or a window's `watch_pr` waiting on its fresh read.
struct Start {
    asker: Asker,
    thread: ThreadId,
    target: Target,
    until: Option<Until>,
    answer: mpsc::Receiver<Result<Reading, ReadError>>,
}

/// Who started a watch, and is answered once its fresh read is in.
enum Asker {
    Api { caller: CallerId, id: String },
    Window { client: ClientId, request: u64 },
}

impl Server {
    /// One pump pass of the watcher: notice the agents' merge and close
    /// commands, answer the watch calls whose read is in, step the pass,
    /// and send the wakes that can go.
    pub(super) fn serve_pr_watches(&mut self) {
        self.scan_commands();
        self.hear_watch_starts();
        self.step_pass();
        if self.watcher.pass.is_none() {
            let ask = match self.watcher.asked.pop_front() {
                Some(ask) => Some(ask),
                None => ((self.clock)() >= self.watcher.sweep_due_ms).then_some(Ask {
                    only: None,
                    fresh: false,
                    release: Vec::new(),
                }),
            };
            if let Some(ask) = ask {
                self.start_pass(ask);
                self.step_pass();
            }
        }
    }

    /// `pr.watch`: watch a PR for a Thread, answered at once when it is
    /// already watched, else after a fresh read says whether it is still
    /// open (`None`: the answer comes then).
    pub(super) fn api_pr_watch(
        &mut self,
        caller: CallerId,
        id: &str,
        from: Option<ThreadId>,
        params: PrWatchParams,
    ) -> Option<Result<Value, ApiError>> {
        let (thread, target) = match self.pr_target(from, &params.pr) {
            Ok(found) => found,
            Err(error) => return Some(Err(error)),
        };
        let until = params.until.map(Until::from);
        let asker = Asker::Api {
            caller,
            id: id.to_string(),
        };
        let started = self.start_watch(asker, thread, target, until);
        if started.is_none() {
            self.api.owe(caller, id);
        }
        started
    }

    /// A window's `watch_pr` that starts a watch, at its default `until`,
    /// as T3's PR menu does (`pr.watch`). Stopping one is the Engine's.
    pub(super) fn window_watch_pr(
        &mut self,
        client: ClientId,
        request: u64,
        thread: ThreadId,
        url: &str,
    ) {
        let params = PrParams {
            thread: Some(thread),
            url: Some(url.to_string()),
            ..PrParams::default()
        };
        let asker = Asker::Window { client, request };
        let answered = match self.pr_target(None, &params) {
            Ok((thread, target)) => match self.start_watch(asker, thread, target, None) {
                Some(answered) => answered,
                None => return,
            },
            Err(error) => Err(error),
        };
        self.answer_start(Asker::Window { client, request }, answered);
    }

    /// Watch `target` for `thread`: at once when it is already watched
    /// (`Some`, the answer), else after a fresh read says whether it is
    /// still open (`None`: `asker` is answered then).
    fn start_watch(
        &mut self,
        asker: Asker,
        thread: ThreadId,
        target: Target,
        until: Option<Until>,
    ) -> Option<Result<Value, ApiError>> {
        let watched = self
            .engine
            .pr_watch(thread)
            .is_some_and(|fact| fact.on(&target.repository, target.number).is_some());
        if watched {
            let answer = answer(&target, true, true);
            return Some(self.watch_pr(thread, target, until, None).map(|_| answer));
        }
        let (tell, answer) = mpsc::channel();
        let github = Arc::clone(&self.github);
        let (repository, number) = (target.repository.clone(), target.number);
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
            let _ = tell.send(github.pr_reading(&repository, number, false));
        });
        self.watcher.starts.push(Start {
            asker,
            thread,
            target,
            until,
            answer,
        });
        None
    }

    /// Answer whoever started a watch. A window reads a PR that already
    /// ended as a refusal, in the words the agent is told.
    fn answer_start(&mut self, asker: Asker, answered: Result<Value, ApiError>) {
        match asker {
            Asker::Api { caller, id } => self.api.reply(caller, id, answered),
            Asker::Window { client, request } => {
                let failed = |message| WireError {
                    code: ErrorCode::Failed,
                    message,
                };
                let replied = match answered {
                    Ok(value) => match value.get("state").and_then(Value::as_str) {
                        Some(state) => Err(failed(format!(
                            "This PR is already {state}, so there is nothing to watch."
                        ))),
                        None => Ok(Some(Done::PrWatch {
                            watching: value["watching"].as_bool().unwrap_or_default(),
                        })),
                    },
                    Err(error) => Err(failed(error.message)),
                };
                self.reply(client, request, replied);
            }
        }
    }

    /// `pr.unwatch`: stop watching a PR for a Thread.
    pub(super) fn api_pr_unwatch(
        &mut self,
        from: Option<ThreadId>,
        params: &PrParams,
    ) -> Result<Value, ApiError> {
        let (thread, target) = self.pr_target(from, params)?;
        let was = self
            .engine
            .unwatch_pr(thread, &target)
            .map_err(|error| ApiError::new(Code::Failed, error.to_string()))?;
        Ok(answer(&target, false, was))
    }

    /// The open Thread and the PR a call names.
    fn pr_target(
        &self,
        from: Option<ThreadId>,
        params: &PrParams,
    ) -> Result<(ThreadId, Target), ApiError> {
        let thread = params.thread.or(from).ok_or_else(|| {
            ApiError::new(
                Code::InvalidRequest,
                "Pass thread: this call is not running inside a Ferrite thread.",
            )
        })?;
        self.engine.pr_watch(thread).ok_or_else(unknown_thread)?;
        let target = match (&params.url, &params.repository, params.number) {
            (Some(url), _, _) => Target::of_url(url.trim()),
            (None, Some(repository), Some(number)) => Target::of(repository.trim(), number),
            _ => None,
        };
        let target = target.ok_or_else(|| {
            ApiError::new(
                Code::InvalidRequest,
                "Name the pull request by its GitHub url, or by repository (owner/name) and number.",
            )
        })?;
        Ok((thread, target))
    }

    /// Answer each `pr.watch` whose fresh read is in. A PR that already
    /// merged or closed has nothing to watch; a read that failed leaves
    /// the first sweep to find whether checks were running, unless the
    /// Server cannot use GitHub at all.
    fn hear_watch_starts(&mut self) {
        for start in std::mem::take(&mut self.watcher.starts) {
            let read = match start.answer.try_recv() {
                Ok(read) => read,
                Err(mpsc::TryRecvError::Empty) => {
                    self.watcher.starts.push(start);
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err(ReadError::Failed("the read ended".into()))
                }
            };
            let checks_pending = match read {
                Ok(reading) if reading.state != PrState::Open => {
                    let state = match reading.state {
                        PrState::Merged => "merged",
                        _ => "closed",
                    };
                    let mut ended = answer(&start.target, false, false);
                    ended["state"] = json!(state);
                    ended["next"] = json!(format!(
                        "This PR is already {state}, so there is nothing to watch. Report that and end your turn."
                    ));
                    self.answer_start(start.asker, Ok(ended));
                    continue;
                }
                // A pending rollup has no failure yet, so any failure the
                // first read finds is news.
                Ok(reading) => Some(
                    !reading.checks.iter().any(|check| {
                        matches!(check.status, CheckStatus::Failure | CheckStatus::Cancelled)
                    }) && reading
                        .checks
                        .iter()
                        .any(|check| check.status == CheckStatus::Pending),
                ),
                Err(ReadError::Unusable { setup }) => {
                    let refused = ApiError::new(Code::Failed, setup);
                    self.answer_start(start.asker, Err(refused));
                    continue;
                }
                Err(_) => None,
            };
            let was = self.engine.pr_watch(start.thread).is_some_and(|fact| {
                fact.on(&start.target.repository, start.target.number)
                    .is_some()
            });
            let answer = answer(&start.target, true, was);
            let result = self
                .watch_pr(start.thread, start.target, start.until, checks_pending)
                .map(|_| answer);
            self.answer_start(start.asker, result);
        }
    }

    /// Save a watch on `target`; a new one is read at once, which is its
    /// baseline.
    fn watch_pr(
        &mut self,
        thread: ThreadId,
        target: Target,
        until: Option<Until>,
        checks_pending: Option<bool>,
    ) -> Result<(), ApiError> {
        let mut fact = self
            .engine
            .pr_watch(thread)
            .ok_or_else(unknown_thread)?
            .clone();
        let key = (target.repository.to_ascii_lowercase(), target.number);
        let was = fact.watch(target, until, checks_pending, (self.clock)());
        self.engine
            .set_pr_watch(thread, fact)
            .map_err(|error| ApiError::new(Code::Failed, error.to_string()))?;
        if !was {
            self.watcher.asked.push_back(Ask {
                only: Some(BTreeSet::from([key])),
                fresh: false,
                release: Vec::new(),
            });
        }
        Ok(())
    }

    /// Notice each open Thread's merge and close commands as its
    /// transcript shows them. One that ends has its Thread's watches read
    /// at once, so a merge the agent made ends its watch with no wake.
    fn scan_commands(&mut self) {
        let open = self.engine.threads();
        self.watcher
            .commands
            .retain(|thread, _| open.contains(thread));
        let mut ended = Vec::new();
        for thread in open {
            let Some(view) = self.engine.thread(thread) else {
                continue;
            };
            let transcript = view.transcript();
            let commands = self.watcher.commands.entry(thread).or_default();
            let revision = transcript.revision();
            if commands.revision == Some(revision) {
                continue;
            }
            let first = commands.revision.is_none();
            let blocks = transcript.blocks();
            let from = match blocks.len() < commands.scanned {
                // Rebuilt: every block again, the ended ones known by call.
                true => 0,
                false => commands
                    .running
                    .values()
                    .map(|(at, _)| *at)
                    .fold(commands.scanned, usize::min),
            };
            for (at, block) in blocks.iter().enumerate().skip(from) {
                let Body::Tool(tool) = &block.body else {
                    continue;
                };
                if tools::policy(&tool.name).kind != tools::ToolKind::Command
                    || commands.ended.contains(&tool.call)
                {
                    continue;
                }
                let Some(closes) = pr_watch::closed_by(&tool.summary) else {
                    continue;
                };
                if tool.state == ToolState::Running {
                    commands.running.insert(tool.call.clone(), (at, closes));
                    continue;
                }
                commands.running.remove(&tool.call);
                commands.ended.insert(tool.call.clone());
                if first {
                    continue;
                }
                let call = (tool.state == ToolState::Ok).then(|| {
                    commands.succeeded.insert(tool.call.clone(), closes);
                    tool.call.clone()
                });
                ended.push((thread, call));
            }
            commands.scanned = blocks.len();
            commands.revision = Some(revision);
        }
        for (thread, call) in ended {
            self.read_thread_watches(thread, call);
        }
    }

    /// Read every PR `thread` watches in full, then release `call`.
    fn read_thread_watches(&mut self, thread: ThreadId, call: Option<String>) {
        let keys: BTreeSet<Key> = self
            .engine
            .pr_watch(thread)
            .map(|fact| fact.on.iter().map(key_of).collect())
            .unwrap_or_default();
        let release: Vec<_> = call.into_iter().map(|call| (thread, call)).collect();
        if keys.is_empty() {
            self.release(&release);
            return;
        }
        self.watcher.asked.push_back(Ask {
            only: Some(keys),
            fresh: true,
            release,
        });
    }

    fn release(&mut self, commands: &[(ThreadId, String)]) {
        for (thread, call) in commands {
            if let Some(commands) = self.watcher.commands.get_mut(thread) {
                commands.succeeded.remove(call);
            }
        }
    }

    fn start_pass(&mut self, ask: Ask) {
        let mut groups: BTreeMap<Key, Vec<Watched>> = BTreeMap::new();
        for (thread, fact) in self.engine.pr_watches() {
            for watch in &fact.on {
                let key = key_of(watch);
                if ask.only.as_ref().is_some_and(|only| !only.contains(&key)) {
                    continue;
                }
                groups.entry(key).or_default().push(Watched {
                    thread,
                    watch: watch.clone(),
                });
            }
        }
        let watcher = &mut self.watcher;
        let sweep = ask.only.is_none();
        if sweep {
            watcher.last_reads.retain(|key, _| groups.contains_key(key));
            watcher.failures.retain(|key, _| groups.contains_key(key));
        }
        if ask.fresh {
            for key in groups.keys() {
                watcher.last_reads.remove(key);
            }
        }
        watcher.pass = Some(Pass {
            sweep,
            release: ask.release,
            waiting: groups
                .into_iter()
                .map(|(key, watches)| Group { key, watches })
                .collect(),
            reading: Vec::new(),
            held: Vec::new(),
        });
    }

    /// Start the reads the pass has room for, weigh the ones that are in,
    /// and end the pass once every one is.
    fn step_pass(&mut self) {
        let Some(pass) = self.watcher.pass.as_mut() else {
            return;
        };
        while pass.reading.len() < GROUP_READS {
            let Some(group) = pass.waiting.pop_front() else {
                break;
            };
            let (tell, answer) = mpsc::channel();
            let github = Arc::clone(&self.github);
            let clock = Arc::clone(&self.clock);
            let last = self.watcher.last_reads.get(&group.key).cloned();
            let read = group.clone();
            std::thread::spawn(move || {
                ferrite_core::spawn::serve_the_operator();
                let _ = tell.send(read_group(&*github, &read, last.as_ref(), &*clock));
            });
            pass.reading.push((group, answer));
        }
        let mut done = Vec::new();
        pass.reading.retain(|(group, answer)| match answer.try_recv() {
            Ok(read) => {
                done.push((group.clone(), read));
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => {
                let ended = ReadError::Failed("the read ended".into());
                done.push((group.clone(), Read::Failed(ended)));
                false
            }
        });
        for (group, read) in done {
            self.weigh(group, read);
        }
        let pass = self.watcher.pass.as_ref().expect("stepping");
        if pass.waiting.is_empty() && pass.reading.is_empty() {
            self.end_pass();
        }
    }

    /// What a pass held goes out, one wake per Thread.
    fn end_pass(&mut self) {
        let pass = self.watcher.pass.take().expect("ending");
        let mut by_thread: BTreeMap<ThreadId, Vec<Held>> = BTreeMap::new();
        for held in pass.held {
            by_thread.entry(held.watched.thread).or_default().push(held);
        }
        for (thread, held) in by_thread {
            self.send_news(thread, held);
        }
        self.release(&pass.release);
        if pass.sweep {
            self.watcher.sweep_due_ms = (self.clock)() + SWEEP_MS;
        }
    }

    /// Weigh one PR's read against each of its watches.
    fn weigh(&mut self, group: Group, read: Read) {
        let key = group.key;
        let (reading, activity, fingerprint, now_ms) = match read {
            Read::Paused | Read::Quiet => return,
            Read::Failed(error) => return self.read_failed(key, group.watches, error),
            Read::Done {
                reading,
                activity,
                fingerprint,
                now_ms,
            } => (reading, activity, fingerprint, now_ms),
        };
        self.watcher.failures.remove(&key);
        if reading.state != PrState::Open {
            self.watcher.last_reads.remove(&key);
            for watched in group.watches {
                self.finished(&key, watched, &reading, now_ms);
            }
            return;
        }
        let last = self.watcher.last_reads.get(&key);
        let seen = LastRead {
            at_ms: match (activity, last) {
                (false, Some(last)) => last.at_ms,
                _ => now_ms,
            },
            in_flight: reading.mergeable == Mergeable::Unknown
                || reading
                    .checks
                    .iter()
                    .any(|check| check.status == CheckStatus::Pending),
            remarks_complete: match activity {
                true => reading.remarks.is_some(),
                false => last.is_some_and(|last| last.remarks_complete),
            },
            watches: group.watches.iter().map(Watched::id).collect(),
            status: fingerprint.as_ref().map(|print| print.status.clone()),
            remarks: fingerprint.map(|print| print.remarks),
        };
        self.watcher.last_reads.insert(key.clone(), seen);
        for watched in group.watches {
            let report = pr_watch::evaluate(&watched.watch.cursor, &reading, now_ms);
            if report.changes.is_empty() {
                if report.next != watched.watch.cursor {
                    self.save(watched.thread, &watched.watch, Commit::Advance(report.next));
                }
                continue;
            }
            let Some(wake) = pr_watch::wake_text(&watched.watch, &report) else {
                continue;
            };
            let remarks = report
                .changes
                .iter()
                .any(|change| matches!(change, Change::Remarks(_)));
            // An ending watch keeps what this pass saw, so watching again
            // starts from it.
            let commit = match report.end {
                Some(End::CommentLimit) => Commit::End(Some(report.next)),
                _ => Commit::Advance(report.next),
            };
            self.hold(Held {
                key: key.clone(),
                watched,
                wake,
                commit,
                remarks,
            });
        }
    }

    /// A read that failed: a rate limit waits, a lost connection is tried
    /// again, and anything else counts, giving up on the PR's watches at
    /// [`pr_watch::READ_FAILURE_LIMIT`] in a row.
    fn read_failed(&mut self, key: Key, watches: Vec<Watched>, error: ReadError) {
        self.watcher.last_reads.remove(&key);
        let problem = match error {
            ReadError::RateLimited => return,
            ReadError::Failed(why) => {
                ferrite_core::log!("pr watch: {}#{}: read failed: {why}", key.0, key.1);
                self.watcher.failures.remove(&key);
                return;
            }
            ReadError::Refused(why) => {
                ferrite_core::log!("pr watch: {}#{}: read refused: {why}", key.0, key.1);
                None
            }
            ReadError::Unusable { setup } => Some(setup),
        };
        let failures = self.watcher.failures.entry(key.clone()).or_default();
        *failures += 1;
        if *failures < pr_watch::READ_FAILURE_LIMIT {
            return;
        }
        let mut gave_up = true;
        for watched in watches {
            let report = Report {
                changes: Vec::new(),
                next: watched.watch.cursor.clone(),
                end: Some(End::Unreadable {
                    problem: problem.clone(),
                }),
            };
            let Some(wake) = pr_watch::wake_text(&watched.watch, &report) else {
                continue;
            };
            let thread = watched.thread;
            gave_up &= self.send_news(
                thread,
                vec![Held {
                    key: key.clone(),
                    watched,
                    wake,
                    commit: Commit::End(None),
                    remarks: false,
                }],
            );
        }
        if gave_up {
            self.watcher.failures.remove(&key);
        }
    }

    /// A merged or closed PR ends its watch with one wake, or none when the
    /// agent's own command merged or closed it. A later end, as a `--auto`
    /// merge's, is news.
    fn finished(&mut self, key: &Key, watched: Watched, reading: &Reading, now_ms: i64) {
        let by_agent = self
            .watcher
            .commands
            .get(&watched.thread)
            .is_some_and(|commands| commands.closes(watched.watch.number));
        if by_agent {
            self.save(watched.thread, &watched.watch, Commit::End(None));
            return;
        }
        let report = pr_watch::evaluate(&watched.watch.cursor, reading, now_ms);
        let Some(wake) = pr_watch::wake_text(&watched.watch, &report) else {
            return;
        };
        self.hold(Held {
            key: key.clone(),
            watched,
            wake,
            commit: Commit::End(None),
            remarks: false,
        });
    }

    fn hold(&mut self, held: Held) {
        if let Some(pass) = self.watcher.pass.as_mut() {
            pass.held.push(held);
        }
    }

    /// Save what moved on one watch, with no wake.
    fn save(&mut self, thread: ThreadId, watch: &Watch, commit: Commit) {
        let Some(mut fact) = self.engine.pr_watch(thread).cloned() else {
            return;
        };
        if commit_to(&mut fact, watch, commit) {
            if let Err(error) = self.engine.set_pr_watch(thread, fact) {
                ferrite_core::log!("thread {thread}: a PR watch was not saved: {error}");
            }
        }
    }

    /// Save a Thread's news, then hold its one wake. With a Decision
    /// pending or a wake already waiting, nothing is saved, and the PRs
    /// are read in full next time. Answers whether the news went.
    fn send_news(&mut self, thread: ThreadId, held: Vec<Held>) -> bool {
        if self.engine.decision_pending(thread) || self.wakes.waiting(thread) {
            for held in &held {
                self.watcher.last_reads.remove(&held.key);
            }
            return false;
        }
        let Some(mut fact) = self.engine.pr_watch(thread).cloned() else {
            return true;
        };
        let mut sections = Vec::new();
        let mut keys = Vec::new();
        for Held {
            key,
            watched,
            wake,
            commit,
            remarks,
        } in held
        {
            let watch = &watched.watch;
            let stamp = Stamp {
                repository: watch.repository.clone(),
                number: watch.number,
                started_ms: watch.cursor.started_ms,
                head: match &commit {
                    Commit::Advance(next) if !remarks => next.head.clone(),
                    _ => None,
                },
                ended: matches!(commit, Commit::End(_)),
            };
            // A watch that ended or moved during the pass decides alone.
            if commit_to(&mut fact, watch, commit) {
                sections.push(Section { stamp, wake });
            } else {
                self.watcher.last_reads.remove(&key);
            }
            keys.push(key);
        }
        if sections.is_empty() {
            return true;
        }
        if let Err(error) = self.engine.set_pr_watch(thread, fact) {
            ferrite_core::log!("thread {thread}: PR watch news was not saved: {error}");
            for key in keys {
                self.watcher.last_reads.remove(&key);
            }
            return false;
        }
        self.wakes.hold(thread, sections);
        true
    }
}

/// Apply `commit` to the watch it is about, while that watch is on.
fn commit_to(fact: &mut pull_request::WatchFact, watch: &Watch, commit: Commit) -> bool {
    let (repository, number, started) = (&watch.repository, watch.number, watch.cursor.started_ms);
    match commit {
        Commit::Advance(next) => fact.advance(repository, number, started, next),
        Commit::End(last) => fact.end(repository, number, Some(started), last),
    }
}

/// A watch call's answer (T3's `setWatching` result).
fn answer(target: &Target, watching: bool, was_watching: bool) -> Value {
    json!({
        "host": "github.com",
        "repository": target.repository,
        "number": target.number,
        "url": target.url,
        "watching": watching,
        "was_watching": was_watching,
    })
}

/// Read one PR as its plan says, off the pump: the cheap fingerprint
/// first, and the full read only when it says something may have changed.
fn read_group(
    github: &dyn GitHub,
    group: &Group,
    last: Option<&LastRead>,
    clock: &(dyn Fn() -> i64 + Send + Sync),
) -> Read {
    let first = &group.watches[0].watch;
    let (repository, number) = (first.repository.as_str(), first.number);
    let fingerprint = match github.pr_fingerprint(repository, number) {
        Ok(fingerprint) => Some(fingerprint),
        Err(ReadError::RateLimited) => return Read::Paused,
        Err(_) => None,
    };
    let now_ms = clock();
    let plan = plan(group, last, fingerprint.as_ref(), now_ms);
    if !plan.detail {
        return Read::Quiet;
    }
    match github.pr_reading(repository, number, plan.activity) {
        Ok(reading) => Read::Done {
            reading,
            activity: plan.activity,
            fingerprint,
            now_ms,
        },
        Err(error) => Read::Failed(error),
    }
}

/// Whether a PR needs its full read, and its remarks too (T3's
/// `planRead`): always for a watch the last read did not cover.
fn plan(group: &Group, last: Option<&LastRead>, fingerprint: Option<&Fingerprint>, now: i64) -> Plan {
    let full = Plan {
        detail: true,
        activity: true,
    };
    let Some(last) = last else {
        return full;
    };
    if group
        .watches
        .iter()
        .any(|watched| !last.watches.contains(&watched.id()))
    {
        return full;
    }
    // Nothing on GitHub changes when the no-checks deadline passes, so it
    // alone asks for a read.
    let checks_due = group.watches.iter().any(|watched| {
        watched
            .watch
            .cursor
            .checks_awaited_since_ms
            .is_some_and(|since| now - since >= pr_watch::NO_CHECKS_MS)
    });
    let Some(fingerprint) = fingerprint else {
        let read = last.in_flight || !last.remarks_complete || now - last.at_ms >= QUIET_REREAD_MS;
        return Plan {
            detail: read || checks_due,
            activity: read,
        };
    };
    let activity = last.remarks.as_deref() != Some(fingerprint.remarks.as_str())
        || !last.remarks_complete
        || now - last.at_ms >= FINGERPRINT_REREAD_MS;
    Plan {
        detail: activity
            || checks_due
            || last.status.as_deref() != Some(fingerprint.status.as_str())
            || last.in_flight,
        activity,
    }
}
