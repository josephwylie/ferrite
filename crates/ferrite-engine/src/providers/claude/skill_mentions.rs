//! How the skills a prompt names reach the model.
//!
//! Two things can load a skill. The CLI expands a leading `/name` itself,
//! but only when the text block that starts with it is the content's last
//! block, and only that one skill. Once the CLI has rewritten the message,
//! the Skill tool refuses every other user-only skill the message names
//! (2.1.328, probed). The Skill tool runs a user-only skill
//! (`disable-model-invocation`) when a user message it has not rewritten
//! holds `/name` as a whole word. But the CLI only tells the model about a
//! skill named mid-prompt for text its own composer typed
//! (`preExpansionInput`), and stream-json cannot carry that. So Ferrite
//! writes the terminal's `skill_mention` note in the CLI's own words.
//!
//! The order follows from that. A prompt whose only skill leads it goes to
//! the CLI, with its text last. A prompt led by one skill of several gets
//! its notes after the text, which keeps the CLI from expanding anything,
//! and the Skill tool loads every skill. Any other prompt keeps its text
//! last, with the notes ahead of it: one with no head, or one led by a
//! built-in or an MCP prompt, which only the CLI can dispatch.
use serde_json::Value;

/// The CLI's skills, kept current from the lines that announce its menu.
#[derive(Default)]
pub(super) struct Skills {
    names: Vec<String>,
}

impl Skills {
    /// Learn the menu from the initialize answer (spawn's and the MCP
    /// settle's) or a `commands_changed` line. Built-ins and MCP prompts are
    /// not skills; the Skill tool cannot run them.
    pub fn observe(&mut self, value: &Value) {
        let commands = if value["type"] == "control_response" {
            &value["response"]["response"]["commands"]
        } else if value["type"] == "system" && value["subtype"] == "commands_changed" {
            &value["commands"]
        } else {
            return;
        };
        let Some(commands) = commands.as_array() else {
            return;
        };
        self.names = commands
            .iter()
            .filter(|command| command["builtin"] != true)
            .filter_map(|command| command["name"].as_str())
            .filter(|name| super::wire::mcp_prompt_command(name).is_none())
            .map(str::to_owned)
            .collect();
    }

    /// The user message content for `text`: its `media`, its own text block
    /// (quoted `@` defused), and a note for each skill the CLI will not
    /// load itself. The blocks go in the one order that loads every skill
    /// the operator named.
    pub fn content(&self, text: &str, media: Vec<Value>) -> Vec<Value> {
        let authored = ferrite_core::replies::authored(text);
        let mut words = authored.split_whitespace();
        // The CLI reads a head only at the very start of the block.
        let head = if text.starts_with('/') {
            words.next().and_then(|word| self.skill(word))
        } else {
            None
        };
        let mut rest: Vec<&str> = Vec::new();
        for name in words.filter_map(|word| self.skill(word)) {
            if Some(name) != head && !rest.contains(&name) {
                rest.push(name);
            }
        }
        let prompt = media
            .into_iter()
            .chain([serde_json::json!({"type": "text", "text": ferrite_core::replies::inert(text)})]);
        match head {
            // Expanding the head would lock the other skills out, so a note
            // after the text stops the expansion and the Skill tool loads
            // every skill.
            Some(head) if !rest.is_empty() => prompt
                .chain(std::iter::once(head).chain(rest).map(note))
                .collect(),
            _ => rest.into_iter().map(note).chain(prompt).collect(),
        }
    }

    /// The skill `word` names: a `/` and then a skill's whole name.
    fn skill(&self, word: &str) -> Option<&str> {
        let name = word.strip_prefix('/')?;
        self.names
            .iter()
            .map(String::as_str)
            .find(|skill| *skill == name)
    }
}

