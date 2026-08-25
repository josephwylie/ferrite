//! What a Pane shows: SessionEvents folded into view state, no window attached.

use ferrite_core::{SessionEvent, TurnOutcome};

/// How much of a tool failure a Pane shows; the model got all of it.
const ERROR_CHARS: usize = 200;

/// Cut to `limit` characters, marking the cut.
fn trim(text: String, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text;
    }
    text.chars().take(limit).chain(['…']).collect()
}

#[derive(Default)]
pub struct Transcript {
    model: Option<String>,
    session_id: Option<String>,
    segments: Vec<Segment>,
    status: Status,
    last_cost: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Status {
    #[default]
    Idle,
    Streaming,
    /// Stopped on a Decision only the operator can answer.
    Blocked,
    Closed,
}

pub struct Segment {
    pub kind: Kind,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    User,
    Assistant,
    Thinking,
    Tool,
    Meta,
    Notice,
}

impl Transcript {
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn last_cost(&self) -> Option<f64> {
        self.last_cost
    }

    pub fn apply(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Init { session_id, model } => {
                self.session_id = Some(session_id);
                self.model = Some(model);
            }
            SessionEvent::TextDelta { text } => {
                self.status = Status::Streaming;
                self.append(Kind::Assistant, &text);
            }
            SessionEvent::ThinkingDelta { text } => {
                self.status = Status::Streaming;
                self.append(Kind::Thinking, &text);
            }
            SessionEvent::ToolStarted { name, .. } => self.push(Kind::Tool, name),
            // A tool that worked has nothing to add: its result went to the
            // model, not the operator. A failure is the operator's business.
            SessionEvent::ToolCompleted {
                output, is_error, ..
            } => {
                if is_error {
                    self.push(Kind::Notice, trim(output, ERROR_CHARS));
                }
            }
            SessionEvent::DecisionRequested {
                tool_name,
                description,
                ..
            } => {
                self.status = Status::Blocked;
                self.push(
                    Kind::Notice,
                    format!("decision needed: {tool_name} — {description}"),
                );
            }
            SessionEvent::TurnEnded { outcome, cost_usd } => {
                self.status = Status::Idle;
                self.last_cost = cost_usd;
                match outcome {
                    TurnOutcome::Completed => {
                        if let Some(cost) = cost_usd {
                            self.push(Kind::Meta, format!("${cost:.4}"));
                        }
                    }
                    TurnOutcome::Interrupted => self.push(Kind::Meta, "interrupted".into()),
                    TurnOutcome::Error(message) => self.push(Kind::Notice, message),
                }
            }
            SessionEvent::Closed { reason } => {
                self.status = Status::Closed;
                self.push(Kind::Notice, reason);
            }
        }
    }

    /// Echo a prompt the operator just sent. The turn is under way from here,
    /// not from the first delta — which can be seconds out. A Closed session
    /// stays closed and a Blocked one stays blocked: nothing is streaming in
    /// either, and the status line must not say otherwise.
    pub fn push_user(&mut self, text: String) {
        self.push(Kind::User, text);
        if let Status::Idle | Status::Streaming = self.status {
            self.status = Status::Streaming;
        }
    }

    /// Something the app itself needs to say, not the provider.
    pub fn push_notice(&mut self, text: String) {
        self.push(Kind::Notice, text);
    }

    fn push(&mut self, kind: Kind, text: String) {
        self.segments.push(Segment { kind, text });
    }

