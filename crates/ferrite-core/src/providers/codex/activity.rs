//! Child observation on the existing app-server connection. This decoder owns
//! provider ancestry, bounded attribution, and read/live reconciliation. It
//! never resumes a thread or sends an agent prompt.

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::{json, Value};

use crate::activity::{
    ActivityEvent, AgentInfo, AgentKey, AgentStatus, ExecutionEvent, Subject, TranscriptCoverage,
};
use crate::store::Provider;
use crate::{SessionEvent, TurnOutcome};

use super::wire;

const MAX_CONCURRENT_READS: usize = 128;
const MAX_PENDING_FRAMES: usize = 256;
const MAX_PENDING_BYTES: usize = 2 * 1024 * 1024;
const MAX_ITEM_REVISIONS: usize = 8192;

#[derive(Default)]
pub(super) struct Update {
    pub events: Vec<SessionEvent>,
    pub requests: Vec<Value>,
}

impl Update {
    fn activity(&mut self, event: ActivityEvent) {
        self.events.push(SessionEvent::Activity(event));
    }
}

struct Child {
    info: AgentInfo,
    parent: String,
    reading: bool,
    refresh_after_read: bool,
    lifecycle_revision: u64,
    active_turn: Option<String>,
    read_queued: bool,
}

struct Read {
    child: String,
    revision: u64,
}

#[derive(Default)]
pub(super) struct Router {
    content: wire::Decoder,
    completed_turns: HashSet<(String, String)>,
    completed_order: VecDeque<(String, String)>,
    root: Option<String>,
    children: HashMap<String, Child>,
    pending: VecDeque<(Value, usize)>,
    pending_bytes: usize,
    discarded: bool,
    reads: HashMap<String, Read>,
    queued_reads: VecDeque<String>,
    next_request: u64,
    revision: u64,
    // A bounded conservative guard: if exhausted, history stops replacing
    // live content rather than forgetting a newer item's authority.
    item_revisions: HashMap<(String, String), (u64, bool)>,
    revisions_full: bool,
    requests: HashMap<String, String>,
    unrelated: HashSet<String>,
    conflicts: HashSet<String>,
}

impl Router {
    pub(super) fn identify_main(&mut self, root: &str) -> Update {
        self.root = Some(root.to_owned());
        let mut update = Update::default();
        self.replay_pending(&mut update);
        update
    }

    /// A resumed Main may be the first evidence of previously spawned children.
    /// Inspect its stored collaboration items for discovery only, never replay
    /// Main's history into the already-restored transcript.
    pub(super) fn root_history(&mut self, thread: &Value) -> Update {
        let mut update = Update::default();
        if let (Some(root), Some(turns)) = (self.root.clone(), thread["turns"].as_array()) {
            for turn in turns {
                if let Some(items) = turn["items"].as_array() {
                    for item in items {
                        self.discover_from_item(&root, item, false, &mut update);
                    }
                }
            }
        }
        self.replay_pending(&mut update);
        update
    }

    pub(super) fn observe(&mut self, value: Value) -> Update {
        let mut update = Update::default();
        let known_children = self.children.len();
        self.consume(value, &mut update, true);
        if self.children.len() != known_children {
            self.replay_pending(&mut update);
        }
        self.start_queued_reads(&mut update);
        update
    }

    pub(super) fn request_failed(&mut self, request: &Value) -> Update {
        let mut update = Update::default();
        if let Some(id) = request["id"].as_str() {
            if let Some(read) = self.reads.remove(id) {
                if let Some(child) = self.children.get_mut(&read.child) {
                    child.reading = false;
                    child.info.coverage = TranscriptCoverage::Unavailable;
                    update.activity(ActivityEvent::Coverage {
                        key: child.info.key.clone(),
                        coverage: TranscriptCoverage::Unavailable,
                    });
                }
            }
        }
        update
    }

    fn known(&self, id: &str) -> bool {
        self.root.as_deref() == Some(id) || self.children.contains_key(id)
    }

    fn key(&self, id: &str) -> AgentKey {
        AgentKey::new(
            Provider::Codex,
            self.root.as_deref().unwrap_or_default(),
            id,
        )
    }

    fn parent_subject(&self, id: &str) -> Subject {
        if self.root.as_deref() == Some(id) {
            Subject::Main
        } else {
            Subject::Subagent(self.key(id))
        }
    }

    fn buffer(&mut self, frame: Value) {
        let bytes = frame.to_string().len();
        if bytes > MAX_PENDING_BYTES {
            self.discarded = true;
            return;
        }
        while self.pending.len() >= MAX_PENDING_FRAMES
            || self.pending_bytes + bytes > MAX_PENDING_BYTES
        {
            if let Some((_, bytes)) = self.pending.pop_front() {
                self.pending_bytes -= bytes;
                self.discarded = true;
            } else {
                break;
            }
        }
        self.pending_bytes += bytes;
        self.pending.push_back((frame, bytes));
    }

    fn replay_pending(&mut self, update: &mut Update) {
        loop {
            let before = self.children.len();
            let count = self.pending.len();
            for _ in 0..count {
                let (frame, bytes) = self.pending.pop_front().expect("counted pending frames");
                self.pending_bytes -= bytes;
                let known = frame_scope(&frame).is_some_and(|id| self.known(id));
                let known_parent =
                    metadata_parent(&frame["params"]["thread"]).is_some_and(|id| self.known(id));
                if known || known_parent {
                    self.consume(frame, update, false);
                } else {
                    self.pending_bytes += bytes;
                    self.pending.push_back((frame, bytes));
                }
            }
            if self.children.len() == before {
                break;
            }
        }
    }

