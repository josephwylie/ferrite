//! Claude's async command lifecycle, independent of result/turn boundaries.
//!
//! A prompt sent while a turn runs (`isAsync`) is confirmed taken by its
//! `command_lifecycle` start, or — the same fact from the other side, and
//! the one a turn's own frames carry — by the request's and the result's
//! `user_message_uuids` naming it (HD5, `claude-steer-2.1.318`: a message
//! folded into the running turn is listed by that turn's next request and
//! its result). Whichever comes first starts it; the other is the same
//! start.
use crate::{QueueEvent, QueuedPrompt, SessionEvent};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct Queue {
    pub supported: bool,
    pub submitted: HashMap<String, String>,
    pub cancellations: HashMap<String, String>,
    started: HashSet<String>,
}
impl Queue {
    pub fn observe(&mut self, frame: &Value) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        if frame["type"] == "system" && frame["subtype"] == "init" {
            self.supported = frame["capabilities"]
                .as_array()
                .is_some_and(|caps| caps.iter().any(|cap| cap == "msg_lifecycle_v1"));
        }
        if frame["type"] == "control_response" {
            let response = &frame["response"];
            if let Some(id) = response["request_id"]
                .as_str()
                .and_then(|id| self.cancellations.remove(id))
            {
                let error = (response["subtype"] != "success").then(|| {
                    response["error"]
                        .as_str()
                        .unwrap_or("native cancellation failed")
                        .into()
                });
                events.push(SessionEvent::Queue(QueueEvent::Cancelled {
                    id,
                    cancelled: error.is_none() && response["response"]["cancelled"] == true,
                    error,
                }));
            }
        }
        let answered = (frame["type"] == "result"
            || (frame["type"] == "system" && frame["subtype"] == "status"))
            .then(|| frame["user_message_uuids"].as_array())
            .flatten();
        for id in answered.into_iter().flatten().filter_map(Value::as_str) {
            if self.submitted.contains_key(id) && self.started.insert(id.into()) {
                events.push(SessionEvent::Queue(QueueEvent::Started {
                    historical: false,
                    client_id: id.into(),
                    text: None,
                }));
            }
        }
        if frame["type"] == "command_lifecycle" {
            if let Some(id) = frame["command_uuid"].as_str() {
                if let Some(text) = self.submitted.get(id) {
                    let event = match frame["state"].as_str() {
                        Some("queued") if !self.started.contains(id) => {
                            Some(QueueEvent::Accepted(QueuedPrompt {
                                id: id.into(),
                                client_id: id.into(),
                                text: text.clone(),
                            }))
                        }
                        Some("started") if self.started.insert(id.into()) => {
                            Some(QueueEvent::Started {
                                historical: false,
                                client_id: id.into(),
                                text: None,
                            })
                        }
                        Some("cancelled") => Some(QueueEvent::Removed { id: id.into() }),
                        Some("completed" | "started" | "queued") => None,
                        _ => Some(QueueEvent::Failed {
                            client_id: id.into(),
                            error: format!("unknown Claude queue lifecycle: {}", frame["state"]),
                        }),
                    };
                    if let Some(event) = event {
                        events.push(SessionEvent::Queue(event));
                    }
                    if matches!(frame["state"].as_str(), Some("completed" | "cancelled")) {
                        self.submitted.remove(id);
                        self.started.remove(id);
                    }
                }
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn steer_frames() -> Vec<Value> {
        include_str!("../../../tests/fixtures/claude-steer-2.1.318.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    const STEER: &str = "c747d678-96d7-4b97-b511-834e1ade3631";

    fn starts(queue: &mut Queue, frames: &[Value]) -> Vec<usize> {
        let mut at = Vec::new();
        for (index, frame) in frames.iter().enumerate() {
            for event in queue.observe(frame) {
                if matches!(event, SessionEvent::Queue(QueueEvent::Started { ref client_id, .. }) if client_id == STEER) {
                    at.push(index);
                }
            }
        }
        at
    }

    /// The folded message starts once, mid-turn, before the result that
    /// ends both — by its lifecycle or by the request naming it.
    #[test]
    fn a_folded_message_starts_once_mid_turn() {
        let frames = steer_frames();
        let result = frames.iter().position(|frame| frame["type"] == "result").unwrap();
        let mut queue = Queue::default();
        queue.submitted.insert(STEER.into(), "also".into());
        let at = starts(&mut queue, &frames);
        assert_eq!(at.len(), 1, "{at:?}");
        assert!(at[0] < result);
    }

    /// With no lifecycle start (a CLI that sends none, or a frame lost),
    /// the turn's `user_message_uuids` still confirm it (HD5).
    #[test]
    fn user_message_uuids_confirm_a_folded_message() {
        let frames: Vec<Value> = steer_frames()
            .into_iter()
            .filter(|frame| frame["type"] != "command_lifecycle")
            .collect();
        let result = frames.iter().position(|frame| frame["type"] == "result").unwrap();
        let mut queue = Queue::default();
        queue.submitted.insert(STEER.into(), "also".into());
        let at = starts(&mut queue, &frames);
        assert_eq!(at.len(), 1, "{at:?}");
        assert!(at[0] < result, "the next request names it, before the result");
    }

    #[test]
    fn native_batch_fixture_consumes_each_uuid_independently_of_results() {
        let fixture = include_str!(
            "../../../../../docs/research/fixtures/claude-native-queue-batch-2.1.263.jsonl"
        );
        let frames: Vec<Value> = fixture
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let mut queue = Queue::default();
        for frame in &frames {
            if frame["state"] == "queued" {
                queue.submitted.insert(
                    frame["command_uuid"].as_str().unwrap().into(),
                    "operator text".into(),
                );
            }
        }
        let mut accepted = 0;
        let mut started = 0;
        for frame in frames {
            for event in queue.observe(&frame) {
                match event {
                    SessionEvent::Queue(QueueEvent::Accepted(_)) => accepted += 1,
                    SessionEvent::Queue(QueueEvent::Started { .. }) => started += 1,
                    _ => {}
                }
            }
            if frame["type"] == "result" && started == 1 {
                assert_eq!(accepted, 3);
            }
        }
        assert!(queue.supported);
        assert_eq!((accepted, started), (3, 3));
        assert_eq!(
            queue.submitted.len(),
            2,
            "the capture ends before the batch's completed events"
        );
    }
}
