//! Codex's asynchronous questions are structured agent messages. Native
//! request_user_input calls block on their JSON-RPC response instead.
use crate::{
    activity::ActivityEvent, validate_form, Decision, DecisionAnswer, DecisionKind, FormField,
    FormFieldKind, SessionEvent,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    io,
};

const PREFIX: &str = "codex-async-question:";

/// Normalize the native request into the provider-neutral form UI input.
/// Provider response construction remains below, so renderer code never sees
/// the app-server schema.
pub(super) fn decode_native(params: &Value, id: String) -> Option<Decision> {
    let item_id = params.get("itemId")?.as_str()?;
    let questions = params
        .get("questions")?
        .as_array()?
        .iter()
        .map(|question| {
            let id = question.get("id")?.as_str()?;
            let header = question.get("header")?.as_str()?.trim();
            let text = question.get("question")?.as_str()?.trim();
            if id.is_empty() || text.is_empty() {
                return None;
            }
            let options = match question.get("options") {
            None | Some(Value::Null) => vec![],
            Some(Value::Array(options)) => options.iter().map(|option| {
                Some(json!({
                    "label": option.get("label")?.as_str()?,
                    "description": option.get("description").and_then(Value::as_str).unwrap_or(""),
                }))
            }).collect::<Option<Vec<_>>>()?,
            _ => return None,
        };
            Some(json!({
                "id": id,
                "header": header,
                "question": text,
                "options": options,
                "secret": question["isSecret"].as_bool().unwrap_or(false),
                "allowOther": question["isOther"].as_bool().unwrap_or(true),
            }))
        })
        .collect::<Option<Vec<_>>>()?;
    let input = json!({"questions": questions});
    let parsed = crate::questions::parse(&input)?;
    let description = crate::questions::summary(&parsed);
    Some(Decision {
        delivery: crate::DecisionDelivery::Blocking,
        kind: DecisionKind::Questions(parsed),
        policy: Default::default(),
        id,
        tool_use_id: item_id.into(),
        tool_name: crate::questions::NATIVE_TOOL_NAME.into(),
        description,
        input,
        suggestions: vec![],
    })
}

