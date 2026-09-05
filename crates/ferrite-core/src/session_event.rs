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
    /// Provider-reported context occupancy and output accounting. Codex's
    /// output counters accumulate across the Session; Claude reports them
    /// per message/turn. Neither is the current context size.
    TokenUsage {
        /// The latest active context size, including input and output.
        total_tokens: u64,
        input_tokens: u64,
        cached_input_tokens: u64,
        output_tokens: u64,
        reasoning_output_tokens: u64,
        /// The model's context window, when the provider states it.
        context_window: Option<u64>,
    },
    /// The Session's own command menu, announced at Session start — what the
    /// Composer's `/` popover lists (#23). Claude lifts it from the initialize
    /// handshake's `commands[]`; Codex answers a `skills/list` request. Session
    /// state like a Decision, not durable history: a replacement Session
    /// announces its own.
    Commands { commands: Vec<SessionCommand> },
    /// The permission mode the Session started in, in the provider's own
    /// word (`"acceptEdits"`, `"bypassPermissions"`, …) — the Composer's
    /// meta-row mode chip (#23). Claude lifts it from the same initialize
    /// handshake; display-only, and Session state like the menu above.
    PermissionMode { mode: String },
    /// The models this install offers, each with the name the provider's
    /// own menu shows — the model picker's rows (#25). Claude lifts the
    /// list from the same initialize handshake; Codex asks `model/list`
    /// once its thread is up. Until either speaks the picker falls back
    /// to the catalog in `providers::models`. Session state exactly like
    /// the command menu: gone with the Session.
    Models { models: Vec<ModelInfo> },
    /// The session process exited; no further events will arrive.
    Closed { reason: String },
}

/// One model a provider offers: the value its CLI accepts, and the name a
/// person reads. The value is what goes on the wire (`--model sonnet`,
/// `"model": "gpt-5.6"`); the display is what every chip and row shows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModelInfo {
    /// What the provider accepts as the model choice: an alias (`sonnet`,
    /// `opus[1m]`) or a full id (`claude-fable-5-1`, `gpt-5.6`).
    pub value: String,
    /// The human name — `Fable 5.1`, `Opus (1M context)`, `GPT-5.6 Sol`.
    pub display: String,
    /// One line of detail the provider's menu shows beside the name, or
    /// empty.
    pub detail: String,
    /// The full id the value resolves to, when the provider says — how the
    /// model a Session's Init names is matched back to its row.
    pub resolved: Option<String>,
    /// The effort levels this model accepts, in the provider's own words
    /// and order (`low` … `max`, Codex adds `ultra` on some). Empty means
    /// the model takes no effort setting — Haiku, for one — and the picker
    /// offers none.
    pub efforts: Vec<String>,
    /// The effort the provider applies when none is asked for, when it
    /// says (Codex does; Claude does not).
    pub default_effort: Option<String>,
}

impl ModelInfo {
    /// A model known only by its value: the display is groomed from it,
    /// and nothing is known about its efforts.
    pub fn bare(value: impl Into<String>) -> Self {
        let value = value.into();
        Self {
            display: crate::providers::models::display_name(&value),
            detail: String::new(),
            resolved: None,
            efforts: Vec::new(),
            default_effort: None,
            value,
        }
    }

    /// Whether `model` — an Init's full id or a chosen value — is this row.
    pub fn is(&self, model: &str) -> bool {
        self.value == model || self.resolved.as_deref() == Some(model)
    }
}

impl From<&str> for ModelInfo {
    fn from(value: &str) -> Self {
        Self::bare(value)
    }
}

impl PartialEq<str> for ModelInfo {
    fn eq(&self, other: &str) -> bool {
        self.value == other
    }
}

/// One entry of a Session's command menu, in the provider's own words.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionCommand {
    /// What the operator types after `/`.
    pub name: String,
    pub description: String,
    /// Codex only: the SKILL.md a typed `{"type":"skill"}` input item must
    /// carry. Claude commands have none — the CLI dispatches on the text.
    pub path: Option<String>,
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
