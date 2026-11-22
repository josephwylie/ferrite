//! Delegates on this Server (ADR 0047): T3 Code's `RemoteDelegation`. A
//! Thread here starts a Thread on another saved machine to do one task
//! (`delegate.start`); this Server follows it there until it ends, then
//! wakes the starting Thread's agent with its result through the Server
//! wake (ADR 0046). A cancel, or its parent stopping or parking, ends it
//! here at once, and a stop there is owed until one gets there.
//!
//! The record on the parent Thread's log is the truth (`ferrite_core::
//! delegate`). Each pump pass hears what the workers heard, saves it, then
//! sets the workers by the record: one follower thread per open Delegate,
//! one stopper per owed stop, and a wake for those ended untold. So a
//! restarted Server picks every one up again from the logs alone.
//!
//! What a start runs there is worked out first (`start`), and what an
//! agent asks of the linked machines themselves is asked off the pump
//! (`reach`).

mod follow;
mod reach;
mod start;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use ferrite_core::api::schema::{
    AgentWaitParams, DelegateCancelParams, DelegateListParams, DelegateParams,
    DelegateStartParams, ThreadParams, ThreadStartParams,
};
use ferrite_core::settings::Settings;
use ferrite_core::api::{ApiError, CallerId, Code, Method, Outcome};
use ferrite_core::delegate::{self, Cancel, Delegate, DelegateId, Delegates, Started};
use ferrite_core::runtime_mode::RuntimeMode;
use ferrite_core::wake::Wake;
use ferrite_core::{Attention, DelegatedFrom, ThreadId};

use self::follow::{Ask, Follow, Heard, Then};
use self::start::{Place, Runs};
use super::peers::{self, Peer, PeerFailure, Peers, WAIT_PIECE};
use super::Server;

/// A Delegate, by its parent here and its id there.
type Key = (ThreadId, DelegateId);

/// Why nothing acts on another machine from here, in T3 Code's words.
const OFF_HERE: &str = "Linked machines is off on this machine, so no tool can act on another \
                        machine. The user can turn it on in Settings → Labs.";

/// The Server's Delegates at work: their followers and stoppers, and the
/// starts still out.
pub(super) struct Delegation {
    peers: Arc<Peers>,
    /// The first retry's wait after a call there failed (T3's 2 s).
    first_retry: Duration,
    followers: HashMap<Key, Arc<AtomicBool>>,
    stoppers: HashMap<Key, Arc<AtomicBool>>,
    tell: Sender<Report>,
    heard: Receiver<Report>,
    launches: Vec<Launch>,
    /// What callers asked of the linked machines, answered off the pump.
    asked: Vec<reach::Asked>,
    /// Stops owed for parked Threads' Delegates, as their logs say.
    parked: BTreeMap<Key, Delegate>,
    /// The open Threads that had Delegates on the last pass.
    open: BTreeSet<ThreadId>,
    /// The parked logs were read for stops they owe, once at boot.
    scanned: bool,
}

/// What a worker heard, for the pump to save.
enum Report {
    Seen {
        key: Key,
        status: delegate::Status,
        problem: Option<String>,
    },
    Ended {
        key: Key,
        status: delegate::Status,
        result: String,
    },
    /// A stop got there, or nothing is left there to stop.
    Stopped(Key),
    Unstopped { key: Key, problem: String },
    /// The parked logs' owed stops (the boot scan).
    Parked(Vec<(ThreadId, Delegate)>),
}

/// A `delegate.start` waiting on its Thread there.
struct Launch {
    parent: ThreadId,
    request: String,
    title: Option<String>,
    task: String,
    runs: Runs,
    callers: Vec<(CallerId, String)>,
    answer: Receiver<Result<(Peer, ThreadId), ApiError>>,
}

impl Delegation {
    pub fn new(peers: Peers) -> Self {
        Self::with(peers, Duration::from_secs(2))
    }

    /// The same, retrying after `first_retry` at first.
    pub fn with(peers: Peers, first_retry: Duration) -> Self {
        let (tell, heard) = mpsc::channel();
        Self {
            peers: Arc::new(peers),
            first_retry,
            followers: HashMap::new(),
            stoppers: HashMap::new(),
            tell,
            heard,
            launches: Vec::new(),
            asked: Vec::new(),
            parked: BTreeMap::new(),
            open: BTreeSet::new(),
            scanned: false,
        }
    }
}

