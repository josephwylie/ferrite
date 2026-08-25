//! Shared vocabulary: the typed event stream a provider Session emits.
//!
//! SessionEvent grows as a superset, append-only — provider concepts keep
//! typed payloads; nothing is flattened to a lowest common denominator.

/// One structured event from a provider Session.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// The provider session is live. `session_id` is the provider-native id
    /// later used for resume.
    Init { session_id: String, model: String },
    /// Assistant text streamed in.
    TextDelta { text: String },
    /// Extended thinking streamed in (Claude).
    ThinkingDelta { text: String },
    /// A turn finished.
    TurnEnded {
        outcome: TurnOutcome,
        cost_usd: Option<f64>,
    },
    /// The session process exited; no further events will arrive.
    Closed { reason: String },
}

/// How a turn ended.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnOutcome {
    Completed,
    Interrupted,
    Error(String),
}
