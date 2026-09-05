//! Codex's fire-and-return questions are structured agent messages. Replies
//! enter as new user input, with correlated acknowledgements, never approvals.
use crate::{activity::ActivityEvent, Decision, DecisionAnswer, SessionEvent};
use serde_json::{json, Value};
use std::{collections::HashMap, io};

const PREFIX: &str = "codex-async-question:";

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
    Some(Decision {
        delivery: crate::DecisionDelivery::Async,
        id: format!("{PREFIX}{}", json!([thread, id])),
        tool_use_id: id.into(),
        tool_name: crate::questions::ASYNC_TOOL_NAME.into(),
        description: crate::questions::summary(&parsed),
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
