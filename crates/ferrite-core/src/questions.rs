//! Provider-normalized question forms and answer composition. Adapters own
//! delivery: Claude resumes a blocked tool; Codex async replies steer a turn.
//! No provider wire protocol or UI controls cross this seam.

use serde_json::{Map, Value};

/// The tool name Claude Code uses for a multiple-choice question.
const TOOL_NAME: &str = "AskUserQuestion";

/// Normalized nonblocking question delivery; the provider owns reply transport.
pub const ASYNC_TOOL_NAME: &str = "request_user_input_async";
pub fn is_async(tool_name: &str) -> bool {
    tool_name == ASYNC_TOOL_NAME
}

/// One question the model put to the operator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    /// The full question text; also the key of its answer in `updatedInput`.
    pub question: String,
    /// The model's short label for the question ("Approach", "Library").
    pub header: String,
    pub options: Vec<Choice>,
    /// Whether the operator may pick more than one option.
    pub multi_select: bool,
}

/// One option of a [`Question`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    /// What the operator sees and what the answer carries back verbatim.
    pub label: String,
    pub description: String,
    /// Optional longer content (code, a plan) the model attached to this
    /// option, for a Pane with room to show it.
    pub preview: Option<String>,
}

/// The operator's answer to one [`Question`], by position in its options.
///
/// `picks` index `Question::options`; `other` is free text typed instead of
/// (or, for multi-select, beside) the offered options — Claude Code's own UI
/// offers an "Other" entry that takes typed text, and the harness accepts
/// any string as an answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    pub picks: Vec<usize>,
    pub other: Option<String>,
}

/// Whether a Decision's tool is the question tool.
pub fn is_question_tool(tool_name: &str) -> bool {
    tool_name == TOOL_NAME || is_async(tool_name)
}

/// The questions in an `AskUserQuestion` input, or None if the input is not
/// one. Well-formed means at least one question with text and at least two
/// labelled options each; the tool's own upper bounds (four questions, four
/// options) are not enforced here, because rejecting an over-long but
/// answerable question would leave the operator with a Decision the Pane
/// cannot answer. Missing `header`, `description` and `multiSelect` default
/// rather than fail, for the same reason.
pub fn parse(decision_input: &Value) -> Option<Vec<Question>> {
    let questions = decision_input.get("questions")?.as_array()?;
    if questions.is_empty() {
        return None;
    }
    let asynchronous = decision_input["delivery"] == "async";
    questions
        .iter()
        .map(|value| parse_question(value, asynchronous))
        .collect()
}

