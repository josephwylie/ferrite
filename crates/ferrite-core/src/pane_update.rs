//! What one frame changed for one Pane (ADR 0045), and whether an event
//! changes what a Pane shows. The Engine makes them for a window in its
//! own process; a window's mirror of a Server's Thread makes the same.

use crate::activity::{ActivityEvent, ActivityUpdate, AgentKey, Subject};
use crate::transcript::BlockId;
use crate::{SessionEvent, ThreadId};

/// What one frame changed for one Pane. Threads that produced nothing are
/// absent: a quiet Pane costs the renderer nothing. The Engine names its
/// Thread by id; a Cockpit, which holds several machines, by its key
/// (`cockpit::PaneUpdate`, ADR 0045).
#[derive(Debug, Clone, PartialEq)]
pub struct PaneUpdate<T = ThreadId> {
    pub thread: T,
    /// Exactly the Blocks whose content moved.
    pub dirty: Vec<BlockId>,
    /// Blocks that fell off the far end and no longer exist.
    pub evicted: Vec<BlockId>,
    /// Child changes never enter the Main-only compatibility lists above.
    pub subjects: Vec<SubjectUpdate>,
    pub activity_changed: bool,
    pub redirects: Vec<(AgentKey, AgentKey)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubjectUpdate {
    pub subject: Subject,
    pub dirty: Vec<BlockId>,
    pub evicted: Vec<BlockId>,
    pub content_changed: bool,
}

impl<T> PaneUpdate<T> {
    pub fn new(thread: T) -> Self {
        Self {
            thread,
            dirty: Vec::new(),
            evicted: Vec::new(),
            subjects: Vec::new(),
            activity_changed: false,
            redirects: Vec::new(),
        }
    }

    pub fn absorb(&mut self, applied: ActivityUpdate, content_changed: bool) {
        self.activity_changed |=
            !applied.changed.is_empty() || applied.attention_changed || applied.order_changed;
        self.redirects.extend(applied.redirects);
        for subject in applied.changed {
            if !self.subjects.iter().any(|change| change.subject == subject) {
                self.subjects.push(SubjectUpdate {
                    subject,
                    dirty: Vec::new(),
                    evicted: Vec::new(),
                    content_changed: false,
                });
            }
        }
        for (subject, blocks) in applied.blocks {
            if subject == Subject::Main {
                self.dirty.extend(blocks.dirty.iter().copied());
                self.evicted.extend(blocks.evicted.iter().copied());
            }
            let index = self
                .subjects
                .iter()
                .position(|change| change.subject == subject)
                .unwrap_or_else(|| {
                    self.subjects.push(SubjectUpdate {
                        subject,
                        dirty: Vec::new(),
                        evicted: Vec::new(),
                        content_changed: false,
                    });
                    self.subjects.len() - 1
                });
            let change = &mut self.subjects[index];
            change.content_changed |=
                content_changed && (!blocks.dirty.is_empty() || !blocks.evicted.is_empty());
            change.dirty.extend(blocks.dirty);
            change.evicted.extend(blocks.evicted);
        }
    }
}

pub fn event_changes_content(event: &SessionEvent) -> bool {
    use crate::activity::ExecutionEvent;
    match event {
        SessionEvent::Activity(
            ActivityEvent::Content { event, .. }
            | ActivityEvent::HistoryContent { event, .. }
            | ActivityEvent::MainContent { event, .. },
        ) => !matches!(event, ExecutionEvent::TokenUsage { .. }),
        SessionEvent::Activity(ActivityEvent::Status { .. }) => true,
        SessionEvent::Activity(_)
        | SessionEvent::TokenUsage { .. }
        | SessionEvent::Commands { .. }
        | SessionEvent::Models { .. }
        | SessionEvent::PermissionMode { .. }
        | SessionEvent::ContextDetails { .. }
        | SessionEvent::McpServers { .. }
        | SessionEvent::McpAuthorization { .. }
        | SessionEvent::ThinkingTokens { .. }
        | SessionEvent::AccountPlan { .. }
        | SessionEvent::Handshake { .. }
        | SessionEvent::HookRan { .. }
        | SessionEvent::ControlRefused { .. }
        | SessionEvent::ScheduledWork { .. } => false,
        _ => true,
    }
}