impl Drop for Delegation {
    fn drop(&mut self) {
        for flag in self.followers.values().chain(self.stoppers.values()) {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

impl Server {
    /// One pump pass of the Delegates: save what the workers heard, answer
    /// the starts whose Thread there is up, and set the workers and wakes
    /// by the records.
    pub(super) fn serve_delegates(&mut self) {
        self.scan_parked();
        while let Ok(report) = self.delegation.heard.try_recv() {
            self.hear_report(report);
        }
        self.hear_launches();
        self.hear_asked();
        self.reconcile_delegates();
    }

    /// Settings › Labs › Linked machines is on here, and `machine`, if
    /// named, is a machine saved here and turned on: in T3 Code's words
    /// when not.
    pub(super) fn linked(&self, machine: Option<&str>) -> Result<(), ApiError> {
        let settings = Settings::load(&self.settings_dir)
            .map_err(|error| ApiError::new(Code::Failed, error.to_string()))?;
        if !settings.labs.linked_machines {
            return Err(ApiError::new(Code::LinkedMachinesOff, OFF_HERE));
        }
        let Some(name) = machine else { return Ok(()) };
        match peers::saved_machine(&settings.machines, name) {
            None => Err(ApiError::new(
                Code::InvalidRequest,
                format!("This machine is not linked to {name}. `machine.list` lists the linked ones."),
            )),
            Some(saved) if !saved.enabled => Err(ApiError::new(
                Code::InvalidRequest,
                format!("{name} is turned off here. The user turns it on in Settings → Connections."),
            )),
            Some(_) => Ok(()),
        }
    }

    /// `delegate.start`: start a Thread on a saved machine for an open
    /// Thread here, as T3's `delegate_task` with a target does, and answer
    /// once it is up there (`None`: the answer comes then). A repeated
    /// `request` answers with the Delegate the first one started.
    pub(super) fn api_delegate_start(
        &mut self,
        caller: CallerId,
        id: &str,
        from: Option<ThreadId>,
        params: DelegateStartParams,
    ) -> Option<Result<Value, ApiError>> {
        let parent = match delegating(params.thread, from).and_then(|parent| self.open_thread(parent)) {
            Ok(parent) => parent,
            Err(error) => return Some(Err(error)),
        };
        if let Err(refused) = self.linked(Some(&params.machine)) {
            return Some(Err(refused));
        }
        let key = params
            .request
            .clone()
            .unwrap_or_else(|| ferrite_core::machine::remote::random_secret(16));
        let request = format!("delegate:{}:{parent}:{key}", self.id.as_str());
        let recorded = self.engine.all_delegates().find(|(thread, _)| *thread == parent);
        if let Some(existing) = recorded.and_then(|(_, delegates)| delegates.by_request(&request)) {
            return Some(Ok(started(parent, existing)));
        }
        if let Some(launch) = self
            .delegation
            .launches
            .iter_mut()
            .find(|launch| launch.request == request)
        {
            self.api.owe(caller, id);
            launch.callers.push((caller, id.to_string()));
            return None;
        }
        let (inherited, reach) = self.inheritance(parent);
        let view = self.engine.thread(parent).expect("open");
        let repository = view
            .workspace()
            .and_then(|workspace| ferrite_core::workspace::repository_key(workspace.cwd()));
        let machine = params.machine.clone();
        let chosen = start::mode(params.mode, reach).and_then(|mode| {
            let runs = start::runs(&inherited, params.provider, params.model, params.effort)?;
            let place = Place::of(params.checkout, params.project, repository)?;
            Ok((mode, runs, place))
        });
        let (mode, runs, place) = match chosen {
            Ok(chosen) => chosen,
            Err(refused) => return Some(Err(refused)),
        };
        let delegated_from = DelegatedFrom {
            server: self.id.clone(),
            label: this_computer(),
            thread: parent,
            title: view
                .title()
                .map_or_else(|| format!("Thread {parent}"), str::to_string),
        };
        // Its `cwd` and `project` are where `place` puts it there.
        let start = ThreadStartParams {
            cwd: Default::default(),
            provider: Some(runs.provider),
            worktree: false,
            title: params.title.clone(),
            focus: false,
            args: runs.args(),
            branch: None,
            base: None,
            project: None,
            ready: false,
            timeout_ms: None,
            resume: None,
            env: Default::default(),
            launcher: Vec::new(),
            owner: None,
            request: Some(request.clone()),
            prompt: Some(params.task.clone()),
            mode: Some(mode),
            delegated_from: Some(delegated_from),
        };
        let (tell, answer) = mpsc::channel();
        let peers = Arc::clone(&self.delegation.peers);
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
            let _ = tell.send(start::launch(&peers, &machine, place, start));
        });
        self.api.owe(caller, id);
        self.delegation.launches.push(Launch {
            parent,
            request,
            title: params.title,
            task: params.task,
            runs,
            callers: vec![(caller, id.to_string())],
            answer,
        });
        None
    }

    /// `delegate.defaults`: what a Delegate `thread` starts inherits, as
    /// T3 Code's `orchestrator_capabilities` says it.
    pub(super) fn api_delegate_defaults(&self, params: &ThreadParams) -> Result<Value, ApiError> {
        let parent = self.open_thread(params.thread)?;
        let (runs, reach) = self.inheritance(parent);
        Ok(json!({
            "thread": parent,
            "provider": runs.provider,
            "model": runs.model,
            "effort": runs.effort,
            "mode": reach,
        }))
    }

    /// What the open Thread `parent` hands a Delegate that names nothing:
    /// the provider, model and effort it runs on, and its own reach, which
    /// caps the Delegate's (G-A6): what its provider announced, else this
    /// machine's default, never past its own cap.
    fn inheritance(&self, parent: ThreadId) -> (Runs, RuntimeMode) {
        let view = self.engine.thread(parent).expect("open");
        let defaults = self.defaults.lock().map(|defaults| defaults.clone()).unwrap_or_default();
        let reach = RuntimeMode::of_thread(
            view.provider(),
            view.permission_mode(),
            view.sandbox(),
            &defaults,
            self.engine.cap(parent).map(|(_, cap)| cap),
        );
        let runs = Runs {
            provider: view.provider(),
            model: view.running_model().map(str::to_string),
            effort: view.effort().map(str::to_string),
        };
        (runs, reach)
    }

    /// `delegate.get`: one Delegate. Its parent's own agent reading an
    /// ended one has read its result, so no wake carries it again (T3's
    /// `task_status`).
    pub(super) fn api_delegate_get(
        &mut self,
        from: Option<ThreadId>,
        params: &DelegateParams,
    ) -> Result<Value, ApiError> {
        let parent = delegating(params.thread, from)?;
        let delegates = self.delegates_of(parent)?;
        let delegate = delegates
            .get(params.delegate)
            .ok_or_else(|| unknown_delegate(params.delegate, parent))?;
        let answer = json!({ "delegate": delegate.info(parent) });
        if from == Some(parent) {
            self.amend_delegates(parent, |delegates| delegates.tell(&[params.delegate]));
        }
        Ok(answer)
    }

    /// `delegate.list`: a Thread's Delegates, or every open Thread's.
    pub(super) fn api_delegate_list(
        &self,
        from: Option<ThreadId>,
        params: &DelegateListParams,
    ) -> Result<Value, ApiError> {
        let infos: Vec<Value> = match params.thread.or(from) {
            Some(parent) => self
                .delegates_of(parent)?
                .iter()
                .map(|delegate| delegate.info(parent))
                .collect(),
            None => self
                .engine
                .all_delegates()
                .flat_map(|(parent, delegates)| delegates.iter().map(move |delegate| delegate.info(parent)))
                .collect(),
        };
        Ok(json!({ "delegates": infos }))
    }

    /// `delegate.cancel`: T3's `task_cancel` of a remote task. An open one
    /// ends cancelled here at once (`cancel_requested`); an ended one keeps
    /// its result and status. Either way a stop is owed there, which the
    /// Server carries.
    pub(super) fn api_delegate_cancel(
        &mut self,
        from: Option<ThreadId>,
        params: &DelegateCancelParams,
    ) -> Result<Value, ApiError> {
        let parent = delegating(params.delegate.thread, from)?;
        self.delegates_of(parent)?;
        let id = params.delegate.delegate;
        let mut done = None;
        self.engine
            .amend_delegates(parent, |delegates| {
                done = delegates.cancel(id, params.reason.as_deref());
                done.is_some()
            })
            .map_err(|error| ApiError::new(Code::Failed, error.to_string()))?;
        let status = match done.ok_or_else(|| unknown_delegate(id, parent))? {
            Cancel::Requested => "cancel_requested",
            Cancel::Ended(status) => status.word(),
        };
        self.reconcile_delegates();
        Ok(json!({ "delegate": id, "status": status }))
    }

    /// The wake owed for `thread`'s Delegates that ended untold, with the
    /// ids it tells.
    pub(super) fn delegates_wake(&self, thread: ThreadId) -> Option<(Wake, Vec<DelegateId>)> {
        let (_, delegates) = self
            .engine
            .all_delegates()
            .find(|(parent, _)| *parent == thread)?;
        let untold = delegates.untold();
        Some((delegates.wake(&untold)?, untold))
    }

    /// The wake for `told` reached the agent, or never can.
    pub(super) fn told_delegates(&mut self, thread: ThreadId, told: &[DelegateId]) {
        self.amend_delegates(thread, |delegates| delegates.tell(told));
    }

    fn delegates_of(&self, parent: ThreadId) -> Result<Delegates, ApiError> {
        self.is_open(parent)?;
        self.engine
            .delegates(parent)
            .map_err(|error| ApiError::new(Code::Failed, error.to_string()))
    }

    /// Change `parent`'s Delegates, saying a failure in the log: what a
    /// worker heard is heard again after a restart.
    fn amend_delegates(&mut self, parent: ThreadId, change: impl FnOnce(&mut Delegates) -> bool) {
        if let Err(error) = self.engine.amend_delegates(parent, change) {
            ferrite_core::log!("thread {parent}: its Delegates could not be saved: {error}");
        }
    }

    fn hear_report(&mut self, report: Report) {
        match report {
            Report::Seen {
                key: (parent, id),
                status,
                problem,
            } => self.amend_delegates(parent, |delegates| delegates.seen(id, status, problem)),
            Report::Ended {
                key: (parent, id),
                status,
                result,
            } => self.amend_delegates(parent, |delegates| delegates.end(id, status, result)),
            Report::Stopped(key) => {
                self.delegation.stoppers.remove(&key);
                self.delegation.parked.remove(&key);
                self.amend_delegates(key.0, |delegates| delegates.stopped(key.1));
            }
            Report::Unstopped {
                key: (parent, id),
                problem,
            } => self.amend_delegates(parent, |delegates| delegates.unstopped(id, problem)),
            Report::Parked(owed) => {
                for (parent, delegate) in owed {
                    self.delegation.parked.insert((parent, delegate.id), delegate);
                }
            }
        }
    }

    /// Answer each `delegate.start` whose Thread there is up, once it is
    /// recorded. A Thread there that could not be recorded here is
    /// stopped there.
    fn hear_launches(&mut self) {
        for launch in std::mem::take(&mut self.delegation.launches) {
            let launched = match launch.answer.try_recv() {
                Ok(launched) => launched,
                Err(mpsc::TryRecvError::Empty) => {
                    self.delegation.launches.push(launch);
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err(ApiError::new(Code::Failed, "the start ended with no answer"))
                }
            };
            let answer = launched.and_then(|(peer, thread)| self.record_launch(&launch, peer, thread));
            for (caller, id) in launch.callers {
                self.api.reply(caller, id, answer.clone());
            }
        }
    }

    fn record_launch(&mut self, launch: &Launch, peer: Peer, thread: ThreadId) -> Result<Value, ApiError> {
        let parent = launch.parent;
        // A parent parked while it started stops it at once.
        let open = self.engine.thread(parent).is_some();
        let recorded = self.engine.amend_delegates(parent, |delegates| {
            if delegates.by_request(&launch.request).is_some() {
                return false;
            }
            delegates.start(Started {
                server: peer.server.clone(),
                machine: peer.label.clone(),
                thread,
                request: launch.request.clone(),
                title: launch.title.clone(),
                task: launch.task.clone(),
                provider: launch.runs.provider,
                model: launch.runs.model.clone(),
            });
            if !open {
                delegates.stop_with_parent();
            }
            true
        });
        let delegates = recorded.and_then(|_| self.engine.delegates(parent));
        let delegate = match delegates {
            Ok(delegates) => delegates.by_request(&launch.request).cloned(),
            Err(error) => {
                ferrite_core::log!("thread {parent}: a Delegate could not be recorded: {error}");
                None
            }
        };
        let Some(delegate) = delegate else {
            let peers = Arc::clone(&self.delegation.peers);
            let server = peer.server.clone();
            std::thread::spawn(move || {
                ferrite_core::spawn::serve_the_operator();
                let _ = peers.call(&server, Method::ThreadStop(ThreadParams { thread }));
            });
            return Err(ApiError::new(
                Code::Failed,
                format!(
                    "the Delegate could not be recorded here, so its Thread {thread} on {} is being stopped",
                    peer.label
                ),
            ));
        };
        if !open {
            self.delegation.parked.insert((parent, delegate.id), delegate.clone());
        }
        self.reconcile_delegates();
        Ok(started(parent, &delegate))
    }

    /// Set the workers by the records: a follower for each open Delegate
    /// and none for any other, a stopper for each owed stop, and a wake for
    /// each Thread with Delegates ended untold.
    fn reconcile_delegates(&mut self) {
        let mut open = BTreeSet::new();
        let mut following = BTreeSet::new();
        let mut follow = Vec::new();
        let mut stop = Vec::new();
        let mut wake = Vec::new();
        for (parent, delegates) in self.engine.all_delegates() {
            open.insert(parent);
            for delegate in delegates.open() {
                following.insert((parent, delegate.id));
                if !self.delegation.followers.contains_key(&(parent, delegate.id)) {
                    follow.push((parent, delegate.clone()));
                }
            }
            stop.extend(delegates.owed_stops().map(|delegate| (parent, delegate.clone())));
            if !delegates.untold().is_empty() {
                wake.push(parent);
            }
        }
        self.delegation.followers.retain(|key, cancelled| {
            let keep = following.contains(key);
            if !keep {
                cancelled.store(true, Ordering::Relaxed);
            }
            keep
        });
        // A Thread parked or deleted since: its log says what it still owes.
        let left: Vec<ThreadId> = self.delegation.open.difference(&open).copied().collect();
        for parent in left {
            for delegate in self.engine.delegates(parent).iter().flat_map(Delegates::owed_stops) {
                self.delegation.parked.insert((parent, delegate.id), delegate.clone());
            }
        }
        self.delegation.open = open;
        stop.extend(
            self.delegation
                .parked
                .iter()
                .map(|((parent, _), delegate)| (*parent, delegate.clone())),
        );
        for (parent, delegate) in follow {
            let cancelled = Arc::new(AtomicBool::new(false));
            self.delegation
                .followers
                .insert((parent, delegate.id), Arc::clone(&cancelled));
            let (peers, tell) = (Arc::clone(&self.delegation.peers), self.delegation.tell.clone());
            let first_retry = self.delegation.first_retry;
            std::thread::spawn(move || {
                ferrite_core::spawn::serve_the_operator();
                follow_one(&peers, parent, &delegate, first_retry, &cancelled, &tell);
            });
        }
        for (parent, delegate) in stop {
            let key = (parent, delegate.id);
            if self.delegation.stoppers.contains_key(&key) {
                continue;
            }
            let cancelled = Arc::new(AtomicBool::new(false));
            self.delegation.stoppers.insert(key, Arc::clone(&cancelled));
            let (peers, tell) = (Arc::clone(&self.delegation.peers), self.delegation.tell.clone());
            let first_retry = self.delegation.first_retry;
            std::thread::spawn(move || {
                ferrite_core::spawn::serve_the_operator();
                stop_one(&peers, key, &delegate, first_retry, &cancelled, &tell);
            });
        }
        for parent in wake {
            if !self.wakes.waiting(parent) {
                self.wakes.hold_delegates(parent);
            }
        }
    }

    /// Read the parked logs once for the stops they still owe, off the
    /// pump: a Server stopped before a stop got there sends it again.
    fn scan_parked(&mut self) {
        if std::mem::replace(&mut self.delegation.scanned, true) {
            return;
        }
        let Ok(parked) = self.engine.parked() else {
            return;
        };
        let store = self.engine.browser_store().clone();
        let tell = self.delegation.tell.clone();
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
            let owed = parked
                .into_iter()
                .filter_map(|parent| Some((parent, store.peek(parent).ok()?.delegates)))
                .flat_map(|(parent, delegates)| {
                    delegates
                        .owed_stops()
                        .map(|delegate| (parent, delegate.clone()))
                        .collect::<Vec<_>>()
                })
                .collect();
            let _ = tell.send(Report::Parked(owed));
        });
    }
}