pub(super) fn decode_elicitation(params: &Value, id: String) -> Option<Decision> {
    let message = params.get("message")?.as_str()?.trim();
    if message.is_empty() {
        return None;
    }
    let kind = match params.get("mode")?.as_str()? {
        "form" => DecisionKind::Form {
            fields: form_fields(params.get("requestedSchema")?)?,
        },
        "url" => DecisionKind::External {
            url: params.get("url")?.as_str()?.to_string(),
        },
        _ => return None,
    };
    Some(Decision {
        delivery: crate::DecisionDelivery::Blocking,
        kind,
        policy: Default::default(),
        id,
        tool_use_id: params
            .get("itemId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        tool_name: "mcp_elicitation".into(),
        description: message.into(),
        input: Value::Null,
        suggestions: vec![],
    })
}

pub(super) fn decode_permissions(params: &Value, id: String) -> Option<Decision> {
    let permissions = params.get("permissions")?.as_object()?;
    Some(Decision {
        delivery: crate::DecisionDelivery::Blocking,
        kind: DecisionKind::Approval,
        policy: Default::default(),
        id,
        tool_use_id: params.get("itemId")?.as_str()?.to_string(),
        tool_name: "permissions".into(),
        description: params
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        input: Value::Object(permissions.clone()),
        suggestions: vec![],
    })
}

pub(super) fn form_fields(schema: &Value) -> Option<Vec<FormField>> {
    if schema.get("type")?.as_str()? != "object" {
        return None;
    }
    let required = schema.get("required").and_then(Value::as_array);
    schema
        .get("properties")?
        .as_object()?
        .iter()
        .map(|(id, field)| {
            let kind = match field.get("type")?.as_str()? {
                "string" if field.get("enum").is_some() => FormFieldKind::Enum {
                    options: field
                        .get("enum")?
                        .as_array()?
                        .iter()
                        .map(Value::as_str)
                        .collect::<Option<Vec<_>>>()?
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                    multi_select: false,
                    default: field.get("default").cloned(),
                },
                "string" => FormFieldKind::String {
                    min_length: field
                        .get("minLength")
                        .and_then(Value::as_u64)
                        .map(|v| v as usize),
                    max_length: field
                        .get("maxLength")
                        .and_then(Value::as_u64)
                        .map(|v| v as usize),
                    default: field
                        .get("default")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                },
                "number" => FormFieldKind::Number {
                    minimum: field.get("minimum").and_then(Value::as_f64),
                    maximum: field.get("maximum").and_then(Value::as_f64),
                    default: field.get("default").and_then(Value::as_f64),
                },
                "integer" => FormFieldKind::Integer {
                    minimum: field.get("minimum").and_then(Value::as_i64),
                    maximum: field.get("maximum").and_then(Value::as_i64),
                    default: field.get("default").and_then(Value::as_i64),
                },
                "boolean" => FormFieldKind::Boolean {
                    default: field.get("default").and_then(Value::as_bool),
                },
                "array" => FormFieldKind::Enum {
                    options: field
                        .get("items")?
                        .get("enum")?
                        .as_array()?
                        .iter()
                        .map(Value::as_str)
                        .collect::<Option<Vec<_>>>()?
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                    multi_select: true,
                    default: field.get("default").cloned(),
                },
                _ => return None,
            };
            Some(FormField {
                id: id.clone(),
                label: field
                    .get("title")
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                    .unwrap_or(id)
                    .into(),
                description: field
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                required: required.is_some_and(|required| {
                    required.iter().any(|value| value.as_str() == Some(id))
                }),
                kind,
            })
        })
        .collect()
}

/// Native request handles are registered by the reader, and selected here at
/// reply time. This keeps reply matching in the provider adapter instead of
/// inferring it from a renderer-visible tool name.
#[derive(Default)]
pub(super) struct NativeRequests {
    pending: HashMap<String, NativeRequest>,
    resolved: HashSet<String>,
}

enum NativeRequest {
    Questions(Vec<crate::questions::Question>),
    Form(Vec<FormField>),
    External,
    Permissions(Value),
}

impl NativeRequests {
    pub fn observe(&mut self, frame: &Value) {
        if frame.get("method").and_then(Value::as_str) == Some("serverRequest/resolved") {
            if let Some(id @ (Value::Number(_) | Value::String(_))) =
                frame["params"].get("requestId")
            {
                let id = id.to_string();
                self.pending.remove(&id);
                self.resolved.insert(id);
            }
            return;
        }
        let Some(id @ (Value::Number(_) | Value::String(_))) = frame.get("id") else {
            return;
        };
        let id = id.to_string();
        let request = match frame.get("method").and_then(Value::as_str) {
            Some("item/tool/requestUserInput") => decode_native(&frame["params"], id.clone())
                .and_then(|decision| match decision.kind {
                    DecisionKind::Questions(questions) => Some(NativeRequest::Questions(questions)),
                    _ => None,
                }),
            Some("mcpServer/elicitation/request") => {
                decode_elicitation(&frame["params"], id.clone()).and_then(|decision| match decision
                    .kind
                {
                    DecisionKind::Form { fields } => Some(NativeRequest::Form(fields)),
                    DecisionKind::External { .. } => Some(NativeRequest::External),
                    _ => None,
                })
            }
            Some("item/permissions/requestApproval") => {
                decode_permissions(&frame["params"], id.clone())
                    .map(|decision| NativeRequest::Permissions(decision.input))
            }
            _ => None,
        };
        if let Some(request) = request {
            self.pending.insert(id, request);
        }
    }

    pub fn response(&self, id: &str, answer: &DecisionAnswer) -> Option<io::Result<Value>> {
        if self.resolved.contains(id) {
            return Some(Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Decision is no longer pending",
            )));
        }
        let request = self.pending.get(id)?;
        Some(match request {
            NativeRequest::Questions(questions) => question_response(questions, answer),
            NativeRequest::Form(fields) => elicitation_response(Some(fields), answer),
            NativeRequest::External => elicitation_response(None, answer),
            NativeRequest::Permissions(profile) => permission_response(profile, answer),
        })
    }

    pub fn resolved(&mut self, id: &str) {
        self.pending.remove(id);
        self.resolved.insert(id.into());
    }
}

fn question_response(
    questions: &[crate::questions::Question],
    answer: &DecisionAnswer,
) -> io::Result<Value> {
    match answer {
        DecisionAnswer::Questions { answers } => {
            let mut values = serde_json::Map::new();
            for (question, answer) in questions.iter().zip(answers) {
                let id = question
                    .id
                    .as_ref()
                    .ok_or_else(|| io::Error::other("native question has no id"))?;
                let values_for_question = crate::questions::selected_values(question, answer);
                if !values_for_question.is_empty() {
                    values.insert(id.clone(), json!({"answers": values_for_question}));
                }
            }
            Ok(json!({"answers": values}))
        }
        DecisionAnswer::Allow { input } | DecisionAnswer::AllowAlways { input, .. } => {
            native_response(input)
        }
        DecisionAnswer::Deny { .. } | DecisionAnswer::Cancel => Ok(json!({"answers": {}})),
        DecisionAnswer::Form { .. } => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "question requires question answers",
        )),
    }
}

