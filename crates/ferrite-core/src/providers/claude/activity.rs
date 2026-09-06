//! Attribution for one Claude connection, before any Main event is emitted.
//!
//! Claude forwards completed child blocks rather than child token deltas.
//! Invocation, task, and agent IDs are separate aliases; a resumed task can
//! name a new SendMessage invocation while its text still names the first Agent.

use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::Value;

use crate::activity::{
    ActivityEvent, AgentInfo, AgentKey, AgentStatus, ExecutionEvent, Subject, TranscriptCoverage,
};
use crate::store::Provider;
use crate::SessionEvent;

use super::wire;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Alias {
    Invocation(String),
    Task(String),
    Agent(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MainStreamKind {
    Text,
    Thinking,
}

struct MainStreamBlock {
    message: String,
    index: u64,
    kind: MainStreamKind,
    delivery: Option<String>,
}

#[derive(Default)]
pub(super) struct Decoder {
    root: String,
    aliases: HashMap<Alias, AgentKey>,
    agents: HashMap<AgentKey, AgentInfo>,
    originals: HashSet<AgentKey>,
    states: HashMap<AgentKey, AgentStatus>,
    /// Only unsettled tool calls are retained. Permission requests can precede
    /// execution but never need an already completed tool's live owner.
    tools: HashMap<String, Vec<Subject>>,
    requests: HashMap<String, (Option<Subject>, String)>,
    seen_child_frames: HashSet<String>,
    frame_order: VecDeque<String>,
    excluded_tasks: HashSet<String>,
    main_stream_message: Option<String>,
    main_stream_blocks: HashMap<(String, u64), MainStreamBlock>,
    main_stream_order: VecDeque<(String, u64)>,
    seen_main_frames: HashSet<String>,
    main_frame_order: VecDeque<String>,
    deliveries: HashMap<(Subject, String), Vec<String>>,
    delivery_order: VecDeque<(Subject, String)>,
    pending_retractions: HashSet<(Subject, String)>,
    /// Main's last provider-reported occupancy. Result aggregates account for
    /// a turn but do not always carry a new occupancy snapshot.
    usage: Usage,
    rate_limits: (
        Option<crate::RateLimitWindow>,
        Option<crate::RateLimitWindow>,
    ),
}

#[derive(Default)]
struct Usage {
    occupancy: Option<u64>,
    context_window: Option<u64>,
}

impl Decoder {
    /// The host successfully answered this request. Actual execution state
    /// still comes from the provider; a sent answer is not proof of progress.
    pub(super) fn decision_resolved(&mut self, id: &str) {
        self.requests.remove(id);
    }

    pub(super) fn decode(&mut self, line: &str) -> Vec<SessionEvent> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let mut events = Vec::new();
        // The CLI announces `init` at the head of every turn, the first one
        // at Session start. A repeat is therefore the head of a turn — an
        // operator's, or an autonomous one a task notification started —
        // and the earliest fact that Main is at work, well before the API
        // has answered. It is what keeps a Main between its own result
        // and its resume from reading as finished.
        let turn_head = value["type"] == "system"
            && value["subtype"] == "init"
            && !self.root.is_empty()
            && string(&value, "session_id") == Some(self.root.as_str());
        if let Some(root) = string(&value, "session_id") {
            if value["type"] == "system"
                && value["subtype"] == "init"
                && !self.root.is_empty()
                && self.root != root
            {
                for key in self.agents.keys() {
                    push(
                        &mut events,
                        ActivityEvent::Status {
                            key: key.clone(),
                            state: AgentStatus::Unknown,
                        },
                    );
                }
                for id in self.requests.keys() {
                    push(
                        &mut events,
                        ActivityEvent::DecisionCancelled { id: id.clone() },
                    );
                }
                *self = Self::default();
            } else if !self.root.is_empty() && self.root != root {
                // Another conversation's frame cannot mutate this connection's
                // Main or join aliases that happen to use the same native ID.
                return events;
            }
            if self.root.is_empty() {
                self.root = root.to_owned();
            }
        }
        if let Some(SessionEvent::RateLimits { five_hour, weekly }) = wire::parse_rate_limits(line)
        {
            if value["rate_limit_info"]["unifiedWindows"].is_object() {
                self.rate_limits = (five_hour, weekly);
            } else {
                if five_hour.is_some() {
                    self.rate_limits.0 = five_hour;
                }
                if weekly.is_some() {
                    self.rate_limits.1 = weekly;
                }
            }
            events.push(SessionEvent::RateLimits {
                five_hour: self.rate_limits.0,
                weekly: self.rate_limits.1,
            });
        }
        match string(&value, "type") {
            Some("system") if string(&value, "subtype") == Some("init") => {
                if let Some(mode) = string(&value, "permissionMode") {
                    events.push(SessionEvent::PermissionMode { mode: mode.into() });
                }
                if let Some(event) = wire::parse_value(&value) {
                    events.push(event);
                }
                if turn_head {
                    events.push(SessionEvent::Progress {
                        event: crate::progress::ProgressEvent::Phase {
                            phase: crate::progress::Phase::Working,
                            detail: String::new(),
                        },
                    });
                }
            }
            Some("system") if string(&value, "subtype") != Some("init") => {
                if string(&value, "subtype") == Some("informational") {
                    self.notice(&value, string(&value, "content"), &mut events);
                }
                if string(&value, "subtype") == Some("model_refusal_fallback") {
                    let subject = self.scope(&value, &mut events).unwrap_or(Subject::Main);
                    self.retract_uuids(
                        &subject,
                        value["retracted_message_uuids"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str),
                        &mut events,
                    );
                }
                if string(&value, "subtype") == Some("local_command_output")
                    && !self.seen_main_frame(&value)
                {
                    if let Some(text) = string(&value, "content") {
                        self.main_content(
                            &value,
                            delivery_id(&value, "0"),
                            ExecutionEvent::Text { text: text.to_owned() },
                            &mut events,
                        );
                    }
                }
                self.task(&value, &mut events);
                if let Some(event) = wire::parse_value(&value) {
                    events.push(event);
                }
                if !matches!(
                    string(&value, "subtype"),
                    Some("task_started" | "task_progress" | "task_updated" | "task_notification")
                ) {
                    self.progress(&value, &mut events);
                }
            }
            Some("conversation_reset") => {
                if let Some(next) = string(&value, "new_conversation_id") {
                    *self = Self::default();
                    self.root = next.into();
                    events.push(SessionEvent::ConversationReset {
                        session_id: next.into(),
                    });
                }
            }
            Some("auth_status") => {
                let output = value["output"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n");
                let error = value["error"].as_str().unwrap_or("");
                let text = [output.as_str(), error]
                    .into_iter()
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                self.notice(&value, (!text.is_empty()).then_some(&text), &mut events);
            }
            Some("control_request") => self.decision(&value, &mut events),
            Some("control_cancel_request") => {
                if let Some(id) = string(&value, "request_id") {
                    if let Some((Some(Subject::Subagent(key)), _)) = self.requests.remove(id) {
                        if !self.waiting(&key) {
                            self.status(&key, AgentStatus::Unknown, &mut events);
                        }
                    }
                    push(
                        &mut events,
                        ActivityEvent::DecisionCancelled { id: id.to_owned() },
                    );
                }
            }
            Some("assistant" | "user") => self.message(&value, &mut events),
            Some("stream_event") => {
                // Only the null/absent Main stream is part of Claude's current
                // contract. An unexpected child stream must never become Main.
                if matches!(value.get("parent_tool_use_id"), None | Some(Value::Null)) {
                    self.main_stream(&value, &mut events);
                }
            }
            Some("result") => {
                // Claude's result envelope is Main-only. Explicit child or
                // malformed attribution must never retire Main's queue.
                if !matches!(value.get("parent_tool_use_id"), None | Some(Value::Null)) {
                    return events;
                }
                if let Some(usage) = self.main_usage(&value) {
                    events.push(usage);
                }
                if let Some(details) = wire::parse_usage_details_value(&value) {
                    events.push(details);
                }
                if let Some(event) = wire::parse_value(&value) {
                    // The published SDK names only `human` as operator
                    // origin. Peer/observer/coordinator and future origins
                    // cannot acknowledge Ferrite's pending operator prompt.
                    // Absent/null keeps compatibility with older Main frames.
                    let main_origin = matches!(value.get("origin"), None | Some(Value::Null))
                        || value["origin"]["kind"] == "human";
                    if !main_origin {
                        if let SessionEvent::TurnEnded { outcome, cost_usd } = event {
                            push(
                                &mut events,
                                ActivityEvent::BackgroundTurnEnded { outcome, cost_usd },
                            );
                        }
                    } else {
                        events.push(event);
                    }
                }
            }
            _ => {
                if let Some(event) = wire::parse_value(&value) {
                    events.push(event);
                }
                if turn_head {
                    events.push(SessionEvent::Progress {
                        event: crate::progress::ProgressEvent::Phase {
                            phase: crate::progress::Phase::Working,
                            detail: String::new(),
                        },
                    });
                }
                self.progress(&value, &mut events);
            }
        }
        events
    }

    /// Extra SDK observations share the execution path and attribution used
    /// for tools. Completed message snapshots are already decoded by message().
    fn progress(&mut self, value: &Value, events: &mut Vec<SessionEvent>) {
        let extras: Vec<_> = wire::parse_events_value(value)
            .into_iter()
            .filter(|event| {
                matches!(
                    event,
                    SessionEvent::Progress { .. } | SessionEvent::ContentBoundary
                )
            })
            .collect();
        if extras.is_empty() {
            return;
        }
        let owner = string(value, "tool_use_id").and_then(|id| self.tool_owner(id));
        if value["owned_by_subagent"] == true
            && owner.is_none()
            && matches!(value.get("parent_tool_use_id"), None | Some(Value::Null))
        {
            return;
        }

        let Some(subject) = owner.or_else(|| self.scope(value, events)) else {
            return;
        };
        for event in extras {
            match &subject {
                Subject::Main => events.push(event),
                Subject::Subagent(key) => {
                    if let Some(event) = ExecutionEvent::from_session(&event) {
                        self.content(key, None, event, events);
                    }
                }
            }
        }
    }

    fn native_key(&self, native: &str) -> AgentKey {
        AgentKey::new(Provider::Claude, &self.root, native)
    }

    fn scope(&mut self, value: &Value, events: &mut Vec<SessionEvent>) -> Option<Subject> {
        match value.get("parent_tool_use_id") {
            None | Some(Value::Null) => Some(Subject::Main),
            Some(Value::String(id)) if !id.is_empty() => {
                let alias = Alias::Invocation(id.clone());
                let key = self
                    .aliases
                    .get(&alias)
                    .cloned()
                    .unwrap_or_else(|| self.native_key(id));
                self.link(alias, &key, events);
                let mut info = self.info(&key);
                info.kind = string(value, "subagent_type")
                    .map(str::to_owned)
                    .or(info.kind);
                info.description = string(value, "task_description")
                    .map(str::to_owned)
                    .or(info.description);
                self.discover(info, events);
                Some(Subject::Subagent(key))
            }
            // A malformed attribution is not proof that Main owns a frame.
            _ => None,
        }
    }

    fn info(&self, key: &AgentKey) -> AgentInfo {
        self.agents
            .get(key)
            .cloned()
            .unwrap_or_else(|| AgentInfo::new(key.clone()))
    }

    fn discover(&mut self, info: AgentInfo, events: &mut Vec<SessionEvent>) {
        if self.agents.get(&info.key) != Some(&info) {
            self.agents.insert(info.key.clone(), info.clone());
            push(events, ActivityEvent::Discovered(info));
        }
    }

    fn link(&mut self, alias: Alias, key: &AgentKey, events: &mut Vec<SessionEvent>) {
        let native = match &alias {
            Alias::Invocation(id) => self.native_key(id),
            Alias::Task(id) => self.native_key(&format!("task:{id}")),
            Alias::Agent(id) => self.native_key(&format!("agent:{id}")),
        };
        if let Some(previous) = self.aliases.insert(alias, key.clone()) {
            if previous != *key {
                for target in self.aliases.values_mut() {
                    if *target == previous {
                        *target = key.clone();
                    }
                }
                for owners in self.tools.values_mut() {
                    for owner in owners {
                        if *owner == Subject::Subagent(previous.clone()) {
                            *owner = Subject::Subagent(key.clone());
                        }
                    }
                }
                for (owner, _) in self.requests.values_mut() {
                    if *owner == Some(Subject::Subagent(previous.clone())) {
                        *owner = Some(Subject::Subagent(key.clone()));
                    }
                }
                if let Some(old) = self.agents.remove(&previous) {
                    let mut info = self.info(key);
                    info.name = info.name.or(old.name);
                    info.parent = info.parent.or(old.parent);
                    info.kind = info.kind.or(old.kind);
                    info.description = info.description.or(old.description);
                    if info.coverage == TranscriptCoverage::Unavailable {
                        info.coverage = old.coverage;
                    }
                    self.agents.insert(key.clone(), info);
                }
                if let Some(state) = self.states.remove(&previous) {
                    self.states.entry(key.clone()).or_insert(state);
                }
                push(
                    events,
                    ActivityEvent::Alias {
                        from: previous,
                        to: key.clone(),
                    },
                );
            }
        } else if native != *key {
            // Persist explicit native aliases even if no provisional tab was
            // needed. A fresh decoder can emit the same task/agent reference
            // after reconnect, and restored Activity still resolves it to the
            // original spawning-tool key.
            push(
                events,
                ActivityEvent::Alias {
                    from: native,
                    to: key.clone(),
                },
            );
        }
    }

    fn status(&mut self, key: &AgentKey, state: AgentStatus, events: &mut Vec<SessionEvent>) {
        if matches!(
            state,
            AgentStatus::Idle
                | AgentStatus::Failed
                | AgentStatus::Interrupted
                | AgentStatus::Shutdown
        ) {
            self.requests
                .retain(|_, (owner, _)| owner.as_ref() != Some(&Subject::Subagent(key.clone())));
        }
        if self.states.insert(key.clone(), state) != Some(state) {
            push(
                events,
                ActivityEvent::Status {
                    key: key.clone(),
                    state,
                },
            );
        }
    }

    fn waiting(&self, key: &AgentKey) -> bool {
        self.requests
            .values()
            .any(|(subject, _)| subject.as_ref() == Some(&Subject::Subagent(key.clone())))
    }

    fn tool_owner(&self, id: &str) -> Option<Subject> {
        let owners = self.tools.get(id)?;
        (owners.len() == 1).then(|| owners[0].clone())
    }

    fn working(&mut self, key: &AgentKey, events: &mut Vec<SessionEvent>) {
        if !self.waiting(key) {
            self.status(key, AgentStatus::Working, events);
        }
    }

    fn content(
        &mut self,
        key: &AgentKey,
        id: Option<String>,
        event: ExecutionEvent,
        events: &mut Vec<SessionEvent>,
    ) {
        push(
            events,
            ActivityEvent::Content {
                key: key.clone(),
                id,
                event,
            },
        );
    }

    fn content_message(
        &mut self,
        key: &AgentKey,
        value: &Value,
        id: Option<String>,
        event: ExecutionEvent,
        events: &mut Vec<SessionEvent>,
    ) {
        if let Some(id) = &id {
            self.record_delivery(&Subject::Subagent(key.clone()), value, id, events);
        }
        self.content(key, id, event, events);
    }

    fn seen(&mut self, value: &Value) -> bool {
        let Some(uuid) = string(value, "uuid") else {
            return false;
        };
        let identity = format!("{}:{uuid}", self.root);
        if !self.seen_child_frames.insert(identity.clone()) {
            return true;
        }
        self.frame_order.push_back(identity);
        if self.frame_order.len() > 8192 {
            if let Some(old) = self.frame_order.pop_front() {
                self.seen_child_frames.remove(&old);
            }
        }
        false
    }

    fn message(&mut self, value: &Value, events: &mut Vec<SessionEvent>) {
        if string(value, "parent_tool_use_id").is_some() && self.seen(value) {
            return;
        }
        let Some(subject) = self.scope(value, events) else {
            return;
        };
        if subject == Subject::Main && self.seen_main_frame(value) {
            return;
        }
        self.retract_subject(&subject, value, events);
        self.progress(value, events);
        let child = match &subject {
            Subject::Main => None,
            Subject::Subagent(key) => Some(key.clone()),
        };
        let assistant = value["type"] == "assistant";
        if let Some(usage) = wire::parse_usage_value(value) {
            match &child {
                Some(key) => {
                    if let Some(event) = ExecutionEvent::from_session(&usage) {
                        self.content_message(
                            key,
                            value,
                            delivery_id(value, "usage"),
                            event,
                            events,
                        );
                    }
                }
                None => events.push(self.record_main_usage(value, usage)),
            }
        }
        if let Some(details) = wire::parse_usage_details_value(value) {
            match &child {
                Some(key) => {
                    if let Some(event) = ExecutionEvent::from_session(&details) {
                        self.content_message(
                            key,
                            value,
                            delivery_id(value, "usage-details"),
                            event,
                            events,
                        );
                    }
                }
                None => events.push(details),
            }
        }
        if let Some(text) = value["message"]["content"].as_str() {
            if child.is_none() && !assistant {
                return;
            }
            let event = if assistant {
                ExecutionEvent::Text {
                    text: text.to_owned(),
                }
            } else {
                ExecutionEvent::Prompt {
                    text: text.to_owned(),
                }
            };
            if let Some(key) = &child {
                let mut info = self.info(key);
                info.coverage = TranscriptCoverage::Live;
                self.discover(info, events);
                self.working(key, events);
                self.content_message(key, value, delivery_id(value, "0"), event, events);
            } else {
                self.main_content(value, delivery_id(value, "0"), event, events);
            }
            return;
        }
        let Some(blocks) = value["message"]["content"].as_array() else {
            return;
        };
        // A structured envelope accompanies one result in current Claude. Do
        // not copy it onto unrelated results if a future frame batches them.
        let structured_result = (blocks.iter().filter(|b| b["type"] == "tool_result").count() == 1)
            .then(|| value.get("tool_use_result"))
            .flatten();
        for (ordinal, block) in blocks.iter().enumerate() {
            let kind = match string(block, "type") {
                Some("text") => Some(MainStreamKind::Text),
                Some("thinking") => Some(MainStreamKind::Thinking),
                _ => None,
            };
            let streamed = child
                .is_none()
                .then(|| kind.and_then(|kind| self.bind_main_stream_block(value, kind)))
                .flatten();
            let id = streamed
                .clone()
                .or_else(|| delivery_id(value, &ordinal.to_string()));
            match string(block, "type") {
                Some("text" | "thinking") => {
                    if child.is_none() && !assistant {
                        continue;
                    }
                    let thinking = block["type"] == "thinking";
                    let Some(text) = block
                        .get(if thinking { "thinking" } else { "text" })
                        .and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let event = if !assistant {
                        ExecutionEvent::Prompt {
                            text: text.to_owned(),
                        }
                    } else if thinking {
                        match streamed {
                            Some(_) => ExecutionEvent::ThinkingSnapshot {
                                text: text.to_owned(),
                            },
                            None => ExecutionEvent::Thinking {
                                text: text.to_owned(),
                            },
                        }
                    } else {
                        match streamed {
                            Some(_) => ExecutionEvent::TextSnapshot {
                                text: text.to_owned(),
                            },
                            None => ExecutionEvent::Text {
                                text: text.to_owned(),
                            },
                        }
                    };
                    if let Some(key) = &child {
                        let mut info = self.info(key);
                        info.coverage = TranscriptCoverage::Live;
                        self.discover(info, events);
                        self.working(key, events);
                        self.content_message(key, value, id, event, events);
                    } else {
                        self.main_content(value, id, event, events);
                    }
                }
                Some("tool_use") if assistant => {
                    let (Some(tool_id), Some(name)) = (string(block, "id"), string(block, "name"))
                    else {
                        continue;
                    };
                    let owners = self.tools.entry(tool_id.to_owned()).or_default();
                    if !owners.contains(&subject) {
                        owners.push(subject.clone());
                    }
                    let input = block.get("input").cloned().unwrap_or(Value::Null);
                    if let Some(key) = &child {
                        self.working(key, events);
                        self.content_message(
                            key,
                            value,
                            id,
                            ExecutionEvent::ToolStarted {
                                id: tool_id.to_owned(),
                                name: name.to_owned(),
                                input: input.clone(),
                            },
                            events,
                        );
                    } else {
                        self.main_content(
                            value,
                            id,
                            ExecutionEvent::ToolStarted {
                                id: tool_id.to_owned(),
                                name: name.to_owned(),
                                input: input.clone(),
                            },
                            events,
                        );
                    }
                    self.invocation(tool_id, name, &input, &subject, events);
                }
                Some("tool_result") if !assistant => {
                    let Some(tool_id) = string(block, "tool_use_id") else {
                        continue;
                    };
                    let output = match block.get("content") {
                        Some(Value::String(text)) => text.clone(),
                        Some(other) => other.to_string(),
                        None => String::new(),
                    };
                    let is_error = block["is_error"].as_bool().unwrap_or(false);
                    let structured = structured_result;
                    let result = wire::parse_tool_result(structured);
                    self.requests.retain(|_, (owner, gated)| {
                        gated != tool_id || owner.as_ref() != Some(&subject)
                    });
                    if let Some(key) = &child {
                        self.working(key, events);
                        self.content_message(
                            key,
                            value,
                            id,
                            ExecutionEvent::ToolCompleted {
                                id: tool_id.to_owned(),
                                output,
                                is_error,
                                result,
                            },
                            events,
                        );
                    } else {
                        self.main_content(
                            value,
                            id,
                            ExecutionEvent::ToolCompleted {
                                id: tool_id.to_owned(),
                                output,
                                is_error,
                                result,
                            },
                            events,
                        );
                    }
                    self.completed_invocation(tool_id, structured, is_error, events);
                    if let Some(owners) = self.tools.get_mut(tool_id) {
                        owners.retain(|owner| *owner != subject);
                        if owners.is_empty() {
                            self.tools.remove(tool_id);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn main_usage(&mut self, value: &Value) -> Option<SessionEvent> {
        wire::parse_usage_value(value).map(|usage| self.record_main_usage(value, usage))
    }

    fn record_main_usage(&mut self, value: &Value, usage: SessionEvent) -> SessionEvent {
        let SessionEvent::TokenUsage {
            mut total_tokens,
            input_tokens,
            cached_input_tokens,
            output_tokens,
            reasoning_output_tokens,
            context_window,
        } = usage
        else {
            return usage;
        };
        let has_occupancy = value["type"] == "assistant"
            || value["usage"]["iterations"]
                .as_array()
                .is_some_and(|iterations| !iterations.is_empty());
        if has_occupancy {
            self.usage.occupancy = Some(total_tokens);
        } else if let Some(occupancy) = self.usage.occupancy {
            total_tokens = occupancy;
        }
        if let Some(window) = context_window {
            self.usage.context_window = Some(window);
        }
        SessionEvent::TokenUsage {
            total_tokens,
            input_tokens,
            cached_input_tokens,
            output_tokens,
            reasoning_output_tokens,
            context_window: self.usage.context_window,
        }
    }

    /// Claude stream frames name a message once, then each delta carries only
    /// its native block index. Retain that proven pair so the later completed
    /// block can replace the live stream. A full frame with no observed stream
    /// still keeps its outer delivery UUID.
    fn main_stream(&mut self, value: &Value, events: &mut Vec<SessionEvent>) {
        match value["event"]["type"].as_str() {
            Some("message_start") => {
                self.main_stream_message = value["event"]["message"]["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned);
            }
            Some("content_block_start") => {
                let Some((id, kind)) = self.register_main_stream_block(value) else {
                    events.extend(wire::parse_events_value(value));
                    return;
                };
                let block = &value["event"]["content_block"];
                let text = match kind {
                    MainStreamKind::Text => block["text"].as_str(),
                    MainStreamKind::Thinking => block["thinking"].as_str(),
                };
                if let Some(text) = text.filter(|text| !text.is_empty()) {
                    push(
                        events,
                        ActivityEvent::MainContent {
                            id: Some(id),
                            event: match kind {
                                MainStreamKind::Text => {
                                    ExecutionEvent::TextDelta { text: text.into() }
                                }
                                MainStreamKind::Thinking => {
                                    ExecutionEvent::ThinkingDelta { text: text.into() }
                                }
                            },
                        },
                    );
                }
            }
            Some("content_block_stop") => {}
            Some("message_stop") => self.main_stream_message = None,
            Some("content_block_delta") => {
                let Some(id) = self.main_stream_block_id(value) else {
                    events.extend(wire::parse_events_value(value));
                    return;
                };
                for event in wire::parse_events_value(value) {
                    match event {
                        SessionEvent::TextDelta { text } => push(
                            events,
                            ActivityEvent::MainContent {
                                id: Some(id.clone()),
                                event: ExecutionEvent::TextDelta { text },
                            },
                        ),
                        SessionEvent::ThinkingDelta { text } => push(
                            events,
                            ActivityEvent::MainContent {
                                id: Some(id.clone()),
                                event: ExecutionEvent::ThinkingDelta { text },
                            },
                        ),
                        event => events.push(event),
                    }
                }
                return;
            }
            _ => {}
        }
        events.extend(wire::parse_events_value(value));
    }

    fn main_content(
        &mut self,
        value: &Value,
        id: Option<String>,
        event: ExecutionEvent,
        events: &mut Vec<SessionEvent>,
    ) {
        if let Some(id) = &id {
            self.record_delivery(&Subject::Main, value, id, events);
        }
        push(events, ActivityEvent::MainContent { id, event });
    }

    fn retract_subject(
        &mut self,
        subject: &Subject,
        value: &Value,
        events: &mut Vec<SessionEvent>,
    ) {
        let retracted = value["supersedes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str);
        self.retract_uuids(subject, retracted, events);
    }

    fn retract_uuids<'a>(
        &mut self,
        subject: &Subject,
        retracted: impl Iterator<Item = &'a str>,
        events: &mut Vec<SessionEvent>,
    ) {
        let mut ids = Vec::new();
        for uuid in retracted {
            let delivery = (subject.clone(), uuid.into());
            if let Some(known) = self.deliveries.get(&delivery) {
                ids.extend(known.iter().cloned());
            } else if self.pending_retractions.len() < 8192 {
                self.pending_retractions.insert(delivery);
            }
        }
        if !ids.is_empty() {
            self.retract_content(subject, ids, events);
        }
    }

    fn record_delivery(
        &mut self,
        subject: &Subject,
        value: &Value,
        id: &str,
        events: &mut Vec<SessionEvent>,
    ) {
        let Some(uuid) = string(value, "uuid") else {
            return;
        };
        let key = (subject.clone(), uuid.into());
        let delivery = self.deliveries.entry(key.clone()).or_default();
        if !delivery.iter().any(|known| known == id) {
            delivery.push(id.into());
        }
        if delivery.len() == 1 {
            self.delivery_order.push_back(key.clone());
            if self.delivery_order.len() > 8192 {
                if let Some(old) = self.delivery_order.pop_front() {
                    self.deliveries.remove(&old);
                }
            }
        }
        if self.pending_retractions.contains(&key) {
            self.retract_content(subject, vec![id.into()], events);
        }
    }

    fn retract_content(
        &self,
        subject: &Subject,
        ids: Vec<String>,
        events: &mut Vec<SessionEvent>,
    ) {
        let event = ExecutionEvent::Retract { ids };
        match subject {
            Subject::Main => push(events, ActivityEvent::MainContent { id: None, event }),
            Subject::Subagent(key) => push(
                events,
                ActivityEvent::Content {
                    key: key.clone(),
                    id: None,
                    event,
                },
            ),
        }
    }

    fn register_main_stream_block(&mut self, value: &Value) -> Option<(String, MainStreamKind)> {
        let message = self.main_stream_message.as_deref()?;
        let index = value["event"]["index"].as_u64()?;
        let kind = match value["event"]["content_block"]["type"].as_str()? {
            "text" => MainStreamKind::Text,
            "thinking" => MainStreamKind::Thinking,
            _ => return None,
        };
        let key = (message.to_owned(), index);
        if !self.main_stream_blocks.contains_key(&key) {
            self.main_stream_blocks.insert(
                key.clone(),
                MainStreamBlock {
                    message: message.into(),
                    index,
                    kind,
                    delivery: None,
                },
            );
            self.main_stream_order.push_back(key);
            if self.main_stream_order.len() > 8192 {
                if let Some(old) = self.main_stream_order.pop_front() {
                    self.main_stream_blocks.remove(&old);
                }
            }
        }
        Some((main_block_id(message, index), kind))
    }

    fn main_stream_block_id(&self, value: &Value) -> Option<String> {
        let message = self.main_stream_message.as_deref()?;
        let index = value["event"]["index"].as_u64()?;
        self.main_stream_blocks
            .contains_key(&(message.to_owned(), index))
            .then(|| main_block_id(message, index))
    }

    fn bind_main_stream_block(&mut self, value: &Value, kind: MainStreamKind) -> Option<String> {
        let message = value["message"]["id"]
            .as_str()
            .filter(|id| !id.is_empty())?;
        let candidates: Vec<_> = self
            .main_stream_blocks
            .values()
            .filter(|block| {
                block.message == message && block.kind == kind && block.delivery.is_none()
            })
            .map(|block| (block.message.clone(), block.index))
            .collect();
        let [key] = candidates.as_slice() else {
            return None;
        };
        let block = self.main_stream_blocks.get_mut(key)?;
        block.delivery = string(value, "uuid").map(str::to_owned);
        Some(main_block_id(&block.message, block.index))
    }

    fn seen_main_frame(&mut self, value: &Value) -> bool {
        let Some(uuid) = string(value, "uuid") else {
            return false;
        };
        if !self.seen_main_frames.insert(uuid.into()) {
            return true;
        }
        self.main_frame_order.push_back(uuid.into());
        if self.main_frame_order.len() > 8192 {
            if let Some(old) = self.main_frame_order.pop_front() {
                self.seen_main_frames.remove(&old);
            }
        }
        false
    }

    fn invocation(
        &mut self,
        tool: &str,
        name: &str,
        input: &Value,
        owner: &Subject,
        events: &mut Vec<SessionEvent>,
    ) {
        if matches!(name, "Agent" | "Task") {
            let resumed = string(input, "resume");
            let key = if let Some(id) = resumed {
                self.aliases
                    .get(&Alias::Agent(id.to_owned()))
                    .cloned()
                    .unwrap_or_else(|| self.native_key(&format!("agent:{id}")))
            } else {
                self.native_key(tool)
            };
            self.link(Alias::Invocation(tool.to_owned()), &key, events);
            if let Some(id) = resumed {
                self.link(Alias::Agent(id.to_owned()), &key, events);
            }
            if resumed.is_none() {
                self.originals.insert(key.clone());
            }
            let mut info = self.info(&key);
            if resumed.is_none() {
                info.parent = Some(owner.clone());
            }
            info.name = string(input, "name").map(str::to_owned).or(info.name);
            info.kind = string(input, "subagent_type")
                .map(str::to_owned)
                .or(info.kind);
            info.description = string(input, "description")
                .map(str::to_owned)
                .or(info.description);
            if info.coverage == TranscriptCoverage::Unavailable {
                info.coverage = TranscriptCoverage::ToolActivity;
            }
            self.discover(info, events);
            self.status(&key, AgentStatus::Pending, events);
        } else if name == "SendMessage" {
            if let Some(target) = string(input, "to") {
                // SendMessage also addresses named teammates. Only an agent ID
                // already joined by provider evidence identifies a child here.
                if let Some(key) = self.aliases.get(&Alias::Agent(target.to_owned())).cloned() {
                    self.link(Alias::Invocation(tool.to_owned()), &key, events);
                }
            }
        }
    }

    fn completed_invocation(
        &mut self,
        tool: &str,
        result: Option<&Value>,
        failed: bool,
        events: &mut Vec<SessionEvent>,
    ) {
        let mut key = self
            .aliases
            .get(&Alias::Invocation(tool.to_owned()))
            .cloned();
        if let Some(result) = result {
            if let Some(agent) =
                string(result, "agentId").or_else(|| string(result, "resumedAgentId"))
            {
                let known = self.aliases.get(&Alias::Agent(agent.to_owned())).cloned();
                let canonical = key
                    .as_ref()
                    .filter(|key| self.originals.contains(*key))
                    .cloned()
                    .or_else(|| {
                        known
                            .as_ref()
                            .filter(|key| self.originals.contains(*key))
                            .cloned()
                    })
                    .or(known)
                    .unwrap_or_else(|| self.native_key(&format!("agent:{agent}")));
                self.link(Alias::Invocation(tool.to_owned()), &canonical, events);
                self.link(Alias::Agent(agent.to_owned()), &canonical, events);
                key = Some(canonical);
            }
        }
        let Some(key) = key else {
            return;
        };
        let mut info = self.info(&key);
        if let Some(result) = result {
            info.kind = string(result, "agentType").map(str::to_owned).or(info.kind);
            info.description = string(result, "description")
                .map(str::to_owned)
                .or(info.description);
        }
        self.discover(info, events);
        if failed {
            self.status(&key, AgentStatus::Failed, events);
        } else if let Some(state) = result.and_then(|r| string(r, "status")).map(status) {
            self.status(&key, state, events);
        }
    }

    fn notice(&mut self, value: &Value, text: Option<&str>, events: &mut Vec<SessionEvent>) {
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            return;
        };
        match self.scope(value, events) {
            Some(Subject::Main) => push(
                events,
                ActivityEvent::MainContent {
                    id: delivery_id(value, "notice"),
                    event: ExecutionEvent::Notice { text: text.into() },
                },
            ),
            Some(Subject::Subagent(key)) => self.content(
                &key,
                delivery_id(value, "notice"),
                ExecutionEvent::Notice { text: text.into() },
                events,
            ),
            None => {}
        }
    }

    fn task(&mut self, value: &Value, events: &mut Vec<SessionEvent>) {
        let subtype = string(value, "subtype").unwrap_or("");
        if !matches!(
            subtype,
            "task_started" | "task_progress" | "task_updated" | "task_notification"
        ) {
            return;
        }
        if self.seen(value) {
            return;
        }
        let Some(task_id) = string(value, "task_id") else {
            return;
        };
        if value["ambient"] == true || value["skip_transcript"] == true {
            self.excluded_tasks.insert(task_id.into());
            return;
        }
        if self.excluded_tasks.contains(task_id) {
            return;
        }
        // Bash jobs have task IDs too. Classification or an existing agent
        // alias is necessary; owned_by_subagent means a child tool, not a child.
        if string(value, "task_type").is_some_and(|kind| kind != "local_agent") {
            self.progress(value, events);
            return;
        }
        let invocation = string(value, "tool_use_id");
        let known_task = self.aliases.get(&Alias::Task(task_id.to_owned())).cloned();
        let known_invocation =
            invocation.and_then(|id| self.aliases.get(&Alias::Invocation(id.to_owned())).cloned());
        let key = match known_task.or(known_invocation) {
            Some(key) => key,
            None if string(value, "task_type") == Some("local_agent") => invocation
                .and_then(|id| self.aliases.get(&Alias::Invocation(id.to_owned())).cloned())
                .unwrap_or_else(|| self.native_key(&format!("task:{task_id}"))),
            None => {
                self.progress(value, events);
                return;
            }
        };
        self.link(Alias::Task(task_id.to_owned()), &key, events);
        if let Some(id) = invocation {
            self.link(Alias::Invocation(id.to_owned()), &key, events);
        }
        let mut info = self.info(&key);
        info.kind = string(value, "subagent_type")
            .map(str::to_owned)
            .or(info.kind);
        // Progress descriptions describe the current tool; they must not rename
        // the tab's durable task description on every tool call.
        if subtype == "task_started" {
            info.description = string(value, "description")
                .map(str::to_owned)
                .or(info.description);
        }
        self.discover(info, events);
        if matches!(subtype, "task_started" | "task_progress") && !self.waiting(&key) {
            let detail = string(value, "summary")
                .or_else(|| string(value, "description"))
                .or_else(|| string(value, "last_tool_name"))
                .unwrap_or("");
            self.content(
                &key,
                None,
                ExecutionEvent::Progress {
                    event: crate::progress::ProgressEvent::Phase {
                        phase: crate::progress::Phase::Working,
                        detail: detail.into(),
                    },
                },
                events,
            );
        }
        let state = match subtype {
            "task_started" => Some(AgentStatus::Working),
            "task_progress" => (!self.waiting(&key)).then_some(AgentStatus::Working),
            "task_updated" => string(&value["patch"], "status").map(status),
            "task_notification" => string(value, "status").map(status),
            _ => None,
        };
        if let Some(state) = state {
            self.status(&key, state, events);
            if subtype == "task_notification"
                && matches!(state, AgentStatus::Failed | AgentStatus::Interrupted)
            {
                if let Some(summary) = string(value, "summary") {
                    self.content(
                        &key,
                        delivery_id(value, "outcome"),
                        ExecutionEvent::Notice {
                            text: summary.to_owned(),
                        },
                        events,
                    );
                }
            }
        }
    }

    fn decision(&mut self, value: &Value, events: &mut Vec<SessionEvent>) {
        let Some(SessionEvent::DecisionRequested { decision }) = wire::parse_value(value) else {
            return;
        };
        let request = &value["request"];
        let malformed_agent = !matches!(request.get("agent_id"), None | Some(Value::Null))
            && string(request, "agent_id").is_none();
        let subject = if self.root.is_empty() || malformed_agent {
            None
        } else if let Some(agent) = string(request, "agent_id") {
            if let Some(key) = self.aliases.get(&Alias::Agent(agent.to_owned())).cloned() {
                Some(Subject::Subagent(key))
            } else if let Some(Subject::Subagent(key)) = self.tool_owner(&decision.tool_use_id) {
                self.link(Alias::Agent(agent.to_owned()), &key, events);
                Some(Subject::Subagent(key))
            } else {
                // An explicit agent ID is useful identity even when discovery
                // is late. A later join redirects this provisional key.
                let key = self.native_key(&format!("agent:{agent}"));
                self.link(Alias::Agent(agent.to_owned()), &key, events);
                self.discover(self.info(&key), events);
                Some(Subject::Subagent(key))
            }
        } else {
            self.tool_owner(&decision.tool_use_id)
        };
        self.requests.insert(
            decision.id.clone(),
            (subject.clone(), decision.tool_use_id.clone()),
        );
        if let Some(Subject::Subagent(key)) = &subject {
            self.status(key, AgentStatus::Waiting, events);
        }
        if subject == Some(Subject::Main) {
            events.push(SessionEvent::DecisionRequested { decision });
        } else {
            push(events, ActivityEvent::Decision { subject, decision });
        }
    }
}

fn string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

fn delivery_id(value: &Value, part: &str) -> Option<String> {
    string(value, "uuid").map(|uuid| format!("{uuid}:{part}"))
}

fn main_block_id(message: &str, index: u64) -> String {
    format!("stream:{message}:{index}")
}

fn push(events: &mut Vec<SessionEvent>, event: ActivityEvent) {
    events.push(SessionEvent::Activity(event));
}

fn status(value: &str) -> AgentStatus {
    match value {
        "pending" => AgentStatus::Pending,
        "running" | "async_launched" => AgentStatus::Working,
        "completed" => AgentStatus::Idle,
        "failed" => AgentStatus::Failed,
        "stopped" | "killed" => AgentStatus::Interrupted,
        "paused" => AgentStatus::Paused,
        _ => AgentStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{Activity, ActivityInput, ActivityLimits};
    use serde_json::json;
    use std::time::Instant;

    const OVERLAP: &str =
        include_str!("../../../tests/fixtures/subagents/claude-overlap-2.1.261.jsonl");
    const NESTED: &str =
        include_str!("../../../tests/fixtures/subagents/claude-nested-2.1.261.jsonl");
    const REUSED: &str =
        include_str!("../../../tests/fixtures/subagents/claude-reuse-2.1.261.jsonl");
    const DECISIONS: &str =
        include_str!("../../../tests/fixtures/subagents/claude-decisions-2.1.261.jsonl");

    fn replay(fixture: &str) -> (Decoder, Vec<SessionEvent>) {
        let mut decoder = Decoder::default();
        let events = fixture
            .lines()
            .flat_map(|line| decoder.decode(line))
            .collect();
        (decoder, events)
    }

    #[test]
    fn existing_main_captures_keep_their_event_and_usage_projection() {
        for fixture in [
            include_str!("../../../tests/fixtures/claude-hello-2.1.243.jsonl"),
            include_str!("../../../tests/fixtures/claude-tool-2.1.243.jsonl"),
            include_str!("../../../tests/fixtures/claude-permission-allow-2.1.243.jsonl"),
            include_str!("../../../tests/fixtures/claude-permission-deny-2.1.243.jsonl"),
            include_str!("../../../tests/fixtures/claude-edit-2.1.243.jsonl"),
            include_str!("../../../tests/fixtures/claude-todo-2.1.243.jsonl"),
        ] {
            let old: Vec<_> = fixture
                .lines()
                .flat_map(|line| {
                    wire::parse_usage(line)
                        .into_iter()
                        .chain(wire::parse_rate_limits(line))
                        .chain(wire::parse_line(line))
                })
                .collect();
            let (_, new) = replay(fixture);
            // Added native metadata does not change the existing prose,
            // tool, usage, approval, or turn-result projection.
            let old_projection: Vec<_> = new
                .into_iter()
                .filter(|event| {
                    !matches!(
                        event,
                        SessionEvent::Progress { .. } | SessionEvent::ContentBoundary
                    )
                })
                .collect();
            assert_eq!(old_projection, old);
        }
    }

    #[test]
    fn every_child_frame_is_scoped_before_tools_or_usage_can_reach_main() {
        let mut decoder = Decoder::default();
        let mut child_frames = 0;
        for line in OVERLAP.lines() {
            let value: Value = serde_json::from_str(line).unwrap();
            let events = decoder.decode(line);
            if string(&value, "parent_tool_use_id").is_some() {
                child_frames += 1;
                assert!(!events.is_empty(), "child frame was discarded: {line}");
                assert!(
                    events
                        .iter()
                        .all(|event| matches!(event, SessionEvent::Activity(_))),
                    "unscoped child events: {events:?}"
                );
            }
        }
        assert!(child_frames >= 8);
        assert_eq!(decoder.agents.len(), 2, "Bash tasks are not children");
        assert!(decoder
            .states
            .values()
            .all(|state| *state == AgentStatus::Idle));
        assert!(decoder.tools.is_empty());
    }

    #[test]
    fn nested_tool_invocation_preserves_the_direct_parent() {
        let (decoder, _) = replay(NESTED);
        let branch = decoder.native_key("toolu_01Gr7QcnKEMTUUZn1JABxzq1");
        let leaf = decoder.native_key("toolu_01Xta8pMN844BJKVcx3W5CLG");
        assert_eq!(decoder.agents[&branch].parent, Some(Subject::Main));
        assert_eq!(
            decoder.agents[&leaf].parent,
            Some(Subject::Subagent(branch))
        );
        assert_eq!(decoder.agents[&leaf].coverage, TranscriptCoverage::Live);
    }

    #[test]
    fn resumed_content_and_new_task_invocation_keep_the_original_key() {
        let (decoder, events) = replay(REUSED);
        let original = decoder.native_key("toolu_01KaThct3sAoj92tCAJPZsDV");
        assert_eq!(decoder.agents.len(), 1);
        for alias in [
            Alias::Invocation("toolu_01KaThct3sAoj92tCAJPZsDV".into()),
            Alias::Invocation("toolu_01WWmgV4yDDVfNmxECCq6ow3".into()),
            Alias::Task("a5b6de4f9ee3a5060".into()),
            Alias::Agent("a5b6de4f9ee3a5060".into()),
        ] {
            assert_eq!(decoder.aliases[&alias], original);
        }
        let content: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::Activity(ActivityEvent::Content {
                    key,
                    event: ExecutionEvent::Text { text },
                    ..
                }) => Some((key, text.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            content,
            [(&original, "REUSE_FIRST"), (&original, "REUSE_SECOND")]
        );
        let states: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::Activity(ActivityEvent::Status { state, .. }) => Some(*state),
                _ => None,
            })
            .collect();
        assert_eq!(
            states,
            [
                AgentStatus::Pending,
                AgentStatus::Working,
                AgentStatus::Idle,
                AgentStatus::Working,
                AgentStatus::Idle
            ]
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(
                    event,
                    SessionEvent::Activity(ActivityEvent::BackgroundTurnEnded { .. })
                ))
                .count(),
            1
        );
    }

    #[test]
    fn persisted_native_aliases_resolve_a_fresh_decoder_back_to_the_original_tab() {
        // Replay through the first completed Main turn, then reconnect. Keep
        // the boundary semantic so trimming unrelated wire chatter is harmless.
        let first_turn_end = REUSED
            .lines()
            .position(|line| serde_json::from_str::<Value>(line).unwrap()["type"] == "result")
            .expect("fixture contains the first Main result")
            + 1;
        let initial = REUSED
            .lines()
            .take(first_turn_end)
            .collect::<Vec<_>>()
            .join("\n");
        let (first_decoder, initial_events) = replay(&initial);
        let original = first_decoder.native_key("toolu_01KaThct3sAoj92tCAJPZsDV");
        let mut restored = Activity::new(ActivityLimits::default());
        for event in initial_events {
            if let SessionEvent::Activity(event) = event {
                restored.apply(ActivityInput::ReplayEvent(event));
            }
        }
        restored.apply(ActivityInput::Connect { generation: 2 });
        let mut resumed_decoder = Decoder::default();
        for line in REUSED.lines().skip(first_turn_end) {
            for event in resumed_decoder.decode(line) {
                if let SessionEvent::Activity(event) = event {
                    restored.apply(ActivityInput::Observe {
                        generation: 2,
                        event,
                        at: Instant::now(),
                    });
                }
            }
        }
        let children = restored.view().children();
        assert_eq!(children.len(), 1, "reconnect created a duplicate tab");
        assert_eq!(children[0].key(), &original);
        assert_eq!(children[0].status(), AgentStatus::Idle);
        assert_eq!(children[0].info().parent, Some(Subject::Main));
    }

    #[test]
    fn complete_block_identity_and_duplicates_do_not_restart_settled_children() {
        let (mut decoder, _) = replay(OVERLAP);
        let child_line = OVERLAP
            .lines()
            .find(|line| {
                let value: Value = serde_json::from_str(line).unwrap();
                value["type"] == "assistant"
                    && string(&value, "parent_tool_use_id").is_some()
                    && value["message"]["content"][0]["text"] == "ALPHA_START"
            })
            .unwrap();
        assert!(decoder.decode(child_line).is_empty());
        assert!(decoder
            .states
            .values()
            .all(|state| *state == AgentStatus::Idle));
        let events = decoder.decode(&json!({
            "type":"assistant", "session_id":decoder.root, "parent_tool_use_id":"toolu_01E3tZojsnhrYd88HXfBMon9", "uuid":"multi-block",
            "message":{"id":"same-message", "content":[{"type":"text","text":"first"},{"type":"thinking","thinking":"second"},{"type":"tool_use","id":"third","name":"Read","input":{}}]}
        }).to_string());
        let ids: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::Activity(ActivityEvent::Content { id, .. }) => id.clone(),
                _ => None,
            })
            .collect();
        assert_eq!(ids, ["multi-block:0", "multi-block:1", "multi-block:2"]);
    }

    #[test]
    fn captured_decisions_are_two_independent_child_routes() {
        let (_, events) = replay(DECISIONS);
        let decisions: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::Activity(ActivityEvent::Decision {
                    subject: Some(Subject::Subagent(key)),
                    decision,
                }) => Some((key, decision)),
                _ => None,
            })
            .collect();
        assert_eq!(decisions.len(), 2);
        assert_ne!(decisions[0].0, decisions[1].0);
        assert_ne!(decisions[0].1.id, decisions[1].1.id);
        assert!(events
            .iter()
            .all(|event| !matches!(event, SessionEvent::DecisionRequested { .. })));
    }

    #[test]
    fn unknown_decision_and_its_cancellation_remain_connection_scoped() {
        let mut decoder = Decoder::default();
        let events = decoder.decode(r#"{"type":"control_request","request_id":"unknown","request":{"subtype":"can_use_tool","tool_use_id":"missing"}}"#);
        assert!(
            matches!(events.as_slice(), [SessionEvent::Activity(ActivityEvent::Decision { subject: None, decision })] if decision.id == "unknown")
        );
        let cancelled =
            decoder.decode(r#"{"type":"control_cancel_request","request_id":"unknown"}"#);
        assert_eq!(
            cancelled,
            [SessionEvent::Activity(ActivityEvent::DecisionCancelled {
                id: "unknown".into()
            })]
        );
        assert!(decoder.requests.is_empty());
    }

    #[test]
    fn answered_request_does_not_keep_a_child_waiting_after_its_other_request_is_cancelled() {
        let mut decoder = Decoder::default();
        decoder.decode(
            &json!({"type":"assistant", "session_id":"root", "parent_tool_use_id":"child",
            "message":{"content":[{"type":"tool_use","id":"tool-a","name":"Read","input":{}},
                {"type":"tool_use","id":"tool-b","name":"Read","input":{}}]}})
            .to_string(),
        );
        decoder.decode(r#"{"type":"system","subtype":"task_started","task_id":"task","tool_use_id":"child","task_type":"local_agent"}"#);
        for suffix in ["a", "b"] {
            decoder.decode(&json!({"type":"control_request", "request_id":suffix, "request":{
                "subtype":"can_use_tool", "tool_use_id":format!("tool-{suffix}"), "tool_name":"Read"}}).to_string());
        }
        decoder.decision_resolved("a");
        let cancelled = decoder.decode(r#"{"type":"control_cancel_request","request_id":"b"}"#);
        assert!(cancelled.iter().any(|event| matches!(
            event,
            SessionEvent::Activity(ActivityEvent::Status {
                state: AgentStatus::Unknown,
                ..
            })
        )));
        // No tool result has arrived. The next actual progress still resumes it.
        let progress =
            decoder.decode(r#"{"type":"system","subtype":"task_progress","task_id":"task"}"#);
        assert!(progress.iter().any(|event| matches!(
            event,
            SessionEvent::Activity(ActivityEvent::Status {
                state: AgentStatus::Working,
                ..
            })
        )));
        assert!(decoder.requests.is_empty());
    }

    #[test]
    fn ambiguous_tool_ids_do_not_guess_a_decision_owner_or_complete_another_childs_tool() {
        let mut decoder = Decoder::default();
        for child in ["alpha", "beta"] {
            decoder.decode(&json!({"type":"assistant", "session_id":"root", "parent_tool_use_id":child,
                "message":{"content":[{"type":"tool_use","id":"shared-tool","name":"Read","input":{}}]}}).to_string());
        }
        let request = json!({"type":"control_request", "request_id":"ambiguous", "request":{
            "subtype":"can_use_tool", "tool_use_id":"shared-tool"}});
        assert!(matches!(
            decoder.decode(&request.to_string()).as_slice(),
            [SessionEvent::Activity(ActivityEvent::Decision {
                subject: None,
                ..
            })]
        ));
        decoder.decode(&json!({"type":"user", "session_id":"root", "parent_tool_use_id":"alpha",
            "message":{"content":[{"type":"tool_result","tool_use_id":"shared-tool","content":"done"}]}}).to_string());
        assert_eq!(
            decoder.tool_owner("shared-tool"),
            Some(Subject::Subagent(decoder.native_key("beta")))
        );
        assert!(decoder.requests.contains_key("ambiguous"));
    }

    #[test]
    fn child_string_prompt_is_preserved_without_becoming_main_input() {
        let mut decoder = Decoder::default();
        let events = decoder.decode(r#"{"type":"user","session_id":"root","parent_tool_use_id":"child","uuid":"prompt-frame","message":{"content":"child prompt"}}"#);
        assert!(events.iter().any(|event| matches!(event,
            SessionEvent::Activity(ActivityEvent::Content { id: Some(id), event: ExecutionEvent::Prompt { text }, .. })
            if id == "prompt-frame:0" && text == "child prompt")));
        assert!(events
            .iter()
            .all(|event| matches!(event, SessionEvent::Activity(_))));
    }

    #[test]
    fn unknown_task_status_is_not_false_completion_and_bash_is_not_an_agent() {
        let mut decoder = Decoder::default();
        decoder.decode(r#"{"type":"system","subtype":"init","session_id":"root","model":"model"}"#);
        decoder.decode(r#"{"type":"system","subtype":"task_started","task_id":"agent","tool_use_id":"spawn","task_type":"local_agent"}"#);
        let events = decoder.decode(r#"{"type":"system","subtype":"task_updated","task_id":"agent","patch":{"status":"future-status"}}"#);
        assert!(matches!(
            events.as_slice(),
            [SessionEvent::Activity(ActivityEvent::Status {
                state: AgentStatus::Unknown,
                ..
            })]
        ));
        assert!(decoder.decode(r#"{"type":"system","subtype":"task_started","task_id":"bash","task_type":"local_bash","owned_by_subagent":true}"#).is_empty());
        assert_eq!(decoder.agents.len(), 1);
    }

    #[test]
    fn attributed_or_malformed_result_cannot_end_main_or_report_main_usage() {
        let mut decoder = Decoder::default();
        for attribution in [json!("child-spawn"), json!(""), json!(false), json!({})] {
            let frame = json!({"type":"result", "parent_tool_use_id":attribution,
                "session_id":"root", "result":"done", "usage":{"input_tokens":10}});
            assert!(decoder.decode(&frame.to_string()).is_empty());
        }
    }

    #[test]
    fn schema_non_human_and_unknown_origins_cannot_acknowledge_an_operator_turn() {
        // Published SDK 0.3.224/.243/.261 origin union, plus unknown/malformed
        // future producers. Only task-notification is in our live captures.
        for origin in [
            json!({"kind":"channel"}),
            json!({"kind":"peer","senderTaskId":"child"}),
            json!({"kind":"task-notification"}),
            json!({"kind":"coordinator"}),
            json!({"kind":"unclassified"}),
            json!({"kind":"observer"}),
            json!({"kind":"auto-continuation"}),
            json!({"kind":"observer-activity"}),
            json!({"kind":"future-origin"}),
            json!({}),
            json!(false),
        ] {
            let mut decoder = Decoder::default();
            let events = decoder.decode(
                &json!({"type":"result", "subtype":"success",
                "session_id":"root", "origin":origin})
                .to_string(),
            );
            assert!(events.iter().any(|event| matches!(
                event,
                SessionEvent::Activity(ActivityEvent::BackgroundTurnEnded { .. })
            )));
            assert!(events
                .iter()
                .all(|event| !matches!(event, SessionEvent::TurnEnded { .. })));
        }
        for origin in [json!(null), json!({"kind":"human"})] {
            let mut decoder = Decoder::default();
            let events = decoder.decode(
                &json!({"type":"result", "subtype":"success",
                "session_id":"root", "origin":origin})
                .to_string(),
            );
            assert!(events
                .iter()
                .any(|event| matches!(event, SessionEvent::TurnEnded { .. })));
        }
    }
}