fn parse_question(value: &Value, asynchronous: bool) -> Option<Question> {
    let question = value.get("question")?.as_str()?.trim();
    if question.is_empty() {
        return None;
    }
    let options = value
        .get("options")?
        .as_array()?
        .iter()
        .map(parse_choice)
        .collect::<Option<Vec<_>>>()?;
    if !asynchronous && options.len() < 2 {
        return None;
    }
    Some(Question {
        question: question.to_string(),
        header: string_or_empty(value.get("header")),
        options,
        multi_select: value
            .get("multiSelect")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_choice(value: &Value) -> Option<Choice> {
    let label = value.get("label")?.as_str()?.trim();
    if label.is_empty() {
        return None;
    }
    Some(Choice {
        label: label.to_string(),
        description: string_or_empty(value.get("description")),
        preview: value
            .get("preview")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn string_or_empty(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

/// The `updatedInput` that answers `questions`: the original input with an
/// `answers` object added, keyed by question text, each value the picked
/// labels (and any typed text) joined by ", " — the exact string Claude
/// Code's own UI writes, so the model reads Ferrite's answers as it reads
/// its own. A question with neither picks nor text is left out of the map,
/// not answered with an empty string. `answers` is matched to `questions`
/// by position; extra entries on either side are ignored.
pub fn answered_input(input: &Value, answers: &[Answer], questions: &[Question]) -> Value {
    let mut map = match input {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };
    let mut answered = Map::new();
    for (question, answer) in questions.iter().zip(answers) {
        if let Some(text) = answer_text(question, answer) {
            answered.insert(question.question.clone(), Value::String(text));
        }
    }
    map.insert("answers".to_string(), Value::Object(answered));
    Value::Object(map)
}

fn answer_text(question: &Question, answer: &Answer) -> Option<String> {
    let mut parts: Vec<&str> = answer
        .picks
        .iter()
        .filter_map(|&pick| question.options.get(pick))
        .map(|choice| choice.label.as_str())
        .collect();
    if let Some(other) = answer.other.as_deref().map(str::trim) {
        if !other.is_empty() {
            parts.push(other);
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

/// One line for a cell too small to show the questions: the count and the
/// headers ("2 questions · Approach, Library"). A question without a header
/// is named by the start of its text.
pub fn summary(questions: &[Question]) -> String {
    const HEADLESS_CHARS: usize = 24;
    let noun = if questions.len() == 1 {
        "question"
    } else {
        "questions"
    };
    let names: Vec<String> = questions
        .iter()
        .map(|question| {
            if question.header.is_empty() {
                truncate(&question.question, HEADLESS_CHARS)
            } else {
                question.header.clone()
            }
        })
        .collect();
    format!("{} {noun} · {}", questions.len(), names.join(", "))
}

fn truncate(text: &str, chars: usize) -> String {
    let mut iter = text.chars();
    let cut: String = iter.by_ref().take(chars).collect();
    if iter.next().is_some() {
        format!("{}…", cut.trim_end())
    } else {
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn realistic() -> Value {
        json!({
            "questions": [
                {
                    "question": "Which approach should we take for the retry logic?",
                    "header": "Approach",
                    "options": [
                        {"label": "Exponential backoff", "description": "Doubles the delay each attempt, capped."},
                        {"label": "Fixed delay", "description": "Waits the same interval every time."},
                        {"label": "No retry", "description": "Fail fast and let the caller decide.", "preview": "fn send() -> Result<Response> { client.send()? }"}
                    ],
                    "multiSelect": false
                },
                {
                    "question": "Which libraries may I add?",
                    "header": "Library",
                    "options": [
                        {"label": "backoff", "description": "Small, well-known."},
                        {"label": "tokio-retry", "description": "Async-native."},
                        {"label": "None", "description": "Hand-roll it."}
                    ],
                    "multiSelect": true
                }
            ]
        })
    }

    #[test]
    fn recognises_the_tool_by_name() {
        assert!(is_question_tool("AskUserQuestion"));
        assert!(!is_question_tool("Bash"));
        assert!(!is_question_tool("askuserquestion"));
    }

    #[test]
    fn parses_a_realistic_input() {
        let questions = parse(&realistic()).expect("well-formed");
        assert_eq!(questions.len(), 2);
        let first = &questions[0];
        assert_eq!(first.header, "Approach");
        assert_eq!(
            first.question,
            "Which approach should we take for the retry logic?"
        );
        assert!(!first.multi_select);
        assert_eq!(first.options.len(), 3);
        assert_eq!(first.options[0].label, "Exponential backoff");
        assert_eq!(first.options[0].preview, None);
        assert_eq!(
            first.options[2].preview.as_deref(),
            Some("fn send() -> Result<Response> { client.send()? }")
        );
        assert!(questions[1].multi_select);
    }

    #[test]
    fn missing_optional_fields_default() {
        let input = json!({"questions": [{
            "question": "Proceed?",
            "options": [{"label": "Yes"}, {"label": "No"}]
        }]});
        let questions = parse(&input).expect("optional fields default");
        assert_eq!(questions[0].header, "");
        assert!(!questions[0].multi_select);
        assert_eq!(questions[0].options[0].description, "");
    }

    #[test]
    fn rejects_inputs_that_are_not_questions() {
        assert_eq!(parse(&json!({"command": "ls"})), None);
        assert_eq!(parse(&json!({"questions": []})), None);
        assert_eq!(parse(&json!({"questions": "nope"})), None);
        assert_eq!(
            parse(
                &json!({"questions": [{"question": "", "options": [{"label": "a"}, {"label": "b"}]}]})
            ),
            None
        );
        assert_eq!(
            parse(
                &json!({"questions": [{"question": "One option only?", "options": [{"label": "a"}]}]})
            ),
            None
        );
        assert_eq!(
            parse(
                &json!({"questions": [{"question": "Unlabelled?", "options": [{"label": "a"}, {"description": "b"}]}]})
            ),
            None
        );
    }

    #[test]
    fn answered_input_keeps_the_original_and_adds_answers() {
        let input = realistic();
        let questions = parse(&input).unwrap();
        let answers = [
            Answer {
                picks: vec![0],
                other: None,
            },
            Answer {
                picks: vec![0, 2],
                other: None,
            },
        ];
        let updated = answered_input(&input, &answers, &questions);
        assert_eq!(updated["questions"], input["questions"]);
        assert_eq!(
            updated["answers"],
            json!({
                "Which approach should we take for the retry logic?": "Exponential backoff",
                "Which libraries may I add?": "backoff, None"
            })
        );
    }

    #[test]
    fn free_text_answers_and_unanswered_questions() {
        let input = realistic();
        let questions = parse(&input).unwrap();
        let answers = [
            Answer {
                picks: vec![],
                other: Some("  Jittered backoff  ".into()),
            },
            Answer {
                picks: vec![1],
                other: Some("reqwest-retry".into()),
            },
        ];
        let updated = answered_input(&input, &answers, &questions);
        assert_eq!(
            updated["answers"],
            json!({
                "Which approach should we take for the retry logic?": "Jittered backoff",
                "Which libraries may I add?": "tokio-retry, reqwest-retry"
            })
        );

        let only_first = [Answer {
            picks: vec![2],
            other: None,
        }];
        let updated = answered_input(&input, &only_first, &questions);
        assert_eq!(
            updated["answers"],
            json!({"Which approach should we take for the retry logic?": "No retry"})
        );

        let blank = [Answer::default(), Answer::default()];
        assert_eq!(
            answered_input(&input, &blank, &questions)["answers"],
            json!({})
        );
    }

    #[test]
    fn out_of_range_picks_are_ignored() {
        let input = realistic();
        let questions = parse(&input).unwrap();
        let answers = [Answer {
            picks: vec![7, 1],
            other: None,
        }];
        let updated = answered_input(&input, &answers, &questions);
        assert_eq!(
            updated["answers"]["Which approach should we take for the retry logic?"],
            "Fixed delay"
        );
    }

    #[test]
    fn summary_names_questions_by_header() {
        let questions = parse(&realistic()).unwrap();
        assert_eq!(summary(&questions), "2 questions · Approach, Library");
        assert_eq!(summary(&questions[..1]), "1 question · Approach");
    }

    #[test]
    fn summary_falls_back_to_question_text_without_a_header() {
        let input = json!({"questions": [{
            "question": "Should the cache be invalidated on every write or only on schema change?",
            "options": [{"label": "Every write"}, {"label": "Schema change"}]
        }]});
        let questions = parse(&input).unwrap();
        assert_eq!(
            summary(&questions),
            "1 question · Should the cache be inva…"
        );
    }
}
