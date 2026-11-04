//! Live drafts of a call whose row draws its input as it streams in
//! (`input_json_delta`), read with the tolerant partial-JSON reader: a
//! `show_visual` page builds up while the agent is still writing it, and an
//! Edit or a Write has its row while its (often long) input arrives (HD39),
//! rather than nothing until the settled call. Every other tool's streamed
//! input stays dropped — its row waits for the settled call.

use std::collections::HashMap;

use serde_json::Value;

use crate::SessionEvent;

/// How much more input must arrive before the next draft: often enough that
/// the page visibly builds up, seldom enough that a long page is not
/// re-read and re-sent per few-byte delta.
const DRAFT_STEP: usize = 256;

#[derive(Default)]
pub(super) struct Drafts {
    /// Open calls by their block index in the current message.
    open: HashMap<u64, Draft>,
}

struct Draft {
    id: String,
    name: String,
    json: String,
    /// How much of `json` the last draft was read from.
    sent: usize,
}

impl Drafts {
    /// Read one Main `stream_event`; answer the draft it moves, if any.
    pub(super) fn observe(&mut self, value: &Value) -> Option<SessionEvent> {
        let event = &value["event"];
        let index = event["index"].as_u64();
        match event["type"].as_str()? {
            "message_start" => {
                self.open.clear();
                None
            }
            "content_block_start" => {
                let block = &event["content_block"];
                let name = block["name"].as_str()?;
                if block["type"] != "tool_use" || !drafted(name) {
                    return None;
                }
                self.open.insert(
                    index?,
                    Draft {
                        id: block["id"].as_str()?.to_owned(),
                        name: name.to_owned(),
                        json: String::new(),
                        sent: 0,
                    },
                );
                None
            }
            "content_block_delta" if event["delta"]["type"] == "input_json_delta" => {
                let draft = self.open.get_mut(&index?)?;
                draft
                    .json
                    .push_str(event["delta"]["partial_json"].as_str()?);
                if draft.sent > 0 && draft.json.len() < draft.sent + DRAFT_STEP {
                    return None;
                }
                let input = crate::visual::partial_input(&draft.json)?;
                draft.sent = draft.json.len().max(1);
                Some(SessionEvent::ToolDraft {
                    id: draft.id.clone(),
                    name: draft.name.clone(),
                    input,
                })
            }
            "content_block_stop" => {
                self.open.remove(&index?);
                None
            }
            _ => None,
        }
    }
}

/// Whether `name`'s row draws its input while it streams.
fn drafted(name: &str) -> bool {
    crate::visual::is_tool(name) || crate::transcript::drafts_edit(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn frame(event: Value) -> Value {
        json!({"type": "stream_event", "event": event})
    }

    fn delta(index: u64, text: &str) -> Value {
        frame(json!({"type": "content_block_delta", "index": index,
            "delta": {"type": "input_json_delta", "partial_json": text}}))
    }

    fn start(index: u64, name: &str) -> Value {
        frame(json!({"type": "content_block_start", "index": index,
            "content_block": {"type": "tool_use", "id": format!("toolu_{index}"), "name": name, "input": {}}}))
    }

    /// An edit drafts as its input streams (HD39); a command does not.
    #[test]
    fn an_edit_drafts_as_it_streams() {
        let mut drafts = Drafts::default();
        assert_eq!(drafts.observe(&start(0, "Write")), None);
        assert_eq!(
            drafts.observe(&delta(0, "{\"file_path\": \"/w/a.rs\", \"content\": \"fn")),
            Some(SessionEvent::ToolDraft {
                id: "toolu_0".into(),
                name: "Write".into(),
                input: json!({"file_path": "/w/a.rs", "content": "fn"}),
            })
        );
    }

    #[test]
    fn a_visual_drafts_as_it_streams_and_other_tools_do_not() {
        let mut drafts = Drafts::default();
        assert_eq!(drafts.observe(&start(0, "Bash")), None);
        assert_eq!(drafts.observe(&delta(0, "{\"command\": \"ls")), None);
        assert_eq!(drafts.observe(&start(1, crate::visual::CLAUDE_NAME)), None);
        let first = drafts.observe(&delta(1, "{\"title\": \"Hel"));
        assert_eq!(
            first,
            Some(SessionEvent::ToolDraft {
                id: "toolu_1".into(),
                name: crate::visual::CLAUDE_NAME.into(),
                input: json!({"title": "Hel"}),
            })
        );
        // Small deltas wait for a step's worth of input.
        assert_eq!(drafts.observe(&delta(1, "lo\", \"html\": \"<p>")), None);
        let page = "x".repeat(DRAFT_STEP);
        let Some(SessionEvent::ToolDraft { input, .. }) = drafts.observe(&delta(1, &page)) else {
            panic!("a step of input drafts again");
        };
        assert_eq!(
            input,
            json!({"title": "Hello", "html": format!("<p>{page}")})
        );
        assert_eq!(
            drafts.observe(&frame(json!({"type": "content_block_stop", "index": 1}))),
            None
        );
        assert_eq!(drafts.observe(&delta(1, "more")), None);
    }
}
