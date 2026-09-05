//! Provider-reported work, shared by every semantic zoom. No inference,
//! timers or provider calls: this is a bounded projection of native events.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Working,
    Thinking,
    Answering,
    Compacting,
    Retrying,
    Waiting,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Working => "Working",
            Self::Thinking => "Thinking",
            Self::Answering => "Answering",
            Self::Compacting => "Compacting context",
            Self::Retrying => "Retrying",
            Self::Waiting => "Waiting",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    pub text: String,
    pub status: StepStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Working,
    Completed,
    Failed,
    Stopped,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressEvent {
    Phase {
        phase: Phase,
        detail: String,
    },
    Tool {
        id: String,
        message: String,
        elapsed_ms: Option<u64>,
    },
    Plan {
        steps: Vec<PlanStep>,
        explanation: String,
    },
    Task {
        id: String,
        subject: String,
        status: Option<StepStatus>,
        deleted: bool,
    },
    Background {
        id: String,
        label: String,
        status: TaskStatus,
        detail: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolProgress {
    pub message: String,
    pub elapsed_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundTask {
    pub id: String,
    pub label: String,
    pub status: TaskStatus,
    pub detail: String,
}

/// Limits apply to the rendered projection, not the durable native event log.
const MAX_ENTRIES: usize = 128;
const MAX_TEXT: usize = 512;

#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub phase: Option<Phase>,
    pub detail: String,
    pub plan: Vec<PlanStep>,
    pub explanation: String,
    pub has_plan: bool,
    summary: String,
    tools: BTreeMap<String, ToolProgress>,
    latest_tool: Option<String>,
    task_ids: Vec<String>,
    tasks: Vec<BackgroundTask>,
}

impl Progress {
    pub fn apply(&mut self, event: &ProgressEvent) {
        match event {
            ProgressEvent::Phase { phase, detail } => {
                self.phase = Some(*phase);
                self.detail = one_line(detail, MAX_TEXT);
            }
            ProgressEvent::Tool {
                id,
                message,
                elapsed_ms,
            } => {
                if self.tools.len() < MAX_ENTRIES || self.tools.contains_key(id) {
                    self.latest_tool = Some(id.clone());
                    let tool = self.tools.entry(id.clone()).or_default();
                    if !message.is_empty() {
                        tool.message = one_line(message, MAX_TEXT);
                    }
                    if let Some(elapsed) = elapsed_ms {
                        tool.elapsed_ms = Some(tool.elapsed_ms.unwrap_or(0).max(*elapsed));
                    }
                }
                self.phase = Some(Phase::Working);
                self.detail.clear();
            }
            ProgressEvent::Plan { steps, explanation } => {
                self.has_plan = true;
                self.task_ids.clear();
                self.plan = steps
                    .iter()
                    .take(MAX_ENTRIES)
                    .map(|step| PlanStep {
                        text: one_line(&step.text, MAX_TEXT),
                        status: step.status,
                    })
                    .collect();
                self.explanation = one_line(explanation, MAX_TEXT);
            }
            ProgressEvent::Task {
                id,
                subject,
                status,
                deleted,
            } => {
                self.has_plan = true;
                if let Some(at) = self.task_ids.iter().position(|key| key == id) {
                    if *deleted {
                        self.task_ids.remove(at);
                        self.plan.remove(at);
                    } else {
                        if !subject.is_empty() {
                            self.plan[at].text = one_line(subject, MAX_TEXT);
                        }
                        if let Some(status) = status {
                            self.plan[at].status = *status;
                        }
                    }
                } else if !deleted && !subject.is_empty() && self.plan.len() < MAX_ENTRIES {
                    // The first native task receipt supersedes any plan snapshot.
                    if self.task_ids.is_empty() {
                        self.plan.clear();
                    }
                    self.task_ids.push(id.clone());
                    self.plan.push(PlanStep {
                        text: one_line(subject, MAX_TEXT),
                        status: status.unwrap_or(StepStatus::Pending),
                    });
                }
            }
            ProgressEvent::Background {
                id,
                label,
                status,
                detail,
            } => {
                if let Some(task) = self.tasks.iter_mut().find(|task| task.id == *id) {
                    if !label.is_empty() {
                        task.label = one_line(label, MAX_TEXT);
                    }
                    if !detail.is_empty() {
                        task.detail = one_line(detail, MAX_TEXT);
                    }
                    task.status = *status;
                } else {
                    if self.tasks.len() == MAX_ENTRIES {
                        // Retire settled history first; never evict a live task
                        // to make room for another settled notification.
                        if let Some(at) = self
                            .tasks
                            .iter()
                            .position(|task| task.status != TaskStatus::Working)
                        {
                            self.tasks.remove(at);
                        } else {
                            return;
                        }
                    }
                    self.tasks.push(BackgroundTask {
                        id: id.clone(),
                        label: one_line(label, MAX_TEXT),
                        status: *status,
                        detail: one_line(detail, MAX_TEXT),
                    });
                }
            }
        }
    }

    /// Native summary text, shortened for the always-visible status line.
    /// Markdown emphasis is presentation, not part of the heading's words.
    pub fn summary(&mut self, text: &str) {
        self.summary = headline(text);
    }
    pub fn caption(&self) -> Option<String> {
        let phase = self.phase?;
        if matches!(phase, Phase::Retrying | Phase::Compacting | Phase::Waiting) {
            return Some(if self.detail.is_empty() {
                phase.label().into()
            } else {
                format!("{} · {}", phase.label(), self.detail)
            });
        }
        if !self.summary.is_empty() {
            return Some(self.summary.clone());
        }
        if !self.detail.is_empty() {
            return Some(self.detail.clone());
        }
        self.latest_tool()
            .filter(|tool| !tool.message.is_empty())
            .map(|tool| tool.message.clone())
            .or_else(|| Some(phase.label().into()))
    }

    pub fn phase(&mut self, phase: Phase) {
        self.phase = Some(phase);
        self.detail.clear();
    }

    pub fn finish_tool(&mut self, id: &str) {
        self.tools.remove(id);
        if self.latest_tool.as_deref() == Some(id) {
            self.latest_tool = None;
        }
    }
    pub fn latest_tool(&self) -> Option<&ToolProgress> {
        self.latest_tool.as_ref().and_then(|id| self.tools.get(id))
    }
    pub fn tool(&self, id: &str) -> Option<&ToolProgress> {
        self.tools.get(id)
    }
    pub fn background(&self) -> &[BackgroundTask] {
        &self.tasks
    }
    pub fn working_background(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Working)
            .count()
    }
    pub fn current_step(&self) -> Option<&str> {
        self.plan
            .iter()
            .find(|step| step.status == StepStatus::InProgress)
            .or_else(|| {
                self.plan
                    .iter()
                    .find(|step| step.status == StepStatus::Pending)
            })
            .map(|step| step.text.as_str())
    }
    pub fn end_turn(&mut self) {
        self.phase = None;
        self.detail.clear();
        self.summary.clear();
        self.tools.clear();
        self.latest_tool = None;
        // Background work may outlive Main. Only explicit native task facts
        // or losing the owning Session retires its live indication.
    }
    pub fn disconnected(&mut self) {
        self.end_turn();
        for task in &mut self.tasks {
            if task.status == TaskStatus::Working {
                task.status = TaskStatus::Unknown;
            }
        }
    }
}

/// A status line is one readable line. Bound Unicode by characters, and
/// remove control characters. ANSI CSI/OSC escapes are discarded too.
pub fn one_line(text: &str, max: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    let clean = strip_ansi(text);
    for word in clean.split_whitespace() {
        if !out.is_empty() {
            if count == max {
                out.push('…');
                return out;
            }
            out.push(' ');
            count += 1;
        }
        for c in word.chars().filter(|c| !c.is_control()) {
            if count == max {
                out.push('…');
                return out;
            }
            out.push(c);
            count += 1;
        }
    }
    out
}

/// Only a provider-authored first paragraph/heading is used. No summarizer.
pub fn headline(text: &str) -> String {
    // Codex's CLI uses the first complete bold heading as its live status.
    if let Some((_, rest)) = text.split_once("**") {
        if let Some((heading, _)) = rest.split_once("**") {
            if !heading.trim().is_empty() {
                return one_line(heading, 160);
            }
        }
    }
    let text = text.trim_start().trim_start_matches('#').trim_start();
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    one_line(line.trim_matches(|c| matches!(c, '*' | '_' | '`')), 160)
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(1024));
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' || (c == '\u{1b}' && chars.next() == Some('\\')) {
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl ProgressEvent {
    pub(crate) fn retained_bytes(&self) -> usize {
        match self {
            Self::Phase { detail, .. } => detail.len(),
            Self::Tool { id, message, .. } => id.len() + message.len(),
            Self::Plan { steps, explanation } => {
                explanation.len() + steps.iter().map(|step| step.text.len() + 16).sum::<usize>()
            }
            Self::Task { id, subject, .. } => id.len() + subject.len(),
            Self::Background {
                id, label, detail, ..
            } => id.len() + label.len() + detail.len(),
        }
        .saturating_add(64)
    }
}
