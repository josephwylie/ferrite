//! Ferrite: keys and typed text as what they meant. herdr presses keys and types text in an agent's terminal; Ferrite
//! has no terminal, and what an agent asks there is a pending Decision (`decision.list`), answered whole
//! (`decision.answer`). So a menu key, and text typed at a question, become the Decision's answer, as the menu herdr's
//! terminal showed would have taken them.
use crate::json::*;

/// What a pending Decision asks, in its own words.
pub fn decision_text(decision: &Value) -> String {
    let mut parts: Vec<&str> = ["description", "tool_name"].iter().filter_map(|k| decision.at(k).as_str()).filter(|t| !t.is_empty()).collect();
    parts.extend(decision.at("kind").arr("questions").iter().filter_map(|q| q.at("question").as_str()));
    parts.join("\n")
}

/// The menu a pending Decision shows, as the answers its items give, in order: an approval's allow, then each
/// suggestion (allow always, or a choice where plain allow is not offered), then deny; a single question's options; a
/// form's single choice field's options.
fn menu(decision: &Value) -> Vec<Value> {
    let kind = decision.at("kind");
    match kind.s("kind") {
        "approval" => {
            let offered = |rule: &str| decision.at("policy").at(rule).as_bool().unwrap_or(true);
            let input = decision.at("input");
            let mut items = Vec::new();
            if offered("allow") { items.push(json!({ "kind": "allow", "input": input })); }
            for suggestion in decision.arr("suggestions") {
                items.push(match offered("allow") {
                    true => json!({ "kind": "allow_always", "input": input, "suggestion": suggestion.at("value") }),
                    false => json!({ "kind": "choose", "value": suggestion.at("value") }),
                });
            }
            if offered("deny") { items.push(json!({ "kind": "deny", "message": "" })); }
            items
        }
        "questions" => match kind.arr("questions") {
            [question] => (0..question.arr("options").len()).map(|pick| json!({ "kind": "questions", "answers": [{ "picks": [pick] }] })).collect(),
            _ => Vec::new(),
        },
        "form" => match kind.arr("fields") {
            [field] if field.at("kind").s("kind") == "enum" => field.at("kind").arr("options").iter().map(|option| {
                let value = if field.at("kind").at("multi_select").as_bool() == Some(true) { json!([option.at("value")]) } else { option.at("value").clone() };
                json!({ "kind": "form", "values": { field.s("id"): value } })
            }).collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The answer a key press gives a pending Decision: enter takes the first item (a form's defaults, when every required
/// field has one), a digit that item, `y` allows and `n` denies (a form's single yes/no field: true or false). None
/// when the keys mean nothing to it.
pub fn key_answer(decision: &Value, keys: &[String]) -> Option<Value> {
    let key = match keys {
        [key] => key.as_str(),
        [key, enter] if enter == "enter" => key.as_str(),
        _ => return None,
    };
    let items = menu(decision);
    let fields = (decision.at("kind").s("kind") == "form").then(|| decision.at("kind").arr("fields"));
    match (key, fields) {
        ("enter", Some(fields)) if items.is_empty() => form_defaults(fields),
        ("enter", _) => items.first().cloned(),
        ("y" | "n", Some([field])) if field.at("kind").s("kind") == "boolean" => Some(json!({ "kind": "form", "values": { field.s("id"): key == "y" } })),
        ("y", _) => items.iter().find(|item| item.s("kind") == "allow").cloned(),
        ("n", _) => items.iter().find(|item| item.s("kind") == "deny").cloned(),
        (digit, _) => items.get(digit.parse::<usize>().ok()?.checked_sub(1)?).cloned(),
    }
}

/// A form answered with its defaults, unless a required field has none.
fn form_defaults(fields: &[Value]) -> Option<Value> {
    let mut values = Obj::new();
    for field in fields {
        match field.at("kind").at("default") {
            Value::Null if field.at("required") == &json!(true) => return None,
            Value::Null => {}
            default => { values.insert(field.t("id"), default.clone()); }
        }
    }
    Some(json!({ "kind": "form", "values": values }))
}

/// The answer text typed at a pending Decision gives it: a single question's own answer, where it takes one, or a
/// form's single text field.
pub fn text_answer(decision: &Value, text: &str) -> Option<Value> {
    let kind = decision.at("kind");
    match kind.s("kind") {
        "questions" => match kind.arr("questions") {
            [question] if question.at("allow_other") == &json!(true) => Some(json!({ "kind": "questions", "answers": [{ "picks": [], "other": text }] })),
            _ => None,
        },
        "form" => match kind.arr("fields") {
            [field] if field.at("kind").s("kind") == "string" => Some(json!({ "kind": "form", "values": { field.s("id"): text } })),
            _ => None,
        },
        _ => None,
    }
}
