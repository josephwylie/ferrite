//! `wrap_up`: how an agent ends a turn that did work (HD58, Claude's turn
//! box, `claude-desktop-transcript-handoff.md` §13). One short recap, the
//! question it waits on if any, and at most two next steps the operator can
//! send back with one press — each step's words are exactly what is sent.
//!
//! Offered on Ferrite's in-band server beside `show_visual`
//! (`providers::claude::mcp`); the transcript folds the call into its own
//! row (`Body::WrapUp`), never a tool row. Like a visual, it needs no
//! persistence of its own: the call's input is durable history.

use serde_json::{json, Value};

/// The tool's name inside Ferrite's server (`visual::SERVER`).
pub const TOOL: &str = "wrap_up";
/// What Claude calls it.
pub const CLAUDE_NAME: &str = "mcp__ferrite__wrap_up";

/// How many next steps a box offers; the first is the one recommended.
pub const MAX_NEXT: usize = 2;

/// Whether a call (as the provider names it) is this tool.
pub fn is_tool(name: &str) -> bool {
    name == CLAUDE_NAME
}

/// A turn's wrap-up, as its row shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrapUp {
    /// The call it came from.
    pub call: String,
    pub recap: String,
    /// The question the turn waits on.
    pub ask: Option<String>,
    /// The turn is waiting on the operator, not finished: its box says so
    /// even with no question or step (§13: distinct from completion).
    pub blocked: bool,
    /// What the operator can send next, recommended first; never more than
    /// `MAX_NEXT`, never empty words.
    pub next: Vec<String>,
}

impl WrapUp {
    /// Read off the call's input. Anything malformed is dropped rather than
    /// refused: the box shows what is usable.
    pub fn from_input(call: String, input: &Value) -> Self {
        let text = |value: &Value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        };
        Self {
            call,
            recap: text(&input["recap"]).unwrap_or_default(),
            ask: text(&input["ask"]),
            blocked: input["blocked"].as_bool().unwrap_or(false),
            next: input["next"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(text)
                // A label is sent back as the next prompt: one that would
                // run a command or a `!` shell line is not a next step.
                .filter(|step| !step.starts_with(['/', '!']))
                .take(MAX_NEXT)
                .collect(),
        }
    }

    /// Whether the turn gets a box: a recap alone (nothing asked, nothing
    /// to send, not blocked) only updates the Thread's status, as in Claude.
    pub fn shows_box(&self) -> bool {
        self.ask.is_some() || self.blocked || !self.next.is_empty()
    }

    /// Its words for copy and search: the recap, the ask, each step.
    pub fn text(&self) -> String {
        std::iter::once(self.recap.as_str())
            .chain(self.ask.as_deref())
            .chain(self.next.iter().map(String::as_str))
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The tool's description, as the agent reads it.
pub const DESCRIPTION: &str = "Close a turn in which you did work (edited files, ran commands) with a short \
wrap-up box the operator sees under your answer. Call it once, as the last thing you do in such a turn; \
skip it for a turn that only answered a question. `recap`: one or two plain sentences on what changed and \
where things stand. `ask`: only when you need the operator to decide or answer something before you go on. \
`next`: at most two next steps you recommend, the best one first, each written as the exact instruction the \
operator would send you (\"Commit these changes\", \"Run the full test suite\"); leave it out when there is \
no obvious next step.";

/// The tool's input schema.
pub fn input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "recap": {
                "type": "string",
                "description": "One or two sentences: what changed and where things stand."
            },
            "ask": {
                "type": "string",
                "description": "The question you need answered before going on, if any."
            },
            "blocked": {
                "type": "boolean",
                "description": "True when you cannot go on until the operator answers or acts."
            },
            "next": {
                "type": "array",
                "items": {"type": "string"},
                "maxItems": MAX_NEXT,
                "description": "Up to two next steps, best first, each the exact instruction to send you."
            }
        },
        "required": ["recap"],
        "additionalProperties": false
    })
}

/// What the call is answered with: the box is drawn from its input.
pub const ANSWER: &str = "Shown to the operator.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrap_up_keeps_two_usable_steps() {
        let wrap = WrapUp::from_input(
            "c".into(),
            &json!({"recap": " Fixed the parser. ", "ask": "", "next": ["Commit", " ", "Push", "Merge"]}),
        );
        assert_eq!(wrap.recap, "Fixed the parser.");
        assert_eq!(wrap.ask, None);
        assert_eq!(wrap.next, ["Commit", "Push"]);
        assert_eq!(wrap.text(), "Fixed the parser.\nCommit\nPush");
    }

    /// A recap alone draws no box; a question, a step or a block does. A
    /// step that would run a command is not a step.
    #[test]
    fn only_more_than_a_recap_makes_a_box() {
        let wrap = |input: Value| WrapUp::from_input("c".into(), &input);
        assert!(!wrap(json!({"recap": "Done."})).shows_box());
        assert!(wrap(json!({"recap": "Done.", "ask": "Commit?"})).shows_box());
        assert!(wrap(json!({"recap": "Done.", "next": ["Commit"]})).shows_box());
        assert!(wrap(json!({"recap": "Stuck.", "blocked": true})).shows_box());
        let slashed = wrap(json!({"recap": "Done.", "next": ["/clear", "!rm x"]}));
        assert!(slashed.next.is_empty() && !slashed.shows_box());
    }
}