    fn consume(&mut self, frame: Value, update: &mut Update, allow_buffer: bool) {
        if frame.get("method").is_none() {
            if frame.get("result").is_none() && frame.get("error").is_none() {
                return;
            }
            if let Some(id) = frame["id"].as_str() {
                if let Some(read) = self.reads.remove(id) {
                    self.read_result(read, &frame, update);
                }
            }
            return;
        }
        let method = frame["method"].as_str().unwrap_or_default();
        if method == "account/rateLimits/updated" {
            if let Some(event) = wire::parse_line(&frame.to_string()) {
                update.events.push(event);
            }
            return;
        }
        if frame_scope_checked(&frame).is_err() {
            return; // Contradictory owner fields cannot route to either subject.
        }
        if method == "thread/started" {
            let thread = &frame["params"]["thread"];
            if let Some(id) = thread["id"].as_str() {
                let supplied_parent = !thread["parentThreadId"].is_null()
                    || !thread["source"]["subAgent"]["thread_spawn"]["parent_thread_id"].is_null();
                if supplied_parent
                    && self
                        .children
                        .get(id)
                        .is_some_and(|child| metadata_parent(thread) != Some(child.parent.as_str()))
                {
                    self.unrelated.insert(id.to_owned());
                    self.conflicts.insert(id.to_owned());
                    update.activity(ActivityEvent::Detached { key: self.key(id) });
                    update.activity(ActivityEvent::Coverage {
                        key: self.key(id),
                        coverage: TranscriptCoverage::Unavailable,
                    });
                    return;
                }
            }
            if let (Some(id), Some(parent)) = (thread["id"].as_str(), metadata_parent(thread)) {
                if self.known(parent) {
                    self.discover(id, parent, thread, update);
                }
            } else if let Some(id) = thread["id"].as_str() {
                if self.root.is_some()
                    && !self.known(id)
                    && self.unrelated.len() < MAX_PENDING_FRAMES
                {
                    self.unrelated.insert(id.to_owned());
                }
            }
        }
        let scope = frame_scope(&frame).map(str::to_owned);
        if scope
            .as_deref()
            .is_some_and(|id| self.unrelated.contains(id))
        {
            return;
        }
        if method == "serverRequest/resolved" {
            let raw = &frame["params"]["requestId"];
            if raw.is_string() || raw.is_number() {
                let id = raw.to_string();
                let matches_owner = self.requests.get(&id).is_some_and(|owner| {
                    owner.is_empty() || scope.as_deref() == Some(owner.as_str())
                });
                if matches_owner {
                    self.requests.remove(&id);
                    // Do not resurrect an approval when ancestry arrives after
                    // its resolution and the buffered frames are replayed.
                    self.pending
                        .retain(|(frame, _)| frame["id"].to_string() != id);
                    self.pending_bytes = self.pending.iter().map(|(_, bytes)| *bytes).sum();
                    update.activity(ActivityEvent::DecisionCancelled { id });
                }
            }
            return;
        }
        let known = scope.as_deref().is_some_and(|id| self.known(id));
        if !known {
            // Missing or not-yet-established attribution must never reach Main.
            // Keep an approval visible, with no guessed owner, while retaining
            // its original typed response handle for later explicit enrichment.
            if allow_buffer {
                if let Some(SessionEvent::DecisionRequested { decision }) =
                    wire::parse_line(&frame.to_string())
                {
                    if self.requests.len() < MAX_PENDING_FRAMES {
                        self.requests
                            .insert(decision.id.clone(), scope.clone().unwrap_or_default());
                        update.activity(ActivityEvent::Decision {
                            subject: None,
                            decision,
                        });
                    }
                }
                if scope.is_some() {
                    self.buffer(frame);
                }
            }
            return;
        }
        let scope = scope.expect("known scope");
        let params = &frame["params"];
        let turn = params["turnId"]
            .as_str()
            .or_else(|| params["turn"]["id"].as_str());
        if let Some(turn) = turn {
            let identity = (scope.clone(), turn.to_owned());
            if self.completed_turns.contains(&identity) && method != "turn/completed" {
                // Ordered native streams can still replay final snapshots after
                // a reconnect. Enrich child history without restarting its clock.
                if method == "item/completed" && self.root.as_deref() != Some(scope.as_str()) {
                    self.item(&scope, turn, &params["item"], true, true, update);
                }
                return;
            }
            if method == "turn/completed"
                && matches!(
                    params["turn"]["status"].as_str(),
                    Some("completed" | "interrupted" | "failed")
                )
                && self.completed_turns.insert(identity.clone())
            {
                self.completed_order.push_back(identity);
                if self.completed_order.len() > MAX_ITEM_REVISIONS {
                    if let Some(old) = self.completed_order.pop_front() {
                        self.completed_turns.remove(&old);
                    }
                }
            }
        }
        if matches!(method, "item/started" | "item/completed") {
            self.discover_from_item(&scope, &params["item"], true, update);
        }
        if self.root.as_deref() == Some(scope.as_str()) {
            for event in self.content.parse(&frame.to_string()) {
                if let SessionEvent::DecisionRequested { decision } = &event {
                    if self.requests.len() < MAX_PENDING_FRAMES
                        || self.requests.contains_key(&decision.id)
                    {
                        self.requests.insert(decision.id.clone(), scope.clone());
                    }
                }
                update.events.push(event);
            }
            return;
        }
        self.child_frame(&scope, &frame, update);
    }