    /// Deltas grow the segment they belong to; a change of kind starts one.
    fn append(&mut self, kind: Kind, text: &str) {
        match self.segments.last_mut() {
            Some(segment) if segment.kind == kind => segment.text.push_str(text),
            _ => self.segments.push(Segment {
                kind,
                text: text.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() -> SessionEvent {
        SessionEvent::Init {
            session_id: "4f2a1c9e-7b30".into(),
            model: "claude-sonnet-4-5".into(),
        }
    }

    #[test]
    fn init_names_the_session_the_header_shows() {
        let mut transcript = Transcript::default();
        assert_eq!(transcript.model(), None);

        transcript.apply(init());

        assert_eq!(transcript.model(), Some("claude-sonnet-4-5"));
        assert_eq!(transcript.session_id(), Some("4f2a1c9e-7b30"));
        assert!(transcript.segments().is_empty()); // Init writes no transcript line
    }

    #[test]
    fn streamed_text_deltas_grow_one_assistant_segment() {
        let mut transcript = Transcript::default();

        for word in ["Reading ", "the ", "composer "] {
            transcript.apply(SessionEvent::TextDelta { text: word.into() });
        }

        assert_eq!(transcript.segments().len(), 1);
        assert_eq!(transcript.segments()[0].kind, Kind::Assistant);
        assert_eq!(transcript.segments()[0].text, "Reading the composer ");
    }

    #[test]
    fn thinking_stays_separate_from_the_answer() {
        let mut transcript = Transcript::default();

        transcript.apply(SessionEvent::ThinkingDelta {
            text: "weighing ".into(),
        });
        transcript.apply(SessionEvent::ThinkingDelta {
            text: "options ".into(),
        });
        transcript.apply(SessionEvent::TextDelta {
            text: "Here. ".into(),
        });
        transcript.apply(SessionEvent::ThinkingDelta {
            text: "again ".into(),
        });

        let kinds: Vec<Kind> = transcript.segments().iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [Kind::Thinking, Kind::Assistant, Kind::Thinking]);
        assert_eq!(transcript.segments()[0].text, "weighing options ");
    }

    #[test]
    fn a_finished_turn_goes_idle_and_records_its_cost() {
        let mut transcript = Transcript::default();
        assert_eq!(transcript.status(), Status::Idle);

        transcript.apply(SessionEvent::TextDelta {
            text: "working ".into(),
        });
        assert_eq!(transcript.status(), Status::Streaming);

        transcript.apply(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.038),
        });

        assert_eq!(transcript.status(), Status::Idle);
        assert_eq!(transcript.last_cost(), Some(0.038));
        let last = transcript.segments().last().unwrap();
        assert_eq!(last.kind, Kind::Meta);
        assert_eq!(last.text, "$0.0380");
    }

    #[test]
    fn an_interrupted_turn_says_so_and_carries_no_cost() {
        let mut transcript = Transcript::default();
        transcript.apply(SessionEvent::TextDelta {
            text: "half a thou".into(),
        });

        transcript.apply(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Interrupted,
            cost_usd: None,
        });

        assert_eq!(transcript.status(), Status::Idle);
        assert_eq!(transcript.last_cost(), None);
        assert_eq!(transcript.segments().last().unwrap().text, "interrupted");
    }

    #[test]
    fn a_failed_turn_surfaces_the_providers_message() {
        let mut transcript = Transcript::default();

        transcript.apply(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Error("model overloaded".into()),
            cost_usd: None,
        });

