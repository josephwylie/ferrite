//! Native queue RPC correlation and serialized, all-page reconciliation.
//!
//! Input written while a turn runs is *steered* (`turn/steer`): the server
//! folds it into the running turn at its next tool boundary, as the Codex
//! CLI's own Enter does. Only input with no turn to steer goes through the
//! after-turn queue (`thread/queue/add`). Both are mirrored the same way;
//! a steer's mirror id carries the `steer:` prefix because the server never
//! names it and cannot take it back.
use crate::{QueueEvent, QueuedPrompt, SessionEvent};
use serde_json::{json, Value};
use std::collections::HashMap;

/// The mirror id of a steered prompt: the server names only the turn.
fn steer_id(client_id: &str) -> String {
    format!("steer:{client_id}")
}

fn is_steer_id(id: &str) -> bool {
    id.starts_with("steer:")
}

/// The server's refusals that mean "nothing is running any more", from its
/// turn/steer handler: no active turn, or the expected turn is not the
/// active one.
fn steer_missed_turn(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("no active turn")
        || error.contains("expected turn")
        || error.contains("mismatch")
}

#[derive(Default)]
pub(super) struct Queue {
    pub supported: bool,
    thread: String,
    next: u64,
    requests: HashMap<String, Request>,
    listing: bool,
    dirty: bool,
    early: Vec<Value>,
    paused: bool,
    page: Vec<QueuedPrompt>,
}
enum Request {
    List,
    Add(String),
    Steer { client_id: String, input: Value },
    Delete(String),
    Start,
}