/// The terminal's `skill_mention` note.
fn note(name: &str) -> Value {
    serde_json::json!({"type": "text", "text": format!(
        "The user's message contains /{name}, which is the name of a skill. \
         If they are asking you to run it, call the Skill tool with skill: \
         \"{name}\", passing any arguments they gave as args. If they only \
         mention it, do not run it."
    )})
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn skills() -> Skills {
        let mut skills = Skills::default();
        skills.observe(&json!({"type":"control_response","response":{
        "subtype":"success","request_id":"req_1","response":{"commands":[
            {"name":"mattpocock-skills:grill-with-docs","description":""},
            {"name":"review","description":""},
            {"name":"clear","description":"","builtin":true},
            {"name":"reui:improve (MCP)","description":""},
        ]}}}));
        skills
    }

    fn image(id: &str) -> Value {
        json!({"type":"image","id":id})
    }

    /// Each block as `image <id>`, `note <skill>`, or `text` for the prompt.
    fn shape(content: &[Value]) -> Vec<String> {
        content
            .iter()
            .map(|block| {
                if block["type"] == "image" {
                    return format!("image {}", block["id"].as_str().unwrap());
                }
                let text = block["text"].as_str().unwrap();
                match text.strip_prefix("The user's message contains /") {
                    Some(rest) => format!("note {}", rest.split(',').next().unwrap()),
                    None => "text".into(),
                }
            })
            .collect()
    }

    #[test]
    fn a_lone_leading_skill_is_left_to_the_cli() {
        let skills = skills();
        assert_eq!(shape(&skills.content("/review this", vec![])), ["text"]);
        assert_eq!(
            shape(&skills.content("/review this", vec![image("a")])),
            ["image a", "text"]
        );
        assert_eq!(
            shape(&skills.content("/review this", vec![image("a"), image("b")])),
            ["image a", "image b", "text"]
        );
        assert_eq!(
            shape(&skills.content("/review this, then /review again", vec![image("a")])),
            ["image a", "text"]
        );
        assert_eq!(
            shape(&skills.content("/review then /clear", vec![])),
            ["text"]
        );
    }

    #[test]
    fn a_leading_skill_among_others_is_noted_with_them_after_the_text() {
        let skills = skills();
        assert_eq!(
            shape(&skills.content(
                "/review then /mattpocock-skills:grill-with-docs",
                vec![image("a")]
            )),
            [
                "image a",
                "text",
                "note review",
                "note mattpocock-skills:grill-with-docs"
            ]
        );
        assert_eq!(
            shape(&skills.content("/review /mattpocock-skills:grill-with-docs rest", vec![])),
            [
                "text",
                "note review",
                "note mattpocock-skills:grill-with-docs"
            ]
        );
        assert_eq!(
            shape(&skills.content(
                "/review /mattpocock-skills:grill-with-docs then /review again",
                vec![]
            )),
            [
                "text",
                "note review",
                "note mattpocock-skills:grill-with-docs"
            ]
        );
    }

    #[test]
    fn a_skill_named_mid_prompt_gets_the_terminals_note() {
        let skills = skills();
        let text = "Wait, do we need /mattpocock-skills:grill-with-docs";
        assert_eq!(
            skills.content(text, vec![]),
            [
                json!({"type":"text","text":
                "The user's message contains /mattpocock-skills:grill-with-docs, which is the \
                 name of a skill. If they are asking you to run it, call the Skill tool with \
                 skill: \"mattpocock-skills:grill-with-docs\", passing any arguments they gave \
                 as args. If they only mention it, do not run it."}),
                json!({"type":"text","text":text}),
            ]
        );
        assert_eq!(
            shape(&skills.content(text, vec![image("a")])),
            ["note mattpocock-skills:grill-with-docs", "image a", "text"]
        );
    }

    #[test]
    fn a_leading_command_keeps_the_text_last_with_the_notes_ahead() {
        let skills = skills();
        assert_eq!(
            shape(&skills.content("/clear then /review", vec![image("a")])),
            ["note review", "image a", "text"]
        );
        assert_eq!(
            shape(&skills.content("/mcp__reui__improve /review", vec![])),
            ["note review", "text"]
        );
    }

    #[test]
    fn a_skill_after_leading_space_is_named_mid_prompt() {
        assert_eq!(
            shape(&skills().content(" /review this", vec![])),
            ["note review", "text"]
        );
    }

    #[test]
    fn no_skill_named_sends_the_media_then_the_text() {
        assert_eq!(
            shape(&skills().content("look at this", vec![image("a")])),
            ["image a", "text"]
        );
    }

    #[test]
    fn a_quoted_skill_name_is_not_noted_but_the_authors_is() {
        use ferrite_core::replies::{compose, Reply};
        let skills = skills();
        let quoted = compose(&[Reply::new("run /review on @pic.png", "")], "why?");
        assert_eq!(
            skills.content(&quoted, vec![]),
            [json!({"type":"text","text":"> run /review on @\u{2060}pic.png\n\nwhy?"})]
        );
        let both = compose(&[Reply::new("run /review", "")], "/review");
        assert_eq!(
            shape(&skills.content(&both, vec![])),
            ["note review", "text"]
        );
    }

    #[test]
    fn only_whole_words_naming_skills_are_noted_once_each() {
        let skills = skills();
        assert_eq!(
            shape(&skills.content(
                "try /review then /review again, or /clear /reui:improve",
                vec![]
            )),
            ["note review", "text"]
        );
        assert_eq!(
            shape(&skills.content("is /review? or path/review", vec![])),
            ["text"]
        );
    }

    #[test]
    fn the_menu_follows_commands_changed_and_ignores_other_lines() {
        let mut skills = skills();
        skills.observe(&json!({"type":"result","commands":[{"name":"stray"}]}));
        assert_eq!(
            shape(&skills.content("x /review", vec![])),
            ["note review", "text"]
        );
        skills.observe(&json!({"type":"system","subtype":"commands_changed",
            "commands":[{"name":"fresh","description":""}]}));
        assert_eq!(shape(&skills.content("x /review", vec![])), ["text"]);
        assert_eq!(
            shape(&skills.content("x /fresh", vec![])),
            ["note fresh", "text"]
        );
    }
}