/// The Thread a call is about: the one it names, else the one it runs in.
fn delegating(thread: Option<ThreadId>, from: Option<ThreadId>) -> Result<ThreadId, ApiError> {
    thread.or(from).ok_or_else(|| {
        ApiError::new(
            Code::InvalidRequest,
            "Pass thread: this call is not running inside a Ferrite thread.",
        )
    })
}

/// T3 Code's `task_not_found`, in its words.
fn unknown_delegate(id: DelegateId, parent: ThreadId) -> ApiError {
    ApiError::new(
        Code::UnknownDelegate,
        format!("Delegated task {id} does not belong to thread {parent}."),
    )
}

/// A start's answer: the Delegate, and what its agent does next.
fn started(parent: ThreadId, delegate: &Delegate) -> Value {
    json!({ "delegate": delegate.info(parent), "next": delegate::NEXT })
}

/// What this computer is called, for the Thread there to say where it came
/// from.
fn this_computer() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "another machine".into())
}

/// Follow one Delegate there until it ends or is cancelled here.
fn follow_one(
    peers: &Peers,
    parent: ThreadId,
    delegate: &Delegate,
    first_retry: Duration,
    cancelled: &AtomicBool,
    tell: &Sender<Report>,
) {
    let key = (parent, delegate.id);
    let mut follow = Follow::new(&delegate.machine, delegate.thread, delegate.status, first_retry);
    let mut ask = follow.first();
    let (mut status, mut problem) = (delegate.status, delegate.problem.clone());
    while !cancelled.load(Ordering::Relaxed) {
        let heard = hear(peers, delegate, ask);
        if cancelled.load(Ordering::Relaxed) {
            return;
        }
        match follow.heard(ask, heard) {
            Then::Ask { status: now, ask: next } => {
                if (now, None) != (status, problem.clone()) {
                    (status, problem) = (now, None);
                    let _ = tell.send(Report::Seen { key, status, problem: None });
                }
                ask = next;
            }
            Then::Retry { problem: now, after } => {
                if problem.as_ref() != Some(&now) {
                    problem = Some(now.clone());
                    let _ = tell.send(Report::Seen {
                        key,
                        status,
                        problem: Some(now),
                    });
                }
                nap(after, cancelled);
            }
            Then::End { status, result } => {
                let _ = tell.send(Report::Ended { key, status, result });
                return;
            }
        }
    }
}