impl Queue {
    fn request(&mut self, method: &str, params: Value, operation: Request) -> Value {
        self.next += 1;
        let id = format!("ferrite-queue-{}", self.next);
        self.requests.insert(id.clone(), operation);
        json!({"jsonrpc":"2.0", "id":id, "method":method,"params":params})
    }
    pub fn identify(&mut self, thread: &Value) -> Vec<SessionEvent> {
        self.thread = thread["id"].as_str().unwrap_or_default().into();
        self.paused = thread["turns"]
            .as_array()
            .and_then(|turns| turns.last())
            .is_some_and(|turn| turn["status"] == "interrupted");
        let mut events = Vec::new();
        if let Some(turns) = thread["turns"].as_array() {
            for turn in turns {
                if let Some(items) = turn["items"].as_array() {
                    for item in items {
                        let frame = json!({"method":"item/completed","params":{"threadId":self.thread,"item":item}});
                        for mut event in self.observe(&frame).0 {
                            if let SessionEvent::Queue(QueueEvent::Started { historical, .. }) =
                                &mut event
                            {
                                *historical = true;
                            }
                            events.push(event);
                        }
                    }
                }
            }
        }
        for frame in std::mem::take(&mut self.early) {
            events.extend(self.observe(&frame).0);
        }
        events
    }
    pub fn initialize(&mut self, thread: &str) -> Value {
        self.thread = thread.into();
        self.list(None)
    }
    fn list(&mut self, cursor: Option<&str>) -> Value {
        self.listing = true;
        self.request(
            "thread/queue/list",
            json!({"threadId":self.thread,"cursor":cursor,"limit":100}),
            Request::List,
        )
    }
    pub fn add(&mut self, client: &str, input: Value) -> Value {
        self.request(
            "thread/queue/add",
            json!({"threadId":self.thread,"clientUserMessageId":client,"input":input}),
            Request::Add(client.into()),
        )
    }
    /// Fold `input` into the running turn `turn` at its next tool boundary.
    pub fn steer(&mut self, client: &str, input: Value, turn: &str) -> Value {
        self.request(
            "turn/steer",
            json!({"threadId":self.thread,"expectedTurnId":turn,"clientUserMessageId":client,"input":input}),
            Request::Steer {
                client_id: client.into(),
                input,
            },
        )
    }
    /// A steer has no server-side handle: once submitted it belongs to the
    /// turn, and only an interrupt gets it back.
    pub fn delete(&mut self, id: &str) -> std::io::Result<Value> {
        if is_steer_id(id) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "Codex cannot take back input already steering the running turn; Escape interrupts and resends it",
            ));
        }
        Ok(self.request(
            "thread/queue/delete",
            json!({"threadId":self.thread,"queuedSubmissionId":id}),
            Request::Delete(id.into()),
        ))
    }
    pub fn observe(&mut self, frame: &Value) -> (Vec<SessionEvent>, Vec<Value>) {
        let mut events = Vec::new();
        let mut requests = Vec::new();
        if self.thread.is_empty() {
            if matches!(
                frame["method"].as_str(),
                Some("item/started" | "item/completed" | "turn/completed" | "turn/started")
            ) && self.early.len() < 256
            {
                self.early.push(frame.clone());
            }
            return (events, requests);
        }
        let scoped = frame["params"]["threadId"].as_str() == Some(self.thread.as_str());
        if scoped && frame["method"] == "turn/started" {
            self.paused = false;
        }
        if scoped && frame["method"] == "turn/completed" {
            self.paused = frame["params"]["turn"]["status"] == "interrupted";
        }
        if scoped && frame["method"] == "thread/queue/changed" {
            if self.listing {
                self.dirty = true;
            } else {
                requests.push(self.list(None));
            }
        }
        if scoped
            && matches!(
                frame["method"].as_str(),
                Some("item/started" | "item/completed")
            )
        {
            let item = &frame["params"]["item"];
            if item["type"] == "userMessage" {
                if let Some(client) = item["clientId"].as_str() {
                    {
                        events.push(SessionEvent::Queue(QueueEvent::Started {
                            historical: false,
                            client_id: client.into(),
                            text: item["content"].as_array().map(|parts| {
                                parts
                                    .iter()
                                    .filter_map(|part| part["text"].as_str())
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            }),
                        }));
                    }
                }
            }
        }
        let operation = frame["id"].as_str().and_then(|id| self.requests.remove(id));
        let result = &frame["result"];
        let error = frame.get("error").map(|e| {
            e["message"]
                .as_str()
                .unwrap_or("native queue request failed")
                .to_string()
        });
        match operation {
            Some(Request::Add(client_id)) => {
                if let Some(error) = error {
                    events.push(SessionEvent::Queue(QueueEvent::Failed { client_id, error }));
                } else if let Some(item) = prompt(&result["queuedSubmission"]) {
                    events.push(SessionEvent::Queue(QueueEvent::Accepted(item)));
                    if self.paused {
                        self.paused = false;
                        requests.push(self.request(
                            "thread/queue/start",
                            json!({"threadId":self.thread}),
                            Request::Start,
                        ));
                    }
                } else {
                    events.push(SessionEvent::Queue(QueueEvent::Failed {
                        client_id,
                        error: "invalid native queue acknowledgment; delivery uncertain".into(),
                    }));
                }
            }
            Some(Request::Steer { client_id, input }) => {
                if let Some(error) = error {
                    // The turn ended under the steer: the after-turn queue
                    // starts it as soon as the thread is idle.
                    if steer_missed_turn(&error) {
                        requests.push(self.add(&client_id, input));
                    } else {
                        events.push(SessionEvent::Queue(QueueEvent::Failed { client_id, error }));
                    }
                } else {
                    events.push(SessionEvent::Queue(QueueEvent::Accepted(QueuedPrompt {
                        id: steer_id(&client_id),
                        client_id,
                        text: input_text(&input),
                    })));
                }
            }
            Some(Request::Delete(id)) => {
                events.push(SessionEvent::Queue(QueueEvent::Cancelled {
                    id,
                    cancelled: error.is_none() && result["deleted"] == true,
                    error,
                }));
                if self.listing {
                    self.dirty = true;
                } else {
                    requests.push(self.list(None));
                }
            }
            Some(Request::List) => {
                if error.is_some() || !result["data"].is_array() {
                    self.supported = false;
                    self.listing = false;
                    self.page.clear();
                } else {
                    self.supported = true;
                    self.page
                        .extend(result["data"].as_array().unwrap().iter().filter_map(prompt));
                    if let Some(cursor) = result["nextCursor"].as_str() {
                        requests.push(self.list(Some(cursor)));
                    } else {
                        self.listing = false;
                        if self.dirty {
                            self.dirty = false;
                            self.page.clear();
                            requests.push(self.list(None));
                        } else {
                            events.push(SessionEvent::Queue(QueueEvent::Snapshot(std::mem::take(
                                &mut self.page,
                            ))));
                        }
                    }
                }
            }
            Some(Request::Start) => {
                if let Some(error) = error {
                    self.paused = true;
                    events.push(SessionEvent::Queue(QueueEvent::Failed {
                        client_id: String::new(),
                        error: format!("native queue remains paused: {error}"),
                    }));
                }
            }
            None => {}
        }
        (events, requests)
    }
}
fn prompt(value: &Value) -> Option<QueuedPrompt> {
    value["input"].as_array()?;
    Some(QueuedPrompt {
        id: value["id"].as_str()?.into(),
        client_id: value["clientUserMessageId"]
            .as_str()
            .unwrap_or_default()
            .into(),
        text: input_text(&value["input"]),
    })
}
fn input_text(input: &Value) -> String {
    input
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_snapshot_pages_and_change_race_reconcile_before_publishing() {
        let capture: Value = serde_json::from_str(include_str!(
            "../../../../../docs/research/native-queues/codex-0.153.4.json"
        ))
        .unwrap();
        let items = capture["after_process_restart"].as_array().unwrap();
        let mut queue = Queue::default();
        let request = queue.initialize("root");
        let (_, next) = queue.observe(
            &json!({"id":request["id"],"result":{"data":[items[0]],"nextCursor":"page2"}}),
        );
        assert!(queue.supported);
        queue.observe(&json!({"method":"thread/queue/changed","params":{"threadId":"root"}}));
        let (events, refresh) = queue
            .observe(&json!({"id":next[0]["id"],"result":{"data":[items[1]],"nextCursor":null}}));
        assert!(events.is_empty(), "a raced snapshot is not authoritative");
        let (events, _) = queue
            .observe(&json!({"id":refresh[0]["id"],"result":{"data":items,"nextCursor":null}}));
        assert!(
            matches!(&events[..], [SessionEvent::Queue(QueueEvent::Snapshot(items))] if items.len() == 2)
        );
        let add = queue.add("client", json!([]));
        let (events, _) = queue.observe(&json!({"id":add["id"],"error":{"message":"refused"}}));
        assert!(
            matches!(&events[..], [SessionEvent::Queue(QueueEvent::Failed { client_id, .. })] if client_id == "client")
        );
    }

    /// Mid-turn input steers the running turn; a steer that finds the turn
    /// already gone falls through to the after-turn queue instead of being
    /// lost, and a steer can never be deleted.
    #[test]
    fn a_steer_mirrors_by_client_id_and_falls_back_when_the_turn_is_gone() {
        let mut queue = Queue::default();
        queue.initialize("root");
        let input = json!([{"type":"text","text":"also run the tests"}]);
        let steer = queue.steer("c1", input.clone(), "turn-1");
        assert_eq!(steer["method"], "turn/steer");
        assert_eq!(steer["params"]["expectedTurnId"], "turn-1");
        assert_eq!(steer["params"]["clientUserMessageId"], "c1");
        let (events, requests) =
            queue.observe(&json!({"id":steer["id"],"result":{"turnId":"turn-1"}}));
        assert!(requests.is_empty());
        assert!(
            matches!(&events[..], [SessionEvent::Queue(QueueEvent::Accepted(item))]
                if item.id == "steer:c1" && item.client_id == "c1" && item.text == "also run the tests")
        );
        assert!(queue.delete("steer:c1").is_err());

        let late = queue.steer("c2", input.clone(), "turn-1");
        let (events, requests) =
            queue.observe(&json!({"id":late["id"],"error":{"message":"no active turn to steer"}}));
        assert!(events.is_empty());
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["method"], "thread/queue/add");
        assert_eq!(requests[0]["params"]["clientUserMessageId"], "c2");
        assert_eq!(requests[0]["params"]["input"], input);
    }
}