    fn discover_from_item(&mut self, owner: &str, item: &Value, live: bool, update: &mut Update) {
        match item["type"].as_str() {
            Some("subAgentActivity") => {
                if let Some(id) = item["agentThreadId"].as_str() {
                    // This item is emitted on the spawning parent's thread.
                    // The read enriches the explicit relationship; path/name is
                    // display data and never parsed as identity.
                    self.discover(id, owner, &json!({"preview":item["agentPath"]}), update);
                }
            }
            Some("collabAgentToolCall")
                if item["tool"].as_str() == Some("spawnAgent")
                    || item["tool"].as_str() == Some("spawn") =>
            {
                if item["senderThreadId"].as_str() != Some(owner) {
                    return;
                }
                if let Some(receivers) = item["receiverThreadIds"].as_array() {
                    for id in receivers.iter().filter_map(Value::as_str) {
                        self.discover(id, owner, &json!({"preview":item["prompt"]}), update);
                    }
                }
            }
            _ => {}
        }
        if live {
            if item["type"].as_str() == Some("subAgentActivity") {
                if let Some(id) = item["agentThreadId"].as_str() {
                    let state = match item["kind"].as_str() {
                        Some("started") => Some(AgentStatus::Pending),
                        Some("completed") => Some(AgentStatus::Idle),
                        _ => None,
                    };
                    if let Some(state) = state {
                        self.supplement_status(id, state, false, update);
                    }
                }
            }
            if let Some(states) = item["agentsStates"].as_object() {
                for (id, state) in states {
                    let state = match state["status"].as_str() {
                        Some("pendingInit") => AgentStatus::Pending,
                        Some("running") => AgentStatus::Working,
                        Some("completed") => AgentStatus::Idle,
                        Some("interrupted") => AgentStatus::Interrupted,
                        Some("errored") => AgentStatus::Failed,
                        Some("shutdown") => AgentStatus::Shutdown,
                        Some("notFound") => AgentStatus::NotFound,
                        _ => continue,
                    };
                    let closed = item["tool"].as_str() == Some("closeAgent")
                        && matches!(state, AgentStatus::Shutdown | AgentStatus::NotFound);
                    self.supplement_status(id, state, closed, update);
                }
            }
        }
    }

    fn supplement_status(
        &self,
        id: &str,
        state: AgentStatus,
        explicit_close: bool,
        update: &mut Update,
    ) {
        if let Some(child) = self.children.get(id) {
            if !self.unrelated.contains(id) && (child.lifecycle_revision == 0 || explicit_close) {
                update.activity(ActivityEvent::Status {
                    key: child.info.key.clone(),
                    state,
                });
            }
        }
    }

    fn discover(&mut self, id: &str, parent: &str, metadata: &Value, update: &mut Update) {
        if self.root.as_deref() == Some(id) || !self.known(parent) || id == parent {
            return;
        }
        if self.conflicts.contains(id) && metadata_parent(metadata) != Some(parent) {
            return;
        }
        self.conflicts.remove(id);
        if let Some(child) = self.children.get(id) {
            if child.parent != parent {
                return; // Conflicting ancestry never silently re-parents a tab.
            }
        } else {
            let mut info = AgentInfo::new(self.key(id));
            info.parent = Some(self.parent_subject(parent));
            info.coverage = TranscriptCoverage::Partial;
            self.children.insert(
                id.to_owned(),
                Child {
                    info,
                    parent: parent.to_owned(),
                    reading: false,
                    refresh_after_read: false,
                    lifecycle_revision: 0,
                    active_turn: None,
                    read_queued: false,
                },
            );
        }
        // Incomplete thread/started metadata cannot permanently veto later
        // explicit ancestry from this Session's known parent.
        self.unrelated.remove(id);
        let child = self.children.get_mut(id).expect("discovered child");
        if let Some(name) =
            nonempty(metadata, "agentNickname").or_else(|| nonempty(metadata, "name"))
        {
            child.info.name = Some(name.to_owned());
        }
        if let Some(description) = nonempty(metadata, "preview") {
            child.info.description = Some(description.to_owned());
        }
        if let Some(role) = nonempty(metadata, "agentRole") {
            child.info.kind = Some(role.to_owned());
        }
        update.activity(ActivityEvent::Discovered(child.info.clone()));
        if !child.reading
            && !self.reads.values().any(|read| read.child == id)
            && child.info.coverage != TranscriptCoverage::Complete
        {
            self.read_child(id, update);
        }
    }

    fn read_child(&mut self, id: &str, update: &mut Update) {
        let Some(child) = self.children.get_mut(id) else {
            return;
        };
        if child.reading {
            return;
        }
        if self.reads.len() >= MAX_CONCURRENT_READS {
            if !child.read_queued {
                child.read_queued = true;
                self.queued_reads.push_back(id.to_owned());
            }
            return;
        }
        child.read_queued = false;
        child.reading = true;
        self.next_request += 1;
        let request = format!("ferrite-agent-history-{}", self.next_request);
        self.reads.insert(
            request.clone(),
            Read {
                child: id.to_owned(),
                revision: self.revision,
            },
        );
        update.requests.push(
            json!({"jsonrpc":"2.0", "id":request, "method":"thread/read",
            "params":{"threadId":id,"includeTurns":true}}),
        );
    }

    fn start_queued_reads(&mut self, update: &mut Update) {
        while self.reads.len() < MAX_CONCURRENT_READS {
            let Some(id) = self.queued_reads.pop_front() else {
                break;
            };
            self.read_child(&id, update);
        }
    }