/// One call there, as the follower hears it.
fn hear(peers: &Peers, delegate: &Delegate, ask: Ask) -> Heard {
    let (server, thread) = (&delegate.server, delegate.thread);
    let answer = match ask {
        Ask::Look => peers.call(server, Method::ThreadGet(ThreadParams { thread })),
        Ask::Wait => peers.wait(
            server,
            AgentWaitParams {
                thread,
                until: None,
                turn_ended: true,
                timeout_ms: Some(WAIT_PIECE.as_millis() as u64),
            },
        ),
        Ask::Result => peers.call(server, Method::AgentResult(ThreadParams { thread })),
    };
    let answer = match answer {
        Ok(answer) => answer,
        Err(failure) => return heard_failure(failure, &delegate.machine),
    };
    let unreadable = |what: &str| Heard::Unreadable(format!("{} answered a {what} this Server cannot read", delegate.machine));
    match ask {
        Ask::Look => serde_json::from_value::<Attention>(answer["thread"]["attention"].clone())
            .map_or_else(|_| unreadable("thread.get"), Heard::Looked),
        Ask::Wait => Heard::TurnEnded,
        Ask::Result => {
            let outcome = match answer["outcome"].as_str() {
                None => None,
                Some("completed") => Some(Outcome::Completed),
                Some("failed") => Some(Outcome::Failed),
                Some("interrupted") => Some(Outcome::Interrupted),
                Some(_) => return unreadable("agent.result"),
            };
            Heard::Result {
                outcome,
                reply: answer["text"].as_str().map(str::to_string),
                parked: answer["parked"].as_bool().unwrap_or(false),
            }
        }
    }
}

