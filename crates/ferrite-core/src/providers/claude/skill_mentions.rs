//! Skills named mid-prompt, told to the model the way the terminal tells it.
//!
//! The terminal attaches a `skill_mention` note when a prompt names a skill
//! anywhere but its head. The CLI only does that for text its own composer
//! marked as typed (`preExpansionInput`), which stream-json cannot carry, so
//! a user-only skill (`disable-model-invocation`) is never mentioned to the
//! model over stdin. The Skill tool still runs one whose `/name` the turn's
//! text contains, so the note is the whole gap; Ferrite writes it, in the
//! CLI's own words. See docs/research/claude-inline-skill-mentions.md.
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

    /// One note per skill the prompt names as a whole word, once each. A
    /// leading `/name` is left alone: the CLI dispatches that itself.
    pub fn notes(&self, text: &str) -> Vec<Value> {
        let mut named: Vec<&str> = Vec::new();
        let words = text
            .split_whitespace()
            .skip(usize::from(text.starts_with('/')));
        for word in words {
            let Some(name) = word.strip_prefix('/') else {
                continue;
            };
            if self.names.iter().any(|skill| skill == name) && !named.contains(&name) {
                named.push(name);
            }
        }
        named
            .into_iter()
            .map(|name| {
                serde_json::json!({"type": "text", "text": format!(
                    "The user's message contains /{name}, which is the name of a skill. \
                     If they are asking you to run it, call the Skill tool with skill: \
                     \"{name}\", passing any arguments they gave as args. If they only \
                     mention it, do not run it."
                )})
            })
            .collect()
    }
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

    fn named(notes: &[Value]) -> Vec<&str> {
        notes
            .iter()
            .map(|note| {
                let text = note["text"].as_str().unwrap();
                let name = text.split_whitespace().nth(4).unwrap();
                name.trim_start_matches('/').trim_end_matches(',')
            })
            .collect()
    }

    #[test]
    fn a_skill_named_mid_prompt_gets_the_terminals_note() {
        let notes = skills().notes("Wait, do we need /mattpocock-skills:grill-with-docs");
        assert_eq!(
            notes,
            [json!({"type":"text","text":
                "The user's message contains /mattpocock-skills:grill-with-docs, which is the \
                 name of a skill. If they are asking you to run it, call the Skill tool with \
                 skill: \"mattpocock-skills:grill-with-docs\", passing any arguments they gave \
                 as args. If they only mention it, do not run it."})]
        );
    }

    #[test]
    fn only_whole_words_naming_skills_are_noted_once_each() {
        let skills = skills();
        assert_eq!(
            named(&skills.notes("try /review then /review again, or /clear /reui:improve")),
            ["review"]
        );
        assert!(skills.notes("is /review? or path/review").is_empty());
    }

    #[test]
    fn a_leading_skill_is_left_to_the_cli() {
        let skills = skills();
        assert!(skills.notes("/review this").is_empty());
        assert_eq!(
            named(&skills.notes("/review then /mattpocock-skills:grill-with-docs")),
            ["mattpocock-skills:grill-with-docs"]
        );
    }

    #[test]
    fn the_menu_follows_commands_changed_and_ignores_other_lines() {
        let mut skills = skills();
        skills.observe(&json!({"type":"result","commands":[{"name":"stray"}]}));
        assert_eq!(named(&skills.notes("x /review")), ["review"]);
        skills.observe(&json!({"type":"system","subtype":"commands_changed",
            "commands":[{"name":"fresh","description":""}]}));
        assert!(skills.notes("x /review").is_empty());
        assert_eq!(named(&skills.notes("x /fresh")), ["fresh"]);
    }
}