fn elicitation_response(
    fields: Option<&[FormField]>,
    answer: &DecisionAnswer,
) -> io::Result<Value> {
    match answer {
        DecisionAnswer::Form { values } => {
            if let Some(fields) = fields {
                validate_form(fields, values)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
            }
            Ok(json!({"action":"accept","content":values,"_meta":null}))
        }
        DecisionAnswer::Allow { .. } | DecisionAnswer::AllowAlways { .. } if fields.is_none() => {
            Ok(json!({"action":"accept","content":null,"_meta":null}))
        }
        DecisionAnswer::Deny { .. } => Ok(json!({"action":"decline","content":null,"_meta":null})),
        DecisionAnswer::Cancel => Ok(json!({"action":"cancel","content":null,"_meta":null})),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "elicitation answer does not match its form",
        )),
    }
}

fn permission_response(profile: &Value, answer: &DecisionAnswer) -> io::Result<Value> {
    let empty = json!({});
    let permissions = match answer {
        DecisionAnswer::Allow { .. } | DecisionAnswer::AllowAlways { .. } => profile,
        DecisionAnswer::Deny { .. } | DecisionAnswer::Cancel => &empty,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "permission approval requires allow or deny",
            ))
        }
    };
    let mut granted = serde_json::Map::new();
    for key in ["network", "fileSystem"] {
        if let Some(value) = permissions.get(key).filter(|value| !value.is_null()) {
            granted.insert(key.into(), value.clone());
        }
    }
    Ok(json!({"permissions": granted, "scope":"turn"}))
}

/// The native wire accepts only its response object, never the normalized
/// questions carried for rendering.
pub(super) fn native_response(input: &Value) -> io::Result<Value> {
    let answers = input
        .get("answers")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "missing native question answers",
            )
        })?;
    Ok(json!({"answers": answers}))
}

pub(super) fn decode(params: &Value) -> Option<Decision> {
    let item = &params["item"];
    if item["type"] != "agentMessage" || item["delivery"] != "async" {
        return None;
    }
    let thread = params["threadId"].as_str()?;
    let id = item["id"].as_str()?;
    let questions: Option<Vec<_>> = item["questions"]
        .as_array()?
        .iter()
        .map(|q| {
            let title = q["title"].as_str()?;
            let options = match &q["options"] {
                Value::Null => vec![],
                Value::Array(options) => options
                    .iter()
                    .map(|o| Some(json!({"label":o.as_str()?})))
                    .collect::<Option<Vec<_>>>()?,
                _ => return None,
            };
            Some(json!({"question":title,"options":options}))
        })
        .collect();
    let input = json!({"delivery":"async","questions":questions?});
    let parsed = crate::questions::parse(&input)?;
    let description = crate::questions::summary(&parsed);
    Some(Decision {
        delivery: crate::DecisionDelivery::Async,
        kind: DecisionKind::Questions(parsed),
        policy: Default::default(),
        id: format!("{PREFIX}{}", json!([thread, id])),
        tool_use_id: id.into(),
        tool_name: crate::questions::ASYNC_TOOL_NAME.into(),
        description,
        input,
        suggestions: vec![],
    })
}

