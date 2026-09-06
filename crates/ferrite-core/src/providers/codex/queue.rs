//! Native queue RPC correlation and serialized, all-page reconciliation.
use crate::{QueueEvent, QueuedPrompt, SessionEvent};
use serde_json::{json, Value};
use std::collections::HashMap;

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
    pub fn delete(&mut self, id: &str) -> Value {
        self.request(
            "thread/queue/delete",
            json!({"threadId":self.thread,"queuedSubmissionId":id}),
            Request::Delete(id.into()),
        )
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
    Some(QueuedPrompt {
        id: value["id"].as_str()?.into(),
        client_id: value["clientUserMessageId"]
            .as_str()
            .unwrap_or_default()
            .into(),
        text: value["input"]
            .as_array()?
            .iter()
            .filter_map(|v| v["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    })
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
}