    fn read_result(&mut self, read: Read, frame: &Value, update: &mut Update) {
        let thread = &frame["result"]["thread"];
        let expected_parent = self
            .children
            .get(&read.child)
            .map(|child| child.parent.clone());
        let valid = thread["id"].as_str() == Some(read.child.as_str())
            && metadata_parent(thread) == expected_parent.as_deref();
        let Some(child) = self.children.get_mut(&read.child) else {
            return;
        };
        child.reading = false;
        if !valid || frame.get("error").is_some() {
            child.info.coverage = TranscriptCoverage::Unavailable;
            update.activity(ActivityEvent::Coverage {
                key: child.info.key.clone(),
                coverage: TranscriptCoverage::Unavailable,
            });
            if frame.get("error").is_none() {
                self.unrelated.insert(read.child.clone());
                self.conflicts.insert(read.child.clone());
                update.activity(ActivityEvent::Detached {
                    key: child.info.key.clone(),
                });
            }
            return;
        }
        if let Some(name) = nonempty(thread, "agentNickname").or_else(|| nonempty(thread, "name")) {
            child.info.name = Some(name.to_owned());
        }
        if let Some(role) = nonempty(thread, "agentRole") {
            child.info.kind = Some(role.to_owned());
        }
        let mut complete = !self.discarded && !self.revisions_full;
        let has_live_items = self
            .item_revisions
            .keys()
            .any(|(scope, _)| scope == &read.child);
        if let Some(turns) = thread["turns"].as_array() {
            for turn in turns {
                let full = turn["itemsView"].as_str().is_none_or(|view| view == "full");
                let settled = matches!(
                    turn["status"].as_str(),
                    Some("completed" | "failed" | "interrupted")
                );
                if !full || !settled {
                    complete = false;
                    continue;
                }
                let Some(turn_id) = turn["id"].as_str() else {
                    complete = false;
                    continue;
                };
                if let Some(items) = turn["items"].as_array() {
                    for item in items {
                        self.discover_from_item(&read.child, item, false, update);
                        let Some(item_id) = item["id"].as_str() else {
                            continue;
                        };
                        let id = item_key(turn_id, item_id);
                        if has_live_items
                            && !self
                                .item_revisions
                                .contains_key(&(read.child.clone(), id.clone()))
                        {
                            // No replay cursor places an unseen older item among
                            // live records. Keep live order and disclose the gap.
                            complete = false;
                            continue;
                        }
                        let newer = self.revisions_full
                            || self
                                .item_revisions
                                .get(&(read.child.clone(), id.clone()))
                                .is_some_and(|(revision, settled)| {
                                    *settled || *revision > read.revision
                                });
                        if !newer {
                            self.item(&read.child, turn_id, item, true, true, update);
                        }
                    }
                }
            }
        } else {
            complete = false;
        }
        let child = self
            .children
            .get_mut(&read.child)
            .expect("read child remains known");
        child.info.coverage = if complete {
            TranscriptCoverage::Complete
        } else {
            TranscriptCoverage::Partial
        };
        update.activity(ActivityEvent::Discovered(child.info.clone()));
        // Historical/notLoaded status is observation availability, never fresh
        // lifecycle. Only live scoped notifications can animate a child.
        if thread["status"]["type"].as_str() == Some("notLoaded")
            && child.lifecycle_revision <= read.revision
        {
            update.activity(ActivityEvent::Status {
                key: child.info.key.clone(),
                state: AgentStatus::NotLoaded,
            });
        }
        if child.refresh_after_read {
            child.refresh_after_read = false;
            self.read_child(&read.child, update);
        }
    }