/// How the follower hears a call that had no answer.
fn heard_failure(failure: PeerFailure, machine: &str) -> Heard {
    match failure {
        PeerFailure::Refused(refusal) => match refusal.code {
            Code::ThreadParked => Heard::Parked,
            Code::Timeout | Code::ThreadClosed => Heard::StillGoing,
            _ => Heard::Unanswered(format!("{machine}: {}", refusal.message)),
        },
        failure @ (PeerFailure::Final(_) | PeerFailure::OtherServer { .. }) => {
            Heard::Final(words(&failure, machine))
        }
        failure => Heard::Unanswered(words(&failure, machine)),
    }
}

/// Stop one Delegate's Thread there, asking again until a stop gets there
/// or nothing is left there to stop.
fn stop_one(
    peers: &Peers,
    key: Key,
    delegate: &Delegate,
    first_retry: Duration,
    cancelled: &AtomicBool,
    tell: &Sender<Report>,
) {
    let mut failures = 0;
    while !cancelled.load(Ordering::Relaxed) {
        let thread = delegate.thread;
        match peers.call(&delegate.server, Method::ThreadStop(ThreadParams { thread })) {
            // Gone there, refused there, or no way left to reach it: no
            // retry changes that (T3 logs a refused stop and goes on).
            Ok(_)
            | Err(PeerFailure::Final(_) | PeerFailure::OtherServer { .. } | PeerFailure::Refused(_)) => {
                let _ = tell.send(Report::Stopped(key));
                return;
            }
            Err(failure) => {
                let _ = tell.send(Report::Unstopped {
                    key,
                    problem: words(&failure, &delegate.machine),
                });
                nap(follow::backoff(first_retry, failures), cancelled);
                failures += 1;
            }
        }
    }
}