#[derive(Default)]
pub(super) struct Replies {
    pending: HashMap<u64, String>,
}
impl Replies {
    pub fn prepare(
        &mut self,
        id: &str,
        answer: &DecisionAnswer,
        rpc: u64,
        thread: &str,
        turn: Option<&str>,
    ) -> io::Result<Option<Value>> {
        let Some(encoded) = id.strip_prefix(PREFIX) else {
            return Ok(None);
        };
        let identity: Vec<String> = serde_json::from_str(encoded).map_err(io::Error::other)?;
        if identity.len() != 2 || identity[0] != thread {
            return Err(io::Error::other("question belongs to another thread"));
        }
        if self.pending.values().any(|pending| pending == id) {
            return Err(io::Error::other("answer is already being sent"));
        }
        if self.pending.len() >= 128 {
            return Err(io::Error::other("too many unanswered deliveries"));
        }
        let text = match answer {
            DecisionAnswer::Allow { input } | DecisionAnswer::AllowAlways { input, .. } => {
                let answers = input["answers"]
                    .as_object()
                    .ok_or_else(|| io::Error::other("missing question answers"))?;
                let mut text = format!("Answer to your async question {}:\n", identity[1]);
                for (question, answer) in answers {
                    let answer = answer
                        .as_str()
                        .ok_or_else(|| io::Error::other("invalid question answer"))?;
                    text.push_str(&format!("\n{question}\n{answer}\n"));
                }
                if answers.is_empty() {
                    return Err(io::Error::other("no answers selected"));
                }
                text
            }
            DecisionAnswer::Deny { message } => {
                format!("Skip your async question {}. {}", identity[1], message)
            }
            DecisionAnswer::Questions { .. }
            | DecisionAnswer::Form { .. }
            | DecisionAnswer::Cancel => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "async question reply requires its legacy input",
                ));
            }
        };
        let mut params = json!({"threadId":thread,"input":[{"type":"text","text":text}]});
        let method = if let Some(turn) = turn {
            params["expectedTurnId"] = turn.into();
            "turn/steer"
        } else {
            params["summary"] = "detailed".into();
            "turn/start"
        };
        self.pending.insert(rpc, id.into());
        Ok(Some(
            json!({"jsonrpc":"2.0","id":rpc,"method":method,"params":params}),
        ))
    }
    pub fn discard(&mut self, rpc: u64) {
        self.pending.remove(&rpc);
    }
    pub fn observe(&mut self, frame: &Value) -> Option<SessionEvent> {
        if frame.get("result").is_none() && frame.get("error").is_none() {
            return None;
        }
        let rpc = frame["id"].as_u64()?;
        let id = self.pending.remove(&rpc)?;
        let error = frame.get("error").map(|error| {
            format!(
                "Answer not delivered: {}. Your choices are saved; try again.",
                error["message"]
                    .as_str()
                    .unwrap_or("Codex rejected the answer")
            )
        });
        Some(SessionEvent::Activity(ActivityEvent::DecisionReply {
            id,
            error,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        activity::{Activity, ActivityInput, AgentStatus},
        transcript::{Input, Status},
        TurnOutcome,
    };
    use std::time::Instant;

    #[test]
    fn async_question_delivery_keeps_work_live_and_retains_rejected_answers() {
        let mut decoder = super::super::wire::Decoder::default();
        let mut activity = Activity::new(Default::default());
        activity.apply(ActivityInput::Connect { generation: 1 });
        let apply = |activity: &mut Activity, event| {
            activity.apply(ActivityInput::Main {
                input: Input::Event(event),
                at: Instant::now(),
            });
        };
        apply(
            &mut activity,
            SessionEvent::TextDelta {
                text: "Working".into(),
            },
        );
        let mut frame = json!({"method":"item/started","params":{"threadId":"main","item":{
            "type":"agentMessage","id":"q1","text":"Fallback must not duplicate the form",
            "phase":"final_answer","delivery":"async","questions":[
                {"title":"Which approach?","options":["Small change"]},
                {"title":"Any constraints?"}
            ]
        }}});
        for event in decoder.parse(&frame.to_string()) {
            apply(&mut activity, event);
        }
        frame["method"] = "item/completed".into();
        assert!(decoder.parse(&frame.to_string()).is_empty());
        assert_eq!(activity.view().main().status(), AgentStatus::Working);
        assert_eq!(
            activity.view().main().transcript().status(),
            Status::Streaming
        );
        let pending = activity.view().pending_decisions()[0].clone();
        let questions = crate::questions::parse(&pending.decision.input).unwrap();
        assert_eq!(questions.len(), 2);
        assert!(questions[1].options.is_empty());
        let answer = DecisionAnswer::Allow {
            input: json!({"answers":{"Which approach?":"Small change","Any constraints?":"Keep it narrow"}}),
        };
        let mut replies = Replies::default();
        let packet = replies
            .prepare(
                &pending.decision.id,
                &answer,
                5,
                "main",
                Some("active-turn"),
            )
            .unwrap()
            .unwrap();
        assert_eq!(packet["method"], "turn/steer");
        assert_eq!(packet["params"]["expectedTurnId"], "active-turn");
        assert!(packet["params"]["input"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Which approach?\nSmall change"));
        activity.apply(ActivityInput::AnswerSubmitted {
            handle: pending.handle.clone(),
        });
        assert!(replies
            .prepare(
                &pending.decision.id,
                &answer,
                6,
                "main",
                Some("active-turn")
            )
            .is_err());
        apply(
            &mut activity,
            replies
                .observe(&json!({"id":5,"error":{"message":"turn mismatch"}}))
                .unwrap(),
        );
        assert!(!activity.view().pending_decisions()[0].submitting);
        assert!(activity.view().pending_decisions()[0].reply_error.is_some());
        apply(
            &mut activity,
            SessionEvent::TurnEnded {
                outcome: TurnOutcome::Completed,
                cost_usd: None,
            },
        );
        assert_eq!(
            activity.view().pending_decisions().len(),
            1,
            "question survives turn completion"
        );
        let packet = replies
            .prepare(&pending.decision.id, &answer, 7, "main", None)
            .unwrap()
            .unwrap();
        assert_eq!(packet["method"], "turn/start");
        assert_eq!(packet["params"]["summary"], "detailed");
        assert!(packet["params"].get("expectedTurnId").is_none());
        activity.apply(ActivityInput::AnswerSubmitted {
            handle: pending.handle,
        });
        apply(
            &mut activity,
            replies.observe(&json!({"id":7,"result":{}})).unwrap(),
        );
        assert!(activity.view().pending_decisions().is_empty());
        assert!(
            !activity.view().main().busy(),
            "an acknowledgement cannot resurrect a completed turn"
        );
    }
}
