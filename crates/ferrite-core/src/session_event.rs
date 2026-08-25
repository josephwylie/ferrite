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
    /// A tool call the provider has settled and is about to run. `input` is
    /// the tool's own schema, so it stays a `Value`: inventing a Ferrite type
    /// per tool would be a guess that goes stale on the vendor's next release.
    ToolStarted {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// A tool call finished. `output` is exactly what the provider fed back to
    /// the model — text as text, anything else as its compact JSON.
    ///
    /// Claude also reports a *structured* result alongside it (a stdout/stderr
    /// pair for Bash, a patch for an edit). Nothing renders that yet, so it is
    /// not modelled here; the committed fixtures carry it under
    /// `tool_use_result` for whoever builds diff cards.
    ToolCompleted {
        id: String,
        output: String,
        is_error: bool,
        /// The provider's own structured result, where Ferrite models the
        /// shape — a diff card cannot be drawn from `output`, which is prose
        /// written for the model.
        result: ToolResult,
    },
    /// The Session is blocked on a Decision: only the operator can say whether
    /// this tool may run. Answer it with the provider's respond-to-Decision
    /// call, quoting the Decision's `id`.
    DecisionRequested { decision: Decision },
    /// A turn finished.
    TurnEnded {
        outcome: TurnOutcome,
        cost_usd: Option<f64>,
    },
    /// A slice of a reasoning summary streamed in (Codex). Not a thinking
    /// delta: Codex never streams raw chain-of-thought over app-server —
    /// these are the model-authored summaries of hidden reasoning, arriving
    /// part by part.
    ReasoningSummaryDelta {
        text: String,
        /// Which summary part this delta extends; a new index starts a new
        /// part, so a Pane can break paragraphs where the provider did.
        summary_index: u64,
    },
    /// Cumulative token accounting for the Session (Codex). Codex reports
    /// spend in tokens against a context window, never in dollars —
    /// `TurnEnded::cost_usd` stays `None` for it, and this event is where its
    /// cost and compaction risk actually live.
    TokenUsage {
        total_tokens: u64,
        input_tokens: u64,
        cached_input_tokens: u64,
        output_tokens: u64,
        reasoning_output_tokens: u64,
        /// The model's context window, when the provider states it.
        context_window: Option<u64>,
    },
    /// The session process exited; no further events will arrive.
    Closed { reason: String },
}

/// The structured half of a tool result. Every shape here was read off a
/// recorded capture; anything else stays `Opaque` rather than being guessed at.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum ToolResult {
    /// No structured payload, or one Ferrite does not model.
    #[default]
    Opaque,
    /// A command ran and wrote to the usual two streams.
    Command { stdout: String, stderr: String },
    /// A file was written. `hunks` is empty when the file was created, which
    /// has nothing to diff against.
    FileEdit { path: String, hunks: Vec<Hunk> },
}

/// One changed region of a file, in the provider's own unified-diff form.
#[derive(Debug, Clone, PartialEq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// Each line keeps its marker: ' ' context, '-' removed, '+' added.
    pub lines: Vec<String>,
}

/// A tool call the operator has to rule on before it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    /// The provider's handle for this Decision.
    pub id: String,
    /// The tool call being gated — the id of its `ToolStarted`, so a Pane can
    /// put the Decision on the tool card it blocks.
    pub tool_use_id: String,
    pub tool_name: String,
    /// The provider's own one-line summary, for a Pane too small to render
    /// `input` (the wall answers Decisions without focusing).
    pub description: String,
    pub input: serde_json::Value,
    /// Standing answers this request offers ("allow edits for this session"),
    /// verbatim. Empty means this request has none to offer — Codex's
    /// file-change approvals carry none — and the card offers no "always".
    pub suggestions: Vec<serde_json::Value>,
}

impl Decision {
    /// The standing answer this request offers, if it offers one. Both
    /// providers put structured choices among plainer ones — Codex lists
    /// `"accept"` and `"cancel"` beside its amendment object — so the
    /// structured entry is the one that means "and don't ask again".
    pub fn standing_answer(&self) -> Option<&serde_json::Value> {
        self.suggestions.iter().find(|offered| offered.is_object())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decision(suggestions: Vec<serde_json::Value>) -> Decision {
        Decision {
            id: "1".into(),
            tool_use_id: "toolu_1".into(),
            tool_name: "Write".into(),
            description: String::new(),
            input: serde_json::Value::Null,
            suggestions,
        }
    }

    /// Both shapes as the captures carry them.
    #[test]
    fn the_standing_answer_is_the_structured_one_each_provider_offers() {
        let claude = decision(vec![serde_json::json!({
            "type": "setMode", "mode": "acceptEdits", "destination": "session"
        })]);
        assert_eq!(claude.standing_answer(), claude.suggestions.first());

        let codex = decision(vec![
            serde_json::json!("accept"),
            serde_json::json!({"acceptWithExecpolicyAmendment": {"execpolicy_amendment": ["x"]}}),
            serde_json::json!("cancel"),
        ]);
        assert_eq!(codex.standing_answer(), codex.suggestions.get(1));

        // A file-change approval offers none at all, and the card must not
        // pretend otherwise.
        assert_eq!(decision(vec![]).standing_answer(), None);
        assert_eq!(
            decision(vec![serde_json::json!("accept")]).standing_answer(),
            None
        );
    }
}

/// How the operator answered a `DecisionRequested`.
#[derive(Debug, Clone, PartialEq)]
pub enum DecisionAnswer {
    /// Run the tool, with this input. Normally the input the Decision arrived
    /// with, echoed back; a different value runs the tool with edits.
    Allow { input: serde_json::Value },
    /// Refuse. The message reaches the model as the tool's failed result, so
    /// the turn continues with a refusal rather than dying.
    Deny { message: String },
    /// Allow, and adopt one of the standing answers this request offered —
    /// echoed back exactly as it arrived in `Decision::suggestions`. Each
    /// provider spells the adoption its own way; both were captured answering
    /// a gated call whose repeat then ran unasked.
    AllowAlways {
        input: serde_json::Value,
        suggestion: serde_json::Value,
    },
}

/// How a turn ended.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnOutcome {
    Completed,
    Interrupted,
    Error(String),
}