/// Sleep `long`, or until `cancelled`.
fn nap(long: Duration, cancelled: &AtomicBool) {
    const SLICE: Duration = Duration::from_millis(100);
    let mut left = long;
    while !left.is_zero() && !cancelled.load(Ordering::Relaxed) {
        let slice = left.min(SLICE);
        std::thread::sleep(slice);
        left -= slice;
    }
}

/// A failed call there, in words.
fn words(failure: &PeerFailure, machine: &str) -> String {
    match failure {
        PeerFailure::Unreachable(said) => format!("{machine} did not answer: {said}"),
        PeerFailure::Silent(at) => format!("{machine} did not answer at {at}."),
        PeerFailure::Expired => format!(
            "The link to {machine} expired. The user links it again in Settings → Connections, \
             with a new pairing code from {machine}."
        ),
        PeerFailure::SignIn => {
            format!("{machine} needs a sign-in: the user signs in to it from a Ferrite window.")
        }
        PeerFailure::NoFerrited => format!("{machine} has no ferrited"),
        PeerFailure::Unsupported(said) | PeerFailure::Final(said) => format!("{machine}: {said}"),
        PeerFailure::OtherServer { expected, answered } => format!(
            "{machine} now answers as the Server {}, not {}, which this Delegate runs on: it was pointed at another machine",
            answered.as_str(),
            expected.as_str()
        ),
        PeerFailure::Refused(ApiError {
            code: Code::LinkedMachinesOff,
            ..
        }) => format!("{machine} has Linked machines turned off. Turn it on in its Settings → Labs."),
        PeerFailure::Refused(refusal) => format!("{machine}: {}", refusal.message),
    }
}

/// A start that failed, as its caller hears it: the machine's own refusal
/// keeps its code.
fn refusal(failure: &PeerFailure, machine: &str) -> ApiError {
    let code = match failure {
        PeerFailure::Refused(refusal) => refusal.code,
        _ => Code::Failed,
    };
    ApiError::new(code, words(failure, machine))
}
