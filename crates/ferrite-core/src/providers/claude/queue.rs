//! Claude's async command lifecycle, independent of result/turn boundaries.
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