        assert_eq!(transcript.status(), Status::Idle);
        let last = transcript.segments().last().unwrap();
        assert_eq!(last.kind, Kind::Notice);
        assert_eq!(last.text, "model overloaded");
    }

    #[test]
    fn a_closed_session_is_final_and_shows_why() {
        let mut transcript = Transcript::default();
        transcript.apply(init());

        transcript.apply(SessionEvent::Closed {
            reason: "claude CLI exited with code 1".into(),
        });

        assert_eq!(transcript.status(), Status::Closed);
        let last = transcript.segments().last().unwrap();
        assert_eq!(last.kind, Kind::Notice);
        assert_eq!(last.text, "claude CLI exited with code 1");
    }

    #[test]
    fn each_sent_prompt_is_echoed_as_its_own_segment() {
        let mut transcript = Transcript::default();
        transcript.apply(SessionEvent::TextDelta {
            text: "earlier answer".into(),
        });

        transcript.push_user("run the tests".into());
        transcript.push_user("again".into()); // consecutive prompts never merge

        let kinds: Vec<Kind> = transcript.segments().iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [Kind::Assistant, Kind::User, Kind::User]);
        assert_eq!(transcript.segments()[1].text, "run the tests");
    }

    #[test]
    fn sending_a_prompt_streams_before_the_first_delta_arrives() {
        let mut transcript = Transcript::default();
        transcript.apply(SessionEvent::TurnEnded {
            outcome: TurnOutcome::Completed,
            cost_usd: Some(0.01),
        });
        assert_eq!(transcript.status(), Status::Idle);

        transcript.push_user("go".into());

        assert_eq!(transcript.status(), Status::Streaming);
    }

    #[test]
    fn prompting_a_closed_session_never_shows_streaming() {
        let mut transcript = Transcript::default();
        transcript.apply(SessionEvent::Closed {
            reason: "claude CLI exited".into(),
        });

        transcript.push_user("anyone there?".into());

        assert_eq!(transcript.status(), Status::Closed);
    }

    #[test]
    fn prompting_a_blocked_session_never_shows_streaming() {
        let mut transcript = Transcript::default();
        transcript.apply(SessionEvent::DecisionRequested {
            id: "perm_01".into(),
            tool_use_id: "toolu_01".into(),
            tool_name: "Write".into(),
            description: "ferrite-perm.txt".into(),
            input: serde_json::Value::Null,
            suggestions: vec![],
        });

        transcript.push_user("go ahead".into());

        // The Decision is still what the Session is waiting on, not this prompt.
        assert_eq!(transcript.status(), Status::Blocked);
    }

    #[test]
    fn a_local_failure_is_shown_as_a_notice() {
        let mut transcript = Transcript::default();

        transcript.push_notice("send failed: broken pipe".into());

        let last = transcript.segments().last().unwrap();
        assert_eq!(last.kind, Kind::Notice);
        assert_eq!(last.text, "send failed: broken pipe");
    }

    #[test]
    fn a_tool_call_breaks_the_answer_and_names_the_tool() {
        let mut transcript = Transcript::default();
        transcript.apply(SessionEvent::TextDelta {
            text: "let me look ".into(),
        });

        transcript.apply(SessionEvent::ToolStarted {
            id: "toolu_01".into(),
            name: "Read".into(),
            input: serde_json::json!({ "file_path": "CONTEXT.md" }),
        });
        transcript.apply(SessionEvent::TextDelta {
            text: "found it".into(),
        });

        let kinds: Vec<Kind> = transcript.segments().iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [Kind::Assistant, Kind::Tool, Kind::Assistant]);
        assert_eq!(transcript.segments()[1].text, "Read");
    }

    #[test]
    fn a_failed_tool_surfaces_its_error_and_a_working_one_stays_quiet() {
        let mut transcript = Transcript::default();
        transcript.apply(SessionEvent::ToolStarted {
            id: "toolu_01".into(),
            name: "Read".into(),
            input: serde_json::Value::Null,
        });

        transcript.apply(SessionEvent::ToolCompleted {
            id: "toolu_01".into(),
            output: "the whole file, which the operator does not need".into(),
            is_error: false,
        });
        assert_eq!(transcript.segments().len(), 1); // the tool line, nothing more

        transcript.apply(SessionEvent::ToolCompleted {
            id: "toolu_02".into(),
            output: "No such file or directory".into(),
            is_error: true,
        });

        let last = transcript.segments().last().unwrap();
        assert_eq!(last.kind, Kind::Notice);
        assert_eq!(last.text, "No such file or directory");
    }

    // A failed Bash can dump kilobytes; a Pane is one cell of a 24-pane wall.
    #[test]
    fn a_long_tool_error_is_trimmed_to_keep_the_pane_dense() {
        let mut transcript = Transcript::default();

        transcript.apply(SessionEvent::ToolCompleted {
            id: "toolu_01".into(),
            output: "x".repeat(500),
            is_error: true,
        });

        let text = &transcript.segments().last().unwrap().text;
        assert_eq!(text.chars().count(), 201);
        assert!(text.ends_with('…'));
    }

    #[test]
    fn a_decision_blocks_the_session_and_says_what_is_waiting() {
        let mut transcript = Transcript::default();
        transcript.apply(SessionEvent::TextDelta {
            text: "writing the file ".into(),
        });
        assert_eq!(transcript.status(), Status::Streaming);

        transcript.apply(SessionEvent::DecisionRequested {
            id: "perm_01".into(),
            tool_use_id: "toolu_01".into(),
            tool_name: "Write".into(),
            description: "ferrite-perm.txt".into(),
            input: serde_json::Value::Null,
            suggestions: vec![],
        });

        // Nothing is streaming: the Session is stopped until the operator answers.
        assert_eq!(transcript.status(), Status::Blocked);
        let last = transcript.segments().last().unwrap();
        assert_eq!(last.kind, Kind::Notice);
        assert_eq!(last.text, "decision needed: Write — ferrite-perm.txt");
    }
}