    fn child_frame(&mut self, scope: &str, frame: &Value, update: &mut Update) {
        let params = &frame["params"];
        let method = frame["method"].as_str().unwrap_or_default();
        let key = self.key(scope);
        let turn = params["turnId"]
            .as_str()
            .or_else(|| params["turn"]["id"].as_str());
        let native_item = params["itemId"]
            .as_str()
            .or_else(|| params["item"]["id"].as_str());
        let id = turn
            .zip(native_item)
            .map(|(turn, item)| item_key(turn, item));
        self.revision += 1;
        if matches!(
            method,
            "thread/status/changed" | "thread/closed" | "turn/started" | "turn/completed"
        ) {
            if let Some(child) = self.children.get_mut(scope) {
                child.lifecycle_revision = self.revision;
            }
        }
        if let Some(id) = &id {
            let entry = (scope.to_owned(), id.clone());
            if self.item_revisions.len() < MAX_ITEM_REVISIONS
                || self.item_revisions.contains_key(&entry)
            {
                let settled = method == "item/completed"
                    || self
                        .item_revisions
                        .get(&entry)
                        .is_some_and(|(_, settled)| *settled);
                self.item_revisions.insert(entry, (self.revision, settled));
            } else {
                self.revisions_full = true;
            }
        }
        match method {
            "thread/status/changed" => {
                let state = match params["status"]["type"].as_str() {
                    Some("active") => {
                        let flags = params["status"]["activeFlags"].as_array();
                        if flags.is_some_and(|flags| {
                            flags.iter().any(|flag| {
                                matches!(
                                    flag.as_str(),
                                    Some("waitingOnApproval" | "waitingOnUserInput")
                                )
                            })
                        }) {
                            AgentStatus::Waiting
                        } else {
                            AgentStatus::Working
                        }
                    }
                    Some("idle") => AgentStatus::Idle,
                    Some("systemError") => AgentStatus::Failed,
                    Some("notLoaded") => AgentStatus::NotLoaded,
                    _ => AgentStatus::Unknown,
                };
                update.activity(ActivityEvent::Status { key, state });
            }
            "thread/closed" => update.activity(ActivityEvent::Status {
                key,
                state: AgentStatus::NotLoaded,
            }),
            "turn/started" => {
                if let Some(child) = self.children.get_mut(scope) {
                    child.active_turn = turn.map(str::to_owned);
                }
                update.activity(ActivityEvent::Status {
                    key,
                    state: AgentStatus::Working,
                });
            }
            "turn/completed" => {
                let child = self.children.get_mut(scope).expect("known child");
                if child
                    .active_turn
                    .as_deref()
                    .is_some_and(|active| Some(active) != turn)
                {
                    return;
                }
                if !matches!(
                    params["turn"]["status"].as_str(),
                    Some("completed" | "failed" | "interrupted")
                ) {
                    update.activity(ActivityEvent::Status {
                        key,
                        state: AgentStatus::Unknown,
                    });
                    return;
                }
                child.active_turn = None;
                if let Some(SessionEvent::TurnEnded { outcome, cost_usd }) =
                    wire::parse_line(&frame.to_string())
                {
                    let state = match &outcome {
                        TurnOutcome::Completed => AgentStatus::Idle,
                        TurnOutcome::Interrupted => AgentStatus::Interrupted,
                        TurnOutcome::Error(_) => AgentStatus::Failed,
                    };
                    update.activity(ActivityEvent::Content {
                        key: key.clone(),
                        id: turn.map(|turn| format!("turn:{turn}")),
                        event: ExecutionEvent::TurnEnded { outcome, cost_usd },
                    });
                    update.activity(ActivityEvent::Status { key, state });
                    if let Some(child) = self.children.get_mut(scope) {
                        if child.reading {
                            child.refresh_after_read = true;
                        } else if child.info.coverage != TranscriptCoverage::Complete {
                            self.read_child(scope, update);
                        }
                    }
                }
            }
            "item/started" | "item/completed" => {
                if let Some(turn) = turn {
                    self.item(
                        scope,
                        turn,
                        &params["item"],
                        method == "item/completed",
                        false,
                        update,
                    );
                    for event in
                        wire::parse_events(&frame.to_string())
                            .into_iter()
                            .filter(|event| {
                                matches!(
                                    event,
                                    SessionEvent::ContentBoundary
                                        | SessionEvent::Progress {
                                            event: crate::progress::ProgressEvent::Phase { .. }
                                        }
                                )
                            })
                    {
                        if let Some(event) = execution(event) {
                            update.activity(ActivityEvent::Content {
                                key: key.clone(),
                                id: id.clone(),
                                event,
                            });
                        }
                    }
                }
            }
            "thread/name/updated" => {
                if let (Some(name), Some(child)) =
                    (params["name"].as_str(), self.children.get_mut(scope))
                {
                    child.info.name = Some(name.to_owned());
                    update.activity(ActivityEvent::Discovered(child.info.clone()));
                }
            }
            _ => {
                for event in wire::parse_events(&frame.to_string()) {
                    match event {
                        SessionEvent::DecisionRequested { mut decision } => {
                            if let Some(turn) = turn {
                                decision.tool_use_id = item_key(turn, &decision.tool_use_id);
                            }
                            if self.requests.len() < MAX_PENDING_FRAMES
                                || self.requests.contains_key(&decision.id)
                            {
                                self.requests.insert(decision.id.clone(), scope.to_owned());
                            }
                            update.activity(ActivityEvent::Decision {
                                subject: Some(Subject::Subagent(key.clone())),
                                decision,
                            });
                        }
                        event => {
                            if let Some(event) = execution(event) {
                                // Content streams require item identity. Usage has
                                // no item and is scoped by the child key alone.
                                if id.is_some()
                                    || matches!(
                                        event,
                                        ExecutionEvent::TokenUsage { .. }
                                            | ExecutionEvent::Progress { .. }
                                            | ExecutionEvent::ContentBoundary
                                    )
                                {
                                    update.activity(ActivityEvent::Content {
                                        key: key.clone(),
                                        id: id.clone(),
                                        event: scoped_execution(event, turn),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn item(
        &mut self,
        scope: &str,
        turn: &str,
        item: &Value,
        completed: bool,
        historical: bool,
        update: &mut Update,
    ) {
        let (Some(native_id), Some(kind)) = (item["id"].as_str(), item["type"].as_str()) else {
            return;
        };
        let id = item_key(turn, native_id);
        let key = self.key(scope);
        let emit = |event, update: &mut Update| {
            update.activity(if historical {
                ActivityEvent::HistoryContent {
                    key: key.clone(),
                    id: Some(id.clone()),
                    event,
                }
            } else {
                ActivityEvent::Content {
                    key: key.clone(),
                    id: Some(id.clone()),
                    event,
                }
            })
        };
        match kind {
            "agentMessage" | "plan" if completed => {
                if let Some(text) = item["text"].as_str() {
                    emit(
                        ExecutionEvent::TextSnapshot {
                            text: text.to_owned(),
                        },
                        update,
                    );
                }
            }
            "reasoning" if completed => {
                if let Some(parts) = item["summary"].as_array() {
                    for (index, part) in parts.iter().enumerate() {
                        if let Some(text) = part.as_str() {
                            emit(
                                ExecutionEvent::ReasoningSummaryPart {
                                    item_id: id.clone(),
                                    summary_index: index as u64,
                                    text: text.into(),
                                    snapshot: true,
                                },
                                update,
                            );
                        }
                    }
                }
                emit(ExecutionEvent::ContentBoundary, update);
            }
            "userMessage" if completed => {
                let text = item["content"].as_array().map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| part["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                });
                if let Some(text) = text {
                    emit(ExecutionEvent::Prompt { text }, update);
                }
            }
            "agentMessage" | "plan" | "reasoning" | "userMessage" => {}
            "commandExecution"
            | "fileChange"
            | "mcpToolCall"
            | "dynamicToolCall"
            | "sleep"
            | "webSearch"
            | "imageView"
            | "imageGeneration"
            | "collabAgentToolCall"
            | "collabToolCall"
            | "subAgentActivity" => {
                let params = json!({"item":item});
                if let Some(event) = wire::parse_item(&params, false).and_then(execution) {
                    emit(scoped_execution(event, Some(turn)), update);
                }
                if completed {
                    if let Some(event) = wire::parse_item(&params, true).and_then(execution) {
                        emit(scoped_execution(event, Some(turn)), update);
                    }
                }
            }

            _ if completed => {
                if let Some(text) = item["text"].as_str() {
                    emit(
                        ExecutionEvent::Notice {
                            text: text.to_owned(),
                        },
                        update,
                    );
                }
            }
            _ => {}
        }
    }
}

pub(super) fn frame_scope(value: &Value) -> Option<&str> {
    frame_scope_checked(value).ok().flatten()
}

fn frame_scope_checked(value: &Value) -> Result<Option<&str>, ()> {
    consistent_id(
        &value["params"]["threadId"],
        &value["params"]["thread"]["id"],
    )
}

fn metadata_parent(value: &Value) -> Option<&str> {
    consistent_id(
        &value["parentThreadId"],
        &value["source"]["subAgent"]["thread_spawn"]["parent_thread_id"],
    )
    .ok()
    .flatten()
}

// The current protocol may include both legacy and current metadata. Null is
// absent; any supplied non-null identifier must be valid and agree.
fn consistent_id<'a>(first: &'a Value, second: &'a Value) -> Result<Option<&'a str>, ()> {
    let decode = |value: &'a Value| {
        if value.is_null() {
            Ok(None)
        } else {
            value
                .as_str()
                .filter(|id| !id.is_empty())
                .map(Some)
                .ok_or(())
        }
    };
    match (decode(first)?, decode(second)?) {
        (Some(first), Some(second)) if first != second => Err(()),
        (first, second) => Ok(first.or(second)),
    }
}

fn nonempty<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value[key].as_str().filter(|text| !text.is_empty())
}

fn item_key(turn: &str, item: &str) -> String {
    serde_json::to_string(&(turn, item)).expect("string tuple serializes")
}

fn execution(event: SessionEvent) -> Option<ExecutionEvent> {
    ExecutionEvent::from_session(&event)
}

fn scoped_execution(mut event: ExecutionEvent, turn: Option<&str>) -> ExecutionEvent {
    if let Some(turn) = turn {
        match &mut event {
            ExecutionEvent::ToolStarted { id, .. }
            | ExecutionEvent::ToolCompleted { id, .. }
            | ExecutionEvent::ToolOutputDelta { id, .. } => *id = item_key(turn, id),
            ExecutionEvent::ReasoningSummaryPart { item_id, .. } => {
                *item_id = item_key(turn, item_id)
            }
            ExecutionEvent::Progress {
                event: crate::progress::ProgressEvent::Tool { id, .. },
            } => *id = item_key(turn, id),
            _ => {}
        }
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{Activity, ActivityInput};
    use std::time::Instant;

    fn spawn(parent: &str, child: &str) -> Value {
        json!({"method":"item/completed","params":{"threadId":parent,"turnId":"parent-turn",
            "item":{"type":"subAgentActivity","id":format!("spawn-{child}"),"kind":"started",
                "agentThreadId":child,"agentPath":format!("/root/{child}")}}})
    }

    fn delta(child: &str, text: &str) -> Value {
        json!({"method":"item/agentMessage/delta","params":{"threadId":child,"turnId":"turn",
            "itemId":"message","delta":text}})
    }

    fn history(id: Value, child: &str, parent: &str, text: &str) -> Value {
        json!({"id":id,"result":{"thread":{"id":child,"parentThreadId":parent,"agentNickname":"Plato",
            "status":{"type":"notLoaded"},"turns":[{"id":"turn","status":"completed","itemsView":"full",
                "items":[{"type":"agentMessage","id":"message","text":text}]}]}}})
    }

    fn fixture(source: &str) -> (Router, Vec<SessionEvent>, Vec<Value>) {
        let frames: Vec<Value> = source
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let root = frames
            .iter()
            .find_map(|frame| frame.get("result")?.get("thread")?.get("id")?.as_str())
            .unwrap()
            .to_owned();
        let mut router = Router::default();
        router.identify_main(&root);
        let mut events = Vec::new();
        let mut requests = Vec::new();
        for mut frame in frames {
            if let Some(child) = frame["result"]["thread"]["id"].as_str() {
                if child != root {
                    if let Some((id, _)) = router.reads.iter().find(|(_, read)| read.child == child)
                    {
                        frame["id"] = json!(id);
                    }
                }
            }
            let update = router.observe(frame);
            events.extend(update.events);
            requests.extend(update.requests);
        }
        (router, events, requests)
    }

    #[test]
    fn captured_overlap_reuse_stays_out_of_main_and_recovers_provider_names() {
        let (router, events, requests) = fixture(include_str!(
            "../../../tests/fixtures/subagents/codex-overlap-reuse-0.153.4.jsonl"
        ));
        assert_eq!(router.children.len(), 2);
        let text: String = events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::TextDelta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(text.contains("MAIN_DONE"));
        assert!(!text.contains("ALPHA_DONE") && !text.contains("BETA_DONE"));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, SessionEvent::TurnEnded { .. }))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    SessionEvent::Activity(ActivityEvent::Content {
                        event: ExecutionEvent::TurnEnded { .. },
                        ..
                    })
                ))
                .count(),
            3
        );
        assert!(router
            .children
            .values()
            .any(|child| child.info.name.as_deref() == Some("Plato")));
        assert!(router
            .children
            .values()
            .any(|child| child.info.name.as_deref() == Some("Euler")));
        assert!(requests
            .iter()
            .all(|request| request["method"] == "thread/read"));
    }

    #[test]
    fn captured_nested_tree_uses_immediate_parent_and_root_scoped_identity() {
        let (router, events, _) = fixture(include_str!(
            "../../../tests/fixtures/subagents/codex-nested-0.153.4.jsonl"
        ));
        let root = router.root.as_deref().unwrap();
        assert_eq!(router.children.len(), 2);
        let (child_id, _) = router
            .children
            .iter()
            .find(|(_, child)| child.parent == root)
            .unwrap();
        let (grandchild_id, grandchild) = router
            .children
            .iter()
            .find(|(_, child)| child.parent == *child_id)
            .unwrap();
        assert_eq!(
            grandchild.info.parent,
            Some(Subject::Subagent(router.key(child_id)))
        );
        assert_eq!(
            grandchild.info.key,
            AgentKey::new(Provider::Codex, root, grandchild_id)
        );
        assert!(events.iter().any(|event|matches!(event,SessionEvent::Activity(ActivityEvent::Content{key,event:ExecutionEvent::TextSnapshot{text},..}) if *key==grandchild.info.key && text=="GRANDCHILD_DONE")));
    }

    #[test]
    fn unknown_attribution_is_bounded_then_replayed_only_after_explicit_parentage() {
        let mut router = Router::default();
        router.identify_main("main");
        for _ in 0..400 {
            assert!(router.observe(delta("unknown", "x")).events.is_empty());
        }
        assert!(router.pending.len() <= MAX_PENDING_FRAMES);
        assert!(router.pending_bytes <= MAX_PENDING_BYTES);
        assert!(router.discarded);
        let update = router.observe(spawn("main", "unknown"));
        assert!(update
            .events
            .iter()
            .all(|event| matches!(event, SessionEvent::Activity(_))));
        assert!(update.events.iter().any(|event| matches!(
            event,
            SessionEvent::Activity(ActivityEvent::Content {
                event: ExecutionEvent::TextDelta { .. },
                ..
            })
        )));
        assert_eq!(update.requests.len(), 1);
        assert_eq!(update.requests[0]["method"], "thread/read");
        assert!(router
            .observe(delta("unrelated", "never-main"))
            .events
            .is_empty());
        assert!(router
            .observe(json!({"method":"item/agentMessage/delta","params":{"delta":"missing-owner"}}))
            .events
            .is_empty());
    }

    #[test]
    fn incomplete_metadata_does_not_veto_later_explicit_discovery() {
        let mut router = Router::default();
        router.identify_main("main");
        router.observe(json!({"method":"thread/started","params":{"thread":{"id":"child"}}}));
        assert!(router
            .observe(delta("child", "untrusted"))
            .events
            .is_empty());
        router.observe(spawn("main", "child"));
        assert!(!router.observe(delta("child", "trusted")).events.is_empty());
    }

    #[test]
    fn conflicting_or_malformed_scope_and_parent_fields_never_fall_back() {
        let mut router = Router::default();
        router.identify_main("main");
        for first in [json!("main"), json!(37), json!("")] {
            let mut frame = delta("child", "ambiguous");
            frame["params"]["threadId"] = first;
            frame["params"]["thread"] = json!({"id":"child"});
            assert!(router.observe(frame).events.is_empty());
        }
        for first in [json!("other"), json!(37), json!("")] {
            router.observe(json!({"method":"thread/started","params":{"thread":{
                "id":"child","parentThreadId":first,
                "source":{"subAgent":{"thread_spawn":{"parent_thread_id":"main"}}}
            }}}));
            assert!(!router.children.contains_key("child"));
        }
        let request = router.observe(spawn("main", "child")).requests.remove(0);
        let mut reply = history(request["id"].clone(), "child", "main", "wrong ancestry");
        reply["result"]["thread"]["source"] =
            json!({"subAgent":{"thread_spawn":{"parent_thread_id":"other"}}});
        let result = router.observe(reply);
        assert!(result.events.iter().any(|event| matches!(
            event,
            SessionEvent::Activity(ActivityEvent::Detached { .. })
        )));
        assert!(router
            .observe(delta("child", "quarantined"))
            .events
            .is_empty());
    }

    #[test]
    fn read_capacity_does_not_limit_discovery_content_or_decisions() {
        let mut router = Router::default();
        router.identify_main("main");
        let mut requests = Vec::new();
        for index in 0..150 {
            requests.extend(
                router
                    .observe(spawn("main", &format!("child-{index}")))
                    .requests,
            );
        }
        assert_eq!(router.children.len(), 150);
        assert_eq!(requests.len(), MAX_CONCURRENT_READS);
        assert_eq!(router.queued_reads.len(), 22);
        assert!(router
            .observe(delta("child-149", "live beyond cache"))
            .events
            .iter()
            .any(|event| matches!(
                event,
                SessionEvent::Activity(ActivityEvent::Content {
                    event: ExecutionEvent::TextDelta { .. },
                    ..
                })
            )));
        let decision = router.observe(json!({"id":"last","method":"item/commandExecution/requestApproval",
            "params":{"threadId":"child-149","turnId":"turn","itemId":"tool","command":"synthetic"}}));
        assert!(decision.events.iter().any(|event| matches!(
            event,
            SessionEvent::Activity(ActivityEvent::Decision {
                subject: Some(Subject::Subagent(_)),
                ..
            })
        )));
        let freed = router.observe(history(
            requests[0]["id"].clone(),
            "child-0",
            "main",
            "past",
        ));
        assert_eq!(freed.requests.len(), 1);
        assert_eq!(freed.requests[0]["params"]["threadId"], "child-128");
        assert_eq!(router.reads.len(), MAX_CONCURRENT_READS);
        assert_eq!(router.queued_reads.len(), 21);
    }

    #[test]
    fn malformed_history_reply_keeps_request_and_stale_history_cannot_replace_live_final() {
        let mut router = Router::default();
        router.identify_main("main");
        let request = router.observe(spawn("main", "child")).requests.remove(0);
        assert!(router
            .observe(json!({"id":request["id"]}))
            .events
            .is_empty());
        assert_eq!(router.reads.len(), 1);
        router.observe(delta("child", "live"));
        router.observe(
            json!({"method":"item/completed","params":{"threadId":"child","turnId":"turn",
            "item":{"type":"agentMessage","id":"message","text":"live-final"}}}),
        );
        let update = router.observe(history(
            request["id"].clone(),
            "child",
            "main",
            "stale-final",
        ));
        assert!(!update.events.iter().any(|event| matches!(
            event,
            SessionEvent::Activity(ActivityEvent::HistoryContent { .. })
        )));
    }

    #[test]
    fn history_is_read_without_resuming_and_does_not_create_fresh_work() {
        let mut router = Router::default();
        router.identify_main("main");
        let discovered = router.observe(spawn("main", "child"));
        let response = router.observe(history(
            discovered.requests[0]["id"].clone(),
            "child",
            "main",
            "past answer",
        ));
        let mut activity = Activity::default();
        activity.apply(ActivityInput::Connect { generation: 1 });
        for event in discovered.events.into_iter().chain(response.events) {
            if let SessionEvent::Activity(event) = event {
                activity.apply(ActivityInput::Observe {
                    generation: 1,
                    event,
                    at: Instant::now(),
                });
            }
        }
        let view = activity.view();
        let child = view.children().into_iter().next().unwrap();
        assert_eq!(child.status(), AgentStatus::NotLoaded);
        assert!(!child.fresh());
        assert_eq!(child.coverage(), TranscriptCoverage::Complete);
        assert!(!child.transcript().blocks().is_empty());
    }

    #[test]
    fn contradictory_history_quarantines_further_child_events() {
        let mut router = Router::default();
        router.identify_main("main");
        let request = router.observe(spawn("main", "child")).requests.remove(0);
        let update = router.observe(history(
            request["id"].clone(),
            "child",
            "other-root",
            "wrong tree",
        ));
        assert!(update.events.iter().any(|event| matches!(
            event,
            SessionEvent::Activity(ActivityEvent::Detached { .. })
        )));
        router.observe(spawn("main", "child")); // Repeating weaker evidence cannot undo conflict.
        assert!(router
            .observe(delta("child", "never-main"))
            .events
            .is_empty());
    }

    #[test]
    fn root_resume_discovers_stored_children_without_replaying_main_content() {
        let mut router = Router::default();
        router.identify_main("main");
        let item = spawn("main", "child")["params"]["item"].clone();
        let update = router.root_history(&json!({"id":"main","turns":[{"items":[item]}]}));
        assert_eq!(router.children.len(), 1);
        assert_eq!(update.requests[0]["method"], "thread/read");
        assert!(update
            .events
            .iter()
            .all(|event| matches!(event, SessionEvent::Activity(ActivityEvent::Discovered(_)))));
    }

    #[test]
    fn child_approval_retains_typed_handle_and_cancellation() {
        let mut router = Router::default();
        router.identify_main("main");
        router.observe(spawn("main", "child"));
        for id in [json!(0), json!("0")] {
            let update=router.observe(json!({"id":id,"method":"item/commandExecution/requestApproval",
                "params":{"threadId":"child","turnId":"turn","itemId":"tool","command":"echo test"}}));
            let SessionEvent::Activity(ActivityEvent::Decision { subject, decision }) =
                &update.events[0]
            else {
                panic!("missing child Decision")
            };
            assert_eq!(subject, &Some(Subject::Subagent(router.key("child"))));
            assert_eq!(wire::decision_request_id(&decision.id).unwrap(), id);
            assert_eq!(decision.tool_use_id, item_key("turn", "tool"));
        }
        let update=router.observe(json!({"method":"serverRequest/resolved","params":{"threadId":"child","requestId":"0"}}));
        assert!(
            matches!(&update.events[0],SessionEvent::Activity(ActivityEvent::DecisionCancelled{id}) if id=="\"0\"")
        );
        assert!(router.requests.contains_key("0"));
    }
    #[test]
    fn a_read_cannot_append_unseen_old_items_after_new_live_content() {
        let mut router = Router::default();
        router.identify_main("main");
        let request = router.observe(spawn("main", "child")).requests.remove(0);
        router.observe(delta("child", "new live content"));
        let mut response = history(request["id"].clone(), "child", "main", "old missing item");
        response["result"]["thread"]["turns"][0]["items"][0]["id"] = json!("older-item");
        let update = router.observe(response);
        assert!(!update.events.iter().any(|event| matches!(
            event,
            SessionEvent::Activity(ActivityEvent::HistoryContent { .. })
        )));
        assert_eq!(
            router.children["child"].info.coverage,
            TranscriptCoverage::Partial
        );
    }

    #[test]
    fn resolved_unknown_approval_is_not_resurrected_after_discovery() {
        let mut router = Router::default();
        router.identify_main("main");
        router.observe(
            json!({"id":7,"method":"item/commandExecution/requestApproval",
            "params":{"threadId":"child","turnId":"turn","itemId":"tool","command":"synthetic"}}),
        );
        let cancelled = router.observe(
            json!({"method":"serverRequest/resolved","params":{"threadId":"child","requestId":7}}),
        );
        assert!(
            matches!(&cancelled.events[0],SessionEvent::Activity(ActivityEvent::DecisionCancelled{id}) if id=="7")
        );
        let discovered = router.observe(spawn("main", "child"));
        assert!(!discovered.events.iter().any(|event| matches!(
            event,
            SessionEvent::Activity(ActivityEvent::Decision { .. })
        )));
    }

    #[test]
    fn old_child_completion_cannot_retire_a_reused_child_turn() {
        let mut router = Router::default();
        router.identify_main("main");
        router.observe(spawn("main", "child"));
        router.observe(
            json!({"method":"turn/started","params":{"threadId":"child","turn":{"id":"new-turn"}}}),
        );
        let stale=router.observe(json!({"method":"turn/completed","params":{"threadId":"child","turn":{"id":"old-turn","status":"completed"}}}));
        assert!(stale.events.is_empty());
        assert_eq!(
            router.children["child"].active_turn.as_deref(),
            Some("new-turn")
        );
    }
}
